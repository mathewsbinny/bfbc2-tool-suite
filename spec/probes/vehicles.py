"""Vehicles as the game copy ships them: the DBX classes of a vehicle, its components, seats, health and weapons, and
the level spawners that place it (read-only; spec/engine/vehicles.md).

    python spec/probes/vehicles.py [--opfor <folder>] [--all] [--no-levels]

Reads every bundle of Project OPFOR BFBC2's game copy (`game/**/*.fbrb`), one at a time, and takes each partition
under `objects/vehicles/` and `objects/weapons/stationary/` once, from the first bundle that holds it: Package before
Dist (a Package copy is DICE's later patch, PREC-ORD-3), each tree in sorted path order; DICE's bundles only (dated
before 2013, as the install leaves them) unless --all. Then (unless --no-levels) every level partition
(`Levels/<LV>.dbx`) of those bundles, one at a time, for its vehicle spawners.

Prints structure, counts and numeric ranges only (DBX class, field, enum and material names, and the input-mapping
partitions' folder names, which are structural), never strings in bulk:

- corpus: bundles, partitions, VehicleEntityData instances by folder;
- kinds: each vehicle classed by its components (rotor: helicopter, wing: plane, track wheels: tracked, wheels:
  wheeled, a float-physics config without wheels: boat, else stationary) and by folder;
- per kind: component classes, seats (player and soldier entries), MaxPlayersInVehicle, the health zones' MaxHealth
  and damage-angle fields, the chassis and part health states, the physics material, the body mass, the engine
  config classes, the gears and wheels, the weapons and their projectiles;
- seats: the InputConceptDefinition / InputMapping partitions a driver seat (EntryOrderNumber 0) and the other seats
  name, per kind;
- spawners: VehicleSpawnEntityData per level, their delays, limits, abandon times and the folders of the vehicles
  they spawn;
- console-only: the partitions named for the Mark V per folder, and whether any is a vehicle.
"""
from __future__ import annotations

import argparse
import collections
import struct
import sys
import time
from collections.abc import Callable, Iterable
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths

PREFIXES = ('objects/vehicles/', 'objects/weapons/stationary/')
INTS = {'MaxPlayersInVehicle', 'EntryOrderNumber', 'MaxVehicles', 'MaxVehiclesPerMap', 'Amount',
        'NumberOfMagazines', 'MagazineCapacity', 'NumberOfBulletsPerShell', 'TakeControlEntryIndex'}


# ------------------------------------------------------------------------------------------------ the corpus

def bundles(game: Path, every: bool) -> list[Path]:
    """Every bundle, Package before Dist, each tree sorted; DICE's only (dated before 2013) unless every."""
    out = []
    for top in (game / 'Package', game / 'Dist' / 'win32'):
        for p in sorted(top.glob('**/*.fbrb'), key=lambda q: str(q).lower()):
            if every or time.gmtime(p.stat().st_mtime).tm_year < 2013:
                out.append(p)
    return out


def index(items: list[Path], want: Callable[[str], bool]) -> dict[str, list[tuple[Path, str]]]:
    """{lower name: [(bundle, name) ...]} of every live entry want() accepts, bundles in order."""
    out: dict[str, list[tuple[Path, str]]] = {}
    for b in items:
        for e in fbrb.entries(b):
            low = e.name.lower()
            if not e.deleted and want(low):
                out.setdefault(low, []).append((b, e.name))
    return out


def read(where: dict[str, tuple[Path, str]], chunk: int = 300) -> Iterable[tuple[str, bytes]]:
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


def num(e: dbx.Element | None, path: str) -> float | None:
    """A number read from its binary value (int for the count fields, else f32)."""
    x = kid(e, path)
    if x is None or x.kind != dbx.NUMBERS:
        return None
    size, vals = dbx.numbers(x)
    if size != 4 or not vals:
        return None
    if path.rsplit('.', 1)[-1] in INTS:
        return float(struct.unpack('>i', vals[0])[0])
    return float(struct.unpack('>f', vals[0])[0])


def word(e: dbx.Element | None, path: str) -> str | None:
    x = kid(e, path)
    if x is None or x.kind in (dbx.NUMBERS, dbx.CONTAINER):
        return None
    return x.text if x.kind == dbx.TEXT else dbx.value(x)


def ref(e: dbx.Element | None, path: str) -> str | None:
    x = kid(e, path)
    r = x.attr('ref') if x is not None else None
    return None if r in (None, 'null') else r


def items(e: dbx.Element | None, path: str) -> list[str]:
    x = kid(e, path)
    return [r for c in (x.children or []) if x is not None and (r := c.attr('ref')) not in (None, 'null')] \
        if x is not None else []


def short(e: dbx.Element) -> str:
    return (e.attr('type') or '').rsplit('.', 1)[-1]


def rng(vals: Iterable[float | None]) -> str:
    v = [x for x in vals if x is not None]
    if not v:
        return '-'
    c = collections.Counter(round(x, 4) for x in v)
    if len(c) <= 5:
        return ', '.join(f'{k:g} x{n}' for k, n in sorted(c.items()))
    return f'{min(v):g} .. {max(v):g} ({len(c)} distinct; mode {c.most_common(1)[0][0]:g})'


def tally(vals: Iterable[object]) -> str:
    return ', '.join(f'{v} x{n}' for v, n in collections.Counter(vals).most_common()) or '-'


def folder(r: str | None, depth: int = 3) -> str:
    """The first depth folders of a reference's partition (structural), or '-'."""
    if not r:
        return '-'
    return '/'.join(r.lower().split('/')[:depth]) if '/' in r else '(local)'


class Parts:
    """The parsed partitions, by lower name; an instance found by a reference (local guid or partition/guid)."""

    def __init__(self) -> None:
        self.files: dict[str, dbx.DbxFile] = {}
        self.by_guid: dict[str, dict[str, dbx.Element]] = {}

    def add(self, name: str, data: bytes) -> None:
        try:
            f = dbx.parse(data)
        except ValueError:
            return
        self.files[name] = f
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


# ------------------------------------------------------------------------------------------------ the vehicles

class Vehicle:
    def __init__(self, P: Parts, name: str, inst: dbx.Element) -> None:
        self.name, self.inst = name, inst
        self.comps: list[tuple[str, dbx.Element]] = []
        seen: set[int] = set()
        todo = [(name, r) for r in items(inst, 'Components')]
        while todo:
            part, r = todo.pop(0)
            got = P.resolve(part, r)
            if got is None or id(got[1]) in seen:
                continue
            seen.add(id(got[1]))
            self.comps.append(got)
            todo += [(got[0], c) for c in items(got[1], 'Components')]
        self.types = collections.Counter(short(c) for _, c in self.comps)
        chassis = [c for c in self.comps if short(c[1]) == 'VehicleComponentData']
        self.chassis = chassis[0] if chassis else None
        self.config = P.resolve(self.chassis[0], ref(self.chassis[1], 'VehicleConfig')) if self.chassis else None

    @property
    def group(self) -> str:
        parts = self.name.split('/')
        return 'stationary weapons' if parts[1] == 'weapons' else parts[2]

    @property
    def kind(self) -> str:
        """DICE's own class: HudData.VehicleType without its prefix."""
        return (word(self.inst, 'HudData.VehicleType') or '-').replace('VehicleHudType_', '')

    @property
    def build(self) -> str:
        """The class the components give (a cross-check of kind)."""
        t = self.types
        if t['RotorComponentData']:
            return 'helicopter'
        if t['WingComponentData']:
            return 'plane'
        if t['TrackWheelComponentData'] or t['TrackComponentData']:
            return 'tracked'
        if t['WheelComponentData']:
            return 'wheeled'
        if self.config is not None and ref(self.config[1], 'FloatPhysics'):
            return 'boat'
        return 'stationary'

    def of(self, typ: str) -> list[tuple[str, dbx.Element]]:
        return [c for c in self.comps if short(c[1]) == typ]


def health_states(P: Parts, part: str, inst: dbx.Element) -> list[float | None]:
    out = []
    for r in items(inst, 'HealthStates'):
        got = P.resolve(part, r)
        out.append(num(got[1], 'Health') if got else None)
    return out


def report_kinds(P: Parts, vs: list[Vehicle]) -> None:
    by: dict[str, list[Vehicle]] = collections.defaultdict(list)
    for v in vs:
        by[v.kind].append(v)
    for kind in sorted(by):
        g = by[kind]
        print(f'\n-- {kind}: {len(g)} vehicles ({tally(v.group for v in g)}) --')
        types: collections.Counter[str] = collections.Counter()
        for v in g:
            types.update(set(v.types))
        print(f'  component classes (vehicles having it): {", ".join(f"{t} {n}" for t, n in types.most_common(18))}')
        print(f'  player entries per vehicle: {rng(len(v.of("PlayerEntryComponentData")) for v in g)}; soldier entries: '
              f'{rng(len(v.of("SoldierEntryComponentData")) for v in g)}; MaxPlayersInVehicle: '
              f'{rng(num(v.inst, "MaxPlayersInVehicle") for v in g)}')
        print(f'  Material: {tally(word(v.inst, "Material") for v in g)}')
        for z in ('Front', 'Rear', 'Left', 'Right'):
            print(f'  {z}HealthZone MaxHealth {rng(num(v.inst, f"{z}HealthZone.MaxHealth") for v in g)}; '
                  f'MinDamageAngle {rng(num(v.inst, f"{z}HealthZone.MinDamageAngle") for v in g)}; '
                  f'DamageAngleMultiplier {rng(num(v.inst, f"{z}HealthZone.DamageAngleMultiplier") for v in g)}; '
                  f'UseDamageAngleCalculation {tally(word(v.inst, f"{z}HealthZone.UseDamageAngleCalculation") for v in g)}')
        print(f'  HealthZonesShareDamage: {tally(word(v.inst, "HealthZonesShareDamage") for v in g)}; LowHealthTreshold / '
              f'front MaxHealth: {rng(round((num(v.inst, "LowHealthTreshold") or 0) / m, 3) for v in g if (m := num(v.inst, "FrontHealthZone.MaxHealth")))}')
        for f in ('WaterDamage', 'UpsideDownDamage', 'CriticalDamageTime', 'KillSoldierCollisionSpeedThreshold'):
            print(f'  {f}: {rng(num(v.inst, f) for v in g)}')
        ch = [health_states(P, *v.chassis) for v in g if v.chassis]
        print(f'  chassis HealthStates: {tally("/".join(f"{x:g}" for x in s if x is not None) for s in ch)}')
        parts = [p for v in g for p in v.of('PartComponentData')]
        print(f'  PartComponentData per vehicle: {rng(len(v.of("PartComponentData")) for v in g)}; their first state health: '
              f'{rng(s[0] if (s := health_states(P, *p)) else None for p in parts)}')
        print(f'  body mass kg: {rng(num(v.config[1], "BodyMass") for v in g if v.config)}')
        eng = [P.resolve(e[0], ref(e[1], 'Config')) for v in g for e in v.of('EngineComponentData')]
        print(f'  engine configs: {tally(short(e[1]) for e in eng if e)}')
        gear = [P.resolve(v.chassis[0], ref(v.chassis[1], 'GearboxConfig')) for v in g if v.chassis]
        print(f'  gearbox configs: {tally(short(x[1]) for x in gear if x)}; forward gears: '
              f'{rng(float(len(dbx.numbers(e)[1])) for x in gear if x and (e := kid(x[1], "ForwardGearRatios")) is not None and e.kind == dbx.NUMBERS)}')
        print(f'  wheels per vehicle: {rng(v.types["WheelComponentData"] + v.types["TrackWheelComponentData"] for v in g)}')
        weapons = [(v, w) for v in g for w in v.of('WeaponComponentData')]
        print(f'  WeaponComponentData per vehicle: {rng(len(v.of("WeaponComponentData")) for v in g)}')
        ptypes: collections.Counter[str] = collections.Counter()
        dmg: list[float | None] = []
        blast: list[float | None] = []
        for _v, w in weapons:
            fd = P.resolve(w[0], ref(w[1], 'WeaponFiring'))
            fn = P.resolve(fd[0], ref(fd[1], 'PrimaryFire')) if fd else None
            pj = P.resolve(fn[0], ref(fn[1], 'Shot.ProjectileData')) if fn else None
            ptypes[short(pj[1]) if pj else '-'] += 1
            if pj:
                dmg.append(num(pj[1], 'StartDamage'))
                ex = P.resolve(pj[0], ref(pj[1], 'Explosion'))
                blast.append(num(ex[1], 'BlastDamage') if ex else None)
        print(f'  weapon projectiles: {", ".join(f"{k} x{n}" for k, n in ptypes.most_common()) or "-"}')
        print(f'  projectile StartDamage: {rng(dmg)}; explosion BlastDamage: {rng(blast)}')


def report_seats(vs: list[Vehicle]) -> None:
    print('\n== seats: the input partitions each seat names (folder of InputConceptDefinition + InputMapping) ==')
    by: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    orders: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    for v in vs:
        entries = sorted(v.of('PlayerEntryComponentData'), key=lambda e: num(e[1], 'EntryOrderNumber') or 0)
        orders[v.kind][','.join(f'{num(e[1], "EntryOrderNumber"):g}' for e in entries)] += 1
        for e in entries:
            seat = 'driver' if num(e[1], 'EntryOrderNumber') == 0 else 'other'
            key = f'{seat}: {folder(ref(e[1], "InputConceptDefinition"), 2)} + {folder(ref(e[1], "InputMapping"), 3)}'
            by[f'{v.kind} ({v.group})'][key] += 1
    for k in sorted(by):
        print(f'  {k}: {", ".join(f"{s} x{n}" for s, n in by[k].most_common())}')
    print('  EntryOrderNumber sequences per kind:')
    for k in sorted(orders):
        print(f'    {k}: {", ".join(f"[{s}] x{n}" for s, n in orders[k].most_common(6))}')


def report_spawners(items_: list[Path]) -> None:
    print('\n== spawners: VehicleSpawnEntityData in the level partitions ==')
    ix = index(items_, lambda n: n.startswith('levels/') and n.count('/') == 1 and n.endswith('.dbx')
               and not n.endswith('_description.dbx'))
    first = {n: v[0] for n, v in ix.items() if v[0][0].name.lower().startswith('level-')}
    per_level: dict[str, int] = {}
    vals: dict[str, list[float | None]] = collections.defaultdict(list)
    words: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    domains: collections.Counter[str] = collections.Counter()
    for n, data in read(first, chunk=1):
        try:
            f = dbx.parse(data)
        except ValueError:
            continue
        sp = [i for i in f.instances if short(i) == 'VehicleSpawnEntityData']
        kind = 'sp' if n.split('/')[1].startswith('sp_') else 'mp'
        per_level[f'{kind}:{n}'] = len(sp)
        for i in sp:
            for fld in ('SpawnDelay', 'InitializedSpawnDelay', 'MaxVehicles', 'MaxVehiclesPerMap', 'TimeUntilAbandoned',
                        'TimeUntilAbandonedIsDestroyed', 'RespawnRange', 'SpawnAreaRadius', 'KeepAliveRadius',
                        'BotBailWhenHealthBelow', 'WreckDuration', 'LodDistance'):
                vals[f'{kind} {fld}'].append(num(i, fld))
            for fld in ('AutoSpawn', 'Immortal', 'SpawnVehicleFromBeginning', 'ApplyDamageToAbandondedVehicles',
                        'EnterRestriction', 'Team'):
                words[f'{kind} {fld}'][str(word(i, fld))] += 1
            refs = items(i, 'Vehicles') or ([r] if (r := ref(i, 'Vehicle')) else [])
            vlist = kid(i, 'Vehicles')
            if vlist is not None and vlist.children:
                refs = [r for c in vlist.children if (r := ref(c, 'Vehicle') or c.attr('ref'))]
            words[f'{kind} Vehicles items'][str(len(vlist.children or []) if vlist is not None else 0)] += 1
            for r in refs:
                domains[f'{kind} {folder(r, 3)}'] += 1
        del f
    mps = [k for k in per_level if k.startswith('mp:')]
    sps = [k for k in per_level if k.startswith('sp:')]
    print(f'  level partitions: {len(mps)} multiplayer, {len(sps)} campaign; spawners per level: mp '
          f'{rng(per_level[k] for k in mps)}; sp {rng(per_level[k] for k in sps)}; total mp '
          f'{sum(per_level[k] for k in mps)}, sp {sum(per_level[k] for k in sps)}')
    for k in sorted(vals):
        print(f'  {k}: {rng(vals[k])}')
    for k in sorted(words):
        print(f'  {k}: {", ".join(f"{a} x{n}" for a, n in words[k].most_common())}')
    print(f'  spawned vehicles by folder: {", ".join(f"{a} x{n}" for a, n in domains.most_common())}')


def report_beach(P: Parts, vs: list[Vehicle], ix: dict[str, list[tuple[Path, str]]]) -> None:
    """The vehicles in BF1943's folder, one line each (folder names are DICE's structural names)."""
    print('\n== vehicles in BF1943\'s folder (objects/vehicles/beach/), one line each ==')
    for v in sorted((v for v in vs if v.group == 'beach'), key=lambda v: v.name):
        kinds = '+'.join(sorted({'sp' if b.parent.name.lower().startswith('sp_') else 'mp' for b, _ in ix[v.name]}))
        eng = [P.resolve(e[0], ref(e[1], 'Config')) for e in v.of('EngineComponentData')]
        seats = [f'{folder(ref(e[1], "InputMapping"), 3).rsplit("/", 1)[-1]}'
                 for e in sorted(v.of('PlayerEntryComponentData'), key=lambda e: num(e[1], 'EntryOrderNumber') or 0)]
        print(f'  {v.name[len("objects/vehicles/beach/"):-4]} ({kinds}): {v.kind}/{v.build}; {word(v.inst, "Material")}; '
              f'zones {num(v.inst, "FrontHealthZone.MaxHealth")}; engines {tally(short(e[1]) for e in eng if e)}; '
              f'seats {seats}; weapons {len(v.of("WeaponComponentData"))}')
        guns: collections.Counter[str] = collections.Counter()
        for w in v.of('WeaponComponentData'):
            fd = P.resolve(w[0], ref(w[1], 'WeaponFiring'))
            fn = P.resolve(fd[0], ref(fd[1], 'PrimaryFire')) if fd else None
            pj = P.resolve(fn[0], ref(fn[1], 'Shot.ProjectileData')) if fn else None
            sp = kid(fn[1], 'Shot.InitialSpeed') if fn else None
            z = struct.unpack('>f', dbx.numbers(sp)[1][2])[0] if sp is not None and sp.kind == dbx.NUMBERS else None
            guns[f'{short(pj[1]) if pj else "-"} speed {z} gravity {num(pj[1], "Gravity") if pj else None} damage '
                 f'{num(pj[1], "StartDamage") if pj else None} rate {num(fn[1], "FireLogic.RateOfFire") if fn else None} '
                 f'auto-aim {word(fd[1], "UseAutoAiming") if fd else None}'] += 1
        for g, k in guns.items():
            print(f'    {g} x{k}')


def report_console(found: list[Path], P: Parts) -> None:
    print('\n== console-only vehicles: entries named for the Mark V (every entry, any folder) ==')
    hits = index(found, lambda n: 'markv' in n)
    by: collections.Counter[str] = collections.Counter('/'.join(n.split('/')[:3]) for n in hits)
    print(f'  entry names: {len(hits)} ({", ".join(f"{a} x{n}" for a, n in sorted(by.items())) or "none"})')
    lv: dict[str, set[Path]] = collections.defaultdict(set)
    for n, copies in hits.items():
        if n.startswith('objects/'):
            lv['/'.join(n.split('/')[:3])] |= {b.parent for b, _ in copies}
    print(f'  level folders holding them, per object folder: '
          f'{", ".join(f"{a} {len(s)}" for a, s in sorted(lv.items()))}')
    print(f'  under objects/vehicles/: {sum(1 for n in hits if n.startswith("objects/vehicles/"))}; holding a '
          f'VehicleEntityData: {sum(1 for n in hits if n in P.files and any(short(i) == "VehicleEntityData" for i in P.files[n].instances))}')


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or '').split('\n\n')[0])
    ap.add_argument('--opfor', help='Project OPFOR BFBC2\'s folder (default: found as the suite finds it)')
    ap.add_argument('--all', action='store_true', help='OPFOR\'s bundles too (default: DICE\'s only)')
    ap.add_argument('--no-levels', action='store_true', help='skip the level partitions (spawners)')
    a = ap.parse_args(argv)
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    found = bundles(game, a.all)
    ix = index(found, lambda n: n.startswith(PREFIXES) and n.endswith('.dbx'))
    print(f'== corpus: {len(found)} bundles ({"all" if a.all else "DICE" + chr(39) + "s"}), {len(ix)} vehicle and '
          f'stationary-weapon partitions, {sum(len(v) for v in ix.values())} copies ==')
    P = Parts()
    for n, data in read({n: v[0] for n, v in ix.items()}):
        P.add(n, data)
    vs = [Vehicle(P, n, i) for n, f in P.files.items() for i in f.instances if short(i) == 'VehicleEntityData']
    print(f'  VehicleEntityData: {len(vs)} ({tally(v.group for v in vs)}); HUD types: {tally(v.kind for v in vs)}')
    print(f'  HUD type / build from the components: {tally(f"{v.kind}/{v.build}" for v in vs)}')
    print(f'  VehicleEntityData in the campaign only (copies in sp_* bundles alone): '
          f'{sum(1 for v in vs if all(b.parent.name.lower().startswith("sp_") for b, _ in ix[v.name]))}')
    print('\n== per kind ==')
    report_kinds(P, vs)
    report_seats(vs)
    report_beach(P, vs, ix)
    report_console(found, P)
    if not a.no_levels:
        report_spawners(found)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
