r"""The specification's probe counts as checks (rounds SPG2, SPG2-rest, SPV1): the stable facts the probes of `spec/probes/`
measure on DICE's bundles of Project OPFOR BFBC2's game copy, asserted, so that the rules they back are verified
instead of observed. Read only; skipped without the game copy. `terrain.py` and `effects_chain.py` are read through
their functions; SPG's five print-only probes (`materials`, `destruction`, `lighting`, `vehicles`, `weapons`) by the
lines of their output that hold a rule's counts (EXPECT; `<head>§<line>` for a line inside one block of the output);
`grime.py`, `menus.py`, `level.py` and `registry.py` by their own `checks()`.

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


# spec: FX-PROC-1, FX-PROC-2, FX-PROC-3
def emitters(bundles: list[Path], fx: ModuleType) -> None:
    seen: set[str] = set()
    docs = spawn_root = order_ok = order_bad = type_ok = type_bad = 0
    emittable: collections.Counter[str] = collections.Counter()
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
            if tmpl is not None:
                emittable[fx.text(tmpl.child('EmittableType'))] += 1
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
    check(emittable == collections.Counter({'ScreenAlignedQuad': 30443, 'DirectionAlignedQuad': 7830, 'ParticleMesh': 7339,
                                            'Quad': 4529}), f'FX-PROC-3: EmittableType over the templates ({dict(emittable)})')


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
        ('MAT-GRID-7', 'CollisionDamageMultiplier: 0 .. 1000 (162950 values, 24 distinct; most: 1 x 70432, 0 x 39961, 3 x 10932, 20 x 10846)'),
        ('MAT-GRID-7', 'CollisionDamageThreshold: 0 .. 800 (162950 values, 20 distinct; most: 30 x 109192, 0 x 20102'),
        ('MAT-GRID-7', 'DamagePenetrationMultiplier: 0 .. 1.6 (162950 values, 11 distinct; most: 1 x 158562, 0 x 1682, 0.8 x 1320'),
        ('MAT-GRID-7', 'DamageProtectionMultiplier: 0 .. 10 (162950 values, 43 distinct; most: 1 x 93256, 0 x 49257, 0.5 x 9806'),
        ('MAT-GRID-7', 'DamageProtectionThreshold: -100 .. 290 (162950 values, 17 distinct; most: 2 x 148258, 0 x 8986, 100 x 1560'),
        ('MAT-GRID-7', 'ExplosionCoverDamageModifier: 0 .. 1 (162950 values, 6 distinct; most: 1 x 150585, 0 x 5736, 0.5 x 4523'),
        ('MAT-GRID-7', 'InflictsDemolitionDamage: true 127194, false 35756'),
        ('MAT-GRID-7', 'NeverPenetrate: true 11246, false 151704'),
        ('MAT-GRID-9', 'MaterialPropertyImpulseData.ImpulseAbsorptionMultiplier: 0.1 .. 0.1 (939 values, 1 distinct'),
        ('MAT-GRID-9', 'MaterialPropertyPhysicsData.DynamicFrictionModifier: 1 .. 1 (651 values, 1 distinct'),
        ('MAT-GRID-9', 'MaterialPropertyPhysicsData.RestitutionModifier: 1 .. 1 (651 values, 1 distinct'),
        ('MAT-GRID-9', 'MaterialPropertyPhysicsData.StaticFrictionModifier: 1 .. 1 (651 values, 1 distinct'),
        ('MAT-GRID-9', 'MaterialPropertySoundData.Softness: 0 .. 0 (3352 values, 1 distinct'),
        ('MAT-GRID-9', 'MaterialPropertyTerrainData.DirtTriggerFactor: 0.001 .. 0.1 (363 values, 2 distinct; most: 0.1 x 321, 0.001 x 42)'),
        ('MAT-GRID-9', 'MaterialPropertyTerrainData.DestructionEffect null: 363'),
        ('MAT-GRID-9', 'MaterialPropertySoundData.SoldierSettings set: 3298'),
        ('MAT-GRID-9', 'MaterialSoldierSoundSettings.FootStepSound set: 2067'),
        ('MAT-GRID-9', 'MaterialSoldierSoundSettings.LandSound set: 2167'),
        ('MAT-GRID-9', 'MaterialSoldierSoundSettings.ProneSound null: 2167'),
        ('MAT-SLOT-3', 'PartInfoDatas: 4454; part materials (lower case), most used: brick 898, metal 425, wood_furniture 350, glass_breakable 332, wood 190, wood_fragile 182, concrete 174, defaultmaterial 167, tree_banger_branch 125, concrete_indestructible 122'),
        ('MAT-SLOT-3', 'distinct part materials: 65; parts on a material with Penetrable 1264, ClientDestructible 427, SeeThrough 520; {}'),
        ('MAT-SLOT-4', 'FaceMaterials items per file: 1 .. 9 (1183 values, 9 distinct; most: 1 x 532, 2 x 337, 3 x 183'),
        ('MAT-SLOT-4', 'face lists equal to MaterialMappings: 525'),
        ('DSTR-MESH-4', 'SootPerPixel 74, SootPerVertex 74'),
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
        ('DSTR-PART-1', "part flags set: {'IsSupported': 181, 'IsFragile': 196, 'IsLoadBearing': 1, 'CanTriggerCollapse': 2904, 'CanCollapse': 3103}"),
        ('DSTR-PART-1', 'parts: 3135'),
        ('DSTR-PART-2', "SpawnedBanger references: {'other name': 16, 'target named *_banger': 20}; distinct targets 35"),
        ('DSTR-PART-2', "'SpawnedBanger': 36"),
        ('DSTR-PART-2', "'CopyDamageToBanger true': 5949"),
        ('DSTR-PART-2', "'Explosion': 22, 'LoosePartPhysics': 20, 'LoosePartPhysicsArray items': 96, 'PreferedShapeType STBox': 5945"),
        ('DSTR-PART-2', "{'banger': 44,"),
        ('DSTR-MG-1', "member types: {'TreeMemberData': 396, 'MiddlegrowthModelGroupMemberData': 341, 'RigidModelGroupMemberData': 32, 'StaticModelGroupMemberData': 2583}"),
        ('DSTR-MG-1', 'MiddlegrowthModelGroupMemberData PhysicsPartCountPerInstance: {1: 341}'),
        ('DSTR-MG-1', 'RigidModelGroupMemberData PhysicsPartCountPerInstance: {1: 32}'),
        ('DSTR-MG-1', 'StaticModelGroupMemberData PhysicsPartCountPerInstance: {1: 1725, 2: 375, 3: 91, 4: 85, 5: 174, 6: 88, 8: 10, 9: 4, 14: 31}'),
        ('DSTR-MG-1', 'TreeMemberData PhysicsPartCountPerInstance: {1: 37, 2: 110, 3: 101, 4: 135, 5: 13}'),
        ('DSTR-MG-1', "member types whose blueprint has destructible parts: {'TreeMemberData': 396, 'StaticModelGroupMemberData': 1837}; static group instances of them: 80239"),
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
        ('FX-CLS-3', 'EffectEntityData: 998'),
        ('FX-CLS-3', 'CullDistance: 0 .. 350 (998 values, 8 distinct; most: 0 x 977, 300 x 11, 350 x 3'),
        ('FX-CLS-3', 'MaxInstanceCount: 1 .. 100 (998 values, 22 distinct; most: 10 x 237, 50 x 214, 30 x 172, 20 x 147)'),
        ('LIGHT-BAKE-3', "distinct ITexture names by suffix: {'_oc': 65, '_ocrgb': 43, '_occlusion': 31, '_ao': 20, '_skyoc': 14, '_envmap': 13, '_ambientocclusion': 6, '_lightmapcolor': 1, '_lightmap': 1}"),
        ('WTH-CLS-1', "'Weather.WeatherSystemSettings': 30, 'Weather.StaticCloudLayerData': 28, 'Weather.WeatherSystemSetup': 28, 'Weather.WeatherSystemData': 28, 'Weather.StaticOutdoorLightData': 27, 'Weather.StaticFogData': 27, 'Weather.WeatherSystemEntityData': 24, 'Entity.ObjectBlueprint': 3"),
        ('WTH-CLS-2', "primary classes: {'Weather.WeatherSystemState': 355, 'Weather.DynamicOutdoorLightData': 353, 'Weather.DynamicSkyData': 353, 'Render.DynamicPostProcessData': 353, 'Weather.DynamicSunData': 353, 'Weather.DynamicFogData': 353, 'Weather.DynamicCloudLayerData': 352"),
        ('WTH-STATE-3', 'ShadowViewDistance: -1 .. 315 (350 values, 27 distinct; most: -1 x 155'),
        ('WTH-STATE-3', 'ViewDistance: 1 .. 10000 (350 values'),
        ('WTH-STATE-3', 'WorldViewDistance: 10 .. 20000 (350 values'),
        ('WTH-STATE-3', "ApplyFog: {'false': 350}"),
        ('WTH-STATE-3', 'BottomColorIntensity: 0 .. 5 (348 values'),
        ('WTH-STATE-3', 'DiffuseEnvmapScale: 0 .. 10 (348 values'),
        ('WTH-STATE-3', 'KeyColorIntensity: 0 .. 10 (348 values, 58 distinct; most: 3 x 60'),
        ('WTH-STATE-3', 'KeySpecularScale: 0 .. 10 (348 values'),
        ('WTH-STATE-3', 'ReflectionEnvmapScale: 0 .. 10 (348 values, 4 distinct; most: 1 x 321, 0 x 10, 2 x 10, 10 x 7)'),
        ('WTH-STATE-3', 'ShadowScale: 0.01 .. 1 (348 values'),
        ('WTH-STATE-3', 'TopColorIntensity: 0 .. 3 (348 values'),
        ('WTH-STATE-3', 'BottomColor: [0] 0 .. 1; [1] 0 .. 1; [2] 0 .. 1 (348 values'),
        ('WTH-STATE-3', 'KeyColor: [0] 0 .. 10; [1] 0 .. 10; [2] 0 .. 10 (348 values'),
        ('WTH-STATE-3', 'TopColor: [0] 0 .. 1; [1] 0.011 .. 1; [2] 0.029 .. 1 (348 values'),
        ('WTH-STATE-3', 'Gain: 0 .. 200 (348 values, 18 distinct; most: 40 x 98, 10 x 46, 30 x 36, 0 x 35)'),
        ('WTH-STATE-3', 'Phi: 14.694 .. 180 (348 values'),
        ('WTH-STATE-3', 'Size: 0 .. 0.025 (348 values'),
        ('WTH-STATE-3', 'Theta: 0 .. 360 (348 values'),
        ('WTH-STATE-3', 'Color: [0] 0 .. 10; [1] 0 .. 9.287; [2] 0 .. 5.961 (348 values'),
        ('WTH-STATE-3', 'End: 100 .. 200 (348 values, 2 distinct'),
        ('WTH-STATE-3', 'Start: 0 .. 100 (348 values, 2 distinct'),
        ('WTH-STATE-3', 'Color: [0] 0 .. 0.984; [1] 0 .. 0.819; [2] 0 .. 0.851 (348 values'),
        ('WTH-STATE-3', 'MaxExposure: 0.4 .. 20 (348 values'),
        ('WTH-STATE-3', 'MiddleGray: 0.004 .. 1 (348 values'),
        ('WTH-STATE-3', 'MinExposure: 0 .. 20 (348 values'),
        ('WTH-STATE-3', 'BloomScale: [0] 0 .. 1; [1] 0 .. 1; [2] 0 .. 1 (348 values'),
        ('WTH-STATE-3', 'Brightness: [0] 0 .. 10; [1] 0 .. 10; [2] 0 .. 10 (348 values'),
        ('WTH-STATE-3', 'Contrast: [0] 0.1 .. 2; [1] 0.1 .. 2; [2] 0.1 .. 2 (348 values'),
        ('WTH-STATE-3', 'Saturation: [0] 0 .. 1.2; [1] 0 .. 1.2; [2] 0 .. 1.2 (348 values'),
    ],
    'vehicles': [
        ('VEH-ENT-1', 'VehicleEntityData: 97 (land x26, air x24, stationary weapons x17, vietnam x9, sea x8, '
                      'beach x8, static x5)'),
        ('VEH-ENT-2', 'HUD types: Car x22, Helicopter x18, Stationary x15, Tank x11, Dummy x10, Boat x9'),
        ('VEH-SPAWN-1', 'level partitions: 61 multiplayer, 13 campaign'),
        ('VEH-SPAWN-1', 'total mp 1044, sp 367'),
        ('VEH-SPAWN-2', 'mp RespawnRange: 10 x1, 15 x1026, 30 x17'),
        ('VEH-PHYS-1', '-- Car: §engine configs: CombustionEngineConfigData x22'),
        ('VEH-PHYS-1', '-- Tank: §engine configs: CombustionEngineConfigData x11'),
        ('VEH-PHYS-1', '-- Helicopter: §engine configs: PropellerEngineConfigData x38'),
        ('VEH-PHYS-1', '-- Boat: §engine configs: JetEngineConfigData x9'),
        ('VEH-PHYS-1', '-- Car: §gearbox configs: GearboxConfigData x22; forward gears: 3 x6, 4 x11, 5 x5'),
        ('VEH-PHYS-1', '-- Tank: §gearbox configs: GearboxConfigData x11; forward gears: 3 x6, 4 x5'),
        ('VEH-PHYS-1', '-- Helicopter: §gearbox configs: GearboxConfigData x18; forward gears: 5 x18'),
        ('VEH-PHYS-1', '-- Boat: §gearbox configs: GearboxConfigData x9; forward gears: 1 x3, 2 x2, 3 x4'),
        ('VEH-PHYS-2', '-- Car: §body mass kg: 1000 x2, 1500 x15, 2500 x5'),
        ('VEH-PHYS-2', '-- Tank: §body mass kg: 4000 x1, 5000 x5, 8000 x4, 9000 x1'),
        ('VEH-PHYS-2', '-- Helicopter: §body mass kg: 4000 x1, 5000 x17'),
        ('VEH-PHYS-2', '-- Boat: §body mass kg: 750 x4, 1000 x3, 2500 x1, 4500 x1'),
        ('VEH-PHYS-2', '-- Car: §wheels per vehicle: 3 x1, 4 x16, 6 x5'),
        ('VEH-PHYS-2', '-- Tank: §wheels per vehicle: 14 x5, 16 x2, 18 x2, 22 x1, 26 x1'),
        ('VEH-PHYS-2', '-- Helicopter: §wheels per vehicle: 2 x1, 3 x17'),
        ('VEH-HP-1', '-- Boat: §HealthZonesShareDamage: true x9'),
        ('VEH-HP-1', '-- Car: §HealthZonesShareDamage: true x22'),
        ('VEH-HP-1', '-- Count: §HealthZonesShareDamage: true x1'),
        ('VEH-HP-1', '-- Dummy: §HealthZonesShareDamage: true x10'),
        ('VEH-HP-1', '-- Helicopter: §HealthZonesShareDamage: true x18'),
        ('VEH-HP-1', '-- Stationary: §HealthZonesShareDamage: true x15'),
        ('VEH-HP-1', '-- StationaryUAV: §HealthZonesShareDamage: true x4'),
        ('VEH-HP-1', '-- Strategic: §HealthZonesShareDamage: true x4'),
        ('VEH-HP-1', '-- Tank: §HealthZonesShareDamage: true x11'),
        ('VEH-HP-1', '-- UAV: §HealthZonesShareDamage: true x3'),
        ('VEH-HP-1', '-- Tank: §FrontHealthZone MaxHealth 500 x1, 1000 x1, 1200 x1, 1250 x8'),
        ('VEH-HP-1', '-- Helicopter: §FrontHealthZone MaxHealth 500 x9, 800 x2, 1200 x2, 1500 x1, 1600 x4'),
        ('VEH-HP-1', '-- Car: §FrontHealthZone MaxHealth 200 .. 1250 (6 distinct; mode 500)'),
        ('VEH-HP-1', '-- Boat: §FrontHealthZone MaxHealth 100 x3, 250 x1, 500 x3, 850 x1, 1000 x1'),
        ('VEH-HP-1', '-- Stationary: §FrontHealthZone MaxHealth 125 x15'),
        ('VEH-HP-1', '-- Strategic: §FrontHealthZone MaxHealth 200 x3, 500 x1'),
        ('VEH-HP-1', '-- UAV: §FrontHealthZone MaxHealth 50 x2, 100 x1'),
        ('VEH-HP-1', '-- Tank: §LowHealthTreshold / front MaxHealth: 0.04 .. 0.6 (6 distinct; mode 0.27)'),
        ('VEH-HP-1', '-- Car: §LowHealthTreshold / front MaxHealth: 0.1 .. 0.75 (7 distinct; mode 0.3)'),
        ('VEH-HP-1', '-- Helicopter: §LowHealthTreshold / front MaxHealth: 0.033 .. 0.3 (8 distinct; mode 0.3)'),
        ('VEH-HP-1', '-- Stationary: §LowHealthTreshold / front MaxHealth: 0.4 x15'),
        ('VEH-HP-2', '-- Tank: §MinDamageAngle 0 x3, 1 x8; DamageAngleMultiplier 0.75 x2, 1 x9; UseDamageAngleCalculation true x10, false x1'),
        ('VEH-HP-2', '-- Tank: §RearHealthZone MaxHealth 500 x2, 1000 x1, 1250 x8; MinDamageAngle 0 x1, 0.5 x1, 0.58 x4, 0.6 x4, 1 x1; DamageAngleMultiplier 1 x2, 2 x8, 3 x1'),
        ('VEH-HP-2', '-- Tank: §LeftHealthZone MaxHealth 500 x1, 1000 x1, 1200 x1, 1250 x8; MinDamageAngle 0 x1, 0.43 x2, 0.5 x4, 0.6 x3, 1 x1; DamageAngleMultiplier 1 x2, 2 x9'),
        ('VEH-HP-2', '-- Boat: §UseDamageAngleCalculation false x9'),
        ('VEH-HP-2', '-- Car: §UseDamageAngleCalculation false x22'),
        ('VEH-HP-2', '-- Count: §UseDamageAngleCalculation false x1'),
        ('VEH-HP-2', '-- Dummy: §UseDamageAngleCalculation false x10'),
        ('VEH-HP-2', '-- Helicopter: §UseDamageAngleCalculation false x18'),
        ('VEH-HP-2', '-- Stationary: §UseDamageAngleCalculation false x15'),
        ('VEH-HP-2', '-- StationaryUAV: §UseDamageAngleCalculation false x4'),
        ('VEH-HP-2', '-- Strategic: §UseDamageAngleCalculation false x4'),
        ('VEH-HP-2', '-- UAV: §UseDamageAngleCalculation false x3'),
        ('VEH-HP-4', '-- Boat: §chassis HealthStates: 0/100000 x5, 10000/100000 x4'),
        ('VEH-HP-4', '-- Car: §chassis HealthStates: 0/100000 x22'),
        ('VEH-HP-4', '-- Count: §chassis HealthStates: 100000 x1'),
        ('VEH-HP-4', '-- Dummy: §chassis HealthStates: 0/100000 x7, 100/100000 x2, 10000 x1'),
        ('VEH-HP-4', '-- Helicopter: §chassis HealthStates: 0/100000 x10, 100/100000 x8'),
        ('VEH-HP-4', '-- Stationary: §chassis HealthStates: 0/100000 x15'),
        ('VEH-HP-4', '-- StationaryUAV: §chassis HealthStates: 0/100000 x4'),
        ('VEH-HP-4', '-- Strategic: §chassis HealthStates: 0/100000 x4'),
        ('VEH-HP-4', '-- Tank: §chassis HealthStates: 0/100000 x11'),
        ('VEH-HP-4', '-- UAV: §chassis HealthStates: 0 x3'),
        ('VEH-HP-4', '-- Car: §PartComponentData per vehicle: 2 .. 42 (12 distinct; mode 36)'),
        ('VEH-HP-4', '-- Helicopter: §PartComponentData per vehicle: 8 .. 31 (9 distinct; mode 24); their first state health: 0 x334, 100 x12, 10000 x3, 100000 x15'),
        ('VEH-HP-4', '-- Tank: §PartComponentData per vehicle: 0 .. 35 (8 distinct; mode 17)'),
        ('VEH-SEAT-1', '-- Car: §player entries per vehicle: 1 x2, 2 x3, 3 x2, 4 x11, 5 x4; soldier entries: 0 x6, 1 x11, 2 x1, 3 x1, 5 x3; MaxPlayersInVehicle: -1 x22'),
        ('VEH-SEAT-1', '-- Tank: §player entries per vehicle: 2 x3, 4 x8; soldier entries: 0 x11; MaxPlayersInVehicle: -1 x11'),
        ('VEH-SEAT-1', '-- Helicopter: §player entries per vehicle: 2 x12, 3 x6; soldier entries: 0 x4, 2 x9, 3 x5; MaxPlayersInVehicle: -1 x18'),
        ('VEH-SEAT-1', '-- Boat: §player entries per vehicle: 1 x1, 2 x1, 3 x2, 4 x5; soldier entries: 0 x5, 1 x4; MaxPlayersInVehicle: -1 x9'),
        ('VEH-SEAT-1', '-- UAV: §MaxPlayersInVehicle: 1 x3'),
        ('VEH-SEAT-1', 'Tank: [0,1,2,3] x7, [0,1] x3, [1,1,2,3] x1'),
        ('VEH-SEAT-1', 'Helicopter: [0,1] x12, [0,1,2] x6'),
        ('VEH-SEAT-1', 'Car: [0,1,3,4] x8,'),
        ('VEH-SEAT-1', 'Stationary: [1] x15'),
        ('VEH-SEAT-2', 'Helicopter (air): driver: input/bfbc2airinputconcepts + input/inputmappings/bfbc2airinputmapping x17, other: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x17, other: input/bfbc2soldierinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x3'),
        ('VEH-SEAT-2', 'Stationary (stationary weapons): other: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x13'),
        ('VEH-SEAT-2', 'Car (land): other: input/bfbc2soldierinputconcepts + input/inputmappings/bfbc2soldierinputmapping x22, driver: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2vehicleinputmapping x13'),
        ('VEH-SEAT-2', 'Tank (land): other: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x14, driver: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2vehicleinputmapping x5'),
        ('VEH-SEAT-2', 'Tank (vietnam): driver: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2vehicleinputmapping x2, other: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x2'),
        ('VEH-SEAT-2', 'Boat (sea): other: input/bfbc2soldierinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x7, driver: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2vehicleinputmapping x6'),
        ('VEH-SEAT-2', 'other: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x3, other: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2vehicleinputmapping x3'),
        ('VEH-SEAT-2', 'Boat (vietnam): other: input/bfbc2vehicleinputconcepts + input/inputmappings/bfbc2gunnerinputmapping x2'),
        ('VEH-SEAT-2', 'Tank (beach): driver: input/venicetankinputconcepts + objects/vehicles/input x1, other: input/venicesoldierinputconcepts + objects/vehicles/input x1'),
        ('VEH-SEAT-2', 'Dummy (beach): driver: - + - x2'),
        ('VEH-WPN-1', '-- Tank: §WeaponComponentData per vehicle: 3 .. 13 (6 distinct; mode 10)'),
        ('VEH-WPN-1', '-- Helicopter: §WeaponComponentData per vehicle: 2 .. 8 (6 distinct; mode 3)'),
        ('VEH-WPN-1', '-- Car: §WeaponComponentData per vehicle: 0 x4, 1 x7, 2 x10, 3 x1'),
        ('VEH-WPN-1', '-- Boat: §WeaponComponentData per vehicle: 0 x4, 1 x1, 2 x3, 3 x1'),
        ('VEH-WPN-1', '-- Stationary: §WeaponComponentData per vehicle: 1 x15'),
        ('VEH-WPN-1', '-- Tank: §weapon projectiles: BulletEntityData x109, MissileEntityData x3'),
        ('VEH-WPN-1', '-- Helicopter: §weapon projectiles: BulletEntityData x39, MissileEntityData x26, TracerDartEntityData x2'),
        ('VEH-WPN-1', '-- StationaryUAV: §weapon projectiles: VehicleProjectileEntityData x3'),
        ('VEH-MAT-1', '-- Tank: §Material: Medium_Tank x6, Tank x3, Tank_Vietnam x2'),
        ('VEH-MAT-1', '-- Car: §Material: Armoured_Jeep x8, Civilian_Vehicle x6, Light_Jeep x5, Light_Jeep_Vietnam x3'),
        ('VEH-MAT-1', '-- Helicopter: §Material: Transport_Helo x13, Attack_Helo x4, HUEY x1'),
        ('VEH-MAT-1', '-- Boat: §Material: Light_Boat x7, Civilian_Vehicle x1, Light_Rubber_Boat x1'),
        ('VEH-MAT-1', '-- Stationary: §Material: Stationary_Metal x15'),
        ('VEH-MAT-1', '-- Strategic: §Material: Stationary_Metal x3, Stationary_Metal_AA x1'),
        ('VEH-MAT-1', 'jap_zero_fighter/zero_fighter (sp): Dummy/helicopter; Attack_Helo;'),
        ('VEH-MAT-1', 'us_corsair/corsair (sp): Dummy/helicopter; default;'),
        ('VEH-WW2-1', "jap_zero_fighter/zero_fighter (sp): Dummy/helicopter; Attack_Helo; zones 500.0; engines PropellerEngineConfigData x1; seats ['-']"),
        ('VEH-WW2-1', 'kurogane/kurogane (sp): Car/wheeled;'),
        ('VEH-WW2-1', 'kurogane/kurogane_ai (sp): Car/wheeled;'),
        ('VEH-WW2-1', 'stationary_aa_bofors/aa_bofors (sp): Strategic/stationary;'),
        ('VEH-WW2-1', 'stationary_hmg_30cal/30cal_cutscenesp01 (sp): Stationary/stationary;'),
        ('VEH-WW2-1', 'stationary_hmg_30cal/30cal_window (sp): Stationary/stationary;'),
        ('VEH-WW2-1', 'type97-chiha/type97-chiha (sp): Tank/tracked;'),
        ('VEH-WW2-1', "us_corsair/corsair (sp): Dummy/helicopter; default; zones 500.0; engines PropellerEngineConfigData x1; seats ['-']"),
        ('VEH-CON-1', 'entry names: 47 (animations/cutscenes/rome x32, objects/mesh_particles/specific x3, objects/objects/distant_ships x8, sound/weapons/stationary x2, textures/vehicles/markv x1, textures/weapons/stationary x1)'),
        ('VEH-CON-1', 'level folders holding them, per object folder: objects/mesh_particles/specific 2, objects/objects/distant_ships 2'),
        ('VEH-CON-1', 'under objects/vehicles/: 0; holding a VehicleEntityData: 0'),
    ],
    'weapons': [
        ('WPN-CLS-1', '359 GameSharedResources.SoldierWeaponData'),
        ('WPN-CLS-1', '574 GameSharedResources.ZoomLevelData'),
        ('WPN-PATCH-1', 'with a Dist copy too: 351, of which differ from Dist\'s (XML form): 325'),
        ('WPN-WW2-1', 'partitions in BF1943\'s folder (objects/weapons/handheld/beach/): 76'),
        ('WPN-CLS-2', '-- wcUgl: 82 weapons'),
        ('WPN-CLS-2', '-- wcAssault: 52 weapons'),
        ('WPN-CLS-2', '-- wcSmg: 44 weapons'),
        ('WPN-CLS-2', '-- wcShotgun: 42 weapons'),
        ('WPN-CLS-2', '-- wcLmg: 34 weapons'),
        ('WPN-CLS-2', '-- wcSniper: 32 weapons'),
        ('WPN-CLS-2', '-- wcAt: 6 weapons'),
        ('WPN-CLS-2', '-- wcHgr: 4 weapons'),
        ('WPN-CLS-2', '-- wcKnife: 4 weapons'),
        ('WPN-CLS-2', '-- wcC4: 3 weapons'),
        ('WPN-CLS-2', '-- wcMortarStrike: 3 weapons'),
        ('WPN-CLS-2', '-- wcNone: 3 weapons'),
        ('WPN-CLS-2', '-- wcATMine: 2 weapons'),
        ('WPN-CLS-2', '-- wcAmmoBox: 2 weapons'),
        ('WPN-CLS-2', '-- wcMedKit: 2 weapons'),
        ('WPN-CLS-2', '-- wcPowerTool: 2 weapons'),
        ('WPN-CLS-2', '-- wcLaserDesignator: 2 weapons'),
        ('WPN-CLS-2', '-- wcMotionSensor: 1 weapons'),
        ('WPN-CLS-2', '-- wcTracerDart: 1 weapons'),
        ('WPN-CLS-2', '-- wcSmg:§AnimBaseSet: wabsRif x35, wabsHg x9'),
        ('WPN-CLS-2', '-- wcAssault:§AnimBaseSet: wabsRif x51, wabsHg x1'),
        ('WPN-CLS-3', 'AnimBaseSet: wabsRif x197, wabsShg x20, wabsHg x10'),
        ('WPN-CLS-3', '-- wcAt:§AnimBaseSet: wabsAT x6'),
        ('WPN-CLS-3', '-- wcHgr:§AnimBaseSet: wabsHgr x4'),
        ('WPN-CLS-3', '-- wcC4:§AnimBaseSet: wabsHgr x3'),
        ('WPN-CLS-3', '-- wcATMine:§AnimBaseSet: wabsHgr x2'),
        ('WPN-CLS-3', '-- wcPowerTool:§AnimBaseSet: wabsHg x2'),
        ('WPN-CLS-3', '-- wcShotgun:§AnimBaseSet: wabsRif x22, wabsShg x20'),
        ('WPN-DMG-1', 'BulletEntityData: 293 weapons over 85 partitions'),
        ('WPN-DMG-1', 'with an Explosion: 72 (ExplosionEntityData x72)'),
        ('WPN-DMG-3', 'wcAssault / fltAutomaticFire: 14 firings; 16.7->14.3 over 18-64 m x4, 14.3->14.3 over 18-64 m x2, 16.7->12.5 over 18-64 m x2, 12.5->12.5 over 18-42 m x2, 25->20 over 18-64 m x1, 22->18 over 18-64 m x1, 18->13 over 18-64 m x1, 20->17 over 18-64 m x1'),
        ('WPN-DMG-3', 'wcAssault / fltSingleFire: 9 firings; 39->28 over 8-64 m x3, 14.3->14.3 over 18-64 m x2, 16.7->14.3 over 18-64 m x2, 39->28 over 12-64 m x1, 33.4->25 over 12-42 m x1'),
        ('WPN-DMG-3', 'wcLmg / fltAutomaticFire: 14 firings; 14.3->12.5 over 18-64 m x4, 14.3->11.2 over 18-64 m x3, 16.7->14 over 18-64 m x2, 11.2->10 over 18-64 m x2, 16.7->14.3 over 18-64 m x2, 20->16.7 over 18-64 m x1'),
        ('WPN-DMG-3', 'wcShotgun / fltAutomaticFire: 3 firings; 39->28 over 8-64 m x1, 29->18 over 8-64 m x1, 14.3->6 over 8-18 m x1'),
        ('WPN-DMG-3', 'wcShotgun / fltSingleFire: 3 firings; 29->18 over 8-64 m x1, 39->28 over 8-42 m x1, 14.3->6 over 8-18 m x1'),
        ('WPN-DMG-3', 'wcShotgun / fltSingleFireWithBoltAction: 15 firings; 100->50 over 10-36 m x5, 85->30 over 10-36 m x5, 14.3->6 over 8-18 m x4, 15->6 over 8-18 m x1'),
        ('WPN-DMG-3', 'wcSmg / fltAutomaticFire: 13 firings; 16.7->11.2 over 18-42 m x2, 16.7->12.5 over 18-42 m x2, 20->12.5 over 18-42 m x2, 20.04->15 over 18-42 m x1, 14->11.2 over 18-42 m x1, 15->13.44 over 18-42 m x1, 25->15 over 18-42 m x1, 20->14.3 over 18-42 m x1'),
        ('WPN-DMG-3', 'wcSmg / fltSingleFire: 7 firings; 33.4->25 over 12-42 m x2, 25->16.7 over 12-42 m x2, 12.5->9.1 over 8-42 m x1, 20->14.3 over 12-42 m x1, 40->33.4 over 6-42 m x1'),
        ('WPN-DMG-3', 'wcSniper / fltAutomaticFire: 2 firings; 25->20 over 18-64 m x2'),
        ('WPN-DMG-3', 'wcSniper / fltSingleFire: 8 firings; 39->28 over 8-64 m x4, 39->28 over 12-64 m x2, 45->45 over 8-75 m x2'),
        ('WPN-DMG-3', 'wcSniper / fltSingleFireWithBoltAction: 10 firings; 100->50 over 10-36 m x6, 100->55 over 18-36 m x2, 85->30 over 10-36 m x1, 105->70 over 20-56 m x1'),
        ('WPN-DMG-4', '-- wcKnife:§StartDamage: 120 x3, 125 x1'),
        ('WPN-DMG-4', '-- wcKnife:§InitialSpeed m/s: 1.75 x2, 1.8 x1, 2.25 x1'),
        ('WPN-DMG-4', '-- wcKnife:§TimeToLive s: 1 x4'),
        ('WPN-DMG-4', '-- wcKnife:§Gravity: 0 x4'),
        ('WPN-DMG-4', '-- wcKnife:§RateOfFire rpm: 52 x3, 60 x1'),
        ('WPN-DMG-4', '-- wcKnife:§NumberOfMagazines: -1 x4'),
        ('WPN-DMG-4', '-- wcNone:§StartDamage: 120 x2'),
        ('WPN-DMG-4', '-- wcNone:§InitialSpeed m/s: 1.6 x2, 40 x1'),
        ('WPN-DMG-4', '-- wcPowerTool:§StartDamage: 6.5 x2'),
        ('WPN-DMG-4', '-- wcPowerTool:§RateOfFire rpm: 900 x2'),
        ('WPN-DMG-4', '-- wcPowerTool:§InitialSpeed m/s: 2 x2'),
        ('WPN-DMG-4', '-- wcPowerTool:§NumberOfMagazines: -1 x2'),
        ('WPN-DMG-4', 'gravity -10 x1, -9.81 x260, -9.8 x24, 0 x8'),
        ('WPN-PROJ-1', 'InitialSpeed: 200 x11, 300 x26, 400 x1, 600 x189'),
        ('WPN-PROJ-1', 'InitialSpeed: 300 x25'),
        ('WPN-PROJ-1', 'MissileEntityData: 10 weapons over 10 partitions; speed 25 .. 100 (6 distinct; mode 75)'),
        ('WPN-PROJ-1', 'GrenadeEntityData: 5 weapons over 5 partitions; speed 20 x4, 40 x1'),
        ('WPN-PROJ-1', '-- wcUgl:§InitialSpeed m/s: 40 x1, 50 x57, 300 x24'),
        ('WPN-PROJ-2', 'Gravity: -9.81 x227'),
        ('WPN-PROJ-2', 'Gravity: -9.81 x25'),
        ('WPN-PROJ-2', 'gravity -10 x1, -9.81 x260, -9.8 x24, 0 x8'),
        ('WPN-PROJ-2', 'speed 20 x4, 40 x1; gravity -8 x4, -7 x1'),
        ('WPN-PROJ-2', 'gravity -9.81 .. 0 (7 distinct; mode -1.5)'),
        ('WPN-PROJ-2', '-- wcAt:§TimeToLive s: 10 x1, 30 x5'),
        ('WPN-PROJ-2', '-- wcAssault:§TimeToLive s: 2 x52'),
        ('WPN-PROJ-2', '-- wcSmg:§TimeToLive s: 2 x44'),
        ('WPN-PROJ-2', '-- wcSniper:§TimeToLive s: 2 x32'),
        ('WPN-PROJ-2', '-- wcUgl:§Gravity: -9.81 x57, -9.8 x24, -7 x1'),
        ('WPN-PROJ-3', 'blast damage 0 x24, 10 x4, 20 x10, 100 x34; blast radius 0 x1, 1.1 x14, 5 x57; inner radius 0.1 x25, 0.34 x33, 1 x14'),
        ('WPN-PROJ-3', 'blast damage 290 x5; blast radius 5 x1, 8 x4; inner radius 0.5 x5'),
        ('WPN-PROJ-3', 'blast damage 102 x1, 110 x4; blast radius 7 x1, 8 x4; inner radius 1 x4, 2 x1'),
        ('WPN-PROJ-3', 'blast damage 80 x1, 100 x5, 135 x1, 200 x3; blast radius 2 .. 8 (6 distinct; mode 6)'),
        ('WPN-PROJ-3', '-- wcUgl:§StartDamage: 16.7 x24, 125 x57'),
        ('WPN-AMMO-1', 'NumberOfMagazines: 2 .. 9999 (6 distinct; mode 4)'),
        ('WPN-AMMO-1', 'AmmoPickupMin/Max zero: 0'),
        ('WPN-AMMO-1', 'wcAssault / fltAutomaticFire:§NumberOfMagazines: 4 x13, 6 x1'),
        ('WPN-AMMO-1', 'wcAssault / fltAutomaticFire:§AmmoPickupMin: 30 x14'),
        ('WPN-AMMO-1', 'wcAssault / fltAutomaticFire:§AmmoPickupMax: 60 x14'),
        ('WPN-AMMO-1', 'wcAssault / fltSingleFire:§AmmoPickupMin: 5 x1, 8 x3, 20 x1, 30 x4'),
        ('WPN-AMMO-1', 'wcAssault / fltSingleFire:§AmmoPickupMax: 8 .. 60 (5 distinct; mode 60)'),
        ('WPN-AMMO-1', 'wcLmg / fltAutomaticFire:§MagazineCapacity: 100 x9, 200 x5'),
        ('WPN-AMMO-1', 'wcLmg / fltAutomaticFire:§NumberOfMagazines: 2 x6, 3 x8'),
        ('WPN-AMMO-1', 'wcLmg / fltAutomaticFire:§AmmoPickupMin: 45 x14'),
        ('WPN-AMMO-1', 'wcLmg / fltAutomaticFire:§AmmoPickupMax: 110 x14'),
        ('WPN-AMMO-1', 'wcSmg / fltAutomaticFire:§AmmoPickupMin: 20 x1, 30 x6, 60 x6'),
        ('WPN-AMMO-1', 'wcSmg / fltAutomaticFire:§AmmoPickupMax: 30 x1, 40 x1, 60 x5, 120 x6'),
        ('WPN-AMMO-1', 'wcSmg / fltSingleFire:§AmmoPickupMin: 5 x7'),
        ('WPN-AMMO-1', 'wcSmg / fltSingleFire:§AmmoPickupMax: 10 x5, 12 x2'),
        ('WPN-AMMO-1', 'wcShotgun / fltSingleFireWithBoltAction:§AmmoPickupMax: 16 x15'),
        ('WPN-AMMO-1', '-- wcSniper:§AmmoPickupMin: 3 x32'),
        ('WPN-AMMO-1', '-- wcSniper:§AmmoPickupMax: 8 x32'),
        ('WPN-AMMO-1', '-- wcSniper:§MagazineCapacity: 5 x16, 10 x13, 20 x3'),
        ('WPN-AMMO-1', '-- wcSniper:§NumberOfMagazines: 3 x5, 4 x6, 5 x21'),
        ('WPN-AMMO-2', 'NumberOfMagazines: 4 x4, 6 x2, 9999 x2'),
        ('WPN-AMMO-2', 'jap_type5/type5 (sp): wcAssault BulletEntityData 39->28 over 8-64 m; speed 600.0; gravity -9.8100004196167; mags 9999.0 pickup 8.0/8.0'),
        ('WPN-AMMO-2', 'nambu100/nambu100 (sp): wcSmg BulletEntityData 16.7->12.5 over 18-42 m; speed 600.0; gravity -9.8100004196167; mags 9999.0 pickup 30.0/30.0'),
        ('WPN-SWAY-1', '-- wcSmg:§MaxRecoilAngleX: 0 x41, 0.5 x3'),
        ('WPN-SWAY-1', '-- wcAssault:§MaxRecoilAngleX: 0 x52'),
        ('WPN-SWAY-1', '-- wcLmg:§MaxRecoilAngleX: 0 x34'),
        ('WPN-SWAY-1', '-- wcShotgun:§MaxRecoilAngleX: 0 x42'),
        ('WPN-SWAY-1', '-- wcSniper:§MaxRecoilAngleX: 0 x32'),
        ('WPN-SWAY-1', '-- wcSniper:§MaxRecoilFov: 0.5 x32'),
        ('WPN-SWAY-1', '-- wcLmg:§MaxRecoilFov: 0 x3, 0.6 x27, 0.8 x4'),
        ('WPN-SWAY-2', 'wcAssault / fltAutomaticFire:§Stand.NoZoom.Base.Min: 1 x2, 1.35 x2, 1.5 x8, 2 x2'),
        ('WPN-SWAY-2', 'wcAssault / fltSingleFire:§Stand.NoZoom.Base.Max: 3 x1, 4 x1, 5 x7'),
        ('WPN-SWAY-2', 'wcSmg / fltAutomaticFire:§Stand.NoZoom.Base.Min: 1 x10, 1.25 x2, 1.5 x1'),
        ('WPN-SWAY-2', 'wcLmg / fltAutomaticFire:§Stand.NoZoom.Base.Max: 7 x14'),
        ('WPN-SWAY-2', 'wcShotgun / fltSingleFire:§Stand.NoZoom.Base.Min: 0 x1, 2 x2'),
        ('WPN-SWAY-2', 'wcShotgun / fltSingleFireWithBoltAction:§Stand.NoZoom.Base.Max: 5 x14'),
        ('WPN-SWAY-2', 'wcSniper / fltSingleFireWithBoltAction:§Stand.NoZoom.Base.Min: 7 x10'),
        ('WPN-SWAY-2', 'wcSniper / fltSingleFireWithBoltAction:§Stand.Zoom.RecoilAmpMax: 10 x10'),
        ('WPN-SWAY-2', 'wcSniper / fltSingleFireWithBoltAction:§DeviationScaleFactorNoZoom: 4 x10'),
        ('WPN-SWAY-2', '-- wcSmg:§Stand.Zoom.Base.Min: 0 x35, 0.2 x1, 0.3 x7, 0.8 x1'),
        ('WPN-SWAY-2', '-- wcAssault:§Stand.Zoom.Base.Min: 0 x51, 0.3 x1'),
        ('WPN-SWAY-2', '-- wcLmg:§Stand.Zoom.Base.Min: 0 x34'),
        ('WPN-SWAY-2', '-- wcSniper:§Stand.Zoom.Base.Min: 0 x32'),
        ('WPN-SWAY-3', '-- wcAssault:§AimSteadiness (stand/crouch/prone): 0 x156'),
        ('WPN-SWAY-3', '-- wcSmg:§AimSteadiness (stand/crouch/prone): 0 x132'),
        ('WPN-SWAY-3', '-- wcLmg:§AimSteadiness (stand/crouch/prone): 0 x102'),
        ('WPN-SWAY-3', 'wcSniper / fltSingleFireWithBoltAction:§stand AimSteadiness 0 x2, 1 x8'),
        ('WPN-SWAY-3', 'wcSniper / fltSingleFire:§stand AimSteadiness 0 x4, 1 x4'),
        ('WPN-ZOOM-1', '-- wcLmg:§ZoomLevels: 2 x33, 1 x1'),
        ('WPN-ZOOM-1', '-- wcAssault:§zoom[1].FieldOfView: 17 x14, 40 x35, 45 x2, 50 x1'),
        ('WPN-ZOOM-1', '-- wcAssault:§zoom[1].LookSpeedMultiplier: 0.15 x14, 0.2 x35, 0.3 x3'),
        ('WPN-ZOOM-1', '-- wcSniper:§zoom[1].FieldOfView: 8 .. 40 (5 distinct; mode 11.5)'),
        ('WPN-ZOOM-1', '-- wcSniper:§zoom[1].LookSpeedMultiplier: 0.1 x13, 0.15 x10, 0.2 x9'),
        ('WPN-ZOOM-1', '-- wcSniper:§zoom[1].SwayPitchMultiplier: 0 x14, 0.002 x18'),
        ('WPN-ZOOM-1', 'wcSniper / fltSingleFireWithBoltAction:§zoom[1] RecoilMultiplier 0.2 x6, 0.4 x1, 1 x3; zoom[1] RecoilFovMultiplier -6 x4, 1 x6'),
        ('WPN-ZOOM-2', 'ZoomType: ztPressButtonToSwitchZoomLevel x226, ztHoldButtonToZoom x1'),
        ('WPN-ZOOM-2', 'ZoomType: ztHoldButtonToZoom x25'),
        ('WPN-ZOOM-2', '-- wcKnife:§ZoomType: ztHoldButtonToZoom x4'),
        ('WPN-ZOOM-2', '-- wcSniper:§ZoomType: ztPressButtonToSwitchZoomLevel x31, ztHoldButtonToZoom x1'),
        ('WPN-ZOOM-2', '-- wcHgr:§ZoomType: ztPressButtonToSwitchZoomLevel x2, ztHoldButtonToZoom x2'),
        ('WPN-ZOOM-3', 'SprintLookSpeedMultiplier (every zoom level): 1 x454'),
        ('WPN-ZOOM-3', 'SprintLookSpeedMultiplier (every zoom level): 1 x50'),
        ('WPN-ZOOM-4', 'AlwaysAimHead: true x190, false x37'),
        ('WPN-ZOOM-4', '-- wcSniper:§AlwaysAimHead: false x32'),
        ('WPN-ZOOM-4', '-- wcSmg:§AlwaysAimHead: true x40, false x4'),
        ('WPN-ZOOM-4', '-- wcAssault:§AlwaysAimHead: true x51, false x1'),
        ('WPN-ZOOM-4', '-- wcAt:§AlwaysAimHead: false x5, true x1'),
        ('WPN-AIM-1', 'UseAutoAiming over every WeaponFiringData a weapon names (359): false x358, true x1; true on: wcKnife (bfbc2) x1'),
        ('WPN-GREN-1', '-- wcUgl: 82 weapons (bfbc2 x80, vietnam x1, beach x1) --'),
        ('WPN-GREN-1', '-- wcUgl:§projectile classes: BulletEntityData x81, GrenadeEntityData x1'),
        ('WPN-GREN-1', '-- wcUgl:§MagazineCapacity: 1 x82'),
        ('WPN-GREN-1', '-- wcUgl:§NumberOfMagazines: 2 x82'),
        ('WPN-GREN-1', '-- wcUgl:§FireLogicType: fltSingleFire x82'),
        ('WPN-GREN-1', '-- wcUgl:§AnimBaseSet: wabsRif x82'),
        ('WPN-GREN-1', '-- wcUgl:§zoom[1].FieldOfView: 50 x82'),
        ('WPN-GREN-1', '-- wcUgl:§Stand.NoZoom.Base.Max: 5 x81'),
        ('WPN-GREN-1', '-- wcUgl:§Stand.NoZoom.RecoilAmpMax: 10 x81'),
        ('WPN-GREN-1', '-- wcUgl:§BulletsPerShell: 1 x58, 12 x24'),
        ('WPN-GREN-1', 'weapons with SpecializationWeapons items: 47'),
        ('WPN-GREN-2', '-- wcHgr:§projectile classes: GrenadeEntityData x4'),
        ('WPN-GREN-2', '-- wcHgr:§InitialSpeed m/s: 20 x4'),
        ('WPN-GREN-2', '-- wcHgr:§Gravity: -8 x4'),
        ('WPN-GREN-2', '-- wcHgr:§FireLogicType: fltHoldAndRelease x4'),
        ('WPN-GREN-2', '-- wcHgr:§RateOfFire rpm: 85 x4'),
        ('WPN-GREN-2', '-- wcHgr:§NumberOfMagazines: 1 x3, 2 x1'),
        ('WPN-GREN-2', 'type5_rgl/type5_rgl (sp): wcUgl GrenadeEntityData -; speed 40.0; gravity -7.0; mags 2.0 pickup 1.0/2.0'),
        ('WPN-ATT-1', 'specializations per weapon: 0 x248, 3 x33, 1 x23, 4 x13, 2 x2, 5 x2'),
        ('WPN-ATT-1', 'specializations that swap in a weapon asset, per weapon: 0 x273, 2 x46, 3 x2'),
        ('WPN-ATT-1', 'weapons with a specialization swap, per class: wcAssault x12, wcShotgun x12, wcSniper x8, wcLmg x8, wcSmg x8'),
        ('WPN-WW2-2', "-- BF1943's folder (beach/): 8 firearms"),
        ('WPN-WW2-2', 'Gravity: -9.81 x8'),
        ('WPN-WW2-2', 'InitialSpeed: 600 x8'),
        ('WPN-WW2-2', 'UseAutoAiming: false x8'),
        ('WPN-WW2-2', 'ZoomType: ztPressButtonToSwitchZoomLevel x8'),
        ('WPN-WW2-2', 'AnimBaseSet: wabsRif x6, wabsHg x2'),
        ('WPN-WW2-2', 'SprintLookSpeedMultiplier (every zoom level): 1 x16'),
        ('WPN-WW2-4', 'AlwaysAimHead: true x5, false x3'),
        ('WPN-WW2-4', '-- wcSniper:§AlwaysAimHead: false x32'),
        ('WPN-WW2-4', 'thompson/thompson (mp+sp): wcSmg BulletEntityData 16.7->12.5 over 18-42 m; speed 600.0; gravity -9.8100004196167; mags 4.0 pickup 30.0/60.0; ztPressButtonToSwitchZoomLevel; aim head false'),
    ],
}


def holds(out: str, text: str) -> bool:
    """Whether the probe's output holds `text`; `<head>§<line>` asks for the line inside the block the first line
    starting with `<head>` (after its indent) opens (the lines under it indented deeper, up to the first that is not) holding `<line>`."""
    if '§' not in text:
        return text in out
    head, want = text.split('§', 1)
    lines = out.splitlines()
    for i, line in enumerate(lines):
        if line.lstrip().startswith(head):
            indent = len(line) - len(line.lstrip())
            for sub in lines[i + 1:]:
                if not sub.strip() or len(sub) - len(sub.lstrip()) <= indent:
                    break
                if want in sub:
                    return True
            return False
    return False


def expect_lines(name: str) -> None:
    out = probe_output(name)
    for rule, text in EXPECT[name]:
        check(holds(out, text), f'{rule}: spec/probes/{name}.py prints {text[:70]!r}')


# spec: MAT-GRID-1, MAT-GRID-2, MAT-GRID-3, MAT-GRID-4, MAT-GRID-5, MAT-GRID-6, MAT-SLOT-1, DSTR-TERR-3
# spec: MAT-GRID-7, MAT-GRID-9, MAT-SLOT-3, MAT-SLOT-4, DSTR-MESH-4
def spg_materials() -> None:
    expect_lines('materials')


# spec: DSTR-STATE-1, DSTR-STATE-2, DSTR-STATE-4, DSTR-STATE-6, DSTR-MESH-2, DSTR-MESH-3, DSTR-MG-3, DSTR-MG-4
# spec: DSTR-TERR-1, DSTR-PART-1, DSTR-PART-2, DSTR-MG-1
def spg_destruction() -> None:
    expect_lines('destruction')


# spec: LIGHT-PT-5, LIGHT-ENV-1, FX-PLACE-1, LIGHT-SKY-1, LIGHT-SKY-2, FX-CLS-1, FX-PLACE-3, LIGHT-IRR-1, LIGHT-IRR-2
# spec: WTH-STATE-1, FX-CLS-3, LIGHT-BAKE-3, WTH-CLS-1, WTH-CLS-2, WTH-STATE-3
def spg_lighting() -> None:
    expect_lines('lighting')


# spec: VEH-ENT-1, VEH-ENT-2, VEH-SPAWN-1, VEH-SPAWN-2, VEH-PHYS-1, VEH-PHYS-2, VEH-HP-1, VEH-HP-2, VEH-HP-4
# spec: VEH-SEAT-1, VEH-SEAT-2, VEH-WPN-1, VEH-MAT-1, VEH-WW2-1, VEH-CON-1
def spg_vehicles() -> None:
    expect_lines('vehicles')


# spec: WPN-CLS-1, WPN-PATCH-1, WPN-WW2-1, WPN-CLS-2, WPN-CLS-3, WPN-DMG-1, WPN-DMG-3, WPN-DMG-4, WPN-PROJ-1
# spec: WPN-PROJ-2, WPN-PROJ-3, WPN-AMMO-1, WPN-AMMO-2, WPN-SWAY-1, WPN-SWAY-2, WPN-SWAY-3, WPN-ZOOM-1, WPN-ZOOM-2
# spec: WPN-ZOOM-3, WPN-ZOOM-4, WPN-AIM-1, WPN-GREN-1, WPN-GREN-2, WPN-ATT-1, WPN-WW2-2, WPN-WW2-4
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


# spec: LEVEL-PART-1, LEVEL-PART-2, LEVEL-PART-3, LEVEL-DATA-1, LEVEL-OBJ-1, LEVEL-EVT-1, LEVEL-EVT-2, LEVEL-SLOT-2
# spec: LEVEL-MG-1, LEVEL-ENV-1, CULL-LVL-1, CULL-SPH-2, CULL-SPH-3, VEG-CLS-1, DECAL-SRC-2
def level_partitions(game: Path) -> None:
    probe_checks('level', game)


# spec: REG-MFT-1, REG-MFT-3, REG-MFT-5, REG-BM-2, BNDL-KIND-1, BNDL-LOC-1, BNDL-LOC-3, STRM-REG-1
def registries(game: Path) -> None:
    probe_checks('registry', game)


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
                                       lambda: grime(game), lambda: menus(game), lambda: level_partitions(game),
                                       lambda: registries(game)]
    for step in steps:
        step()
    print(f'{"FAILED " + str(len(FAILED)) if FAILED else "passed"}: the specification probes\' counts '
          f'({len(bundles)} DICE bundles)')
    return 1 if FAILED else 0


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    sys.exit(main())
