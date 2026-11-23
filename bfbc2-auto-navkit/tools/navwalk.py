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
from collections.abc import Iterable, Sequence
from typing import Any, cast

import numpy as np

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools')
import navsample  # noqa: E402

# The Rust core (rust/bc2-navbuild through bc2_native): walkable's flood, islands_of and
# matched_flags (round E1d), the walkability rules - headroom, stair_slope, narrow_lips, the stair landings,
# near_objects / on_objects (E2a), near_characters, the seed picks, the roof rule (E3c) and the unseeds (E3h) - run in
# it. Their Python was retired after the full test tier (2026-11-17 and 2026-11-18, round R1: rust/BENCHMARKS.md
# "Retired Python"; the answers recorded in tests/navbuild_golden.json); the module is required (require_native).
# Round NK4 moved the rest - walkable's glue, near_points, ladder_ends, objective_floor, coarse - into it
# (bc2_navbuild::walk; rust/BENCHMARKS.md "NK4").

def require_native() -> Any:
    """The Rust core for the walkability rules and floods, or bfbc2core.native.NativeMissing saying how to get it."""
    suite_paths.use('core')
    from bfbc2core import native
    return native.require("NavKit's walkability rules (navwalk.py)", 'navkit_walkable')


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


def native_samples(s: navsample.Samples) -> Any:
    """The samples in the Rust core for walkable's rules (NavGrid: headroom, stair_slope, narrow_lips, the stair
    landings, near_objects, on_objects, near_characters, the seed picks). Heights go in as s.y.astype(float), as the
    rules read them; the pitch and origin as Python floats."""
    return require_native().NavGrid(int(s.nx), int(s.nz), np.ascontiguousarray(s.start, '<i8').tobytes(),
                                    np.ascontiguousarray(s.y, '<f8').tobytes(), False, float(s.pitch),
                                    float(s.min_x), float(s.min_z))


def _grid_of(s: navsample.Samples, grid: Any) -> Any:
    """grid (a NavGrid walkable made once for all its rules) or one made for s now."""
    return grid if grid is not None else native_samples(s)


def _flags(a: Any) -> bytes:
    """A per-surface truth array as one byte each (each value's truth)."""
    return np.ascontiguousarray(a, bool).tobytes()


def _tris_f64(tris: Any) -> bytes:
    """Triangles (n x 3 x 3) as float64 bytes, as tris.astype(float) holds them."""
    t = np.asarray(tris)
    if t.ndim != 3 or t.shape[1:] != (3, 3):
        raise ValueError(f'triangles are n x 3 x 3 (corners x, y, z), not {t.shape}')
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


# Round NK4: walkability end to end in the Rust core (bc2_navbuild::walk): walkable with its NumPy glue (the masks,
# the gradients, the edge seeds, the floods, the ladders, the unseeds, the roof islands), near_points, ladder_ends,
# objective_floor and coarse. Their Python was retired after the parity run on every regression level
# (rust/BENCHMARKS.md "NK4"); these are thin calls. A run crosses as a dict (run_dict / samples_of).

def run_dict(s: navsample.Samples, normals: bool = True) -> dict[str, Any]:
    """A sample run as the Rust core takes it: the grid, the header's floats, the arrays' bytes (heights and
    normals in their dtype when float32 / float64, else float64; flags as uint8; the hit words as uint32, None when
    the run has none). normals=False leaves out what only walkable reads (normals, flags, hit words)."""
    y = getattr(s, 'y', None)
    y = np.zeros(int(np.asarray(s.start)[-1])) if y is None else np.asarray(y)      # a stand-in with columns only
    y32 = y.dtype == np.float32
    d: dict[str, Any] = dict(nx=int(s.nx), nz=int(s.nz), pitch=float(s.pitch), min_x=float(s.min_x),
                             min_z=float(s.min_z),
                             header=[float(getattr(s, k, 0.0)) for k in ('y_top', 'y_bottom', 'radius', 'cap_bottom',
                                                                          'cap_top', 'ray_radius')],
                             start=np.ascontiguousarray(s.start, '<i8').tobytes(),
                             y=np.ascontiguousarray(y, '<f4' if y32 else '<f8').tobytes(), y_f32=bool(y32))
    if normals:
        n = np.asarray(s.normal)
        n32 = n.dtype == np.float32
        d.update(normal=np.ascontiguousarray(n, '<f4' if n32 else '<f8').tobytes(), normal_f32=bool(n32),
                 flags=np.ascontiguousarray(np.asarray(s.flags).astype(np.uint8)).tobytes(),
                 hit=None if s.hit is None or not len(s.hit) else np.ascontiguousarray(s.hit, '<u4').tobytes())
    return d


def samples_of(d: dict[str, Any]) -> navsample.Samples:
    """navsample.Samples from the Rust core's run dict (the arrays over its bytes, no copy)."""
    hit, h = d['hit'], d['header']
    return navsample.Samples(d['pitch'], d['min_x'], d['min_z'], d['nx'], d['nz'], h[0], h[1], h[2], h[3], h[4], h[5],
                             np.frombuffer(d['start'], '<i8'), np.frombuffer(d['y'], '<f4' if d['y_f32'] else '<f8'),
                             np.frombuffer(d['normal'], '<f4' if d['normal_f32'] else '<f8').reshape(-1, 3),
                             np.frombuffer(d['flags'], np.uint8),
                             None if hit is None else np.frombuffer(hit, '<u4').reshape(-1, 3))


def coarse(f: navsample.Samples) -> navsample.Samples:
    """Every (PITCH / f.pitch)-th column of a fine run, keeping all surfaces of the column; in the Rust core."""
    return samples_of(require_native().navkit_coarse(run_dict(f), float(PITCH)))


def on_objects(s: navsample.Samples, tris: np.ndarray | None, tol: float = 0.2, grid: Any = None) -> np.ndarray:
    """Surfaces lying on one of the given world-space triangles (within tol metres vertically)."""
    out = np.zeros(len(s.y), bool)
    if tris is None or not len(tris):
        return out
    x0, x1 = s.min_x - 2, s.min_x + s.nx * s.pitch + 2
    z0, z1 = s.min_z - 2, s.min_z + s.nz * s.pitch + 2
    t = tris[(tris[:, :, 0].max(1) >= x0) & (tris[:, :, 0].min(1) <= x1) &
             (tris[:, :, 2].max(1) >= z0) & (tris[:, :, 2].min(1) <= z1)].astype(float)
    if not len(t):
        return out
    return np.frombuffer(_grid_of(s, grid).on_objects(_tris_f64(t), float(tol)), np.bool_).copy()


def near_characters(s: navsample.Samples, character: np.ndarray, grow: float = 0.9, below: float = 2.2,
                    above: float = 0.3, grid: Any = None) -> np.ndarray:
    """Surfaces a capsule test would see blocked by a soldier: within `grow` metres (horizontally) of a character hit
    (CHARACTER_SHAPE_VTABLE) and from `below` metres under to `above` metres over it. In the Rust core when it is there
    (round E3c: in the Rust core; grid: native_samples(s))."""
    return np.frombuffer(_grid_of(s, grid).near_characters(_flags(character), float(grow), float(below), float(above)),
                         np.bool_).copy()


def headroom(s: navsample.Samples, ignore: np.ndarray | None = None, grid: Any = None) -> np.ndarray:
    """Per surface: the gap to the next sampled surface above it in its column (inf when none); surfaces flagged in
    `ignore` (soldiers) do not count as above (a surface counts as above when it is over 0.05 m higher). In the Rust
    core; grid: native_samples(s), made once by the caller (else here)."""
    got = _grid_of(s, grid).headroom(None if ignore is None else _flags(ignore))
    return np.frombuffer(got, '<f8').astype(np.float64)


def low_headroom(s: navsample.Samples, ignore: np.ndarray | None, lo: float, hi: float,
                 grid: Any = None) -> np.ndarray:
    """Surfaces with the next surface above them between lo and hi metres higher (HEADROOM_BAND)."""
    h = headroom(s, ignore, grid)
    return (h >= lo) & (h < hi)


def near_points(s: navsample.Samples, points: Iterable[Sequence[float]], grow: float = 0.9, below: float = 0.5,
                above: float = 0.5) -> np.ndarray:
    """Surfaces within `grow` metres (horizontally) of a point and from `below` under to `above` over its height;
    in the Rust core."""
    got = require_native().navkit_near_points(run_dict(s, normals=False), [tuple(p) for p in points], float(grow),
                                              float(below), float(above))
    return np.frombuffer(got, np.bool_).copy()


def near_objects(s: navsample.Samples, tris: np.ndarray | None, grow: float = 0.5, below: float = 2.2,
                 above: float = 0.3, grid: Any = None) -> np.ndarray:
    """Surfaces within `grow` metres (horizontally) of a triangle's bounding box and from `below` metres under
    to `above` metres over it - the ground a capsule test around an object would see as blocked."""
    out = np.zeros(len(s.y), bool)
    if tris is None or not len(tris):
        return out
    got = _grid_of(s, grid).near_objects(_tris_f64(tris), float(grow), float(below), float(above))
    return np.frombuffer(got, np.bool_).copy()


def stair_slope(s: navsample.Samples, mask: np.ndarray, r: int = 1, tol: float = 0.7,
                grid: Any = None) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Slope (degrees) from neighbouring sample heights on the same layer (central differences, +-1 sample),
    for the surfaces in mask; 90 where a neighbour is missing. (Measured over +-0.8 m at pitch 0.2 it recognised far
    more stairs, but with a 0.5 m step on them it joined decks and ledges to streets - over-connection 2.2 -> 35.3 % on
    ONS_MP_002; not adopted, 2026-10-21.) The neighbour on the same layer: the surface of its column nearest the
    height within tol; one-sided differences where one is missing. Returns (slope, gx, gz); in the Rust core."""
    if int(r) != r:
        raise ValueError(f'stair_slope: r is a whole number of samples, not {r!r}')
    got = _grid_of(s, grid).stair_slope(_flags(mask), int(r), float(tol))
    return tuple(np.frombuffer(b, '<f8').astype(np.float64) for b in got)    # type: ignore[return-value]


def narrow_lips(s: navsample.Samples, base: np.ndarray, cand: np.ndarray, slope: float, width: int, rise: float | None = None,
                flat: float | None = None, grid: Any = None) -> np.ndarray:
    """The cand surfaces (steep terrain) with a base surface (walkable) within width samples on both opposite sides
    along an axis or a diagonal - same layer, each step within 0.7 m - and a slope within the limit between those two
    (LIP_WIDTH). rise: the candidate also within this height of both of those (LIP_OBJECT_RISE); flat: both of those
    within this many degrees of level. In the Rust core; normals of another type than float32 / float64 are widened to
    float64 for the flat test."""
    if int(width) != width:
        raise ValueError(f'narrow_lips: width is a whole number of samples, not {width!r}')
    fl = None
    if flat is not None:
        ny = np.asarray(s.normal[:, 1])
        f32 = ny.dtype == np.float32
        fl = (np.ascontiguousarray(ny, '<f4' if f32 else '<f8').tobytes(), f32, math.cos(math.radians(flat)))
    got = _grid_of(s, grid).narrow_lips(_flags(base), _flags(cand), math.tan(math.radians(slope)), int(width),
                                        None if rise is None else float(rise), fl)
    return np.frombuffer(got, np.bool_).copy()


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
    there is none. In the Rust core."""
    cfg = _settings(40.0)
    cfg['ladder_reach'] = float(reach)
    got = require_native().navkit_ladder_ends(run_dict(s, normals=False), _flags(ok), _flags(keep),
                                              _ladders([lad]), cfg)
    return cast(tuple[int | None, int | None], tuple(got[0]))


def _settings(slope: float) -> dict[str, Any]:
    """The module's settings as the Rust core reads them at each call, with the call's slope; the trigonometric
    constants computed here (the C library's cos / tan, as the retired Python computed them)."""
    band = HEADROOM_BAND
    return dict(characters_absent=bool(CHARACTERS_ABSENT),
                headroom_band=None if band is None else (float(band[0]), float(band[1])),
                steep_terrain=bool(STEEP_TERRAIN), steep_max_cos=math.cos(math.radians(STEEP_TERRAIN_MAX)),
                steep_span=float(STEEP_TERRAIN_SPAN), lip_width=int(LIP_WIDTH) if LIP_WIDTH else None,
                lip_object_rise=None if LIP_OBJECT_RISE is None else float(LIP_OBJECT_RISE),
                lip_object_slope_max=bool(LIP_OBJECT_SLOPE_MAX),
                lip_flat_cos=None if LIP_OBJECT_FLAT is None else math.cos(math.radians(LIP_OBJECT_FLAT)),
                roof_prune=bool(ROOF_PRUNE),
                roof=(float(ROOF_RAISE), float(ROOF_SHARE), float(ROOF_NEAR), float(ROOF_NEAR_DY)),
                dynamic_absent_headroom=bool(DYNAMIC_ABSENT_HEADROOM),
                dynamic_floor=None if DYNAMIC_FLOOR is None else float(DYNAMIC_FLOOR),
                objective_floor=bool(OBJECTIVE_FLOOR), objective_reach=float(OBJECTIVE_REACH), sector=float(SECTOR),
                ladder_reach=float(LADDER_REACH), ladder_foot=tuple(float(v) for v in LADDER_FOOT),
                ladder_top=tuple(float(v) for v in LADDER_TOP), terrain_vtable=int(TERRAIN_SHAPE_VTABLE),
                character_vtable=int(CHARACTER_SHAPE_VTABLE), slope=float(slope),
                slope_cos=math.cos(math.radians(slope)) - 0.01, slope_tan=math.tan(math.radians(slope)))


def _tris(t: Any) -> tuple[bytes, bool] | None:
    """Object triangles as the Rust core takes them (float64 bytes, the array was float32), None for none."""
    if t is None or not len(t):
        return None
    return _tris_f64(t), np.asarray(t).dtype == np.float32


def _capsule(other: navsample.Samples | None) -> tuple[bytes, bytes, bytes] | None:
    """A second run (high or narrow capsule) as matched_flags reads it."""
    if other is None:
        return None
    return (np.ascontiguousarray(other.start, '<i8').tobytes(), np.asarray(other.y, '<f8').tobytes(),
            np.ascontiguousarray(other.flags, np.uint8).tobytes())


def _ladders(ladders: Iterable[dict[str, Any]] | None) -> list[tuple[tuple[float, ...], tuple[float, ...]]]:
    return [(tuple(lad['lo']), tuple(lad['hi'])) for lad in (ladders or ())]


def walk_inputs(forbid_tris: np.ndarray | None = None, step: float = 0.28, terrain_seeds_only: bool = False,
                dynamic_tris: np.ndarray | None = None, high_capsule: navsample.Samples | None = None,
                stairs: bool = True, narrow_capsule: navsample.Samples | None = None, raised_bars: bool = False,
                stair_tris: np.ndarray | None = None, exclude_tris: np.ndarray | None = None,
                exclude_footprint_tris: np.ndarray | None = None, forbid_tol: float = 0.08,
                extra_seeds: Iterable[Sequence[float]] | None = None, force_walkable: np.ndarray | None = None,
                force_unwalkable: np.ndarray | None = None, unseeds: Iterable[Sequence[float]] | None = None,
                soldier_points: Sequence[Sequence[float]] | None = None,
                ladders: Sequence[dict[str, Any]] | None = None, anchors: Iterable[Sequence[float]] | None = None,
                objective_points: Sequence[Sequence[float]] | None = None) -> dict[str, Any]:
    """walkable's arguments as the Rust core takes them."""
    return dict(forbid=_tris(forbid_tris), dynamic=_tris(dynamic_tris), stair_tris=_tris(stair_tris),
                exclude=_tris(exclude_tris), exclude_foot=_tris(exclude_footprint_tris), high=_capsule(high_capsule),
                narrow=_capsule(narrow_capsule), step=float(step), terrain_seeds_only=bool(terrain_seeds_only),
                stairs=bool(stairs), raised_bars=bool(raised_bars), forbid_tol=float(forbid_tol),
                extra_seeds=list(extra_seeds or ()),
                force_walkable=None if force_walkable is None else _flags(force_walkable),
                force_unwalkable=None if force_unwalkable is None else _flags(force_unwalkable),
                unseeds=list(unseeds or ()),
                soldier_points=list(soldier_points) if soldier_points is not None and len(soldier_points) else [],
                ladders=_ladders(ladders), anchors=list(anchors or ()),
                objective_points=list(objective_points) if objective_points else [])


def walk_answer(got: Sequence[Any], return_gradients: bool, return_stairs: bool,
                grid_out: list[Any] | None) -> Any:
    """walkable's answer from the Rust core's (keep, (gx, f32), (gz, f32), stair, NavLevel, stats): the statistics
    added to ROOF_STATS / OBJECTIVE_STATS / STEEP_STATS as the retired Python added them."""
    keep_b, (gx_b, gx32), (gz_b, gz32), stair_b, level, (steep, roof, objective, _ladder_pairs) = got
    for k, v in steep:
        STEEP_STATS[k] = STEEP_STATS.get(k, 0) + v
    if roof is not None:
        ROOF_STATS['islands'] = ROOF_STATS.get('islands', 0) + roof[0]
        if roof[1] is not None:
            ROOF_STATS['dropped'] = ROOF_STATS.get('dropped', 0) + roof[1][0]
            ROOF_STATS['surfaces'] = ROOF_STATS.get('surfaces', 0) + roof[1][1]
    if objective is not None:
        OBJECTIVE_STATS['points'] = OBJECTIVE_STATS.get('points', 0) + objective[0]
        OBJECTIVE_STATS['surfaces'] = OBJECTIVE_STATS.get('surfaces', 0) + objective[1]
    keep = np.frombuffer(keep_b, np.bool_).copy()
    gx = np.frombuffer(gx_b, '<f4' if gx32 else '<f8')
    gz = np.frombuffer(gz_b, '<f4' if gz32 else '<f8')
    stair = np.frombuffer(stair_b, np.bool_).copy()
    if grid_out is not None:
        grid_out.append(level)
    if return_gradients:
        return (keep, gx, gz, stair) if return_stairs else (keep, gx, gz)
    return keep


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
    seeding; unseeds (points) drop the island each lies on after seeding. grid_out: a list the floods' NavLevel is
    appended to. In the Rust core
    (bc2_navbuild::walk::walkable, round NK4; the rules in the order this docstring's module notes give them)."""
    nat = require_native()
    args = walk_inputs(forbid_tris, step, terrain_seeds_only, dynamic_tris, high_capsule, stairs, narrow_capsule,
                       raised_bars, stair_tris, exclude_tris, exclude_footprint_tris, forbid_tol, extra_seeds,
                       force_walkable, force_unwalkable, unseeds, soldier_points, ladders, anchors, objective_points)
    got = nat.navkit_walkable(run_dict(s), args, _settings(slope))
    return walk_answer(got, return_gradients, return_stairs, grid_out)


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
    (round E3c): an island (islands_of) is dropped when it holds no anchor, is not the largest, has ROOF_SHARE of its
    surfaces on objects ROOF_RAISE m or more over their column's lowest surface, and has no anchored ground within
    ROOF_NEAR m (ROOF_NEAR_DY in height) of its surfaces on the 0.8 m grid. grid: see islands_of."""
    comp, n = islands_of(s, keep, gx, gz, stair, step, grid)
    rule = (ROOF_RAISE, ROOF_SHARE, ROOF_NEAR, ROOF_NEAR_DY)
    got, stats = require_native().navkit_roof_drop(
        int(s.nx), int(s.nz), np.ascontiguousarray(s.start, '<i8').tobytes(),
        np.ascontiguousarray(s.y.astype(float), '<f8').tobytes(), _flags(keep), _flags(terrain),
        np.ascontiguousarray(comp, '<i8').tobytes(), int(n), [int(k) for k in anchor_k],
        (float(s.min_x), float(s.min_z), float(s.pitch)), tuple(float(v) for v in rule))
    if stats is not None:
        ROOF_STATS['islands'] = ROOF_STATS.get('islands', 0) + stats[0]
        if stats[1] is not None:
            ROOF_STATS['dropped'] = ROOF_STATS.get('dropped', 0) + stats[1][0]
            ROOF_STATS['surfaces'] = ROOF_STATS.get('surfaces', 0) + stats[1][1]
    return np.frombuffer(got, np.bool_).copy()


# ------------------------------------------------------------------------------------------------ the build
# NavKit's whole build runs in the Rust core (round NK5: bc2_navbuild::build through bc2_native.navkit_build). Its
# settings and switches are the Rust defaults (bc2_navbuild::stage::BuildSettings / Switches; bc2_native.
# navkit_settings() lists them); a caller passes only what it changes. navbuild.py, which held them as a Python
# dataclass and module names beside the retired Python stage, went in round NK5-rest (rust/BENCHMARKS.md
# "NK5-rest").

MAX_WATER_DEPTH = 1.5           # ground deeper under water is not walkable (DICE's data: 99.9 % of its ground under
                                # water is within 1.45 m, median over 11 levels; navwater.py)


def water_tops(s: Any, vertices: np.ndarray | None, triangles: np.ndarray) -> np.ndarray:
    """The water surface height over every sample column (-inf where there is no water). The columns are a regular
    grid, so each water triangle only visits the columns its bounding box covers. In the Rust core (round NK4:
    bc2_navbuild::walk::water_tops); s: anything with nx, nz, pitch, min_x, min_z."""
    if vertices is None or not len(triangles):
        return np.full(int(s.nx) * int(s.nz), -np.inf)
    got = require_native().navkit_water_tops(
        int(s.nx), int(s.nz), float(s.pitch), float(s.min_x), float(s.min_z),
        np.ascontiguousarray(vertices, '<f8').tobytes(), np.ascontiguousarray(triangles, '<i8').tobytes())
    return np.frombuffer(got, '<f8').copy()


def outside_bounds(s: Any, bounds: Sequence[float]) -> np.ndarray:
    """Surfaces whose column lies outside bounds (minX, minZ, maxX, maxZ) - bool per surface (the build setting
    clip_to_reference: the bound to DICE's path data's extent); in the Rust core."""
    got = require_native().navkit_outside_bounds(run_dict(s, normals=False), [float(v) for v in bounds[:4]])
    return np.frombuffer(got, np.bool_).copy()


def deep_water(s: Any, water: tuple[np.ndarray, np.ndarray] | None, limit: float = MAX_WATER_DEPTH) -> np.ndarray:
    """Surfaces more than limit metres under the level's water (bool per surface). water: (vertices, triangles). In
    the Rust core."""
    if water is None:
        return np.zeros(len(s.y), dtype=bool)
    top = water_tops(s, water[0], water[1])
    got = require_native().navkit_deep_water(run_dict(s, normals=False),
                                             np.ascontiguousarray(top, '<f8').tobytes(), float(limit))
    return np.frombuffer(got, np.bool_).copy()


def clock_words(stamp: Any) -> tuple[int, int]:
    """A sector's (date, clock) words of a time.localtime() stamp."""
    return ((stamp.tm_year % 100) * 10000 + (stamp.tm_mon - 1) * 100 + stamp.tm_mday,
            stamp.tm_hour * 1000000 + stamp.tm_min * 10000 + stamp.tm_sec * 100)


# DICE's terrain rule for D sides (KYG-DF-1, round NKG3): a slope threshold with the level's height field, None: the
# step rule. On by the user's ruling (2026-11-22; sp_02's D recall 21.86 -> 21.53 % a named exception).
DF_SLOPE: float | None = 0.77

# DICE's node placement (KYG-NODE-7, KYG-NODE-8, KYG-EDGE-5, round NKS-deep B): Kynapse's propagation, simplification and
# over-connection pass in place of NavKit's own nodes and spanner. On by the user's ruling (2026-11-23; its regressions
# named exceptions in spec/transforms/navkit.md, "Parity target").
PLACEMENT: bool = True

# DICE's seed list in the placement (KYG-SEED-2, round NKS-deep E): the sector's soldier spawns and perimeter cells as
# unlocked seeds after the border nodes and order positions; KYG-AREA-4's midline cut (KYG-AREA-5's record size).
# Both measured and off: neither leaves every measure at least as good (rust/BENCHMARKS.md "NKS-deep E").
DICE_SEEDS: bool = False
MIDLINE_CUT: bool = False


def switches() -> dict[str, Any]:
    """The build's switches this module owns (the stage's log line names what walkability did); the others are the
    Rust defaults."""
    return dict(roof_prune=bool(ROOF_PRUNE), objective_floor=bool(OBJECTIVE_FLOOR), df_slope=DF_SLOPE,
                placement=bool(PLACEMENT), dice_seeds=bool(DICE_SEEDS), midline_cut=bool(MIDLINE_CUT))


def build(samples_path: str, tris_path: str | None, xoff: int, zoff: int, out_path: str, step: float = 0.28,
          log: Any = print, dynamic_path: str | None = None, high_path: str | None = None,
          narrow_path: str | None = None, stairs_path: str | None = None, exclude_path: str | None = None,
          exclude_foot_path: str | None = None, seeds: Sequence[Sequence[float]] | None = None,
          edits: list[dict[str, Any]] | None = None, water: tuple[np.ndarray, np.ndarray] | None = None,
          max_water_depth: float | None = MAX_WATER_DEPTH, bounds: Sequence[float] | None = None,
          order_points: Sequence[Sequence[float]] | None = None, detailed: dict[str, Any] | None = None,
          ladders: Sequence[dict[str, Any]] | None = None, area_orders: Sequence[Sequence[float]] | None = None,
          settings: dict[str, Any] | None = None, heightfield: str | None = None) -> dict[str, Any]:
    """A level's path data from its sample runs, written to out_path: NavKit's whole build in the Rust core
    (bc2_navbuild::build; the steps are rust/bc2-navbuild/src/stage.rs's). The log lines come through log as the build
    makes them; returns the build's counts (order nodes, ladders, D / F, shadow islands, steep ground, what was
    written).
    edits: the operations of a navigation side file (navedits.regions_of reads the walkability ones).
    water: the level's water mesh (vertices, triangles; core/bfbc2core/water.py): ground more than max_water_depth
    under it is not walkable, as in DICE's data (hand edits forcing ground walkable still win).
    bounds: (minX, minZ, maxX, maxZ) of the reference path data (clip_to_reference), or None.
    detailed: {'samples': a pitch-0.2 sampling run, 'high': its high-capsule run (or None), 'sectors': {(X, Y)}} -
    those sectors are built at pitch 0.2 (DICE's detailed zones) and merged with the normal build.
    heightfield: the level's terrain .Heightfield-000.res (level.heightfield_path): D lines and F cells between
    terrain cells follow DICE's slope rule (KYG-DF-1); None: the step rule.
    settings: build settings over the Rust defaults (e.g. {'detail_features': False}; bc2_native.navkit_settings()
    lists them), None: the defaults."""
    import time
    nat = require_native()                  # the build runs in the Rust core: say so before reading anything
    t0 = time.time()

    def npy(p: str | None) -> np.ndarray | None:
        return np.load(p) if p else None
    wet = water is not None and max_water_depth is not None
    wmesh = None
    if wet and water is not None:
        wmesh = ((np.ascontiguousarray(water[0], '<f8').tobytes(), np.ascontiguousarray(water[1], '<i8').tobytes())
                 if water[0] is not None and len(water[1]) else (b'', b''))
    job: dict[str, Any] = dict(
        samples=samples_path, high=high_path, narrow=narrow_path, forbid=_tris(npy(tris_path)),
        dynamic=_tris(npy(dynamic_path)), stairs=_tris(npy(stairs_path)),
        exclude=_tris(npy(exclude_path)), exclude_foot=_tris(npy(exclude_foot_path)),
        step=float(step), xoff=int(xoff), zoff=int(zoff), seeds=list(seeds or []), water=wmesh,
        max_water_depth=float(max_water_depth) if wet and max_water_depth is not None else None,
        bounds=[float(v) for v in bounds[:4]] if bounds is not None else None,
        order_points=list(order_points) if order_points is not None else None,
        area_orders=list(area_orders) if area_orders else None,
        ladders=_ladders(ladders) if ladders is not None else None, heightfield=heightfield)
    if edits:
        import navedits
        regions, hand_seeds, unseeds, counts = navedits.regions_of(edits)
        job['edits'] = (regions, hand_seeds, unseeds, list(counts.items()))
    if detailed:
        job['detailed'] = (detailed['samples'], detailed.get('high'), sorted(detailed['sectors']))
    stamp = time.localtime()
    try:
        return cast(dict[str, Any], nat.navkit_build(job, settings, switches(), _settings(40.0), clock_words(stamp),
                                                     out_path, log, t0))
    except ValueError as e:
        if str(e).startswith('no walkable ground') or 'outside the 256 x 256 grid' in str(e):
            raise RuntimeError(str(e)) from None      # the stage's refusals, as the Python stage raised them
        for p in (samples_path, high_path, narrow_path,
                  *((detailed['samples'], detailed.get('high')) if detailed else ())):
            if p:
                navsample.load(p)                    # a file the core does not read: navsample.load says why
        raise


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
