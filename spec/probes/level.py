"""DICE's level partitions as the data model describes them (read-only; spec/model/level.md: LEVEL-PART, LEVEL-DATA,
LEVEL-OBJ, LEVEL-EVT, LEVEL-SLOT, LEVEL-MG, LEVEL-ENV; spec/formats/culltree.md: CULL-LVL-1, CULL-SPH-2, CULL-SPH-3,
the level's two cull trees from its own bundle; spec/engine/vegetation.md VEG-CLS-1 and spec/engine/decals.md
DECAL-SRC-2: the vegetation and terrain-spline classes).

    python spec/probes/level.py [--opfor <folder>]

Reads each DICE level's partition `Levels/<LV>.dbx` once: from `game/Package/levels/<lv>/level-00.fbrb` when that copy
is DICE's (dated before 2013), else from `game/Dist/win32/levels/<lv>/level-00.fbrb`. Prints structure and counts only
(class and field names, numbers). `checks(game)` recomputes the stable counts the rules cite, for
core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import math
import re
import struct
import sys
import time
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths  # noqa: E402

Counter = collections.Counter
GUID = re.compile(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
# LEVEL-DATA-1: LevelData's members and their kinds (0xA container, 2 text, 6 scalar, 7 numbers)
DATA_MEMBERS = {
    'Name': 2, 'Objects': None, 'EventConnections': None, 'LevelDescription': 0xA, 'MaterialGrid': 2,
    'ExclusiveStaticCullTree': 2, 'ClientStaticCullTree': 2, 'ClientTreeStaticCullTree': 2, 'SpawnPointManager': 2,
    'SubtitleAsset': 2, 'WaypointAnimations': None, 'AmbientSounds': None, 'SoundStates': None, 'HasAI': 6,
    'HasFlyingAI': 6, 'StartLevelFadedToBlack': 6, 'DefaultFOV': 7, 'InfantryFOVMultiplier': 7,
    'FriendlyDamageModifier': 7, 'MinimapSettings': 0xA, 'EmitterSystemMetrics': 0xA, 'VoiceOvers': None,
    'VoiceOverLines': None,
}
WORLD = ('TerrainEntityData', 'StaticModelGroupEntityData', 'MiddlegrowthModelGroupEntityData',
         'RigidModelGroupEntityData', 'ForestEntityData', 'StaticEnvmapEntityData')
GROUPS = ('StaticModelGroupEntityData', 'RigidModelGroupEntityData', 'MiddlegrowthModelGroupEntityData',
          'ForestEntityData')
VEG_SPLINE = ('ForestEntityData', 'TreeMemberData', 'TreeMeshEntityData', 'MiddlegrowthModelGroupEntityData',
              'MiddlegrowthModelGroupMemberData', 'TerrainSplineData', 'TerrainSplinePointData', 'TerrainSplinePlaneData',
              'TerrainSplinePlanePointData')
MESH_ENTITIES = ('RigidMeshEntityData', 'CompositeMeshEntityData', 'TreeMeshEntityData')


def dice(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def level_bundles(game: Path) -> dict[str, Path]:
    """Level name (lower case) -> the DICE level-00 bundle that holds its partition, Package's copy first."""
    out: dict[str, Path] = {}
    for root in (game / 'Package' / 'levels', game / 'Dist' / 'win32' / 'levels'):
        for p in sorted(root.glob('*/level-00.fbrb'), key=lambda q: str(q).lower()):
            lv = p.parent.name.lower()
            if lv in out or not dice(p):
                continue
            if f'levels/{lv}.dbx' in fbrb.names(p):
                out[lv] = p
    return out


def kind(i: dbx.Element) -> str:
    return (i.attr('type') or '').split('.')[-1]


def ref(e: dbx.Element | None) -> str:
    return 'null' if e is None else (e.attr('ref') or 'null')


def form(r: str) -> str:
    if r == 'null':
        return 'null'
    return 'local' if GUID.match(r) else 'external'


def items(e: dbx.Element | None) -> list[dbx.Element]:
    return list(e.children or ()) if e is not None and e.kind == dbx.CONTAINER else []


def whole(e: dbx.Element | None) -> int | None:
    if e is None or e.kind != dbx.NUMBERS:
        return None
    size, vals = dbx.numbers(e)
    return int.from_bytes(vals[0], 'big', signed=True) if size == 4 and len(vals) == 1 else None


def nums(e: dbx.Element | None, fmt: str) -> float | None:
    """A one-value number element read with struct format `fmt` (big-endian, DBX's order)."""
    if e is None or e.kind != dbx.NUMBERS:
        return None
    _size, vals = dbx.numbers(e)
    return float(struct.unpack(fmt, vals[0])[0]) if len(vals) == 1 else None


def cull_trees(path: Path, ld: dbx.Element, c: Counter[str], spheres: dict[str, list[float]]) -> None:
    """CULL-LVL-1, CULL-SPH-2, CULL-SPH-3: the level's two trees from its own bundle."""
    c['cull: ClientTreeStaticCullTree null'] += ref(ld.child('ClientTreeStaticCullTree')) == 'null'
    names = {e.name.lower(): e.name for e in fbrb.entries(path) if not e.deleted}
    for field, leaf in (('ExclusiveStaticCullTree', 'staticculltree_exclusive'),
                        ('ClientStaticCullTree', 'staticculltree_client')):
        r = ref(ld.child(field)).lower()
        part = r.rsplit('/', 1)[0] if form(r) == 'external' else ''
        if not part.endswith('/' + leaf) or part + '.dbx' not in names:
            c[f'cull: {field} not a {leaf} partition of the bundle'] += 1
            continue
        c[f'cull: {field} names its partition'] += 1
        n = names[part + '.dbx']
        f = dbx.parse(fbrb.read(path, [n])[n])
        trees = [i for i in f.instances if i.attr('type') == 'World.StaticCullTreeData']
        c['cull: partitions with one StaticCullTreeData'] += len(trees) == 1 and len(f.instances) == 1
        nodes: list[tuple[int, float, tuple[float, float, float, float]]] = []
        for it in items(trees[0].child('Nodes')):
            right = nums(it.child('Right'), '>i')
            mx = nums(it.child('Max'), '>f')
            keys = tuple(nums(it.child(f'SphereKey{a}'), '>h') for a in 'XYZW')
            if right is None or mx is None or any(k is None for k in keys):
                continue
            kx, ky, kz, kw = (float(k or 0) for k in keys)
            nodes.append((int(right), mx, (kx / 16, ky / 16, kz / 16, kw / 4)))
        while nodes and nodes[-1][0] == 0 and nodes[-1][1] == 0 and nodes[-1][2] == (0, 0, 0, 0):
            nodes.pop()
        for i, (right, mx, (x, y, z, w)) in enumerate(nodes):
            if right < 0:
                c['cull: leaves'] += 1
                c['cull: leaf W >= 4 Max + 0.5'] += w * 4 >= 4 * mx + 0.5
                if mx > 1.5:
                    c[f'cull: leaf above 1.5 m, W - ceil(4 Max) = {int(w * 4) - math.ceil(4 * mx)}'] += 1
                continue
            for child in (i + 1, i + right):
                if child >= len(nodes):
                    continue
                cx, cy, cz, cw = nodes[child][2]
                out = math.dist((x, y, z), (cx, cy, cz)) + cw - w
                c['cull: child spheres'] += 1
                if out > 1e-6:
                    c['cull: child spheres sticking out'] += 1
                    spheres['out'].append(out)


def analyse(game: Path) -> dict[str, object]:
    """The counts, by key."""
    c: Counter[str] = Counter()
    spheres: dict[str, list[float]] = {'out': []}
    objects_per: list[int] = []
    unlisted: Counter[str] = Counter()
    slots_dup = 0
    levels = level_bundles(game)
    for lv, path in sorted(levels.items()):
        name = next(e.name for e in fbrb.entries(path) if e.name.lower() == f'levels/{lv}.dbx')
        f = dbx.parse(fbrb.read(path, [name])[name])
        insts = f.instances
        by = {(i.attr('guid') or '').lower(): i for i in insts}
        c['partitions'] += 1
        # LEVEL-PART-1
        prim = (f.partition.attr('primaryInstance') or '').lower()
        datas = [i for i in insts if kind(i) == 'LevelData']
        c['part1: one LevelData, primary, first, id = level'] += (
            len(datas) == 1 and (insts[0].attr('guid') or '').lower() == prim and kind(insts[0]) == 'LevelData'
            and insts[0].attr('id') == name.rsplit('/', 1)[-1][:-4])
        # LEVEL-PART-2
        c['part2: SubWorld / Layer types'] += sum(1 for i in insts if 'SubWorld' in kind(i) or 'Layer' in kind(i))
        if not datas:
            continue
        ld = datas[0]
        cull_trees(path, ld, c, spheres)
        # LEVEL-DATA-1
        ok = True
        for m, k in DATA_MEMBERS.items():
            e = ld.child(m)
            ok &= e is not None and (k is None or e.kind == k or (k == 2 and e.attr('ref') is not None))
        c['data1: every member present'] += ok
        # LEVEL-OBJ-1
        objs = items(ld.child('Objects'))
        objects_per.append(len(objs))
        listed = {ref(o).lower() for o in objs}
        c['obj1: items'] += len(objs)
        c['obj1: items not local'] += sum(1 for o in objs if form(ref(o)) != 'local')
        for i in insts:
            g = (i.attr('guid') or '').lower()
            if kind(i) in WORLD:
                c[f'obj1: world {"listed" if g in listed else "unlisted"}'] += 1
            elif g not in listed:
                unlisted[kind(i)] += 1
        # LEVEL-EVT-1, 2
        c['evt1: other instances with EventConnections'] += sum(
            1 for i in insts if i is not ld and i.child('EventConnections') is not None)
        for conn in items(ld.child('EventConnections')):
            c['evt1: connections'] += 1
            names = [m.attr('name') for m in conn.children or ()]
            c['evt1: members Source, Target, SourceEvent, TargetEvent'] += names == [
                'Source', 'Target', 'SourceEvent', 'TargetEvent']
            for side in ('SourceEvent', 'TargetEvent'):
                ev = conn.child(side)
                sub = [(m.attr('name'), m.kind) for m in (ev.children or ())] if ev is not None else []
                c['evt1: event complexes Name text, Id numbers'] += sub == [('Name', dbx.TEXT), ('Id', dbx.NUMBERS)]
            for side in ('Source', 'Target'):
                c[f'evt2: {side} {form(ref(conn.child(side)))}'] += 1
        # LEVEL-SLOT-2
        slots = Counter(whole(i.child('Enumeration')) for i in insts if kind(i) == 'ReferenceObjectData')
        slots_dup += sum(n - 1 for s, n in slots.items() if s and n > 1)
        # LEVEL-MG-1
        for grp in (i for i in insts if kind(i) in GROUPS):
            c['mg1: groups'] += 1
            pd = by.get(ref(grp.child('PhysicsData')).lower())
            c['mg1: PhysicsData local PhysicsEntityData'] += pd is not None and kind(pd) == 'PhysicsEntityData'
            c['mg1: groups with Transform, Enumeration, MemberDatas'] += all(
                grp.child(m) is not None for m in ('Transform', 'Enumeration', 'MemberDatas'))
            for it in items(grp.child('MemberDatas')):
                mem = by.get(ref(it).lower())
                c['mg1: members'] += 1
                if mem is None:
                    c['mg1: member not local'] += 1
                    continue
                c['mg1: member listed'] += (mem.attr('guid') or '').lower() in listed
                me = by.get(ref(mem.child('MeshEntityType')).lower())
                c['mg1: MeshEntityType local mesh entity'] += me is not None and kind(me) in MESH_ENTITIES
                mt = ref(mem.child('MemberType')).lower()
                c['mg1: MemberType external under objects/ vegetation/ terrains/'] += (
                    form(mt) == 'external' and mt.startswith(('objects/', 'vegetation/', 'terrains/')))
                c['mg1: member with the four counts'] += all(mem.child(x) is not None for x in (
                    'InstanceCount', 'FirstPhysicsPart', 'PhysicsPartCount', 'PhysicsPartCountPerInstance'))
        c['obj1: LevelData listed'] += (ld.attr('guid') or '').lower() in listed
        # VEG-CLS-1, DECAL-SRC-2: the vegetation and terrain spline classes
        kinds = Counter(kind(i) for i in insts)
        for cls in VEG_SPLINE:
            c[f'veg: {cls}'] += kinds[cls]
            c[f'veg: levels holding {cls}'] += kinds[cls] > 0
        for i in insts:
            cls = kind(i)
            if cls in ('TreeMemberData', 'MiddlegrowthModelGroupMemberData'):
                count = whole(i.child('InstanceCount'))
                c[f'veg: {cls} instances'] += count or 0
                c[f'veg: {cls} -> {ref(i.child("MemberType")).split("/")[0].lower()}'] += 1
            if cls == 'TerrainSplinePlaneData':
                c['veg: spline planes with the water fields'] += all(i.child(f) is not None for f in (
                    'BaseSpeed', 'DefaultFoamLeft', 'DefaultFoamRight', 'DefaultWaveHeightLeft', 'DefaultWaveHeightRight'))
            if cls == 'TerrainSplinePlanePointData':
                c['veg: spline plane points with WaterSpeed'] += i.child('WaterSpeed') is not None
        # LEVEL-ENV-1
        envs = [i for i in insts if kind(i) == 'StaticEnvmapEntityData']
        c[f'env1: levels with {len(envs)}'] += 1
        for e in envs:
            c['env1: envmaps'] += 1
            r = ref(e.child('StaticEnvmap')).lower()
            listed_env = (e.attr('guid') or '').lower() in listed
            c['env1: listed'] += listed_env
            c['env1: StaticEnvmap external under terrains/'] += form(r) == 'external' and r.startswith('terrains/')
            c['env1: StaticEnvmap null'] += r == 'null'
    return {'counts': c, 'objects_per': objects_per, 'unlisted': unlisted, 'slots_dup': slots_dup,
            'spheres_out': spheres['out']}


# The stable counts of 2026-11-22 (74 DICE levels).
EXPECTED: dict[str, object] = {
    'partitions': 74,
    'part1: one LevelData, primary, first, id = level': 74,
    'part2: SubWorld / Layer types': 0,
    'data1: every member present': 74,
    'obj1: items': 230818,
    'obj1: items not local': 0,
    'obj1: world listed': 445,
    'obj1: world unlisted': None,
    'obj1: LevelData listed': 0,
    'obj1: Objects per level min': 1690,
    'obj1: Objects per level max': 8419,
    'evt1: other instances with EventConnections': 0,
    'evt1: connections': 57667,
    'evt1: members Source, Target, SourceEvent, TargetEvent': 57667,
    'evt1: event complexes Name text, Id numbers': 115334,
    'evt2: Source local': 57661,
    'evt2: Source external': 6,
    'evt2: Target local': 55934,
    'evt2: Target external': 1733,
    'evt2: Source null': None,
    'evt2: Target null': None,
    'slot2: duplicate nonzero slots': 0,
    'mg1: groups': 299,
    'mg1: groups with Transform, Enumeration, MemberDatas': 299,
    'mg1: PhysicsData local PhysicsEntityData': 299,
    'mg1: members': 3352,
    'mg1: member not local': None,
    'mg1: member listed': 0,
    'mg1: MeshEntityType local mesh entity': 3352,
    'mg1: MemberType external under objects/ vegetation/ terrains/': 3352,
    'mg1: member with the four counts': 3352,
    'env1: levels with 0': 6,
    'env1: levels with 1': 66,
    'env1: levels with 3': 2,
    'env1: envmaps': 72,
    'env1: listed': 72,
    'env1: StaticEnvmap external under terrains/': 68,
    'env1: StaticEnvmap null': 4,
    'cull: ClientTreeStaticCullTree null': 74,
    'cull: ExclusiveStaticCullTree names its partition': 74,
    'cull: ClientStaticCullTree names its partition': 74,
    'cull: partitions with one StaticCullTreeData': 148,
    'cull: leaves': 516718,
    'cull: leaf W >= 4 Max + 0.5': 516718,
    'cull: leaf above 1.5 m, W - ceil(4 Max) = 0': 84356,
    'cull: leaf above 1.5 m, W - ceil(4 Max) = 1': 314876,
    'cull: child spheres': 1033140,
    'cull: child spheres sticking out': 395342,
    'cull: largest stick-out (cm)': 110,
    'veg: ForestEntityData': 74, 'veg: levels holding ForestEntityData': 74,
    'veg: TreeMemberData': 396, 'veg: TreeMemberData instances': 97199, 'veg: TreeMeshEntityData': 737,
    'veg: MiddlegrowthModelGroupEntityData': 93, 'veg: levels holding MiddlegrowthModelGroupEntityData': 71,
    'veg: MiddlegrowthModelGroupMemberData': 341, 'veg: MiddlegrowthModelGroupMemberData instances': 82916,
    'veg: TreeMemberData -> vegetation': 293, 'veg: TreeMemberData -> objects': 89, 'veg: TreeMemberData -> terrains': 14,
    'veg: MiddlegrowthModelGroupMemberData -> vegetation': 284, 'veg: MiddlegrowthModelGroupMemberData -> objects': 41,
    'veg: MiddlegrowthModelGroupMemberData -> terrains': 16,
    'veg: TerrainSplineData': 1360, 'veg: TerrainSplinePointData': 14449, 'veg: TerrainSplinePlaneData': 1372,
    'veg: TerrainSplinePlanePointData': 44132, 'veg: spline planes with the water fields': 1372,
    'veg: spline plane points with WaterSpeed': 44132,
}


def summary(a: dict[str, object]) -> dict[str, object]:
    c = a['counts']
    assert isinstance(c, Counter)
    per = a['objects_per']
    assert isinstance(per, list)
    out: dict[str, object] = dict(c)
    out['obj1: Objects per level min'] = min(per)
    out['obj1: Objects per level max'] = max(per)
    out['slot2: duplicate nonzero slots'] = a['slots_dup']
    so = a['spheres_out']
    assert isinstance(so, list)
    out['cull: largest stick-out (cm)'] = round(max(so, default=0.0) * 100)
    return out


def checks(game: Path) -> list[tuple[bool, str]]:
    got = summary(analyse(game))
    return [(got.get(k) == want, f'level probe: {k} = {want!r} (got {got.get(k)!r})') for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    got = analyse(game)
    for k, v in sorted(summary(got).items()):
        print(f'{k}: {v}')
    unlisted = got['unlisted']
    assert isinstance(unlisted, Counter)
    print('unlisted types:', ', '.join(f'{k} {v}' for k, v in unlisted.most_common(12)))


if __name__ == '__main__':
    main()
