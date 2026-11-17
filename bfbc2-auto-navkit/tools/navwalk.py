"""NavKit: walkable surfaces from in-game collision samples, and scoring against an existing pathdata.hpd.

Rules (research/KYNAPSE_SPEC.md section 3):
  1. a sampled surface is a candidate when the soldier capsule fits and its normal is within
     GroundSlopeMax (40 deg) of vertical;
  2. surfaces lying on a ForbidPathfinding object (forbid_tris.npy from levelobjects.py) are removed;
  3. regions grow 8-connected between neighbouring samples when the height step, relative to the local
     ground plane (from both normals), is at most StepMax (0.28 m);
  4. regions are seeded on the sector perimeter lines (1.6 m inside every 100 m sector edge), on terrain
     and objects alike (terrain-only seeding cut off decks, platforms and building floors: sp_01's AI order
     positions went from 463 to 613 of DICE's 633 with object seeds; terrain_seeds_only=True restores it).

Usage:
  navwalk.py score <samples.bin> <pathdata.hpd> [forbid_tris.npy] [--step 0.28] [--slope 40]
                   scores at the 0.8 m sample grid (a 0.2 m run is subsampled)
"""
import math
import os
import sys
from collections import defaultdict, deque
from collections.abc import Iterable, Sequence
from typing import Any, cast

import numpy as np

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools')
import navsample  # noqa: E402

# The Rust core (rust/bc2-navbuild through bc2_native; navbuild.py's notes): walkable's flood, islands_of and
# matched_flags always run in it (their Python was retired on 2026-11-17: require_native); the walkability rules
# (headroom, stair_slope, narrow_lips, landings, near_objects / on_objects: round E2a) run in it when it is there,
# with their Python here as the reference and the fallback (BFBC2_PURE_PYTHON=1, use_native(False)).

def require_native() -> Any:
    """The Rust core for the floods, or bfbc2core.native.NativeMissing saying how to get it."""
    suite_paths.use('core')
    from bfbc2core import native
    return native.require("NavKit's grid walks (navbuild.py / navwalk.py)", 'NavLevel')


def _load_native() -> Any:
    """The Rust core for the walkability rules (NavGrid), or None: not built, or BFBC2_PURE_PYTHON set."""
    try:
        suite_paths.use('core')
        from bfbc2core import dbx as _dbx
    except ImportError:
        return None
    if os.environ.get(_dbx.PURE_PYTHON_ENV, '') not in ('', '0'):
        return None
    m = _dbx._import_native()
    return m if m is not None and hasattr(m, 'NavLevel') else None


_native = _load_native()
_NATIVE_ERRORS = (ValueError, TypeError, OverflowError)   # the rules: the Rust core refused, the Python code runs


def use_native(on: bool) -> str:
    """The walkability rules in the Rust core (when built, whatever BFBC2_PURE_PYTHON says) or in Python for this
    process; 'rust' or 'python'. The floods are not switched: they have no Python."""
    global _native
    if on:
        saved = os.environ.pop('BFBC2_PURE_PYTHON', None)
        try:
            _native = _load_native()
        finally:
            if saved is not None:
                os.environ['BFBC2_PURE_PYTHON'] = saved
    else:
        _native = None
    return 'rust' if _native is not None else 'python'


def grid_arrays(walk: np.ndarray, gx: np.ndarray, gz: np.ndarray,
                stair: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """walk, gx, gz and stair as the Rust core takes them: the flags as one byte each (a value's truth, as the step
    rule tests them), the gradients little-endian float32 or float64 as they are (step_fits computes in their type);
    gradients of any other type are widened to float64 (no build makes them: they come from float32 normals or
    float64 stair slopes)."""
    def grad(a: np.ndarray) -> np.ndarray:
        a = np.asarray(a)
        return np.ascontiguousarray(a, a.dtype.newbyteorder('<') if a.dtype in (np.float32, np.float64) else '<f8')
    return np.ascontiguousarray(walk, bool), grad(gx), grad(gz), np.ascontiguousarray(stair, bool)


def native_grid(s: navsample.Samples, walk: np.ndarray, gx: np.ndarray, gz: np.ndarray, stair: np.ndarray,
                step: float) -> Any:
    """The samples in the Rust core for the step-rule floods (NavLevel: walk the surfaces it may enter, the
    gradients and stair flags step_fits reads, s.pitch, step; grid_arrays)."""
    walk, gx, gz, stair = grid_arrays(walk, gx, gz, stair)
    return require_native().NavLevel(
        int(s.nx), int(s.nz), np.ascontiguousarray(s.start, '<i8').tobytes(), walk.tobytes(),
        np.ascontiguousarray(s.y, '<f8').tobytes(), gx.tobytes(), gx.dtype == np.float32, gz.tobytes(),
        gz.dtype == np.float32, stair.tobytes(), bytes(len(walk)), float(s.pitch), float(step), 0.6)


def _num(*vals: Any) -> bool:
    """Python numbers (int, float or a float64 scalar): the port computes with them as the reference does. A float32
    pitch or origin would make the reference compute in float32 in places: the Python code runs then."""
    return all(isinstance(v, (int, float)) and not isinstance(v, bool) for v in vals)


def native_samples(s: navsample.Samples) -> Any:
    """The samples in the Rust core for walkable's rules (NavGrid: headroom, stair_slope, narrow_lips, the stair
    landings, near_objects, on_objects), or None: the core is not there or the run's numbers are of a kind the port
    does not take. Heights go in as s.y.astype(float), as the rules read them."""
    if _native is None or not hasattr(_native, 'NavGrid') or not _num(s.pitch, s.min_x, s.min_z):
        return None
    try:
        return _native.NavGrid(int(s.nx), int(s.nz), np.ascontiguousarray(s.start, '<i8').tobytes(),
                               np.ascontiguousarray(s.y, '<f8').tobytes(), False, float(s.pitch), float(s.min_x),
                               float(s.min_z))
    except _NATIVE_ERRORS:
        return None


def _grid_of(s: navsample.Samples, grid: Any) -> Any:
    """grid (a NavGrid walkable made once for all its rules) or one made for s now; None: Python runs."""
    return grid if grid is not None else native_samples(s)


def _column_picks(grid: Any, points: list[Any], mask: np.ndarray, lo: float, hi: float) -> list[int] | None:
    """walkable's surface at each point (seeds, anchors): in the point's column, the surface mask marks with height - y
    in [lo, hi] nearest y, or -1 - from the Rust core (round E3c); None: the caller's loop (the reference) runs."""
    if grid is None or not hasattr(grid, 'column_picks') or not points:
        return None
    try:
        return cast(list[int], grid.column_picks(points, _flags(mask), lo, hi))
    except _NATIVE_ERRORS:
        return None


def _flags(a: Any) -> bytes:
    """A per-surface truth array as one byte each (the reference tests each value's truth)."""
    return np.ascontiguousarray(a, bool).tobytes()


def _tris_f64(tris: Any) -> bytes | None:
    """Triangles (n x 3 x 3) as float64 bytes, as tris.astype(float) holds them; None for another shape."""
    t = np.asarray(tris)
    if t.ndim != 3 or t.shape[1:] != (3, 3):
        return None
    return np.ascontiguousarray(t, '<f8').tobytes()

TERRAIN_SHAPE_VTABLE = 0x1454D14   # shape class of terrain hits (navsample version 2 hit info), build 795745
CHARACTER_SHAPE_VTABLE = 0x1483378  # shape class of soldiers standing in the level while it is sampled (their spawns):
                                    # hits 1.3-1.5 m over the floor at the spawn points (sp_006: player, Sweetwater,
                                    # Redford; sp_004_b: player), 137 soldier-sized clusters on sp_004_b; build 795745
HEADROOM_BAND: tuple[float, float] | None       # None: off (also LIP_WIDTH, LIP_OBJECT_RISE, DYNAMIC_FLOOR)
HEADROOM_BAND = (0.5, 2.0)          # ground with the next sampled surface above it this much higher is not walkable: DICE
                                    # keeps almost none (sp_004_b and sp_01, NavKit ground cells against DICE's: 0.4-0.5 m
                                    # above 50-52 % kept, 0.5-1.2 m 0-18 %, 1.2-2.0 m 0-1 %; from 2.0 m sp_004_b 32-94 %).
                                    # Under 0.5 m: thin things (steps, rails) - kept. None to switch off. (2026-10-19)
CHARACTERS_ABSENT = True            # soldiers are not in path data: their surfaces are not floor and the ground under
                                    # and beside them is not blocked by them (as dynamic props) - before 2026-10-19 the
                                    # squad's spawn cells were left out, and in game the squad could not path to the
                                    # player (sp_004_b NavKit play test: 7 of 73 go-to requests found a path)
STEEP_TERRAIN = False              # terrain whose sampled normal is steeper than the slope limit (up to
                                    # STEEP_TERRAIN_MAX) is walkable when its slope from the heights STEEP_TERRAIN_SPAN m
                                    # either side is within the limit: DICE judges slope over a distance, not by a terrain
                                    # triangle's normal (sp_03, 0.8 m samples: 1.9 % of DICE's ground has a normal over
                                    # 40 deg, median 36.8 deg from the heights). The 0.8 m grid stepped over such bands; at
                                    # 0.2 m every column of one failed and cut valley-to-plateau routes (sp_03 detailed
                                    # build: connectivity 98.8 -> 63.2, 2026-10-22). Off: on sp_03 it raised ground
                                    # 97.3 -> 98.9 but extra ground 6.2 -> 12.0 and over-connection 19 -> 42 %, and
                                    # connectivity only 63.2 -> 63.8 - those routes were cut by something else
STEEP_TERRAIN_MAX = 55.0
LIP_WIDTH: int | None
LIP_WIDTH = 2                      # pitch < 0.5 only: a terrain sample steeper than the slope limit (up to
                                    # STEEP_TERRAIN_MAX) is walkable when walkable ground lies within LIP_WIDTH samples on
                                    # both opposite sides (an axis or a diagonal) with a slope within the limit between them
                                    # - a narrow lip. The 0.8 m step rule steps over such lips; at 0.2 m one cut sp_007's
                                    # north strip off (connectivity 96.2 -> 81.2, 2026-10-22). DICE at pitch 0.2 covers
                                    # 81-86 % of terrain under 40 deg and 15 % at 40-45, so a blanket rule is wrong
                                    # (STEEP_TERRAIN); of the lips this accepts DICE covers 40 % (sp_007 window: +0.9 %
                                    # ground, +1.4 % extra; width 3: +1.3 / +3.0). None to switch off
LIP_OBJECT_RISE: float | None
LIP_OBJECT_RISE = 0.3              # ... and on objects when the steep sample is within this of the walkable ground on
                                    # both sides: door sills and thresholds (sp_009's aircraft: a 5 cm sill with steep
                                    # sides at every hull-section seam cut the aisle at pitch 0.2), never wall tops or
                                    # ledges; None for terrain only
LIP_OBJECT_SLOPE_MAX = None        # a slope cap (deg) on object lips; None: any steepness, above STEEP_TERRAIN_MAX
                                    # only with flat floor on both sides (LIP_OBJECT_FLAT deg; 2026-10-22 evening)
LIP_OBJECT_FLAT = 15.0
ROOF_PRUNE = True                  # drop roof islands nothing reaches: a kept island (step-rule connected) that
                                    # holds no anchor (soldier spawn, AI order position, ladder end, or ground the
                                    # normal build kept - stage b), is not the largest, is mostly raised (ROOF_SHARE of
                                    # it on objects with a surface ROOF_RAISE m or more below), and has no anchored
                                    # ground within ROOF_NEAR m at its own height (ROOF_NEAR_DY). Ground-level islands
                                    # stay: DICE keeps those (all 15 levels, 2026-10-22: dropping unanchored islands
                                    # under 25 nodes cost as much DICE ground as extra it removed), but not detached
                                    # roofs and scenery tops (this rule: mean -1.31 points extra for -0.28 shared;
                                    # sp_008 -7.4 / -2.25, sp_01 -3.6 / -0.01)
ROOF_RAISE = 2.0
ROOF_SHARE = 0.5
ROOF_NEAR = 2.0
ROOF_NEAR_DY = 2.5
ROOF_STATS: dict[str, int] = {}
DYNAMIC_ABSENT_HEADROOM = True     # dynamic props do not count as a ceiling for HEADROOM_BAND (they are absent for path
                                    # data - a closed gate's leaves made the floor beside it 'low headroom')
DYNAMIC_FLOOR: float | None
DYNAMIC_FLOOR = 0.3                # a surface on a dynamic prop within this of walkable floor in a neighbouring column
                                    # stands for the floor under the prop (the sampler records only a prop lying on the
                                    # floor, e.g. a gate's bottom rail 0.09 m up - its columns were a hole, and at pitch
                                    # 0.2 a closed gate cut the only way through a wall: sp_003_b's monastery gate,
                                    # 2026-10-22); None to switch off
OBJECTIVE_FLOOR = True             # an AI order position on a ForbidPathfinding object keeps the floor under and around
                                    # it: the floor whose forbidden top stands 0.3..2 m above it, and the floor at that
                                    # height within OBJECTIVE_REACH that the object blocks by clearance. M-COMs
                                    # (mpobjective) are ForbidPathfinding; the order positions where bots arm them stood
                                    # in its hole - 14 of 15 Onslaught M-COM targets were on no area (2026-10-23)
OBJECTIVE_REACH = 1.5
OBJECTIVE_STATS: dict[str, int] = {}
STEEP_TERRAIN_SPAN = 0.8
STEEP_STATS: dict[str, int] = {}
OUTLINE_Y_OFFSET = 0.95            # outline heights in pathdata.hpd = ground + EntityHeight / 2
PITCH = 0.8
SECTOR = 100.0


def coarse(f: navsample.Samples) -> navsample.Samples:
    """Every (PITCH / f.pitch)-th column of a fine run, keeping all surfaces of the column."""
    step = int(round(PITCH / f.pitch))
    nx, nz = (f.nx - 1) // step + 1, (f.nz - 1) // step + 1
    picked: list[int]
    start, picked = [0], []
    for j in range(nz):
        for i in range(nx):
            c = (j * step) * f.nx + i * step
            picked.extend(range(f.start[c], f.start[c + 1]))
            start.append(len(picked))
    idx = np.array(picked, dtype=np.int64)
    hit = f.hit[idx] if f.hit is not None and len(f.hit) else None
    return navsample.Samples(PITCH, f.min_x, f.min_z, nx, nz, f.y_top, f.y_bottom, f.radius, f.cap_bottom,
                             f.cap_top, f.ray_radius, np.array(start), f.y[idx], f.normal[idx], f.flags[idx], hit)


def on_objects(s: navsample.Samples, tris: np.ndarray | None, tol: float = 0.2, grid: Any = None) -> np.ndarray:
    """Surfaces lying on one of the given world-space triangles (within tol metres vertically)."""
    out = np.zeros(len(s.y), bool)
    if tris is None or not len(tris):
        return out
    x0, x1 = s.min_x - 2, s.min_x + s.nx * s.pitch + 2
    z0, z1 = s.min_z - 2, s.min_z + s.nz * s.pitch + 2
    t = tris[(tris[:, :, 0].max(1) >= x0) & (tris[:, :, 0].min(1) <= x1) &
             (tris[:, :, 2].max(1) >= z0) & (tris[:, :, 2].min(1) <= z1)].astype(float)
    g = _grid_of(s, grid) if _num(tol) else None
    t64 = _tris_f64(t) if g is not None and len(t) else None
    if g is not None and t64 is not None:
        try:
            return np.frombuffer(g.on_objects(t64, float(tol)), np.bool_).copy()
        except _NATIVE_ERRORS:
            pass
    y = s.y.astype(float)
    for (ax, ay, az), (bx, by, bz), (cx, cy, cz) in t:
        det = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz)
        if abs(det) < 1e-9:
            continue
        i0 = max(0, math.ceil((min(ax, bx, cx) - s.min_x) / s.pitch))
        i1 = min(s.nx - 1, math.floor((max(ax, bx, cx) - s.min_x) / s.pitch))
        j0 = max(0, math.ceil((min(az, bz, cz) - s.min_z) / s.pitch))
        j1 = min(s.nz - 1, math.floor((max(az, bz, cz) - s.min_z) / s.pitch))
        for j in range(j0, j1 + 1):
            pz = s.min_z + j * s.pitch
            for i in range(i0, i1 + 1):
                px = s.min_x + i * s.pitch
                l1 = ((bz - cz) * (px - cx) + (cx - bx) * (pz - cz)) / det
                l2 = ((cz - az) * (px - cx) + (ax - cx) * (pz - cz)) / det
                if l1 < -1e-6 or l2 < -1e-6 or 1 - l1 - l2 < -1e-6:
                    continue
                yt = l1 * ay + l2 * by + (1 - l1 - l2) * cy
                c = j * s.nx + i
                for k in range(s.start[c], s.start[c + 1]):
                    if abs(y[k] - yt) <= tol:
                        out[k] = True
    return out


def near_characters(s: navsample.Samples, character: np.ndarray, grow: float = 0.9, below: float = 2.2,
                    above: float = 0.3, grid: Any = None) -> np.ndarray:
    """Surfaces a capsule test would see blocked by a soldier: within `grow` metres (horizontally) of a character hit
    (CHARACTER_SHAPE_VTABLE) and from `below` metres under to `above` metres over it. In the Rust core when it is there
    (round E3c; grid: native_samples(s); the Python below is its reference and fallback)."""
    g = _grid_of(s, grid)
    if g is not None and hasattr(g, 'near_characters') and _num(grow, below, above):
        try:
            return np.frombuffer(g.near_characters(_flags(character), float(grow), float(below), float(above)),
                                 np.bool_).copy()
        except _NATIVE_ERRORS:
            pass
    out = np.zeros(len(s.y), bool)
    y = s.y.astype(float)
    col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start[:s.nx * s.nz + 1]))
    r = int(math.floor(grow / s.pitch + 1e-9))
    for k in np.nonzero(character)[0]:
        c = col[k]
        i, j = c % s.nx, c // s.nx
        for dj in range(-r, r + 1):
            for di in range(-r, r + 1):
                a, b = i + di, j + dj
                if 0 <= a < s.nx and 0 <= b < s.nz and math.hypot(di, dj) * s.pitch <= grow + 1e-9:
                    cc = b * s.nx + a
                    for q in range(s.start[cc], s.start[cc + 1]):
                        if y[k] - below <= y[q] <= y[k] + above:
                            out[q] = True
    return out


def headroom(s: navsample.Samples, ignore: np.ndarray | None = None, grid: Any = None) -> np.ndarray:
    """Per surface: the gap to the next sampled surface above it in its column (inf when none); surfaces flagged in
    `ignore` (soldiers) do not count as above. grid: native_samples(s), made once by the caller (else here)."""
    g = _grid_of(s, grid)
    if g is not None:
        try:
            return np.frombuffer(g.headroom(None if ignore is None else _flags(ignore)), '<f8').astype(np.float64)
        except _NATIVE_ERRORS:
            pass
    y = s.y.astype(float)
    out = np.full(len(y), np.inf)
    ign = ignore if ignore is not None else np.zeros(len(y), bool)
    for c in range(s.nx * s.nz):
        a, b = s.start[c], s.start[c + 1]
        if b - a < 2:
            continue
        ks = [k for k in range(a, b) if not ign[k]]
        if len(ks) < 2:
            continue
        ys = np.sort(y[ks])
        for k in ks:
            higher = ys[ys > y[k] + 0.05]
            if len(higher):
                out[k] = higher[0] - y[k]
    return out


def low_headroom(s: navsample.Samples, ignore: np.ndarray | None, lo: float, hi: float,
                 grid: Any = None) -> np.ndarray:
    """Surfaces with the next surface above them between lo and hi metres higher (HEADROOM_BAND)."""
    h = headroom(s, ignore, grid)
    return (h >= lo) & (h < hi)


def near_points(s: navsample.Samples, points: Iterable[Sequence[float]], grow: float = 0.9, below: float = 0.5,
                above: float = 0.5) -> np.ndarray:
    """Surfaces within `grow` metres (horizontally) of a point and from `below` under to `above` over its height."""
    out = np.zeros(len(s.y), bool)
    y = s.y.astype(float)
    r = int(math.floor(grow / s.pitch + 1e-9))
    for px, py, pz in points:
        i0, j0 = int(round((px - s.min_x) / s.pitch)), int(round((pz - s.min_z) / s.pitch))
        for dj in range(-r - 1, r + 2):
            for di in range(-r - 1, r + 2):
                a, b = i0 + di, j0 + dj
                if not (0 <= a < s.nx and 0 <= b < s.nz):
                    continue
                if math.hypot(s.min_x + a * s.pitch - px, s.min_z + b * s.pitch - pz) > grow + 1e-9:
                    continue
                c = b * s.nx + a
                for q in range(s.start[c], s.start[c + 1]):
                    if py - below <= y[q] <= py + above:
                        out[q] = True
    return out


def near_objects(s: navsample.Samples, tris: np.ndarray | None, grow: float = 0.5, below: float = 2.2,
                 above: float = 0.3, grid: Any = None) -> np.ndarray:
    """Surfaces within `grow` metres (horizontally) of a triangle's bounding box and from `below` metres under
    to `above` metres over it - the ground a capsule test around an object would see as blocked."""
    out = np.zeros(len(s.y), bool)
    if tris is None or not len(tris):
        return out
    g = _grid_of(s, grid) if _num(grow, below, above) else None
    t64 = _tris_f64(tris) if g is not None else None
    if g is not None and t64 is not None:
        try:
            return np.frombuffer(g.near_objects(t64, float(grow), float(below), float(above)), np.bool_).copy()
        except _NATIVE_ERRORS:
            pass
    y = s.y.astype(float)
    for t in tris.astype(float):
        x0, x1 = t[:, 0].min() - grow, t[:, 0].max() + grow
        z0, z1 = t[:, 2].min() - grow, t[:, 2].max() + grow
        i0 = max(0, math.ceil((x0 - s.min_x) / s.pitch)); i1 = min(s.nx - 1, math.floor((x1 - s.min_x) / s.pitch))
        j0 = max(0, math.ceil((z0 - s.min_z) / s.pitch)); j1 = min(s.nz - 1, math.floor((z1 - s.min_z) / s.pitch))
        y0, y1 = t[:, 1].min() - below, t[:, 1].max() + above
        for j in range(j0, j1 + 1):
            for i in range(i0, i1 + 1):
                c = j * s.nx + i
                for k in range(s.start[c], s.start[c + 1]):
                    if y0 <= y[k] <= y1:
                        out[k] = True
    return out


def stair_slope(s: navsample.Samples, mask: np.ndarray, r: int = 1, tol: float = 0.7,
                grid: Any = None) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Slope (degrees) from neighbouring sample heights on the same layer (central differences, +-1 sample),
    for the surfaces in mask; 90 where a neighbour is missing. (Measured over +-0.8 m at pitch 0.2 it recognised far
    more stairs, but with a 0.5 m step on them it joined decks and ledges to streets - over-connection 2.2 -> 35.3 % on
    ONS_MP_002; not adopted, 2026-10-21.)"""
    g = _grid_of(s, grid) if isinstance(r, int) and not isinstance(r, bool) and _num(tol) else None
    if g is not None:
        try:
            got = g.stair_slope(_flags(mask), r, float(tol))
            return tuple(np.frombuffer(b, '<f8').astype(np.float64) for b in got)    # type: ignore[return-value]
        except _NATIVE_ERRORS:
            pass
    y = s.y.astype(float)
    col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
    out = np.full(len(y), 90.0)
    gxo = np.zeros(len(y)); gzo = np.zeros(len(y))

    def layer(i: int, j: int, yref: float) -> float | None:
        if not (0 <= i < s.nx and 0 <= j < s.nz):
            return None
        c = j * s.nx + i
        best = None
        for k in range(s.start[c], s.start[c + 1]):
            if abs(y[k] - yref) <= tol and (best is None or abs(y[k] - yref) < abs(best - yref)):
                best = y[k]
        return best
    for k in np.nonzero(mask)[0]:
        c = col_of[k]
        i, j = c % s.nx, c // s.nx
        a, b, cc, d = layer(i + r, j, y[k]), layer(i - r, j, y[k]), layer(i, j + r, y[k]), layer(i, j - r, y[k])
        if a is None and b is None and cc is None and d is None:
            continue

        def diff(p: float | None, m: float | None, k: int = k) -> float:   # central difference, or one-sided where a neighbour is missing (narrow stairs)
            if p is not None and m is not None:
                return (p - m) / (2 * r * s.pitch)
            if p is not None:
                return (p - y[k]) / (r * s.pitch)
            if m is not None:
                return (y[k] - m) / (r * s.pitch)
            return 0.0
        gxo[k] = diff(a, b)
        gzo[k] = diff(cc, d)
        out[k] = math.degrees(math.atan(math.hypot(gxo[k], gzo[k])))
    return out, gxo, gzo


def narrow_lips(s: navsample.Samples, base: np.ndarray, cand: np.ndarray, slope: float, width: int, rise: float | None = None,
                flat: float | None = None, grid: Any = None) -> np.ndarray:
    """The cand surfaces (steep terrain) with a base surface (walkable) within width samples on both opposite sides
    along an axis or a diagonal - same layer, each step within 0.7 m - and a slope within the limit between those two
    (LIP_WIDTH). rise: the candidate also within this height of both of those (LIP_OBJECT_RISE); flat: both of those
    within this many degrees of level."""
    ny = s.normal[:, 1]
    ok = (isinstance(width, int) and not isinstance(width, bool) and _num(slope)
          and (rise is None or _num(rise)) and (flat is None or (_num(flat) and ny.dtype in (np.float32, np.float64))))
    g = _grid_of(s, grid) if ok else None
    if g is not None:
        try:
            fl = None
            if flat is not None:
                f32 = ny.dtype == np.float32
                fl = (np.ascontiguousarray(ny, '<f4' if f32 else '<f8').tobytes(), f32, math.cos(math.radians(flat)))
            got = g.narrow_lips(_flags(base), _flags(cand), math.tan(math.radians(slope)), width,
                                None if rise is None else float(rise), fl)
            return np.frombuffer(got, np.bool_).copy()
        except _NATIVE_ERRORS:
            pass
    y = s.y.astype(float)
    col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
    lim = math.tan(math.radians(slope))
    out = np.zeros(len(y), bool)

    def side(i: int, j: int, yref: float, di: int, dj: int) -> tuple[int, int] | None:
        for t in range(1, width + 1):
            a, b = i + di * t, j + dj * t
            if not (0 <= a < s.nx and 0 <= b < s.nz):
                return None
            c = b * s.nx + a
            best = None
            for n in range(s.start[c], s.start[c + 1]):
                if abs(y[n] - yref) <= 0.7 * t and (best is None or abs(y[n] - yref) < abs(y[best] - yref)):
                    best = n
            if best is None:
                return None
            if base[best]:
                return best, t
        return None
    for k in np.nonzero(cand)[0]:
        c = col_of[k]
        i, j = c % s.nx, c // s.nx
        for di, dj in ((1, 0), (0, 1), (1, 1), (1, -1)):
            p = side(i, j, y[k], di, dj)
            if p is None:
                continue
            m = side(i, j, y[k], -di, -dj)
            if m is None:
                continue
            d = (p[1] + m[1]) * s.pitch * (math.sqrt(2) if di and dj else 1)
            if rise is not None and (abs(y[k] - y[p[0]]) > rise or abs(y[k] - y[m[0]]) > rise):
                continue
            if flat is not None and min(s.normal[p[0], 1], s.normal[m[0], 1]) < math.cos(math.radians(flat)):
                continue
            if abs(y[p[0]] - y[m[0]]) / d <= lim:
                out[k] = True
                break
    return out


def matched_flags(s: navsample.Samples, other: navsample.Samples | None) -> np.ndarray:
    """Capsule flags of a second run over the same grid (e.g. a higher capsule), matched per column by height: each
    surface takes the flags of the first surface of its column in the other run within 0.05 m of its height, else
    1 (uint8 per surface); in the Rust core. The flags go in as uint8 (a run's flags are; others are cast)."""
    if other is None:
        return np.full(len(s.y), 1, dtype=np.uint8)
    got = require_native().navkit_matched_flags(
        s.nx * s.nz, np.ascontiguousarray(s.start, '<i8').tobytes(), np.asarray(s.y, '<f8').tobytes(),
        np.ascontiguousarray(other.start, '<i8').tobytes(), np.asarray(other.y, '<f8').tobytes(),
        np.ascontiguousarray(other.flags, np.uint8).tobytes())
    return np.frombuffer(got, np.uint8).copy()


def step_fits(dy: float, gk: Sequence[float], gn: Sequence[float], sk: bool, sn: bool, di: int, dj: int, pitch: float,
              step: float) -> bool:
    """Height change dy from surface k to its neighbour n (direction di, dj) fits the step rule: within step of the
    rise predicted by the mean local gradient - or, when one of them is a stair surface, by the stair's own
    incline (a stair sample next to a flat landing would otherwise halve the predicted rise of the last step)."""
    if abs(dy - pitch * ((gk[0] + gn[0]) / 2 * di + (gk[1] + gn[1]) / 2 * dj)) <= step:
        return True
    return any(st and abs(dy - pitch * (g[0] * di + g[1] * dj)) <= step for g, st in ((gk, sk), (gn, sn)))


LADDER_REACH = 1.5              # a ladder's ends: the walkable surface nearest the ladder within this (m, horizontally)
LADDER_FOOT = (-1.0, 1.0)       # ... at the foot: from 1 m below to 1 m above the ladder's bottom
LADDER_TOP = (-1.5, 0.5)        # ... at the top: 1.5 m below to 0.5 m above its top (its rails stick out over the deck)


def ladder_ends(s: navsample.Samples, ok: np.ndarray, keep: np.ndarray, lad: dict[str, Any],
                reach: float = LADDER_REACH) -> tuple[int | None, int | None]:
    """(foot surface, top surface) of a ladder {'lo', 'hi'} (ladders.py): at each end the surface allowed by the rules
    (ok) nearest the ladder's box, within reach horizontally and the end's height band; kept ground first. None where
    there is none."""
    lo, hi = lad['lo'], lad['hi']
    i0, i1 = int(math.floor((lo[0] - reach - s.min_x) / s.pitch)), int(math.ceil((hi[0] + reach - s.min_x) / s.pitch))
    j0, j1 = int(math.floor((lo[2] - reach - s.min_z) / s.pitch)), int(math.ceil((hi[2] + reach - s.min_z) / s.pitch))
    out = []
    for y0, y1 in ((lo[1] + LADDER_FOOT[0], lo[1] + LADDER_FOOT[1]), (hi[1] + LADDER_TOP[0], hi[1] + LADDER_TOP[1])):
        best = None
        for j in range(max(j0, 0), min(j1, s.nz - 1) + 1):
            for i in range(max(i0, 0), min(i1, s.nx - 1) + 1):
                x, z = s.min_x + i * s.pitch, s.min_z + j * s.pitch
                d = math.hypot(max(lo[0] - x, 0, x - hi[0]), max(lo[2] - z, 0, z - hi[2]))
                if d > reach:
                    continue
                c = j * s.nx + i
                for k in range(s.start[c], s.start[c + 1]):
                    if ok[k] and y0 <= s.y[k] <= y1:
                        key = (not keep[k], d)
                        if best is None or key < best[0]:
                            best = (key, k)
        out.append(best[1] if best else None)
    return out[0], out[1]


def objective_floor(s: navsample.Samples, onf: np.ndarray, ok: np.ndarray, character: np.ndarray, slope_ok: np.ndarray,
                    points: Iterable[Sequence[float]], reach: float | None = None) -> np.ndarray:
    """OBJECTIVE_FLOOR: the surfaces to add (bool per surface) for the AI order positions lying on a forbidden object."""
    reach = OBJECTIVE_REACH if reach is None else reach
    y = s.y.astype(float)
    add = np.zeros(len(y), bool)
    r = int(math.ceil(reach / s.pitch))
    for (px, py, pz) in points:
        i0, j0 = int(round((px - s.min_x) / s.pitch)), int(round((pz - s.min_z) / s.pitch))
        cols = []
        for dj in range(-r, r + 1):
            for di in range(-r, r + 1):
                i, j = i0 + di, j0 + dj
                if 0 <= i < s.nx and 0 <= j < s.nz and math.hypot(di, dj) * s.pitch <= reach:
                    c = j * s.nx + i
                    cols.append(range(s.start[c], s.start[c + 1]))
        # on a forbidden object: a forbidden surface around the position, from 0.5 m below it to 2.5 m above
        if not any(onf[n] and -0.5 <= y[n] - py <= 2.5 for ns in cols for n in ns):
            continue
        # the floor under it: surfaces near the position's height with a forbidden top 0.3..2 m above. They may count
        # as on the object themselves - an M-COM's collision has a bottom face on the ground, so the ground under it
        # lies on a forbidden triangle (ONS_MP_002 / 004: 9 of 14 targets missed until 2026-10-23)
        def under_top(n: int, ns: range) -> bool:
            return any(onf[m] and 0.3 <= y[m] - y[n] <= 2.0 for m in ns)
        under = [y[n] for ns in cols for n in ns if slope_ok[n] and abs(y[n] - py) <= 1.5 and under_top(n, ns)]
        if not under:
            continue
        # the layer nearest the position's height: an object can stand on a slab above the terrain, and the terrain
        # under the slab has the object's top within 2 m too - a median over both took the terrain (ONS_MP_002
        # "Arm A3": slab 0.9 m above the terrain, 2026-10-24). Layers are 0.1 m height bins with at least 2 hits and a
        # quarter of the fullest bin's: a few hits on the object's sides sit nearer a position on the object's top than
        # the floor does (ONS_MP_004: two targets 0.8 m above their floor, stray hits 0.2-0.6 m under them)
        b, cnt = np.unique(np.round(np.asarray(under) / 0.1).astype(int), return_counts=True)
        layers = b[cnt >= max(2, 0.25 * cnt.max())] * 0.1
        if not len(layers):
            layers = b * 0.1
        nearest = float(layers[np.argmin(np.abs(layers - py))])
        floor = float(np.median([v for v in under if abs(v - nearest) <= 0.2]))
        n_add = 0
        for ns in cols:
            for n in ns:
                if (not ok[n] and not character[n] and slope_ok[n] and abs(y[n] - floor) <= 0.3
                        and (not onf[n] or under_top(n, ns))):
                    add[n] = True
                    n_add += 1
        OBJECTIVE_STATS['points'] = OBJECTIVE_STATS.get('points', 0) + 1
        OBJECTIVE_STATS['surfaces'] = OBJECTIVE_STATS.get('surfaces', 0) + n_add
    return add


def walkable(s: navsample.Samples, forbid_tris: np.ndarray | None = None, slope: float = 40.0, step: float = 0.28,
             terrain_seeds_only: bool = False, dynamic_tris: np.ndarray | None = None,
             high_capsule: navsample.Samples | None = None, stairs: bool = True, return_gradients: bool = False,
             narrow_capsule: navsample.Samples | None = None, raised_bars: bool = False,
             stair_tris: np.ndarray | None = None, exclude_tris: np.ndarray | None = None,
             exclude_footprint_tris: np.ndarray | None = None, forbid_tol: float = 0.08,
             extra_seeds: Iterable[Sequence[float]] | None = None, return_stairs: bool = False,
             force_walkable: np.ndarray | None = None, force_unwalkable: np.ndarray | None = None,
             unseeds: Iterable[Sequence[float]] | None = None, soldier_points: Sequence[Sequence[float]] | None = None,
             ladders: Sequence[dict[str, Any]] | None = None, anchors: Iterable[Sequence[float]] | None = None,
             objective_points: Sequence[Sequence[float]] | None = None, grid_out: list[Any] | None = None) -> Any:
    """Kept walkable surfaces (bool per surface) by rules 1-4, plus the structure rules:
      dynamic props (dynamic_tris) are absent for path data - their tops are not floor, and the ground under and
      right beside them is not blocked by them;
      stairs: object surfaces whose slope over neighbouring samples is 12..40 deg count as walkable, with the
      clearance of the high_capsule run (capsule from 0.6 m) instead of the normal one.
    Hand edits (navedits.py): force_walkable / force_unwalkable (bool per surface) override the rules before
    seeding; unseeds (points) drop the island each lies on after seeding. The floods need the Rust core
    (require_native, checked first). grid_out: a list the floods' NavLevel is appended to (the samples, gradients and
    stairs in the core: navbuild.stage makes its Level's from it - NavLevel.with_walk - instead of sending them again)."""
    require_native()
    y = s.y.astype(float)
    col_of_w = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))     # each surface's column
    nat = native_samples(s)                  # the run in the Rust core for the rules below (None: Python)
    ny = np.maximum(s.normal[:, 1], 0.05)
    gx_all, gz_all = -s.normal[:, 0] / ny, -s.normal[:, 2] / ny
    stair = np.zeros(len(y), bool)
    terrain = (s.hit[:, 0] == TERRAIN_SHAPE_VTABLE) if s.hit is not None else np.ones(len(y), bool)
    clear = (s.flags & 3) == 0
    character = (s.hit[:, 0] == CHARACTER_SHAPE_VTABLE) if (s.hit is not None and CHARACTERS_ABSENT) else np.zeros(len(y), bool)
    if character.any():
        clear |= near_characters(s, character, grid=nat) & ((s.flags & 2) == 0)
    if soldier_points is not None and len(soldier_points) and CHARACTERS_ABSENT:
        # soldiers stood at their spawn points while the level was sampled; a downward ray only sees one where it hits
        # the top of his body (sp_006: Haggard's was missed, his spawn cell still blocked) - so the floor at every
        # soldier spawn point is not blocked by him either
        clear |= near_points(s, soldier_points) & ((s.flags & 2) == 0)
    if dynamic_tris is not None and len(dynamic_tris):
        clear |= near_objects(s, dynamic_tris, grid=nat) & ((s.flags & 2) == 0)
    if exclude_tris is not None and len(exclude_tris):
        # excluded objects do not block clearance; their upper parts are not floor
        clear |= near_objects(s, exclude_tris, grid=nat) & ((s.flags & 2) == 0)
    ok = clear & (s.normal[:, 1] >= math.cos(math.radians(slope)) - 0.01) & ~character
    if STEEP_TERRAIN:
        steep = terrain & clear & ~character & ~ok & (s.normal[:, 1] >= math.cos(math.radians(STEEP_TERRAIN_MAX)))
        if steep.any():
            r = max(1, int(round(STEEP_TERRAIN_SPAN / s.pitch)))
            gs_t, tgx, tgz = stair_slope(s, steep, r=r, tol=max(0.7, 1.2 * r * s.pitch), grid=nat)
            accept = steep & (gs_t <= slope)
            ok |= accept
            # the link rule predicts steps from these gradients: the heights', not the steep normals'
            gx_all = np.where(accept, tgx, gx_all)
            gz_all = np.where(accept, tgz, gz_all)
            STEEP_STATS['accepted'] = STEEP_STATS.get('accepted', 0) + int(accept.sum())
            STEEP_STATS['candidates'] = STEEP_STATS.get('candidates', 0) + int(steep.sum())
    if LIP_WIDTH and s.pitch < 0.5:
        steep_c = clear & ~character & ~ok & (s.normal[:, 1] >= math.cos(math.radians(STEEP_TERRAIN_MAX)))
        lips = narrow_lips(s, ok, terrain & steep_c, slope, LIP_WIDTH, grid=nat)
        if LIP_OBJECT_RISE is not None:
            # on objects any steepness: the height limit alone keeps it to sills (an aircraft door sill's sides are
            # steeper than 55 deg - sp_009's aft cabin stayed cut off under the terrain cap)
            obj_c = ~terrain & clear & ~character & ~ok
            lips |= narrow_lips(s, ok, obj_c & steep_c, slope, LIP_WIDTH, rise=LIP_OBJECT_RISE, grid=nat)
            if not LIP_OBJECT_SLOPE_MAX:
                # steeper than STEEP_TERRAIN_MAX: only between flat floors (a door sill's sides; a rock mesh meeting
                # sloped terrain is not a sill - sp_007's cliff top joined the level through one, 2026-10-22)
                lips |= narrow_lips(s, ok, obj_c & ~steep_c, slope, LIP_WIDTH, rise=LIP_OBJECT_RISE,
                                    flat=LIP_OBJECT_FLAT, grid=nat)
        if lips.any():
            _gs, lgx, lgz = stair_slope(s, lips, grid=nat)
            ok |= lips
            gx_all = np.where(lips, lgx, gx_all)
            gz_all = np.where(lips, lgz, gz_all)
        STEEP_STATS['lips'] = STEEP_STATS.get('lips', 0) + int(lips.sum())
    on_stair = np.zeros(len(y), bool)
    if stair_tris is not None and len(stair_tris):
        # staircase objects: surfaces on / right beside them are not blocked by the staircase itself
        on_stair = near_objects(s, stair_tris, grow=0.4, below=0.6, above=0.6, grid=nat) & ~terrain & ((s.flags & 2) == 0)
    if stairs:
        cand = ~terrain & ((s.flags & 2) == 0)
        gs, hgx, hgz = stair_slope(s, cand, grid=nat)
        stair = cand & (gs >= 12) & (gs <= slope)
        stair_obj = on_stair & (gs <= 45)
        ok |= stair_obj
        stair |= stair_obj & (gs >= 12)
        hi = matched_flags(s, high_capsule) if high_capsule is not None else s.flags
        # landings: object surfaces within two samples of a stair surface on the same level also take the
        # high-capsule clearance (the normal capsule touches the top steps / rail ends there)
        landing = np.zeros(len(y), bool)
        col_of_l = col_of_w
        stair_left = np.nonzero(stair)[0]
        if nat is not None:
            try:
                landing = np.frombuffer(nat.landing(_flags(stair), _flags(cand)), np.bool_).copy()
                stair_left = stair_left[:0]
            except _NATIVE_ERRORS:
                pass
        for k in stair_left:
            c = col_of_l[k]
            i, j = c % s.nx, c // s.nx
            for dj in range(-2, 3):
                for di in range(-2, 3):
                    a, b = i + di, j + dj
                    if 0 <= a < s.nx and 0 <= b < s.nz:
                        cc = b * s.nx + a
                        for n in range(s.start[cc], s.start[cc + 1]):
                            if cand[n] and abs(y[n] - y[k]) <= 0.9:
                                landing[n] = True
        ok |= (stair | (landing & (s.normal[:, 1] >= math.cos(math.radians(slope)) - 0.01))) & ((hi & 3) == 0)
        # on stairs the treads are flat: take the incline from the neighbour heights, not the tread normals
        gx_all = np.where(stair, hgx, gx_all)
        gz_all = np.where(stair, hgz, gz_all)
    onf = on_objects(s, forbid_tris, tol=forbid_tol, grid=nat)
    slope_ok = s.normal[:, 1] >= math.cos(math.radians(slope)) - 0.01

    def same_layer(i: int, j: int, yref: float, tol: float = 0.7) -> list[int]:
        if not (0 <= i < s.nx and 0 <= j < s.nz):
            return []
        c = j * s.nx + i
        return [n for n in range(s.start[c], s.start[c + 1]) if abs(y[n] - yref) <= tol]
    if raised_bars:
        # a forbidden object's raised bar (barrier arm, low overhang) 0.7..1.9 m above the ground within one
        # sample does not block clearance there; the bar itself stays forbidden (onf)
        bar = np.zeros(len(y), bool)
        for k in np.nonzero(onf & ~terrain)[0]:
            c = col_of_w[k]
            i, j = c % s.nx, c // s.nx
            for dj in (-1, 0, 1):
                for di in (-1, 0, 1):
                    a, b = i + di, j + dj
                    if not (0 <= a < s.nx and 0 <= b < s.nz):
                        continue
                    cc = b * s.nx + a
                    for n in range(s.start[cc], s.start[cc + 1]):
                        if 0.7 <= y[k] - y[n] <= 1.9 and (s.flags[n] & 2) == 0:
                            bar[n] = True
        ok |= bar & slope_ok & ~onf
    if narrow_capsule is not None:
        # narrow passages: blocked for the normal capsule, clear for the narrow one, with walkable floor on the
        # same level on opposite sides (left/right or front/back)
        nf = matched_flags(s, narrow_capsule)
        base = ok.copy()
        for k in np.nonzero(~base & ((nf & 3) == 0) & slope_ok & ((s.flags & 2) == 0) & ~onf)[0]:
            c = col_of_w[k]
            i, j = c % s.nx, c // s.nx
            for (d1, d2) in (((1, 0), (-1, 0)), ((0, 1), (0, -1)), ((1, 1), (-1, -1)), ((1, -1), (-1, 1))):
                if any(base[n] for n in same_layer(i + d1[0], j + d1[1], y[k])) and                         any(base[n] for n in same_layer(i + d2[0], j + d2[1], y[k])):
                    ok[k] = True
                    break
    ok &= ~onf
    ondyn = on_objects(s, dynamic_tris, grid=nat) if dynamic_tris is not None and len(dynamic_tris) else np.zeros(len(y), bool)
    if ondyn.any() or (dynamic_tris is not None and len(dynamic_tris)):
        # steep hits grazing a dynamic object's vertical faces (a closed door leaf standing on a sample row: the rays
        # record its faces 0.8-1.5 m above the floor - no triangle lies under them, so on_objects misses them, and as
        # a 'ceiling' they dropped the floor in sp_009's aircraft doorways) belong to the object too
        ondyn |= near_objects(s, dynamic_tris, grow=0.1, below=-0.05, above=0.0, grid=nat) & ~slope_ok
    ok &= ~ondyn
    if exclude_tris is not None and len(exclude_tris):
        ok &= ~on_objects(s, exclude_tris, grid=nat)
    if exclude_footprint_tris is not None and len(exclude_footprint_tris):
        # the footprint of an excluded object (trunk base, fence posts and boards) stays a barrier
        ok &= ~near_objects(s, exclude_footprint_tris, grow=0.3, below=1.0, above=0.5, grid=nat)
    if HEADROOM_BAND is not None:
        ok &= ~low_headroom(s, (character | ondyn) if DYNAMIC_ABSENT_HEADROOM else character, *HEADROOM_BAND,
                            grid=nat)
    if DYNAMIC_FLOOR is not None and ondyn.any():
        base = ok.copy()
        for k in np.nonzero(ondyn & ((s.flags & 2) == 0) & slope_ok & ~onf)[0]:
            c = col_of_w[k]
            i, j = c % s.nx, c // s.nx
            # not where the column has its own floor just under the prop
            if any(not ondyn[n] and 0 < y[k] - y[n] <= DYNAMIC_FLOOR for n in range(s.start[c], s.start[c + 1])):
                continue
            if any(base[n] for di in (-1, 0, 1) for dj in (-1, 0, 1) if di or dj
                   for n in same_layer(i + di, j + dj, y[k], DYNAMIC_FLOOR)):
                ok[k] = True
    ok &= ~character                    # last, before hand edits: the stair / landing / bar rules above re-added 68 of
                                        # sp_004_b's 468 soldier surfaces as floor (2026-10-19)
    if OBJECTIVE_FLOOR and objective_points and onf.any():
        ok |= objective_floor(s, onf, ok, character, slope_ok, objective_points)
    if force_walkable is not None:
        ok |= force_walkable
    if force_unwalkable is not None:
        ok &= ~force_unwalkable
    col_of = col_of_w
    per_i = {i for i in range(s.nx) if round((s.min_x + i * s.pitch) % SECTOR, 1) in (1.6, 98.4)}
    per_j = {j for j in range(s.nz) if round((s.min_z + j * s.pitch) % SECTOR, 1) in (1.6, 98.4)}
    keep = np.zeros(len(y), bool)
    # the seeds on the sector-edge sample lines: every ok surface of a column on one, in surface order (NumPy: a loop
    # over every ok surface took a second on sp_004_b - round E3h)
    edge_col = np.zeros((s.nz, s.nx), bool)
    edge_col[:, sorted(per_i)] = True
    edge_col[sorted(per_j), :] = True
    edge_seeds = np.nonzero((ok & terrain if terrain_seeds_only else ok) & edge_col.reshape(-1)[col_of])[0]
    keep[edge_seeds] = True
    q: deque[int] = deque(edge_seeds.tolist())
    seed_list = list(extra_seeds or ())
    picks = _column_picks(nat, seed_list, ok, -1.5, 0.5)       # in the Rust core (E3c), or None: the loop below
    for n in (picks or ()):
        if n >= 0 and not keep[n]:
            keep[n] = True
            q.append(n)
    for (sx, sy, sz) in (seed_list if picks is None else ()):
        # level seed points (soldier spawns): the walkable surface at or just below the point
        i, j = int(round((sx - s.min_x) / s.pitch)), int(round((sz - s.min_z) / s.pitch))
        if not (0 <= i < s.nx and 0 <= j < s.nz):
            continue
        c = j * s.nx + i
        cand = [n for n in range(s.start[c], s.start[c + 1]) if ok[n] and -1.5 <= y[n] - sy <= 0.5]
        if cand:
            n = min(cand, key=lambda n: abs(y[n] - sy))
            if not keep[n]:
                keep[n] = True
                q.append(n)
    grid_nat: list[Any] = []                 # the samples in the Rust core, made at the first flood

    def flood() -> None:
        """keep grown from the queued surfaces over ok surfaces by the step rule (8 neighbours, step_fits), in the
        Rust core; the queue emptied. Again (the ladders' floods): from the kept flags the core holds - keep changes
        between the floods only by the queued surfaces (round E3h: sending all flags each time cost more than the
        floods)."""
        if grid_nat:
            keep[np.asarray(grid_nat[0].flood_more([int(k) for k in q]), np.int64)] = True
        else:
            grid_nat.append(native_grid(s, ok, gx_all, gz_all, stair, step))
            keep[:] = np.frombuffer(grid_nat[0].flood(np.ascontiguousarray(keep, bool).tobytes(),
                                                      [int(k) for k in q]), bool)
        q.clear()
    flood()
    if ladders:
        # ladder links: a ladder whose one end stands on kept ground seeds the walkable surface at its other end (a roof
        # or deck reached only by the ladder - no step joins it, so no seed reached it); repeated until no ladder adds
        # ground (ladders in a chain). ladder_ends gives the end surfaces.
        done = set()
        changed = True
        while changed:
            changed = False
            for li, lad in enumerate(ladders):
                if li in done:
                    continue
                foot, top = ladder_ends(s, ok, keep, lad)
                kf, kt = foot is not None and keep[foot], top is not None and keep[top]
                far = top if kf and not kt else foot if kt and not kf else None
                if far is None:
                    if kf and kt:
                        done.add(li)
                    continue
                keep[far] = True
                q.append(far)
                flood()
                done.add(li)
                changed = True
    for (sx, sy, sz) in (unseeds or ()):
        # drop the island the point is on: the kept surface at or just below it, and all kept surfaces it
        # connects to by the same step rule
        i, j = int(round((sx - s.min_x) / s.pitch)), int(round((sz - s.min_z) / s.pitch))
        if not (0 <= i < s.nx and 0 <= j < s.nz):
            continue
        c = j * s.nx + i
        cand = [n for n in range(s.start[c], s.start[c + 1]) if keep[n] and -1.5 <= y[n] - sy <= 0.5]
        if not cand:
            continue
        first = min(cand, key=lambda n: abs(y[n] - sy))
        drop = {first}
        q = deque([first])
        while q:
            k = q.popleft()
            c = col_of[k]
            i, j = c % s.nx, c // s.nx
            for dj in (-1, 0, 1):
                for di in (-1, 0, 1):
                    a, b = i + di, j + dj
                    if not (di or dj) or not (0 <= a < s.nx and 0 <= b < s.nz):
                        continue
                    cc = b * s.nx + a
                    for n in range(s.start[cc], s.start[cc + 1]):
                        if n in drop or not keep[n]:
                            continue
                        if step_fits(y[n] - y[k], (gx_all[k], gz_all[k]), (gx_all[n], gz_all[n]), stair[k], stair[n],
                                     di, dj, s.pitch, step):
                            drop.add(n)
                            q.append(n)
        keep[list(drop)] = False
    if ROOF_PRUNE:
        anchor_k: set[int] = set()
        anchor_list = list(anchors or ())
        picks = _column_picks(nat, anchor_list, keep, -2.5, 2.5)
        anchor_k.update(n for n in (picks or ()) if n >= 0)
        for (sx, sy, sz) in (anchor_list if picks is None else ()):
            i, j = int(round((sx - s.min_x) / s.pitch)), int(round((sz - s.min_z) / s.pitch))
            if 0 <= i < s.nx and 0 <= j < s.nz:
                c = j * s.nx + i
                cand = [n for n in range(s.start[c], s.start[c + 1]) if keep[n] and -2.5 <= y[n] - sy <= 2.5]
                if cand:
                    anchor_k.add(min(cand, key=lambda n: abs(y[n] - sy)))
        for lad in (ladders or ()):
            for e in ladder_ends(s, ok, keep, lad):
                if e is not None and keep[e]:
                    anchor_k.add(int(e))
        # the islands on the flood's NavLevel (the same samples, gradients and stairs: none changed since)
        roof_drop = roof_islands(s, keep, gx_all, gz_all, stair, step, terrain, anchor_k,
                                 grid_nat[0] if grid_nat else None)
        keep &= ~roof_drop
    if grid_out is not None and grid_nat:
        grid_out.append(grid_nat[0])
    if return_gradients:
        return (keep, gx_all, gz_all, stair) if return_stairs else (keep, gx_all, gz_all)
    return keep


def islands_of(s: navsample.Samples, keep: np.ndarray, gx: np.ndarray, gz: np.ndarray, stair: np.ndarray,
               step: float, grid: Any = None) -> tuple[np.ndarray, int]:
    """(island id per kept surface, -1 elsewhere; the number of islands): surfaces joined by the step rule, as
    walkable's flood joins them, numbered from 0 in the order of their lowest surface; in the Rust core. grid: a
    NavLevel of the same samples, gradients, stairs and step (walkable's flood's), whose arrays are reused."""
    nat = grid.with_walk(_flags(keep)) if grid is not None else native_grid(s, keep, gx, gz, stair, step)
    ids, count = nat.islands()
    return np.frombuffer(ids, '<i8').astype(np.int64), int(count)


def roof_islands(s: navsample.Samples, keep: np.ndarray, gx: np.ndarray, gz: np.ndarray, stair: np.ndarray, step: float,
                 terrain: np.ndarray, anchor_k: set[int], grid: Any = None) -> np.ndarray:
    """The kept surfaces ROOF_PRUNE drops (bool per surface). After the islands (Rust), the rule runs in the Rust core
    when it is there (round E3c; this Python is its reference and fallback: BFBC2_PURE_PYTHON, use_native(False)).
    grid: see islands_of."""
    y = s.y.astype(float)
    comp, n = islands_of(s, keep, gx, gz, stair, step, grid)
    rule = (ROOF_RAISE, ROOF_SHARE, ROOF_NEAR, ROOF_NEAR_DY)
    if _native is not None and hasattr(_native, 'navkit_roof_drop') and _num(s.pitch, s.min_x, s.min_z, *rule):
        try:
            got, stats = _native.navkit_roof_drop(
                int(s.nx), int(s.nz), np.ascontiguousarray(s.start, '<i8').tobytes(),
                np.ascontiguousarray(y, '<f8').tobytes(), _flags(keep), _flags(terrain),
                np.ascontiguousarray(comp, '<i8').tobytes(), int(n), [int(k) for k in anchor_k],
                (float(s.min_x), float(s.min_z), float(s.pitch)), tuple(float(v) for v in rule))
        except _NATIVE_ERRORS:
            pass
        else:
            if stats is not None:
                ROOF_STATS['islands'] = ROOF_STATS.get('islands', 0) + stats[0]
                if stats[1] is not None:
                    ROOF_STATS['dropped'] = ROOF_STATS.get('dropped', 0) + stats[1][0]
                    ROOF_STATS['surfaces'] = ROOF_STATS.get('surfaces', 0) + stats[1][1]
            return np.frombuffer(got, np.bool_).copy()
    out = np.zeros(len(y), bool)
    if n < 2:
        return out
    col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
    lowest = np.minimum.reduceat(y, s.start[:-1].clip(max=len(y) - 1)) if len(y) else y
    lowest = lowest[col_of]
    raised = keep & ~terrain & (y - lowest >= ROOF_RAISE)
    kc = comp[keep]
    size = np.bincount(kc, minlength=n)
    rshare = np.bincount(comp[raised], minlength=n) / np.maximum(size, 1)
    anchored = np.zeros(n, bool)
    anchored[[comp[k] for k in anchor_k if comp[k] >= 0]] = True
    anchored[int(np.argmax(size))] = True
    cand = ~anchored & (rshare >= ROOF_SHARE)
    if not cand.any():
        ROOF_STATS['islands'] = ROOF_STATS.get('islands', 0) + n
        return out
    # anchored ground near each candidate at its own height: buckets of ROOF_NEAR m; candidate surfaces every ~0.8 m
    ak = np.nonzero(keep & anchored[np.maximum(comp, 0)] & (comp >= 0))[0]
    ax = s.min_x + (col_of[ak] % s.nx) * s.pitch
    az = s.min_z + (col_of[ak] // s.nx) * s.pitch
    buck: defaultdict[tuple[int, int], list[tuple[float, float, float]]] = defaultdict(list)
    for x_, z_, y_ in zip(ax.tolist(), az.tolist(), y[ak].tolist()):
        buck[(int(math.floor(x_ / ROOF_NEAR)), int(math.floor(z_ / ROOF_NEAR)))].append((x_, z_, y_))
    every = max(1, int(round(0.8 / s.pitch)))
    near = np.zeros(n, bool)
    for k in np.nonzero(keep & cand[np.maximum(comp, 0)] & (comp >= 0))[0]:
        ci = comp[k]
        if near[ci]:
            continue
        c = col_of[k]
        i, j = c % s.nx, c // s.nx
        if i % every or j % every:
            continue
        x_, z_ = s.min_x + i * s.pitch, s.min_z + j * s.pitch
        bx, bz = int(math.floor(x_ / ROOF_NEAR)), int(math.floor(z_ / ROOF_NEAR))
        for dx in (-1, 0, 1):
            for dz in (-1, 0, 1):
                for (px, pz, py) in buck.get((bx + dx, bz + dz), ()):
                    if abs(py - y[k]) <= ROOF_NEAR_DY and math.hypot(px - x_, pz - z_) <= ROOF_NEAR:
                        near[ci] = True
                        break
                if near[ci]:
                    break
            if near[ci]:
                break
    drop_c = cand & ~near
    out = keep & (comp >= 0) & drop_c[np.maximum(comp, 0)]
    ROOF_STATS['islands'] = ROOF_STATS.get('islands', 0) + n
    ROOF_STATS['dropped'] = ROOF_STATS.get('dropped', 0) + int(drop_c.sum())
    ROOF_STATS['surfaces'] = ROOF_STATS.get('surfaces', 0) + int(out.sum())
    return out


def score(s: navsample.Samples, keep: np.ndarray, hpd: str, tol: float = 0.6) -> dict[str, float]:
    """Against the reference: DICE layers (cells fully inside an area) found exactly / within one cell, and
    kept surfaces with no reference layer at that height in the cell or its neighbours."""
    ref = {c: [(lo - OUTLINE_Y_OFFSET, hi - OUTLINE_Y_OFFSET, cv) for lo, hi, cv in layers]
           for c, layers in navsample.reference_cells(s, hpd).items()}
    y = s.y.astype(float)
    col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))

    def kept_near(c: int, lo: float, hi: float, r: int) -> bool:
        i, j = c % s.nx, c // s.nx
        for dj in range(-r, r + 1):
            for di in range(-r, r + 1):
                a, b = i + di, j + dj
                if 0 <= a < s.nx and 0 <= b < s.nz:
                    cc = b * s.nx + a
                    if any(keep[n] and lo - tol <= y[n] <= hi + tol for n in range(s.start[cc], s.start[cc + 1])):
                        return True
        return False

    full = found = near = 0
    for c, layers in ref.items():
        for lo, hi, cov in layers:
            if cov < 1:
                continue
            full += 1
            if kept_near(c, lo, hi, 0):
                found += 1
            elif kept_near(c, lo, hi, 1):
                near += 1
    extra = far = 0
    for k in np.nonzero(keep)[0]:
        c = col_of[k]
        if any(lo - tol <= y[k] <= hi + tol for lo, hi, _ in ref.get(c, [])):
            continue
        extra += 1
        i, j = c % s.nx, c // s.nx
        if not any(lo - tol <= y[k] <= hi + tol for a in (-1, 0, 1) for b in (-1, 0, 1)
                   if 0 <= i + a < s.nx and 0 <= j + b < s.nz for lo, hi, _ in ref.get((j + b) * s.nx + i + a, [])):
            far += 1
    within = 100 - ((full - found - near) + far) / max(full + far, 1) * 100
    return {'reference': full, 'found': found / full * 100, 'found_near': (found + near) / full * 100,
            'extra': extra / full * 100, 'extra_far': far / full * 100, 'within_one_cell': within}


def main(argv: list[str]) -> int:
    if len(argv) < 4 or argv[1] != 'score':
        print(__doc__)
        return 1
    args = [a for a in argv[2:] if not a.startswith('--')]
    opts = dict(zip(argv[2:], argv[3:]))
    f, _ = navsample.load(args[0])
    s = coarse(f) if abs(f.pitch - PITCH) > 1e-3 else f
    tris = np.load(args[2]) if len(args) > 2 else None
    keep = walkable(s, tris, float(opts.get('--slope', 40)), float(opts.get('--step', 0.28)))
    r = score(s, keep, args[1])
    print(f"reference layers {r['reference']}: found {r['found']:.1f}% (within 0.8 m {r['found_near']:.1f}%), "
          f"extra {r['extra']:.1f}% (far {r['extra_far']:.1f}%); agreement within 0.8 m: {r['within_one_cell']:.2f}%")
    return 0


if __name__ == '__main__':
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('navwalk', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
