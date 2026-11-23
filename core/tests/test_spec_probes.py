r"""The specification's probe counts as checks (rounds SPG2, SPG2-rest): the stable facts the probes of `spec/probes/`
measure on DICE's bundles of Project OPFOR BFBC2's game copy, asserted, so that the rules they back are verified
instead of observed. Read only; skipped without the game copy. `terrain.py` and `effects_chain.py` are read through
their functions; SPG's five print-only probes (`materials`, `destruction`, `lighting`, `vehicles`, `weapons`) by the
lines of their output that hold a rule's counts (EXPECT); `grime.py`, `menus.py` and `modes.py` by their own
`checks()`.

  python core\tests\test_spec_probes.py [--opfor <folder>]

Each check function names the rules it proves (`# spec:` comments). The corpus is DICE's: `game/**/levels/*/*.fbrb`
dated before 2013, each resource or partition once.
"""
from __future__ import annotations

import argparse
import collections
import importlib.util
import os
import re
import struct
import sys
from collections.abc import Callable
from pathlib import Path
from types import ModuleType
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths  # noqa: E402

FAILED: list[str] = []


def probe(name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(f'spec_probe_{name}', SUITE / 'spec' / 'probes' / f'{name}.py')
    assert spec is not None and spec.loader is not None
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def text(e: dbx.Element | None) -> str:
    return '' if e is None else dbx.value(e)


def check(ok: bool, what: str) -> None:
    print(('ok    ' if ok else 'FAIL  ') + what)
    if not ok:
        FAILED.append(what)


class Terrains:
    """Every DICE height field, visual terrain, mask atlas and terrain partition, read once."""

    def __init__(self, bundles: list[Path], tp: ModuleType) -> None:
        self.main: dict[str, tuple[int, tuple[float, ...]]] = {}
        self.visual: dict[str, dict[str, Any]] = {}
        self.atlas: dict[str, tuple[int, int, int, int]] = {}
        self.partitions: list[dbx.DbxFile] = []
        self.partition_names: set[str] = set()
        # main blocks' tails (TERR-HF-4, 9 - 11): terrain -> (re-encoded, runs maximal, share under the finest grid)
        self.tails: dict[str, tuple[bool, bool, float]] = {}
        # material maps (TERR-MM-1 - 4): terrain -> (re-encoded, width, rows, names)
        self.material_maps: dict[str, tuple[bool, int, int, list[str]]] = {}
        # level grids' material names, and the terrain of each bundle that holds exactly one terrain partition
        self.grid_names: dict[str, list[str]] = {}
        self.grid_terrain: dict[str, str] = {}
        seen: set[str] = set()
        for path in bundles:
            es = [e for e in fbrb.entries(path) if not e.deleted]
            self.partition_names |= {e.name.lower()[:-4] for e in es if e.name.lower().endswith('.dbx')}
            terrains = {m.group(1) for e in es for m in [re.fullmatch(r'terrains/([^/]+)/\1\.dbx', e.name.lower())] if m}
            grids = [e.name for e in es if re.fullmatch(r'levels/[^/]+/materials_win32\.dbx', e.name.lower())]
            if len(terrains) == 1:
                for g in grids:
                    self.grid_terrain.setdefault(g.split('/')[1].lower(), next(iter(terrains)))
            es = [e for e in es if e.name.lower() not in seen]
            want = [e.name for e in es if e.type in ('TerrainHeightfield', 'VisualTerrain', 'TerrainMaterialMap')
                    or (e.type == 'ITexture' and 'masktile' in e.name.lower())
                    or re.fullmatch(r'terrains/([^/]+)/\1\.dbx', e.name.lower())
                    or e.name in grids]
            if not want:
                continue
            types = {e.name: e.type for e in es}
            got = fbrb.read(path, want)
            for n in want:
                seen.add(n.lower())
                d = got[n]
                if n in grids:
                    self.grid_names[n.split('/')[1].lower()] = [
                        text(i.child('Name')).lower() for i in dbx.parse(d).instances
                        if (i.attr('type') or '').endswith('.MaterialData')]
                elif n.lower().endswith('.dbx'):
                    self.partitions.append(dbx.parse(d))
                elif types[n] == 'ITexture':
                    v = struct.unpack_from('<8I', d)
                    self.atlas[n.lower()[:-4] if n.lower().endswith('.res') else n.lower()] = (v[4], v[5], v[2], v[7])
                elif types[n] == 'TerrainHeightfield':
                    if struct.unpack_from('<I', d, 0x20)[0]:
                        self.main[n] = (struct.unpack_from('<I', d, 0x18)[0], struct.unpack_from('<6f', d))
                        m = tp.main_tail(d)
                        self.tails[n.split('/')[1].lower()] = (m['re_encoded'], m['maximal'],
                                                               tp.below(m['grids'][-1], m['heights']))
                elif types[n] == 'TerrainMaterialMap':
                    mm = tp.material_map(d)
                    self.material_maps[n.split('/')[1].lower()] = (mm['re_encoded'], mm['width'],
                                                                   int(mm['cells'].shape[0]), mm['names'])
                else:
                    self.visual[n] = tp.visual_terrain(d)


# spec: TEDIT-HF-1, TEDIT-HF-2
def height_fields(t: Terrains) -> None:
    sizes = collections.Counter(n for n, _ in t.main.values())
    check(sizes == collections.Counter({1024: 11, 2048: 19}), f'TEDIT-HF-1: 30 main blocks of 1,024 / 2,048 ({dict(sizes)})')
    check(all(b[1] == 0 and (b[3] - b[0]) / n == 1 for n, b in t.main.values()), 'TEDIT-HF-1: min y 0, 1 m spacing')
    ranges = collections.Counter(round(b[4] - b[1]) for _, b in t.main.values())
    check(ranges == collections.Counter({500: 16, 1000: 8, 750: 2, 1700: 1, 1500: 1, 650: 1, 395: 1}),
          f'TEDIT-HF-2: the ranges ({dict(ranges)})')


# spec: TEDIT-VT-1, TEDIT-ATL-1, TEDIT-ATL-2, TEDIT-BLK-1, VEG-UG-1, VEG-UG-2
def visual_terrains(t: Terrains) -> None:
    check(len(t.visual) == 30, f'30 DICE visual terrains ({len(t.visual)})')
    blocks = collections.Counter(int(v['n']) for v in t.visual.values())
    check(blocks == collections.Counter({1: 14, 13: 12, 25: 1, 37: 2, 49: 1}), f'TEDIT-BLK-1: blocks ({dict(blocks)})')
    pair: collections.Counter[tuple[bool, bool]] = collections.Counter()
    second_set: set[str] = set()
    tails: collections.Counter[bytes] = collections.Counter()
    cells_ok = cells_leftover = cells_bad = 0
    aligned_bad_terrains: dict[str, int] = {}
    atlas_ok = atlas_bad = 0
    flags_seen: set[int] = set()
    for name, v in t.visual.items():
        shaders = v['shaders']
        for _n, ug, _l, _t, meshes in shaders:
            pair[(bool(ug), bool(meshes))] += 1
            for _mesh, inst, _s, _vals, tail in meshes:
                tails[tail] += 1
                if inst:
                    second_set.add(name.split('/')[1])
        atlases = set()
        shared = 0
        for header, tiles, fl, s, smap, _hf, atlas in v['maps']:
            atlases.add(atlas.lower())
            side = header[4]
            ratio = max(1, s // max(1, side))
            owner: dict[int, set[tuple[int, int, int]]] = collections.defaultdict(set)
            for c in range(side * side):
                px, pz = c % side, c // side
                tiled = shaders[smap[pz * ratio * s + px * ratio]][3]
                nz = sum(1 for x in fl[4 * c:4 * c + 4] if x)
                if nz == tiled:
                    cells_ok += 1
                elif tiled == 0 and nz == 1:
                    cells_leftover += 1
                else:
                    cells_bad += 1
                for k in range(4):
                    f = fl[4 * c + k]
                    if f:
                        flags_seen.add(f)
                        owner[tiles[4 * c + k]].add((px // f, pz // f, f))
            shared += sum(1 for b in owner.values() if len(b) > 1)
            a = t.atlas.get(atlas.lower())
            if a is not None and (header[2] + 1) * 64 == a[0] and (header[3] + 1) * 64 == a[1] and a[2] == 10:
                atlas_ok += 1
            elif atlas:
                atlas_bad += 1
        check(len(atlases) == 1, f'TEDIT-ATL-1: {name}: one atlas')
        if shared:
            aligned_bad_terrains[name.split('/')[1]] = shared
    check(pair == collections.Counter({(True, True): 1068, (False, False): 447}),
          f'VEG-UG-1: undergrowth mask shader exactly when meshes ({dict(pair)})')
    check(set(tails) == {b'\x01\x00'} and sum(tails.values()) == 3069, 'VEG-UG-2: 3,069 entries ending 01 00')
    check(second_set == {'NAM_MP_003'}, f'VEG-UG-2: second string only on NAM_MP_003 ({sorted(second_set)})')
    check(cells_bad == 0 and cells_ok == 358282 and cells_leftover == 9590,
          f'TEDIT-VT-1: flags = tiled count, else one leftover on a rule-only cell ({cells_ok}, {cells_leftover}, {cells_bad})')
    check(atlas_bad == 0 and atlas_ok >= 30, f'TEDIT-ATL-1: every map header = atlas size / 64 - 1, L8 ({atlas_ok}, {atlas_bad})')
    check(flags_seen == {2, 4, 8, 16, 32, 64}, f'TEDIT-ATL-2: flags {sorted(flags_seen)}')
    check(len(aligned_bad_terrains) == 8 and all(n == 1 for n in aligned_bad_terrains.values()),
          f'TEDIT-ATL-2: one shared tile on 8 terrains ({aligned_bad_terrains})')


# spec: TEDIT-SRC-1, DECAL-SRC-1, DSTR-TERR-4
def terrain_partitions(t: Terrains) -> None:
    classes: collections.Counter[str] = collections.Counter()
    resolved = unresolved = 0
    mask_tile: collections.Counter[str] = collections.Counter()
    for f in t.partitions:
        for i in f.instances:
            k = (i.attr('type') or '').split('.')[-1]
            classes[k] += 1
            if k == 'TerrainData':
                c = i.child('MaskTileSize')
                mask_tile[dbx.value(c) if c is not None else 'none'] += 1
            if 'Decal' in k and k != 'TerrainGroupDecal':
                for field in ('Material', 'Spline'):
                    c = i.child(field)
                    r = (c.attr('ref') or 'null') if c is not None else 'null'
                    if r != 'null':
                        if r.rsplit('/', 1)[0].lower() in t.partition_names:
                            resolved += 1
                        else:
                            unresolved += 1
    want = {'TerrainData': 30, 'TerrainFullDecal': 314, 'TerrainGroupDecal': 80, 'TerrainSplineDecal': 416,
            'TerrainQuadDecal': 40}
    check(all(classes[k] == n for k, n in want.items()), f'DECAL-SRC-1: decal classes ({dict(classes)})')
    check(resolved == 0 and unresolved == 1186, f'TEDIT-SRC-1: decal references in no bundle ({unresolved}, {resolved})')
    check(mask_tile == collections.Counter({'64': 30}), f'DSTR-TERR-4: MaskTileSize 64 on 30 ({dict(mask_tile)})')


# spec: FX-PROC-1, FX-PROC-2
def emitters(bundles: list[Path], fx: ModuleType) -> None:
    seen: set[str] = set()
    docs = spawn_root = order_ok = order_bad = type_ok = type_bad = 0
    for path in bundles:
        names = [e.name for e in fbrb.entries(path) if not e.deleted and e.name.lower().endswith('.dbx')
                 and re.search(r'(^|/)fx/', e.name.lower()) and e.name.lower() not in seen]
        if not names:
            continue
        got = fbrb.read(path, names)
        for n in names:
            seen.add(n.lower())
            f = dbx.parse(got[n])
            doc = next((i for i in f.instances if fx.kind(i) == 'EmitterDocument'), None)
            if doc is None:
                continue
            docs += 1
            by = {(i.attr('guid') or '').lower(): i for i in f.instances}
            tmpl = by.get(fx.ref(doc.child('TemplateData')).lower())
            tc = fx.chain(fx.ref(tmpl.child('RootProcessor')), by) if tmpl is not None else []
            spawn_root += bool(tc) and fx.kind(tc[0]) == 'SpawnRateData'
            ks = [fx.kind(x) for x in tc]
            sp = [j for j, k in enumerate(ks) if k.startswith('Spawn')]
            up = [j for j, k in enumerate(ks) if k.startswith('Update')]
            if sp and up:
                if max(sp) < min(up):
                    order_ok += 1
                else:
                    order_bad += 1
            for p in tc:
                if 'Pt' + fx.kind(p)[:-4] == fx.text(p.child('ProcessorType')):
                    type_ok += 1
                else:
                    type_bad += 1
    check(docs == 50141 and spawn_root == docs, f'FX-PROC-1: every template chain starts at SpawnRateData ({spawn_root} of {docs})')
    check(order_bad == 0 and order_ok > 0, f'FX-PROC-1: spawn before update ({order_ok}, {order_bad})')
    check(type_bad == 0 and type_ok > 0, f'FX-PROC-2: ProcessorType = Pt + class ({type_ok}, {type_bad})')


# spec: TERR-HF-4, TERR-HF-9, TERR-HF-10, TERR-HF-11
def height_field_tails(t: Terrains) -> None:
    check(len(t.tails) == 30, f'30 main blocks read ({len(t.tails)})')
    check(all(r for r, _, _ in t.tails.values()),
          f'TERR-HF-4 / 9 / 10: offsets, depth runs, padding and min / max tree re-encode byte for byte '
          f'({sum(r for r, _, _ in t.tails.values())} of {len(t.tails)})')
    check(all(m for _, m, _ in t.tails.values()), 'TERR-HF-9: every depth run is maximal (no two neighbours equal)')
    shares = sorted(s for _, _, s in t.tails.values())
    strict = sum(1 for s in shares if s == 1.0)
    check(shares[0] >= 0.99 and strict == 15,
          f'TERR-HF-11: the finest grid lies under the heights on >= 99 % of samples on all, all of them on 15 '
          f'(lowest {shares[0]:.4f}, all of them on {strict})')


# spec: TERR-MM-1, TERR-MM-2, TERR-MM-3, TERR-MM-4, TEDIT-MM-1
def material_maps(t: Terrains) -> None:
    mm = t.material_maps
    check(len(mm) == 30 and all(r for r, _, _, _ in mm.values()),
          f'TERR-MM-2: 30 material maps re-encode byte for byte ({sum(r for r, _, _, _ in mm.values())} of {len(mm)})')
    widths = collections.Counter((w, rows) for _, w, rows, _ in mm.values())
    check(widths == collections.Counter({(4096, 4096): 22, (2048, 2048): 8}), f'TERR-MM-3: square maps ({dict(widths)})')
    sizes = {k.split('/')[-1].split('.')[0].lower(): n for k, (n, _) in t.main.items()}
    per_metre = collections.Counter(w // sizes[k] for k, (_, w, _, _) in mm.items() if k in sizes)
    check(per_metre == collections.Counter({2: 27, 4: 3}), f'TERR-MM-3: cells per metre ({dict(per_metre)})')
    check(all(names[0] == 'defaultmaterial' and len(names) <= 16 for _, _, _, names in mm.values()),
          'TERR-MM-4: index 0 is defaultmaterial, at most 16 names (4-bit cells)')
    paired = [(g, tr) for g, tr in t.grid_terrain.items() if g in t.grid_names and tr in mm]
    holds = sum(1 for g, tr in paired if all(n.lower() in t.grid_names[g] for n in mm[tr][3]))
    same_index = sum(1 for g, tr in paired
                     if [t.grid_names[g].index(n.lower()) for n in mm[tr][3] if n.lower() in t.grid_names[g]]
                     == list(range(len(mm[tr][3]))))
    check(len(paired) == 31 and holds == 31 and same_index == 0,
          f'TERR-MM-4: every paired level grid holds its terrain\'s names, never at the same indices '
          f'({holds} of {len(paired)}, same order {same_index})')


def probe_output(name: str, *args: str) -> str:
    """A print-only probe's output: its main() run in this process with these arguments."""
    import contextlib
    import io
    m = probe(name)
    out = io.StringIO()
    argv = sys.argv
    sys.argv = [name, *args]
    try:
        with contextlib.redirect_stdout(out):
            if m.main.__code__.co_argcount:     # vehicles.py and weapons.py take their arguments
                m.main(list(args))
            else:
                m.main()
    finally:
        sys.argv = argv
    return out.getvalue()


# The rules SPG's probes back and the lines of each probe's output that hold their counts (2026-11-22's runs).
EXPECT: dict[str, list[tuple[str, str]]] = {
    'materials': [
        ('MAT-GRID-1', '81 partitions (74 from DICE bundles); materials 51 - 92, relations 953 - 3160'),
        ('MAT-GRID-1', 'MaterialGridLayers items over all grids: 0'),
        ('MAT-GRID-2', 'names whose index in MaterialGridData.Materials differs between grids: 114 of 121'),
        ('MAT-GRID-3', 'Hash = 33-xor hash of the lower-case name: 5746 of 5746 MaterialData instances'),
        ('MAT-GRID-3', "{'Construction': 1771, 'Projectiles': 1135, 'Explosions': 692, 'Vehicles': 677, "
                       "'Terrain': 573, 'Vegetation': 395, 'Soldier': 355, 'Default': 148}"),
        ('MAT-GRID-4', 'names whose flags differ between grids: 0'),
        ('MAT-GRID-4', '121 names; per flag set: Penetrable 23, SeeThrough 9, ClientDestructible 9, '
                       'NoCollisionResponse 3, NoCollisionResponseCombined 5'),
        ('MAT-GRID-5', 'relations with a null side (a material against any): 3494'),
        ('MAT-GRID-5', 'data items per relation: {1: 66833, 2: 49068, 3: 34283, 4: 13800}'),
        ('MAT-GRID-6', 'relations by their MaterialRelationDamageData count: {0: 28783, 1: 107452, 2: 27749}; '
                       'of those with 2 or more, every item with SourceMaterial set: 27728'),
        ('MAT-SLOT-1', 'wrapper material hashes = 33-xor hashes of MaterialMappings, in order: 1183 of 1183'),
        ('DSTR-TERR-3', 'MaterialRelationTerrainDestructionData.Width: 1 .. 13 (3454 values, 8 distinct'),
        ('DSTR-TERR-3', 'MaterialRelationTerrainDestructionData.Depth: 0.1 .. 2 (3454 values, 9 distinct'),
    ],
    'destruction': [
        ('DSTR-STATE-1', '1262 blueprints'),
        ('DSTR-STATE-1', 'with PartComponentData parts: 599; parts per blueprint 1 .. 107'),
        ('DSTR-STATE-2', 'health states per part: {0: 43, 1: 304, 2: 2720, 3: 64, 4: 4}'),
        ('DSTR-STATE-2', "'lowest state at health 0': 2786"),
        ('DSTR-STATE-4', "'lowest state shows nothing': 2512, 'lowest state has Effects': 2366, "
                         "'healthiest state has Effects': 70"),
        ('DSTR-STATE-6', 'DestroyRuinPartPercentage: 35 .. 100 (505 values, 7 distinct; most: 75 x 495'),
        ('DSTR-MESH-2', "MeshName: {'empty (part gone)': 2521, 'the part name': 2574, 'other name': 642, "
                        "'part name + suffix': 215}"),
        ('DSTR-MESH-3', "{'banger': 44, 'wreck': 31, 'ruin': 2, 'destroyed': 0, 'debris': 9}"),
        ('DSTR-MG-3', 'health-state arrays: empty in every member'),
        ('DSTR-MG-3', 'equal to PhysicsPartCountPerInstance 1150, not 1433'),
        ('DSTR-MG-4', 'DamageAreaTriggerEntityData instances: 1283'),
        ('DSTR-TERR-1', 'terrains where every patch shader does: 6'),
    ],
    'lighting': [
        ('LIGHT-PT-5', 'PointLightEntityData: 68 in 12 levels'),
        ('LIGHT-ENV-1', 'StaticEnvmapEntityData: 79 in 73 levels'),
        ('FX-PLACE-1', 'placed GameEffectEntityData: 13083 in 82 levels; camera-proximity 3024 in 62 levels'),
        ('LIGHT-SKY-1', "moon placements: {'mp_008': 1, 'mp_008sdm': 1}"),
        ('LIGHT-SKY-2', 'god-ray placements: 40 in 7 levels'),
        ('FX-CLS-1', 'partitions: 4285'),
        ('FX-PLACE-3', 'EmitterTemplateData: 2882; acting as point lights: 35'),
        ('LIGHT-IRR-1', 'IrradianceVolume resources: 25; 36-byte header + nx*ny*nz bytes exactly: 25'),
        ('LIGHT-IRR-2', 'voxel bytes 51 .. 255; at the floor (51): 152495, at 255: 979692 of 1703118'),
        ('WTH-STATE-1', 'systems: 28; states listed: 457; per system 2 .. 53'),
    ],
    'vehicles': [
        ('VEH-ENT-1', 'VehicleEntityData: 97 (land x26, air x24, stationary weapons x17, vietnam x9, sea x8, '
                      'beach x8, static x5)'),
        ('VEH-ENT-2', 'HUD types: Car x22, Helicopter x18, Stationary x15, Tank x11, Dummy x10, Boat x9'),
        ('VEH-SPAWN-1', 'level partitions: 61 multiplayer, 13 campaign'),
        ('VEH-SPAWN-1', 'total mp 1044, sp 367'),
        ('VEH-SPAWN-2', 'mp RespawnRange: 10 x1, 15 x1026, 30 x17'),
    ],
    'weapons': [
        ('WPN-CLS-1', '359 GameSharedResources.SoldierWeaponData'),
        ('WPN-CLS-1', '574 GameSharedResources.ZoomLevelData'),
        ('WPN-PATCH-1', 'with a Dist copy too: 351, of which differ from Dist\'s (XML form): 325'),
        ('WPN-WW2-1', 'partitions in BF1943\'s folder (objects/weapons/handheld/beach/): 76'),
    ],
}


def expect_lines(name: str) -> None:
    out = probe_output(name)
    for rule, text in EXPECT[name]:
        check(text in out, f'{rule}: spec/probes/{name}.py prints {text[:70]!r}')


# spec: MAT-GRID-1, MAT-GRID-2, MAT-GRID-3, MAT-GRID-4, MAT-GRID-5, MAT-GRID-6, MAT-SLOT-1, DSTR-TERR-3
def spg_materials() -> None:
    expect_lines('materials')


# spec: DSTR-STATE-1, DSTR-STATE-2, DSTR-STATE-4, DSTR-STATE-6, DSTR-MESH-2, DSTR-MESH-3, DSTR-MG-3, DSTR-MG-4
# spec: DSTR-TERR-1
def spg_destruction() -> None:
    expect_lines('destruction')


# spec: LIGHT-PT-5, LIGHT-ENV-1, FX-PLACE-1, LIGHT-SKY-1, LIGHT-SKY-2, FX-CLS-1, FX-PLACE-3, LIGHT-IRR-1, LIGHT-IRR-2
# spec: WTH-STATE-1
def spg_lighting() -> None:
    expect_lines('lighting')


# spec: VEH-ENT-1, VEH-ENT-2, VEH-SPAWN-1, VEH-SPAWN-2
def spg_vehicles() -> None:
    expect_lines('vehicles')


# spec: WPN-CLS-1, WPN-PATCH-1, WPN-WW2-1
def spg_weapons() -> None:
    expect_lines('weapons')


def probe_checks(name: str, game: Path) -> None:
    for ok, what in probe(name).checks(game):
        check(ok, what)


# spec: MAT-TEX-4, MAT-TEX-5, MAT-TEX-6, MAT-TEX-7, MAT-TEX-8, MAT-TEX-9, MAT-TEX-10, MAT-TEX-11
def grime(game: Path) -> None:
    probe_checks('grime', game)


# spec: MENU-AS-1, MENU-AS-2, MENU-AS-3, MENU-AS-4, MENU-AS-5, MENU-AS-6, MENU-AS-7, MENU-AS-8, MENU-DLG-1
# spec: MENU-RT-1, MENU-RT-2, MENU-RT-4, MENU-EVT-2, MENU-HUD-1, MENU-HUD-2, MENU-HUD-3, MENU-ED-2
def menus(game: Path) -> None:
    probe_checks('menus', game)


# spec: BOT-GS-4, BOT-GS-6, BOT-GS-8
def modes(game: Path) -> None:
    probe_checks('modes', game)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    try:
        opfor = paths.find_opfor(a.opfor)
    except Exception:  # noqa: BLE001 - no OPFOR folder on this PC
        opfor = None
    game = Path(paths.game_dir(opfor)) if opfor else None
    if game is None or not game.is_dir():
        print('skipped: no game copy (Project OPFOR BFBC2)')
        return 0
    tp, fx = probe('terrain'), probe('effects_chain')
    bundles = tp.stock_bundles(game)
    t = Terrains(bundles, tp)
    steps: list[Callable[[], None]] = [lambda: height_fields(t), lambda: visual_terrains(t),
                                       lambda: terrain_partitions(t), lambda: emitters(bundles, fx),
                                       lambda: height_field_tails(t), lambda: material_maps(t),
                                       spg_materials, spg_destruction, spg_lighting, spg_vehicles, spg_weapons,
                                       lambda: grime(game), lambda: menus(game), lambda: modes(game)]
    for step in steps:
        step()
    print(f'{"FAILED " + str(len(FAILED)) if FAILED else "passed"}: the specification probes\' counts '
          f'({len(bundles)} DICE bundles)')
    return 1 if FAILED else 0


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    sys.exit(main())
