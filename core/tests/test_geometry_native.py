r"""The Rust geometry readers (rust/bc2-formats terrain, water, havok and rust/bc2-scene collision, through the
bc2_native module) against the Python readers they were ported from (core/bfbc2core/terrain.py, water.py, NavKit's
levelobjects.py / hkpack.py): the same heights bit for bit, the same vertices, triangles and shaders, the same files
refused, the same collision index.

  python core\tests\test_geometry_native.py           quick: built and damaged files, the dispatchers and fallback
  python core\tests\test_geometry_native.py --game    + every height field, water file and Havok packfile in every
                                                       bundle of the game copy (Project OPFOR BFBC2's game\, read
                                                       only) and the collision index of every prepared level
                                                       (work\levels, read only)

Skipped (exit 0) when the module is not built (python rust\build_native.py).
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

suite_paths.use('core', 'navkit-tools')
import hkpack  # noqa: E402
from bfbc2core import collision, dbx, fbrb, paths, terrain, water  # noqa: E402

NATIVE: Any = collision._with_collision(dbx._import_native())
TYPES = {'TerrainHeightfield': 'terrain', 'WaterMesh': 'water mesh', 'VisualWater': 'water visual',
         'HavokPhysicsData': 'havok'}


# ------------------------------------------------------------------------------------------------ one file, both readers

def _bits(a: np.ndarray | None) -> Any:
    """An array's shape, dtype and bytes (bit for bit: NaN payloads and signed zeros count), or None."""
    return None if a is None else (a.shape, a.dtype.str, a.tobytes())


def _outcome(fn: Callable[[], Any]) -> tuple[str, Any]:
    try:
        return 'ok', fn()
    except ValueError:
        return 'ValueError', None


def terrain_outcome(data: bytes, native: bool) -> tuple[str, Any]:
    terrain.use_native(native)

    def run() -> Any:
        hf = terrain.parse(data)
        return (hf.min.tolist(), hf.max.tolist(), hf.size, _bits(hf.raw), _bits(hf.heights))
    return _outcome(run)


def mesh_outcome(data: bytes, native: bool) -> tuple[str, Any]:
    water.use_native(native)
    return _outcome(lambda: tuple(_bits(a) for a in water.parse_mesh(data)))


def visual_outcome(data: bytes, native: bool) -> tuple[str, Any]:
    water.use_native(native)

    def run() -> Any:
        idx, slots, ranges = water.parse_visual(data)
        return _bits(idx), slots, [tuple(r) for r in ranges]
    return _outcome(run)


def shaders_outcome(mesh: bytes, visual: bytes, native: bool) -> tuple[str, Any]:
    water.use_native(native)

    def run() -> Any:
        _, t = water.parse_mesh(mesh)
        return water.shaders_of(mesh, visual, t)
    return _outcome(run)


def havok_outcome(data: bytes, native: bool) -> tuple[str, Any]:
    """levelobjects.triangles on the bytes (hkpack.load pointed at them) or the Rust reader."""
    if native:
        return _outcome(lambda: _bits(collision._array(NATIVE.havok_triangles(data))))
    load = hkpack.load
    hkpack.load = lambda _path: hkpack.Packfile(data)
    try:
        return _outcome(lambda: _bits(collision._py_triangles('<bytes>')))
    finally:
        hkpack.load = load


def compare(kind: str, data: bytes, extra: bytes | None = None) -> str | None:
    """None when both readers agree on data (a mismatch described otherwise)."""
    if kind == 'shaders':
        assert extra is not None
        a, b = shaders_outcome(data, extra, False), shaders_outcome(data, extra, True)
    else:
        f = {'terrain': terrain_outcome, 'water mesh': mesh_outcome, 'water visual': visual_outcome,
             'havok': havok_outcome}[kind]
        a, b = f(data, False), f(data, True)
    if a == b:
        return None
    return f'python {a[0]}, rust {b[0]}' if a[0] != b[0] else 'different results'


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


# ------------------------------------------------------------------------------------------------ the game copy

def corpus(root: str) -> list[str]:
    """Every bundle of the game copy."""
    return sorted(glob.glob(os.path.join(root, 'game', '**', '*.fbrb'), recursive=True))


def check_bundle(path: str) -> tuple[str, Counter[str], list[str], Counter[str]]:
    """Both readers on every geometry entry of one bundle: (bundle, files compared by kind, mismatches, tails)."""
    counts: Counter[str] = Counter()
    tails: Counter[str] = Counter()
    bad: list[str] = []
    try:
        ents = [e for e in fbrb.entries(path) if not e.deleted and e.type in TYPES]
        if not ents:
            return path, counts, bad, tails
        data = fbrb.read(path, [e.name for e in ents])
    except (ValueError, KeyError, OSError) as e:
        return path, counts, [f'{path}: not read ({e})'], tails
    for ent in ents:
        kind = TYPES[ent.type]
        d = data[ent.name]
        counts[kind] += 1
        why = compare(kind, d)
        if why:
            bad.append(f'{path}: {ent.name}: {why}')
        if kind == 'terrain' and why is None and NATIVE is not None:
            try:
                tails['heightfield ' + str(NATIVE.terrain_tail(d)[0][0])] += 1
            except ValueError:
                tails['heightfield refused'] += 1
        if kind == 'water mesh':
            visual = ent.name[:-len('.Mesh.res')] + '.res'
            if visual in data:
                counts['shaders'] += 1
                why = compare('shaders', d, data[visual])
                if why:
                    bad.append(f'{path}: {ent.name} shaders: {why}')
    return path, counts, bad, tails


def index_outcome(bases: list[str], native: bool) -> Any:
    collision.use_native(native)
    havok, aliases = collision.index(bases)
    return list(havok.items()), (type(aliases).__name__ if isinstance(aliases, Exception) else list(aliases.items()))


def level_bases(root: str, lv: str) -> list[str]:
    """The folders level.load reads a level from."""
    base = os.path.join(root, 'work', 'levels', lv)
    bases = [os.path.join(base, 'pkg'), os.path.join(base, 'dist')]
    return bases + [d for d in glob.glob(os.path.join(root, 'work', 'navkit', 'common', '*')) if os.path.isdir(d)]


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f': {detail}' if detail and not ok else ''))

    rng = random.Random(1943)
    # height fields: a few sizes, extreme values, a sub-terrain with its quadtree, refused ones, damaged copies
    files = {
        'terrain 4x4': heightfield(4, list(range(0, 65535, 4096))[:16]),
        'terrain 1x1': heightfield(1, [123]),
        'terrain flat': heightfield(2, [0, 65535, 32768, 1], lo=5.0, hi=5.0),
        'terrain 64 with quadtree': heightfield(64, [rng.randrange(65536) for _ in range(64 * 64)], bytes(4 * 21)),
        'terrain not a power of two': heightfield(3, [0] * 9),
        'terrain size 0': heightfield(0, []),
        'terrain too short': b'\0' * 48,
    }
    for name, d in files.items():
        why = compare('terrain', d)
        check(why is None, f'built: {name}', why or '')
    bad = [why for d in mutations(files['terrain 64 with quadtree'], rng, 150) if (why := compare('terrain', d))]
    check(not bad, 'damaged height fields: the same refused, the same heights', '; '.join(bad[:3]))
    terrain.use_native(True)
    hf = terrain.parse(files['terrain 64 with quadtree'])
    (kind, *_), words = terrain.tail(files['terrain 64 with quadtree'])
    check(kind == 'quadtree' and hf.size == 64 and words[6] == 0, 'height field tail: a sub-terrain quadtree')
    # water: two bodies, shaders in and out of range, damaged copies
    mesh = water_mesh([([0, 1, 0, 1, 1, 0, 0, 1, 1], [0, 1, 2]), ([2, 1, 2, 3, 1, 2, 2, 1, 3, 9, 9, 9], [0, 1, 2, 1, 2, 3])])
    for name, v in {'matching': visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(1, 2, 3, 6), (0, 1, 0, 3)]),
                    'slot past the list, ranges past the end': visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(7, 99, 6, 3)]),
                    'other indices': visual_water([0, 1, 2], [(0, 1, 0, 3)]),
                    'non-ASCII slot': visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(0, 3, 0, 9)], (b'W\xe9', b'B')),
                    'damaged': visual_water([0, 1, 2], [(0, 1, 0, 3)])[:30]}.items():
        why = compare('shaders', mesh, v) or compare('water visual', v)
        check(why is None, f'built water: {name}', why or '')
    check(compare('water mesh', mesh) is None, 'built water mesh: two bodies joined')
    bad = [why for d in mutations(mesh, rng, 150) if (why := compare('water mesh', d))]
    v = visual_water([0, 1, 2, 3, 4, 5, 4, 5, 6], [(1, 2, 3, 6)])
    bad += [why for d in mutations(v, rng, 150) if (why := compare('water visual', d) or compare('shaders', mesh, d))]
    check(not bad, 'damaged water files: the same refused, the same results', '; '.join(bad[:3]))
    # Havok: not a packfile, damaged copies of a game packfile (when the game copy is there)
    check(compare('havok', b'not a packfile') is None and compare('havok', b'') is None, 'havok: not a packfile')
    # the dispatchers: backends switch and BFBC2_PURE_PYTHON is honoured by each module
    for m in (terrain, water, collision):
        check(m.use_native(False) == 'python' and m.use_native(True) == 'rust', f'{m.__name__}: use_native switches')
    env = dict(os.environ, BFBC2_PURE_PYTHON='1')
    import subprocess
    r = subprocess.run([sys.executable, '-c', 'import sys; sys.path.insert(0, sys.argv[1]); '
                        'from bfbc2core import terrain, water, collision; '
                        'print(terrain.backend(), water.backend(), collision.backend())', os.path.join(_ROOT, 'core')],
                       env=env, capture_output=True, text=True)
    check(r.stdout.split() == ['python'] * 3, 'BFBC2_PURE_PYTHON=1 forces the Python readers', r.stdout + r.stderr)

    try:
        root = paths.find_opfor(None, start=_ROOT)
    except Exception:
        root = None
    if root is None or not os.path.isdir(os.path.join(root, 'game')):
        print('game copy not found: the game corpus is skipped')
    else:
        # damaged copies of one real packfile
        sample = next((b for b in corpus(root) if os.path.basename(os.path.dirname(b)).lower() == 'mp_005'
                       and b.endswith('level-00.fbrb')), None)
        if sample:
            names = [e.name for e in fbrb.entries(sample) if e.type == 'HavokPhysicsData' and not e.deleted][:3]
            pk = fbrb.read(sample, names)
            bad = [why for n in names for d in mutations(pk[n], rng, 60) if (why := compare('havok', d))]
            check(not bad, f'damaged Havok packfiles ({len(names)} of MP_005): the same refused, the same triangles',
                  '; '.join(bad[:3]))
        if '--game' in argv:
            t0 = time.time()
            bundles = corpus(root)
            counts: Counter[str] = Counter()
            tails: Counter[str] = Counter()
            bad = []
            with ProcessPoolExecutor(max(1, min(8, (os.cpu_count() or 2) // 2))) as ex:
                for _, c, b, t in ex.map(check_bundle, bundles, chunksize=4):
                    counts.update(c)
                    tails.update(t)
                    bad += b
            detail = ', '.join(f'{v} {k}' for k, v in sorted(counts.items()))
            check(not bad, f'game copy, {len(bundles)} bundles: {detail} ({time.time() - t0:.0f} s)', '; '.join(bad[:5]))
            print(f'     tails: {dict(tails)}')
            # the collision index of every prepared level: both readers, the same dictionaries in the same order
            t0 = time.time()
            levels = sorted(d for d in os.listdir(os.path.join(root, 'work', 'levels'))
                            if os.path.isdir(os.path.join(root, 'work', 'levels', d, 'dist')))
            bad = []
            for lv in levels:
                bases = level_bases(root, lv)
                if index_outcome(bases, False) != index_outcome(bases, True):
                    bad.append(lv)
            collision.use_native(True)
            check(not bad, f'collision index of {len(levels)} prepared levels ({time.time() - t0:.0f} s)', ', '.join(bad))
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
