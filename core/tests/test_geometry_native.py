r"""The geometry readers (rust/bc2-formats terrain, water, havok and rust/bc2-scene collision, through the bc2_native
module) against what the retired Python readers (core/bfbc2core/terrain.py, water.py, collision.py's Python path
through NavKit's levelobjects.py / hkpack.py; round F1a, 2026-11-18) answered, recorded before they were deleted
(geometry_golden.json): the same heights bit for bit, the same vertices, triangles and shaders, the same files
refused, the same collision index.

  python core\tests\test_geometry_native.py           quick: built and damaged files, the collision index of folders
                                                       built here; damaged copies of three of MP_005's packfiles when
                                                       the game copy is there
  python core\tests\test_geometry_native.py --game    + every height field, water file and Havok packfile in every
                                                       bundle of the game copy (Project OPFOR BFBC2's game\, read
                                                       only) and the collision index of every prepared level
                                                       (work\levels, read only)

A bundle or level whose input the recording does not have (changed since) is counted, not compared. Needs the module
(python rust\build_native.py).
"""
import glob
import os
import random
import struct
import sys
import time
from collections import Counter
from collections.abc import Callable
from concurrent.futures import ProcessPoolExecutor
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, HERE)
import goldens  # noqa: E402
from bfbc2core import collision, dbx, fbrb, native, paths, terrain, water  # noqa: E402

GOLDEN = os.path.join(HERE, 'geometry_golden.json')
TYPES = {'TerrainHeightfield': 'terrain', 'WaterMesh': 'water mesh', 'VisualWater': 'water visual',
         'HavokPhysicsData': 'havok'}


# ------------------------------------------------------------------------------------------------ one file

def _bits(a: np.ndarray | None) -> Any:
    """An array's shape, dtype and bytes' digest (bit for bit: NaN payloads and signed zeros count), or None."""
    return None if a is None else (a.shape, a.dtype.str, goldens.h(a.tobytes(), 16))


def _outcome(fn: Callable[[], Any]) -> str:
    try:
        return goldens.rh(('ok', fn()), 10)
    except ValueError:
        return 'ValueError'


def terrain_outcome(data: bytes) -> str:
    def run() -> Any:
        hf = terrain.parse(data)
        return (hf.min.tolist(), hf.max.tolist(), hf.size, _bits(hf.raw), _bits(hf.heights))
    return _outcome(run)


def mesh_outcome(data: bytes) -> str:
    return _outcome(lambda: tuple(_bits(a) for a in water.parse_mesh(data)))


def visual_outcome(data: bytes) -> str:
    def run() -> Any:
        idx, slots, ranges = water.parse_visual(data)
        return _bits(idx), list(slots), [tuple(int(x) for x in r) for r in ranges]
    return _outcome(run)


def shaders_outcome(mesh: bytes, visual: bytes) -> str:
    def run() -> Any:
        _, t = water.parse_mesh(mesh)
        r = water.shaders_of(mesh, visual, t)
        return None if r is None else (list(r[0]), list(r[1]))
    return _outcome(run)


def _rust_triangles(data: bytes) -> np.ndarray | None:
    t = native.require('the Havok reader').havok_triangles(data)
    return None if t is None else np.frombuffer(t, '<f4').reshape(-1, 3, 3)


def havok_outcome(data: bytes) -> str:
    """The collision triangles of a packfile's bytes (levelobjects.triangles' answer; the recording had the retired
    Python's, over hkpack.py)."""
    return _outcome(lambda: _bits(_rust_triangles(data)))


def outcome(kind: str, data: bytes, extra: bytes | None = None) -> str:
    if kind == 'shaders':
        assert extra is not None
        return shaders_outcome(data, extra)
    return {'terrain': terrain_outcome, 'water mesh': mesh_outcome, 'water visual': visual_outcome,
            'havok': havok_outcome}[kind](data)


# ------------------------------------------------------------------------------------------------ built files

def heightfield(n: int, values: list[int], tail: bytes = b'', lo: float = -10.0, hi: float = 90.0, first: int = 0) -> bytes:
    """first: the header's first-section length (0 on a sub-terrain)."""
    head = struct.pack('<6f6IB', -100.0, lo, -50.0, 100.0, hi, 150.0, n, 0, 0, 0, 0, 0, 0)[:0x2D] + struct.pack('<I', first)
    return head + struct.pack(f'<{len(values)}H', *values) + tail


def water_mesh(bodies: list[tuple[list[float], list[int]]]) -> bytes:
    out = struct.pack('<I', len(bodies))
    for v, i in bodies:
        out += struct.pack(f'<I{len(v)}f', len(v) // 3, *v) + struct.pack(f'<I{len(i)}I', len(i), *i)
    return out


def visual_water(indices: list[int], ranges: list[tuple[int, int, int, int]], slots: tuple[bytes, ...] = (b'A', b'B')) -> bytes:
    out = struct.pack('<4I', 10, 36, 4, 0) + struct.pack(f'<I{len(indices)}I', 4 * len(indices), *indices)
    out += struct.pack('<I', len(slots)) + b''.join(s + b'\0\0' for s in slots) + b'Systems/Water/WaterOceanFragment\0'
    out += struct.pack('<I', len(ranges)) + b''.join(struct.pack('<4I', *r) for r in ranges) + struct.pack('<I', 0) + b'F'
    return out


def mutations(data: bytes, rng: random.Random, n: int) -> list[bytes]:
    """Truncations at every length up to 200 bytes (then a sample), and random byte changes."""
    out = [data[:k] for k in range(min(len(data), 200))]
    out += [data[:rng.randrange(len(data))] for _ in range(n)]
    for _ in range(n):
        b = bytearray(data)
        for _ in range(rng.randint(1, 4)):
            b[rng.randrange(len(b))] = rng.randrange(256)
        out.append(bytes(b))
    return out


def quick_cases() -> list[tuple[str, str, bytes, bytes | None]]:
    """(label, kind, data, extra) of the quick tier, in the recording's order (seeded)."""
    rng = random.Random(1943)
    out: list[tuple[str, str, bytes, bytes | None]] = []
    files = {
        'terrain 4x4': heightfield(4, list(range(0, 65535, 4096))[:16]),
        'terrain 1x1': heightfield(1, [123]),
        'terrain flat': heightfield(2, [0, 65535, 32768, 1], lo=5.0, hi=5.0),
        'terrain 64 with quadtree': heightfield(64, [rng.randrange(65536) for _ in range(64 * 64)], bytes(4 * 21)),
        'terrain not a power of two': heightfield(3, [0] * 9),
        'terrain size 0': heightfield(0, []),
        'terrain too short': b'\0' * 48,
    }
    out += [(name, 'terrain', d, None) for name, d in files.items()]
    out += [('damaged height field', 'terrain', d, None) for d in mutations(files['terrain 64 with quadtree'], rng, 150)]
    mesh = water_mesh([([0, 1, 0, 1, 1, 0, 0, 1, 1], [0, 1, 2]), ([2, 1, 2, 3, 1, 2, 2, 1, 3, 9, 9, 9], [0, 1, 2, 1, 2, 3])])
    for name, v in {'matching': visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(1, 2, 3, 6), (0, 1, 0, 3)]),
                    'slot past the list, ranges past the end': visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(7, 99, 6, 3)]),
                    'other indices': visual_water([0, 1, 2], [(0, 1, 0, 3)]),
                    'non-ASCII slot': visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(0, 3, 0, 9)], (b'W\xe9', b'B')),
                    'damaged': visual_water([0, 1, 2], [(0, 1, 0, 3)])[:30]}.items():
        out += [(f'water {name}: shaders', 'shaders', mesh, v), (f'water {name}: visual', 'water visual', v, None)]
    out.append(('water mesh: two bodies joined', 'water mesh', mesh, None))
    out += [('damaged water mesh', 'water mesh', d, None) for d in mutations(mesh, rng, 150)]
    v = visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(1, 2, 3, 6)])
    for d in mutations(v, rng, 150):
        out += [('damaged water visual', 'water visual', d, None), ('damaged water visual: shaders', 'shaders', mesh, d)]
    out += [('havok: not a packfile', 'havok', b'not a packfile', None), ('havok: empty', 'havok', b'', None)]
    return out


def index_outcome(bases: list[str], strip: str) -> str:
    """collision.index's answer (strip: the folder its paths start with, left out)."""
    havok, aliases = collision.index(bases)
    return goldens.rh(([(k, os.path.relpath(v, strip)) for k, v in havok.items()],
                       type(aliases).__name__ if isinstance(aliases, Exception) else list(aliases.items())), 16)


def built_index(tmp: str) -> list[str]:
    """index, havok_index and blueprint_triangles on two folders built under tmp: partitions with and without a Mesh, a
    deleted marker whose XML is left over (MP_005's emitter_sw_meshparticle_tow_01), a text partition, a damaged one, a
    partition with no XML (not counted), packfiles in both folders; the Rust core's mesh-alias cache off, cold and
    warm (each the same answer)."""
    def mesh_file(ref: str | None, extra: int = 0) -> bytes:
        kids = [dbx.Element('field', dbx.TEXT, [('name', 'Other'), ('ref', 'objects/o/o_mesh/1')])]
        kids += [dbx.Element('field', dbx.TEXT, [('name', f'F{k}')], text=f'v{k}') for k in range(extra)]
        if ref is not None:
            kids.append(dbx.Element('field', dbx.TEXT, [('ref', ref), ('name', 'Mesh')]))
        inst = dbx.Element('instance', dbx.CONTAINER, [('guid', '1'), ('type', 'Physics.X')], children=kids)
        return dbx.write(dbx.DbxFile([], [dbx.Element('partition', dbx.CONTAINER, [('guid', '0')], children=[inst])]))

    def put(path: str, data: bytes) -> None:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'wb') as f:
            f.write(data)

    a, b = os.path.join(tmp, 'pkg'), os.path.join(tmp, 'dist')
    files = {
        'objects/wall/wall': mesh_file('Objects/InvisibleWall/InvWall_Mesh/12'),
        'objects/crate/Crate_02': mesh_file('Objects/Crate/Crate_Mesh/1', 3),
        'objects/crate/crate': mesh_file(None),
        'objects/null/n': mesh_file('null'),
        'objects/Mixed/Case': mesh_file('Objects/Mixed/MIXED_mesh/3'),
    }
    for base in (a, b):
        for rel, data in files.items():
            put(os.path.join(base, rel + '.dbx'), data)
            put(os.path.join(base, rel + '.xml'), dbx.to_xml(data).encode('utf-8'))
    put(os.path.join(a, 'objects', 'crate', 'Crate_Havok.res'), b'not a packfile')
    put(os.path.join(b, 'objects', 'invisiblewall', 'invwall_Havok.res'), b'')
    put(os.path.join(b, 'objects', 'door', 'door_Havok_Havok.res'), b'')
    put(os.path.join(b, 'fx', 'tow.dbx'), b'')                                             # deleted marker
    put(os.path.join(b, 'fx', 'tow.xml'), dbx.to_xml(mesh_file('Objects/Tow/Tow_Mesh/1')).encode('utf-8'))
    put(os.path.join(b, 'fx', 'text.dbx'), b'<partition/>')
    put(os.path.join(b, 'fx', 'text.xml'), dbx.to_xml(mesh_file('Objects/Text/Text_Mesh/1')).encode('utf-8'))
    damaged = bytearray(mesh_file('Objects/Bad/Bad_Mesh/1'))
    damaged[20:24] = b'\xff\xff\xff\xff'
    put(os.path.join(b, 'fx', 'damaged.dbx'), bytes(damaged))
    put(os.path.join(b, 'fx', 'damaged.xml'), dbx.to_xml(mesh_file('Objects/Kept/Kept_Mesh/1')).encode('utf-8'))
    put(os.path.join(b, 'fx', 'unconverted.dbx'), mesh_file('Objects/None/None_Mesh/1'))
    outcomes = []
    saved = collision.cache_folder()
    for folder in (None, os.path.join(tmp, 'cache'), os.path.join(tmp, 'cache')):
        collision.use_cache(folder)
        havok, aliases = collision.index([a, b])
        alone = collision.havok_index([a, b])
        tris = collision.blueprint_triangles(['objects/crate/crate', 'objects/wall/wall', 'objects/x'], havok)
        outcomes.append(goldens.rh(([(k, os.path.relpath(v, tmp)) for k, v in havok.items()],
                                    [(k, os.path.relpath(v, tmp)) for k, v in alone.items()],
                                    list(aliases.items()) if isinstance(aliases, dict) else type(aliases).__name__,
                                    [(k, _bits(v)) for k, v in tris.items()]), 16))
    collision.use_cache(saved)
    return outcomes


def level_bases(root: str, lv: str) -> list[str]:
    """The folders level.load reads a level from."""
    base = os.path.join(root, 'work', 'levels', lv)
    bases = [os.path.join(base, 'pkg'), os.path.join(base, 'dist')]
    return bases + [d for d in glob.glob(os.path.join(root, 'work', 'navkit', 'common', '*')) if os.path.isdir(d)]


_STAMPS: dict[str, Any] = {}


def inputs_stamp(bases: list[str], root: str) -> str:
    """What the index reads, as counts and sizes per folder (*.xml, *.dbx, *_havok.res): a level re-prepared since the
    recording has another stamp."""
    out = []
    for b in bases:
        if b not in _STAMPS:
            c: Counter[str] = Counter()
            for d, _, names in os.walk(b):
                for n in names:
                    low = n.lower()
                    k = 'havok' if low.endswith('_havok.res') else low.rsplit('.', 1)[-1] if low.endswith(('.xml', '.dbx')) else None
                    if k:
                        c[k] += 1
                        c[k + ' bytes'] += os.path.getsize(os.path.join(d, n))
            _STAMPS[b] = sorted(c.items())
        out.append((os.path.relpath(b, root), _STAMPS[b]))
    return goldens.rh(out, 16)


# ------------------------------------------------------------------------------------------------ the game copy

def corpus(root: str) -> list[str]:
    """Every bundle of the game copy."""
    return sorted(glob.glob(os.path.join(root, 'game', '**', '*.fbrb'), recursive=True))


def bundle_outcome(path: str, root: str) -> tuple[str, str | None, str, dict[str, int]]:
    """(bundle path relative to root, file SHA-1 or None without geometry, digest of every geometry entry's answer,
    entries by kind)."""
    rel = os.path.relpath(path, root).replace(os.sep, '/')
    counts: Counter[str] = Counter()
    try:
        ents = [e for e in fbrb.entries(path) if not e.deleted and e.type in TYPES]
        if not ents:
            return rel, None, '', {}
        sha = goldens.file_sha(path)[:16]
        data = fbrb.read(path, [e.name for e in ents])
    except (ValueError, KeyError, OSError) as e:
        return rel, None, f'not read: {type(e).__name__}', {}
    rows = []
    for ent in ents:
        kind = TYPES[ent.type]
        d = data[ent.name]
        counts[kind] += 1
        rows.append((ent.name, kind, outcome(kind, d)))
        if kind == 'water mesh':
            visual = ent.name[:-len('.Mesh.res')] + '.res'
            if visual in data:
                counts['shaders'] += 1
                rows.append((ent.name, 'shaders', outcome('shaders', d, data[visual])))
    return rel, sha, goldens.rh(rows, 16), dict(counts)


def _bundle_job(job: tuple[str, str]) -> tuple[str, str | None, str, dict[str, int]]:
    return bundle_outcome(*job)


def mp005_packfiles(root: str) -> tuple[str, list[bytes]] | None:
    """(MP_005's level bundle's SHA-1, three of its packfiles), or None without the game copy."""
    sample = next((b for b in corpus(root) if os.path.basename(os.path.dirname(b)).lower() == 'mp_005'
                   and b.endswith('level-00.fbrb')), None)
    if not sample:
        return None
    names = [e.name for e in fbrb.entries(sample) if e.type == 'HavokPhysicsData' and not e.deleted][:3]
    pk = fbrb.read(sample, names)
    return goldens.file_sha(sample)[:16], [pk[n] for n in names]


def damaged_packfiles(packs: list[bytes]) -> list[bytes]:
    rng = random.Random(1943)
    return [d for p in packs for d in mutations(p, rng, 60)]


def prepared_levels(root: str) -> list[str]:
    return sorted(d for d in os.listdir(os.path.join(root, 'work', 'levels'))
                  if os.path.isdir(os.path.join(root, 'work', 'levels', d, 'dist')))


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    native.require('the geometry readers', 'collision_index')
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f': {detail}' if detail and not ok else ''))

    rec = goldens.load(GOLDEN)
    cases = quick_cases()
    got = [outcome(kind, d, extra) for _, kind, d, extra in cases]
    bad = [f'{cases[i][0]} #{i}' for i, (a, b) in enumerate(zip(got, rec['cases'])) if a != b]
    kinds = Counter(f'{k} {"refused" if g == "ValueError" else "read"}' for (_, k, _, _), g in zip(cases, got))
    check(len(got) == len(rec['cases']) and not bad,
          f'{len(cases)} built and damaged height fields, water meshes, water visuals and packfiles: the heights, '
          f'vertices, triangles, shaders and refusals the Python readers gave ({", ".join(f"{n} {k}" for k, n in sorted(kinds.items()))})',
          '; '.join(bad[:4]))
    hf = terrain.parse(cases[3][2])
    (kind, *_), words = terrain.tail(cases[3][2])
    check(kind == 'quadtree' and hf.size == 64 and words[6] == 0, 'height field tail: a sub-terrain quadtree')

    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        outs = built_index(tmp)
        kept = [f for f in os.listdir(os.path.join(tmp, 'cache')) if not f.startswith('.')] if os.path.isdir(os.path.join(tmp, 'cache')) else []
        check(outs[0] == rec['index_built'] and len(set(outs)) == 1 and len(kept) == 2,
              'collision index on built folders (binary partitions and XML; the mesh-alias cache off, cold and warm): '
              'the Python index\'s tables', f'{outs} / {rec["index_built"]}, {len(kept)} cache files')

    try:
        root = paths.find_opfor(None, start=_ROOT)
    except Exception:
        root = None
    if root is None or not os.path.isdir(os.path.join(root, 'game')):
        print('game copy not found: the game corpus is skipped')
    else:
        mp = mp005_packfiles(root)
        if mp is not None:
            mp_sha, packs = mp
            if mp_sha != rec['mp005']['sha']:
                print('skipped: damaged MP_005 packfiles (its bundle changed since the recording)')
            else:
                g = [havok_outcome(d) for d in damaged_packfiles(packs)]
                bad_at = [i for i, (a, b) in enumerate(zip(g, rec['mp005']['cases'])) if a != b]
                check(not bad_at and len(g) == len(rec['mp005']['cases']),
                      f'{len(g)} damaged Havok packfiles (three of MP_005\'s): the triangles and refusals NavKit\'s '
                      'reader gave', f'first: {bad_at[:5]}')
        if '--game' in argv:
            t0 = time.time()
            bundles = corpus(root)
            counts: Counter[str] = Counter()
            differ: list[str] = []
            changed = 0
            with ProcessPoolExecutor(max(1, min(3, (os.cpu_count() or 2) // 2))) as ex:
                for rel, sha, d, c in ex.map(_bundle_job, [(b, root) for b in bundles], chunksize=4):
                    if sha is None and not d:
                        continue
                    counts.update(c)
                    want = rec['game'].get(rel)
                    if want is None or want[0] != sha:
                        changed += 1
                    elif want[1] != d:
                        differ.append(rel)
            detail = ', '.join(f'{v} {k}' for k, v in sorted(counts.items()))
            check(not differ, f'game copy, {len(bundles)} bundles: {detail} - the Python readers\' answers '
                  f'({changed} bundles changed since the recording; {time.time() - t0:.0f} s)', '; '.join(differ[:5]))
            t0 = time.time()
            differ, changed = [], 0
            levels = prepared_levels(root)
            for lv in levels:
                bases = level_bases(root, lv)
                want = rec['index'].get(lv)
                if want is None or want[0] != inputs_stamp(bases, root):
                    changed += 1
                elif want[1] != index_outcome(bases, root):
                    differ.append(lv)
            check(not differ, f'collision index of {len(levels)} prepared levels: the Python index\'s tables, keys and '
                  f'values in the same order ({changed} re-prepared since the recording; {time.time() - t0:.0f} s)',
                  ', '.join(differ))
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
