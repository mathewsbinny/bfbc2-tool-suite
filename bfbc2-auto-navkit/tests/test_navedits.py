r"""navedits.py, the walkability edits as generator inputs, on a small sample grid built here (no game data):
surface positions, a polygon and a box selecting exactly the surfaces inside them (x / z and the height
range, stacked floors told apart), later edits winning where regions overlap, seeds and unseeds collected in
order, and links / route edges / traversals / notes left to navkit apply.

  python test_navedits.py
"""
import os
import sys
from types import SimpleNamespace

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import navedits  # noqa: E402
from bfbc2core import navtune as nt  # noqa: E402


def grid():
    """10 x 10 columns at 1 m pitch from (0, 0); each column has ground at y 0, and columns with x >= 5 also a
    floor at y 4 (a deck over the ground)."""
    nx = nz = 10
    ys, start = [], [0]
    for _j in range(nz):
        for i in range(nx):
            col = [0.0] + ([4.0] if i >= 5 else [])
            ys += col
            start.append(start[-1] + len(col))
    return SimpleNamespace(nx=nx, nz=nz, pitch=1.0, min_x=0.0, min_z=0.0, start=np.array(start), y=np.array(ys, np.float32))


def matplotlib_region(x, z, y, region):
    """The retired region_mask's polygon branch (round NK4 moved it to the Rust core): the box prefilter, then
    matplotlib's Path.contains_points."""
    from matplotlib.path import Path
    lo, hi = region['y']
    poly = np.asarray(region['polygon'], dtype=np.float64)
    m = (y >= lo) & (y <= hi) & (x >= poly[:, 0].min()) & (x <= poly[:, 0].max()) & (z >= poly[:, 1].min()) & (z <= poly[:, 1].max())
    if m.any():
        idx = np.nonzero(m)[0]
        m[idx] = Path(poly).contains_points(np.stack([x[idx], z[idx]], axis=1))
    return m


def matplotlib_agreement(polygons=400):
    """Random polygons (3-9 vertices; half of them on a 0.5 m lattice, so sample points fall on their outlines and
    vertices; some self-crossing) against sample points on 0.25 m and 1 m lattices: the polygons whose mask differs
    from matplotlib's."""
    rng = np.random.default_rng(20261122)
    gx, gz = np.meshgrid(np.arange(-2, 12.25, 0.25), np.arange(-2, 12.25, 0.25))
    x, z = gx.ravel(), gz.ravel()
    y = np.zeros(len(x))
    bad = 0
    for n in range(polygons):
        k = int(rng.integers(3, 10))
        pts = rng.uniform(-1, 11, (k, 2))
        if n % 2 == 0:
            pts = np.round(pts * 2) / 2
        region = nt.polygon([(float(a), float(b)) for a, b in pts], -1, 1)
        bad += not np.array_equal(navedits.region_mask(x, z, y, region), matplotlib_region(x, z, y, region))
    return bad


def main(argv):
    failed = 0

    def check(ok, what):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}')

    s = grid()
    x, z, y = navedits.surface_positions(s)
    check(len(x) == 150 and x[0] == 0 and z[0] == 0 and (x[-1], z[-1], y[-1]) == (9, 9, 4), 'surface positions (150 surfaces, stacked)')

    poly = nt.polygon([(1.5, 1.5), (3.5, 1.5), (3.5, 3.5), (1.5, 3.5)], -1, 1)        # columns 2-3 x 2-3, ground only
    m = navedits.region_mask(x, z, y, poly)
    check(m.sum() == 4 and set(zip(x[m], z[m])) == {(2, 2), (3, 2), (2, 3), (3, 3)}, 'a polygon selects the 4 surfaces inside it')
    deck = nt.box(4.5, 3, 0, 9.5, 5, 9.5)
    m = navedits.region_mask(x, z, y, deck)
    check(m.sum() == 50 and (y[m] == 4).all(), 'a box at deck height selects the 50 deck surfaces, not the ground under them')
    under = nt.polygon([(4.5, -0.5), (9.5, -0.5), (9.5, 9.5), (4.5, 9.5)], -1, 1)
    check((y[navedits.region_mask(x, z, y, under)] == 0).all() and navedits.region_mask(x, z, y, under).sum() == 50,
          'the same outline at ground height selects only the ground under the deck')

    ops = [nt.op('unwalkable', region=nt.box(-1, -1, -1, 9.5, 1, 9.5)),          # all ground
           nt.op('walkable', region=poly, value=True),                          # 4 ground surfaces back
           nt.op('seed', at=(2, 0, 2)), nt.op('unseed', at=(7, 4, 7)), nt.op('seed', at=(8, 4, 8)),
           nt.op('link-areas', a=(0, 0, 0), b=(1, 0, 1)), nt.op('note', at=(0, 0, 0), text='x'),
           nt.op('route-edge', a=(0, 0, 0), b=(1, 0, 1), action='add'),
           nt.op('traversal', kind='ladder', bottom=(6, 0, 6), top=(6, 4, 6))]
    e = navedits.edits_for(s, ops)
    check(e['surfaces_unwalkable'] == 96 and e['surfaces_walkable'] == 4, 'later edits win: 100 ground surfaces unwalkable, then 4 walkable again')
    check(not (e['force_walkable'] & e['force_unwalkable']).any(), 'no surface is both')
    check(e['seeds'] == [(2.0, 0.0, 2.0), (8.0, 4.0, 8.0)] and e['unseeds'] == [(7.0, 4.0, 7.0)], 'seeds and unseeds, in order')
    check(e['counts'] == {'unwalkable': 1, 'walkable': 1, 'seed': 2, 'unseed': 1}, 'links, route edges, traversals and notes are left to navkit apply')
    e2 = navedits.edits_for(s, [])
    check(not e2['force_walkable'].any() and not e2['force_unwalkable'].any() and not e2['seeds'], 'no edits: nothing forced')
    bad = matplotlib_agreement()
    check(bad == 0, f'polygon regions as matplotlib answers them (Path.contains_points, the retired region_mask): '
                    f'{bad} of 400 random polygons differ (points on outlines and vertices included)')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
