r"""navbuild's grid walks in the Rust core (rust/bc2-navbuild through bc2_native: Level.neighbour, components,
trace_edges' side neighbours, line_walkable, build_graph's search) against the Python reference in the same module:
the same answers, call by call, and the same path data, byte for byte.

  python test_navbuild_native.py                     quick: random multi-layer sample grids (float32 and float64
                                                     gradients, stairs, terrain, every link rule, D sides), every
                                                     walk on both backends; the dispatcher and the fallback
  python test_navbuild_native.py --game [--levels sp_009,sp_004_b]
                                                     + whole builds (navregress.build_level, Project OPFOR BFBC2's
                                                     in-game samples, read only) with each backend into a temporary
                                                     folder: the files must be identical

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
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

FAILS: list[str] = []


def check(cond: bool, msg: str) -> None:
    if not cond:
        FAILS.append(msg)
        print('FAIL', msg)


def py(fn: Any, *a: Any, **k: Any) -> Any:
    """fn on the Python reference (the Rust core off for the call)."""
    nb.use_native(False)
    try:
        return fn(*a, **k)
    finally:
        nb.use_native(True)


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


def quick() -> int:
    if nb.backend() != 'rust':
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    rng = random.Random(943)
    calls = 0
    dirs = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1), (2, 1), (0, 0)]
    for case in range(24):
        f32 = case % 2 == 0
        pitch = 0.8 if case % 3 else 0.2
        lv = grid(rng, rng.randint(3, 9), rng.randint(3, 9), f32, pitch)
        check(lv.native() is not None, f'case {case}: a native level')
        check((lv.gx.dtype == np.float32) == f32, f'case {case}: gradients float32 {f32}')
        s = lv.s
        col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
        # neighbour: every surface, direction and rule, the column its own or another
        for k in range(len(s.y)):
            c = int(col[k])
            for gi, gj in ((c % s.nx, c // s.nx), (rng.randint(-1, s.nx), rng.randint(-1, s.nz))):
                for di, dj in dirs:
                    for rule in (None, 'slope', 'raw', 'terrain'):
                        a = lv.neighbour(k, gi, gj, di, dj, rule=rule)
                        b = py(lv.neighbour, k, gi, gj, di, dj, rule=rule)
                        calls += 1
                        if a != b:
                            check(False, f'case {case}: neighbour({k}, {gi}, {gj}, {di}, {dj}, {rule}) {a} != {b}')
        # areas (components), traced sides, lines and searches over each sector-like window
        areas = nb.build_areas(lv, 0, 0, cfg=lv.cfg)
        check(areas == py(nb.build_areas, lv, 0, 0, cfg=lv.cfg), f'case {case}: build_areas')
        members = {k: (int(col[k] % s.nx), int(col[k] // s.nx)) for k in range(len(s.y)) if lv.keep[k]}
        check(nb.components(lv, members, 0, 0) == py(nb.components, lv, members, 0, 0), f'case {case}: components')
        area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
        check(nb.trace_edges(lv, areas, area_of, 0, 0, lv.cfg) == py(nb.trace_edges, lv, areas, area_of, 0, 0, lv.cfg),
              f'case {case}: trace_edges')
        kept = [k for k in range(len(s.y)) if lv.keep[k]]
        if not kept:
            continue
        cell = {k: (int(col[k] % s.nx), int(col[k] // s.nx)) for k in kept}
        for blocked in (set(), {(rng.choice(kept), rng.choice(kept)) for _ in range(len(kept) // 2)}):
            lv.blocked = blocked
            for _ in range(60):
                k0, k1 = rng.choice(kept), rng.choice(kept)
                pa, pb = cell[k0], (rng.randint(-1, s.nx), rng.randint(-1, s.nz))
                r = nb.line_walkable(lv, k0, pa, pb, 0, 0, k1)
                calls += 1
                check(r == py(nb.line_walkable, lv, k0, pa, pb, 0, 0, k1), f'case {case}: line_walkable {k0} {pa} {pb}')
        # the search, as build_graph runs it: groups of nodes on surfaces, union-find, cell map
        nodes = rng.sample(kept, min(len(kept), 6))
        pts = [(i, (0.0, 0.0, 0.0), k, cell[k][0], cell[k][1]) for i, k in enumerate(nodes)]
        node_at = {k: i for i, k in enumerate(nodes)}
        parent = [0, 0, 2, 2, 4, 5][:len(nodes)]

        def find(x: int, parent: list[int] = parent) -> int:
            while parent[x] != x:
                x = parent[x]
            return x
        for root in sorted({find(i) for i in range(len(nodes))}):
            group = [i for i in range(len(nodes)) if find(i) == root]
            other = {k for k, at in node_at.items() if find(at) != root}
            c1, c2 = {pts[m][2]: cell[pts[m][2]] for m in group}, {pts[m][2]: cell[pts[m][2]] for m in group}
            window = lv.cfg.window
            found = lv.native().search([pts[m][2] for m in group], other, c1, (window.start, window.stop), 0, 0,
                                       nb.LINK_RULE)
            p = py(nb._search, lv, pts, group, root, node_at, find, c2, 0, 0, lv.cfg)
            calls += 1
            check(found == p and c1 == c2, f'case {case}: search from group {root}: {found} != {p}')
    # the dispatcher and the fallback
    lv = grid(rng, 4, 4, True)
    check(nb.use_native(False) == 'python' and lv.native() is None, 'use_native(False): no native level')
    check(nb.use_native(True) == 'rust' and lv.native() is not None, 'use_native(True)')
    first = int(np.nonzero(lv.keep)[0][0])
    c = int(np.searchsorted(lv.s.start, first, side='right')) - 1          # its column
    try:
        lv.neighbour(10 ** 6, c % lv.s.nx - 1, c // lv.s.nx, 1, 0)       # asks that column: the reference reads y[k]
        check(False, 'a surface outside the level: the Rust core refuses, the reference raises')
    except IndexError:
        pass
    check(cast(Any, lv).neighbour(np.int64(0), np.int64(0), np.int64(0), 1, 0) == py(lv.neighbour, 0, 0, 0, 1, 0),
          'numpy integers')
    odd: Any = grid(rng, 3, 3, False)
    odd.gx = odd.gx.astype(np.float16)                       # a dtype the core does not take: Python runs
    check(odd.native() is None, 'float16 gradients: no native level')
    check(navwalk.native_grid(odd.s, odd.keep, odd.gx, odd.gz, odd.stair, 0.28) is None, 'float16: no native grid')
    # walkability (navwalk.walkable: its flood, roof islands) and islands_of, both backends, float32 and float64
    for case in range(16):
        lv = grid(rng, rng.randint(4, 12), rng.randint(4, 12), case % 2 == 0)
        s = lv.s
        seeds = [(rng.uniform(0, s.nx * s.pitch), rng.uniform(0, 3), rng.uniform(0, s.nz * s.pitch)) for _ in range(3)]
        a = navwalk.walkable(s, return_gradients=True, return_stairs=True, extra_seeds=seeds, anchors=seeds[:1])
        b = py(navwalk.walkable, s, return_gradients=True, return_stairs=True, extra_seeds=seeds, anchors=seeds[:1])
        check(all(np.array_equal(x, y_) and x.dtype == y_.dtype for x, y_ in zip(a, b)), f'walkable case {case}')
        for keep in (lv.keep, a[0]):
            ia = navwalk.islands_of(s, keep, lv.gx, lv.gz, lv.stair, 0.28)
            ib = py(navwalk.islands_of, s, keep, lv.gx, lv.gz, lv.stair, 0.28)
            check(ia[1] == ib[1] and np.array_equal(ia[0], ib[0]) and ia[0].dtype == ib[0].dtype,
                  f'islands_of case {case}: {ia[1]} / {ib[1]} islands')
        calls += 3
    check(nb.use_native(False) == 'python' and navwalk._native is None, 'use_native(False) turns the floods off too')
    nb.use_native(True)
    print(f'{calls} calls compared on 40 random levels: {len(FAILS)} differences')
    return 0


def game(levels: list[str]) -> None:
    if nb.backend() != 'rust':
        return
    import navregress
    suite_paths.use('navkit')
    import navkit
    root = navkit.find_opfor(None)
    # the files carry each sector's build date and time (navbuild.write_file: time.localtime()); both builds are
    # given one moment, so the files can be compared byte for byte
    fixed = time.localtime()
    clock = time.localtime
    time.localtime = lambda *a: fixed
    try:
        _game(levels, root, navregress)
    finally:
        time.localtime = clock


def _game(levels: list[str], root: str, navregress: Any) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        for lv in levels:
            out = {}
            for backend in ('rust', 'python'):
                nb.use_native(backend == 'rust')
                p = os.path.join(tmp, f'{lv}_{backend}.hpd')
                t = time.perf_counter()
                navregress.build_level(root, lv, p)
                out[backend] = (open(p, 'rb').read(), time.perf_counter() - t)
            nb.use_native(True)
            same = out['rust'][0] == out['python'][0]
            check(same, f'{lv}: the path data differs')
            print(f'{lv}: {"identical" if same else "DIFFERENT"} ({len(out["rust"][0]):,} bytes); Python '
                  f'{out["python"][1]:.0f} s, Rust {out["rust"][1]:.0f} s')


if __name__ == '__main__':
    quick()
    if '--game' in sys.argv:
        levels = ['sp_009']
        if '--levels' in sys.argv:
            levels = sys.argv[sys.argv.index('--levels') + 1].split(',')
        game(levels)
    print('all passed' if not FAILS else f'{len(FAILS)} failures')
    sys.exit(1 if FAILS else 0)
