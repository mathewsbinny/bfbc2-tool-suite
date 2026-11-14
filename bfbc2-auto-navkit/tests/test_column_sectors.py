r"""navbuild.column_sectors: the sector of every sample column is floor(x / 100) of its position from the stored grid
minimum, as DICE's path data places it - for box minima every 10 m from -3000 to +3000 at pitch 0.8 and 0.2, against
the scalar formula the build used per column. The grid's minimum is aligned as navsample.cpp's Begin()
(navsample_offline.Grid.from_box): a float32 multiple of the pitch, so a box minimum such as -910.4 is stored
-910.4000244 and the column on the sector line x = -900 lands a hair below it, in the sector below. That is DICE's
layout: SP_008's file has the six sectors X = -10 (ids 257-262) holding only that column and its margin; a rule
that put line columns in the sector above lost them (navregress 2026-11-14, sectors_missing 0 -> 6).

  python test_column_sectors.py
"""
import math
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
import navbuild  # noqa: E402


def grid(lo, pitch, span=260.0):
    """Samples-like grid from box minimum lo (both axes) as navsample.cpp's Begin() aligns it."""
    p = np.float32(pitch)
    mn = float(np.float32(math.floor(np.float32(lo) / p)) * p)
    n = int(math.ceil((np.float32(lo + span) - mn) / p)) + 1
    return SimpleNamespace(min_x=mn, min_z=mn, nx=n, nz=n, pitch=pitch)


def check(pitch, minima):
    for lo in minima:
        s = grid(lo, pitch)
        xoff = zoff = int(math.floor(lo / 100.0)) - 1
        cols = np.arange(s.nx)                         # row 0: every x index (z index 0)
        X, Y = navbuild.column_sectors(s, cols, xoff, zoff, pitch)
        for gi in range(s.nx):
            x = s.min_x + gi * pitch                   # the per-column formula of the build before vectorising
            assert X[gi] == int(math.floor(x / 100.0)) - xoff, (pitch, lo, gi, int(X[gi]))
        assert set(Y.tolist()) == {int(math.floor(s.min_z / 100.0)) - zoff}
        # columns of the z axis too (whole rows)
        Xz, Yz = navbuild.column_sectors(s, np.arange(s.nz) * s.nx, xoff, zoff, pitch)
        assert (Yz == X[:s.nz]).all() and set(Xz.tolist()) == {int(X[0])}


def test_many_minima():
    check(0.8, list(range(-3000, 3001, 10)))
    check(0.2, list(range(-3000, 3001, 10)))
    check(0.8, [-910.4, -812.3, -799.9, -0.4, 123.45, 1999.99])


def test_sp008_line_column():
    """SP_008 (box minimum x -910.4, XOffset -11): the x = -900 column is in sector X = 1 (world -10), as DICE's
    sectors 257-262 hold it; the next column is in X = 2. On a grid that starts exactly on a line the line column
    starts its own sector."""
    s = grid(-910.4, 0.8)
    assert s.min_x == float(np.float32(-910.4)) and s.min_x < -910.4
    line = 13                                          # -910.4 + 13 * 0.8 = -900
    X, _ = navbuild.column_sectors(s, np.array([line - 1, line, line + 1]), -11, -1, 0.8)
    assert X.tolist() == [1, 1, 2], X.tolist()
    s = grid(-1000.0, 0.8)
    assert s.min_x == -1000.0
    X, _ = navbuild.column_sectors(s, np.array([124, 125, 126]), -11, -1, 0.8)
    assert X.tolist() == [1, 2, 2], X.tolist()        # x = -900.8, -900 (line, exact), -899.2


if __name__ == '__main__':
    test_many_minima()
    test_sp008_line_column()
    print('ok')
