r"""NavKit's 45-degree bridge outlines (navbuild.trace_emit, the Rust core's union of an area's bridges) on random cell
patterns built here (no game data): an area's outline - its cells' open sides, but those its bridges cover, plus the
bridges' diagonals - is closed (every end point is met an even number of times) for every pattern, as in DICE's
files, overlapping bridges (chains, a cell shared by two corners) included; a lone bridge gives exactly its two
diagonals and covers the four sides at its corner. (The old clip_bridges, which left open ends on some patterns, went
with trace_emit's Python on 2026-11-18: round R1.)

  python test_bridge_union.py
"""
import os
import random
import sys
from collections import Counter
from types import SimpleNamespace
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools')
import navbuild  # noqa: E402

N = 10                                   # the grid: N x N columns, the patterns inside it
P = navbuild.PITCH


def bridges_of(cells):
    """trace_emit's bridges: diagonal-only neighbours (both side cells outside the area)."""
    out = []
    for i, j in sorted(cells):
        for di, dj in ((1, 1), (1, -1)):
            if (i + di, j + dj) in cells and (i + di, j) not in cells and (i, j + dj) not in cells:
                out.append((i, j, di, dj))
    return out


def outline(cells, heights=None):
    """trace_emit's outline of one area holding the cells (one surface in each of their columns, none elsewhere; the
    cells need not be connected): [((x, z, y), (x, z, y))] in local sample units."""
    heights = heights or {}
    start: list[int] = [0]
    ys: list[float] = []
    cell_k = {}
    for j in range(N):
        for i in range(N):
            if (i, j) in cells:
                cell_k[(i, j)] = len(ys)
                ys.append(heights.get((i, j), 0.0))
            start.append(len(ys))
    n = len(ys)
    s: Any = SimpleNamespace(nx=N, nz=N, pitch=P, min_x=0.0, min_z=0.0, start=np.array(start, np.int64),
                             y=np.array(ys, np.float32), normal=np.tile(np.array([0, 1, 0], np.float32), (n, 1)),
                             hit=None, flags=np.zeros(n, np.uint8))
    lv = navbuild.Level(s, np.ones(n, bool), 0.28)
    area = {'cells': [(i, j, k) for (i, j), k in sorted(cell_k.items(), key=lambda t: t[1])]}
    per_area, _one_way = navbuild.trace_edges(lv, [area], {k: 0 for k in cell_k.values()}, 0, 0)
    (_portals, _borders, lines), = navbuild.trace_emit(lv, [area], per_area, 0, 0, 0.0, 0.0)
    return [((a[0] / P, a[2] / P, a[1]), (b[0] / P, b[2] / P, b[1])) for a, b in lines]


def open_ends(lines):
    """End points (half-sample units) of the outline met an odd number of times."""
    deg: Counter[tuple[int, int]] = Counter()
    for p, q in lines:
        deg[(round(p[0] * 2), round(p[1] * 2))] += 1
        deg[(round(q[0] * 2), round(q[1] * 2))] += 1
    return [k for k, v in deg.items() if v % 2]


def main(argv):
    failed = 0

    def check(ok, what):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}')

    rng = random.Random(1)
    bad = with_bridges = overlapping = 0
    patterns = 600
    for _ in range(patterns):
        n = rng.choice((4, 6, 8))
        cells = {(i, j) for i in range(n) for j in range(n) if rng.random() < rng.choice((0.35, 0.5, 0.65))}
        bridges = bridges_of(cells)
        if not bridges:
            continue
        with_bridges += 1
        corners: Counter[tuple[int, int]] = Counter()
        for i, j, di, dj in bridges:
            corners[(i + di, j)] += 1
            corners[(i, j + dj)] += 1
        overlapping += any(v > 1 for v in corners.values())
        if open_ends(outline(cells)):
            bad += 1
    check(with_bridges > 300 and overlapping > 50,
          f'{with_bridges} of {patterns} random patterns have bridges ({overlapping} with overlapping ones)')
    check(bad == 0, f'the bridges\' union: every outline closed ({bad} with open ends)')

    # one bridge alone: exactly its two diagonals, from A's corners to B's corners; the four sides at its corner
    # covered (each cell keeps its two outer sides)
    lines = outline({(0, 0), (1, 1)}, {(0, 0): 1.0, (1, 1): 2.0})
    y = navbuild.Y_OFFSET
    diag = sorted(tuple(sorted((tuple(round(v, 6) for v in p), tuple(round(v, 6) for v in q))))
                  for p, q in lines if p[0] != q[0] and p[1] != q[1])
    check(diag == [((-0.5, 0.5, 1.0 + y), (0.5, 1.5, 2.0 + y)), ((0.5, -0.5, 1.0 + y), (1.5, 0.5, 2.0 + y))],
          f'a lone bridge: two diagonals, A end at A\'s height, B end at B\'s ({diag})')
    sides = sorted(tuple(sorted(((round(p[0], 6), round(p[1], 6)), (round(q[0], 6), round(q[1], 6)))))
                   for p, q in lines if p[0] == q[0] or p[1] == q[1])
    want = sorted(tuple(sorted(x)) for x in (((-0.5, -0.5), (-0.5, 0.5)), ((-0.5, -0.5), (0.5, -0.5)),
                                             ((1.5, 0.5), (1.5, 1.5)), ((0.5, 1.5), (1.5, 1.5))))
    check(sides == want, f'a lone bridge covers the four sides at its corner (the outer sides left: {sides})')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
