r"""The specification's probe counts as checks (round SPG2): the stable facts `spec/probes/terrain.py` and
`spec/probes/effects_chain.py` measured on DICE's bundles of Project OPFOR BFBC2's game copy, asserted, so that the
rules they back are verified instead of observed. Read only; skipped without the game copy.

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
        seen: set[str] = set()
        for path in bundles:
            es = [e for e in fbrb.entries(path) if not e.deleted]
            self.partition_names |= {e.name.lower()[:-4] for e in es if e.name.lower().endswith('.dbx')}
            es = [e for e in es if e.name.lower() not in seen]
            want = [e.name for e in es if e.type in ('TerrainHeightfield', 'VisualTerrain')
                    or (e.type == 'ITexture' and 'masktile' in e.name.lower())
                    or re.fullmatch(r'terrains/([^/]+)/\1\.dbx', e.name.lower())]
            if not want:
                continue
            types = {e.name: e.type for e in es}
            got = fbrb.read(path, want)
            for n in want:
                seen.add(n.lower())
                d = got[n]
                if n.lower().endswith('.dbx'):
                    self.partitions.append(dbx.parse(d))
                elif types[n] == 'ITexture':
                    v = struct.unpack_from('<8I', d)
                    self.atlas[n.lower()[:-4] if n.lower().endswith('.res') else n.lower()] = (v[4], v[5], v[2], v[7])
                elif types[n] == 'TerrainHeightfield':
                    if struct.unpack_from('<I', d, 0x20)[0]:
                        self.main[n] = (struct.unpack_from('<I', d, 0x18)[0], struct.unpack_from('<6f', d))
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
                                       lambda: terrain_partitions(t), lambda: emitters(bundles, fx)]
    for step in steps:
        step()
    print(f'{"FAILED " + str(len(FAILED)) if FAILED else "passed"}: the specification probes\' counts '
          f'({len(bundles)} DICE bundles)')
    return 1 if FAILED else 0


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    sys.exit(main())
