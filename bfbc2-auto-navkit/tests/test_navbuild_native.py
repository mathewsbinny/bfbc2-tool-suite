r"""NavKit in the Rust core (rust/bc2-navbuild through bc2_native).

The grid walks (round E1d: Level.neighbour's linking, components, trace_edges' side neighbours, line_walkable,
build_graph's route-group search; navwalk's flood, islands_of and matched_flags) had a Python twin that was their
reference until 2026-11-17, when the port had passed the full test tier (140/140) and the twin was retired
(rust/BENCHMARKS.md, "Retired Python"). Their results were recorded first, from the Python code itself, in
navbuild_golden.json beside this file: a SHA-1 of every walk's answers on seeded random levels, and of whole levels'
path data (the build clock fixed). They are the oracle now; navregress (DICE's own path data) stays the independent
check of what NavKit builds.

The walkability rules, build_areas and build_graph's cell picking (round E2a) keep their Python twin: they are
still compared with it, call by call.

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
import os
import random
import sys
import tempfile
import time
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


def quick(golden: dict[str, Any], record: bool) -> None:
    nb.require_native()
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
    """CPython's math.hypot, x ** 2 and degrees(atan(hypot)) as the port computes them, on gradients like the rules'
    (height differences over a pitch) and on values over the whole float range."""
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
    h, s, d = (np.frombuffer(b, '<f8') for b in navwalk.require_native().navkit_float_ops(
        np.array(xs, '<f8').tobytes(), np.array(zs, '<f8').tobytes()))
    bad = 0
    for k, (x, z) in enumerate(zip(xs, zs)):
        try:
            sq = x ** 2
        except OverflowError:                                   # CPython raises where pow overflows
            sq = math.inf
        want = (math.hypot(x, z), sq, math.degrees(math.atan(math.hypot(x, z))))
        got = (float(h[k]), float(s[k]), float(d[k]))
        if any(not (a == b or (a != a and b != b)) for a, b in zip(want, got)):
            bad += 1
            if bad <= 5:
                check(False, f'float ops ({x!r}, {z!r}): {got} != {want}')
    check(bad == 0, f'float ops: {bad} of {len(xs)} differ')
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
            check(same(navwalk.headroom(s, ign), py(navwalk.headroom, s, ign)), f'headroom case {case}')
            calls += 1
        for r, tol in ((1, 0.7), (2, 1.1), (4, 0.7), (1, 0.25)):
            m = mask(0.5)
            check(same(navwalk.stair_slope(s, m, r=r, tol=tol), py(navwalk.stair_slope, s, m, r=r, tol=tol)),
                  f'stair_slope case {case} r {r} tol {tol}')
            calls += 1
        for rise, flat, width, slope in ((None, None, 2, 40.0), (0.3, None, 2, 40.0), (0.3, 15.0, 2, 40.0),
                                         (None, 15.0, 3, 30.0), (0.05, None, 1, 45.0)):
            base, cand = mask(0.6), mask(0.4)
            a = navwalk.narrow_lips(s, base, cand, slope, width, rise=rise, flat=flat)
            b = py(navwalk.narrow_lips, s, base, cand, slope, width, rise=rise, flat=flat)
            check(same(a, b), f'narrow_lips case {case} rise {rise} flat {flat} width {width}')
            calls += 1
        tris = tris_near(rng, s, 30)
        for grow, below, above in ((0.5, 2.2, 0.3), (0.4, 0.6, 0.6), (0.1, -0.05, 0.0), (1, 1, 0)):
            check(same(navwalk.near_objects(s, tris, grow, below, above),
                       py(navwalk.near_objects, s, tris, grow, below, above)), f'near_objects case {case}')
            calls += 1
        for tol in (0.2, 0.08, 0.5):
            check(same(navwalk.on_objects(s, tris, tol), py(navwalk.on_objects, s, tris, tol)), f'on_objects case {case}')
            calls += 1
        seeds = [(rng.uniform(0, s.nx * s.pitch), rng.uniform(0, 3), rng.uniform(0, s.nz * s.pitch)) for _ in range(3)]
        kw: dict[str, Any] = dict(return_gradients=True, return_stairs=True, extra_seeds=seeds, anchors=seeds[:1],
                                  dynamic_tris=tris_near(rng, s, 6), stair_tris=tris_near(rng, s, 6),
                                  exclude_tris=tris_near(rng, s, 4), exclude_footprint_tris=tris_near(rng, s, 3),
                                  forbid_tris=tris_near(rng, s, 5))
        a, b = navwalk.walkable(s, **kw), py(navwalk.walkable, s, **kw)
        check(same(a, b), f'walkable (every rule) case {case}')
        calls += 1
    # the run's numbers of a kind the port does not take: Python runs, the same answers
    s = runs(rng, 5, 5, 0.2)
    s.pitch = np.float32(0.2)
    check(navwalk.native_samples(s) is None, 'a float32 pitch: no native grid')
    s = runs(rng, 5, 5, 0.2)
    check(navwalk._tris_f64(np.zeros((2, 9))) is None, 'triangles not n x 3 x 3: Python runs')
    zero = np.zeros((2, 3, 3), np.float32)                                     # two triangles in one point
    check(same(navwalk.near_objects(s, zero), py(navwalk.near_objects, s, zero)), 'near_objects on point triangles')
    nan_tris = tris_near(rng, s, 3)
    nan_tris[1, 0, 0] = np.nan                                                 # the port refuses, the reference raises
    try:
        navwalk.near_objects(s, nan_tris)
        check(False, 'near_objects with a NaN x: the reference raises')
    except ValueError:
        pass
    check(same(navwalk.on_objects(s, nan_tris), py(navwalk.on_objects, s, nan_tris)), 'on_objects with a NaN x')
    check(same(navwalk.stair_slope(s, np.ones(len(s.y), bool), r=cast(Any, np.int64(1))),
               py(navwalk.stair_slope, s, np.ones(len(s.y), bool), r=1)), 'stair_slope with a NumPy r: Python, the same')
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
                a = nb.build_areas(lv, 0, 0, start, tile, tiles, cfg=lv.cfg)
                b = py(nb.build_areas, lv, 0, 0, start, tile, tiles, cfg=lv.cfg)
                check(a == b, f'build_areas case {case} start {start} tile {tile} bisect {nb.BISECT_AREAS}')
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
