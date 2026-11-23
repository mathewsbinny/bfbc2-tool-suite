"""How DICE's terrains are shaped, what each biome's levels reuse, and where the playable area and the objectives sit
on the ground: the statistics a terrain generator must match (read-only; spec/transforms/terrain_gen.md).

    python spec/probes/terrain_shape.py [--opfor <folder>] [--ww2]

Reads the bundles of Project OPFOR BFBC2's game copy that are DICE's (`game/**/levels/*/*.fbrb` dated before 2013),
each resource once (the first bundle that holds it, as spec/probes/terrain.py), and the DICE level partitions
(spec/probes/level.py's `level_bundles`). `--ww2` also reads OPFOR's four WW2 levels (its conversions of BF1943's
terrains, `game/Package/levels/<wake_island_s | guadal_canal | iwo_jima_s | coral_sea>`), printed apart and never part
of the checks (OPFOR's build, not DICE's data).

Prints numbers only, no game content:

- per main height-field block (TGEN-SHAPE-*): size, height range, elevation percentiles, the hypsometric integral,
  slope percentiles and shares, local relief at 8 m and 64 m, the radial power spectrum's slope, the border ring
  against the centre, the water level, the share of ground under it and the coastline length;
- per terrain (TGEN-BIOME-*): its biome (from its levels' vegetation sets, checked against the material names), material map shares, the
  mask atlas layout, patch shader and undergrowth counts, the undergrowth meshes' folders;
- per level partition with a combat area (TGEN-PLAY-*): the combat area's area and box against the terrain, the
  ground inside it against the whole terrain, and per capture point its height percentile inside the combat area,
  its slope, its relief against its 100 m surroundings and its distance to water;
- per biome: the forest, middlegrowth and placed-object blueprint folders of its levels (TGEN-ASSET-*).
"""
from __future__ import annotations

import argparse
import collections
import math
import re
import struct
import sys
from pathlib import Path
from typing import Any

import numpy as np

HERE = Path(__file__).resolve().parent
SUITE = HERE.parents[1]
sys.path.insert(0, str(SUITE / 'core'))
sys.path.insert(0, str(HERE))
import level as lp  # noqa: E402
import terrain as tp  # noqa: E402
from bfbc2core import dbx, fbrb, paths  # noqa: E402

Counter = collections.Counter
WW2 = ('wake_island_s', 'guadal_canal', 'iwo_jima_s', 'coral_sea')

# TGEN-BIOME-1: a terrain's biome is the vegetation set its levels' forest and middlegrowth members come from
# (`vegetation/<set>/...`, by instances), when that set holds at least a fifth of the levels' forest and middlegrowth
# instances; a terrain whose levels use no set that much (only the shared `objects/vegetation` and their own props) is
# arid. Checked against the material map: every snow terrain names a snow
# material, every arid one no grass majority.
SETS = {'jungle': 'jungle', 'autumn': 'temperate', 'snow': 'snow', 'nam': 'vietnam'}


def biome_of(veg: Counter[str]) -> str:
    by: Counter[str] = Counter()
    for name, n in veg.items():
        p = name.split('/')
        if len(p) > 1 and p[0] == 'vegetation' and p[1] in SETS:
            by[SETS[p[1]]] += n
    best = by.most_common(1)
    return best[0][0] if best and 5 * best[0][1] >= sum(veg.values()) else 'arid'


def pct(a: Any, qs: tuple[float, ...]) -> list[float]:
    return [round(float(x), 1) for x in np.percentile(a, qs)]


def box_blur(h: Any, r: int) -> Any:
    """Mean over a (2r + 1)-sample square, edges clamped (summed-area table)."""
    p = np.pad(h, r + 1, mode='edge').astype(np.float64)
    s = p.cumsum(0).cumsum(1)
    k = 2 * r + 1
    out = s[k:, k:] - s[:-k, k:] - s[k:, :-k] + s[:-k, :-k]
    return out[:h.shape[0], :h.shape[1]] / (k * k)


def spectrum_slope(h2: Any, step: float, lo: float = 16.0, hi: float = 256.0) -> float:
    """beta in P(k) ~ k^-beta of the radially averaged power spectrum, wavelengths lo .. hi metres (plane removed,
    Hann window)."""
    m = h2.shape[0]
    z, x = np.mgrid[0:m, 0:m]
    a = np.c_[x.ravel(), z.ravel(), np.ones(m * m)]
    coef = np.linalg.lstsq(a, h2.ravel(), rcond=None)[0]
    d = h2 - (a @ coef).reshape(m, m)
    w = np.hanning(m)
    p = np.abs(np.fft.fft2(d * np.outer(w, w))) ** 2
    k = np.fft.fftfreq(m, step)
    kr = np.hypot(k[None, :], k[:, None])
    sel = (kr >= 1 / hi) & (kr <= 1 / lo)
    bins = np.logspace(math.log10(1 / hi), math.log10(1 / lo), 13)
    idx = np.digitize(kr[sel], bins)
    pk = [(float(kr[sel][idx == b].mean()), float(p[sel][idx == b].mean())) for b in range(1, len(bins)) if (idx == b).any()]
    lk, lp_ = np.log10([q[0] for q in pk]), np.log10([q[1] for q in pk])
    return float(-np.polyfit(lk, lp_, 1)[0])


def heights_of(d: bytes) -> tuple[Any, tuple[float, ...], int]:
    b = struct.unpack_from('<6f', d)
    n = struct.unpack_from('<I', d, 0x18)[0]
    v = np.frombuffer(d, '<u2', n * n, 0x31).reshape(n, n)
    h = (b[1] + v.astype(np.float32) * np.float32((b[4] - b[1]) / 65535)).astype(np.float32)
    return h, b, n


def shape(h: Any, b: tuple[float, ...], n: int, water_y: float | None) -> dict[str, Any]:
    """TGEN-SHAPE-1 .. 6 for one main block (heights in metres, rows along z)."""
    cell = (b[3] - b[0]) / n
    gz, gx = np.gradient(h, cell)
    slope = np.degrees(np.arctan(np.hypot(gx, gz)))
    h2 = h.reshape(n // 2, 2, n // 2, 2).mean(axis=(1, 3))         # 2 m grid
    rough8 = float(np.std(h2 - box_blur(h2, 2)))                   # 10 m window
    rough64 = float(np.std(h2 - box_blur(h2, 16)))                 # 66 m window
    lo, hi = float(h.min()), float(h.max())
    ring = max(1, n // 16)
    edge = np.concatenate([h[:ring].ravel(), h[-ring:].ravel(), h[:, :ring].ravel(), h[:, -ring:].ravel()])
    q = n // 4
    centre = h[q:-q, q:-q]
    out: dict[str, Any] = {
        'n': n, 'cell': round(cell, 3), 'range': round(b[4] - b[1], 1),
        'centred': b[0] == -b[3] and b[2] == -b[5] and b[3] - b[0] == n * cell and b[1] == 0,
        'elev': pct(h, (0, 1, 10, 50, 90, 99, 100)),
        'hi': round((float(h.mean()) - lo) / max(hi - lo, 1e-6), 3),
        'slope': pct(slope, (10, 50, 90, 99)),
        'steep': [round(float((slope > t).mean()), 3) for t in (15, 30, 45)],
        'rough8': round(rough8, 2), 'rough64': round(rough64, 2),
        'beta': round(spectrum_slope(h2, 2 * cell), 2),
        'edge_minus_centre': round(float(np.median(edge) - np.median(centre)), 1),
        'water_y': None if water_y is None else round(water_y, 2),
        'wet': 0.0, 'coast_km': 0.0,
    }
    if water_y is not None:
        wet2 = h2 < water_y
        out['wet'] = round(float((h < water_y).mean()), 3)
        out['coast_km'] = round(float((wet2[1:] != wet2[:-1]).sum() + (wet2[:, 1:] != wet2[:, :-1]).sum())
                                * 2 * cell / 1000, 1)
    return out


def water_level(d: bytes) -> tuple[float, int]:
    """WATER-MESH-1: the median vertex height over every body (NaN for a mesh with no vertices: SP_006, SP_009), and
    the body count."""
    o, ys = 4, []
    bodies = struct.unpack_from('<I', d)[0]
    for _ in range(bodies):
        v = struct.unpack_from('<I', d, o)[0]
        ys.append(np.frombuffer(d, '<f4', 3 * v, o + 4).reshape(v, 3)[:, 1])
        o += 4 + 12 * v
        c = struct.unpack_from('<I', d, o)[0]
        o += 4 + 4 * c
    y = np.concatenate(ys) if ys else np.zeros(0)
    return float(np.median(y)) if len(y) else float('nan'), bodies


def folder(name: str, depth: int = 2) -> str:
    return '/'.join(name.lower().replace('\\', '/').split('/')[:depth])


def terrains(bundles: list[Path]) -> dict[str, dict[str, Any]]:
    """Per terrain (lower case name): the main block's shape, the material map's shares, the visual terrain's atlas,
    patch shaders and undergrowth."""
    seen: set[str] = set()
    raw: dict[str, dict[str, Any]] = collections.defaultdict(dict)
    for path in bundles:
        es = [e for e in fbrb.entries(path) if not e.deleted and e.name.lower() not in seen
              and e.type in ('TerrainHeightfield', 'TerrainMaterialMap', 'VisualTerrain', 'WaterMesh')]
        es = [e for e in es if e.type != 'TerrainHeightfield' or e.name.lower().endswith('-000.res')]
        for e in es:
            seen.add(e.name.lower())
            t = e.name.split('/')[1].lower()
            d = fbrb.read(path, [e.name])[e.name]
            if e.type == 'TerrainHeightfield':
                raw[t]['hf'] = d
                raw[t]['src'] = (path, e.name)
            elif e.type == 'WaterMesh':
                raw[t]['water'] = water_level(d)
            elif e.type == 'TerrainMaterialMap':
                mm = tp.material_map(d)
                cells = np.bincount(mm['cells'].ravel(), minlength=16)[:len(mm['names'])]
                raw[t]['materials'] = {nm.lower(): round(float(c) / mm['cells'].size, 3)
                                       for nm, c in zip(mm['names'], cells, strict=True)}
            else:
                vt = tp.visual_terrain(d)
                header = vt['maps'][0][0]
                ug = {m[0].lower() for s in vt['shaders'] for m in s[4]}
                raw[t]['visual'] = {
                    'blocks': vt['n'], 'atlas': ((header[2] + 1) * 64, (header[3] + 1) * 64), 'W': header[4],
                    'patch_shaders': len(vt['shaders']),
                    'tiled': Counter(s[3] for s in vt['shaders']),
                    'with_undergrowth': sum(1 for s in vt['shaders'] if s[1]),
                    'ug_meshes': sorted(ug),
                    'road_strips': len(vt['strips']),
                }
        for r in raw.values():           # a terrain's water mesh is in its height field's bundle (TERR-NAME-1)
            if 'hf' in r:
                h, b, n = heights_of(r.pop('hf'))
                wy = r.get('water', (None, 0))[0]
                r['shape'] = shape(h, b, n, None if wy is None or math.isnan(wy) else wy)
                r['bounds'] = b
    return dict(raw)


# ---------------------------------------------------------------------------------------------------- level partitions

def nums(e: dbx.Element | None) -> list[float]:
    if e is None or e.kind != dbx.NUMBERS:
        return []
    size, vals = dbx.numbers(e)
    return [struct.unpack('>f', v)[0] for v in vals] if size == 4 else []


def flag(e: dbx.Element | None) -> bool:
    return e is not None and e.kind != dbx.CONTAINER and e.kind != dbx.NUMBERS and dbx.value(e) == 'true'


def level_ground(f: dbx.DbxFile) -> dict[str, Any]:
    """A level partition's terrain, combat areas (LEVEL-ENT-2: Geometry outlines of AreaGeometryEntityData chained
    by Next), capture points (bases: UnableToChangeTeam) and vegetation / object blueprint folders."""
    by = {(i.attr('guid') or '').upper(): i for i in f.instances}
    kind = {g: (i.attr('type') or '').split('.')[-1] for g, i in by.items()}

    def pos(i: dbx.Element) -> tuple[float, float, float] | None:
        m = nums(i.child('Transform'))
        return (m[12], m[13], m[14]) if len(m) >= 15 else None

    def outline(start: str) -> list[tuple[float, float]]:
        pts, cur, seen = [], start, set()
        while cur in by and cur not in seen and kind[cur] == 'AreaGeometryEntityData':
            seen.add(cur)
            p = pos(by[cur])
            if p:
                pts.append((p[0], p[2]))
            cur = lp.ref(by[cur].child('Next')).upper()
        return pts

    areas: list[list[tuple[float, float]]] = []
    for g, i in by.items():
        if kind[g] != 'CombatAreaTriggerEntityData':
            continue
        for it in lp.items(i.child('Geometry')):
            r = (it.attr('ref') or '').upper()
            pts = outline(r)
            if len(pts) >= 3 and pts not in areas:
                areas.append(pts)
    cps = []
    for g, i in by.items():
        if kind[g] == 'CapturePointEntityData':
            p = pos(i)
            if p:
                cps.append((p, flag(i.child('UnableToChangeTeam'))))
    blueprints: Counter[str] = Counter()
    veg: dict[str, Counter[str]] = {'forest': Counter(), 'middlegrowth': Counter()}
    for g, i in by.items():
        k = kind[g]
        if k in ('TreeMemberData', 'MiddlegrowthModelGroupMemberData', 'StaticModelGroupMemberData'):
            r = lp.ref(i.child('MemberType'))
            if lp.form(r) != 'external':
                continue
            n = int(lp.whole(i.child('InstanceCount')) or 0)
            target = {'TreeMemberData': veg['forest'], 'MiddlegrowthModelGroupMemberData': veg['middlegrowth']}
            target.get(k, blueprints)[r.rsplit('/', 1)[0].lower()] += n
        elif k == 'ReferenceObjectData':
            r = lp.ref(i.child('ReferencedObject'))
            if lp.form(r) == 'external':
                blueprints[r.rsplit('/', 1)[0].lower()] += 1
    terrain = None
    for g, i in by.items():
        if kind[g] == 'TerrainEntityData':
            parts = lp.ref(i.child('TerrainAsset')).split('/')
            terrain = parts[1].lower() if len(parts) >= 3 else None
    return {'terrain': terrain, 'areas': areas, 'cps': cps, 'blueprints': blueprints, **veg}


def inside(px: Any, pz: Any, poly: list[tuple[float, float]]) -> Any:
    """Even-odd point in polygon over arrays."""
    res = np.zeros(px.shape, bool)
    xs, zs = [p[0] for p in poly], [p[1] for p in poly]
    j = len(poly) - 1
    for i in range(len(poly)):
        xi, zi, xj, zj = xs[i], zs[i], xs[j], zs[j]
        cross = ((zi > pz) != (zj > pz)) & (px < (xj - xi) * (pz - zi) / ((zj - zi) or 1e-12) + xi)
        res ^= cross
        j = i
    return res


def play(lv: str, g: dict[str, Any], t: dict[str, Any], hf: tuple[Any, tuple[float, ...], int]) -> dict[str, Any]:
    """TGEN-PLAY-1 .. 4 for one level: the combat area (union of its outlines) on the terrain, and its capture
    points on the ground."""
    h, b, n = hf
    step = 4
    hs = h[::step, ::step]
    m = hs.shape[0]
    cell = (b[3] - b[0]) / n
    xs = b[0] + (np.arange(m) * step + 0.5) * cell
    zs = b[2] + (np.arange(m) * step + 0.5) * cell
    px, pz = np.meshgrid(xs, zs)
    mask = np.zeros(px.shape, bool)
    for poly in g['areas']:
        mask |= inside(px, pz, poly)
    area = float(mask.sum()) * (step * cell) ** 2
    gz, gx = np.gradient(h[::2, ::2], 2 * cell)
    slope = np.degrees(np.arctan(np.hypot(gx, gz)))[::step // 2, ::step // 2]
    wy = t['shape']['water_y']
    allx = np.concatenate([[p[0] for p in a] for a in g['areas']])
    allz = np.concatenate([[p[1] for p in a] for a in g['areas']])
    bx = (float(allx.max() - allx.min()), float(allz.max() - allz.min()))
    cx, cz = (b[0] + b[3]) / 2, (b[2] + b[5]) / 2
    off = math.hypot((allx.max() + allx.min()) / 2 - cx, (allz.max() + allz.min()) / 2 - cz) / (b[3] - b[0])
    ins = hs[mask] if mask.any() else hs.ravel()
    out: dict[str, Any] = {
        'terrain': g['terrain'], 'area_km2': round(area / 1e6, 3),
        'share_of_terrain': round(area / ((b[3] - b[0]) * (b[5] - b[2])), 3),
        'box_m': (round(bx[0]), round(bx[1])), 'box_aspect': round(max(bx) / max(min(bx), 1.0), 2),
        'convexity': 0.0,
        'centre_offset': round(off, 3),
        'relief_in': round(float(np.percentile(ins, 99) - np.percentile(ins, 1)), 1),
        'slope_in': pct(slope[mask], (50, 90)) if mask.any() else [],
        'slope_out': pct(slope[~mask], (50, 90)) if (~mask).any() else [],
        'wet_in': round(float((ins < wy).mean()), 3) if wy is not None else 0.0,
        'cps': [],
    }
    # convexity: union area over its convex hull's area (1 = convex)
    pts = np.c_[px[mask], pz[mask]] if mask.any() else np.zeros((0, 2))
    if len(pts) > 3:
        out['convexity'] = round(area / max(hull_area(pts), 1.0), 3)
    sorted_in = np.sort(ins)
    for (x, y, z), base in g['cps']:
        i, j = int((x - b[0]) / cell), int((z - b[2]) / cell)
        if not (0 <= i < n and 0 <= j < n):
            continue
        gh = float(h[j, i])
        r = int(100 / cell)
        win = h[max(0, j - r):j + r + 1, max(0, i - r):i + r + 1]
        s = float(slope[min(j // step, m - 1), min(i // step, m - 1)])
        wd = None
        if wy is not None:
            wet = np.argwhere(hs < wy)
            if len(wet):
                d2 = ((wet[:, 0] * step - j) ** 2 + (wet[:, 1] * step - i) ** 2).min()
                wd = round(math.sqrt(float(d2)) * cell)
        out['cps'].append({'base': base, 'pctile': round(float(np.searchsorted(sorted_in, gh)) / len(sorted_in), 3),
                           'slope': round(s, 1), 'relief100': round(gh - float(win.mean()), 1),
                           'above_ground': round(y - gh, 1), 'water_m': wd})
    return out


def hull_area(pts: Any) -> float:
    """Area of the convex hull (monotone chain)."""
    p = sorted(set(map(tuple, pts.tolist())))
    if len(p) < 3:
        return 0.0

    def half(seq: list[tuple[float, float]]) -> list[tuple[float, float]]:
        h: list[tuple[float, float]] = []
        for q in seq:
            while len(h) >= 2 and ((h[-1][0] - h[-2][0]) * (q[1] - h[-2][1])
                                   - (h[-1][1] - h[-2][1]) * (q[0] - h[-2][0])) <= 0:
                h.pop()
            h.append(q)
        return h

    hull = half(p)[:-1] + half(p[::-1])[:-1]
    xs, zs = np.array([q[0] for q in hull]), np.array([q[1] for q in hull])
    return float(abs(np.dot(xs, np.roll(zs, 1)) - np.dot(zs, np.roll(xs, 1))) / 2)


MODES = {'conquest': 'CQ', 'rush': 'Rush', 'squad rush': 'SR', 'squad deathmatch': 'SDM'}


def modes_of(game: Path) -> dict[str, str]:
    """Level (lower case) -> mode, from the game copy's maps.ini (OPFOR's catalogue: a key per mode, the value the
    level path), as the placement study L1 reads it; levels it does not list are left out."""
    import configparser
    cp = configparser.ConfigParser(strict=False, interpolation=None)
    cp.read(game / 'maps.ini', encoding='utf-8')
    return {v.split('/')[-1].lower(): MODES[k.lower()] for sec in cp.sections() for k, v in cp.items(sec)
            if k.lower() in MODES}


def read_level(path: Path, lv: str) -> dict[str, Any] | None:
    name = f'levels/{lv}.dbx'
    ent = fbrb.names(path).get(name)
    return None if ent is None else level_ground(dbx.parse(fbrb.read(path, [ent.name])[ent.name]))


def analyse(game: Path, ww2: bool = False) -> dict[str, Any]:
    bundles = tp.stock_bundles(game)
    ts = terrains(bundles)
    levels: dict[str, dict[str, Any]] = {}
    grounds: dict[str, dict[str, Any]] = {}
    for lv, path in sorted(lp.level_bundles(game).items()):
        g = read_level(path, lv)
        if g is None or g['terrain'] not in ts:
            continue
        grounds[lv] = g
        a = ts[g['terrain']].setdefault('assets', {'forest': Counter(), 'middlegrowth': Counter(),
                                                    'blueprints': Counter(), 'levels': []})
        a['levels'].append(lv)
        for k in ('forest', 'middlegrowth', 'blueprints'):
            a[k].update(g[k])
    for r in ts.values():
        a = r.get('assets')
        r['biome'] = biome_of(a['forest'] + a['middlegrowth']) if a else 'arid'
    hf: tuple[str, Any] | None = None
    modes = modes_of(game)
    for lv, g in grounds.items():
        t = g['terrain']
        if not g['areas'] or lv not in modes:
            continue
        if hf is None or hf[0] != t:
            src, entry = ts[t]['src']
            hf = (t, heights_of(fbrb.read(src, [entry])[entry]))
        levels[lv] = play(lv, g, ts[t], hf[1])
        levels[lv]['mode'] = modes[lv]
    out: dict[str, Any] = {'terrains': ts, 'levels': levels}
    if ww2:
        w = terrains([p for lv in WW2 for p in (game / 'Package' / 'levels' / lv).glob('terrain-00.fbrb')])
        for lv in WW2:
            g = read_level(game / 'Package' / 'levels' / lv / 'level-00.fbrb', lv)
            if g and g['terrain'] in w:
                w[g['terrain']]['assets'] = {k: g[k] for k in ('forest', 'middlegrowth', 'blueprints')}
        out['ww2'] = w
    return out


def biome_check(r: dict[str, Any]) -> bool:
    """TGEN-BIOME-1: the biome against the material map: snow names a snow material; arid has no grass or forest
    majority; the others name no snow material."""
    mats = r.get('materials', {})
    names = ' '.join(mats)
    b = r.get('biome')
    if b == 'snow':
        return 'snow' in names
    if b == 'arid':
        return mats.get('grass', 0) + mats.get('forest', 0) < 0.5
    return 'snow' not in names


def flags_on_ground(lvs: dict[str, Any]) -> list[dict[str, Any]]:
    """Conquest flags (capture points a team can take) whose entity stands on the ground (within 5 m)."""
    return [c for v in lvs.values() if v['mode'] == 'CQ' for c in v['cps'] if not c['base'] and c['above_ground'] < 5]


def med(vals: list[float]) -> float:
    return round(float(np.median(vals)), 2) if vals else float('nan')


def summary(got: dict[str, Any]) -> dict[str, Any]:
    ts, lvs = got['terrains'], got['levels']
    sh = [r['shape'] for r in ts.values() if 'shape' in r]
    cq = [v for v in lvs.values() if v['mode'] == 'CQ']
    fl = flags_on_ground(lvs)
    return {
        'terrains': len(sh),
        'main_blocks_centred_from_0': sum(bool(x['centred']) for x in sh),
        'sizes': dict(sorted(Counter((x['n'], x['cell']) for x in sh).items())),
        'terrains_by_biome': dict(sorted(Counter(r['biome'] for r in ts.values()).items())),
        'biome_matches_materials': sum(biome_check(r) for r in ts.values()),
        'water_meshes': sum('water' in r for r in ts.values()),
        'terrains_with_ground_under_water': sum(1 for s in sh if s['wet'] > 0.01),
        'slope_p50_deg_median': med([s['slope'][1] for s in sh]),
        'spectrum_beta_range': (round(min(s['beta'] for s in sh), 1), round(max(s['beta'] for s in sh), 1)),
        'levels_with_combat_area': len(lvs),
        'cq_levels': len(cq),
        'cq_flags_on_ground': len(fl),
        'cq_area_thousand_m2_median': med([round(1000 * v['area_km2']) for v in cq]),
        'cq_share_of_terrain_max': round(max((v['share_of_terrain'] for v in cq), default=0), 3),
        'cq_flags_slope_under_10': round(sum(c['slope'] < 10 for c in fl) / max(len(fl), 1), 2),
    }


# What the probe measured on DICE's files (2026-11-23); checks() holds the game copy to it (TGEN-REQ-1, TGEN-REQ-2,
# TGEN-BIOME-1, TGEN-SHAPE-1 - 3, TGEN-PLAY-1 - 3)
EXPECTED: dict[str, Any] = {
    'terrains': 30,
    'main_blocks_centred_from_0': 30,
    'sizes': {(1024, 1.0): 11, (2048, 1.0): 19},
    'terrains_by_biome': {'arid': 5, 'jungle': 9, 'snow': 6, 'temperate': 5, 'vietnam': 5},
    'biome_matches_materials': 30,
    'water_meshes': 30,
    'terrains_with_ground_under_water': 27,
    'slope_p50_deg_median': 20.4,
    'spectrum_beta_range': (3.2, 4.6),
    'levels_with_combat_area': 61,
    'cq_levels': 16,
    'cq_flags_on_ground': 50,
    'cq_area_thousand_m2_median': 98.5,
    'cq_share_of_terrain_max': 0.164,
    'cq_flags_slope_under_10': 0.92,
}


def checks(game: Path) -> list[tuple[bool, str]]:
    got = summary(analyse(game))
    return [(got.get(k) == want, f'terrain_shape probe: {k} = {want!r} (got {got.get(k)!r})')
            for k, want in EXPECTED.items()]


SHAPE_KEYS = ('range', 'hi', 'rough8', 'rough64', 'beta', 'edge_minus_centre', 'wet', 'coast_km')


def shape_line(t: str, r: dict[str, Any]) -> str:
    s = r['shape']
    return (f"  {t:16s} {r.get('biome', '-'):9s} {s['n']:5d} {s['range']:5.0f} | {s['elev']} | {s['hi']} | "
            f"{s['slope']} | {s['steep']} | {s['rough8']} {s['rough64']} | {s['beta']} | {s['edge_minus_centre']} | "
            f"{s['water_y']}, {s['wet']}, {s['coast_km']}")


def assets_line(a: dict[str, Counter[str]]) -> str:
    out = []
    for k in ('forest', 'middlegrowth', 'blueprints'):
        c: Counter[str] = Counter()
        for name, n in a[k].items():
            c[folder(name, 3 if k != 'blueprints' else 2)] += n
        out.append(f'{k} {len(a[k])} types / {sum(a[k].values())}: {dict(c.most_common(5))}')
    rocks = {k: n for k, n in a['blueprints'].items() if re.search(r'rock|cliff|stone|boulder', k)}
    out.append(f'rocks / cliffs {len(rocks)} types / {sum(rocks.values())}')
    return '\n      '.join(out)


def report(got: dict[str, Any]) -> None:
    ts = got['terrains']
    print('== TGEN-SHAPE: main blocks (terrain biome n range | elev p0 p1 p10 p50 p90 p99 p100 | HI | slope p10 p50 '
          'p90 p99 | share > 15 30 45 deg | relief 10 m 66 m | beta | border - centre | water y, wet, coast km)')
    for t, r in sorted(ts.items()):
        if 'shape' in r:
            print(shape_line(t, r))
    print('== TGEN-SHAPE per biome (medians; min .. max)')
    for b in sorted({r['biome'] for r in ts.values()}):
        rs = [r['shape'] for r in ts.values() if r['biome'] == b and 'shape' in r]
        parts = [f"slope p50 {med([s['slope'][1] for s in rs])} ({min(s['slope'][1] for s in rs)} .. "
                 f"{max(s['slope'][1] for s in rs)})"]
        for k in SHAPE_KEYS:
            vals = [float(s[k]) for s in rs]
            parts.append(f'{k} {med(vals)} ({min(vals):g} .. {max(vals):g})')
        print(f'  {b:9s} {len(rs)}: ' + '; '.join(parts))
    print('== TGEN-BIOME: materials, atlas, patch shaders, undergrowth, assets of the levels on the terrain')
    for t, r in sorted(ts.items()):
        v = r.get('visual', {})
        mats = ', '.join(f'{k} {x:.2f}' for k, x in sorted(r.get('materials', {}).items(), key=lambda kv: -kv[1]))
        print(f"  {t:16s} {r['biome']:9s} materials ok {biome_check(r)}: atlas {v.get('atlas')} W {v.get('W')} "
              f"blocks {v.get('blocks')} patch shaders {v.get('patch_shaders')} tiled "
              f"{dict(sorted(v.get('tiled', Counter()).items()))} with undergrowth {v.get('with_undergrowth')} "
              f"undergrowth meshes {len(v.get('ug_meshes', []))} road strips {v.get('road_strips')}")
        print(f'      materials: {mats}')
        print(f"      undergrowth folders: {dict(Counter(folder(m, 3) for m in v.get('ug_meshes', [])).most_common(4))}")
        if 'assets' in r:
            print(f"      levels {r['assets']['levels']}\n      {assets_line(r['assets'])}")
    print("== TGEN-ASSET per biome (union of its terrains' levels)")
    for b in sorted({r['biome'] for r in ts.values()}):
        a: dict[str, Counter[str]] = {'forest': Counter(), 'middlegrowth': Counter(), 'blueprints': Counter()}
        ug: set[str] = set()
        names: set[str] = set()
        atl: Counter[Any] = Counter()
        for r in ts.values():
            if r['biome'] != b:
                continue
            ug |= set(r.get('visual', {}).get('ug_meshes', []))
            names |= set(r.get('materials', {}))
            atl[r.get('visual', {}).get('atlas')] += 1
            for k in a:
                a[k].update(r.get('assets', {}).get(k, Counter()))
        print(f'  {b:9s} materials {sorted(names)}; atlases {dict(atl)}; undergrowth meshes {len(ug)}')
        print(f'      {assets_line(a)}')
    print('== TGEN-PLAY: levels with a combat area (level mode terrain | km2, share of terrain, box m, aspect, '
          'convexity, centre offset | relief inside (p99 - p1) | slope inside p50 p90 / outside | ground under water '
          'inside)')
    for lv, v in sorted(got['levels'].items()):
        print(f"  {lv:18s} {v['mode']:4s} {v['terrain']:14s} | {v['area_km2']} {v['share_of_terrain']} {v['box_m']} "
              f"{v['box_aspect']} {v['convexity']} {v['centre_offset']} | {v['relief_in']} | {v['slope_in']} / "
              f"{v['slope_out']} | {v['wet_in']}")
    fl = flags_on_ground(got['levels'])
    print(f"== TGEN-PLAY: {len(fl)} Conquest flags on the ground: height percentile in the combat area "
          f"{pct([c['pctile'] for c in fl], (10, 50, 90))}; slope {pct([c['slope'] for c in fl], (10, 50, 90))}; "
          f"relief against 100 m {pct([c['relief100'] for c in fl], (10, 50, 90))}; metres to water "
          f"{pct([c['water_m'] for c in fl if c['water_m'] is not None], (10, 50, 90))}")
    for mode in ('CQ', 'Rush', 'SR', 'SDM'):
        vs = [v for v in got['levels'].values() if v['mode'] == mode]
        if vs:
            print(f"  {mode:4s} {len(vs)} levels: thousand m2 {pct([1000 * v['area_km2'] for v in vs], (0, 50, 100))}, share of "
                  f"terrain {pct([100 * v['share_of_terrain'] for v in vs], (0, 50, 100))} %, aspect "
                  f"{pct([v['box_aspect'] for v in vs], (0, 50, 100))}, convexity "
                  f"{pct([100 * v['convexity'] for v in vs], (0, 50, 100))} %, centre offset "
                  f"{pct([100 * v['centre_offset'] for v in vs], (0, 50, 100))} % of the side, relief inside "
                  f"{pct([v['relief_in'] for v in vs], (0, 50, 100))} m")
    if 'ww2' in got:
        print('== WW2 (OPFOR conversions of BF1943, not DICE data)')
        for t, r in sorted(got['ww2'].items()):
            if 'shape' in r:
                print(shape_line(t, r))
            v = r.get('visual', {})
            print(f"      atlas {v.get('atlas')} patch shaders {v.get('patch_shaders')} undergrowth meshes "
                  f"{v.get('ug_meshes')}; materials {sorted(r.get('materials', {}))}")
            if 'assets' in r:
                print(f"      {assets_line(r['assets'])}")
    for k, val in summary(got).items():
        print(f'{k}: {val}')


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    ap.add_argument('--ww2', action='store_true', help="also OPFOR's four WW2 terrains (printed apart)")
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    report(analyse(game, a.ww2))


if __name__ == '__main__':
    main()
