"""Soldier weapons as the game copy ships them: the DBX classes of a weapon, its firing and its projectile, and the
measured ranges of their balance fields per weapon class (read-only; spec/engine/weapons.md).

    python spec/probes/weapons.py [--opfor <folder>] [--all]

Reads every bundle of Project OPFOR BFBC2's game copy (`game/**/*.fbrb`), one at a time, and takes each partition
under `objects/weapons/` once, from the first bundle that holds it, in the order the game prefers its copies: Package
before Dist (a Package copy is DICE's later patch, PREC-ORD-3), each tree in sorted path order. DICE's bundles only
(dated before 2013, as the install leaves them) unless --all. It also reads the Dist copy of every partition that
has a Package copy, to count what the patches changed.

Prints structure, counts and numeric ranges only (DBX class, field and enum names; weapon folders appear only as
counts per class), never strings in bulk:

- corpus: bundles, partitions, copies, Package copies that differ from Dist's (the balance patches);
- classes: instance types over the weapon partitions;
- per weapon class (`SoldierWeaponData.WeaponClass`, players' weapons; the AI clones counted apart): the weapons, their
  firings' projectile classes, damage and falloff, muzzle speed, gravity, rate of fire, magazines and pickups,
  recoil and sway, zoom levels and field of view, sprint look, AlwaysAimHead, AnimBaseSet, auto-aim;
- conventions: how many player firearms hold each of the conventions the WW2 balance ruling names, overall and on
  the weapons in BF1943's folder (`objects/weapons/handheld/beach/`);
- attachments: specializations per weapon, those that swap in another weapon asset;
- the patch: which balance fields the Package copies changed, per class.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import struct
import sys
import time
from collections.abc import Callable, Iterable
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths

PREFIX = 'objects/weapons/'
INTS = {'MagazineCapacity', 'NumberOfMagazines', 'AmmoPickupMinAmount', 'AmmoPickupMaxAmount', 'NumberOfBulletsPerShell',
        'NumberOfBulletsPerShot', 'AmmoBoxPickupAmount', 'TraceFrequency'}
BEACH = 'objects/weapons/handheld/beach/'
AI = 'objects/weapons/handheld/aiclones/'


# ------------------------------------------------------------------------------------------------ the corpus

def bundles(game: Path, every: bool) -> list[Path]:
    """Every bundle, Package before Dist, each tree sorted; DICE's only (dated before 2013) unless every."""
    out = []
    for top in (game / 'Package', game / 'Dist' / 'win32'):
        for p in sorted(top.glob('**/*.fbrb'), key=lambda q: str(q).lower()):
            if every or time.gmtime(p.stat().st_mtime).tm_year < 2013:
                out.append(p)
    return out


def is_dist(p: Path, game: Path) -> bool:
    return p.relative_to(game).parts[0].lower() == 'dist'


def index(items: list[Path], want: Callable[[str], bool]) -> dict[str, list[tuple[Path, str]]]:
    """{lower name: [(bundle, name) ...]} of every live entry want() accepts, bundles in order."""
    out: dict[str, list[tuple[Path, str]]] = {}
    for b in items:
        for e in fbrb.entries(b):
            low = e.name.lower()
            if not e.deleted and want(low):
                out.setdefault(low, []).append((b, e.name))
    return out


def read(where: dict[str, tuple[Path, str]], chunk: int = 400) -> Iterable[tuple[str, bytes]]:
    """(lower name, bytes) for each name at its bundle, one bundle (and chunk of names) at a time."""
    per: dict[Path, list[str]] = collections.defaultdict(list)
    for _low, (b, name) in where.items():
        per[b].append(name)
    for b, names in per.items():
        for k in range(0, len(names), chunk):
            for name, data in fbrb.read(b, names[k:k + chunk]).items():
                yield name.lower(), data


# ------------------------------------------------------------------------------------------------ values

def kid(e: dbx.Element | None, path: str) -> dbx.Element | None:
    for step in path.split('.'):
        if e is None:
            return None
        e = e.child(step)
    return e


def num(e: dbx.Element | None, path: str = '', at: int = 0) -> float | None:
    """A number read from its binary value (int for the count fields, else f32); at: the item of a vector."""
    x = kid(e, path) if path else e
    if x is None or x.kind != dbx.NUMBERS:
        return None
    size, vals = dbx.numbers(x)
    if size != 4 or len(vals) <= at:
        return None
    name = path.rsplit('.', 1)[-1] if path else (x.attr('name') or '')
    if name in INTS:
        return float(struct.unpack('>i', vals[at])[0])
    return float(struct.unpack('>f', vals[at])[0])


def word(e: dbx.Element | None, path: str) -> str | None:
    x = kid(e, path)
    if x is None:
        return None
    if x.kind == dbx.TEXT:
        return x.text
    if x.kind == dbx.NUMBERS or x.kind == dbx.CONTAINER:
        return None
    return dbx.value(x)


def ref(e: dbx.Element | None, path: str) -> str | None:
    x = kid(e, path)
    r = x.attr('ref') if x is not None else None
    return None if r in (None, 'null') else r


def short(e: dbx.Element) -> str:
    return (e.attr('type') or '').rsplit('.', 1)[-1]


def rng(given: Iterable[float | None]) -> str:
    vals = [v for v in given if v is not None]
    if not vals:
        return '-'
    c = collections.Counter(round(v, 4) for v in vals)
    if len(c) <= 4:
        return ', '.join(f'{v:g} x{n}' for v, n in sorted(c.items()))
    return f'{min(vals):g} .. {max(vals):g} ({len(c)} distinct; mode {c.most_common(1)[0][0]:g})'


def tally(vals: Iterable[object]) -> str:
    c = collections.Counter(v for v in vals)
    return ', '.join(f'{v} x{n}' for v, n in c.most_common()) or '-'


class Parts:
    """The parsed partitions, by lower name; an instance found by a reference (local guid or partition/guid)."""

    def __init__(self) -> None:
        self.files: dict[str, dbx.DbxFile] = {}
        self.xml: dict[str, str] = {}                               # the XML form's digest (the same content)
        self.by_guid: dict[str, dict[str, dbx.Element]] = {}

    def add(self, name: str, data: bytes) -> None:
        try:
            f = dbx.parse(data)
        except ValueError:
            return
        self.files[name] = f
        self.xml[name] = hashlib.md5(dbx.to_xml(data).encode('utf-8')).hexdigest()
        self.by_guid[name] = {(i.attr('guid') or '').lower(): i for i in f.instances}

    def resolve(self, part: str, r: str | None) -> tuple[str, dbx.Element] | None:
        if not r:
            return None
        if '/' in r:
            p, g = r.rsplit('/', 1)
            p = p.lower() + '.dbx'
        else:
            p, g = part, r
        i = self.by_guid.get(p, {}).get(g.lower())
        return (p, i) if i is not None else None


# ------------------------------------------------------------------------------------------------ the weapons

class Weapon:
    def __init__(self, name: str, inst: dbx.Element) -> None:
        self.name = name
        self.inst = inst
        self.cls = word(inst, 'WeaponClass') or '?'
        self.firing: tuple[str, dbx.Element] | None = None      # WeaponFiringData
        self.function: tuple[str, dbx.Element] | None = None    # FiringFunctionData
        self.projectile: tuple[str, dbx.Element] | None = None
        self.sway: tuple[str, dbx.Element] | None = None
        self.aim: tuple[str, dbx.Element] | None = None
        self.zooms: list[dbx.Element] = []

    @property
    def group(self) -> str:
        if self.name.startswith(AI):
            return 'ai'
        if self.name.startswith(BEACH):
            return 'beach'
        if 'vietnam/' in self.name:
            return 'vietnam'
        if self.name.startswith('objects/weapons/stationary/'):
            return 'stationary'
        return 'bfbc2'

    def speed(self) -> float | None:
        return num(self.function[1], 'Shot.InitialSpeed', 2) if self.function else None

    def ptype(self) -> str:
        return short(self.projectile[1]) if self.projectile else '-'

    def firearm(self) -> bool:
        s = self.speed()
        return self.ptype() == 'BulletEntityData' and s is not None and s >= 100


def weapons(P: Parts) -> list[Weapon]:
    out = []
    for name, f in P.files.items():
        for i in f.instances:
            if short(i) != 'SoldierWeaponData':
                continue
            w = Weapon(name, i)
            w.firing = P.resolve(name, ref(i, 'WeaponFiring'))
            if w.firing:
                w.function = P.resolve(w.firing[0], ref(w.firing[1], 'PrimaryFire'))
                w.sway = P.resolve(w.firing[0], ref(w.firing[1], 'WeaponSway'))
            if w.function:
                w.projectile = P.resolve(w.function[0], ref(w.function[1], 'Shot.ProjectileData'))
            w.aim = P.resolve(name, ref(i, 'AimingController'))
            if w.aim:
                for it in (kid(w.aim[1], 'ZoomLevels') or dbx.Element('array', dbx.CONTAINER)).children or ():
                    z = P.resolve(w.aim[0], it.attr('ref'))
                    if z is not None:
                        w.zooms.append(z[1])
            out.append(w)
    return out


FIRING_FIELDS = [('StartDamage', 'p', 'StartDamage'), ('EndDamage', 'p', 'EndDamage'),
                 ('FalloffStart m', 'p', 'DamageFalloffStartDistance'), ('FalloffEnd m', 'p', 'DamageFalloffEndDistance'),
                 ('Gravity', 'p', 'Gravity'), ('TimeToLive s', 'p', 'TimeToLive'),
                 ('RateOfFire rpm', 'f', 'FireLogic.RateOfFire'), ('ReloadTime s', 'f', 'FireLogic.ReloadTime'),
                 ('MagazineCapacity', 'f', 'Ammo.MagazineCapacity'), ('NumberOfMagazines', 'f', 'Ammo.NumberOfMagazines'),
                 ('AmmoPickupMin', 'f', 'Ammo.AmmoPickupMinAmount'), ('AmmoPickupMax', 'f', 'Ammo.AmmoPickupMaxAmount'),
                 ('BulletsPerShell', 'f', 'Shot.NumberOfBulletsPerShell'),
                 ('MaxRecoilAngleX', 'f', 'FireLogic.Recoil.MaxRecoilAngleX'),
                 ('MaxRecoilFov', 'f', 'FireLogic.Recoil.MaxRecoilFov'),
                 ('Stand.NoZoom.Base.Min', 's', 'Stand.NoZoom.BaseValue.MinAngle'),
                 ('Stand.NoZoom.Base.Max', 's', 'Stand.NoZoom.BaseValue.MaxAngle'),
                 ('Stand.Zoom.Base.Min', 's', 'Stand.Zoom.BaseValue.MinAngle'),
                 ('Stand.Zoom.Base.Max', 's', 'Stand.Zoom.BaseValue.MaxAngle'),
                 ('Stand.NoZoom.Base.IncPerShot', 's', 'Stand.NoZoom.BaseValue.IncreasePerShot'),
                 ('Stand.NoZoom.RecoilAmpMax', 's', 'Stand.NoZoom.Recoil.RecoilAmplitudeMax'),
                 ('Stand.Zoom.RecoilAmpMax', 's', 'Stand.Zoom.Recoil.RecoilAmplitudeMax'),
                 ('SuppressionModifier', 's', 'SuppressionModifier'),
                 ('DeviationScaleFactorNoZoom', 's', 'DeviationScaleFactorNoZoom')]


def firing_values(w: Weapon) -> dict[str, float | None]:
    src = {'p': w.projectile, 'f': w.function, 's': w.sway}
    out: dict[str, float | None] = {'InitialSpeed m/s': w.speed()}
    for label, where, path in FIRING_FIELDS:
        got = src[where]
        out[label] = num(got[1], path) if got else None
    return out


def weapon_values(w: Weapon) -> dict[str, object]:
    states = (kid(w.inst, 'WeaponStates') or dbx.Element('array', dbx.CONTAINER)).children or []
    heads = [word(s, 'AlwaysAimHead') for s in states]
    return {
        'AnimBaseSet': word(w.inst, 'AnimBaseSet'),
        'AlwaysAimHead': '/'.join(sorted({h for h in heads if h})) or '-',
        'ZoomType': word(w.aim[1], 'ZoomType') if w.aim else None,
        'ZoomLevels': len(w.zooms),
        'UseAutoAiming': word(w.firing[1], 'UseAutoAiming') if w.firing else None,
        'FireLogicType': word(w.function[1], 'FireLogic.FireLogicType') if w.function else None,
        'Specializations': len((kid(w.inst, 'Specializations') or dbx.Element('a', dbx.CONTAINER)).children or []),
        'SpecWeaponAssets': sum(1 for s in (kid(w.inst, 'Specializations') or dbx.Element('a', dbx.CONTAINER)).children
                                or [] if ref(s, 'WeaponAsset')),
    }


def report_classes(ws: list[Weapon]) -> None:
    by: dict[str, list[Weapon]] = collections.defaultdict(list)
    for w in ws:
        if w.group not in ('ai', 'stationary'):
            by[w.cls].append(w)
    print('\n== per weapon class (players\' handheld weapons; AI clones and stationary apart) ==')
    for cls in sorted(by):
        group = by[cls]
        print(f'\n-- {cls}: {len(group)} weapons ({tally(w.group for w in group)}) --')
        print(f'  projectile classes: {tally(w.ptype() for w in group)}')
        for k, v in (('FireLogicType', 'FireLogicType'), ('AnimBaseSet', 'AnimBaseSet'), ('ZoomType', 'ZoomType'),
                     ('ZoomLevels', 'ZoomLevels'), ('AlwaysAimHead', 'AlwaysAimHead'),
                     ('UseAutoAiming', 'UseAutoAiming')):
            print(f'  {k}: {tally(weapon_values(w)[v] for w in group)}')
        vals = [firing_values(w) for w in group]
        for label in vals[0] if vals else []:
            got = [v[label] for v in vals if v[label] is not None]
            print(f'  {label}: {rng([float(x) for x in got if x is not None])}')
        zs: dict[str, list[float]] = collections.defaultdict(list)
        for w in group:
            for zk, z in enumerate(w.zooms[:3]):
                for zf in ('FieldOfView', 'SprintLookSpeedMultiplier', 'LookSpeedMultiplier', 'RecoilMultiplier',
                           'SwayPitchMultiplier'):
                    zv = num(z, zf)
                    if zv is not None:
                        zs[f'zoom[{zk}].{zf}'].append(zv)
        for k in sorted(zs):
            print(f'  {k}: {rng(zs[k])}')
        steady = [num(w.aim[1], f'{p}.AimSteadiness') for w in group if w.aim for p in ('StandPose', 'CrouchPose', 'PronePose')]
        print(f'  AimSteadiness (stand/crouch/prone): {rng([v for v in steady if v is not None])}')


def profile(w: Weapon) -> str:
    f = firing_values(w)
    vals = [f[k] for k in ('StartDamage', 'EndDamage', 'FalloffStart m', 'FalloffEnd m')]
    if any(v is None for v in vals):
        return '-'
    return '{:g}->{:g} over {:g}-{:g} m'.format(*[float(v or 0) for v in vals])


def report_profiles(ws: list[Weapon]) -> None:
    """Damage profiles per class and fire logic, over distinct firings (variants that share a firing counted once)."""
    print('\n== damage profiles: distinct firings per class and FireLogicType (start->end over falloff start-end) ==')
    by: dict[tuple[str, str], dict[str, Weapon]] = collections.defaultdict(dict)
    for w in ws:
        if w.group in ('ai', 'stationary') or not w.firing or not w.firearm():
            continue
        by[(w.cls, str(weapon_values(w)['FireLogicType']))].setdefault(w.firing[0], w)
    for (cls, logic), firings in sorted(by.items()):
        group = list(firings.values())
        f = [firing_values(w) for w in group]
        print(f'  {cls} / {logic}: {len(group)} firings; {tally(profile(w) for w in group)}')
        for label in ('RateOfFire rpm', 'MagazineCapacity', 'NumberOfMagazines', 'AmmoPickupMin', 'AmmoPickupMax',
                      'Stand.NoZoom.Base.Min', 'Stand.NoZoom.Base.Max', 'Stand.NoZoom.RecoilAmpMax',
                      'Stand.Zoom.RecoilAmpMax', 'DeviationScaleFactorNoZoom'):
            print(f'    {label}: {rng([x[label] for x in f if x[label] is not None])}')
        heads = [weapon_values(w)['AlwaysAimHead'] for w in group]
        steady = [num(w.aim[1], 'StandPose.AimSteadiness') for w in group if w.aim]
        zr = [num(w.zooms[1], 'RecoilMultiplier') for w in group if len(w.zooms) > 1]
        zf = [num(w.zooms[1], 'RecoilFovMultiplier') for w in group if len(w.zooms) > 1]
        print(f'    AlwaysAimHead {tally(heads)}; stand AimSteadiness {rng([v for v in steady if v is not None])}; '
              f'zoom[1] RecoilMultiplier {rng([v for v in zr if v is not None])}; zoom[1] RecoilFovMultiplier '
              f'{rng([v for v in zf if v is not None])}')


def level_kinds(copies: list[tuple[Path, str]]) -> str:
    """'mp' / 'sp' / 'mp+sp': whether a partition has copies in multiplayer or campaign (sp_*) level bundles."""
    return '+'.join(sorted({'sp' if b.parent.name.lower().startswith('sp_') else 'mp' for b, _ in copies}))


def report_beach(ws: list[Weapon], ix: dict[str, list[tuple[Path, str]]]) -> None:
    """The weapons in BF1943's folder, one line each (folder names are DICE's structural names)."""
    print('\n== weapons in BF1943\'s folder (beach/), one line each ==')
    for w in sorted((w for w in ws if w.group == 'beach'), key=lambda w: w.name):
        f, v = firing_values(w), weapon_values(w)
        print(f'  {w.name[len(BEACH):-4]} ({level_kinds(ix[w.name])}): {w.cls} {w.ptype()} {profile(w)}; speed {f["InitialSpeed m/s"]}; '
              f'gravity {f["Gravity"]}; mags {f["NumberOfMagazines"]} pickup {f["AmmoPickupMin"]}/{f["AmmoPickupMax"]}; '
              f'{v["ZoomType"]}; aim head {v["AlwaysAimHead"]}; {v["AnimBaseSet"]}; auto-aim {v["UseAutoAiming"]}; '
              f'sprint look {rng([s for z in w.zooms if (s := num(z, "SprintLookSpeedMultiplier")) is not None])}')


def report_conventions(ws: list[Weapon]) -> None:
    print('\n== the WW2 ruling\'s conventions over player firearms (bullet, >= 100 m/s) ==')
    for label, pick in (('every player firearm', lambda w: w.group not in ('ai', 'stationary')),
                        ('BF1943\'s folder (beach/)', lambda w: w.group == 'beach'),
                        ('AI clones', lambda w: w.group == 'ai'),
                        ('stationary', lambda w: w.group == 'stationary')):
        fa = [w for w in ws if pick(w) and w.firearm()]
        other = [w for w in ws if pick(w) and not w.firearm()]
        f = [firing_values(w) for w in fa]
        v = [weapon_values(w) for w in fa]
        print(f'\n-- {label}: {len(fa)} firearms, {len(other)} other ({tally(w.ptype() for w in other)}) --')
        print(f'  Gravity: {rng([x["Gravity"] for x in f if x["Gravity"] is not None])}')
        print(f'  InitialSpeed: {rng([x["InitialSpeed m/s"] for x in f if x["InitialSpeed m/s"] is not None])}')
        print(f'  NumberOfMagazines: {rng([x["NumberOfMagazines"] for x in f if x["NumberOfMagazines"] is not None])}')
        print(f'  AmmoPickupMin/Max zero: {sum(1 for x in f if x["AmmoPickupMin"] == 0 and x["AmmoPickupMax"] == 0)}')
        print(f'  UseAutoAiming: {tally(x["UseAutoAiming"] for x in v)}')
        print(f'  ZoomType: {tally(x["ZoomType"] for x in v)}')
        print(f'  AlwaysAimHead: {tally(x["AlwaysAimHead"] for x in v)}')
        print(f'  AnimBaseSet: {tally(x["AnimBaseSet"] for x in v)}')
        sl = [num(z, 'SprintLookSpeedMultiplier') for w in fa for z in w.zooms]
        print(f'  SprintLookSpeedMultiplier (every zoom level): {rng([s for s in sl if s is not None])}')
        if other:
            g = [firing_values(w)['Gravity'] for w in other]
            n = [firing_values(w)['NumberOfMagazines'] for w in other]
            print(f'  others: Gravity {rng([x for x in g if x is not None])}; NumberOfMagazines '
                  f'{rng([x for x in n if x is not None])}')
    allw = [w for w in ws if w.firing]
    print(f'\n  UseAutoAiming over every WeaponFiringData a weapon names ({len(allw)}): '
          f'{tally(word(w.firing[1], "UseAutoAiming") for w in allw if w.firing)}; true on: '
          f'{tally(f"{w.cls} ({w.group})" for w in allw if w.firing and word(w.firing[1], "UseAutoAiming") == "true")}')


def report_projectiles(P: Parts, ws: list[Weapon]) -> None:
    print('\n== projectiles: the classes a firing\'s Shot.ProjectileData names (players\' weapons) ==')
    by: dict[str, list[Weapon]] = collections.defaultdict(list)
    for w in ws:
        if w.group not in ('ai',):
            by[w.ptype()].append(w)
    for t, group in sorted(by.items()):
        f = [firing_values(w) for w in group]
        print(f'  {t}: {len(group)} weapons over {len({w.projectile[0] for w in group if w.projectile})} partitions; '
              f'speed {rng([x["InitialSpeed m/s"] for x in f if x["InitialSpeed m/s"] is not None])}; gravity '
              f'{rng([x["Gravity"] for x in f if x["Gravity"] is not None])}; start damage '
              f'{rng([x["StartDamage"] for x in f if x["StartDamage"] is not None])}')
        exp = [w for w in group if w.projectile and ref(w.projectile[1], 'Explosion')]
        if exp:
            ex = [P.resolve(w.projectile[0], ref(w.projectile[1], 'Explosion')) for w in exp if w.projectile]
            got = [e for e in ex if e is not None]
            print(f'    with an Explosion: {len(exp)} ({tally(short(e[1]) for e in got)}); blast damage '
                  f'{rng([v for e in got if (v := num(e[1], "BlastDamage")) is not None])}; blast radius '
                  f'{rng([v for e in got if (v := num(e[1], "BlastRadius")) is not None])}; inner radius '
                  f'{rng([v for e in got if (v := num(e[1], "InnerBlastRadius")) is not None])}')
    secondary = [w for w in ws if w.firing and ref(w.firing[1], 'SecondaryFire') and w.group != 'ai']
    print(f'  weapons whose firing has a SecondaryFire: {len(secondary)}')
    # underbarrel launchers: a weapon whose AltWeaponSlot / SpecializationWeapons name another weapon
    alt = [w for w in ws if w.group != 'ai' and (kid(w.inst, 'SpecializationWeapons') or
                                                  dbx.Element('a', dbx.CONTAINER)).children]
    print(f'  weapons with SpecializationWeapons items: {len(alt)}')


def report_attachments(ws: list[Weapon]) -> None:
    print('\n== attachments: Specializations per player weapon ==')
    pl = [w for w in ws if w.group not in ('ai', 'stationary')]
    v = [weapon_values(w) for w in pl]
    print(f'  specializations per weapon: {tally(x["Specializations"] for x in v)}')
    print(f'  specializations that swap in a weapon asset, per weapon: {tally(x["SpecWeaponAssets"] for x in v)}')
    print(f'  weapons with a specialization swap, per class: '
          f'{tally(w.cls for w, x in zip(pl, v) if isinstance(x["SpecWeaponAssets"], int) and x["SpecWeaponAssets"])}')


def report_patch(P: Parts, dist: Parts, ws: list[Weapon]) -> None:
    print('\n== the patch: Package copies against Dist\'s ==')
    changed: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    examined = 0
    done: set[str] = set()
    for w in ws:
        if w.group in ('ai',) or not w.function or not w.firing or w.firing[0] in done:
            continue
        done.add(w.firing[0])
        if w.firing[0] not in dist.files:
            continue

        def old(part: str, r: str | None) -> tuple[str, dbx.Element] | None:
            return dist.resolve(part, r) or P.resolve(part, r)

        d = Weapon(w.name, w.inst)
        d.firing = old(w.firing[0], w.firing[1].attr('guid'))
        if d.firing is None:
            continue
        examined += 1
        d.function = old(d.firing[0], ref(d.firing[1], 'PrimaryFire'))
        d.sway = old(d.firing[0], ref(d.firing[1], 'WeaponSway'))
        if d.function:
            d.projectile = old(d.function[0], ref(d.function[1], 'Shot.ProjectileData'))
        a, b = firing_values(d), firing_values(w)
        for k in a:
            if a[k] is not None and b[k] is not None and abs((a[k] or 0) - (b[k] or 0)) > 1e-4:
                changed[w.cls][k] += 1
        changed[w.cls]['(firings compared)'] += 1
    print(f'  firings with a Dist copy compared: {examined}')
    for cls in sorted(changed):
        print(f'  {cls}: {", ".join(f"{k} {n}" for k, n in changed[cls].most_common())}')


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--opfor', help='Project OPFOR BFBC2\'s folder (default: found as the suite finds it)')
    ap.add_argument('--all', action='store_true', help='OPFOR\'s bundles too (default: DICE\'s only)')
    a = ap.parse_args(argv)
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    items = bundles(game, a.all)
    ix = index(items, lambda n: n.startswith(PREFIX) and n.endswith('.dbx'))
    copies = sum(len(v) for v in ix.values())
    print(f'== corpus: {len(items)} bundles ({"all" if a.all else "DICE\'s"}), {len(ix)} weapon partitions, '
          f'{copies} copies ==')
    P = Parts()
    first = {n: v[0] for n, v in ix.items()}
    for n, data in read(first):
        P.add(n, data)
    # the Dist copy of each partition whose first copy is a Package one (the patch's base)
    dist_where = {}
    for n, v in ix.items():
        if not is_dist(v[0][0], game):
            d = next((c for c in v if is_dist(c[0], game)), None)
            if d is not None:
                dist_where[n] = d
    dist = Parts()
    for n, data in read(dist_where):
        dist.add(n, data)
    differ = sum(1 for n in dist_where if n in P.xml and n in dist.xml and P.xml[n] != dist.xml[n])
    pkg_only = sum(1 for v in ix.values() if all(not is_dist(c[0], game) for c in v))
    print(f'  first copy in Package: {sum(1 for v in ix.values() if not is_dist(v[0][0], game))}; in Package only: '
          f'{pkg_only}; with a Dist copy too: {len(dist_where)}, of which differ from Dist\'s (XML form): {differ}')
    # every copy of every firing partition: how many distinct contents (XML form) each name has, in Package and overall
    firings = {n: v for n, v in ix.items() if n.endswith('_firing.dbx')}
    forms: dict[str, dict[bool, set[str]]] = collections.defaultdict(lambda: {True: set(), False: set()})
    per: dict[Path, list[str]] = collections.defaultdict(list)
    for v in firings.values():
        for b, name in v:
            per[b].append(name)
    for b, names in per.items():
        for name, data in fbrb.read(b, names).items():
            forms[name.lower()][is_dist(b, game)].add(hashlib.md5(dbx.to_xml(data).encode('utf-8')).hexdigest())
    pk = collections.Counter(len(f[False]) for f in forms.values() if f[False])
    every = collections.Counter(len(f[False] | f[True]) for f in forms.values())
    print(f'  firing partitions: {len(firings)}; distinct contents per name over its Package copies: '
          f'{", ".join(f"{k} form(s) x{n}" for k, n in sorted(pk.items()))}; over every copy: '
          f'{", ".join(f"{k} form(s) x{n}" for k, n in sorted(every.items()))}')
    beach = [n for n in ix if n.startswith(BEACH)]
    print(f'  partitions in BF1943\'s folder ({BEACH}): {len(beach)}')
    types: collections.Counter[str] = collections.Counter()
    for f in P.files.values():
        for i in f.instances:
            types[i.attr('type') or '?'] += 1
    print('\n== instance classes over the weapon partitions (first copies) ==')
    for t, cnt in types.most_common(45):
        print(f'  {cnt:5d} {t}')
    ws = weapons(P)
    print(f'\n  SoldierWeaponData: {len(ws)} ({tally(w.group for w in ws)}); with a resolved firing: '
          f'{sum(1 for w in ws if w.function)}; with a projectile: {sum(1 for w in ws if w.projectile)}')
    report_classes(ws)
    report_profiles(ws)
    report_beach(ws, ix)
    report_conventions(ws)
    report_projectiles(P, ws)
    report_attachments(ws)
    report_patch(P, dist, ws)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
