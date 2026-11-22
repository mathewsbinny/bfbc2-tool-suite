"""NavKit: D lines and F cells of one area (KYNAPSE_SPEC.md 4.5, "D lines and F cells - the generator rule").

DICE's rule, measured on its files (sp_004_b, sp_01, ONS_MP_002; 2026-10-26):
  * a **broken side** is a side between two straight-neighbour cells of the same area whose ground heights differ by
    more than the link step (0.6 m at pitch 0.8; about 0.175 m at pitch 0.2) - the cells are in one area through
    other cells, but not linked across this side;
  * an **F cell** is a cell whose two sides along one axis are both broken (a cell stepped up into, and stepped up
    out of, along x or along z); the F cells of one axis form polygons (8-connected groups, 45-degree bridges as
    in the outline), each stored twice: direction -axis with every corner's height taken from the cells on the
    -axis side of the corner, and +axis with the +axis side's;
  * a **D line** is a broken side that is not an F cell's side along its axis, stored once per side (two lines
    facing each other), the owning cell on the right as in the outline, collinear runs merged, each vertex at the
    mean height of the owner-side cells touching it along the line.
These sides are not outline (E) in DICE's files: the area's outline goes round the area, D and F lie inside it.

The rule runs in the Rust core (round NK3: bc2_navbuild::detail through bc2_native.navkit_detail_features; the
Python twin was retired, rust/BENCHMARKS.md "NK3"). Its order is the grid's, never a set's: where the Python left a
choice to its sets' iteration order, the spec defines it (bc2_navbuild::detail's documentation) - a loop of an F
group's boundary starting at a pinch leaves along the edge whose end is least, and an F vertex with no cell beside
it takes the nearest F cell's height, the least cell on a tie.

detail_features(cells, pitch, ox, oz, step) -> (broken sides, D polylines, F cells); features(...) also gives the D
sides and the F cells of each axis. Pure: no level, no samples - cells is the area's {(i, j): ground height} on its
sector's sample grid (local indices, cell centre at (ox + i * pitch, oz + j * pitch));
tests/test_detail_features.py runs it on synthetic areas.
"""
import os
import sys
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import kymesh  # noqa: E402
from bfbc2core import native  # noqa: E402

Y_OFFSET = 0.95                       # stored heights are ground + EntityHeight / 2 (as the outline)
CUT_CORNERS = True                    # a cell with D on exactly two perpendicular sides: its own line is the diagonal across
                                      # it (the corner cut), the neighbours' lines an L round the corner (DICE, sp_01: 409
                                      # of 453 such corners cut, 354 of them with the L in one line)
CHAIN_TURNS = True                    # D lines continue round a corner where the chain is unambiguous (DICE's 3-8 point
                                      # lines); False: straight runs only (the first version, 2026-10-26)

Cell = tuple[int, int]
Side = tuple[Cell, tuple[int, int]]
Point = tuple[float, float, float]


@dataclass
class Features:
    """One area's detail features."""
    broken: set[Side]                 # sides between two cells (or a bridge's half cell and a cell) stepping over step
    d_sides: set[Side]                # the broken sides that are D sides (no F cell's side along its axis)
    fx: set[Cell]                     # cells with both x sides broken
    fz: set[Cell]                     # ... both z sides
    lines: list[list[Point]]          # D polylines (world points), owner on the right
    cells: list[kymesh.Cell]          # F records: all -x, +x, -z, +z


def _core() -> Any:
    return native.require("NavKit's detail features (navdetail)", 'navkit_detail_features')


def features(cells: dict[Cell, float], pitch: float, ox: float, oz: float, step: float,
             eligible: Callable[[int, int, int, int], Any] | None = None,
             half: dict[Side, float] | None = None, breakable: Callable[[Cell, Cell], Any] | None = None,
             with_f: bool = True) -> Features:
    """One area's features. cells: {(i, j): ground height}; step: the link step (m).
    eligible(i, j, di, dj) -> False leaves a side out (a portal there, for example).
    half: {((i, j), (di, dj)): height} - the legs of 45-degree cut half cells: cell (i, j) is not in the area but half
    of it is (a bridge), its side toward the area cell (i + di, j + dj) lies inside the area; it breaks as any side,
    with the half cell's own height (DICE: 352 of 389 such legs with a step over 0.6 m have D or F, sp_004_b).
    breakable(a, b) -> False: the side between cells a and b never breaks (detailed zones: objects).
    with_f False: no F cells (their sides are D lines). eligible and breakable are asked only about the sides that
    step more than step."""
    broken, d_sides, fx, fz, lines, fcells = _core().navkit_detail_features(
        cells, float(pitch), float(ox), float(oz), float(step), eligible, half, breakable, bool(with_f),
        bool(CUT_CORNERS), bool(CHAIN_TURNS))
    return Features(set(broken), set(d_sides), set(fx), set(fz), lines,
                    [kymesh.Cell(d, pts) for d, pts in fcells])


def detail_features(cells: dict[Cell, float], pitch: float, ox: float, oz: float, step: float,
                    eligible: Callable[[int, int, int, int], Any] | None = None,
                    half: dict[Side, float] | None = None, breakable: Callable[[Cell, Cell], Any] | None = None,
                    with_f: bool = True) -> tuple[set[Side], list[list[Point]], list[kymesh.Cell]]:
    """(broken sides, D polylines, F cells) of one area: see features."""
    f = features(cells, pitch, ox, oz, step, eligible, half, breakable, with_f)
    return f.broken, f.lines, f.cells


def f_cells(fset: Iterable[Cell], axis: str, cells: dict[Cell, float], pitch: float, ox: float,
            oz: float) -> list[kymesh.Cell]:
    """F records of one axis ('x' or 'z') of the cells fset: per group (8-connected, in order of its first cell) the
    -axis record, then every group's +axis record (DICE's order). Each is a closed polygon (first point repeated)
    with every corner's height from the area cells (cells) on that side of it."""
    return [kymesh.Cell(d, pts) for d, pts in
            _core().navkit_detail_f_cells(list(fset), axis, cells, float(pitch), float(ox), float(oz))]
