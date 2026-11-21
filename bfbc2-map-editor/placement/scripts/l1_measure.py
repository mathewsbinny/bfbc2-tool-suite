"""L1 (round L1, 2026-11-20): how DICE laid out gameplay on the stock multiplayer levels - the measurements.

Reads every stock multiplayer level of OPFOR's game copy (read only, from the bundles in memory: l1_read) and writes
l1_measurements.json: one record per level, mode, metric and subject, with the source entity GUIDs. Positions are
numbers only; no game data is copied.

    python l1_measure.py [out.json] [level ...]

Entity semantics used (each checked against the suite's code or the data; see L1_placement_report.md "Method"):
- capture point (CapturePointEntityData): UnableToChangeTeam true = a team's base (HQ, never captured; mpconvert.rs
  capture_points), else a flag (Conquest, Onslaught) or a Rush spawn base (Rush and Squad Rush: no flags at all);
  its Spawners list its soldier spawners and vehicle spawners; its Geometry its capture area (a sphere, an AABB or an
  outline), else CaptureRadius.
- soldier spawner (SoldierSpawnEntityData): one spawner, its own transform plus AlternativeSpawnPoints
  (AlternateSpawnEntityData) = the places a soldier can appear; SpawnEnemyRadius its enemy-proximity check.
- combat area (CombatAreaTriggerEntityData): Geometry = one or more outlines whose union is the allowed area (spec
  LEVEL-ENT-2); SurroundingGeometry a larger outer outline (meaning not proven: see the report); Team +
  UseTeamRestriction = the team it applies to.
- Rush stages: the M-COMs (placed objects whose events reach an arm / disarm GameInteractionEntityData) grouped by
  the MultipleTriggerEntityData they trigger (modelayer.rs stages()); the order and each stage's active bases and
  combat areas come from simulating the Enable / Disable events their destruction fires (simulate()).
- attackers: the team of the LifeCounterEntityData with Attacker = true.
"""
import json
import math
import os
import sys
import time
from collections import Counter

import numpy as np

from l1_geom import Grid, boundary_samples, box_of, heightfield, inside_union, shoelace, signed_margin, xz
from l1_level import Level, load, polygons
from l1_levels import levels
from l1_read import GAME

HERE = os.path.dirname(os.path.abspath(__file__))
GLUE = {'MultipleTriggerEntityData', 'DelayTriggerEntityData', 'EventSplitterEntityData', 'TeamFilterEntityData',
        'EventGateEntityData'}
INT_FIELDS = {'TimeToGetControl', 'TimeToLoseControl', 'Index', 'OnlyTakeableByTeam', 'MinNrToTakeControl',
              'AreaValueTeam1', 'AreaValueTeam2', 'MaxCaptureMultiplier', 'EnemyTicketLossWhenCaptured',
              'InitialTicketCount', 'TicketLossPerMin', 'TicketLossStart', 'TicketLossWhenLostAll',
              'TicketLossWhenLostAllBases', 'TicketLossPerMinPointCountScale', 'StartingLifeCount', 'BaseCount',
              'LifeTresholdValues', 'Amount', 'MaxVehicles', 'MaxVehiclesPerMap', 'KillsToWin', 'Kills',
              'TriggerAmount', 'MaxKillCount', 'FightHarderTreshold', 'UiShowCountLowerThreshold', 'UiShowCountUpperThreshold'}

# vehicle blueprint kind (objects/vehicles/<domain>/<kind>/..., objects/weapons/stationary/<kind>) -> class
VEH = {
    'm1a2': 'tank', 't90': 'tank', 'm4-sherman': 'tank', 'type97-chiha': 'tank', 'm48pattontank': 'tank',
    't-54': 'tank',
    'm3a3': 'ifv', 'bmd3': 'ifv', 'aa-bmd3': 'ifv_aa', 'cav': 'ifv',
    'humvee': 'car', 'cobra': 'car', 'vodnik': 'car', 'willys-mb': 'car', 'kurogane': 'car', 'm151a1': 'car',
    'gaz69': 'car', 'minitruck': 'car', 'truck': 'car', 'quadbike': 'quad',
    'ah60': 'heli', 'ah64': 'heli', 'mi24': 'heli', 'mi28': 'heli', 'huey': 'heli',
    'jap_zero_fighter': 'plane', 'us_corsair': 'plane',
    'uav': 'uav',
    'pbl': 'boat', 'pwc': 'boat', 'markv': 'boat', 'pbr_mkii': 'boat', 'lcvp-landingcraft': 'boat',
    'a10': 'ambient', 'f18': 'ambient', 'su39': 'ambient', 'cargoplane_backdrop': 'ambient', 'phantom': 'ambient',
    'cruiseplatform': 'ambient',
}


def veh_class(ref: str) -> tuple[str, str]:
    p = ref.lower().split('/')
    kind = p[3] if len(p) > 3 else ref.lower()
    if 'stationary' in p or kind.startswith('stationary_'):
        if kind in VEH and VEH[kind] == 'ambient':
            return 'ambient', kind
        if 'uav_station' in ref.lower():
            return 'uav', kind
        return 'emplacement', kind
    if kind == 'uav' and 'station' in ref.lower():
        return 'uav', kind
    return VEH.get(kind, 'other'), kind


def r1(v):
    return None if v is None else round(float(v), 1)


def d2(a, b) -> float:
    return math.hypot(a[0] - b[0], a[2] - b[2])


class Ctx:
    def __init__(self, L: Level, lv: str, era: str, mode: str):
        self.L, self.lv, self.era, self.mode = L, lv, era, mode
        self.recs: list[dict] = []
        self.notes: list[str] = []
        self.hf = None
        self.layout: list[dict] = []

    def rec(self, metric: str, value, unit: str = '', guids=(), **kw):
        r = {'level': self.lv, 'era': self.era, 'mode': self.mode, 'metric': metric,
             'value': r1(value) if isinstance(value, (int, float, np.floating)) and not isinstance(value, bool)
             else value, 'unit': unit, 'guids': list(guids)}
        r.update({k: v for k, v in kw.items() if v is not None})
        self.recs.append(r)


# ------------------------------------------------------------------------------------------------ pieces

def ival(i, name):
    """A field read as an int when DICE stores it so (INT_FIELDS), else as a float, from the raw bits."""
    if name in INT_FIELDS:
        w = i.whole(name)
        if w is not None and abs(w) < (1 << 24):
            return w
    return i.num(name)


def spawn_points(L: Level, sg: str) -> list[tuple[str, tuple]]:
    """A soldier spawner's places: its alternative spawn points; the spawner's own transform only when it has none.

    Reading (not proven): the spawner's own transform is an anchor when it has alternatives - on the Squad
    Deathmatch levels the four squads' spawners stand stacked 2.5 m apart, some outside the combat area, while
    their alternatives spread over the map (nam_mp_007sdm: all four spawners 14-20 m outside)."""
    s = L.insts[sg]
    out = []
    for a in s.refs('AlternativeSpawnPoints'):
        if a in L.insts and L.insts[a].pos:
            out.append((a, L.insts[a].pos))
    if not out and s.pos:
        out = [(sg, s.pos)]
    return out


def cp_spawners(L: Level, cp) -> tuple[list[str], list[str]]:
    sol, veh = [], []
    for s in cp.refs('Spawners'):
        t = L.insts[s].type if s in L.insts else ''
        if t == 'SoldierSpawnEntityData':
            sol.append(s)
        elif t == 'VehicleSpawnEntityData':
            veh.append(s)
    return sol, veh


def vehicles_of(L: Level, vg: str) -> list[tuple[str, str, str]]:
    """(team, class, kind) of each vehicle a vehicle spawner can make: its Vehicles items (per team), else Vehicle."""
    v = L.insts[vg]
    out = []
    c = v.f('Vehicles')
    for it in (c.children or []) if c is not None else []:
        ref = None
        team = None
        for cc in it.children or []:
            if cc.attr('name') == 'Vehicle':
                ref = cc.attr('ref')
            elif cc.attr('name') == 'Team':
                from l1_read import dbx
                team = dbx.value(cc)
        if ref and ref != 'null':
            cl, kind = veh_class(ref)
            out.append((team or v.text('Team') or '', cl, kind))
    if not out:
        for ref in v.extrefs('Vehicle'):
            cl, kind = veh_class(ref)
            out.append((v.text('Team') or '', cl, kind))
    return out


def spawner_class(L: Level, vg: str, team: str | None) -> str | None:
    """The class of the vehicle a spawner makes for `team`: its entry for that team, else its first entry."""
    vs = vehicles_of(L, vg)
    if not vs:
        return None
    for t, cl, _ in vs:
        if t == team:
            return cl
    return vs[0][1]


def capture_zone(L: Level, cp) -> dict:
    """The capture area: from Geometry (sphere radius, AABB extents, outline area), else CaptureRadius."""
    geo = polygons(L, cp, 'Geometry')
    capr = cp.num('CaptureRadius') or 0.0
    z = {'kind': 'none', 'capture_radius_field': r1(capr), 'guids': []}
    if not geo:
        if capr > 0:
            z.update(kind='radius', radius=r1(capr))
        return z
    kinds = sorted({g['kind'] for g in geo})
    z['kind'] = '+'.join(kinds) + (f' x{len(geo)}' if len(geo) > 1 else '')
    z['guids'] = [x for g in geo for x in g['guids']][:8]
    pos = cp.pos
    reach = []
    area = 0.0
    for g in geo:
        if g['kind'] == 'sphere':
            reach.append(d2(pos, g['center']) + (g['radius'] or 0))
            z['radius'] = r1(g['radius'])
            area += math.pi * (g['radius'] or 0) ** 2
        elif g['kind'] == 'aabb':
            e = g['extents'] or [0, 0, 0]
            z['extents'] = [r1(x) for x in e]
            # Extents read as half-sizes (uncertain: the report says why)
            reach.append(d2(pos, g['center']) + math.hypot(e[0], e[2]))
            area += 4 * e[0] * e[2]
        elif g['kind'] == 'polygon':
            p = xz(g['points'])
            area += shoelace(p)
            reach.append(max(d2(pos, q) for q in g['points']))
    z['area_m2'] = r1(area)
    z['eq_radius'] = r1(math.sqrt(area / math.pi)) if area > 0 else None
    z['reach'] = r1(max(reach)) if reach else None
    return z


def ca_union(L: Level, ca) -> list[np.ndarray]:
    return [xz(p['points']) for p in polygons(L, ca, 'Geometry') if p['kind'] == 'polygon']


def ca_outer(L: Level, ca) -> list[np.ndarray]:
    return [xz(p['points']) for p in polygons(L, ca, 'SurroundingGeometry') if p['kind'] == 'polygon']


class Areas:
    """The level's combat areas on one raster: masks, union areas, boundary samples."""

    def __init__(self, L: Level):
        self.L = L
        self.cas = L.of('CombatAreaTriggerEntityData')
        self.polys = {c.guid: ca_union(L, c) for c in self.cas}
        self.outer = {c.guid: ca_outer(L, c) for c in self.cas}
        allp = [p for v in self.polys.values() for p in v] + [p for v in self.outer.values() for p in v]
        self.grid = Grid(box_of(allp)) if allp else None
        self._mask: dict = {}
        self._bnd: dict = {}

    def mask(self, guids: tuple[str, ...]):
        key = tuple(sorted(guids))
        if key not in self._mask:
            polys = [p for g in key for p in self.polys.get(g, [])]
            self._mask[key] = self.grid.mask(polys) if (self.grid and polys) else None
        return self._mask[key]

    def area(self, guids) -> float | None:
        m = self.mask(tuple(guids))
        return None if m is None else self.grid.area(m)

    def bnd(self, guids):
        key = tuple(sorted(guids))
        if key not in self._bnd:
            polys = [p for g in key for p in self.polys.get(g, [])]
            self._bnd[key] = (polys, boundary_samples(polys) if polys else np.zeros((0, 2)))
        return self._bnd[key]

    def margin(self, pos, guids) -> float | None:
        polys, b = self.bnd(guids)
        return signed_margin(pos, polys, b) if polys else None

    def dims(self, guids):
        polys = [p for g in guids for p in self.polys.get(g, [])]
        if not polys:
            return None
        x0, z0, x1, z1 = box_of(polys)
        return x1 - x0, z1 - z0


def terrain_setup(ctx: Ctx):
    t = ctx.L.of('TerrainEntityData')
    c = t[0].f('TerrainAsset') if t else None
    ref = c.attr('ref') if c is not None else None
    if not ref or ref.count('/') < 2:
        ctx.notes.append('no terrain reference')
        return
    name = ref.split('/')[1]
    common = {'Vietnam': 'nam_common', 'WW2': 'ww2_common', 'BFBC1': 'mp2_common'}.get(ctx.era, 'mp2_common')
    r = heightfield(name, ctx.lv, common)
    if r is None:
        ctx.notes.append(f'terrain {name}: height field not found')
        return
    ctx.hf = r[0]
    ctx.terrain_source = os.path.relpath(r[1], GAME)


def ground_stats(ctx: Ctx, A: Areas, guids, pts: list[tuple[str, tuple, str]], scope: str, **kw):
    """Per objective: ground height, height above ground, ground percentile within the combat area."""
    if ctx.hf is None or A.grid is None:
        return
    m = A.mask(tuple(guids)) if guids else None
    if m is None or not m.any():
        return
    cells = A.grid.pts[m]
    if len(cells) > 60000:
        cells = cells[np.linspace(0, len(cells) - 1, 60000).astype(int)]
    hs = np.sort(ctx.hf.height(cells[:, 0], cells[:, 1]))
    for g, p, label in pts:
        gh = float(ctx.hf.height(p[0], p[2]))
        pct = float(np.searchsorted(hs, gh, side='right')) / len(hs)
        ctx.rec(f'{scope}.ground_percentile', pct * 100, '%', [g], subject=label, **kw)
        ctx.rec(f'{scope}.above_ground', p[1] - gh, 'm', [g], subject=label, **kw)
        # local prominence: the ground here minus the median ground on rings 30-100 m around (water-independent)
        ang = np.linspace(0, 2 * np.pi, 48, endpoint=False)
        ring = [(p[0] + r * np.cos(a), p[2] + r * np.sin(a)) for r in (30, 50, 75, 100) for a in ang]
        rx = np.array([q[0] for q in ring])
        rz = np.array([q[1] for q in ring])
        around = ctx.hf.height(rx, rz)
        ctx.rec(f'{scope}.prominence', gh - float(np.median(around)), 'm', [g], subject=label,
                note='ground height minus the median ground 30-100 m around', **kw)


# ------------------------------------------------------------------------------------------------ Conquest

def measure_conquest(ctx: Ctx, A: Areas):
    L = ctx.L
    cps = L.of('CapturePointEntityData')
    flags = [c for c in cps if c.text('UnableToChangeTeam') != 'true' and c.pos]
    hqs = [c for c in cps if c.text('UnableToChangeTeam') == 'true' and c.text('InitialOwnerTeam') in ('Team1', 'Team2')]
    flags.sort(key=lambda c: (ival(c, 'Index') or 0))
    ctx.rec('cq.flag_count', len(flags), '', [c.guid for c in flags])
    ctx.rec('cq.hq_count', len(hqs), '', [c.guid for c in hqs])
    ctx.rec('cq.flags_initially_owned', sum(1 for c in flags if c.text('InitialOwnerTeam') in ('Team1', 'Team2')),
            '', [c.guid for c in flags if c.text('InitialOwnerTeam') in ('Team1', 'Team2')])
    lab = {c.guid: (c.text('LocationName') or c.name) for c in cps}
    # pairwise and nearest-neighbour distances
    for a in range(len(flags)):
        for b in range(a + 1, len(flags)):
            fa, fb = flags[a], flags[b]
            ctx.rec('cq.flag_pair_dist', d2(fa.pos, fb.pos), 'm', [fa.guid, fb.guid],
                    subject=f'{lab[fa.guid]} - {lab[fb.guid]}', dy=r1(abs(fa.pos[1] - fb.pos[1])))
    for f in flags:
        others = [o for o in flags if o is not f]
        if others:
            o = min(others, key=lambda o: d2(f.pos, o.pos))
            ctx.rec('cq.flag_nn_dist', d2(f.pos, o.pos), 'm', [f.guid, o.guid], subject=lab[f.guid])
    for h in hqs:
        if flags:
            f = min(flags, key=lambda f: d2(h.pos, f.pos))
            ctx.rec('cq.hq_nearest_flag', d2(h.pos, f.pos), 'm', [h.guid, f.guid], subject=lab[h.guid],
                    team=h.text('InitialOwnerTeam'), flag=lab[f.guid])
            far = max(flags, key=lambda f: d2(h.pos, f.pos))
            ctx.rec('cq.hq_farthest_flag', d2(h.pos, far.pos), 'm', [h.guid, far.guid], subject=lab[h.guid],
                    team=h.text('InitialOwnerTeam'))
    t1 = [h for h in hqs if h.text('InitialOwnerTeam') == 'Team1']
    t2 = [h for h in hqs if h.text('InitialOwnerTeam') == 'Team2']
    if t1 and t2:
        h1, h2 = t1[0], t2[0]
        hh = d2(h1.pos, h2.pos)
        ctx.rec('cq.hq_hq_dist', hh, 'm', [h1.guid, h2.guid])
        # nearest-flag asymmetry between the bases
        if flags:
            n1 = min(d2(h1.pos, f.pos) for f in flags)
            n2 = min(d2(h2.pos, f.pos) for f in flags)
            ctx.rec('cq.hq_nearest_flag_asymmetry', abs(n1 - n2), 'm', [h1.guid, h2.guid])
        # the flags along the base-to-base axis: t (0 at Team1's base, 1 at Team2's) and lateral offset / hh
        ax = np.array([h2.pos[0] - h1.pos[0], h2.pos[2] - h1.pos[2]])
        if hh > 1:
            u = ax / hh
            ts = []
            for f in flags:
                v = np.array([f.pos[0] - h1.pos[0], f.pos[2] - h1.pos[2]])
                t = float(v @ u) / hh
                lat = float(abs(v[0] * u[1] - v[1] * u[0])) / hh
                ts.append(t)
                ctx.rec('cq.flag_axis_t', t, '', [f.guid], subject=lab[f.guid])
                ctx.rec('cq.flag_axis_lateral', lat, '', [f.guid], subject=lab[f.guid])
            if len(flags) >= 3:
                order = np.argsort(np.argsort(ts))
                idx = np.arange(len(flags))
                rho = float(np.corrcoef(order, idx)[0, 1]) if np.std(order) > 0 else 0.0
                ctx.rec('cq.flag_index_axis_rho', rho, '', [f.guid for f in flags],
                        note='Spearman: flags in Index order vs their order along the Team1->Team2 base axis')
                # the letters shown in game (LocationName "A: ...", "B", ...) against the axis
                import re
                lets = [re.match(r'^\s*([A-E])(\W|$)', f.text('LocationName') or '') for f in flags]
                if all(lets):
                    lo = np.argsort(np.argsort([m.group(1) for m in lets]))
                    rl = float(np.corrcoef(order, lo)[0, 1]) if np.std(order) > 0 else 0.0
                    ctx.rec('cq.flag_letter_axis_rho', rl, '', [f.guid for f in flags],
                            note='Spearman: flag letters (A, B, ...) vs order along the Team1->Team2 base axis')
            ctx.rec('cq.flags_between_bases', sum(1 for t in ts if 0 <= t <= 1) / max(1, len(ts)) * 100, '%',
                    [f.guid for f in flags])
    # layout shape
    if len(flags) >= 3:
        P = np.array([[f.pos[0], f.pos[2]] for f in flags])
        ev = np.sort(np.linalg.eigvalsh(np.cov(P.T)))
        lin = math.sqrt(max(ev[0], 0) / ev[1]) if ev[1] > 0 else 0
        shape = 'line' if lin < 0.25 else ('band' if lin < 0.5 else ('triangle' if len(flags) == 3 else 'spread'))
        ctx.rec('cq.layout_thinness', lin, '', [f.guid for f in flags], shape=shape,
                note='sqrt(minor / major eigenvalue) of the flags\' XZ covariance; 0 = a line')
    # capture areas and timing
    for f in flags:
        z = capture_zone(L, f)
        ctx.rec('cq.capture_zone', z.get('eq_radius') or z.get('radius'), 'm', [f.guid] + z['guids'],
                subject=lab[f.guid], zone=z)
        if z.get('reach') is not None:
            ctx.rec('cq.capture_zone_reach', z['reach'], 'm', [f.guid], subject=lab[f.guid], kind=z['kind'])
        ctx.rec('cq.time_to_capture', ival(f, 'TimeToGetControl'), 's', [f.guid], subject=lab[f.guid])
        ctx.rec('cq.area_value', ival(f, 'AreaValueTeam1'), '', [f.guid], subject=lab[f.guid],
                team2=ival(f, 'AreaValueTeam2'))
    for h in hqs:
        z = capture_zone(L, h)
        ctx.rec('cq.hq_zone', z.get('eq_radius') or z.get('radius'), 'm', [h.guid] + z['guids'], subject=lab[h.guid],
                zone=z)
    # spawns
    for c in flags + hqs:
        is_hq = c in hqs
        sol, veh = cp_spawners(L, c)
        by_team = Counter(L.insts[s].text('Team') for s in sol)
        pts = [p for s in sol for p in spawn_points(L, s)]
        ds = [d2(c.pos, p) for _, p in pts]
        kind = 'hq' if is_hq else 'flag'
        ctx.rec(f'cq.{kind}_spawners', len(sol), '', sol, subject=lab[c.guid], teams=dict(by_team))
        ctx.rec(f'cq.{kind}_spawn_points', len(pts), '', [g for g, _ in pts][:40], subject=lab[c.guid])
        if ds:
            ctx.rec(f'cq.{kind}_spawn_dist_median', float(np.median(ds)), 'm', [g for g, _ in pts][:40],
                    subject=lab[c.guid])
            ctx.rec(f'cq.{kind}_spawn_dist_max', max(ds), 'm', [pts[int(np.argmax(ds))][0]], subject=lab[c.guid])
            ctx.rec(f'cq.{kind}_spawn_dist_min', min(ds), 'm', [pts[int(np.argmin(ds))][0]], subject=lab[c.guid])
        vc = Counter()
        for v in veh:
            cl = spawner_class(L, v, c.text('InitialOwnerTeam'))
            if cl and cl != 'ambient':
                vc[cl] += 1
        ctx.rec(f'cq.{kind}_vehicles', sum(vc.values()), '', veh, subject=lab[c.guid], by_class=dict(sorted(vc.items())),
                note='vehicle spawners in the capture point Spawners list, one per spawner')
    # every vehicle spawner (attached to a capture point or not) by its nearest base or flag
    attached = {v: c for c in flags + hqs for v in cp_spawners(L, c)[1]}
    for v in L.of('VehicleSpawnEntityData'):
        if not v.pos or not (flags + hqs):
            continue
        vs = [x for x in vehicles_of(L, v.guid) if x[1] != 'ambient']
        if not vs:
            continue
        near = min(flags + hqs, key=lambda c: d2(c.pos, v.pos))
        owner = attached.get(v.guid)
        ctx.rec('cq.vehicle_spawner', d2(near.pos, v.pos), 'm', [v.guid, near.guid], subject=lab[near.guid],
                near_hq=near in hqs, near_team=near.text('InitialOwnerTeam'),
                attached_to=lab[owner.guid] if owner else None,
                vehicles=sorted({f'{t}:{cl}' for t, cl, _ in vs}))
    # tickets
    for t in L.of('TicketCounterEntityData'):
        ctx.rec('cq.tickets', ival(t, 'InitialTicketCount'), '', [t.guid], team=t.text('TeamId'),
                loss_per_min=ival(t, 'TicketLossPerMin'), loss_start=ival(t, 'TicketLossStart'),
                scale=ival(t, 'TicketLossPerMinPointCountScale'),
                halt_on_equal=t.text('HaltTicketLossOnEqualPointCount'),
                loss_when_lost_all=ival(t, 'TicketLossWhenLostAll'))
    # combat areas
    measure_areas(ctx, A, [('flag', f.guid, f.pos, lab[f.guid], None) for f in flags]
                  + [('hq', h.guid, h.pos, lab[h.guid], h.text('InitialOwnerTeam')) for h in hqs], hqs)
    # heights
    allca = [c.guid for c in A.cas]
    ground_stats(ctx, A, allca, [(f.guid, f.pos, lab[f.guid]) for f in flags], 'cq.flag')
    ground_stats(ctx, A, allca, [(h.guid, h.pos, lab[h.guid]) for h in hqs], 'cq.hq')


def team_area_guids(A: Areas, team: str) -> list[str]:
    return [c.guid for c in A.cas if c.text('Team') in (team, 'TeamNeutral') or c.text('UseTeamRestriction') != 'true']


def measure_areas(ctx: Ctx, A: Areas, things, hqs, prefix: str = 'cq', spawn_cps=None):
    """Combat area sizes; margins of objectives, bases and spawn points; base protection (per-team areas)."""
    L = ctx.L
    if not A.cas:
        ctx.rec(f'{prefix}.combat_area_count', 0)
        return
    ctx.rec(f'{prefix}.combat_area_count', len(A.cas), '', [c.guid for c in A.cas],
            teams=dict(Counter(c.text('Team') for c in A.cas)))
    allg = [c.guid for c in A.cas]
    ctx.rec(f'{prefix}.combat_area_union', A.area(allg), 'm2', allg, dims=[r1(x) for x in A.dims(allg)])
    for c in A.cas:
        dims = A.dims([c.guid])
        outer = A.outer.get(c.guid) or []
        oa = sum(shoelace(p) for p in outer) if outer else None
        ctx.rec(f'{prefix}.combat_area', A.area([c.guid]), 'm2', [c.guid], subject=c.name, team=c.text('Team'),
                dims=[r1(x) for x in dims] if dims else None, pieces=len(A.polys[c.guid]), outer_m2=r1(oa),
                time_to_return=c.num('TimeToReturn'))
    per_team = {t: team_area_guids(A, t) for t in ('Team1', 'Team2')}
    for kind, g, pos, label, own in things:
        ms = {t: A.margin(pos, gs) for t, gs in per_team.items()}
        ms = {t: m for t, m in ms.items() if m is not None}
        if not ms:
            continue
        if own is None:          # a flag: both teams must reach it - the smaller margin
            ctx.rec(f'{prefix}.{kind}_margin', min(ms.values()), 'm', [g], subject=label,
                    note='signed distance to the nearer edge of either team\'s combat area (+ inside)')
        else:                    # a base: its own team's area, and the enemy's
            if own in ms:
                ctx.rec(f'{prefix}.{kind}_margin', ms[own], 'm', [g], subject=label, team=own,
                        note='signed distance to its own team\'s combat area edge (+ inside)')
            enemy = 'Team2' if own == 'Team1' else 'Team1'
            per_team_areas = any(c.text('UseTeamRestriction') == 'true' and c.text('Team') == enemy for c in A.cas)
            if enemy in ms and per_team_areas:
                ctx.rec(f'{prefix}.{kind}_vs_enemy_area', ms[enemy], 'm', [g], subject=label, team=own,
                        note='signed distance to the enemy\'s combat area edge (+ inside the enemy\'s area)')
    # spawn points of every capture point (flags and bases): margin to their own team's area
    cps = spawn_cps if spawn_cps is not None else L.of('CapturePointEntityData')
    for c in cps:
        sol, _ = cp_spawners(L, c)
        for s in sol:
            team = L.insts[s].text('Team')
            gs = per_team.get(team) or allg
            for g, p in spawn_points(L, s):
                m = A.margin(p, gs)
                if m is not None:
                    ctx.rec(f'{prefix}.spawn_margin', m, 'm', [g, s, c.guid], subject=c.text('LocationName') or c.name,
                            team=team)
    # base protection: a base's spawn points against the enemy's combat area (per-team areas only)
    if any(c.text('UseTeamRestriction') == 'true' and c.text('Team') in ('Team1', 'Team2') for c in A.cas):
        for h in hqs:
            team = h.text('InitialOwnerTeam')
            enemy = 'Team2' if team == 'Team1' else 'Team1'
            gs = [c.guid for c in A.cas if c.text('Team') == enemy]
            if not gs:
                continue
            sol, _ = cp_spawners(L, h)
            ms = [A.margin(p, gs) for s in sol for _, p in spawn_points(L, s)]
            ms = [m for m in ms if m is not None]
            if ms:
                ctx.rec(f'{prefix}.hq_spawns_vs_enemy_area', max(ms), 'm', [h.guid], subject=h.text('LocationName'),
                        team=team, note='max signed margin of the base\'s spawn points in the enemy\'s area: '
                                        'negative = every spawn point is outside the enemy\'s area by that much')


# ------------------------------------------------------------------------------------------------ Rush

def mcoms(L: Level) -> list[str]:
    out = []
    for g in L.order:
        i = L.insts[g]
        if i.type != 'ReferenceObjectData':
            continue
        if not any(L.insts.get(e.dst) is not None and L.insts[e.dst].type == 'GameInteractionEntityData'
                   for e in L.out(g)):
            continue
        c = i.f('ReferencedObject')
        if c is None or not c.attr('ref') or c.attr('ref') == 'null':
            continue
        out.append(g)
    return out


def stage_groups(L: Level) -> list[tuple[frozenset, list[str]]]:
    groups: list[tuple[frozenset, list[str]]] = []
    for m in mcoms(L):
        key = frozenset(e.dst for e in L.out(m) if L.insts.get(e.dst) is not None
                        and L.insts[e.dst].type == 'MultipleTriggerEntityData')
        for k, v in groups:
            if k == key:
                v.append(m)
                break
        else:
            groups.append((key, [m]))
    return groups


def initial_state(L: Level) -> dict[str, bool]:
    st = {}
    for g, i in L.insts.items():
        e = i.text('Enabled')
        if e is not None:
            st[g] = e == 'true'
    return st


def propagate(L: Level, seeds, state: dict, blocked: set, log: list):
    queue = list(seeds)
    seen = set()
    while queue:
        e = queue.pop(0)
        key = (e.src, e.dst, e.sev, e.tev)
        if key in seen:
            continue
        seen.add(key)
        t = L.insts.get(e.dst)
        if t is None:
            continue
        if e.tev == 'Enable':
            state[e.dst] = True
            log.append(('Enable', e.dst))
            queue += [x for x in L.out(e.dst) if x.sev == 'OnEnabled']
        elif e.tev == 'Disable':
            state[e.dst] = False
            log.append(('Disable', e.dst))
            queue += [x for x in L.out(e.dst) if x.sev == 'OnDisabled']
        elif t.type in GLUE:
            if e.dst in blocked:
                continue
            if t.type == 'MultipleTriggerEntityData' and not state.get(e.dst, True):
                continue
            queue += [x for x in L.out(e.dst) if x.sev.startswith('OnTrigger') or x.sev.startswith('OnImpulse')
                      or x.sev.startswith('OnTeam')]
        else:
            log.append((e.tev, e.dst))


def simulate(L: Level):
    """Rush: the stages in play order, each with the state (guid -> enabled) while it is being played."""
    groups = stage_groups(L)
    state = initial_state(L)
    keys_all = set().union(*[k for k, _ in groups]) if groups else set()
    done = []
    out = []
    uncertain = False
    remaining = list(range(len(groups)))
    while remaining:
        cand = [k for k in remaining if all(state.get(m, True) for m in groups[k][0])]
        if not cand:
            cand = [k for k in remaining if any(state.get(m, True) for m in groups[k][0])]
            uncertain = True
        if not cand:
            cand = remaining[:1]
            uncertain = True
        if len(cand) > 1:
            uncertain = True if done else uncertain
        k = cand[0]
        remaining.remove(k)
        key, ms = groups[k]
        snapshot = dict(state)
        seeds = [e for m in ms for e in L.out(m) if e.sev == 'onDestroyed']
        blocked = keys_all - set(key)
        log: list = []
        propagate(L, seeds, state, blocked, log)
        out.append({'mcoms': ms, 'key': key, 'state': snapshot, 'log': log})
        done.append(k)
    return out, uncertain


def measure_rush(ctx: Ctx, A: Areas, prefix: str):
    L = ctx.L
    lcs = L.of('LifeCounterEntityData')
    att = next((c.text('TeamId') for c in lcs if c.text('Attacker') == 'true'), None)
    if att is None:
        ctx.notes.append('no attacker life counter')
        att = 'Team1'
    dfn = 'Team2' if att == 'Team1' else 'Team1'
    for c in lcs:
        ctx.rec(f'{prefix}.lives', ival(c, 'StartingLifeCount'), '', [c.guid], team=c.text('TeamId'),
                attacker=c.text('Attacker'), base_count=ival(c, 'BaseCount'))
    ctx.rec(f'{prefix}.attacker_team', att, '', [c.guid for c in lcs if c.text('Attacker') == 'true'])
    stages, uncertain = simulate(L)
    ctx.rec(f'{prefix}.stage_count', len(stages), '', [m for s in stages for m in s['mcoms']],
            order_uncertain=uncertain)
    ctx.rec(f'{prefix}.mcoms_per_stage', [len(s['mcoms']) for s in stages], '', [])
    cps = L.of('CapturePointEntityData')
    lab = {c.guid: (c.text('LocationName') or c.name) for c in cps}
    attached_all = {v for c in cps for v in cp_spawners(L, c)[1]}
    prev_cent = None
    prev_area = {}
    prev_def: list = []
    for k, s in enumerate(stages, 1):
        st = s['state']
        mp = [(m, L.insts[m].pos) for m in s['mcoms']]
        cent = tuple(np.mean([p for _, p in mp], axis=0))
        if len(mp) == 2:
            ctx.rec(f'{prefix}.mcom_pair_dist', d2(mp[0][1], mp[1][1]), 'm', [mp[0][0], mp[1][0]], stage=k,
                    dy=r1(abs(mp[0][1][1] - mp[1][1][1])))
        if prev_cent is not None:
            ctx.rec(f'{prefix}.stage_step', d2(prev_cent, cent), 'm', s['mcoms'], stage=k,
                    dy=r1(cent[1] - prev_cent[1]), note='centroid of the previous stage\'s M-COMs to this one\'s')
        came_from = prev_cent
        prev_cent = cent
        active = [c for c in cps if st.get(c.guid, False)]
        nearest: dict = {}
        side_pts: dict = {}
        side_cps: dict = {}
        for side, team in (('attacker', att), ('defender', dfn)):
            mine = [c for c in active if c.text('InitialOwnerTeam') == team]
            pts = []
            for c in mine:
                sol, _ = cp_spawners(L, c)
                for sg in sol:
                    pts += [(g, p, c) for g, p in spawn_points(L, sg)]
            side_pts[side] = pts
            side_cps[side] = mine
            ctx.rec(f'{prefix}.{side}_bases', len(mine), '', [c.guid for c in mine], stage=k,
                    names=[lab[c.guid] for c in mine], hq=[c.text('UnableToChangeTeam') == 'true' for c in mine])
            if pts:
                ds = [(min(d2(p, q) for _, q in mp), g) for g, p, _ in pts]
                dmin = min(ds)
                ctx.rec(f'{prefix}.{side}_spawn_to_obj_min', dmin[0], 'm', [dmin[1]], stage=k)
                ctx.rec(f'{prefix}.{side}_spawn_to_obj_median', float(np.median([d for d, _ in ds])), 'm',
                        [c.guid for c in mine], stage=k)
                ctx.rec(f'{prefix}.{side}_spawn_points', len(pts), '', [c.guid for c in mine], stage=k)
                nearest[side] = dmin[0]
                dys = [float(np.mean([q[1] for _, q in mp])) - p[1] for _, p, _ in pts]
                ctx.rec(f'{prefix}.{side}_spawn_dy_to_obj', float(np.median(dys)), 'm', [c.guid for c in mine], stage=k,
                        note='median objective height minus spawn height (+ = objective above)')
            # vehicles of the stage: the vehicle spawners of the team's active bases
            vc = Counter()
            vg = []
            for c in mine:
                _, veh = cp_spawners(L, c)
                for v in veh:
                    cl = spawner_class(L, v, team)
                    if cl and cl != 'ambient':
                        vc[cl] += 1
                        vg.append(v)
            # standalone spawners (in no capture point's Spawners) of this team, enabled while the stage is played
            sc = Counter()
            for v in L.of('VehicleSpawnEntityData'):
                if v.guid in attached_all or not st.get(v.guid, False) or v.text('Team') != team:
                    continue
                cl = spawner_class(L, v.guid, team)
                if cl and cl != 'ambient':
                    sc[cl] += 1
                    vg.append(v.guid)
            ctx.rec(f'{prefix}.{side}_vehicles', sum(vc.values()), '', sorted(set(vg))[:40],
                    stage=k, by_class=dict(sorted(vc.items())), standalone_enabled=dict(sorted(sc.items())),
                    note='vehicle spawners of the side\'s active bases (value, by_class); standalone_enabled: '
                         'spawners in no base, of the team, Enabled while the stage is played - an upper bound '
                         '(ambient fly-bys and spawners started by other events included: uncertain)')
            # combat area of the stage
            cas = [c.guid for c in A.cas if st.get(c.guid, False) and c.text('Team') in (team, 'TeamNeutral')]
            if cas:
                area = A.area(cas)
                ctx.rec(f'{prefix}.{side}_area', area, 'm2', cas, stage=k, dims=[r1(x) for x in A.dims(cas)])
                if side in prev_area and prev_area[side] is not None:
                    pm, pg = prev_area[side]
                    cm = A.mask(tuple(cas))
                    if pm is not None and cm is not None:
                        inter = A.grid.area(pm & cm)
                        ctx.rec(f'{prefix}.{side}_area_kept', inter / max(1.0, A.grid.area(pm)) * 100, '%', cas,
                                stage=k, note='share of the previous stage\'s area still in this stage\'s')
                        ctx.rec(f'{prefix}.{side}_area_new', A.grid.area(cm & ~pm), 'm2', cas, stage=k)
                prev_area[side] = (A.mask(tuple(cas)), cas)
                for m, p in mp:
                    ctx.rec(f'{prefix}.obj_margin_{side}_area', A.margin(p, cas), 'm', [m] + cas, stage=k)
                # spawn protection: the other side's spawn points against this side's area
                other = dfn if team == att else att
                opts = []
                for c in active:
                    if c.text('InitialOwnerTeam') != other:
                        continue
                    sol, _ = cp_spawners(L, c)
                    for sg in sol:
                        opts += [(g, p) for g, p in spawn_points(L, sg)]
                if opts:
                    ms = [(A.margin(p, cas), g) for g, p in opts]
                    ms = [x for x in ms if x[0] is not None]
                    if ms:
                        mx = max(ms)
                        ctx.rec(f'{prefix}.{"defender" if side == "attacker" else "attacker"}_spawns_in_enemy_area',
                                mx[0], 'm', [mx[1]] + cas, stage=k,
                                note='max signed margin of the side\'s spawn points in the other side\'s area '
                                     '(negative = all outside, by that much)')
            else:
                prev_area[side] = None
        # the stage's geometry against the attack direction: from the previous stage's objectives (stage 1: from
        # the attackers' spawn points) to this stage's
        start = came_from
        if start is None and side_pts.get('attacker'):
            start = tuple(np.mean([p for _, p, _ in side_pts['attacker']], axis=0))
        if start is not None:
            u = np.array([cent[0] - start[0], cent[2] - start[2]])
            if np.linalg.norm(u) > 1:
                u = u / np.linalg.norm(u)
                if len(mp) == 2:
                    v = np.array([mp[1][1][0] - mp[0][1][0], mp[1][1][2] - mp[0][1][2]])
                    if np.linalg.norm(v) > 0.1:
                        ang = math.degrees(math.acos(min(1.0, abs(float(v @ u)) / float(np.linalg.norm(v)))))
                        ctx.rec(f'{prefix}.mcom_pair_angle', ang, 'deg', s['mcoms'], stage=k,
                                note='angle between the M-COM pair and the attack direction (90 = side by side '
                                     'across the front, 0 = one behind the other)')
                for side in ('attacker', 'defender'):
                    if side_pts.get(side):
                        sc = np.mean([[p[0], p[2]] for _, p, _ in side_pts[side]], axis=0)
                        ctx.rec(f'{prefix}.{side}_spawn_depth', float((sc - np.array([cent[0], cent[2]])) @ u), 'm',
                                [c.guid for c in side_cps[side]], stage=k,
                                note='the spawn centroid of the side along the attack direction from the objectives '
                                     '(+ = behind the objectives, seen from the attackers)')
        if k > 1 and prev_def and side_cps.get('attacker'):
            dd = min(d2(a.pos, b.pos) for a in side_cps['attacker'] for b in prev_def)
            ctx.rec(f'{prefix}.attacker_base_from_prev_defender_base', dd, 'm',
                    [c.guid for c in side_cps['attacker']], stage=k,
                    note='nearest distance from the attacker bases of this stage to the defender bases of the previous')
        prev_def = side_cps.get('defender') or []
        if nearest.get('attacker') and nearest.get('defender'):
            ctx.rec(f'{prefix}.spawn_ratio', nearest['attacker'] / max(1.0, nearest['defender']), '', s['mcoms'],
                    stage=k, note='nearest attacker spawn to objective / nearest defender spawn to objective')
        # heights of the stage's objectives
        cas_all = [c.guid for c in A.cas if st.get(c.guid, False)]
        ground_stats(ctx, A, cas_all, [(m, p, f'stage {k}') for m, p in mp], f'{prefix}.obj', stage=k)
    return stages


# ------------------------------------------------------------------------------------------------ SDM, Onslaught

def measure_sdm(ctx: Ctx, A: Areas):
    L = ctx.L
    cps = L.of('CapturePointEntityData')
    squads = [c for c in cps if (c.text('InitialOwnerTeam') or '').startswith('Team')
              and c.text('InitialOwnerTeam') != 'TeamNeutral']
    ctx.rec('sdm.squad_count', len(squads), '', [c.guid for c in squads])
    pts = []
    for c in squads:
        sol, _ = cp_spawners(L, c)
        for s in sol:
            pts += spawn_points(L, s)
    uniq = {}
    for g, p in pts:
        uniq[(round(p[0], 1), round(p[2], 1))] = (g, p)
    pts = list(uniq.values())
    ctx.rec('sdm.spawn_points', len(pts), '', [g for g, _ in pts][:40])
    if len(pts) >= 2:
        P = np.array([[p[0], p[2]] for _, p in pts])
        D = np.hypot(P[:, None, 0] - P[None, :, 0], P[:, None, 1] - P[None, :, 1])
        np.fill_diagonal(D, np.inf)
        nn = D.min(axis=1)
        ctx.rec('sdm.spawn_nn_median', float(np.median(nn)), 'm', [])
        ctx.rec('sdm.spawn_nn_min', float(nn.min()), 'm', [pts[int(np.argmin(nn))][0]])
        D[np.isinf(D)] = 0
        ctx.rec('sdm.spawn_spread_max', float(D.max()), 'm', [])
    sp = [c.pos for c in squads if c.pos]
    if len(sp) >= 2:
        ctx.rec('sdm.squad_base_spread', max(d2(a, b) for a in sp for b in sp), 'm', [c.guid for c in squads],
                note='largest distance between the squads\' capture points (0: one shared point)')
    veh = [v for c in cps for v in cp_spawners(L, c)[1]]
    vc = Counter(cl for v in veh for _, cl, _ in vehicles_of(L, v) if cl != 'ambient')
    ctx.rec('sdm.vehicles', sum(vc.values()), '', veh, by_class=dict(vc))
    for k in L.of('KillCounterEntityData'):
        vals = {}
        for c in k.el.children or []:
            n = c.attr('name')
            if c.children is None and n not in ('Components', 'Transform', 'Name', 'Enumeration'):
                vals[n] = (ival(k, n) if n in INT_FIELDS else k.num(n)) if c.kind == 7 else k.text(n)
        ctx.rec('sdm.kill_counter', None, '', [k.guid], fields=vals)
        break
    measure_areas(ctx, A, [], [], prefix='sdm', spawn_cps=[])
    if len(pts) and A.cas:
        allg = [c.guid for c in A.cas]
        ms = [A.margin(p, allg) for _, p in pts]
        ms = [m for m in ms if m is not None]
        if ms:
            ctx.rec('sdm.spawn_margin_min', min(ms), 'm', allg)
            ctx.rec('sdm.spawn_margin_median', float(np.median(ms)), 'm', allg)


def measure_ons(ctx: Ctx, A: Areas):
    L = ctx.L
    cps = L.of('CapturePointEntityData')
    flags = sorted([c for c in cps if c.text('UnableToChangeTeam') != 'true' and c.pos],
                   key=lambda c: ival(c, 'Index') or 0)
    hqs = [c for c in cps if c.text('UnableToChangeTeam') == 'true']
    ctx.rec('ons.flag_count', len(flags), '', [c.guid for c in flags])
    for a, b in zip(flags, flags[1:]):
        ctx.rec('ons.flag_step', d2(a.pos, b.pos), 'm', [a.guid, b.guid],
                subject=f'{a.text("LocationName")} -> {b.text("LocationName")}', dy=r1(b.pos[1] - a.pos[1]))
    if hqs and flags:
        ctx.rec('ons.hq_to_first_flag', d2(hqs[0].pos, flags[0].pos), 'm', [hqs[0].guid, flags[0].guid])
    for f in flags:
        z = capture_zone(L, f)
        ctx.rec('ons.capture_zone', z.get('eq_radius') or z.get('radius'), 'm', [f.guid] + z['guids'],
                subject=f.text('LocationName'), zone=z)
    ss = L.of('SoldierSpawnEntityData')
    ctx.rec('ons.ai_spawners', sum(1 for s in ss if s.text('PlayerType') != 'SSHumanPlayer'), '', [])
    for c in A.cas:
        ctx.rec('ons.combat_area', A.area([c.guid]), 'm2', [c.guid], subject=c.name, team=c.text('Team'))


def measure_common(ctx: Ctx):
    L = ctx.L
    for s in L.of('SoldierSpawnEntityData'):
        ctx.rec('all.spawn_enemy_radius', s.num('SpawnEnemyRadius'), 'm', [s.guid], team=s.text('Team'),
                away_from_sight=s.text('TryToSpawnAwayFromSight'), player=s.text('PlayerType'))
    for v in L.of('VehicleSpawnEntityData'):
        vs = vehicles_of(L, v.guid)
        cl = sorted({c for _, c, _ in vs}) or ['none']
        ctx.rec('all.vehicle_respawn_delay', ival(v, 'SpawnDelay') if v.whole('SpawnDelay') is not None and
                abs(v.whole('SpawnDelay')) < (1 << 24) else v.num('SpawnDelay'), 's', [v.guid], vclass='/'.join(cl))


# ------------------------------------------------------------------------------------------------ driver

def measure(lv: str, era: str, mode: str) -> Ctx:
    L = load(lv)
    ctx = Ctx(L, lv, era, mode)
    ctx.source = {'bundle': os.path.relpath(L.bundle, GAME), 'entry': L.entry}
    terrain_setup(ctx)
    A = Areas(L)
    if mode == 'CQ':
        measure_conquest(ctx, A)
    elif mode in ('Rush', 'SR'):
        stages = measure_rush(ctx, A, 'rush' if mode == 'Rush' else 'sr')
        for k, s in enumerate(stages, 1):
            for m in s['mcoms']:
                p = L.insts[m].pos
                ctx.layout.append({'kind': 'mcom', 'guid': m, 'stage': k, 'pos': [r1(x) for x in p]})
    elif mode == 'SDM':
        measure_sdm(ctx, A)
    elif mode == 'ONS':
        measure_ons(ctx, A)
    else:
        ctx.notes.append(f'mode {mode}: not measured (outside L1\'s modes)')
        return ctx
    for c in L.of('CapturePointEntityData'):
        if not c.pos:
            continue
        base = c.text('UnableToChangeTeam') == 'true'
        ctx.layout.append({'kind': 'base' if base else ('flag' if mode in ('CQ', 'ONS') else 'spawn base'),
                           'guid': c.guid, 'label': c.text('LocationName') or c.name,
                           'team': c.text('InitialOwnerTeam'), 'pos': [r1(x) for x in c.pos]})
    measure_common(ctx)
    return ctx


def main(argv):
    out = argv[0] if argv else os.path.join(HERE, 'l1_measurements.json')
    only = {a.lower() for a in argv[1:]}
    recs, levels_out = [], []
    for lv, era, mode, sec in levels():
        if only and lv not in only:
            continue
        t0 = time.time()
        try:
            ctx = measure(lv, era, mode)
        except Exception as e:  # noqa: BLE001
            import traceback
            traceback.print_exc()
            levels_out.append({'level': lv, 'era': era, 'mode': mode, 'map': sec, 'error': repr(e)})
            continue
        recs += ctx.recs
        levels_out.append({'level': lv, 'era': era, 'mode': mode, 'map': sec, 'source': ctx.source,
                           'terrain': getattr(ctx, 'terrain_source', None), 'notes': ctx.notes,
                           'records': len(ctx.recs), 'layout': ctx.layout})
        print(f'{lv:22} {era:8} {mode:5} {len(ctx.recs):5} records {time.time() - t0:5.1f} s {ctx.notes}', flush=True)
    doc = {'round': 'L1', 'date': '2026-11-20',
           'about': 'Gameplay layout measurements of the stock multiplayer levels (OPFOR game copy, read only). '
                    'Distances are horizontal (XZ) in metres unless named dy; areas m2. Each record names its source '
                    'entity GUIDs. Produced by l1_measure.py.',
           'levels': levels_out, 'records': recs}
    with open(out, 'w', encoding='utf-8') as fh:
        json.dump(doc, fh, indent=1)
    print('wrote', out, len(recs), 'records')


if __name__ == '__main__':
    main(sys.argv[1:])
