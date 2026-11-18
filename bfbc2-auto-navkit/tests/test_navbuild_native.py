r"""NavKit in the Rust core (rust/bc2-navbuild through bc2_native).

The grid walks (round E1d: Level.neighbour's linking, components, trace_edges' side neighbours, line_walkable,
build_graph's route-group search; navwalk's flood, islands_of and matched_flags) had a Python twin that was their
reference until 2026-11-17, when the port had passed the full test tier (140/140) and the twin was retired
(rust/BENCHMARKS.md, "Retired Python"). Their results were recorded first, from the Python code itself, in
navbuild_golden.json beside this file: a SHA-1 of every walk's answers on seeded random levels, and of whole levels'
path data (the build clock fixed). They are the oracle now; navregress (DICE's own path data) stays the independent
check of what NavKit builds.

The walkability rules, build_areas and build_graph's cell picking (round E2a), and the tracing, the roof rule, the
cover nodes, Gabriel pruning, build_graph's route edges and group joining, near_characters, walkable's seed picks and
the sample files' records (round E3c) keep their Python twin: they are still compared with it, call by call (E3c's
with the Rust side strict: no fallback).

  python test_navbuild_native.py            quick: the grid walks on 24 + 16 seeded random multi-layer levels
                                            (float32 and float64 gradients, stairs, terrain, every link rule, D
                                            sides) against the recording; arrays of other types cast as the core
                                            takes them; E2a's rules, build_areas and cell picking on both backends,
                                            and CPython's float functions they need (math.hypot, x ** 2)
  python test_navbuild_native.py --game [--levels sp_009,sp_004_b]
                                            + whole builds (navregress.build_level, Project OPFOR BFBC2's in-game
                                            samples, read only) into a temporary folder: the path data's SHA-1
                                            against the recording (and the inputs' SHA-1: a changed input is
                                            reported as such, not as a regression)
  python test_navbuild_native.py --record [--game --levels ...]
                                            write navbuild_golden.json from the current code: only after a
                                            deliberate change to NavKit's walks or builds, with navregress showing
                                            what it did to the parity with DICE's path data

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import hashlib
import json
import math
import os
import random
import struct
import sys
import tempfile
import time
from dataclasses import replace
from types import SimpleNamespace
from typing import Any, cast

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import navbuild as nb  # noqa: E402
import navwalk  # noqa: E402

GOLDEN = os.path.join(HERE, 'navbuild_golden.json')
# the moment every sector of a recorded build says it was built (navbuild.write_file: time.localtime())
FIXED_CLOCK = time.struct_time((2026, 11, 17, 12, 0, 0, 1, 321, 0))
FAILS: list[str] = []


def check(cond: bool, msg: str) -> None:
    if not cond:
        FAILS.append(msg)
        print('FAIL', msg)


def py(fn: Any, *a: Any, **k: Any) -> Any:
    """fn with E2a's pieces on their Python reference (the grid walks have none: they run in Rust either way)."""
    nb.use_native(False)
    try:
        return fn(*a, **k)
    finally:
        nb.use_native(True)


# ------------------------------------------------------------------------------------------------ recorded answers

def canon(v: Any) -> Any:
    """v as plain JSON values, the same for equal answers whatever their Python or NumPy types: dicts as their items
    sorted, sequences as lists, floats by repr (exact), arrays as dtype, shape and the SHA-1 of their bytes."""
    if isinstance(v, np.ndarray):
        return ['ndarray', v.dtype.str, list(v.shape), hashlib.sha1(np.ascontiguousarray(v).tobytes()).hexdigest()]
    if isinstance(v, np.generic):
        v = v.item()
    if isinstance(v, bool) or v is None or isinstance(v, (int, str)):
        return v
    if isinstance(v, float):
        return ['f', repr(v)]
    if isinstance(v, dict):
        items = [[canon(k), canon(x)] for k, x in v.items()]
        return ['dict', sorted(items, key=lambda kv: json.dumps(kv[0]))]
    if isinstance(v, (list, tuple, range)):
        return [canon(x) for x in v]
    if isinstance(v, (set, frozenset)):
        return ['set', sorted((canon(x) for x in v), key=json.dumps)]
    if hasattr(v, '__dataclass_fields__'):
        return [type(v).__name__] + [canon(getattr(v, f)) for f in v.__dataclass_fields__]
    raise TypeError(f'canon: {type(v).__name__}')


def digest(v: Any) -> str:
    return hashlib.sha1(json.dumps(canon(v), separators=(',', ':')).encode()).hexdigest()


def grid(rng: random.Random, nx: int, nz: int, f32: bool, pitch: float = 0.8) -> nb.Level:
    """A random level: 0-3 surfaces per column on a few floors, rough heights, random keep / stair / terrain."""
    start, ys, nrm, hit = [0], [], [], []
    for _j in range(nz):
        for _i in range(nx):
            floors = rng.sample([0.0, 2.5, 5.0], rng.choice([0, 1, 1, 1, 2, 3]))
            for f in sorted(floors):
                ys.append(f + rng.choice([0.0, 0.1, 0.25, 0.3, 0.55, 0.62, 0.9]) * rng.random())
                n = np.array([rng.uniform(-0.6, 0.6), 1.0, rng.uniform(-0.6, 0.6)])
                nrm.append(n / np.linalg.norm(n))
                hit.append((navwalk.TERRAIN_SHAPE_VTABLE if rng.random() < 0.7 else 1, 0, 0))
            start.append(len(ys))
    count = len(ys)
    s: Any = SimpleNamespace(nx=nx, nz=nz, pitch=pitch, min_x=0.0, min_z=0.0, start=np.array(start, np.int64),
                             y=np.array(ys, np.float32), normal=np.array(nrm, np.float32).reshape(-1, 3),
                             hit=np.array(hit, np.uint32).reshape(-1, 3),
                             flags=np.array([rng.random() < 0.1 for _ in range(count)], np.uint8))
    keep = np.array([rng.random() < 0.85 for _ in range(count)], bool)
    stair = np.array([rng.random() < 0.1 for _ in range(count)], bool)
    if f32:
        return nb.Level(s, keep, 0.28, stair=stair, cfg=nb.DEFAULTS.at_pitch(pitch))
    gx = np.array([rng.uniform(-0.8, 0.8) for _ in range(count)], np.float64)
    gz = np.array([rng.uniform(-0.8, 0.8) for _ in range(count)], np.float64)
    return nb.Level(s, keep, 0.28, gx, gz, stair, cfg=nb.DEFAULTS.at_pitch(pitch))


def search(lv: nb.Level, starts: list[int], other: set[int], cells: dict[int, tuple[int, int]]) -> Any:
    """build_graph's route-group search (NavLevel.search) over the level's window."""
    window = lv.cfg.window
    return lv.native().search(starts, other, cells, (window.start, window.stop), 0, 0, nb.LINK_RULE)


def walks(rng: random.Random) -> dict[str, Any]:
    """Every grid walk's answers on the seeded random levels, as digests per level and walk."""
    out: dict[str, Any] = {'levels': [], 'walkable': []}
    dirs = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1), (2, 1), (0, 0)]
    for case in range(24):
        f32 = case % 2 == 0
        pitch = 0.8 if case % 3 else 0.2
        lv = grid(rng, rng.randint(3, 9), rng.randint(3, 9), f32, pitch)
        s = lv.s
        col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
        got: dict[str, Any] = {}
        # neighbour: every surface, direction and rule, the column its own or another
        ns = []
        for k in range(len(s.y)):
            c = int(col[k])
            for gi, gj in ((c % s.nx, c // s.nx), (rng.randint(-1, s.nx), rng.randint(-1, s.nz))):
                for di, dj in dirs:
                    for rule in (None, 'slope', 'raw', 'terrain'):
                        ns.append(lv.neighbour(k, gi, gj, di, dj, rule=rule))
        got['neighbour'] = ns
        # areas (components), traced sides, lines and searches over each sector-like window
        areas = nb.build_areas(lv, 0, 0, cfg=lv.cfg)
        got['build_areas'] = areas
        members = {k: (int(col[k] % s.nx), int(col[k] // s.nx)) for k in range(len(s.y)) if lv.keep[k]}
        got['components'] = nb.components(lv, members, 0, 0)
        area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
        got['trace_edges'] = nb.trace_edges(lv, areas, area_of, 0, 0, lv.cfg)
        kept = [k for k in range(len(s.y)) if lv.keep[k]]
        if kept:
            cell = {k: (int(col[k] % s.nx), int(col[k] // s.nx)) for k in kept}
            lines = []
            for blocked in (set(), {(rng.choice(kept), rng.choice(kept)) for _ in range(len(kept) // 2)}):
                lv.blocked = blocked
                for _ in range(60):
                    k0, k1 = rng.choice(kept), rng.choice(kept)
                    pa, pb = cell[k0], (rng.randint(-1, s.nx), rng.randint(-1, s.nz))
                    lines.append((k0, pa, pb, k1, nb.line_walkable(lv, k0, pa, pb, 0, 0, k1)))
            got['line_walkable'] = lines
            # the search, as build_graph runs it: groups of nodes on surfaces, union-find, cell map
            nodes = rng.sample(kept, min(len(kept), 6))
            node_at = {k: i for i, k in enumerate(nodes)}
            parent = [0, 0, 2, 2, 4, 5][:len(nodes)]

            def find(x: int, parent: list[int] = parent) -> int:
                while parent[x] != x:
                    x = parent[x]
                return x
            found = []
            for root in sorted({find(i) for i in range(len(nodes))}):
                group = [i for i in range(len(nodes)) if find(i) == root]
                other = {k for k, at in node_at.items() if find(at) != root}
                cells = {nodes[m]: cell[nodes[m]] for m in group}
                found.append((search(lv, [nodes[m] for m in group], other, cells), cells))
            got['search'] = found
        out['levels'].append({k: digest(v) for k, v in got.items()})
    # walkability (navwalk.walkable: its flood, roof islands), islands_of and matched_flags, float32 and float64
    for case in range(16):
        lv = grid(rng, rng.randint(4, 12), rng.randint(4, 12), case % 2 == 0)
        s = lv.s
        seeds = [(rng.uniform(0, s.nx * s.pitch), rng.uniform(0, 3), rng.uniform(0, s.nz * s.pitch)) for _ in range(3)]
        # a second (high-capsule) run over the same grid: the same surfaces, heights moved by up to 0.08 m, its flags
        hi: Any = SimpleNamespace(**s.__dict__)
        hi.y = (s.y + np.array([rng.choice([0.0, 0.02, -0.049, 0.06, 0.08]) for _ in s.y], np.float32)).astype(np.float32)
        hi.flags = np.array([rng.randrange(4) for _ in s.y], np.uint8)
        got = {'matched_flags': navwalk.matched_flags(s, hi)}
        a = navwalk.walkable(s, return_gradients=True, return_stairs=True, extra_seeds=seeds, anchors=seeds[:1],
                             high_capsule=hi)
        got['walkable'] = a
        got['islands_of'] = [navwalk.islands_of(s, keep, lv.gx, lv.gz, lv.stair, 0.28) for keep in (lv.keep, a[0])]
        out['walkable'].append({k: digest(v) for k, v in got.items()})
    return out


def load() -> dict[str, Any]:
    try:
        with open(GOLDEN, encoding='utf-8') as f:
            return dict(json.load(f))
    except FileNotFoundError:
        return {}


def compare(name: str, got: Any, want: Any) -> int:
    """Differences between got and the recorded want (lists of {walk: digest}), each reported."""
    bad = 0
    if len(got) != len(want):
        check(False, f'{name}: {len(got)} cases, {len(want)} recorded')
        return 1
    for case, (g, w) in enumerate(zip(got, want)):
        for walk in sorted(set(g) | set(w)):
            if g.get(walk) != w.get(walk):
                bad += 1
                check(False, f'{name} case {case}: {walk} differs from the recording')
    return bad


TWINS: dict[str, list[str]] = {}            # this run's digests per section (E2a / E3c / E3h)
WANT: dict[str, list[str]] = {}             # the recording's
MODE = {'record': False}


def twin(section: str, name: str, got: Any, want: Any = None) -> None:
    """got (the Rust core's answer) against the retired Python's, recorded as a digest in navbuild_golden.json
    ('retired'): recording, want() is the Python's answer, digested (and got must give the same digest)."""
    n = len(TWINS.setdefault(section, []))
    if MODE['record']:
        w = digest(want())
        check(digest(got) == w, f'{name}: the Rust core differs from its Python twin')
        TWINS[section].append(w)
        return
    g = digest(got)
    TWINS[section].append(g)
    rec = WANT.get(section, [])
    check(n < len(rec) and rec[n] == g, f'{name}: differs from the recorded answer of the retired Python'
          if n < len(rec) else f'{name}: not recorded ({section} #{n})')


def quick(golden: dict[str, Any], record: bool) -> None:
    nb.require_native()
    MODE['record'] = record
    WANT.update(golden.get('retired', {}))
    rng = random.Random(943)
    got = walks(rng)
    if record:
        golden['quick'] = got
        print(f'{len(got["levels"])} + {len(got["walkable"])} random levels recorded')
    else:
        want = golden.get('quick', {})
        bad = compare('grid walks', got['levels'], want.get('levels', [])) + \
            compare('walkable', got['walkable'], want.get('walkable', []))
        print(f'grid walks on {len(got["levels"])} + {len(got["walkable"])} random levels: {bad} differ from the '
              f'recording')
    others(rng)
    calls = float_ops(rng) + rules(rng) + areas_and_picks(rng)
    print(f'E2a: {calls} calls compared with their Python twin: {len(FAILS)} differences')
    before = len(FAILS)
    calls = e3c(rng)
    print(f'E3c: {calls} calls compared with their Python twin: {len(FAILS) - before} differences')
    before = len(FAILS)
    calls = e3h(random.Random(7731))
    print(f'E3h: {calls} calls compared with their Python twin: {len(FAILS) - before} differences')
    if record:
        golden['retired'] = TWINS
    for sec, got in TWINS.items():
        check(len(got) == len(WANT.get(sec, got)), f'{sec}: {len(got)} answers, {len(WANT.get(sec, []))} recorded')
    print('answers recorded from the retired Python: ' + ', '.join(f'{k} {len(v)}' for k, v in sorted(TWINS.items())))


def others(rng: random.Random) -> None:
    """Arrays of other types are cast as the core takes them (the grid walks have no Python to fall back on): the
    same answers as the cast arrays give; E2a's switch leaves the walks running; a surface outside the level."""
    lv = grid(rng, 4, 4, True)
    check(nb.use_native(False) == 'python' and lv.native() is not None, 'use_native(False): the walks still run')
    nb.use_native(True)
    first = int(np.nonzero(lv.keep)[0][0])
    c = int(np.searchsorted(lv.s.start, first, side='right')) - 1          # its column
    try:
        lv.neighbour(10 ** 6, c % lv.s.nx - 1, c // lv.s.nx, 1, 0)
        check(False, 'a surface outside the level must raise')
    except ValueError:
        pass
    check(cast(Any, lv).neighbour(np.int64(0), np.int64(0), np.int64(0), 1, 0) == lv.neighbour(0, 0, 0, 1, 0),
          'numpy integers')
    odd: Any = grid(rng, 5, 5, False)
    ref: Any = nb.Level(odd.s, odd.keep, 0.28, odd.gx.astype(np.float16).astype(np.float64), odd.gz, odd.stair,
                        cfg=odd.cfg)
    odd.gx = odd.gx.astype(np.float16)                                     # widened to float64
    odd.keep = odd.keep.astype(np.uint8)                                   # a value's truth
    check(odd.native() is not None, 'float16 gradients, uint8 keep: a native level')
    s = odd.s
    for k in range(len(s.y)):
        for di, dj in ((1, 0), (1, 1), (0, -1)):
            gi, gj = int(np.searchsorted(s.start, k, side='right')) - 1, 0
            gi, gj = gi % s.nx, gi // s.nx
            check(odd.neighbour(k, gi, gj, di, dj) == ref.neighbour(k, gi, gj, di, dj), f'cast arrays: neighbour {k}')
    a = navwalk.islands_of(s, odd.keep, odd.gx, odd.gz, odd.stair.astype(np.int8), 0.28)
    b = navwalk.islands_of(s, ref.keep, ref.gx, ref.gz, ref.stair, 0.28)
    check(a[1] == b[1] and np.array_equal(a[0], b[0]), 'cast arrays: islands_of')
    hi: Any = SimpleNamespace(**s.__dict__)
    hi.flags = s.flags.astype(np.int32)
    check(np.array_equal(navwalk.matched_flags(s, hi), navwalk.matched_flags(s, s)), 'matched_flags: int32 flags cast')


def same(a: Any, b: Any) -> bool:
    """The same array bit for bit (dtype, shape, bytes), or the same tuple of such arrays."""
    if isinstance(a, tuple):
        return isinstance(b, tuple) and len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    return a.dtype == b.dtype and a.shape == b.shape and a.tobytes() == b.tobytes()


def float_ops(rng: random.Random) -> int:
    """CPython's math.hypot, x ** 2 and degrees(atan(hypot)), and NumPy's np.hypot (E3c's coverage nodes), as the port
    computes them, on gradients like the rules' (height differences over a pitch) and on values over the whole float
    range."""
    import math
    xs, zs = [], []
    for n in range(150_000):
        if n % 3 == 0:
            xs.append(rng.randint(-60, 60) * 0.05 / rng.choice([0.2, 0.4, 0.8, 1.6]))
            zs.append(rng.uniform(-3, 3) / rng.choice([0.2, 0.4]))
        elif n % 3 == 1:
            xs.append(rng.uniform(-1, 1) * 2.0 ** rng.randint(-1074, 1023))
            zs.append(rng.uniform(-1, 1) * 2.0 ** rng.randint(-1074, 1023))
        else:
            xs.append(float(np.float32(rng.uniform(-500, 500))) - rng.uniform(-500, 500))
            zs.append(rng.choice([0.0, -0.0, xs[-1], -xs[-1], rng.uniform(-1e-300, 1e-300)]))
    xs += [0.0, -0.0, math.inf, -math.inf, math.nan, 1e308, 5e-324]
    zs += [0.0, 3.0, math.nan, 1.0, math.inf, 1e308, 5e-324]
    h, s, d, ch = (np.frombuffer(b, '<f8') for b in navwalk.require_native().navkit_float_ops(
        np.array(xs, '<f8').tobytes(), np.array(zs, '<f8').tobytes()))
    np_hyp = np.hypot(np.array(xs, np.float64), np.array(zs, np.float64))     # NumPy's: the C library's hypot
    bad = 0
    for k, (x, z) in enumerate(zip(xs, zs)):
        try:
            sq = x ** 2
        except OverflowError:                                   # CPython raises where pow overflows
            sq = math.inf
        want = (math.hypot(x, z), sq, math.degrees(math.atan(math.hypot(x, z))), float(np_hyp[k]))
        got = (float(h[k]), float(s[k]), float(d[k]), float(ch[k]))
        if any(not (a == b or (a != a and b != b)) for a, b in zip(want, got)):
            bad += 1
            if bad <= 5:
                check(False, f'float ops ({x!r}, {z!r}): {got} != {want}')
    check(bad == 0, f'float ops: {bad} of {len(xs)} differ')
    # gabriel_prune's port squares array differences as x * x: an array's ** 2 is NumPy's square, a multiply
    a = np.array(xs, np.float64)
    with np.errstate(over='ignore', invalid='ignore'):
        check((a ** 2).tobytes() == (a * a).tobytes(), "float ops: an array's ** 2 is not x * x")
    return len(xs)


def runs(rng: random.Random, nx: int, nz: int, pitch: float, f64_normals: bool = False) -> Any:
    """A random sample run: 0-3 surfaces per column on a few floors with steps and steep pieces."""
    lv = grid(rng, nx, nz, True, pitch)
    s: Any = lv.s
    if f64_normals:
        s.normal = s.normal.astype(np.float64)
    # steeper normals on a third of the surfaces (lips, steep terrain), a few exactly at the lip cosines
    for k in range(len(s.y)):
        if rng.random() < 0.33:
            a = rng.uniform(0.3, 1.2)
            n = np.array([a * rng.choice([-1, 1]), 1.0, rng.uniform(-0.3, 0.3)])
            s.normal[k] = n / np.linalg.norm(n)
        if rng.random() < 0.03:
            s.normal[k, 1] = rng.choice([np.cos(np.radians(15.0)), np.float32(np.cos(np.radians(15.0)))])
    return s


def tris_near(rng: random.Random, s: Any, count: int) -> np.ndarray:
    """Random triangles over (and around) the run's extent, at its floors' heights; some degenerate."""
    out = []
    w, d = s.nx * s.pitch, s.nz * s.pitch
    for _ in range(count):
        x, z, y = rng.uniform(-1, w + 1), rng.uniform(-1, d + 1), rng.choice([0.0, 2.5, 5.0]) + rng.uniform(-0.3, 0.6)
        t = [[x + rng.uniform(-1.5, 1.5), y + rng.uniform(-0.4, 0.4), z + rng.uniform(-1.5, 1.5)] for _ in range(3)]
        if rng.random() < 0.1:
            t[2] = t[1]                                         # a degenerate one
        out.append(t)
    return np.array(out, np.float32)


def rules(rng: random.Random) -> int:
    """navwalk's rules (headroom, stair_slope, narrow_lips, near_objects, on_objects) and walkable with every rule
    switched on (pitch 0.2: the lips; stair, dynamic, exclude and forbidden objects), both backends."""
    calls = 0
    for case in range(14):
        pitch = 0.2 if case % 2 == 0 else 0.8
        s = runs(rng, rng.randint(4, 11), rng.randint(4, 11), pitch, f64_normals=case % 4 == 3)
        n = len(s.y)
        check(navwalk.native_samples(s) is not None, f'rules case {case}: a native grid')
        def mask(p: float, n: int = n) -> np.ndarray:
            return np.array([rng.random() < p for _ in range(n)], bool)
        for ign in (None, mask(0.2), mask(0.0)):
            twin('e2a', f'headroom case {case}', navwalk.headroom(s, ign), lambda: py(navwalk.headroom, s, ign))
            calls += 1
        for r, tol in ((1, 0.7), (2, 1.1), (4, 0.7), (1, 0.25)):
            m = mask(0.5)
            twin('e2a', f'stair_slope case {case} r {r} tol {tol}', navwalk.stair_slope(s, m, r=r, tol=tol),
                 lambda: py(navwalk.stair_slope, s, m, r=r, tol=tol))
            calls += 1
        for rise, flat, width, slope in ((None, None, 2, 40.0), (0.3, None, 2, 40.0), (0.3, 15.0, 2, 40.0),
                                         (None, 15.0, 3, 30.0), (0.05, None, 1, 45.0)):
            base, cand = mask(0.6), mask(0.4)
            twin('e2a', f'narrow_lips case {case} rise {rise} flat {flat} width {width}',
                 navwalk.narrow_lips(s, base, cand, slope, width, rise=rise, flat=flat),
                 lambda: py(navwalk.narrow_lips, s, base, cand, slope, width, rise=rise, flat=flat))
            calls += 1
        tris = tris_near(rng, s, 30)
        for grow, below, above in ((0.5, 2.2, 0.3), (0.4, 0.6, 0.6), (0.1, -0.05, 0.0), (1, 1, 0)):
            twin('e2a', f'near_objects case {case}', navwalk.near_objects(s, tris, grow, below, above),
                 lambda: py(navwalk.near_objects, s, tris, grow, below, above))
            calls += 1
        for tol in (0.2, 0.08, 0.5):
            twin('e2a', f'on_objects case {case}', navwalk.on_objects(s, tris, tol),
                 lambda: py(navwalk.on_objects, s, tris, tol))
            calls += 1
        seeds = [(rng.uniform(0, s.nx * s.pitch), rng.uniform(0, 3), rng.uniform(0, s.nz * s.pitch)) for _ in range(3)]
        kw: dict[str, Any] = dict(return_gradients=True, return_stairs=True, extra_seeds=seeds, anchors=seeds[:1],
                                  dynamic_tris=tris_near(rng, s, 6), stair_tris=tris_near(rng, s, 6),
                                  exclude_tris=tris_near(rng, s, 4), exclude_footprint_tris=tris_near(rng, s, 3),
                                  forbid_tris=tris_near(rng, s, 5))
        twin('e2a', f'walkable (every rule) case {case}', navwalk.walkable(s, **kw),
             lambda: py(navwalk.walkable, s, **kw))
        calls += 1
    # the run's numbers of a kind the port does not take: Python runs, the same answers
    s = runs(rng, 5, 5, 0.2)
    s.pitch = np.float32(0.2)
    check(navwalk.native_samples(s) is None, 'a float32 pitch: no native grid')
    s = runs(rng, 5, 5, 0.2)
    check(navwalk._tris_f64(np.zeros((2, 9))) is None, 'triangles not n x 3 x 3: Python runs')
    zero = np.zeros((2, 3, 3), np.float32)                                     # two triangles in one point
    twin('e2a', 'near_objects on point triangles', navwalk.near_objects(s, zero), lambda: py(navwalk.near_objects, s, zero))
    nan_tris = tris_near(rng, s, 3)
    nan_tris[1, 0, 0] = np.nan                                                 # the port refuses, the reference raises
    try:
        navwalk.near_objects(s, nan_tris)
        check(False, 'near_objects with a NaN x: the reference raises')
    except ValueError:
        pass
    twin('e2a', 'on_objects with a NaN x', navwalk.on_objects(s, nan_tris), lambda: py(navwalk.on_objects, s, nan_tris))
    twin('e2a', 'stair_slope with a NumPy r', navwalk.stair_slope(s, np.ones(len(s.y), bool), r=cast(Any, np.int64(1))),
         lambda: py(navwalk.stair_slope, s, np.ones(len(s.y), bool), r=1))
    return calls


def areas_and_picks(rng: random.Random) -> int:
    """build_areas (tiles, components, bisection) and build_graph's centre and portal cells, both backends."""
    calls = 0
    saved = (nb.BISECT_AREAS, nb.MAX_AREA)
    try:
        for case in range(12):
            lv = grid(rng, rng.randint(5, 14), rng.randint(5, 14), case % 2 == 0, 0.8 if case % 3 else 0.2)
            nb.BISECT_AREAS, nb.MAX_AREA = case % 3 != 0, rng.choice([1, 2, 3, 5, 25])
            for start, tile, tiles in (((-1, -1), 3, 3), ((0, 0), 4, 2), ((2, -1), 1, 6), ((0, 3), 100, 1)):
                twin('e2a', f'build_areas case {case} start {start} tile {tile} bisect {nb.BISECT_AREAS}',
                     nb.build_areas(lv, 0, 0, start, tile, tiles, cfg=lv.cfg),
                     lambda: py(nb.build_areas, lv, 0, 0, start, tile, tiles, cfg=lv.cfg))
                calls += 1
            areas = nb.build_areas(lv, 0, 0, cfg=lv.cfg)
            big = [x for x in areas if len(x['cells']) >= 4]
            got = nb._centre_cells(big)
            want: list[int] = []
            for x in big:
                ci = sum(c[0] for c in x['cells']) / len(x['cells'])
                cj = sum(c[1] for c in x['cells']) / len(x['cells'])
                keys = [(c[0] - ci) ** 2 + (c[1] - cj) ** 2 for c in x['cells']]
                want.append(keys.index(min(keys)))
            check(got == want, f'centre cells case {case}')
            ox, oz = rng.choice([0.0, -300.0, 1200.0, -910.4000244140625]), rng.choice([0.0, 500.0, -100.0])
            ends: list[tuple[int, list[tuple[float, float]]]] = []
            for ai, x in enumerate(areas):
                i0, j0 = min(c[0] for c in x['cells']), min(c[1] for c in x['cells'])
                mids = [(ox + (i0 + rng.choice([-0.5, 0.5, 1.5, 3.0])) * lv.cfg.pitch,
                         oz + (j0 + rng.choice([-0.5, 0.0, 2.5])) * lv.cfg.pitch) for _ in range(rng.randint(0, 3))]
                ends.append((ai, mids))
            got2 = nb._nearest_cells(areas, ends, ox, oz, lv.cfg.pitch)
            want2: list[list[int]] = []
            for ai, mids in ends:
                want2.append([])
                for mx, mz in mids:
                    keys = [(ox + c[0] * lv.cfg.pitch - mx) ** 2 + (oz + c[1] * lv.cfg.pitch - mz) ** 2
                            for c in areas[ai]['cells']]
                    want2[-1].append(keys.index(min(keys)))
            check(got2 == want2, f'portal cells case {case}')
            calls += 2
    finally:
        nb.BISECT_AREAS, nb.MAX_AREA = saved
    # numbers of a kind the port does not take: build_graph picks them itself
    check(nb._nearest_cells([{'cells': [(0, 0, 0)]}], [(0, [(0.5, 0.5)])], cast(Any, np.float32(0.0)), 0.0,
                            0.8) is None,
          'a float32 origin: no native picking')
    check(nb._centre_cells([{'cells': [(0.0, 0, 0)] * 4}]) is None, 'float cells: no native picking')
    check(nb._centre_cells([{'cells': []}]) is None, 'an area without cells: the reference raises, Python runs')
    return calls


# ------------------------------------------------------------------------------------------------ round E3c

def ordered(v: Any) -> Any:
    """v as plain values, dicts as their items in order (insertion order is the order runs come out in), floats by
    repr (exact; a float64 scalar as the float it is), the mesh's portal objects as tuples."""
    if isinstance(v, np.ndarray):
        return ['ndarray', v.dtype.str, list(v.shape), v.tobytes().hex()]
    if isinstance(v, (bool, np.bool_)):
        return bool(v)
    if isinstance(v, (int, np.integer)):
        return int(v)
    if isinstance(v, (float, np.floating)):
        return ['f', repr(float(v))]
    if v is None or isinstance(v, str):
        return v
    if isinstance(v, dict):
        return ['dict', [[ordered(k), ordered(x)] for k, x in v.items()]]
    if isinstance(v, (list, tuple)):
        return [ordered(x) for x in v]
    if isinstance(v, (set, frozenset)):
        return ['set', sorted((ordered(x) for x in v), key=repr)]
    if hasattr(v, '__dataclass_fields__'):
        return [type(v).__name__] + [ordered(getattr(v, f)) for f in v.__dataclass_fields__]
    raise TypeError(f'ordered: {type(v).__name__}')


def ramps(rng: random.Random, nx: int, nz: int, pitch: float) -> nb.Level:
    """A random level for the tracing: ground on ramps (steps up to 0.95 m a cell, normals to match, so the slope rule
    links cells the detail step breaks), holes, thin second layers (one-way candidates) and decks 2.5 m over part of
    it."""
    ax, az = rng.uniform(-0.95, 0.95), rng.uniform(-0.4, 0.4)
    deck = (rng.randint(0, nx), rng.randint(0, nz), rng.randint(1, nx), rng.randint(1, nz))
    start, ys, nrm, hit = [0], [], [], []
    for j in range(nz):
        for i in range(nx):
            gx, gz = (ax if (i // 3) % 2 else 0.1 * ax), az        # rise per cell
            col = []
            if rng.random() > 0.08:
                col.append((gx * i + gz * j + rng.choice([0.0, 0.0, 0.05, 0.3]), gx / pitch, gz / pitch))
                if rng.random() < 0.12:                         # a thin second layer (a stair edge, a split-off sheet)
                    col.append((col[0][0] + rng.choice([0.15, 0.3, 0.45]), gx / pitch, gz / pitch))
            if deck[0] <= i < deck[0] + deck[2] and deck[1] <= j < deck[1] + deck[3] and rng.random() > 0.1:
                col.append((gx * i + gz * j + 2.5, 0.0, 0.0))
            for y, gxm, gzm in sorted(col):
                ys.append(y)
                n = np.array([-gxm, 1.0, -gzm])
                nrm.append(n / np.linalg.norm(n))
                hit.append((navwalk.TERRAIN_SHAPE_VTABLE if rng.random() < 0.8 else 1, 0, 0))
            start.append(len(ys))
    count = len(ys)
    s: Any = SimpleNamespace(nx=nx, nz=nz, pitch=pitch, min_x=0.0, min_z=0.0, start=np.array(start, np.int64),
                             y=np.array(ys, np.float32), normal=np.array(nrm, np.float32).reshape(-1, 3),
                             hit=np.array(hit, np.uint32).reshape(-1, 3), flags=np.zeros(count, np.uint8))
    return nb.Level(s, np.array([rng.random() < 0.95 for _ in range(count)], bool), 0.28,
                    stair=np.array([rng.random() < 0.03 for _ in range(count)], bool), cfg=nb.DEFAULTS.at_pitch(pitch))


def windowed(rng: random.Random, lv: nb.Level) -> nb.BuildSettings:
    """lv's settings with a window over part of its grid (border portals where it ends)."""
    s = lv.s
    return replace(lv.cfg, window=range(-1, rng.randint(2, max(3, min(s.nx, s.nz) + 1))))


def trace_chain(lv: nb.Level, cfg: nb.BuildSettings, gi0: int, gj0: int, ox: float, oz: float, keep: set[int],
                many: bool, tiling: tuple[tuple[int, int], int, int]) -> Any:
    """stage's tracing of one sector: areas (tiling: start, samples per tile, tiles), trace_edges (or
    _trace_edges_many), drop_one_way, detail_pass, trace_emit - on whichever backend is switched on."""
    start, tile, tiles = tiling
    areas = nb.build_areas(lv, gi0, gj0, start, tile, tiles, cfg=cfg)
    areas.sort(key=lambda a: (a['tile'][0], a['tile'][1], min(lv.y[c[2]] for c in a['cells'])))
    area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
    if many:
        got = nb._trace_edges_many(lv, [((0, 0), {'areas': areas, 'origin': (gi0, gj0, ox, oz)})], cfg)
        check(got is not None, 'trace_edges_many: native')
        per_area, one_way = got[(0, 0)] if got else nb.trace_edges(lv, areas, area_of, gi0, gj0, cfg)
    else:
        per_area, one_way = nb.trace_edges(lv, areas, area_of, gi0, gj0, cfg)
    traced = ordered((per_area, one_way))
    nb.drop_one_way(per_area, one_way, {n for n in range(len(one_way)) if n in keep})
    lv.blocked = set()
    details = nb.detail_pass(lv, areas, per_area, ox, oz, gi0, gj0, cfg)
    before = ordered(per_area)
    emitted = nb.trace_emit(lv, areas, per_area, gi0, gj0, ox, oz, cfg)
    return traced, before, ordered(details), sorted(lv.blocked), ordered(emitted)


def sector_graph(lv: nb.Level, cfg: nb.BuildSettings, ox: float, oz: float, tiling: Any, rng_seed: int,
                 ladders: int = 0) -> Any:
    """stage's graph of one sector on whichever backend is switched on: areas, tracing, the D sides, border nodes on
    the window's edge columns, build_graph (cover nodes, pairs, Gabriel, groups joined, order nodes)."""
    rng = random.Random(rng_seed)
    start, tile, tiles = tiling
    areas = nb.build_areas(lv, 0, 0, start, tile, tiles, cfg=cfg)
    areas.sort(key=lambda a: (a['tile'][0], a['tile'][1], min(lv.y[c[2]] for c in a['cells'])))
    for n, a in enumerate(areas):
        a['layer'] = n
    area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
    per_area, one_way = nb.trace_edges(lv, areas, area_of, 0, 0, cfg)
    nb.drop_one_way(per_area, one_way, set())
    lv.blocked = set()
    nb.detail_pass(lv, areas, per_area, ox, oz, 0, 0, cfg)
    traced = nb.trace_emit(lv, areas, per_area, 0, 0, ox, oz, cfg)
    cells = [c for a in areas for c in a['cells']]
    borders = []
    for n in range(rng.randint(0, 6) if cells else 0):
        i, j, k = rng.choice(cells)
        borders.append((0x80000000 | (900 + n), (ox + i * cfg.pitch, float(lv.y[k]) + cfg.y_offset, oz + j * cfg.pitch),
                        k))
    orders = [(ox + i * cfg.pitch, float(lv.y[k]), oz + j * cfg.pitch) for i, j, k in rng.sample(cells, min(3, len(cells)))]
    ids = [0]

    def new_id() -> int:
        ids[0] += 1
        return ids[0]
    lads = [(rng.choice(cells)[2], rng.choice(cells)[2]) for _ in range(ladders if cells else 0)]
    pts, pairs = nb.build_graph(lv, areas, area_of, traced, borders, 0, 0, ox, oz, new_id, orders, lads, cfg)
    return ordered(pts), ordered(pairs)


def strict(fn: Any, *a: Any, **k: Any) -> Any:
    """fn on the Rust core with no fallback: a refusal (which would run the Python silently) raises here."""
    mods: list[Any] = [nb, navwalk]
    saved = [m._NATIVE_ERRORS for m in mods]
    for m in mods:
        m._NATIVE_ERRORS = ()
    try:
        return fn(*a, **k)
    finally:
        for m, v in zip(mods, saved):
            m._NATIVE_ERRORS = v


def e3c(rng: random.Random) -> int:
    """Round E3c's pieces against their Python twin on random levels: trace_edges (one sector and the parallel
    batch), trace_emit after drop_one_way and the detail pass, roof_islands, add_cover_nodes, gabriel_prune. The Rust
    side runs strict (no fallback)."""
    calls = 0
    seen = dict(portals=0, borders=0, outline=0, one_way=0, details=0)
    for case in range(60):
        pitch = 0.8 if case % 3 else 0.2
        n1, n2 = rng.randint(4, 16), rng.randint(4, 16)
        lv = ramps(rng, n1, n2, pitch) if case % 2 else grid(rng, n1, n2, case % 4 == 0, pitch)
        cfg = windowed(rng, lv) if case % 4 else lv.cfg
        gi0, gj0 = rng.choice([0, 0, 1, 2]), rng.choice([0, 0, 1])
        ox, oz = rng.choice([0.0, -300.0, 1200.0, -910.4000244140625]), rng.choice([0.0, 500.0, -100.0, 12.8])
        keep = {n for n in range(64) if rng.random() < 0.5}
        tiling = rng.choice([((-1, -1), cfg.tile, cfg.tiles), ((-1, -1), 3, 6), ((0, 0), 4, 4), ((-1, 0), 2, 9)])
        a = strict(trace_chain, lv, cfg, gi0, gj0, ox, oz, keep, case % 3 == 1, tiling)
        b: Any = (lambda: py(trace_chain, lv, cfg, gi0, gj0, ox, oz, keep, False, tiling)) if MODE['record'] else None
        b = b() if b else [None] * 5
        seen['one_way'] += len(a[0][1])
        seen['details'] += sum(len(x[0]) + len(x[1]) for x in a[2])
        for kind, n in (('portals', 0), ('borders', 1), ('outline', 2)):
            seen[kind] += sum(len(x[n]) for x in a[4])
        for name, x, y in zip(('trace_edges', 'drop / detail', 'detail_pass', 'blocked', 'trace_emit'), a, b):
            twin('e3c', f'{name} case {case} (pitch {pitch}, window {cfg.window})', x, lambda y=y: y)
        calls += 3
    check(all(seen.values()), f'the random levels make every kind of edge: {seen}')
    print(f'E3c tracing: {seen}')
    # whole route graphs (cover nodes on the detailed pitch, Gabriel there, the groups joined, order nodes)
    joined = 0
    for case in range(36):
        pitch = 0.2 if case % 2 else 0.8
        n1, n2 = rng.randint(6, 26), rng.randint(6, 26)
        lv = ramps(rng, n1, n2, pitch) if case % 3 else grid(rng, n1, n2, case % 4 == 0, pitch)
        cfg = windowed(rng, lv) if case % 4 == 1 else lv.cfg
        if case % 5 == 2:
            cfg = replace(cfg, dist_edge_max=rng.choice([3.0, 1.6]))         # shorter edges: more groups to join
        ox, oz = rng.choice([0.0, -910.4000244140625, 300.0]), rng.choice([0.0, 12.8, -100.0])
        tiling = rng.choice([((-1, -1), cfg.tile, cfg.tiles), ((-1, -1), 4, 6), ((0, 0), 7, 4)])
        seed = rng.randrange(1 << 30)
        a = strict(sector_graph, lv, cfg, ox, oz, tiling, seed)
        twin('e3c', f'build_graph case {case} (pitch {pitch}, {len(a[0])} nodes, {len(a[1])} pairs)', a,
             lambda: py(sector_graph, lv, cfg, ox, oz, tiling, seed))
        joined += sum(1 for p in a[0] if p[0] < 900)
        calls += 4
    print(f'E3c route graphs: {joined} nodes')
    # build_areas for a stage's sectors at once (with each area's lowest and highest ground) against build_areas
    for case in range(16):
        lv = ramps(rng, rng.randint(6, 20), rng.randint(6, 20), 0.8 if case % 2 else 0.2) if case % 3 else             grid(rng, rng.randint(6, 20), rng.randint(6, 20), case % 2 == 0)
        specs = [(gi, gj, 0.0, 0.0, rng.choice([(-1, -1), (0, 0), (0, -1)]), rng.choice([3, 4, 50]),
                  rng.choice([1, 3, 6])) for gi, gj in ((0, 0), (2, 1), (-1, 3), (5, 0))]
        many = strict(nb._build_areas_many, lv, specs, lv.cfg)

        def want(lv: nb.Level = lv, specs: Any = specs) -> Any:
            out = []
            for gi0, gj0, _ox, _oz, start, tile, tiles in specs:
                areas = py(nb.build_areas, lv, gi0, gj0, start, tile, tiles, cfg=lv.cfg)
                out.append((areas, [(min(lv.y[c[2]] for c in a['cells']), max(lv.y[c[2]] for c in a['cells']))
                                    for a in areas]))
            return ordered(out)
        twin('e3c', f'build_areas_many case {case}', ordered(many), want)
        calls += 1
    # soldiers' surroundings (near_characters): random characters, reaches and height bands
    for case in range(16):
        s = runs(rng, rng.randint(3, 12), rng.randint(3, 12), 0.2 if case % 2 else 0.8)
        char = np.array([rng.random() < 0.08 for _ in range(len(s.y))], bool)
        grow, below, above = rng.choice([0.9, 0.5, 1.7, 0.8]), rng.choice([2.2, 0.5]), rng.choice([0.3, 1.0])
        twin('e3c', f'near_characters case {case}', strict(navwalk.near_characters, s, char, grow, below, above),
             lambda: py(navwalk.near_characters, s, char, grow, below, above))
        calls += 1
    # the sample files' records (versions 1 and 2; a run cut off between columns, a record cut short)
    import navsample
    with tempfile.TemporaryDirectory() as tmp:
        for case in range(12):
            version = 1 + case % 2
            nx, nz = rng.randint(1, 9), rng.randint(1, 9)
            body = bytearray()
            for _c in range(nx * nz if case % 4 else rng.randint(0, nx * nz)):
                n = rng.choice([0, 1, 1, 2, 3, 5])
                body.append(n)
                for _ in range(n):
                    body += struct.pack('<f3bB', rng.uniform(-50, 50), *(rng.randint(-128, 127) for _ in range(3)),
                                        rng.randrange(256))
                    if version == 2:
                        body += struct.pack('<3I', *(rng.randrange(1 << 32) for _ in range(3)))
            if case == 11:
                body = body[:-3]                                   # a record cut short: struct.error either way
            head = navsample.HEADER.pack(b'ONS1', version, 0.8, -12.0, 4.0, nx, nz, 50.0, -50.0, 0.4, 0.6, 1.8,
                                         0.05, 0)
            path = os.path.join(tmp, f'run{case}.bin')
            with open(path, 'wb') as f:
                f.write(head + bytes(body))

            def load(path: str = path) -> Any:
                try:
                    smp, c = navsample.load(path)
                except struct.error as e:
                    return ('struct.error', str(e))
                # (and whether the arrays came from the Rust core's bytearrays)
                return (ordered((smp.start, smp.y, smp.normal, smp.flags, smp.hit, c, smp.pitch, smp.nx, smp.nz)),
                        isinstance(smp.y.base, memoryview))
            a, b = strict(load), py(load)
            check(a[0] == b[0] if case != 11 else a == b, f'navsample.load case {case} (version {version})')
            check(case == 11 or (a[1] and not b[1]), f'navsample.load case {case}: the backends were not the asked ones')
            calls += 1
    # roof islands: random keep and anchors, the rule's settings varied
    saved = (navwalk.ROOF_RAISE, navwalk.ROOF_SHARE, navwalk.ROOF_NEAR, navwalk.ROOF_NEAR_DY)
    try:
        for case in range(24):
            lv = grid(rng, rng.randint(4, 14), rng.randint(4, 14), case % 2 == 0, 0.8 if case % 3 else 0.2)
            s = lv.s
            navwalk.ROOF_RAISE, navwalk.ROOF_SHARE, navwalk.ROOF_NEAR, navwalk.ROOF_NEAR_DY = (
                rng.choice([2.0, 0.5, 0.1]), rng.choice([0.5, 0.0, 0.2, 1.0]), rng.choice([2.0, 0.8, 5.0, 1]),
                rng.choice([2.5, 0.3]))
            kmask = np.array([rng.random() < 0.7 for _ in range(len(s.y))], bool)
            kept = np.nonzero(kmask)[0].tolist()
            anchors = set(rng.sample(kept, min(len(kept), rng.randint(0, 3))))
            terrain = lv.terrain
            navwalk.ROOF_STATS.clear()
            x = strict(navwalk.roof_islands, s, kmask, lv.gx, lv.gz, lv.stair, 0.28, terrain, anchors)
            xs = dict(navwalk.ROOF_STATS)
            navwalk.ROOF_STATS.clear()

            def want(s: Any = s, kmask: Any = kmask, lv: nb.Level = lv, terrain: Any = terrain,
                     anchors: Any = anchors) -> Any:
                y = py(navwalk.roof_islands, s, kmask, lv.gx, lv.gz, lv.stair, 0.28, terrain, anchors)
                return (y, dict(navwalk.ROOF_STATS))
            twin('e3c', f'roof_islands case {case}: {xs}', (x, xs), want)
            calls += 1
    finally:
        navwalk.ROOF_RAISE, navwalk.ROOF_SHARE, navwalk.ROOF_NEAR, navwalk.ROOF_NEAR_DY = saved
        navwalk.ROOF_STATS.clear()
    # coverage nodes: build_graph's add (one node per surface and rounded position) on random areas and nodes
    for case in range(24):
        pitch = 0.8 if case % 2 else 0.2
        lv = grid(rng, rng.randint(5, 16), rng.randint(5, 16), case % 3 == 0, pitch)
        areas = nb.build_areas(lv, 0, 0, cfg=lv.cfg)
        ox, oz = rng.choice([0.0, -910.4000244140625, 300.0]), rng.choice([0.0, 12.8])
        cells = [c for x in areas for c in x['cells']]
        first: list[Any] = []
        for n in range(rng.randint(0, 8)):
            ci, cj, ck = rng.choice(cells) if cells else (0, 0, 0)
            first.append((n + 1, (ox + ci * pitch + rng.choice([0.0, 0.4]), float(lv.y[ck]) + 0.95, oz + cj * pitch),
                          ck, ci, cj))
        cover = rng.choice([4.0, 1.0, 0.5, 2.3, 3])

        def run(cover: float = cover, first: list[Any] = first, lv: nb.Level = lv, areas: list[Any] = areas,
                ox: float = ox, oz: float = oz) -> Any:
            pts = list(first)
            taken = {p[2] for p in pts}
            taken_pos = {tuple(round(v, 3) for v in p[1]) for p in pts}

            def add(i: int, j: int, k: int) -> None:
                pos = (ox + i * lv.cfg.pitch, float(lv.y[k]) + lv.cfg.y_offset, oz + j * lv.cfg.pitch)
                key = tuple(round(v, 3) for v in pos)
                if k in taken or key in taken_pos:
                    return
                taken.add(k)
                taken_pos.add(key)
                pts.append((1000 + len(pts), pos, k, i, j))
            nb.add_cover_nodes(lv, areas, pts, add, ox, oz, cover, lv.cfg)
            return ordered(pts)
        twin('e3c', f'add_cover_nodes case {case} (cover {cover})', strict(run), lambda run=run: py(run))
        calls += 1
    # Gabriel pruning: nodes on a few floors (some at one spot), pairs within 15 m, some lengths equal
    for case in range(40):
        n = rng.randint(0, 60)
        pts = [(q, (rng.choice([rng.uniform(0, 30), float(rng.randint(0, 30))]), rng.choice([0.95, 3.45, 5.0]),
                    rng.choice([rng.uniform(0, 30), float(rng.randint(0, 30))])), q, 0, 0) for q in range(n)]
        if n > 3:
            pts[1] = pts[0]
        pairs = [(a_, b_, math.dist(pts[a_][1], pts[b_][1])) for a_ in range(n) for b_ in range(a_ + 1, n)
                 if math.dist(pts[a_][1], pts[b_][1]) <= 15.0 and rng.random() < 0.6]
        twin('e3c', f'gabriel_prune case {case}', strict(nb.gabriel_prune, pts, pairs),
             lambda: py(nb.gabriel_prune, pts, iter(pairs)))
        calls += 1
    return calls


class _Counting:
    """A NavLevel whose E3h calls are counted (the strict runs must take the Rust paths, not a silent Python one)."""
    seen: dict[str, int] = {}

    def __init__(self, nat: Any) -> None:
        self._nat = nat

    def __getattr__(self, name: str) -> Any:
        f = getattr(self._nat, name)
        if name in ('graph_many', 'trace_edges_stored', 'trace_emit_many', 'detail_candidates', 'build_areas_many'):
            def counted(*a: Any, **k: Any) -> Any:
                _Counting.seen[name] = _Counting.seen.get(name, 0) + 1
                return f(*a, **k)
            return counted
        return f


def whole_stage(lv: nb.Level, cfg: nb.BuildSettings, orders: list[Any]) -> Any:
    """navbuild.stage on lv's grid (its keep, gradients and stairs as the walkability grid) on whichever backend is
    switched on: every sector's areas, border nodes, meshes (written), route graph, and the build's counters."""
    for d in (nb.ORDER_IDS, nb.ORDER_STATS, nb.LADDER_STATS, nb.DETAIL_STATS):
        d.clear()
    a = nb.stage(None, None, 0, 0, log=lambda *_a, **_k: None, cfg=cfg, order_points=orders,
                 grid=(lv.s, lv.keep, lv.gx, lv.gz, lv.stair))
    import kymesh
    meshes = {sec: hashlib.sha1(kymesh.write_mesh(m)).hexdigest() for sec, m in a["meshes"].items()}
    info = {sec: ([(x['tile'], x['layer'], x['cells']) for x in inf['areas']], dict(inf['area_of']), inf['origin'])
            for sec, inf in a['info'].items()}
    graphs = [(sid, b, pts, pairs, o) for sid, b, _m, pts, pairs, o in a['graphs']]
    return ordered((meshes, info, graphs, dict(a['border_nodes']), a['next_id'], a['dropped'], sorted(nb.ORDER_IDS),
                    dict(nb.ORDER_STATS), dict(nb.DETAIL_STATS), sorted(a['level'].blocked)))


def e3h(rng: random.Random) -> int:
    """Round E3h's pieces against their Python twin: whole stages (the areas and edge rows kept in the Rust core,
    drop_one_way and the detail filter there, trace_emit and every sector's route graph in parallel) on random levels
    over the corner of four sectors, at the normal and the detailed pitch; sector graphs with ladders; round(v, 3).
    The Rust side runs strict (no fallback), its E3h calls counted."""
    calls = 0
    real_native = nb.Level.native

    def counting_native(self: nb.Level) -> Any:
        nat = real_native(self)
        return nat if isinstance(nat, _Counting) else _Counting(nat)
    _Counting.seen.clear()
    sizes = dict(sectors=0, areas=0, nodes=0, details=0)
    nb.Level.native = counting_native         # type: ignore[method-assign]
    try:
        for case in range(10):
            pitch = 0.2 if case % 2 else 0.8
            n = rng.randint(14, 40)
            lv = ramps(rng, n, n, pitch) if case % 3 else grid(rng, n, n, case % 4 == 0, pitch)
            # the grid straddles the corner of four sectors (x, z = 100 m)
            lv.s.min_x = 100.0 - rng.randint(3, n - 3) * pitch
            lv.s.min_z = 100.0 - rng.randint(3, n - 3) * pitch
            cfg = nb.DEFAULTS.at_pitch(pitch)
            kept = np.nonzero(lv.keep)[0]
            orders = []
            for k in rng.sample(kept.tolist(), min(4, len(kept))):
                c = int(lv.col_of[k])
                orders.append((lv.s.min_x + (c % lv.s.nx) * pitch, float(lv.y[k]), lv.s.min_z + (c // lv.s.nx) * pitch))
            a = strict(whole_stage, lv, cfg, orders)
            sizes['details'] += nb.DETAIL_STATS.get('d', 0) + nb.DETAIL_STATS.get('f', 0)
            twin('e3h', f'stage case {case} (pitch {pitch}, {n} x {n})', a,
                 lambda: py(whole_stage, lv, cfg, orders))
            sizes['sectors'] += len(a[0][1])
            sizes['areas'] += sum(len(v[0]) for _k, v in a[1][1])
            sizes['nodes'] += sum(len(g[2]) for g in a[2])
            calls += 1
    finally:
        nb.Level.native = real_native         # type: ignore[method-assign]
    want = ('build_areas_many', 'trace_edges_stored', 'detail_candidates', 'trace_emit_many', 'graph_many')
    check(all(_Counting.seen.get(m, 0) >= 5 for m in want), f'stage: the E3h paths ran: {_Counting.seen}')
    print(f'E3h stages: {sizes}, calls {_Counting.seen}')
    # sector graphs with ladder links (and their new nodes)
    for case in range(12):
        pitch = 0.2 if case % 2 else 0.8
        lv = ramps(rng, rng.randint(6, 20), rng.randint(6, 20), pitch)
        cfg = lv.cfg
        seed = rng.randrange(1 << 30)
        x = strict(sector_graph, lv, cfg, 0.0, -100.0, ((-1, -1), cfg.tile, cfg.tiles), seed, 3)
        twin('e3h', f'build_graph with ladders case {case}', x,
             lambda: py(sector_graph, lv, cfg, 0.0, -100.0, ((-1, -1), cfg.tile, cfg.tiles), seed, 3))
        calls += 1
    # walkable's unseeds (hand edits dropping the island a point is on): the island from the flood's NavLevel
    dropped = 0
    for case in range(16):
        s = runs(rng, rng.randint(6, 18), rng.randint(6, 18), 0.2 if case % 2 else 0.8)
        cols = [c for c in range(s.nx * s.nz) if s.start[c + 1] > s.start[c]]
        seeds = []
        for c in rng.sample(cols, min(len(cols), 6)):
            k = rng.randrange(int(s.start[c]), int(s.start[c + 1]))
            seeds.append((float(s.min_x + (c % s.nx) * s.pitch), float(s.y[k]), float(s.min_z + (c // s.nx) * s.pitch)))
        kw = dict(extra_seeds=seeds[:3], unseeds=seeds[3:] + seeds[:1], return_gradients=True)
        x = strict(navwalk.walkable, s, **kw)
        whole = navwalk.walkable(s, extra_seeds=seeds[:3])
        dropped += int(whole.sum() - x[0].sum())
        twin('e3h', f'walkable with unseeds case {case}', x, lambda: py(navwalk.walkable, s, **kw))
        calls += 1
    check(dropped > 0, 'the unseeds drop ground')
    print(f'E3h unseeds: {dropped} kept surfaces dropped')
    # navdiff.area_cells (the parity measures' cells): random closed outlines on the half-pitch lines and off them,
    # 45-degree bridges, stray segments, portals as kymesh objects - the Rust core against the NumPy reference
    import kymesh
    import navdiff
    areas_ = []
    for case in range(60):
        n = rng.randint(3, 14)
        cx, cz = rng.uniform(-900, 900), rng.uniform(-900, 900)
        step = rng.choice([0.4, 0.1, 0.25, 0.37])
        ring = []
        for q in range(n):
            ang = 2 * math.pi * q / n
            r = rng.uniform(1, 12)
            x, z = cx + r * math.cos(ang), cz + r * math.sin(ang)
            if case % 2:
                x, z = round(x / step) * step, round(z / step) * step
            ring.append((x, rng.choice([0.95, 3.2, rng.uniform(0, 40)]), z))
        outline = [ring + [ring[0]]]
        if case % 3 == 0:
            outline.append([(cx, 1.0, cz), (cx + rng.uniform(-3, 3), 2.0, cz + rng.uniform(-3, 3))])
        portals = [kymesh.Portal(ring[0], ring[1], 0)] if case % 4 == 0 else []
        areas_.append(SimpleNamespace(outline=outline, portals=portals, border_portals=[]))
    areas_.append(SimpleNamespace(outline=[], portals=[], border_portals=[]))
    got_c = navdiff.area_cells_many(areas_)
    twin('e3h', 'navdiff.area_cells', [list(c.items()) for c in got_c],
         lambda: [list(navdiff.area_cells_reference(a).items()) for a in areas_])
    print(f'E3h area cells: {len(areas_)} areas, {sum(len(c) for c in got_c)} cells')
    calls += 1
    # round(v, 3): random values, exact ties (multiples of 1/16), negative zero, the build's positions
    vals = [rng.uniform(-2000, 2000) for _ in range(60000)] + [k / 16 for k in range(-40000, 40000)]
    vals += [rng.randint(-2500, 2500) * 0.2 + rng.choice([0.0, 0.95, -0.05]) for _ in range(40000)]
    vals += [-0.0, 0.0, 0.0004, -0.0004, 1e-300, 5e-324, 2.675, 1.0005]
    got = np.frombuffer(nb._native.navkit_round3(np.array(vals, '<f8').tobytes()), '<f8')
    want_r = np.array([round(v, 3) for v in vals], '<f8')
    check(np.array_equal(got.view('<u8'), want_r.view('<u8')), 'round(v, 3): bit for bit as CPython')
    return calls + 1


# ------------------------------------------------------------------------------------------------ whole levels

def input_files(root: str, lv: str, navregress: Any) -> list[str]:
    """The files a level's build reads its data from (navregress.build_level): the in-game samples (normal and
    pitch 0.2, each with its high-capsule run), the level's object files and settings."""
    samples, _x, _z, f = navregress.inputs(root, lv)
    out = [samples, samples[:-4] + '_hi.bin', *f.values()]
    p02 = os.path.join(root, 'work', 'navsample', f'{lv}_navkit_p02.bin')
    out += [p02, p02[:-4].replace('_p02', '_hi_p02') + '.bin']
    L = os.path.join(root, 'work', 'levels', lv)
    out += [os.path.join(L, n) for n in ('order_points.json', 'area_orders.json', 'ladders.json')]
    out.append(os.path.join(root, 'work', 'navkit', lv, 'navkit.json'))
    return sorted({p for p in out if isinstance(p, str) and os.path.isfile(p)})


def file_sha1(p: str) -> str:
    h = hashlib.sha1()
    with open(p, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 22), b''):
            h.update(chunk)
    return h.hexdigest()


def game(golden: dict[str, Any], record: bool, levels: list[str]) -> None:
    import navregress
    suite_paths.use('navkit')
    import navkit
    root = navkit.find_opfor(None)
    # the files carry each sector's build date and time (navbuild.write_file: time.localtime()): a fixed moment
    clock = time.localtime
    time.localtime = lambda *a: FIXED_CLOCK
    try:
        with tempfile.TemporaryDirectory() as tmp:
            for lv in levels:
                p = os.path.join(tmp, f'{lv}.hpd')
                t = time.perf_counter()
                navregress.build_level(root, lv, p)
                took = time.perf_counter() - t
                data = open(p, 'rb').read()
                inputs = {os.path.relpath(q, root).replace(os.sep, '/'): file_sha1(q)
                          for q in input_files(root, lv, navregress)}
                got = {'pathdata': hashlib.sha1(data).hexdigest(), 'bytes': len(data), 'inputs': inputs}
                if record:
                    golden.setdefault('levels', {})[lv] = got
                    print(f'{lv}: recorded ({len(data):,} bytes, {took:.0f} s)')
                    continue
                want = golden.get('levels', {}).get(lv)
                if want is None:
                    check(False, f'{lv}: not recorded (--record --game --levels {lv})')
                    continue
                changed = sorted(k for k in set(inputs) | set(want['inputs']) if inputs.get(k) != want['inputs'].get(k))
                if changed:
                    check(False, f'{lv}: its inputs are not the recorded ones ({", ".join(changed)}): not compared '
                                 f'(check the new inputs with navregress, then re-record)')
                    continue
                ok = got['pathdata'] == want['pathdata']
                check(ok, f'{lv}: the path data differs from the recording ({len(data):,} bytes, recorded '
                          f'{want["bytes"]:,})')
                print(f'{lv}: {"as recorded" if ok else "DIFFERENT"} ({len(data):,} bytes), {took:.0f} s')
    finally:
        time.localtime = clock


if __name__ == '__main__':
    record = '--record' in sys.argv
    golden = load()
    quick(golden, record)
    if '--game' in sys.argv:
        levels = ['sp_009']
        if '--levels' in sys.argv:
            levels = sys.argv[sys.argv.index('--levels') + 1].split(',')
        game(golden, record, levels)
    if record and not FAILS:
        golden['recorded'] = time.strftime('%Y-%m-%d')
        with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(golden, f, indent=1, sort_keys=True)
            f.write('\n')
        print(f'wrote {GOLDEN}')
    print('all passed' if not FAILS else f'{len(FAILS)} failures')
    sys.exit(1 if FAILS else 0)
