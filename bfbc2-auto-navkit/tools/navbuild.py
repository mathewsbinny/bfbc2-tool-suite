"""NavKit generator: in-game collision samples -> Kynapse-format pathdata.hpd (infantry, pitch 0.8).

Pipeline (research/KYNAPSE_SPEC.md sections 3 and 4):
  1. walkable surfaces: navwalk.walkable (slope, capsule, ForbidPathfinding objects, StepMax, perimeter seeds)
  2. sectors: 100 m grid from the level's SectorOffsets; each sector uses the samples -1 .. 126 of its
     window (two-sample overlap with each neighbour)
  3. tiles: 3 x 3 per sector, 50 samples each (edges half a sample outside the samples: -1.2, 38.8,
     78.8, 101.2 m); areas = connected regions of a tile (8-connected, AREA_LINK rule); the old bisection at
     the middle of their extent (over 25 samples) is off - DICE has no such cut (BISECT_AREAS)
  4. area boundary, traced on the half-sample lines and merged into straight runs:
       portal (B)   - to a connected surface in another area of the same sector
       border (C)   - to a connected surface outside the sector window (direction = outward axis)
       outline (E)  - everything else; where two cells of an area touch only diagonally, the two outline
                      corners between them are replaced by 45-degree segments (a bridge through the
                      half cells, as in DICE's outlines)
  5. neighbour links: per neighbouring sector, link objects {tile x, tile z, area index in tile} with the
     C records leading into that area
  6. graph: one node per area (sample nearest the area centre) and per portal run; border nodes on the
     overlap lines, shared by both sectors (same id and position); edges between nodes up to DistEdgeMax
     apart whose straight line stays on connected walkable samples; cost = 0x80000000 | floor(d * 16)
  7. write with kymesh.write_mesh / hpdtool.write
  (4b, between 4 and 5) detail features (DETAIL_FEATURES, navdetail.py): sides between two cells of one area whose
     heights differ by more than DETAIL_STEP become D lines, cells with both sides along an axis so broken become
     F cells (DICE's rule, KYNAPSE_SPEC.md 4.5) - and are taken out of the outline

Detailed zones (pitch 0.2) are a second pass at pitch 0.2 over the chosen sectors (build(..., detailed=...)).
Settings: a BuildSettings value (cfg) passed through build / stage / merge_detailed and their helpers; the module-level
names below are its documented defaults and are not read by a build.

Usage: navbuild.py <samples.bin> <forbid_tris.npy> <XOffset> <ZOffset> <out.hpd> [--step 0.28]
                   [--dynamic dynamic_tris.npy] [--high samples_high_capsule.bin] [--narrow samples_narrow.bin]
                   [--stairs stair_tris.npy] [--exclude exclude_tris.npy] [--exclude-foot exclude_footprint_tris.npy]
                   [--no-detail]   (no D lines / F cells: BuildSettings.detail_features off)
"""
import contextlib
import heapq
import math
import os
import sys
import time
from collections import defaultdict
from collections.abc import Callable, Container, Iterable, Mapping, Sequence
from dataclasses import dataclass, field, replace
from typing import Any, cast

import numpy as np

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools')
import hpdtool  # noqa: E402
import kymesh  # noqa: E402
import navislands  # noqa: E402
import navsample  # noqa: E402
import navwalk  # noqa: E402

type Vec3 = tuple[float, float, float]
type Cell = tuple[int, int, int]                # an area cell: (local i, local j, surface)
type AreaInfo = dict[str, Any]                  # an area: {'tile': (tx, tz), 'cells': [Cell], 'layer', 'borders'}
type EdgeRows = dict[Any, list[Any]]            # one area's boundary edges: {(kind, target, axis, line, dir): [(along, y)]}
type OneWay = tuple[int, int, tuple[Any, ...], Any]   # a one-way candidate: (source area, target area, key, along)
type Traced = tuple[list[kymesh.Portal], list[Any], list[list[Vec3]]]   # (portals, borders, outline) of an area
type RouteNode = tuple[int, Vec3, int, int, int]   # a route node: (id, pos, surface, local i, local j)
type RoutePair = tuple[int, int, float]         # a route edge: (node index, node index, length)
type Graph = tuple[int, tuple[float, ...], kymesh.Mesh, list[RouteNode], list[RoutePair], tuple[float, float]]
type Resolver = Callable[[tuple[int, int], AreaInfo, Any], tuple[int, int, int] | None]
type Log = Callable[..., object]
type Points = Sequence[Sequence[float]]         # world points (x, y, z)

PITCH = 0.8
SECTOR_SAMPLES = 125            # 100 m / 0.8 m
WINDOW = range(-1, 127)         # sample indices of a sector window (local)
TILE = 50                       # samples per tile
TILES = 3
TILE_20M_SECTORS = None         # experiment: sector ids (e.g. {0x901, 0x902}) tiled at 20 m (25 samples, 6 x 6 tiles)
                                # at the normal pitch - DICE's detailed zones have 20 m tiles (but pitch 0.2). For
                                # offline route checks only: how the runtime gets a sector's tile size is not known
                                # (its header has no tile size field), so such a build is not for the game (2026-10-19)
CLIP_TO_REFERENCE = True        # on a level with DICE path data, ground outside its extent (navregress.reference_bounds:
                                # outlines and portals at every pitch) is left out before seeding. A bound to the
                                # reference extent, NOT a found rule: DICE's edges are straight cuts (its last cell row
                                # fills 9-64 % of a side) from a generation rectangle that is not in the shipped level
                                # data. A new map (no DICE data) keeps the sampling box. (KYNAPSE_SPEC.md 4.5)
EDGE_SECTOR_RULE = 'grid_min'   # which sectors start their tiles at their own edge (EDGE_SECTOR_TILES): 'grid_min' = on
                                # the sector grid's minimum X (Z) - DICE 3,798 of 3,798 sector axes; 'no_neighbour' = no
                                # sector on the low side - 3,788 (misses holes inside the map)
SYMMETRIC_PORTALS = True        # every portal stands on both of its areas (DICE: 771 of 771 have a portal back)
ONE_WAY_PORTALS = 'drop'        # a portal piece whose target sees that side as its own interior (a second surface of
                                # the target in the source's column) cannot have a piece back (DICE meets no such case:
                                # its stacked floors are detailed zones): 'drop' = always outline; 'keep' = always a
                                # one-way portal; 'restore' = outline unless the two areas are joined through no other
                                # portal or shared surface level-wide (restore_one_way) - measured on sp_004_b it keeps
                                # 1,066 one-way records (80 % two-way) and AI pairwise falls 91.2 -> 89.1: most pairs a
                                # drop leaves without a portal are still joined by the route graph (walkable lines),
                                # which a portal-level test does not see. Open until the in-game test shows whether the
                                # runtime needs a portal to cross between areas (KYNAPSE_SPEC.md 4.5)
EDGE_SECTOR_TILES = True       # a sector with no sector next to it on the low x (or z) side starts its tiles at its
                                # own edge (sample 0, not -1): tile lines one cell later, 39.6 / 79.6 m into the sector
                                # (DICE's data: 3,788 of 3,798 sector axes on 17 levels; KYNAPSE_SPEC.md 4.5). Latent
                                # for now: NavKit's extra sectors outside DICE's bounds give most edge sectors a neighbour
MAX_AREA = 25                   # samples (20 m) before an area is bisected (only when BISECT_AREAS)
BISECT_AREAS = False            # DICE does not cut areas at the middle of their extent: its same-sector portals lie on
                                # the tile lines (38.8 / 78.8 m into the sector) and only sometimes on the fixed tile
                                # midlines (18.8 / 58.8 m; sp_005 17 %, sp_007 3 %, sp_004_b 2 % of portals), mostly in
                                # ground with long outlines; trigger not yet known (KYNAPSE_SPEC.md 4.5, 2026-10-16)
AREA_LINK = 'terrain'           # which neighbours one area joins: 'terrain' = DICE's rule below between terrain
                                # surfaces (the slope rule elsewhere), 'raw' = DICE's rule everywhere, 'slope' = the
                                # slope rule. Portals, borders and the route graph keep the slope rule ('slope'), so
                                # the steep pieces DICE cuts off stay reachable (NavKit lacks some of the steep cells
                                # that join them in DICE's data: the DICE rule there cost sp_004_b 9 connectivity points)
LINK_RULE = 'slope'             # the rule for everything else
LINK_STEP = 0.6                 # DICE: straight neighbours link when their raw height step is <= 0.6 m (36.9 deg at
                                # 0.8 m - steeper walkable cells are cut off, DICE's single-cell areas); diagonal ones
                                # when a route through either side cell (walkable or not) has both steps <= 0.6 m
                                # (sp_004_b terrain: linked steps max 0.591, unlinked mostly 0.62+; KYNAPSE_SPEC.md 4.5)
RADIUS = 0.4
RESOLUTION = 0.025
DIST_EDGE_MAX = 15.0
NODE_COVER = {'detailed': 4.0, 'normal': None}   # coverage nodes: in each area, a route node wherever walkable ground is
                                # farther than this (m, straight line, same floor) from every node, farthest first.
                                # DICE's detailed sectors: 90 % of outline points within 4.3 m of a node, 99 % within
                                # 8.3; NavKit's area-centre and portal nodes left 5.4 / 18.2 (ONS_MP_002, 2026-10-21).
                                # Normal sectors: off until checked against the campaign regression
GABRIEL_EDGES = 'detailed'      # prune the route graph's walkable pairs to the Gabriel graph (no other same-floor node
                                # inside the circle on the pair as diameter), keeping each sector's connectivity:
                                # 'detailed' = in pitch-0.2 sectors, 'all', or None. DICE's detailed-sector edges are 71 %
                                # Gabriel (RNG 50 %), 2.7 edges per node; NavKit's all-visible-pairs rule gave 15.9 per node
                                # there (174,024 edges against DICE's 24,710; ONS_MP_002, 2026-10-21)
DEDUP_GRAPH_NODES = True        # at most one route node per surface (build_graph; DICE: only border pairs share one)
SORT_NODES_BY_ID = True         # a sector's nodes in the file sorted by unsigned id, border nodes last (finalize_graph)
BORDER_OWNER = True             # node field c bit 16 only on the border copies the sector does not own (finalize_graph)
ORDER_NODES = True              # a route node at AI order positions far from any node (add_order_nodes; needs the level's
                                # order_points.json from levelobjects.py) - sp_004_b play test 2026-10-20: 0 failures. Was off until
                                # the sp_004_b play test confirms it (2026-10-20)
ORDER_NODE_MIN = 3.0            # ... when the nearest node on the same floor is farther than this (m)
ORDER_STATS: dict[str, Any] = {}                # nodes added, and those removed by prune_order_nodes (for reports)
ORDER_IDS: set[int] = set()               # ids of the order nodes of the current build
ORDER_MIN_ISLAND = 25           # an order node in a route island with fewer nodes is dropped (prune_order_nodes): such
                                # pockets took the nearest-node matches next to the main networks (sp_009: connectivity
                                # 66.99 without order nodes, 62.58 with all, 67.45 with this rule; sp_004_b keeps its
                                # stairs-top nodes and gains 0.26 connectivity / 0.6 AI pairwise). A size, not "the main
                                # network": campaign levels have several large networks (sp_004_b: 2,577 and 1,255
                                # nodes). Calibrated on campaign levels; to check on MP geometry (2026-10-20)
DETOUR_REPAIR = False           # add waypoint nodes where the route graph detours (repair_detours) - off: at 1.3x it
                                # adds 65 % nodes / 4x edges on sp_004_b, far tighter than DICE's own graph (2026-10-20)
DETOUR_TRIGGER = 3.0            # repair node pairs within DIST_EDGE_MAX whose route exceeds this x straight - above DICE's
                                # p90 (3.12; median 1.34, sp_004_b 2026-10-20); the failing go-to routes ran 3.15-6.38x
DETOUR_MAX = 1.3                # ... to a walk of at most this x straight (DICE at the failing spot: 1.02-1.11)
DETOUR_LINK_ALL = False         # also link new waypoints to all walkable neighbours within DIST_EDGE_MAX (4x the edges)
DETOUR_PASSES = 2               # pass A, then pass B on the repaired graph
LADDER_STATS: dict[str, int] = {}               # ladder links made, and ladders whose ends lie in two sectors (not linked yet)
BORDER_NODE_SPACING = 10.0
BRIDGE_UNION = True             # 45-degree bridges: outline from the exact union (bridge_union) - False: clip_bridges
MAX_WATER_DEPTH = 1.5           # ground deeper under water is not walkable (DICE's data: 99.9 % of its ground under
                                # water is within 1.45 m, median over 11 levels; navwater.py)
DETAIL_FEATURES = True          # D lines and F cells (navdetail.py): sides between two cells of one area whose heights
                                # differ by more than DETAIL_STEP are D lines (both sides) or, where a cell has both sides
                                # along an axis broken, F cells - and not outline (E), as in DICE's files. The runtime's
                                # straight-move test reads them. False: none (counts 0) and those sides stay outline -
                                # the output before 2026-10-26 (navkit.py build --no-detail, navbuild.py --no-detail)
DETAIL_HALF_CELLS = False       # the side cells of 45-degree bridges take part with their own height: a leg (their side
                                # toward a bridged area cell) breaks as any side (DICE's corner-cut legs with a step over
                                # 0.6 m to an area cell: 352 of 389 have D or F, sp_004_b). Off: NavKit's bridges are not
                                # DICE's corner cuts (it cuts more corners), and on NavKit's builds the legs cost D precision
                                # at pitch 0.8 (sp_01 44.6 -> 27.9 %, sp_004_b 59.3 -> 44.0 %) for 0-3 points of recall
DETAIL_IN_DETAILED = False      # D lines in detailed (pitch 0.2) sectors, by the terrain rule below. Off: NavKit's detailed
                                # areas join steep terrain DICE's do not, and the rule writes 10-20x DICE's lines there at
                                # 2.5-3 % precision (recall 41-69 %; sp_004_b, sp_01, ONS_MP_002, 2026-10-26) - lines that
                                # also keep route edges off them; the all-pitch D F1 fell on all three levels
DETAIL_F = {'normal': True, 'detailed': False}   # F cells per pitch; off: such cells' sides are D lines. In detailed zones
                                # NavKit's F matched none of DICE's (sp_004_b 4,898 records, precision 0.02 %; DICE has 20)
DETAIL_TERRAIN_ONLY = {'normal': False, 'detailed': True}   # sides break only between two terrain cells - in detailed
                                # zones DICE has D / F on 713 of 2,658,574 terrain sides and on 24 of 401,562 sides with an
                                # object cell (sp_004_b); at pitch 0.8 objects break too (sp_01: steps up to 1.2 m)
DETAIL_STEP = {'normal': 0.6, 'detailed': 0.175}   # the height difference (m) that breaks a side. Pitch 0.8: DICE's
                                # area link step - its in-area sides with a step from 0.61 m all have D / F (sp_004_b,
                                # sp_01, ONS_MP_002; spec 4.5). Pitch 0.2: DICE's detailed D sides start at 0.175 m, on
                                # terrain only (DETAIL_TERRAIN_ONLY): on DICE's own detailed areas, terrain sides over 0.173 m
                                # find 86 % of its D / F sides at 25 % precision (all surfaces: 15 %); was 0.3 m (NavKit's
                                # link step there) until the terrain rule - 0.175 on all surfaces gave 17x DICE's lines
DETAIL_BLOCKS_EDGES = True      # route edges do not cross a D line: line_walkable does not step straight across a D side
                                # of the sector being built. DICE's route edges cross D lines almost never (sp_004_b 1 of
                                # 24,514, sp_01 11 of 82,602) - its graph builder uses the same straight-move test; F cell
                                # sides they do cross (0.4-0.6 %), as NavKit's do. NavKit's edges crossed D lines on 1.7 %
                                # (sp_004_b) without it: the slope rule links some sides the 0.6 m step breaks (2026-10-26)
DETAIL_STATS: dict[str, int] = {}               # D lines, F cells and outline sides moved to D / F (for reports)


# ------------------------------------------------------------------------------------------------ the Rust core

# Two kinds of pieces run in the Rust core (rust/bc2-navbuild through bc2_native):
#  - the grid walks (round E1d): Level.neighbour's linking, components, trace_edges' side neighbours,
#    line_walkable, build_graph's route-group search, and navwalk's flood, islands_of and matched_flags. Their Python
#    was retired on 2026-11-17 after the full test tier; the module is required for them (require_native, which a
#    build calls first) and BFBC2_PURE_PYTHON does not change that. Their results are recorded in
#    tests/navbuild_golden.json (tests/test_navbuild_native.py); navregress stays the independent check against
#    DICE's path data.
#  - the walkability rules, build_areas and build_graph's cell picking (round E2a), and trace_edges, trace_emit,
#    add_cover_nodes, gabriel_prune and navwalk's roof_islands (round E3c): their Python stays the reference and the
#    fallback until they pass a full test tier; BFBC2_PURE_PYTHON=1 or use_native(False) runs it.

def require_native() -> Any:
    """The Rust core for the grid walks, or bfbc2core.native.NativeMissing saying how to get it."""
    suite_paths.use('core')
    from bfbc2core import native
    return native.require("NavKit's grid walks (navbuild.py / navwalk.py)", 'NavLevel')


def _load_native() -> Any:
    """The Rust core for E2a's pieces (build_areas, the cell picking; navwalk's rules), or None: not built, or
    BFBC2_PURE_PYTHON set (their Python runs)."""
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
_NATIVE_ERRORS = (ValueError, TypeError, OverflowError)   # E2a's pieces: the Rust core refused, the Python code runs


def backend() -> str:
    """'rust' when E2a's pieces run in the Rust core, else 'python' (the grid walks always run in it)."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """E2a's pieces in the Rust core (when built, whatever BFBC2_PURE_PYTHON says) or in Python for this process,
    navwalk's rules too; returns backend(). The grid walks are not switched: they have no Python."""
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
    navwalk.use_native(on)                   # its rules too
    navsample.use_native(on)                 # and the sample files' records
    return backend()


def water_tops(s: navsample.Samples, vertices: np.ndarray | None, triangles: np.ndarray) -> np.ndarray:
    """The water surface height over every sample column (-inf where there is no water). The columns are a regular
    grid, so each water triangle only visits the columns its bounding box covers."""
    nx, nz, p = s.nx, s.nz, s.pitch
    top = np.full((nz, nx), -np.inf)
    if vertices is None or not len(triangles):
        return top.ravel()
    tri = np.asarray(vertices, dtype=np.float64)[np.asarray(triangles)]
    for (ax, ay, az), (bx, by, bz), (cx, cy, cz) in tri:
        i0 = max(0, int(math.floor((min(ax, bx, cx) - s.min_x) / p)))
        i1 = min(nx - 1, int(math.ceil((max(ax, bx, cx) - s.min_x) / p)))
        j0 = max(0, int(math.floor((min(az, bz, cz) - s.min_z) / p)))
        j1 = min(nz - 1, int(math.ceil((max(az, bz, cz) - s.min_z) / p)))
        if i0 > i1 or j0 > j1:
            continue
        d = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz)
        if abs(d) < 1e-12:
            continue
        X = s.min_x + np.arange(i0, i1 + 1) * p
        Z = s.min_z + np.arange(j0, j1 + 1) * p
        X, Z = np.meshgrid(X, Z)
        l1 = ((bz - cz) * (X - cx) + (cx - bx) * (Z - cz)) / d
        l2 = ((cz - az) * (X - cx) + (ax - cx) * (Z - cz)) / d
        l3 = 1 - l1 - l2
        inside = (l1 >= -1e-9) & (l2 >= -1e-9) & (l3 >= -1e-9)
        if inside.any():
            h = np.where(inside, l1 * ay + l2 * by + l3 * cy, -np.inf)
            sub = top[j0:j1 + 1, i0:i1 + 1]
            np.maximum(sub, h, out=sub)
    return top.ravel()


def outside_bounds(s: navsample.Samples, bounds: Sequence[float]) -> np.ndarray:
    """Surfaces whose column lies outside bounds (minX, minZ, maxX, maxZ) - bool per surface (CLIP_TO_REFERENCE)."""
    col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start[:s.nx * s.nz + 1]))
    x, z = s.min_x + (col % s.nx) * s.pitch, s.min_z + (col // s.nx) * s.pitch
    e = 1e-3                                             # edges inclusive (float noise of the sample grid)
    return (x < bounds[0] - e) | (x > bounds[2] + e) | (z < bounds[1] - e) | (z > bounds[3] + e)


def deep_water(s: navsample.Samples, water: tuple[np.ndarray, np.ndarray] | None, limit: float = MAX_WATER_DEPTH) -> np.ndarray:
    """Surfaces more than limit metres under the level's water (bool per surface). water: (vertices, triangles)."""
    if water is None:
        return np.zeros(len(s.y), dtype=bool)
    top = water_tops(s, water[0], water[1])
    col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start[:s.nx * s.nz + 1]))
    return (top[col] - s.y.astype(np.float64)) > limit
AREA_Y_MIN = 0.0                # area height range relative to its ground: DICE's convention (see build())
AREA_Y_MAX = 1.52
Y_OFFSET = 0.95                 # outline / node height above the ground (EntityHeight / 2)
HEADER = (2, 2, 0, 1, 0, 0, 0, 0, 2, 0, 8, 0) + (0,) * 20 + (36, 12, 1, 0)
DETAILED_PITCH = 0.2            # DICE's detailed zones: whole sectors at pitch 0.2, resolution 0.05, 6 x 6 tiles of 20 m
                                # (header rect -0.2 .. 100.22; KYNAPSE_SPEC.md 4.5). Built in a second pass (build(...,
                                # detailed=...)) from a pitch-0.2 sampling run, merged with the normal build.
CARRY_RADIUS = 1.0              # a shared node carried across a mixed-pitch seam is attached to ground within this (m)
DETAILED_LINK_STEP = 0.3        # LINK_STEP in detailed zones: DICE's one-cell height steps inside its pitch-0.2 areas
                                # (ONS_MP_002, 10,353 outline segments) p99 0.31, p99.9 0.42, max 0.49 m - about half its
                                # pitch-0.8 values (p99 0.96, max 1.03), not a quarter: the same slope (0.15 m) cut sloped
                                # ground into 3x DICE's areas (8,400 against 2,640; 2026-10-21)


def grid_settings(p: float) -> dict[str, Any]:
    """The grid settings of a build at pitch p - 0.8 (normal sectors) or 0.2 (detailed zones) - as BuildSettings
    fields: sector size in samples, window, tiles, mesh resolution and the area link step."""
    sector_samples = int(round(100 / p))                  # 125 / 500
    detailed = abs(p - DETAILED_PITCH) < 1e-6
    return dict(pitch=p, sector_samples=sector_samples,
                window=range(-1, sector_samples + 2),     # two samples of overlap with each neighbour
                tiles=6 if detailed else 3,
                tile=int(round((20 if detailed else 40) / p)),   # 100 / 50 samples
                resolution=0.05 if detailed else 0.025,
                link_step=DETAILED_LINK_STEP if detailed else 0.6)


@dataclass(frozen=True)
class BuildSettings:
    """The settings one build reads, passed through build / stage / merge_detailed and their helpers (no build reads
    a module-level setting: the module's names above are the documented defaults, read-only). Defaults: the normal
    infantry build at pitch 0.8. detailed() gives the pitch-0.2 pass's settings; dataclasses.replace any other change
    (navvehicle.vehicle_settings, navkit.py --no-detail: detail_features=False)."""
    pitch: float = PITCH
    sector_samples: int = SECTOR_SAMPLES
    window: range = WINDOW
    tile: int = TILE
    tiles: int = TILES
    resolution: float = RESOLUTION
    link_step: float = LINK_STEP
    radius: float = RADIUS
    dist_edge_max: float = DIST_EDGE_MAX
    y_offset: float = Y_OFFSET
    area_y_min: float = AREA_Y_MIN
    area_y_max: float = AREA_Y_MAX
    border_node_spacing: float = BORDER_NODE_SPACING
    node_cover: dict[str, float | None] = field(default_factory=lambda: dict(NODE_COVER))
    order_nodes: bool = ORDER_NODES
    clip_to_reference: bool = CLIP_TO_REFERENCE
    one_way_portals: str = ONE_WAY_PORTALS
    detail_features: bool = DETAIL_FEATURES
    detail_step: dict[str, float] = field(default_factory=lambda: dict(DETAIL_STEP))
    detail_blocks_edges: bool = DETAIL_BLOCKS_EDGES
    tile_20m_sectors: frozenset[int] | None = TILE_20M_SECTORS

    def at_pitch(self, p: float) -> 'BuildSettings':
        """These settings with the grid of pitch p (grid_settings)."""
        return replace(self, **grid_settings(p))

    def detailed(self) -> 'BuildSettings':
        """These settings for the detailed-zone pass (pitch 0.2, 6 x 6 tiles of 20 m, resolution 0.05)."""
        return self.at_pitch(DETAILED_PITCH)

    @property
    def kind(self) -> str:
        """'detailed' at pitch 0.2, else 'normal' (the key of the per-pitch settings)."""
        return 'detailed' if abs(self.pitch - DETAILED_PITCH) < 1e-6 else 'normal'


DEFAULTS = BuildSettings()


def reset_stats() -> None:
    """Clears the per-build report counters and id sets: this module's, navislands' and navwalk's (build() at its
    start; ORDER_IDS also decides which order nodes prune_order_nodes may drop)."""
    for d in (ORDER_IDS, ORDER_STATS, LADDER_STATS, DETAIL_STATS, navislands.TERRAIN_IDS, navwalk.ROOF_STATS,
              navwalk.OBJECTIVE_STATS, navwalk.STEEP_STATS):
        d.clear()


class Level:
    """Kept walkable surfaces on the global sample grid, with the step-rule connectivity (cfg: the build's settings -
    its pitch and link step)."""

    def __init__(self, s: navsample.Samples, keep: np.ndarray, step: float, gx: np.ndarray | None = None,
                 gz: np.ndarray | None = None, stair: np.ndarray | None = None, cfg: BuildSettings = DEFAULTS) -> None:
        self.s, self.keep, self.step, self.cfg = s, keep, step, cfg
        self.stair = stair if stair is not None else np.zeros(len(s.y), bool)
        self.y = s.y.astype(float)
        ny = np.maximum(s.normal[:, 1], 0.05)
        self.gx = gx if gx is not None else -s.normal[:, 0] / ny
        self.gz = gz if gz is not None else -s.normal[:, 2] / ny
        self.col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
        self.terrain = (s.hit[:, 0] == navwalk.TERRAIN_SHAPE_VTABLE) if s.hit is not None else np.ones(len(s.y), bool)
        self.blocked: set[tuple[int, int]] = set()       # (surface, surface) pairs line_walkable does not step across (D sides; DETAIL_BLOCKS_EDGES)

    def native(self) -> Any:
        """This level in the Rust core (made on first use; the arrays as it takes them: navwalk.grid_arrays)."""
        nat = self.__dict__.get('_native')
        if nat is None:
            walk, gx, gz, stair = navwalk.grid_arrays(self.keep, self.gx, self.gz, self.stair)
            nat = require_native().NavLevel(
                int(self.s.nx), int(self.s.nz), np.ascontiguousarray(self.s.start, '<i8').tobytes(), walk.tobytes(),
                np.ascontiguousarray(self.y, '<f8').tobytes(), gx.tobytes(), gx.dtype == np.float32, gz.tobytes(),
                gz.dtype == np.float32, stair.tobytes(), np.ascontiguousarray(self.terrain, bool).tobytes(),
                float(self.cfg.pitch), float(self.step), float(self.cfg.link_step))
            self._native = nat
        return nat

    def sync_blocked(self, nat: Any) -> None:
        """The Rust level's D sides made this level's blocked set (when it is another set or has grown)."""
        b = self.blocked
        if self.__dict__.get('_blocked_obj') is not b or self.__dict__.get('_blocked_len') != len(b):
            nat.set_blocked([(int(x), int(y)) for x, y in b])
            self._blocked_obj, self._blocked_len = b, len(b)

    def surfaces(self, gi: int, gj: int) -> Sequence[int]:
        if not (0 <= gi < self.s.nx and 0 <= gj < self.s.nz):
            return range(0)
        c = gj * self.s.nx + gi
        return [k for k in range(self.s.start[c], self.s.start[c + 1]) if self.keep[k]]

    def connected(self, k: int, n: int, di: int, dj: int, rule: str | None = None) -> bool:
        """Surface n (direction di, dj from k) links to k by rule ('slope', 'raw', 'terrain'; LINK_RULE if None)."""
        rule = rule or LINK_RULE
        if rule == 'slope' or (rule == 'terrain' and not (self.terrain[k] and self.terrain[n])):
            return navwalk.step_fits(self.y[n] - self.y[k], (self.gx[k], self.gz[k]), (self.gx[n], self.gz[n]),
                                     self.stair[k], self.stair[n], di, dj, self.cfg.pitch, self.step)
        y = self.y
        if not (di and dj):
            return abs(y[n] - y[k]) <= self.cfg.link_step
        # diagonal: through either side column, over any surface there (walkable or not)
        c = self.col_of[k]
        gi, gj = c % self.s.nx, c // self.s.nx
        for a, b in ((gi + di, gj), (gi, gj + dj)):
            if 0 <= a < self.s.nx and 0 <= b < self.s.nz:
                m0 = b * self.s.nx + a
                for m in range(self.s.start[m0], self.s.start[m0 + 1]):
                    if abs(y[m] - y[k]) <= self.cfg.link_step and abs(y[n] - y[m]) <= self.cfg.link_step:
                        return True
        return False

    def neighbour(self, k: int, gi: int, gj: int, di: int, dj: int, rule: str | None = None) -> int | None:
        """The kept surface in column (gi+di, gj+dj) connected to surface k (connected's rule; the nearest in
        height, the first of equals), or None; in the Rust core. ValueError for a surface outside the level."""
        return cast(int | None, self.native().neighbour(int(k), int(gi), int(gj), int(di), int(dj), rule or LINK_RULE))


class _AreaOf(Mapping[int, int]):
    """A stage sector's area_of (each cell's surface -> its area index, the last area holding it, as the dict
    {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}) answered by the Rust core's copy of the areas
    (NavAreas.area_at); the dict itself is made only when something iterates it."""

    def __init__(self, store: Any, n: int, areas: Sequence[AreaInfo]) -> None:
        self._store, self._n, self._areas = store, n, areas
        self._dict: dict[int, int] | None = None

    def _full(self) -> dict[int, int]:
        if self._dict is None:
            self._dict = {c[2]: ai for ai, a in enumerate(self._areas) for c in a['cells']}
        return self._dict

    def __getitem__(self, k: int) -> int:
        if self._dict is not None:
            return self._dict[k]
        got = self._store.area_at(self._n, int(k)) if isinstance(k, (int, np.integer)) else None
        if got is None:
            raise KeyError(k)
        return cast(int, got)

    def __contains__(self, k: object) -> bool:
        if self._dict is not None:
            return k in self._dict
        return isinstance(k, (int, np.integer)) and self._store.area_at(self._n, int(k)) is not None

    def __iter__(self) -> Any:
        return iter(self._full())

    def __len__(self) -> int:
        return len(self._full())


def sorted_unique(a: np.ndarray) -> np.ndarray:
    """np.unique of an array already sorted ascending (a surface's column grows with the surface): the first of each
    run of equal values - the same array, without np.unique's hashing (2 s of sp_004_b's build)."""
    a = np.asarray(a)
    if not len(a):
        return a.copy()
    return a[np.concatenate(([True], a[1:] != a[:-1]))]


def sector_origin_index(level: Level, X: int, Y: int, xoff: int, zoff: int,
                        cfg: BuildSettings = DEFAULTS) -> tuple[int, int, float, float]:
    """Global sample index of the sector's local sample (0, 0)."""
    s = level.s
    ox, oz = (X + xoff) * 100.0, (Y + zoff) * 100.0
    return int(round((ox - s.min_x) / cfg.pitch)), int(round((oz - s.min_z) / cfg.pitch)), ox, oz


def column_sectors(s: Any, cols: Any, xoff: int, zoff: int, pitch: float) -> tuple[Any, Any]:
    """The sector (X, Y, offsets applied) of each sample column in cols, as DICE's path data places it: floor(x / 100)
    of the column's position x = min + index * pitch, with the grid minimum as stored (float32, pitch-aligned).

    A column on a sector line belongs to the sector below when the float32 minimum puts it a hair under the line,
    and that is DICE's layout, not an error: SP_008's grid starts at -910.4 (stored -910.4000244), its x = -900
    column comes out -900.0000244, and DICE's file has the six sectors X = -10 (ids 257-262) holding only that
    column and its margin (outlines at local x 99.6 .. 101.2). Assigning line columns to the sector above
    (sector_origin_index's local 0) loses those six sectors (navregress 2026-11-14: sectors_missing 0 -> 6, ground
    97.63 -> 97.30); on every other level the two rules give the same sectors."""
    cols = np.asarray(cols, dtype=np.int64)
    x = float(s.min_x) + (cols % s.nx) * pitch
    z = float(s.min_z) + (cols // s.nx) * pitch
    return np.floor(x / 100.0).astype(np.int64) - xoff, np.floor(z / 100.0).astype(np.int64) - zoff


def tiling(sid: int, cfg: BuildSettings = DEFAULTS) -> tuple[int, int]:
    """(samples per tile, tiles per axis) of a sector."""
    return (25, 6) if cfg.tile_20m_sectors and sid in cfg.tile_20m_sectors else (cfg.tile, cfg.tiles)


def _build_areas_many(level: Level, specs: Sequence[tuple[Any, ...]],
                      cfg: BuildSettings) -> list[tuple[list[AreaInfo], list[tuple[float, float]]]] | None:
    """build_areas for every sector of a stage at once, in parallel in the Rust core: specs = [(gi0, gj0, ox, oz,
    start, tile, tiles)]; per sector (areas, [(lowest, highest) level.y of each area's cells]). None: the caller runs
    build_areas sector by sector (E2a's Python, or the core refused)."""
    return _build_areas_kept(level, specs, cfg, False)[0]


def _build_areas_kept(level: Level, specs: Sequence[tuple[Any, ...]], cfg: BuildSettings, store: bool = True
                      ) -> tuple[list[tuple[list[AreaInfo], list[tuple[float, float]]]] | None, Any]:
    """_build_areas_many, and (store) the areas kept in the Rust core as well (a NavAreas, in the specs' order: the
    stage's tracing and route graphs take their cells from it - round E3h), or None."""
    w = cfg.window
    if (_native is None or w.step != 1 or type(MAX_AREA) is not int
            or not all(len(sp[4]) == 2 and all(type(v) is int for v in (*sp[4], sp[5], sp[6])) for sp in specs)):
        return None, None
    kept = None
    try:
        got = level.native().build_areas_many(
            [(sp[0], sp[1], (w.start, w.stop), (sp[4][0], sp[4][1]), sp[5], sp[6]) for sp in specs],
            MAX_AREA if BISECT_AREAS else None, AREA_LINK, store=store)
    except _NATIVE_ERRORS:
        return None, None
    if store:
        got, kept = got
    return [([{'tile': t, 'cells': c} for t, c, _lo, _hi in sector], [(lo, hi) for _t, _c, lo, hi in sector])
            for sector in got], kept


def build_areas(level: Level, gi0: int, gj0: int, start: Sequence[int] = (-1, -1), tile: int | None = None,
                tiles: int | None = None, cfg: BuildSettings = DEFAULTS) -> list[AreaInfo]:
    """Areas of one sector: list of dicts {tile: (tx, tz), cells: [(i, j, k)]} (local i, j). start: the local sample
    index where the tiles begin on x and z (-1, or 0 on a side with no neighbouring sector - EDGE_SECTOR_TILES); tile,
    tiles: samples per tile and tiles per axis (cfg's if None)."""
    tile = cfg.tile if tile is None else tile
    tiles = cfg.tiles if tiles is None else tiles
    w = cfg.window
    if (_native is not None and w.step == 1 and len(start) == 2 and all(type(v) is int for v in (*start, tile, tiles))
            and type(MAX_AREA) is int):
        # in the Rust core: the tiles, their components and the bisection in one call (E2a: the Python below is
        # its reference)
        try:
            got = level.native().build_areas(gi0, gj0, (w.start, w.stop), (start[0], start[1]), tile, tiles,
                                             MAX_AREA if BISECT_AREAS else None, AREA_LINK)
            return [{'tile': t, 'cells': c} for t, c in got]
        except _NATIVE_ERRORS:
            pass
    cells_by_tile: defaultdict[tuple[int, int], list[Cell]] = defaultdict(list)
    for j in cfg.window:
        for i in cfg.window:
            for k in level.surfaces(gi0 + i, gj0 + j):
                tx = min(max(i - start[0], 0) // tile, tiles - 1)
                tz = min(max(j - start[1], 0) // tile, tiles - 1)
                cells_by_tile[(tx, tz)].append((i, j, k))
    areas = []
    for tkey in sorted(cells_by_tile):
        members = {k: (i, j) for i, j, k in cells_by_tile[tkey]}
        for comp in components(level, members, gi0, gj0):
            for piece in (bisect(level, comp, gi0, gj0) if BISECT_AREAS else [comp]):
                areas.append({'tile': tkey, 'cells': piece})
    return areas


def components(level: Level, members: dict[int, tuple[int, int]], gi0: int, gj0: int) -> list[list[Cell]]:
    """8-connected components (AREA_LINK rule) of the given surfaces {k: (i, j)}. An area is single-layer in plan: a
    surface whose column the component already holds (a second floor reached round a corner) starts its own.
    Breadth first from each surface in members' order, the 8 directions +x, -x, +z, -z, then the diagonals; in the
    Rust core."""
    return cast(list[list[Cell]], level.native().components(
        [(int(k), int(i), int(j)) for k, (i, j) in members.items()], int(gi0), int(gj0), AREA_LINK))


def bisect(level: Level, comp: list[Cell], gi0: int, gj0: int) -> list[list[Cell]]:
    i0, i1 = min(c[0] for c in comp), max(c[0] for c in comp)
    j0, j1 = min(c[1] for c in comp), max(c[1] for c in comp)
    if i1 - i0 + 1 <= MAX_AREA and j1 - j0 + 1 <= MAX_AREA:
        return [comp]
    if i1 - i0 >= j1 - j0:
        mid = (i0 + i1 + 1) // 2
        halves = ([c for c in comp if c[0] < mid], [c for c in comp if c[0] >= mid])
    else:
        mid = (j0 + j1 + 1) // 2
        halves = ([c for c in comp if c[1] < mid], [c for c in comp if c[1] >= mid])
    out = []
    for h in halves:
        for sub in components(level, {k: (i, j) for i, j, k in h}, gi0, gj0):
            out.extend(bisect(level, sub, gi0, gj0))
    return out


def clip_bridges(bsegs: Iterable[tuple[int, Vec3, Vec3]],
                 bridges: Sequence[tuple[int, int, int, int]]) -> list[tuple[int, Vec3, Vec3]]:
    """Outline of overlapping 45-degree bridges is the boundary of their union: where edges of two bridges
    cross (one cell with diagonal-only neighbours on both sides of an empty cell), split them at the crossing
    and drop the pieces lying inside the other bridge's strip (half-width 1/sqrt 2 sample about the diagonal)."""
    def inside(px: float, pz: float, b: int) -> bool:
        i, j, di, dj = bridges[b]
        rx, rz = px - i, pz - j
        along = (rx * di + rz * dj) / 2.0                   # 0..1 between the two cell centres
        perp = abs(rx * dj - rz * di) / math.sqrt(2.0)
        return 0.0 < along < 1.0 and perp < 1.0 / math.sqrt(2.0) - 1e-6

    out = list(bsegs)
    changed = True
    while changed:
        changed = False
        for u in range(len(out)):
            for v in range(u + 1, len(out)):
                (bu, p0, p1), (bv, q0, q1) = out[u], out[v]
                if bu == bv:
                    continue
                d = (p1[0] - p0[0]) * (q1[1] - q0[1]) - (p1[1] - p0[1]) * (q1[0] - q0[0])
                if abs(d) < 1e-9:
                    continue
                t = ((q0[0] - p0[0]) * (q1[1] - q0[1]) - (q0[1] - p0[1]) * (q1[0] - q0[0])) / d
                s = ((q0[0] - p0[0]) * (p1[1] - p0[1]) - (q0[1] - p0[1]) * (p1[0] - p0[0])) / d
                if not (1e-6 < t < 1 - 1e-6 and 1e-6 < s < 1 - 1e-6):
                    continue
                pieces = []
                for (b, a0, a1, f), other in (((bu, p0, p1, t), bv), ((bv, q0, q1, s), bu)):
                    m = (a0[0] + (a1[0] - a0[0]) * f, a0[1] + (a1[1] - a0[1]) * f, a0[2] + (a1[2] - a0[2]) * f)
                    for c0, c1 in ((a0, m), (m, a1)):
                        if not inside((c0[0] + c1[0]) / 2, (c0[1] + c1[1]) / 2, other):
                            pieces.append((b, c0, c1))
                out = [x for w, x in enumerate(out) if w not in (u, v)] + pieces
                changed = True
                break
            if changed:
                break
    return out


def bridge_union(bridges: Sequence[tuple[int, int, int, int]], cell_k: dict[tuple[int, int], int], level: Level,
                 cfg: BuildSettings = DEFAULTS) -> tuple[set[tuple[Any, ...]], list[tuple[Vec3, Vec3]]]:
    """BRIDGE_UNION: the outline of an area's 45-degree bridges as the boundary of their union, exactly. A bridge
    between diagonal-only cells A (i, j) and B (i + di, j + dj) covers half of each of the two empty cells beside the
    shared corner - per empty cell, the two quarter triangles (base: a cell side, apex: the centre) whose bases touch
    that corner. Over the set of covered quarters of the empty cells, a half-diagonal (cell corner to centre) shared
    by two quarters is interior; the others are the bridges' outline - halves of one diagonal joined. A side of an
    area cell is covered (no outline there) where the empty cell's quarter on it is. Returns (covered sides in
    trace_edges' keys, [((ax, az, ya), (bx, bz, yb))] in local sample units). With non-overlapping bridges this is
    exactly the two diagonals per bridge; overlapping ones (chains, a cell shared by two corners) close where
    clip_bridges left open ends (1.7-3.1 % of areas on sp_004_b / sp_01, 2026-10-23)."""
    quarters = {}                                   # (cell i, j, base side (sx, sz)) -> (bridge index, ya, yb)
    order = []
    for bi, (i, j, di, dj) in enumerate(bridges):
        ya = level.y[cell_k[(i, j)]] + cfg.y_offset
        yb = level.y[cell_k[(i + di, j + dj)]] + cfg.y_offset
        # empty cell (i, j + dj): its sides towards A (0, -dj) and towards B (di, 0); empty cell (i + di, j): towards
        # A (-di, 0) and towards B (0, dj) - the order clip_bridges emitted them in
        for (ei, ej), toward_a, toward_b in (((i, j + dj), (0, -dj), (di, 0)), ((i + di, j), (-di, 0), (0, dj))):
            for side in (toward_a, toward_b):
                q = (ei, ej, side)
                if q not in quarters:
                    quarters[q] = (bi, ya, yb, toward_a, toward_b)
                    order.append(q)
    def corners(ei: int, ej: int, side: tuple[int, int]) -> tuple[tuple[int, int], tuple[int, int]]:                      # the two corners of a quarter's base, in half units
        sx, sz = side
        if sx:
            return ((2 * ei + sx, 2 * ej - 1), (2 * ei + sx, 2 * ej + 1))
        return ((2 * ei - 1, 2 * ej + sz), (2 * ei + 1, 2 * ej + sz))
    count: defaultdict[tuple[tuple[int, int], int, int], int] = defaultdict(int)
    for (ei, ej, side) in quarters:
        for c in corners(ei, ej, side):
            count[(c, ei, ej)] += 1                 # half-diagonal from corner c to the centre of (ei, ej)
    drop = set()
    for (ei, ej, side) in quarters:
        sx, sz = side
        drop.add(('x', ei + sx * 0.5, ej) if sx else ('z', ej + sz * 0.5, ei))
    emitted: set[tuple[tuple[int, int], int, int]]
    segs, done, emitted = [], set(), set()
    for (ei, ej, side) in order:
        bi, ya, yb, toward_a, toward_b = quarters[(ei, ej, side)]
        if (ei, ej, bi) in done:
            continue
        done.add((ei, ej, bi))
        # this bridge's diagonal through the cell: from A's outer corner to B's outer corner, through the centre
        ca = [c for c in corners(ei, ej, toward_a) if count[(c, ei, ej)] == 1 and (ei, ej, toward_a) in quarters
              and (c, ei, ej) not in emitted]
        cb = [c for c in corners(ei, ej, toward_b) if count[(c, ei, ej)] == 1 and (ei, ej, toward_b) in quarters
              and (c, ei, ej) not in emitted]
        emitted.update((c, ei, ej) for c in ca + cb)
        ctr = (float(ei), float(ej), (ya + yb) / 2.0)
        pa = [(c[0] / 2.0, c[1] / 2.0, ya) for c in ca]
        pb = [(c[0] / 2.0, c[1] / 2.0, yb) for c in cb]
        if len(pa) == 1 and len(pb) == 1:
            segs.append((pa[0], pb[0]))
        else:
            segs.extend((p, ctr) for p in pa)
            segs.extend((ctr, p) for p in pb)
    return drop, segs


def trace(level: Level, sector: tuple[int, int], areas: Sequence[AreaInfo], area_of: dict[int, int], gi0: int,
          gj0: int, ox: float, oz: float, cfg: BuildSettings = DEFAULTS) -> list[Traced]:
    """Boundary edges of every area -> merged runs. Returns per area: portals, borders, outlines. One sector on its
    own: one-way portal pieces are kept ('keep') or become outline (otherwise) - build() decides them level-wide
    ('restore', restore_one_way)."""
    per_area, one_way = trace_edges(level, areas, area_of, gi0, gj0, cfg)
    drop_one_way(per_area, one_way, keep=set(range(len(one_way))) if cfg.one_way_portals == 'keep' else set())
    return trace_emit(level, areas, per_area, gi0, gj0, ox, oz, cfg)


def trace_edges(level: Level, areas: Sequence[AreaInfo], area_of: dict[int, int], gi0: int, gj0: int,
                cfg: BuildSettings = DEFAULTS) -> tuple[list[EdgeRows], list[OneWay]]:
    """Boundary edges of every area: (per area {(kind, target, axis, line, dir): [(along, y)]}, one-way candidates).
    Portals are symmetric (SYMMETRIC_PORTALS): every portal edge A -> B also stands as B -> A, replacing an outline
    edge B had there - neighbour() picks the nearest surface in height, so with two surfaces in a column the two
    sides could disagree and leave a one-way portal (22 % of them on sp_004_b before). Where the target's side is
    its own interior (a second surface of the target in the source's column) the piece cannot be mirrored: it is a
    one-way candidate (source area, target area, key, along) for drop_one_way. In the Rust core when it is there (round
    E3c; the Python below is its reference and fallback)."""
    if _native is not None and cfg.window.step == 1 and isinstance(cfg.y_offset, float) and isinstance(level, Level):
        try:
            got = level.native().trace_edges([a['cells'] for a in areas], area_of, gi0, gj0,
                                             (cfg.window.start, cfg.window.stop), cfg.y_offset, LINK_RULE,
                                             bool(SYMMETRIC_PORTALS))
            return cast(tuple[list[EdgeRows], list[OneWay]], got)
        except _NATIVE_ERRORS:
            pass
    per_area: list[EdgeRows] = []
    one_way: list[OneWay] = []
    mirror: defaultdict[Any, dict[tuple[Any, ...], Any]]
    mirror = defaultdict(dict)       # area -> {(source area, axis, line, dir, along): y of its own cell there}
    side_kind: list[dict[tuple[Any, ...], tuple[str, Any]]] = []                   # per area: {(axis, line, dir, along): (kind 'I' interior / 'B' / 'C' / 'E', target)}
    for ai, area in enumerate(areas):
        edges: EdgeRows = defaultdict(list)    # (kind, target, axis, line, dir) -> [(along, y0, y1)]
        sides: dict[tuple[Any, ...], tuple[str, Any]] = {}
        target: Any
        # the neighbour across each side (+x, -x, +z, -z) of each cell, all at once in the Rust core: neighbour's, but
        # a connected surface of this area itself first
        sides_n = level.native().side_neighbours(area['cells'], gi0, gj0, LINK_RULE)
        for ci_, (i, j, k) in enumerate(area['cells']):
            yk = level.y[k] + cfg.y_offset
            for d_, (di, dj) in enumerate(((1, 0), (-1, 0), (0, 1), (0, -1))):
                side = ('x', i + di * 0.5, di, j) if di else ('z', j + dj * 0.5, dj, i)
                # a side is interior when any connected surface across it is in this area (two connected
                # surfaces in one column - stair edge, split-off layer - must not leave a stray portal inside)
                n = sides_n[4 * ci_ + d_]
                ni, nj = i + di, j + dj
                if n is not None and ni in cfg.window and nj in cfg.window:
                    t = area_of.get(n)
                    if t == ai:
                        sides[side] = ('I', None)
                        continue
                    kind, target = ('B', t) if t is not None else ('E', None)
                elif n is not None:
                    kind, target = 'C', (di, dj)
                else:
                    kind, target = 'E', None
                if di:   # vertical line x = const
                    key = (kind, target, 'x', i + di * 0.5, di)
                    edges[key].append((j, yk))
                else:
                    key = (kind, target, 'z', j + dj * 0.5, dj)
                    edges[key].append((i, yk))
                sides[side] = (kind, target)
                if kind == 'B':
                    axis, line, along = ('x', i + di * 0.5, j) if di else ('z', j + dj * 0.5, i)
                    mirror[target][(ai, axis, line, -(di or dj), along)] = level.y[n] + cfg.y_offset
        per_area.append(edges)
        side_kind.append(sides)
    if SYMMETRIC_PORTALS:
        # only where the target's side is plain outline: an interior side (the column holds a surface of the target
        # too) or a side holding another portal stays as it is - a portal there would break the area's outline
        for t, need in mirror.items():
            edges = per_area[t]
            for (src, axis, line, d, along), y in need.items():
                there = side_kind[t].get((axis, line, d, along), ('E', None))
                if there == ('B', src):                  # a portal back already
                    continue
                if there[0] != 'E':
                    key = ('B', t, axis, line, -d)
                    if any(r[0] == along for r in per_area[src].get(key, [])):
                        one_way.append((src, t, key, along))
                    continue
                for key in [k for k in edges if k[0] == 'E' and k[2] == axis and k[3] == line and k[4] == d]:
                    edges[key] = [r for r in edges[key] if r[0] != along]
                edges[('B', src, axis, line, d)].append((along, y))
                side_kind[t][(axis, line, d, along)] = ('B', src)
    return per_area, one_way


def _trace_edges_many(level: Level, sectors: Sequence[tuple[tuple[int, int], dict[str, Any]]], cfg: BuildSettings,
                      store: Any = None) -> dict[tuple[int, int], tuple[list[EdgeRows], list[OneWay]]] | None:
    """trace_edges for every sector of a stage at once, in parallel in the Rust core (each sector's area_of as stage
    makes it: each cell's surface to its area); None: the caller runs trace_edges sector by sector. store: the
    sectors' areas kept in the core (_build_areas_kept, in the order of sectors), or None: their cells are sent."""
    if _native is None or cfg.window.step != 1 or not isinstance(cfg.y_offset, float):
        return None
    try:
        if store is not None:
            got = level.native().trace_edges_stored(
                store, [(inf['origin'][0], inf['origin'][1]) for _sec, inf in sectors],
                (cfg.window.start, cfg.window.stop), cfg.y_offset, LINK_RULE, bool(SYMMETRIC_PORTALS))
        else:
            got = level.native().trace_edges_many(
                [([a['cells'] for a in inf['areas']], inf['origin'][0], inf['origin'][1]) for _sec, inf in sectors],
                (cfg.window.start, cfg.window.stop), cfg.y_offset, LINK_RULE, bool(SYMMETRIC_PORTALS))
    except _NATIVE_ERRORS:
        return None
    return {sec: (per_area, one_way) for (sec, _inf), (per_area, one_way) in zip(sectors, got)}


def _trace_edges_kept(level: Level, store: Any, sectors: Sequence[tuple[tuple[int, int], dict[str, Any]]],
                      cfg: BuildSettings) -> list[list[OneWay]] | None:
    """trace_edges for every sector of a stage at once, in parallel in the Rust core, the rows kept there (round E3h:
    store.drop_one_way, edges_of / set_edges, trace_emit_many read them); per sector its one-way candidates, or None:
    the rows are made as Python dicts (_trace_edges_many; also ONE_WAY_PORTALS 'restore', which reads them all)."""
    if (store is None or _native is None or cfg.window.step != 1 or not isinstance(cfg.y_offset, float)
            or cfg.one_way_portals == 'restore'):
        return None
    try:
        return cast(list[list[OneWay]], level.native().trace_edges_stored(
            store, [(inf['origin'][0], inf['origin'][1]) for _sec, inf in sectors],
            (cfg.window.start, cfg.window.stop), cfg.y_offset, LINK_RULE, bool(SYMMETRIC_PORTALS), keep_edges=True))
    except _NATIVE_ERRORS:
        return None


def _edges_from_store(store: Any, sectors: Sequence[tuple[tuple[int, int], dict[str, Any]]],
                      one_ways: Sequence[list[OneWay]] | None) -> dict[tuple[int, int], tuple[list[EdgeRows], list[OneWay]]]:
    """The edge rows the Rust core keeps, as trace_edges' dicts (for the Python code when the core refuses a step)."""
    return {sec: (store.edges_of(n, list(range(len(inf['areas'])))), list(one_ways[n]) if one_ways else [])
            for n, (sec, inf) in enumerate(sectors)}


def _detail_candidates(level: Level, store: Any, cfg: BuildSettings) -> list[list[int]] | None:
    """Per sector of store, the areas detail_pass may find broken sides in (two cells side by side more than the
    detail step apart: Rust, in parallel), or None: all of them (half cells on, a step not a number, a refusal)."""
    if (not cfg.detail_features or DETAIL_HALF_CELLS or not _number(cfg.detail_step.get(cfg.kind))):
        return None
    try:
        return cast(list[list[int]], level.native().detail_candidates(store, float(cfg.detail_step[cfg.kind])))
    except _NATIVE_ERRORS:
        return None


def drop_one_way(per_area: Sequence[EdgeRows], one_way: Sequence[OneWay], keep: Container[int]) -> None:
    """One-way candidates (trace_edges) not in keep (indices) become outline on their source area."""
    for n, (src, _t, key, along) in enumerate(one_way):
        if n in keep:
            continue
        se = per_area[src]
        ys = [r for r in se.get(key, []) if r[0] == along]
        se[key] = [r for r in se.get(key, []) if r[0] != along]
        se[('E', None, key[2], key[3], key[4])].extend(ys)


def restore_one_way(sectors: Mapping[tuple[int, int], tuple[Sequence[EdgeRows], Sequence[OneWay]]],
                    shared: Iterable[tuple[tuple[tuple[int, int], int], tuple[tuple[int, int], int]]]
                    ) -> defaultdict[tuple[int, int], set[int]]:
    """ONE_WAY_PORTALS 'restore': which one-way candidates to keep. sectors: {sector: (per_area, one_way)} from
    trace_edges; shared: pairs ((sector, area), (sector, area)) joined across sectors (one surface in both - the
    sectors' two-sample overlap). Areas are joined through every portal piece that is not a candidate and through
    shared; then, pair by pair, a pair's candidates are kept only when its two areas are not joined yet (keeping them
    joins them). Returns {sector: set of kept candidate indices}."""
    parent: dict[tuple[tuple[int, int], int], tuple[tuple[int, int], int]] = {}

    def find(x: tuple[tuple[int, int], int]) -> tuple[tuple[int, int], int]:
        parent.setdefault(x, x)
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a: tuple[tuple[int, int], int], b: tuple[tuple[int, int], int]) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb
    for sec, (per_area, one_way) in sectors.items():
        cand = {(src, key, along) for src, _t, key, along in one_way}
        for ai, edges in enumerate(per_area):
            find((sec, ai))
            for key, rows in edges.items():
                if key[0] == 'B' and any((ai, key, r[0]) not in cand for r in rows):
                    union((sec, ai), (sec, key[1]))
    for a, b in shared:
        union(a, b)
    keep = defaultdict(set)
    by_pair = defaultdict(list)
    for sec, (_per_area, one_way) in sectors.items():
        for n, (src, t, _key, _along) in enumerate(one_way):
            by_pair[(sec, src, t)].append(n)
    for (sec, src, t), ns in sorted(by_pair.items()):
        if find((sec, src)) != find((sec, t)):
            keep[sec].update(ns)
            union((sec, src), (sec, t))
    return keep


def half_cells(level: Level, area: AreaInfo, gi0: int,
               gj0: int) -> tuple[dict[tuple[tuple[int, int], tuple[int, int]], float], dict[tuple[int, int], int]]:
    """The 45-degree bridges' side cells of an area (navdetail's half cells): {((i, j), (di, dj)): height} for each
    leg - the side of a side cell toward the bridged area cell - and {(i, j): surface}. The side cell's surface is the
    sampled one (kept or not) nearest the two bridged cells' mean height, within 1 m."""
    cell_k = {(c[0], c[1]): c[2] for c in area['cells']}
    half, ks = {}, {}
    s = level.s
    for (i, j), k in cell_k.items():
        for di, dj in ((1, 1), (1, -1)):
            kb = cell_k.get((i + di, j + dj))
            if kb is None or (i + di, j) in cell_k or (i, j + dj) in cell_k:
                continue
            y0 = (level.y[k] + level.y[kb]) / 2.0
            for (xi, xj), legs in (((i, j + dj), ((0, -dj), (di, 0))), ((i + di, j), ((-di, 0), (0, dj)))):
                gi, gj = gi0 + xi, gj0 + xj
                if not (0 <= gi < s.nx and 0 <= gj < s.nz):
                    continue
                c = gj * s.nx + gi
                col = range(s.start[c], s.start[c + 1])
                if not len(col):
                    continue
                kx = min(col, key=lambda q: abs(level.y[q] - y0))
                if abs(level.y[kx] - y0) > 1.0:
                    continue
                ks[(xi, xj)] = kx
                for leg in legs:
                    half[((xi, xj), leg)] = float(level.y[kx])
    return half, ks


def detail_pass(level: Any, areas: Sequence[AreaInfo], per_area: Sequence[EdgeRows], ox: float, oz: float,
                gi0: int | None = None, gj0: int | None = None, cfg: BuildSettings = DEFAULTS,
                only: Container[int] | None = None) -> list[tuple[list[Any], list[Any]]]:
    """DETAIL_FEATURES: per area (D polylines, F cells) from navdetail, and the broken sides taken out of the area's
    outline edges (per_area, from trace_edges; before trace_emit). A side holding a portal or border portal stays one.
    gi0, gj0: the sector's sample origin - with them, the bridges' half cells take part (DETAIL_HALF_CELLS).
    only: the areas that can have broken sides (cells side by side more than the step apart: stage's Rust filter),
    or None: all; the others get no features (as navdetail gives them: no broken side), per_area[ai] not read."""
    import navdetail
    kind = cfg.kind
    if kind == 'detailed' and not DETAIL_IN_DETAILED:
        return [([], [])] * len(areas)
    step = cfg.detail_step[kind]
    terrain = getattr(level, 'terrain', None)
    out: list[tuple[list[Any], list[Any]]] = []
    for ai, area in enumerate(areas):
        if only is not None and ai not in only:
            DETAIL_STATS['d'] = DETAIL_STATS.get('d', 0)
            DETAIL_STATS['f'] = DETAIL_STATS.get('f', 0)
            out.append(([], []))
            continue
        edges = per_area[ai]
        cells = {(i, j): float(level.y[k]) for i, j, k in area['cells']}
        ks = {(i, j): k for i, j, k in area['cells']}
        half = None
        if DETAIL_HALF_CELLS and gi0 is not None and hasattr(level, 's'):
            half, hks = half_cells(level, area, gi0, cast(int, gj0))     # gi0, gj0: both or neither
            for c, k in hks.items():
                ks.setdefault(c, k)
        breakable = None
        if DETAIL_TERRAIN_ONLY[kind] and terrain is not None:
            def breakable(a: tuple[int, int], b: tuple[int, int], ks: dict[tuple[int, int], int] = ks) -> bool:         # detailed zones: only between terrain cells (DICE: objects never)
                return a in ks and b in ks and bool(terrain[ks[a]]) and bool(terrain[ks[b]])
        linked = {(key[2], key[3], key[4], r[0]) for key, rows in edges.items() if key[0] in ('B', 'C') for r in rows}

        def eligible(i: int, j: int, di: int, dj: int, linked: set[tuple[Any, ...]] = linked) -> bool:     # neither of the two cells has a portal or border portal on the side
            return (('x', i + di * 0.5, di, j) if di else ('z', j + dj * 0.5, dj, i)) not in linked and \
                (('x', i + di * 0.5, -di, j) if di else ('z', j + dj * 0.5, -dj, i)) not in linked
        broken, lines, fcells = navdetail.detail_features(cells, cfg.pitch, ox, oz, step, eligible, half, breakable,
                                                          with_f=DETAIL_F[kind])
        if broken and cfg.detail_blocks_edges and hasattr(level, 'blocked'):
            fsets = navdetail.f_cell_sets(broken, cells) if DETAIL_F[kind] else (set(), set())
            for (i, j), (di, dj) in navdetail.d_sides(broken, *fsets):
                if (i, j) in ks and (i + di, j + dj) in ks:
                    level.blocked.add((ks[(i, j)], ks[(i + di, j + dj)]))
        broken = {b_ for b_ in broken if b_[0] in cells and (b_[0][0] + b_[1][0], b_[0][1] + b_[1][1]) in cells}
        if broken:
            # a broken side is outline on both of its cells or on neither (the slope rule linked it); taken out of the
            # outline only as a pair, so the area's boundary stays closed
            e_at = {(key[2], key[3], key[4], r[0]) for key, rows in edges.items() if key[0] == 'E' for r in rows}
            gone = defaultdict(set)
            for (i, j), (di, dj) in broken:
                axis, line, d, along = ('x', i + di * 0.5, di, j) if di else ('z', j + dj * 0.5, dj, i)
                if (axis, line, d, along) in e_at and (axis, line, -d, along) in e_at:
                    gone[('E', None, axis, line, d)].add(along)
                elif (axis, line, d, along) in e_at:
                    DETAIL_STATS['outline_one_sided'] = DETAIL_STATS.get('outline_one_sided', 0) + 1
            moved = 0
            for key, alongs in gone.items():
                before = len(edges[key])
                edges[key] = [r for r in edges[key] if r[0] not in alongs]
                moved += before - len(edges[key])
            DETAIL_STATS['outline_moved'] = DETAIL_STATS.get('outline_moved', 0) + moved
        DETAIL_STATS['d'] = DETAIL_STATS.get('d', 0) + len(lines)
        DETAIL_STATS['f'] = DETAIL_STATS.get('f', 0) + len(fcells)
        out.append((lines, fcells))
    return out


def _emitted(got: Any) -> list[Traced]:
    """The Rust core's trace_emit answer as the Python's: the mesh's portal objects."""
    return [([kymesh.Portal(p0, p1, t) for p0, p1, t in portals],
             [(kymesh.BorderPortal(p0, p1, d), first, last, line) for p0, p1, d, first, last, line in borders],
             outline) for portals, borders, outline in got]


def _trace_emit_many(level: Level, store: Any,
                     jobs: Sequence[tuple[int, Sequence[EdgeRows] | None, int, int, float, float]],
                     cfg: BuildSettings) -> list[list[Traced]] | None:
    """trace_emit for several sectors of a stage at once, in parallel in the Rust core (round E3h): jobs = [(the
    sector's index in store, per_area or None - the rows store keeps -, gi0, gj0, ox, oz)]; None: the caller runs
    trace_emit sector by sector."""
    if (store is None or _native is None or not BRIDGE_UNION or not isinstance(level, Level)
            or not all(isinstance(v, float) for v in (cfg.pitch, cfg.y_offset))
            or not all(isinstance(v, float) for j in jobs for v in j[4:6])):
        return None
    try:
        got = level.native().trace_emit_many(
            store, [(n, pa if pa is None or isinstance(pa, list) else list(pa), gi0, gj0, (ox, oz))
                    for n, pa, gi0, gj0, ox, oz in jobs], cfg.pitch, cfg.y_offset, LINK_RULE)
    except _NATIVE_ERRORS:
        return None
    return [_emitted(g) for g in got]


def trace_emit(level: Level, areas: Sequence[AreaInfo], per_area: Sequence[EdgeRows], gi0: int, gj0: int, ox: float,
               oz: float, cfg: BuildSettings = DEFAULTS) -> list[Traced]:
    """Per area (portals, borders, outline) from its boundary edges: runs merged, 45-degree bridges. In the Rust core
    when it is there (round E3c; the Python below is its reference and fallback - it also sorts per_area's rows and
    drops the bridged outline sides from them in place, which the Rust leaves as they are: nothing reads them after)."""
    if (_native is not None and BRIDGE_UNION and isinstance(level, Level)
            and all(isinstance(v, float) for v in (ox, oz, cfg.pitch, cfg.y_offset))):
        try:
            got = level.native().trace_emit([a['cells'] for a in areas], per_area if isinstance(per_area, list)
                                            else list(per_area), gi0, gj0, (ox, oz), cfg.pitch, cfg.y_offset,
                                            LINK_RULE)
        except _NATIVE_ERRORS:
            pass
        else:
            return _emitted(got)
    result = []
    for ai, area in enumerate(areas):
        edges = per_area[ai]
        # 45-degree bridges between diagonal-only neighbours of this area
        cellset = {(c[0], c[1]) for c in area['cells']}
        bridges = []
        cell_k = {(c[0], c[1]): c[2] for c in area['cells']}
        for i, j, k in area['cells']:
            for di, dj in ((1, 1), (1, -1)):
                if (i + di, j + dj) in cellset and (i + di, j) not in cellset and (i, j + dj) not in cellset:
                    # only across an open corner: neither cell has a connected surface (another area) beside it
                    # there - otherwise those sides are portals and the strip would overlap the other area
                    kd = cell_k[(i + di, j + dj)]
                    if any(level.neighbour(kk, gi0 + a, gj0 + b, x, z) is not None
                           for kk, a, b, x, z in ((k, i, j, di, 0), (k, i, j, 0, dj),
                                                  (kd, i + di, j + dj, -di, 0), (kd, i + di, j + dj, 0, -dj))):
                        continue
                    bridges.append((i, j, di, dj))
        portals, borders, outline = [], [], []
        if BRIDGE_UNION:
            drop, dsegs = bridge_union(bridges, cell_k, level, cfg)
            for (ax, az, ya), (bx, bz, yb) in dsegs:
                outline.append([(ox + ax * cfg.pitch, ya, oz + az * cfg.pitch),
                                (ox + bx * cfg.pitch, yb, oz + bz * cfg.pitch)])
        else:
            drop = set()
            for i, j, di, dj in bridges:
                # unit outline edges around the corner point (i + di/2, j + dj/2)
                drop.update({('x', i + di * 0.5, j), ('z', j + dj * 0.5, i), ('x', i + di * 0.5, j + dj), ('z', j + dj * 0.5, i + di)})
            bsegs = []                                      # (bridge, (ax, az, ya), (bx, bz, yb)) local units
            for bi, (i, j, di, dj) in enumerate(bridges):
                cx, cz = i + di * 0.5, j + dj * 0.5         # shared corner (local sample units)
                ya = level.y[cell_k[(i, j)]] + cfg.y_offset
                yb = level.y[cell_k[(i + di, j + dj)]] + cfg.y_offset
                for (ax, az), (bx, bz) in (((cx - di, cz), (cx, cz + dj)), ((cx, cz - dj), (cx + di, cz))):
                    bsegs.append((bi, (ax, az, ya), (bx, bz, yb)))
            for _bi, (ax, az, ya), (bx, bz, yb) in clip_bridges(bsegs, bridges):
                outline.append([(ox + ax * cfg.pitch, ya, oz + az * cfg.pitch),
                                (ox + bx * cfg.pitch, yb, oz + bz * cfg.pitch)])
        for key in list(edges):
            if key[0] == 'E':
                axis, line = key[2], key[3]
                edges[key] = [r for r in edges[key] if (axis, line, r[0]) not in drop]
        for (kind, target, axis, line, _d), runs in edges.items():
            runs.sort()
            start = 0
            for r in range(1, len(runs) + 1):
                if r == len(runs) or runs[r][0] != runs[r - 1][0] + 1:
                    a, b = runs[start][0] - 0.5, runs[r - 1][0] + 0.5
                    ya, yb = runs[start][1], runs[r - 1][1]
                    if axis == 'x':
                        p0 = (ox + line * cfg.pitch, ya, oz + a * cfg.pitch)
                        p1 = (ox + line * cfg.pitch, yb, oz + b * cfg.pitch)
                    else:
                        p0 = (ox + a * cfg.pitch, ya, oz + line * cfg.pitch)
                        p1 = (ox + b * cfg.pitch, yb, oz + line * cfg.pitch)
                    if kind == 'B':
                        portals.append(kymesh.Portal(p0, p1, target))
                    elif kind == 'C':
                        borders.append((kymesh.BorderPortal(p0, p1, (float(target[0]), 0.0, float(target[1]))),
                                        runs[start][0], runs[r - 1][0], line))
                    else:
                        outline.append([p0, p1])
                    start = r
        result.append((portals, borders, outline))
    return result


def build(samples_path: str, tris_path: str | None, xoff: int, zoff: int, out_path: str, step: float = 0.28,
          log: Log = print, dynamic_path: str | None = None, high_path: str | None = None,
          narrow_path: str | None = None, stairs_path: str | None = None, exclude_path: str | None = None,
          exclude_foot_path: str | None = None, seeds: Points | None = None, edits: list[dict[str, Any]] | None = None,
          water: tuple[np.ndarray, np.ndarray] | None = None, max_water_depth: float | None = MAX_WATER_DEPTH,
          bounds: Sequence[float] | None = None, order_points: Points | None = None,
          detailed: dict[str, Any] | None = None, ladders: Sequence[dict[str, Any]] | None = None,
          area_orders: Points | None = None, cfg: BuildSettings = DEFAULTS) -> None:
    """edits: the operations of a navigation side file (navedits.py applies the walkability ones).
    water: the level's water mesh (vertices, triangles; core/bfbc2core/water.py): ground more than max_water_depth
    under it is not walkable, as in DICE's data (hand edits forcing ground walkable still win).
    bounds: (minX, minZ, maxX, maxZ) of the reference path data (CLIP_TO_REFERENCE), or None.
    detailed: {'samples': a pitch-0.2 sampling run, 'high': its high-capsule run (or None), 'sectors': {(X, Y)}} -
    the second pass: those sectors are built at pitch 0.2 (DICE's detailed zones) and merged with the normal build
    (merge_detailed). Without it the build is the normal one, unchanged.
    cfg: the build's settings (BuildSettings; the normal infantry build by default - the detailed pass uses
    cfg.detailed())."""
    require_native()                        # the grid walks need the Rust core: say so before reading anything
    t0 = time.time()
    common: dict[str, Any] = dict(tris_path=tris_path, step=step, log=log, dynamic_path=dynamic_path, narrow_path=narrow_path,
                  stairs_path=stairs_path, exclude_path=exclude_path, exclude_foot_path=exclude_foot_path,
                  edits=edits, water=water, max_water_depth=max_water_depth, bounds=bounds,
                  order_points=order_points, t0=t0, ladders=ladders, area_orders=area_orders)
    reset_stats()                           # reports of this build only (navregress / the GUI build several)
    a = stage(samples_path, high_path, xoff, zoff, seeds=seeds, **common, cfg=cfg)
    graphs = merge_detailed(a, detailed, xoff, zoff, seeds, common, log, cfg) if detailed else a['graphs']
    if ladders:
        log(f'ladders: {len(ladders)}; links made {LADDER_STATS.get("linked", 0)} (counted per stage), ends in two '
            f'sectors {LADDER_STATS.get("split", 0)}')
    if cfg.detail_features:
        log(f'detail features: {DETAIL_STATS.get("d", 0)} D lines, {DETAIL_STATS.get("f", 0)} F cells; '
            f'{DETAIL_STATS.get("outline_moved", 0)} outline sides moved to them, '
            f'{DETAIL_STATS.get("outline_one_sided", 0)} kept (outline on one cell only)')
    else:
        log('detail features: off (no D lines or F cells)')
    # route islands that shadow a larger one lose their nodes (navislands.py: SHADOW_ISLANDS)
    graphs = navislands.prune(graphs, list(seeds or []) + list(order_points or []) + list(area_orders or []), log)
    write_file(graphs, out_path, log, t0, cfg)


def stage(samples_path: str | None, high_path: str | None, xoff: int, zoff: int, tris_path: str | None = None,
          step: float = 0.28, log: Log = print, dynamic_path: str | None = None,
          narrow_path: str | None = None, stairs_path: str | None = None, exclude_path: str | None = None,
          exclude_foot_path: str | None = None, seeds: Points | None = None, edits: list[dict[str, Any]] | None = None,
          water: tuple[np.ndarray, np.ndarray] | None = None, max_water_depth: float | None = MAX_WATER_DEPTH,
          bounds: Sequence[float] | None = None, order_points: Points | None = None, area_orders: Points | None = None,
          t0: float | None = None, sector_filter: Iterable[tuple[int, int]] | None = None,
          grid_min: tuple[int, int] | None = None, id_start: int = 1,
          carried: dict[tuple[int, int], list[tuple[int, Vec3]]] | None = None, seed_points: Points | None = None,
          ladders: Sequence[dict[str, Any]] | None = None, cfg: BuildSettings = DEFAULTS,
          grid: tuple[Any, ...] | None = None) -> dict[str, Any]:
    """One grid's build (cfg: its BuildSettings - pitch, tiles ...): walkable surfaces, sectors, areas, border
    nodes, traced boundaries, meshes and route graphs - not yet written. sector_filter: build only these (X, Y) sectors;
    grid_min: the level's minimum sector X / Y (EDGE_SECTOR_RULE) when the stage holds only part of it; id_start:
    the first node id; carried: {sector: [(id, pos)]} shared nodes from another stage on this stage's sectors (the
    seams with sectors built at another pitch) - attached to this grid's ground where it has some; seed_points:
    more walkable-seed points (x, ground y, z) - not soldier spawns (those are seeds). grid: a ready walkability grid
    (surfaces at cfg.pitch, keep, gx, gz, stair) used as it is - samples_path, high_path and the walkability inputs
    (tris_path ... exclude_foot_path, edits, water, bounds, seeds) are not read (navvehicle.stage_on_grid).
    Returns {'level', 'info', 'graphs', 'border_nodes', 'next_id', 'dropped', 'pitch', 'meshes'}."""
    require_native()                        # the grid walks need the Rust core: say so before reading anything
    t0 = t0 or time.time()
    walk_grid: list[Any] = []                # walkable's NavLevel (its floods'), for the Level's
    if grid is not None:
        s, keep, gx, gz, stair = grid
    else:
        f, _ = navsample.load(cast(str, samples_path))     # read when there is no grid
        s = navwalk.coarse(f) if abs(f.pitch - cfg.pitch) > 1e-3 else f
        hand: dict[str, Any] = {}
        soldiers = list(seeds or [])                 # the level's soldier spawn points (navwalk.CHARACTERS_ABSENT)
        if edits:
            import navedits
            e = navedits.edits_for(s, edits)
            hand = {'force_walkable': e['force_walkable'] if e['surfaces_walkable'] else None,
                    'force_unwalkable': e['force_unwalkable'] if e['surfaces_unwalkable'] else None,
                    'unseeds': e['unseeds']}
            seeds = list(seeds or []) + e['seeds']
            log(f'hand edits: {e["counts"]}; surfaces forced walkable {e["surfaces_walkable"]}, unwalkable {e["surfaces_unwalkable"]}')
        if water is not None and max_water_depth is not None:
            deep = deep_water(s, water, max_water_depth)
            if deep.any():
                fw, fu = hand.get('force_walkable'), hand.get('force_unwalkable')
                if fw is not None:
                    deep &= ~fw                      # a hand edit forcing ground walkable wins over the water rule
                hand['force_unwalkable'] = deep if fu is None else (fu | deep)
            log(f'water: {int(deep.sum())} surfaces more than {max_water_depth} m under water left out')
        if bounds is not None and cfg.clip_to_reference:
            outside = outside_bounds(s, bounds)
            fw, fu = hand.get('force_walkable'), hand.get('force_unwalkable')
            if fw is not None:
                outside &= ~fw                       # a hand edit forcing ground walkable wins here too
            hand['force_unwalkable'] = outside if fu is None else (fu | outside)
            log(f'reference extent x {bounds[0]:.1f}..{bounds[2]:.1f}, z {bounds[1]:.1f}..{bounds[3]:.1f}: '
                f'{int(outside.sum())} surfaces outside left out')
        tris = np.load(tris_path) if tris_path else None
        dyn = np.load(dynamic_path) if dynamic_path else None
        hi = navsample.load(high_path)[0] if high_path else None
        narrow = navsample.load(narrow_path)[0] if narrow_path else None
        st = np.load(stairs_path) if stairs_path else None
        ex = np.load(exclude_path) if exclude_path else None
        exf = np.load(exclude_foot_path) if exclude_foot_path else None
        keep, gx, gz, stair = navwalk.walkable(s, tris, step=step, dynamic_tris=dyn, high_capsule=hi, return_gradients=True,
                                        return_stairs=True,
                                        narrow_capsule=narrow, stair_tris=st, exclude_tris=ex, exclude_footprint_tris=exf,
                                        extra_seeds=list(seeds or []) + list(seed_points or []), soldier_points=soldiers,
                                        ladders=ladders, **hand,
                                        # roof pruning anchors: soldier spawns, AI order positions, and (stage b) the ground
                                        # the normal build kept - stage b drops only roofs that build did not have
                                        anchors=list(seeds or []) + list(order_points or []) + list(seed_points or []),
                                        objective_points=area_orders, grid_out=walk_grid)
    level = Level(s, keep, step, gx, gz, stair, cfg)
    if walk_grid:
        # the level in the Rust core from walkable's (the same samples, gradients and stairs; round E3h); on a
        # refusal Level.native makes it from the arrays
        with contextlib.suppress(*_NATIVE_ERRORS):
            level._native = walk_grid[0].with_walk(navwalk._flags(keep), navwalk._flags(level.terrain),
                                                   float(cfg.pitch), float(cfg.link_step))
    log(f'{len(s.y)} surfaces, {int(keep.sum())} walkable kept ({time.time() - t0:.0f} s)'
        + (f'; roof islands dropped {navwalk.ROOF_STATS.get("dropped", 0)} ({navwalk.ROOF_STATS.get("surfaces", 0)} surfaces)'
           if navwalk.ROOF_PRUNE and grid is None else '')
        + (f'; objective floors {navwalk.OBJECTIVE_STATS.get("points", 0)} ({navwalk.OBJECTIVE_STATS.get("surfaces", 0)} surfaces)'
           if navwalk.OBJECTIVE_FLOOR and grid is None else ''))
    navwalk.ROOF_STATS.clear()
    navwalk.OBJECTIVE_STATS.clear()
    # ladder links: the kept surfaces at each ladder's foot and top (navwalk.ladder_ends)
    ladder_pairs = []
    for lad in (ladders or ()):
        f_, t_ = navwalk.ladder_ends(s, keep, keep, lad)
        if f_ is not None and t_ is not None:
            ladder_pairs.append((int(f_), int(t_)))

    # Sectors that contain kept surfaces.
    secx, secz = column_sectors(s, sorted_unique(level.col_of[np.nonzero(keep)[0]]), xoff, zoff, cfg.pitch)
    found = set(zip(secx.tolist(), secz.tolist()))
    if sector_filter is not None:
        found &= set(sector_filter)
    sectors = sorted(found)
    if not sectors and sector_filter is None:        # (a detailed pass may find none: merge_detailed handles it)
        raise RuntimeError('no walkable ground was kept (the samples hold none inside the box): check the box and '
                           'the samples')
    bad = [(X, Y) for X, Y in sectors if not (0 <= X <= 255 and 0 <= Y <= 255)]
    if bad:
        # a sector id is 0xXXYY: a grid position outside 0..255 would wrap into another sector's id
        raise RuntimeError(f'{len(bad)} sector(s) outside the 256 x 256 grid (e.g. {bad[0]}, offsets {xoff}, {zoff}): '
                           f'the box reaches below the sector offsets - run prepare again after widening the box')
    log(f'{len(sectors)} sectors (pitch {cfg.pitch:g})')

    # Pass 1: areas per sector, and which area each surface belongs to.
    info: dict[tuple[int, int], dict[str, Any]] = {}
    have = set(sectors)
    min_x_sector, min_z_sector = grid_min or (min(X for X, _ in sectors), min(Y for _, Y in sectors))
    specs = []
    for X, Y in sectors:
        gi0, gj0, ox, oz = sector_origin_index(level, X, Y, xoff, zoff, cfg)
        if EDGE_SECTOR_RULE == 'grid_min':
            edge = (min_x_sector == X, min_z_sector == Y)
        else:
            edge = ((X - 1, Y) not in have, (X, Y - 1) not in have)
        start = tuple(0 if EDGE_SECTOR_TILES and e else -1 for e in edge)
        specs.append((gi0, gj0, ox, oz, start, *tiling(((X & 0xFF) << 8) | (Y & 0xFF), cfg)))
    # every sector at once (Rust, in parallel), kept in the core for the tracing and graphs (store), or None: one by one
    many, store = _build_areas_kept(level, specs, cfg)
    orders_of = []
    for n, (X, Y) in enumerate(sectors):
        gi0, gj0, ox, oz, start, tile_n, tiles_n = specs[n]
        if many is not None:
            areas, ylims = many[n]
        else:
            areas = build_areas(level, gi0, gj0, start, tile_n, tiles_n, cfg=cfg)
            ylims = [(min(level.y[c[2]] for c in a['cells']), max(level.y[c[2]] for c in a['cells'])) for a in areas]
        # order: tiles column-major, then layer = index within tile (the lowest ground's height; a stable sort)
        order = sorted(range(len(areas)), key=lambda q: (areas[q]['tile'][0], areas[q]['tile'][1], ylims[q][0]))
        orders_of.append(order)
        areas, ylims = [areas[q] for q in order], [ylims[q] for q in order]
        per_tile: defaultdict[tuple[int, int], int] = defaultdict(int)
        for a in areas:
            a['layer'] = per_tile[a['tile']]
            per_tile[a['tile']] += 1
        info[(X, Y)] = {'areas': areas, 'area_of': None, 'origin': (gi0, gj0, ox, oz), 'start': start,
                        'ylims': ylims}
    if store is not None:
        try:
            store.reorder(orders_of)                 # the core's copy in the same order
        except _NATIVE_ERRORS:
            store = None
    for n, inf in enumerate(info.values()):
        # each surface -> its area: answered by the core when it holds the areas (round E3h: the dict of every cell
        # cost half a second on sp_004_b; it is made only if something iterates it), else a dict
        if store is not None:
            inf['area_of'] = _AreaOf(store, n, inf['areas'])
            for ai, a in enumerate(inf['areas']):
                a['_at'] = (store, n, ai)            # border_cell's lookups
        else:
            inf['area_of'] = {c[2]: ai for ai, a in enumerate(inf['areas']) for c in a['cells']}
    at_of = {sec: n for n, sec in enumerate(sectors)}    # a sector's index in store (info is in the same order)
    log(f'{sum(len(v["areas"]) for v in info.values())} areas ({time.time() - t0:.0f} s)')

    # Graph: border nodes shared between sector pairs (global ids), then inner nodes.
    next_id = [id_start]
    border_nodes: defaultdict[tuple[int, int], list[tuple[int, tuple[float, float, float], int]]]
    border_at: dict[int, tuple[int, tuple[float, float, float]]]
    border_nodes = defaultdict(list)     # sector -> [(id, pos, k)]
    border_at = {}                       # surface -> (id, pos) of the border node on it

    def new_id() -> int:
        next_id[0] += 1
        return next_id[0]

    for (X, Y), inf in info.items():
        gi0, gj0, ox, oz = inf['origin']
        for (dX, dY), line in (((1, 0), cfg.sector_samples + 1), ((0, 1), cfg.sector_samples + 1)):
            other = (X + dX, Y + dY)
            if other not in info:
                continue
            # every kept surface on the line (all layers: floors under roofs too), grouped into connected runs
            # along it, one run per layer
            active: list[list[tuple[int, int]]]
            runs, active = [], []
            for t in cfg.window:
                i, j = (line, t) if dX else (t, line)
                nxt = []
                for k in level.surfaces(gi0 + i, gj0 + j):
                    for r in active:
                        if r[-1][0] == t - 1 and level.connected(r[-1][1], k, 0 if dX else 1, 1 if dX else 0):
                            r.append((t, k))
                            nxt.append(r)
                            active.remove(r)
                            break
                    else:
                        r = [(t, k)]
                        runs.append(r)
                        nxt.append(r)
                active = nxt
            for run in runs:
                n = max(1, int(math.ceil(len(run) * cfg.pitch / cfg.border_node_spacing)))
                for q in range(n):
                    t, k = run[int((q + 0.5) * len(run) / n)]
                    i, j = (line, t) if dX else (t, line)
                    pos = (ox + i * cfg.pitch, float(level.y[k]) + cfg.y_offset, oz + j * cfg.pitch)
                    if DEDUP_GRAPH_NODES and k in border_at:
                        # a surface already holding a border node (a sector corner: on both lines): the same
                        # node, shared with this neighbour too - DICE has border ids in 3 and 4 sectors; two ids
                        # on one spot were joined by zero-length edges (11 of 13 campaign levels, 2026-10-20)
                        nid, pos = border_at[k]
                        for sec in ((X, Y), other):
                            if not any(b[0] == nid for b in border_nodes[sec]):
                                border_nodes[sec].append((nid, pos, k))
                        continue
                    nid = 0x80000000 | new_id()
                    border_at[k] = (nid, pos)
                    border_nodes[(X, Y)].append((nid, pos, k))
                    border_nodes[other].append((nid, pos, k))
    # shared nodes from another stage (seams with sectors at another pitch): on this grid's ground at the node, the
    # nearest kept surface of the sector within 0.6 m of its height; nodes with no such ground are reported dropped
    dropped: set[int] = set()
    for sec, nodes in (carried or {}).items():
        if sec not in info:
            dropped.update(nid for nid, _pos in nodes)
            continue
        gi0, gj0, ox, oz = info[sec]['origin']
        own = info[sec]['area_of']
        for nid, pos in nodes:
            # within CARRY_RADIUS of the node, on its floor; ground of a larger area first (not a stray speck), then
            # the nearest - a 3 x 3 search (0.2 m at pitch 0.2) left 74 ONS_MP_002 seam nodes without ground and put
            # others on specks, and the detailed core lost its links to the normal sectors (connectivity 91.6 -> 60.9)
            gi, gj = int(round((pos[0] - s.min_x) / cfg.pitch)), int(round((pos[2] - s.min_z) / cfg.pitch))
            rad = int(math.ceil(CARRY_RADIUS / cfg.pitch))
            cand = []
            for di in range(-rad, rad + 1):
                for dj in range(-rad, rad + 1):
                    dxz = math.hypot(di, dj) * cfg.pitch
                    if dxz > CARRY_RADIUS:
                        continue
                    for k in level.surfaces(gi + di, gj + dj):
                        if k in own and abs(level.y[k] + cfg.y_offset - pos[1]) <= 0.6:
                            size = len(info[sec]['areas'][own[k]]['cells'])
                            cand.append((size < 25, dxz, k))
            if not cand:
                dropped.add(nid)
                continue
            k = min(cand)[2]
            border_nodes[sec].append((nid, pos, k))

    # boundary edges of every sector, then the one-way portal pieces decided level-wide. The edge rows stay in the
    # Rust core when it holds the areas (round E3h: edge_sets None; drop_one_way runs there, the detail pass fetches
    # the areas it may change), else they are Python dicts (edge_sets)
    edge_sets: dict[tuple[int, int], tuple[list[EdgeRows], list[OneWay]]] | None = None
    one_ways = _trace_edges_kept(level, store, sorted(info.items()), cfg)
    if one_ways is not None:
        n_cand = sum(len(o) for o in one_ways)
        keep = {sec: set(range(len(o))) if cfg.one_way_portals == 'keep' else set()
                for sec, o in zip(sorted(info), one_ways)}
        try:
            store.drop_one_way([sorted(keep[sec]) for sec in sorted(info)])
        except _NATIVE_ERRORS:                   # refused before changing anything: the rows to Python
            edge_sets = _edges_from_store(store, sorted(info.items()), one_ways)
            for sec, (per_area, one_way) in edge_sets.items():
                drop_one_way(per_area, one_way, keep.get(sec, set()))
    else:
        edge_sets = _trace_edges_many(level, sorted(info.items()), cfg, store)   # all sectors at once (Rust), or one by one
        if edge_sets is None:
            edge_sets = {}
            for (X, Y), inf in sorted(info.items()):
                gi0, gj0, ox, oz = inf['origin']
                edge_sets[(X, Y)] = trace_edges(level, inf['areas'], inf['area_of'], gi0, gj0, cfg)
        n_cand = sum(len(v[1]) for v in edge_sets.values())
        if cfg.one_way_portals == 'restore':
            owners = defaultdict(list)
            for sec, inf in info.items():
                for k, ai in inf['area_of'].items():
                    owners[k].append((sec, ai))
            shared = [(o[0], x) for o in owners.values() if len(o) > 1 for x in o[1:]]
            keep = restore_one_way(edge_sets, shared)
        else:
            keep = {sec: set(range(len(v[1]))) if cfg.one_way_portals == 'keep' else set()
                    for sec, v in edge_sets.items()}
        for sec, (per_area, one_way) in edge_sets.items():
            drop_one_way(per_area, one_way, keep.get(sec, set()))
    log(f'one-way portal pieces: {n_cand}, kept one-way {sum(len(v) for v in keep.values())} ({cfg.one_way_portals})')
    # the detail features of every sector (their D sides: each sector's own blocked set), then the outlines of all
    # sectors at once (Rust, in parallel) or one by one. With the rows in the core only the areas with cells side by
    # side more than the detail step apart can have broken sides: the others get no features without being looked at
    details_of: dict[tuple[int, int], list[tuple[list[Any], list[Any]]]] = {}
    blocked_of: dict[tuple[int, int], set[tuple[int, int]]] = {}
    cands = _detail_candidates(level, store, cfg) if edge_sets is None else None
    for n_sec, ((X, Y), inf) in enumerate(sorted(info.items())):
        gi0, gj0, ox, oz = inf['origin']
        level.blocked = set()            # D sides of this sector only (another sector may hold the cells in two areas)
        if not cfg.detail_features:
            details_of[(X, Y)] = [([], [])] * len(inf['areas'])
        elif edge_sets is not None:
            details_of[(X, Y)] = detail_pass(level, inf['areas'], edge_sets[(X, Y)][0], ox, oz, gi0, gj0, cfg)
        else:
            only = cands[n_sec] if cands is not None else list(range(len(inf['areas'])))
            per = dict(zip(only, store.edges_of(at_of[(X, Y)], only))) if only else {}
            details_of[(X, Y)] = detail_pass(level, inf['areas'], cast(Any, per), ox, oz, gi0, gj0, cfg, only=set(only))
            if only:
                store.set_edges(at_of[(X, Y)], only, [per[a] for a in only])
        blocked_of[(X, Y)] = level.blocked
    emitted = _trace_emit_many(level, store, [(at_of[sec], None if edge_sets is None else edge_sets[sec][0],
                                               *inf['origin']) for sec, inf in sorted(info.items())], cfg)
    if emitted is None and edge_sets is None:            # the core refused: its rows to Python, trace_emit there
        edge_sets = _edges_from_store(store, sorted(info.items()), None)
    graphs, meshes, jobs = [], {}, []
    for n_sec, ((X, Y), inf) in enumerate(sorted(info.items())):
        gi0, gj0, ox, oz = inf['origin']
        areas, area_of = inf['areas'], inf['area_of']
        level.blocked = blocked_of[(X, Y)]
        details = details_of[(X, Y)]
        traced = (emitted[n_sec] if emitted is not None
                  else trace_emit(level, areas, cast(Any, edge_sets)[(X, Y)][0], gi0, gj0, ox, oz, cfg))
        sid = ((X & 0xFF) << 8) | (Y & 0xFF)
        mesh_areas = []
        ntiles = tiling(sid, cfg)[1]
        tiles: list[list[kymesh.TileItem]] = [[] for _ in range(ntiles * ntiles)]
        for ai, (a, (portals, borders, outline), (d_lines, f_cells)) in enumerate(zip(areas, traced, details)):
            low, high = inf['ylims'][ai]             # min / max of level.y over the area's cells (pass 1)
            # DICE's area height range: from its lowest ground to its highest ground + 1.52 m (all normal-pitch
            # areas of sp_004_b: median +0.000 / +1.520; NavKit had -0.95 / +1.55, a metre of extra room below
            # each area, so a lower floor's range reached the deck above it - 2026-10-19)
            lo, hi = low + cfg.area_y_min, high + cfg.area_y_max
            mesh_areas.append(kymesh.Area(sid, a['tile'][0], a['tile'][1], a['layer'], cfg.radius, cfg.pitch, hi, lo,
                                          portals, [b[0] for b in borders], d_lines, outline, f_cells))
            a['borders'] = borders
            tiles[a['tile'][0] * ntiles + a['tile'][1]].append(kymesh.TileItem(ai, lo, hi))
        mesh_links = sector_links(X, Y, areas, mesh_areas, same_grid_resolver(level, info, X, Y))
        # the runtime's tile grid starts at the rectangle's minimum (DICE: tile lines 39.6 m past it on 3,748 sector
        # axes) - so an edge sector, whose tiles start at its own edge, starts its rectangle there too (DICE 0x101:
        # -800.0 instead of -800.8). The maximum side keeps the 1.1-pitch margin (DICE drops it at the grid's maximum).
        sx, sz = inf['start']
        rect = (ox + sx * cfg.pitch, ox + 100 + 1.1 * cfg.pitch, oz + sz * cfg.pitch, oz + 100 + 1.1 * cfg.pitch)
        mesh = kymesh.Mesh(2, 0, 0, sid, cfg.resolution, cfg.pitch, rect, ntiles, ntiles, cfg.radius, tiles, mesh_areas,
                           mesh_links, b'\0' * 4)
        meshes[(X, Y)] = mesh
        orders = [p for p in (order_points or ()) if ox <= p[0] < ox + 100 and oz <= p[2] < oz + 100]
        lads = [(f_, t_) for f_, t_ in ladder_pairs if f_ in area_of and t_ in area_of]
        LADDER_STATS['linked'] = LADDER_STATS.get('linked', 0) + len(lads)
        LADDER_STATS['split'] = LADDER_STATS.get('split', 0) + sum(1 for f_, t_ in ladder_pairs if (f_ in area_of) != (t_ in area_of))
        jobs.append(((X, Y), traced, orders, lads, mesh, mesh_areas, rect, sid))
    # the route graphs: every sector at once (Rust, in parallel; round E3h), or sector by sector (build_graph); the
    # node ids are given in the sectors' order either way
    gcfg = _graph_cfg(level, cfg) if store is not None else None
    raw = [_graph_job(at_of[sec], traced, border_nodes[sec], info[sec]['origin'][0], info[sec]['origin'][1],
                      info[sec]['origin'][2], info[sec]['origin'][3], orders, lads, blocked_of[sec])
           for sec, traced, orders, lads, *_rest in jobs] if gcfg else []
    got = None
    if gcfg and raw and all(j is not None for j in raw):
        try:
            got = level.native().graph_many(store, raw, gcfg)
        except _NATIVE_ERRORS:
            got = None
    for n_sec, ((X, Y), traced, orders, lads, mesh, mesh_areas, rect, sid) in enumerate(jobs):
        gi0, gj0, ox, oz = info[(X, Y)]['origin']
        level.blocked = blocked_of[(X, Y)]
        if got is not None:
            pts, pairs = _graph_finish(got[n_sec], border_nodes[(X, Y)], new_id, orders, cfg)
        else:
            pts, pairs = build_graph(level, info[(X, Y)]['areas'], info[(X, Y)]['area_of'], traced,
                                     border_nodes[(X, Y)], gi0, gj0, ox, oz, new_id, orders, lads, cfg)
        navislands.note_nodes(level, pts)        # which nodes stand on terrain (SHADOW_OBJECTS_ONLY)
        ys = [a.min_y for a in mesh_areas] + [a.max_y for a in mesh_areas]
        bounds_ = (rect[0], rect[1], min(ys), max(ys), rect[2], rect[3])
        graphs.append((sid, bounds_, mesh, pts, pairs, (ox, oz)))
    return {'level': level, 'info': info, 'graphs': graphs, 'border_nodes': border_nodes, 'next_id': next_id[0],
            'dropped': dropped, 'pitch': cfg.pitch, 'meshes': meshes, 'sectors': set(info)}


def sector_links(X: int, Y: int, areas: Sequence[AreaInfo], mesh_areas: Sequence[kymesh.Area],
                 resolve: Callable[..., tuple[int, int, int] | None]) -> list[kymesh.NeighbourLink]:
    """The neighbour links of a sector: for every border portal (C), the area across it - resolve(other, area,
    border) gives its (tile x, tile z, layer) in the other sector, or None."""
    links: defaultdict[tuple[int, int], defaultdict[tuple[int, int, int], list[tuple[int, int]]]]
    links = defaultdict(lambda: defaultdict(list))
    for ai, a in enumerate(areas):
        for ci, b in enumerate(a['borders']):
            bp = b[0]
            di, dj = int(bp.direction[0]), int(bp.direction[2])
            other = (X + di, Y + dj)
            hit = resolve(other, a, b)
            if hit is None:
                continue
            rel = 0x50 + len(mesh_areas[ai].portals) * 0x20 + ci * 0x2C
            links[other][hit].append((ai, rel))
    mesh_links = []
    for other in sorted(links):
        oid = ((other[0] & 0xFF) << 8) | (other[1] & 0xFF)
        mesh_links.append(kymesh.NeighbourLink(oid, [kymesh.LinkObject(h, refs) for h, refs in sorted(links[other].items())]))
    return mesh_links


def border_cell(a: AreaInfo, b: tuple[Any, ...]) -> tuple[int, int, int | None, int, int]:
    """(i, j, k) of the area's cell just inside a border portal (the middle of its run), and the step (di, dj)."""
    bp, t0_, t1_, line = b
    di, dj = int(bp.direction[0]), int(bp.direction[2])
    tmid = (t0_ + t1_) // 2
    i, j = (int(line + 0.5 * di), tmid) if di else (tmid, int(line + 0.5 * dj))
    at = a.get('_at')
    if at is not None:                               # the area in the Rust core (stage, round E3h)
        k = at[0].first_cell(at[1], at[2], i - di, j - dj)
    else:
        k = next((c[2] for c in a['cells'] if (c[0], c[1]) == (i - di, j - dj)), None)
    return i - di, j - dj, k, di, dj


def same_grid_resolver(level: Level, info: dict[tuple[int, int], dict[str, Any]], X: int, Y: int) -> Resolver:
    """resolve for sector_links within one stage: the connected surface across, in the other sector's areas."""
    gi0, gj0, _ox, _oz = info[(X, Y)]['origin']

    def resolve(other: tuple[int, int], a: AreaInfo, b: tuple[Any, ...]) -> tuple[int, int, int] | None:
        if other not in info:
            return None
        i, j, k_here, di, dj = border_cell(a, b)
        if k_here is None:
            return None
        n = level.neighbour(k_here, gi0 + i, gj0 + j, di, dj)
        oth = info[other]
        if n is None or n not in oth['area_of']:
            return None
        ta = oth['areas'][oth['area_of'][n]]
        return (ta['tile'][0], ta['tile'][1], ta['layer'])
    return resolve


def cross_grid_resolver(here: dict[str, Any], there: dict[str, Any], X: int, Y: int) -> Resolver:
    """resolve for sector_links across two stages (a detailed sector next to a normal one): the cell just across
    the border, in world coordinates, looked up on the other stage's grid - its kept surface nearest in height
    (within 0.8 m) that belongs to the other sector's areas."""
    lv, inf = here['level'], here['info'][(X, Y)]
    _gi0, _gj0, ox, oz = inf['origin']
    p_here, olv, p_there = here['pitch'], there['level'], there['pitch']
    os_ = olv.s

    def resolve(other: tuple[int, int], a: AreaInfo, b: tuple[Any, ...]) -> tuple[int, int, int] | None:
        if other not in there['info']:
            return None
        i, j, k_here, di, dj = border_cell(a, b)
        if k_here is None:
            return None
        x = ox + i * p_here + di * max(p_here, p_there)
        z = oz + j * p_here + dj * max(p_here, p_there)
        y = lv.y[k_here]
        oth = there['info'][other]
        gi, gj = int(round((x - os_.min_x) / p_there)), int(round((z - os_.min_z) / p_there))
        cand = [k for k in olv.surfaces(gi, gj) if k in oth['area_of'] and abs(olv.y[k] - y) <= 0.8]
        if not cand:
            return None
        n = min(cand, key=lambda k: abs(olv.y[k] - y))
        ta = oth['areas'][oth['area_of'][n]]
        return (ta['tile'][0], ta['tile'][1], ta['layer'])
    return resolve


def merge_detailed(a: dict[str, Any], detailed: dict[str, Any], xoff: int, zoff: int, seeds: Points | None,
                   common: dict[str, Any], log: Log, cfg: BuildSettings = DEFAULTS) -> list[Graph]:
    """The second pass: DICE's detailed-zone sectors built at pitch 0.2 from their own sampling run (stage b),
    merged with the normal build a. Stage b is seeded from a's walkable ground in those sectors (ground joined to the
    rest of the level only through normal sectors would otherwise be lost) and continues a's node ids. Shared nodes
    on the seams between a detailed and a normal sector come from a and are attached to b's ground (dropped from
    both sides where b has none there); neighbour links across those seams are found by position on the other grid.
    cfg: the normal build's settings - stage b runs with cfg.detailed().
    Returns the merged graphs (normal sectors from a, detailed from b)."""
    dset = set(detailed['sectors']) & a['sectors']
    alv = a['level']
    sa = alv.s
    # seeds: a's kept ground in the detailed sectors
    kept = np.nonzero(alv.keep)[0]
    secx, secz = column_sectors(sa, alv.col_of[kept], xoff, zoff, sa.pitch)
    sel = np.zeros(len(kept), bool)
    for X, Y in dset:
        sel |= (secx == X) & (secz == Y)
    ks = kept[sel]                                   # in surface order, as a loop over kept takes them
    c = alv.col_of[ks]
    pts = list(zip((sa.min_x + (c % sa.nx) * sa.pitch).tolist(), alv.y[ks].astype(float).tolist(),
                   (sa.min_z + (c // sa.nx) * sa.pitch).tolist()))
    # a's shared nodes on the seams between a detailed and a normal sector, per detailed sector
    owners = defaultdict(set)
    for sec, nodes in a['border_nodes'].items():
        for nid, _pos, _k in nodes:
            owners[nid].add(sec)
    carried = defaultdict(list)
    for sec in dset:
        for nid, pos, _k in a['border_nodes'].get(sec, ()):
            if owners[nid] - dset:
                carried[sec].append((nid, pos))
    log(f'detailed pass: {len(dset)} sectors at pitch {DETAILED_PITCH}, {len(pts)} seed points from the normal build, '
        f'{sum(len(v) for v in carried.values())} shared nodes on mixed seams')
    grid_min = (min(X for X, _ in a['sectors']), min(Y for _, Y in a['sectors']))
    b = stage(detailed['samples'], detailed.get('high'), xoff, zoff, seeds=seeds, **common,
              sector_filter=dset, grid_min=grid_min, id_start=a['next_id'], carried=carried, seed_points=pts,
              cfg=cfg.detailed())
    missing = dset - b['sectors']
    if missing:
        log(f'detailed pass: {len(missing)} sectors without ground at pitch {DETAILED_PITCH} kept at the normal pitch: '
            + ', '.join(f'{(X & 0xFF) << 8 | (Y & 0xFF):04x}' for X, Y in sorted(missing)))
    dset = b['sectors']
    # only nodes on the seams of detailed sectors stage b built: a detailed sector it had no ground for stays at
    # the normal pitch, and its seam nodes - in a's build on both sides - keep their edges
    drop = {nid for nid in b['dropped'] if owners[nid] & dset}
    log(f'detailed pass: {len(drop)} mixed-seam shared nodes without ground at pitch {DETAILED_PITCH} dropped')
    # links across mixed seams: b's sectors to a's normal neighbours, a's normal sectors to b's detailed ones
    a_norm = {sec: g for sec, g in zip(sorted(a['sectors']), a['graphs']) if sec not in dset}
    b_det = {sec: g for sec, g in zip(sorted(b['sectors']), b['graphs'])}
    a_view = dict(a, info={s_: i for s_, i in a['info'].items() if s_ not in dset})
    for here, there, mine in ((b, a_view, b_det), (a, b, a_norm)):
        for (X, Y), g in mine.items():
            areas, mesh = here['info'][(X, Y)]['areas'], g[2]
            same = same_grid_resolver(here['level'], {s_: i for s_, i in here['info'].items()
                                                      if s_ in mine}, X, Y)
            cross = cross_grid_resolver(here, there, X, Y)
            mesh.links = sector_links(X, Y, areas, mesh.areas,
                                      lambda other, ar, bd, s=same, c=cross, mine=mine: s(other, ar, bd) if other in mine else c(other, ar, bd))
    merged = []
    for sec in sorted(set(a_norm) | set(b_det)):
        sid, bnd, mesh, pts_, pairs, origin = a_norm[sec] if sec in a_norm else b_det[sec]
        if drop and sec in a_norm:
            # a's normal sector: drop the mixed-seam shared nodes stage b had no ground for
            gone = {q for q, p in enumerate(pts_) if p[0] in drop}
            pairs = [p for p in pairs if p[0] not in gone and p[1] not in gone]
        merged.append((sid, bnd, mesh, pts_, pairs, origin))
    log(f'merged: {len(a_norm)} normal + {len(b_det)} detailed sectors')
    return merged


def write_file(graphs: Sequence[Graph], out_path: str, log: Log, t0: float, cfg: BuildSettings = DEFAULTS) -> None:
    """Order-node pruning, final node / edge tables, and the file."""
    hpd_sectors = []
    stamp = time.localtime()
    date = (stamp.tm_year % 100) * 10000 + (stamp.tm_mon - 1) * 100 + stamp.tm_mday
    clock = stamp.tm_hour * 1000000 + stamp.tm_min * 10000 + stamp.tm_sec * 100
    if ORDER_IDS and ORDER_MIN_ISLAND:
        graphs = prune_order_nodes(graphs)
    linked = {pts[i][0] for _sid, _b, _m, pts, pairs, _o in graphs for a_, b_, _d in pairs for i in (a_, b_)}
    for sid, bounds, mesh, pts, pairs, origin in graphs:
        nodes, edges = finalize_graph(pts, pairs, linked, origin, cfg)
        hpd_sectors.append(hpdtool.Sector(sid, bounds, 0, date, clock, nodes, edges, kymesh.write_mesh(mesh)))
    out = hpdtool.HpdFile(HEADER, hpd_sectors)
    data = hpdtool.write(out)
    open(out_path, 'wb').write(data)
    log(f'wrote {out_path}: {len(hpd_sectors)} sectors, {sum(len(x.nodes) for x in hpd_sectors)} nodes, '
        f'{sum(len(x.edges) for x in hpd_sectors)} edges, {len(data)} bytes ({time.time() - t0:.0f} s)')


def line_walkable(level: Level, k0: int, a: tuple[int, int], b: tuple[int, int], gi0: int, gj0: int, k1: int) -> bool:
    """Straight line from local sample a to b (i, j) over connected kept surfaces, starting on k0, ending on k1 (or a
    surface within 0.6 m of k1's height), never stepping across one of level.blocked's D sides (DETAIL_BLOCKS_EDGES:
    a straight step across one; a diagonal step through the side column the line passes, either of them when it
    passes exactly through the corner); in the Rust core."""
    nat = level.native()
    level.sync_blocked(nat)
    return cast(bool, nat.line_walkable(int(k0), (int(a[0]), int(a[1])), (int(b[0]), int(b[1])), int(gi0), int(gj0),
                                        int(k1), LINK_RULE))


def _number(v: Any) -> bool:
    """A Python int or float (or a float64 scalar): the Rust core computes with it as the reference does."""
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _int_cells(areas: Iterable[AreaInfo]) -> bool:
    """Every area's cells hold Python ints (as build_areas makes them): the Rust core's cell picking computes with them
    as the reference does."""
    return all(isinstance(v, (int, np.integer)) and not isinstance(v, bool) for a in areas for c in a['cells'][:1]
               for v in c[:2])


def _centre_cells(areas: Sequence[AreaInfo]) -> list[int] | None:
    """For each area, the index of build_graph's centre cell (the cell nearest the cells' mean) from the Rust core;
    None: build_graph picks them itself."""
    if _native is None or not hasattr(_native, 'navkit_centre_cells') or not _int_cells(areas):
        return None
    try:
        return cast(list[int], _native.navkit_centre_cells([a['cells'] for a in areas]))
    except _NATIVE_ERRORS:
        return None


def _nearest_cells(areas: Sequence[AreaInfo], ends: Sequence[tuple[int, list[tuple[float, float]]]], ox: float,
                   oz: float, pitch: float) -> list[list[int]] | None:
    """For each (area index, [portal middle (x, z)]), the index in the area's cells of the cell nearest each middle,
    from the Rust core; None: build_graph picks them itself (also when a number is not a float: a float32 would make
    the reference's key compute in float32; a float64 scalar's ** 2 is the same C pow as a Python float's)."""
    if _native is None or not hasattr(_native, 'navkit_nearest_cells'):
        return None
    if not (isinstance(ox, float) and isinstance(oz, float) and isinstance(pitch, float)
            and all(isinstance(v, float) for _ai, mids in ends for m in mids for v in m)
            and _int_cells(areas[ai] for ai, mids in ends if mids)):
        return None
    try:
        return cast(list[list[int]], _native.navkit_nearest_cells(
            [(areas[ai]['cells'] if mids else [], mids) for ai, mids in ends], ox, oz, pitch))
    except _NATIVE_ERRORS:
        return None


def _route_native(level: Level, pts: Sequence[RouteNode], ox: Any, oz: Any, cfg: BuildSettings) -> bool:
    """build_graph's route edges may run in the Rust core: it is there, the window is a plain range and the numbers
    are the build's (Python floats: float32 positions would make the reference's // compute in float32)."""
    return (_native is not None and isinstance(level, Level) and cfg.window.step == 1
            and all(isinstance(v, float) for v in (ox, oz, cfg.pitch, cfg.y_offset, cfg.dist_edge_max))
            and all(isinstance(v, float) for p in pts for v in p[1]))


def edge_pairs(level: Level, pts: Sequence[RouteNode], gi0: int, gj0: int, ox: float, oz: float,
               cfg: BuildSettings = DEFAULTS) -> list[RoutePair]:
    """build_graph's walkable pairs: node pairs within DIST_EDGE_MAX (15 m buckets, the 3 x 3 around each node) whose
    straight line is walkable. In the Rust core when it is there (round E3c, the nodes tested in parallel; the Python
    below is its reference and fallback)."""
    if _route_native(level, pts, ox, oz, cfg):
        nat = level.native()
        level.sync_blocked(nat)
        try:
            return cast(list[RoutePair], nat.route_edges(pts if isinstance(pts, list) else list(pts), gi0, gj0,
                                                         (cfg.window.start, cfg.window.stop), cfg.dist_edge_max,
                                                         LINK_RULE))
        except _NATIVE_ERRORS:
            pass
    cell = cfg.dist_edge_max
    grid = defaultdict(list)
    for idx, p in enumerate(pts):
        grid[(int(p[1][0] // cell), int(p[1][2] // cell))].append(idx)
    pairs = []
    for idx, p in enumerate(pts):
        gx, gz = int(p[1][0] // cell), int(p[1][2] // cell)
        for dx in (-1, 0, 1):
            for dz in (-1, 0, 1):
                for q in grid.get((gx + dx, gz + dz), ()):
                    if q <= idx:
                        continue
                    d = math.dist(p[1], pts[q][1])
                    if d <= cfg.dist_edge_max and line_walkable(level, p[2], (p[3], p[4]), (pts[q][3], pts[q][4]), gi0, gj0, pts[q][2]):
                        pairs.append((idx, q, d))
    return pairs


def _graph_cfg(level: Level, cfg: BuildSettings) -> tuple[Any, ...] | None:
    """The build's settings as the Rust core's sector graph takes them (round E3h), or None: build_graph runs its
    pieces (the switches it does not port: DEDUP_GRAPH_NODES off, DETOUR_REPAIR on; numbers not Python floats)."""
    cover = cfg.node_cover.get('detailed' if abs(cfg.pitch - DETAILED_PITCH) < 1e-6 else 'normal')
    s: Any = getattr(level, 's', None)
    if (_native is None or not isinstance(level, Level) or not DEDUP_GRAPH_NODES or DETOUR_REPAIR
            or cfg.window.step != 1 or (cover and not _number(cover)) or not _number(ORDER_NODE_MIN)
            or not all(type(v) is float for v in (cfg.pitch, cfg.y_offset, cfg.dist_edge_max))
            or not all(type(v) is float for v in (getattr(s, 'min_x', None), getattr(s, 'min_z', None)))):
        return None
    gabriel = GABRIEL_EDGES == 'all' or (GABRIEL_EDGES == 'detailed' and abs(cfg.pitch - DETAILED_PITCH) < 1e-6)
    return (cfg.pitch, cfg.y_offset, (cfg.window.start, cfg.window.stop), cfg.dist_edge_max, LINK_RULE,
            float(cover) if cover else None, gabriel, bool(cfg.order_nodes), float(ORDER_NODE_MIN), (s.min_x, s.min_z))


def _graph_job(n: int, traced: Sequence[Traced], borders: Sequence[tuple[int, Vec3, int]], gi0: int, gj0: int,
               ox: float, oz: float, orders: Points, ladders: Sequence[tuple[int, int]],
               blocked: set[tuple[int, int]]) -> tuple[Any, ...] | None:
    """One sector's inputs to the Rust core's sector graph (n: its index in the areas sent), or None: its numbers are
    not the build's own (Python floats: round(v, 3) and // on a NumPy scalar are NumPy's)."""
    mids = [[((p.p0[0] + p.p1[0]) / 2, (p.p0[2] + p.p1[2]) / 2) for p in portals if not p.target < ai]
            for ai, (portals, _b, _o) in enumerate(traced)]
    if (type(ox) is not float or type(oz) is not float
            or not all(type(v) is float for b in borders for v in b[1])
            or not all(type(v) is float for p in orders for v in p) or not all(len(p) == 3 for p in orders)
            or not all(type(v) is float for m in mids for xz in m for v in xz)):
        return None
    return (n, gi0, gj0, (ox, oz), mids, [(pos, k) for _nid, pos, k in borders], [tuple(p) for p in orders],
            list(ladders), [(int(a), int(b)) for a, b in blocked])


def _graph_finish(got: tuple[list[Any], list[RoutePair], list[int]], borders: Sequence[tuple[int, Vec3, int]],
                  new_id: Callable[[], int], orders: Points,
                  cfg: BuildSettings) -> tuple[list[RouteNode], list[RoutePair]]:
    """build_graph's answer from the Rust core's sector graph: the new nodes given their ids in the reference's
    order, the border nodes their own; the order nodes counted as add_order_nodes counts them."""
    nodes, pairs, order_at = got
    pts: list[RouteNode] = [(new_id(), pos, k, i, j) if b < 0 else (borders[b][0], borders[b][1], k, i, j)
                            for b, pos, k, i, j in nodes]
    for u in order_at:
        ORDER_IDS.add(pts[u][0])
    if cfg.order_nodes and orders:
        ORDER_STATS['added'] = ORDER_STATS.get('added', 0) + len(order_at)
    return pts, pairs


def build_graph(level: Level, areas: Sequence[AreaInfo], area_of: dict[int, int], traced: Sequence[Traced],
                borders: Sequence[tuple[int, Vec3, int]], gi0: int, gj0: int, ox: float, oz: float,
                new_id: Callable[[], int], orders: Points = (), ladders: Sequence[tuple[int, int]] = (),
                cfg: BuildSettings = DEFAULTS) -> tuple[list[RouteNode], list[RoutePair]]:
    """A sector's route graph: (pts, pairs). In the Rust core whole when it is there (round E3h: bc2_navbuild::sector,
    also run for a stage's sectors at once by stage); the Python below is its reference and fallback (its pieces
    call the Rust core themselves: E2a's and E3c's)."""
    gcfg = _graph_cfg(level, cfg)
    job = _graph_job(0, traced, borders, gi0, gj0, ox, oz, orders, ladders, level.blocked) if gcfg else None
    if (job is not None and _int_cells(areas)
            and area_of.keys() == {c[2] for a in areas for c in a['cells']}):     # (stage's area_of always is)
        try:
            nat = level.native()
            got = nat.graph_many(nat.areas_from([[a['cells'] for a in areas]]), [job], gcfg)[0]
        except _NATIVE_ERRORS:
            pass
        else:
            return _graph_finish(got, borders, new_id, orders, cfg)
    pts: list[RouteNode] = []    # (id, pos, k, local i, local j)
    # one node per surface: an area-centre or portal node whose surface already holds a node (another area's or
    # portal's, or a border node) is not added - DICE's route graphs have no two nodes at one position except a
    # border node in both of its sectors (sp_004_b: 1,515 shared positions, all border pairs with one id); NavKit
    # had 377 duplicate pairs joined by zero-length edges (2026-10-20)
    taken = {k for _nid, _pos, k in borders} if DEDUP_GRAPH_NODES else set()
    # ... and none at a border node's position: a shared node carried from the other pitch's build is attached to
    # ground up to CARRY_RADIUS away, so its surface can differ from the one under its own position - an area node
    # on that one stood on the border node, joined by a zero-length edge (sp_007 sector 0e0b, 2026-10-23)
    taken_pos = {tuple(round(v, 3) for v in pos) for _nid, pos, _k in borders} if DEDUP_GRAPH_NODES else set()
    a: Any                                  # an area here, a path index in the string pulling below

    def add(i: int, j: int, k: int) -> None:
        pos = (ox + i * cfg.pitch, float(level.y[k]) + cfg.y_offset, oz + j * cfg.pitch)
        if DEDUP_GRAPH_NODES:
            key = tuple(round(v, 3) for v in pos)
            if k in taken or key in taken_pos:
                return
            taken.add(k)
            taken_pos.add(key)
        pts.append((new_id(), pos, k, i, j))
    big = [a for a in areas if len(a['cells']) >= 4]
    centres = _centre_cells(big)
    for n, a in enumerate(big):
        if centres is not None:
            i, j, k = a['cells'][centres[n]]
        else:
            ci = sum(c[0] for c in a['cells']) / len(a['cells'])
            cj = sum(c[1] for c in a['cells']) / len(a['cells'])
            i, j, k = min(a['cells'], key=lambda c: (c[0] - ci) ** 2 + (c[1] - cj) ** 2)
        add(i, j, k)
    cell_of = {c[2]: (c[0], c[1]) for a in areas for c in a['cells']}
    # one node per portal pair, on the area's cell nearest the portal's middle
    portal_ends = [(ai, [((p.p0[0] + p.p1[0]) / 2, (p.p0[2] + p.p1[2]) / 2) for p in portals if not p.target < ai])
            for ai, (portals, _b, _o) in enumerate(traced)]
    picks = _nearest_cells(areas, portal_ends, ox, oz, cfg.pitch)
    for n, (ai, mids) in enumerate(portal_ends):
        for m, (mx, mz) in enumerate(mids):
            if picks is not None:
                i, j, k = areas[ai]['cells'][picks[n][m]]
            else:
                i, j, k = min(areas[ai]['cells'],
                              key=lambda c: (ox + c[0] * cfg.pitch - mx) ** 2 + (oz + c[1] * cfg.pitch - mz) ** 2)
            add(i, j, k)
    for nid, pos, k in borders:
        # the node's own surface cell (a node carried from another pitch's build sits up to CARRY_RADIUS from it;
        # walkable lines must start on that cell, not on the cell under the position)
        i, j = cell_of.get(k) or (int(round((pos[0] - ox) / cfg.pitch)), int(round((pos[2] - oz) / cfg.pitch)))
        pts.append((nid, pos, k, i, j))
    cover = cfg.node_cover.get('detailed' if abs(cfg.pitch - DETAILED_PITCH) < 1e-6 else 'normal')
    if cover:
        add_cover_nodes(level, areas, pts, add, ox, oz, cover, cfg)
    # edges: pairs within DIST_EDGE_MAX whose straight line is walkable
    pairs = edge_pairs(level, pts, gi0, gj0, ox, oz, cfg)
    if GABRIEL_EDGES == 'all' or (GABRIEL_EDGES == 'detailed' and abs(cfg.pitch - DETAILED_PITCH) < 1e-6):
        pairs = gabriel_prune(pts, pairs)
    for kf, kt in ladders:
        # a ladder link: a node on the ground at its foot and at its top, joined directly (the climb is no walkable
        # line); the runtime's mover walks the bot into the ladder, which takes any soldier who walks into it
        ends = []
        for k in (kf, kt):
            at = next((q for q, p in enumerate(pts) if p[2] == k), None)
            if at is None:
                i, j = cell_of[k]
                pts.append((new_id(), (ox + i * cfg.pitch, float(level.y[k]) + cfg.y_offset, oz + j * cfg.pitch), k, i, j))
                at = len(pts) - 1
            ends.append(at)
        pairs.append((ends[0], ends[1], math.dist(pts[ends[0]][1], pts[ends[1]][1])))
    # Connect remaining groups: grid search (8-connected, step rule) from one group to the nearest node
    # of another, then string-pull the path into straight walkable legs with waypoint nodes.
    node_at, cell_ij = connect_groups(level, areas, pts, pairs, gi0, gj0, ox, oz, new_id, cfg)
    if DETOUR_REPAIR:
        for _pass in range(DETOUR_PASSES):
            repair_detours(level, pts, pairs, node_at, cast(dict[int, tuple[int, int]], cell_ij), gi0, gj0, ox, oz,
                           new_id, cfg)
    if cfg.order_nodes and orders:
        ORDER_STATS['added'] = ORDER_STATS.get('added', 0) + add_order_nodes(level, pts, pairs, node_at, area_of, orders,
                                                                             gi0, gj0, ox, oz, new_id, cfg)
    return pts, pairs


def connect_groups(level: Level, areas: Sequence[AreaInfo], pts: list[RouteNode], pairs: list[RoutePair], gi0: int,
                   gj0: int, ox: float, oz: float, new_id: Callable[[], int],
                   cfg: BuildSettings = DEFAULTS) -> tuple[dict[int, int], dict[int, tuple[int, int]] | None]:
    """build_graph's joining of the node groups its pairs leave apart: a grid search (8-connected, step rule) from one
    group to the nearest node of another, string-pulled into straight walkable legs with waypoint nodes (appended to
    pts with new_id, their pairs to pairs), pass after pass until no group is joined. Returns (node_at: surface -> its
    first node; cell_ij: surface -> cell, or None from the Rust core). In the Rust core when it is there and
    DETOUR_REPAIR (which reads cell_ij) is off (round E3c; the Python below is its reference and fallback)."""
    if not DETOUR_REPAIR and _route_native(level, pts, ox, oz, cfg):
        nat = level.native()
        level.sync_blocked(nat)
        try:
            new_nodes, new_pairs = nat.connect_groups(
                pts, pairs if isinstance(pairs, list) else list(pairs), [a['cells'] for a in areas], (ox, oz),
                cfg.pitch, cfg.y_offset, (gi0, gj0, (cfg.window.start, cfg.window.stop), cfg.dist_edge_max), LINK_RULE)
        except _NATIVE_ERRORS:
            pass
        else:
            for k, i, j, pos in new_nodes:          # ids in creation order, as the reference gives them
                pts.append((new_id(), pos, k, i, j))
            pairs.extend(new_pairs)
            node_at: dict[int, int] = {}
            for idx, p in enumerate(pts):
                node_at.setdefault(p[2], idx)
            return node_at, None
    parent = list(range(len(pts)))

    def find(x: int) -> int:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    for a_, b_, _ in pairs:
        parent[find(a_)] = find(b_)
    node_at = {}
    for idx, p in enumerate(pts):
        node_at.setdefault(p[2], idx)
    cell_ij = {c[2]: (c[0], c[1]) for a in areas for c in a['cells']}
    for p in pts:
        cell_ij.setdefault(p[2], (p[3], p[4]))
    changed = True
    while changed:
        changed = False
        groups = defaultdict(list)
        for idx in range(len(pts)):
            groups[find(idx)].append(idx)
        if len(groups) < 2:
            break
        for root, members in sorted(groups.items(), key=lambda g: len(g[1])):
            if find(members[0]) != root:
                continue
            # the search, in the Rust core: breadth first from the members' surfaces over neighbouring kept surfaces
            # of the sector window (each surface reached entered into cell_ij) to the first surface of another
            # group's node; the path of surfaces from a member's to it, or None
            if cfg.window.step != 1:
                raise ValueError(f'a sector window of step {cfg.window.step}: the search walks whole windows')
            other = {k for k, at in node_at.items() if find(at) != root}
            path = level.native().search([pts[m][2] for m in members], other, cell_ij,
                                         (cfg.window.start, cfg.window.stop), gi0, gj0, LINK_RULE)
            if path is None:
                continue
            # string pulling
            way = [path[0]]
            a = 0
            while a < len(path) - 1:
                b = len(path) - 1
                while b > a + 1 and not (math.dist(cell_ij[path[a]], cell_ij[path[b]]) * cfg.pitch <= cfg.dist_edge_max and
                                         line_walkable(level, path[a], cell_ij[path[a]], cell_ij[path[b]], gi0, gj0, path[b])):
                    b -= 1
                way.append(path[b])
                a = b
            chain = []
            for k in way:
                if k in node_at:
                    chain.append(node_at[k])
                else:
                    i, j = cell_ij[k]
                    pts.append((new_id(), (ox + i * cfg.pitch, float(level.y[k]) + cfg.y_offset, oz + j * cfg.pitch), k, i, j))
                    parent.append(len(pts) - 1)
                    node_at[k] = len(pts) - 1
                    chain.append(len(pts) - 1)
            for u, v in zip(chain, chain[1:]):
                pairs.append((u, v, math.dist(pts[u][1], pts[v][1])))
                parent[find(u)] = find(v)
            changed = True
    return node_at, cell_ij


def add_cover_nodes(level: Level, areas: Sequence[AreaInfo], pts: list[RouteNode], add: Callable[[int, int, int], None],
                    ox: float, oz: float, cover: float, cfg: BuildSettings = DEFAULTS) -> None:
    """NODE_COVER: in every area of 4+ cells, add route nodes (through add, so one per surface) at the cell farthest from
    all nodes on the same floor (|dy| < 2.5 m) until no cell is farther than cover metres. In the Rust core when it is
    there (round E3c; the Python below is its reference and fallback - add is called the same way either way)."""
    if (_native is not None and isinstance(level, Level) and isinstance(pts, list) and _number(cover)
            and all(isinstance(v, float) for v in (ox, oz, cfg.pitch, cfg.y_offset))):
        try:
            level.native().cover_nodes([a['cells'] for a in areas], pts, add, (ox, oz), cfg.pitch, cfg.y_offset,
                                       float(cover))
            return
        except _NATIVE_ERRORS:
            pass                                 # refused before add was called: the Python runs
    for a in areas:
        if len(a['cells']) < 4:
            continue
        C = np.array([(ox + i * cfg.pitch, float(level.y[k]) + cfg.y_offset, oz + j * cfg.pitch) for i, j, k in a['cells']])
        x0, x1, z0, z1 = C[:, 0].min() - cover, C[:, 0].max() + cover, C[:, 2].min() - cover, C[:, 2].max() + cover
        P = np.array([p[1] for p in pts if x0 <= p[1][0] <= x1 and z0 <= p[1][2] <= z1], float).reshape(-1, 3)
        d = np.full(len(C), np.inf)
        for q in P:
            same = np.abs(C[:, 1] - q[1]) < 2.5
            d = np.where(same, np.minimum(d, np.hypot(C[:, 0] - q[0], C[:, 2] - q[2])), d)
        while True:
            w = int(np.argmax(d))
            if d[w] <= cover:
                break
            i, j, k = a['cells'][w]
            n = len(pts)
            add(i, j, k)
            q = C[w]
            d[w] = 0.0                               # placed (or the surface already held a node)
            if len(pts) > n:
                same = np.abs(C[:, 1] - q[1]) < 2.5
                d = np.where(same, np.minimum(d, np.hypot(C[:, 0] - q[0], C[:, 2] - q[2])), d)


def gabriel_prune(pts: Sequence[RouteNode], pairs: Iterable[RoutePair]) -> list[RoutePair]:
    """The pairs that are Gabriel edges of the sector's nodes (no other node within 2.5 m of height inside the circle
    with the pair as diameter), plus the shortest dropped pairs needed to keep every group of nodes the full pair set
    joins joined (so pruning never splits a route). In the Rust core when it is there (round E3c; the Python below is
    its reference and fallback)."""
    pairs = list(pairs)
    if _native is not None and hasattr(_native, 'navkit_gabriel') and all(type(p[2]) is float for p in pairs):
        try:
            return [pairs[n] for n in _native.navkit_gabriel([p[1] for p in pts], pairs)]
        except _NATIVE_ERRORS:
            pass
    P = np.array([p[1] for p in pts], float)
    keep: list[tuple[int, int, float]]
    rest: list[tuple[int, int, float]]
    keep, rest = [], []
    for a_, b_, d in pairs:
        m = (P[a_] + P[b_]) / 2
        r2 = (d / 2) ** 2 - 1e-9
        near = (np.abs(P[:, 1] - m[1]) < 2.5) & (((P[:, 0] - m[0]) ** 2 + (P[:, 2] - m[2]) ** 2) < r2)
        near[[a_, b_]] = False
        (rest if near.any() else keep).append((a_, b_, d))
    parent = list(range(len(pts)))

    def find(x: int) -> int:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    for a_, b_, _d in keep:
        parent[find(a_)] = find(b_)
    for a_, b_, d in sorted(rest, key=lambda e: e[2]):
        if find(a_) != find(b_):
            parent[find(a_)] = find(b_)
            keep.append((a_, b_, d))
    return keep


def prune_order_nodes(graphs: Sequence[Graph]) -> Sequence[Graph]:
    """Order nodes in a route island under ORDER_MIN_ISLAND nodes lose their edges (finalize_graph then
    drops them as unlinked); other nodes are untouched. An order node that joins a small pocket becomes the
    nearest node for points next to the main network, and the runtime takes the nearest node (sp_009: the order
    nodes cut connectivity 4.4 points through such pockets; 2026-10-20). Components over all sectors (border ids
    join them). Records the removed ones in ORDER_STATS['removed'] as (id, position, component size)."""
    parent: dict[int, int] = {}

    def find(x: int) -> int:
        parent.setdefault(x, x)
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    for _sid, _b, _m, pts, pairs, _o in graphs:
        for p in pts:
            find(p[0])
        for a_, b_, _d in pairs:
            ra, rb = find(pts[a_][0]), find(pts[b_][0])
            if ra != rb:
                parent[ra] = rb
    size: defaultdict[int, int] = defaultdict(int)
    for x in list(parent):
        size[find(x)] += 1
    if not size:
        return graphs
    drop = {i for i in ORDER_IDS if size[find(i)] < ORDER_MIN_ISLAND}
    seen = set()
    for _sid, _b, _m, pts, _pairs, _o in graphs:
        for p in pts:
            if p[0] in drop and p[0] not in seen:
                seen.add(p[0])
                ORDER_STATS.setdefault('removed', []).append((p[0], tuple(round(v, 2) for v in p[1]), size[find(p[0])]))
    return [(sid, b, m, pts, [q for q in pairs if pts[q[0]][0] not in drop and pts[q[1]][0] not in drop], o)
            for sid, b, m, pts, pairs, o in graphs]


def add_order_nodes(level: Level, pts: list[RouteNode], pairs: list[RoutePair], node_at: dict[int, int],
                    area_of: dict[int, int], orders: Points, gi0: int, gj0: int, ox: float, oz: float,
                    new_id: Callable[[], int], cfg: BuildSettings = DEFAULTS) -> int:
    """A route node at each AI order position (AreaGeometryEntityData) on this sector's walkable ground whose nearest
    route node on the same floor is farther than ORDER_NODE_MIN, linked to every node within DIST_EDGE_MAX it reaches
    in a straight walkable line. The runtime resolves a go-to point to the nearest gathered node (R = 15 m, the first
    candidate accepted; 0x0105ED80) - at sp_004_b's stairs top NavKit's nearest node was 3.6-6.3 m from the order
    points, DICE's 0.2 m, and those requests failed (2026-10-20). Returns the number of nodes added."""
    s = level.s
    added = 0
    for x, y, z in orders:
        gi, gj = int(round((x - s.min_x) / cfg.pitch)), int(round((z - s.min_z) / cfg.pitch))
        cands = [k for k in level.surfaces(gi, gj) if k in area_of and abs(level.y[k] - y) <= 1.0]
        if not cands:
            continue
        k = min(cands, key=lambda q: abs(level.y[q] - y))
        if k in node_at:
            continue
        pos = (ox + (gi - gi0) * cfg.pitch, float(level.y[k]) + cfg.y_offset, oz + (gj - gj0) * cfg.pitch)
        near = [math.hypot(p[1][0] - pos[0], p[1][2] - pos[2]) for p in pts if abs(p[1][1] - pos[1]) < 1.5]
        if near and min(near) <= ORDER_NODE_MIN:
            continue
        u = len(pts)
        pts.append((new_id(), pos, k, gi - gi0, gj - gj0))
        ORDER_IDS.add(pts[u][0])
        node_at[k] = u
        added += 1
        for q in range(u):
            d = math.dist(pos, pts[q][1])
            if d <= cfg.dist_edge_max and line_walkable(level, k, (gi - gi0, gj - gj0), (pts[q][3], pts[q][4]), gi0, gj0, pts[q][2]):
                pairs.append((u, q, d))
    return added


def repair_detours(level: Level, pts: list[RouteNode], pairs: list[RoutePair], node_at: dict[int, int],
                   cell_ij: dict[int, tuple[int, int]], gi0: int, gj0: int, ox: float, oz: float,
                   new_id: Callable[[], int], cfg: BuildSettings = DEFAULTS) -> int:
    """Detour repair: for node pairs of the sector within DIST_EDGE_MAX whose graph route is longer than
    DETOUR_TRIGGER x their straight distance, while a walk over kept surfaces of at most DETOUR_MAX x the straight
    distance exists, add waypoint nodes along the string-pulled walk, linked along the chain (and to the nodes around
    them by the edge rule only with DETOUR_LINK_ALL).
    DICE's routes run 1.02-1.11x the straight distance where NavKit's failed go-to requests ran 3.2-6.3x
    (sp_004_b stairs top, 2026-10-20). Returns the number of nodes added."""
    adj: defaultdict[int, dict[int, float]] = defaultdict(dict)
    for a_, b_, d in pairs:
        adj[a_][b_] = adj[b_][a_] = min(d, adj[a_].get(b_, 1e18))

    def routes(src: int, limit: float) -> dict[int, float]:
        best = {src: 0.0}
        q = [(0.0, src)]
        while q:
            d, u = heapq.heappop(q)
            if d > best[u] or d > limit:
                continue
            for v, w in adj[u].items():
                if d + w < best.get(v, 1e18):
                    best[v] = d + w
                    heapq.heappush(q, (d + w, v))
        return best

    def walk(k0: int, k1: int, limit: float) -> list[int] | None:
        """Shortest 8-connected walk over kept surfaces (step rule) from k0 to k1, at most limit metres; list of
        surfaces or None."""
        best = {k0: 0.0}
        prev: dict[int, Any] = {k0: None}
        q = [(0.0, k0)]
        while q:
            d, k = heapq.heappop(q)
            if k == k1:
                path = [k]
                while prev[path[-1]] is not None:
                    path.append(prev[path[-1]])
                return path[::-1]
            if d > best[k]:
                continue
            i, j = cell_ij[k]
            for di in (-1, 0, 1):
                for dj in (-1, 0, 1):
                    if not (di or dj) or (i + di) not in cfg.window or (j + dj) not in cfg.window:
                        continue
                    n = level.neighbour(k, gi0 + i, gj0 + j, di, dj)
                    if n is None:
                        continue
                    nd = d + (1.4142 if di and dj else 1.0) * cfg.pitch
                    if nd <= limit and nd < best.get(n, 1e18):
                        best[n], prev[n] = nd, k
                        cell_ij.setdefault(n, (i + di, j + dj))
                        heapq.heappush(q, (nd, n))
        return None

    def link(u: int) -> None:
        for q_ in range(len(pts)):
            if q_ == u or q_ in adj[u]:
                continue
            d = math.dist(pts[u][1], pts[q_][1])
            if d <= cfg.dist_edge_max and line_walkable(level, pts[u][2], (pts[u][3], pts[u][4]), (pts[q_][3], pts[q_][4]),
                                                    gi0, gj0, pts[q_][2]):
                pairs.append((u, q_, d))
                adj[u][q_] = adj[q_][u] = d

    added = 0
    n0 = len(pts)
    for a in range(n0):
        best = routes(a, DETOUR_TRIGGER * cfg.dist_edge_max)
        for b in range(a + 1, n0):
            straight = math.dist(pts[a][1], pts[b][1])
            if straight > cfg.dist_edge_max or straight < 1e-3:
                continue
            route = best.get(b, 1e18)
            if route <= DETOUR_TRIGGER * straight:
                continue
            path = walk(pts[a][2], pts[b][2], DETOUR_MAX * straight)   # the walk itself within DETOUR_MAX x straight
            if path is None:
                continue
            way, s = [path[0]], 0
            while s < len(path) - 1:
                e = len(path) - 1
                while e > s + 1 and not (math.dist(cell_ij[path[s]], cell_ij[path[e]]) * cfg.pitch <= cfg.dist_edge_max and
                                         line_walkable(level, path[s], cell_ij[path[s]], cell_ij[path[e]], gi0, gj0, path[e])):
                    e -= 1
                way.append(path[e])
                s = e
            chain = []
            for k in way:
                if k in node_at:
                    chain.append(node_at[k])
                else:
                    i, j = cell_ij[k]
                    pts.append((new_id(), (ox + i * cfg.pitch, float(level.y[k]) + cfg.y_offset, oz + j * cfg.pitch), k, i, j))
                    node_at[k] = len(pts) - 1
                    chain.append(len(pts) - 1)
                    added += 1
            for u, v in zip(chain, chain[1:]):
                if v not in adj[u]:
                    d = math.dist(pts[u][1], pts[v][1])
                    pairs.append((u, v, d))
                    adj[u][v] = adj[v][u] = d
            if DETOUR_LINK_ALL:
                for u in chain:
                    if u >= n0:
                        link(u)
            best = routes(a, DETOUR_TRIGGER * cfg.dist_edge_max)
    return added


def finalize_graph(pts: Sequence[RouteNode], pairs: Iterable[RoutePair], linked_ids: Container[int],
                   origin: tuple[float, float] | None = None,
                   cfg: BuildSettings = DEFAULTS) -> tuple[list[hpdtool.Node], list[hpdtool.Edge]]:
    """Nodes and edges of a sector. Nodes without an edge in any sector (lone nodes on slivers and tiny islands
    the group joining cannot reach; border ids count edges of both sectors) are dropped - DICE's data has none."""
    keep = [idx for idx, p in enumerate(pts) if p[0] in linked_ids]
    if SORT_NODES_BY_ID:
        # DICE writes a sector's nodes sorted by id as an unsigned 32-bit value - ordinary nodes, then border nodes
        # (flag 80000000h) - in 40 of 40 sp_004_b sectors (masked to 31 bits: 0 of 40); the runtime binary-searches
        # these arrays for a path's start / destination ids (0x0108D5E0 / 0x0108D550). NavKit interleaved border
        # nodes (0 of 40 sorted) - 2026-10-20
        keep.sort(key=lambda idx: pts[idx][0] & 0xFFFFFFFF)
    new = {old: q for q, old in enumerate(keep)}
    out_edges = defaultdict(list)
    for a_, b_, d in pairs:
        if a_ in new and b_ in new:
            # the cost from the positions as stored (32-bit floats), as every DICE edge is: from the unrounded
            # positions, 138 of sp_004_b's 60,142 costs came out one unit off
            d = math.dist(*(tuple(float(np.float32(v)) for v in pts[i][1]) for i in (a_, b_)))
            cost = 0x80000000 | int(math.floor(d * 16))
            out_edges[new[a_]].append((cost, new[b_]))
            out_edges[new[b_]].append((cost, new[a_]))
    nodes: list[hpdtool.Node]
    edges: list[hpdtool.Edge]
    nodes, edges = [], []
    for q, idx in enumerate(keep):
        nid, pos = pts[idx][0], pts[idx][1]
        nodes.append(hpdtool.Node(nid, pos, 0x80000000, cfg.dist_edge_max if q == 0 else 0.0, 0, 0, 0))
    for idx in range(len(nodes)):
        first = len(edges)
        for cost, dst in sorted(out_edges[idx], key=lambda e: e[1]):
            edges.append(hpdtool.Edge(cost, idx, dst))
        nodes[idx].a = first + 1 if out_edges[idx] else 0     # DICE: 0 = no outgoing edges
    def foreign(nd: hpdtool.Node) -> bool:
        # bit 16 of c: a border node's copy in a sector that does not own it. DICE: one copy per border id is the
        # owner (bit clear) - the sector whose 100 m square holds the node (1,379 of 1,445 sp_004_b ids), the
        # others set; ordinary nodes never. NavKit set the bit on every border copy, so no border node had an
        # owner (2026-10-20)
        if not nd.id & 0x80000000:
            return False
        if not BORDER_OWNER or origin is None:
            return True
        ox, oz = origin
        return not (ox <= nd.pos[0] < ox + 100 and oz <= nd.pos[2] < oz + 100)
    n = len(nodes)
    by_x = sorted(range(n), key=lambda q: nodes[q].pos[0])
    by_y = sorted(range(n), key=lambda q: nodes[q].pos[1])
    by_z = sorted(range(n), key=lambda q: nodes[q].pos[2])
    for q in range(n):
        nodes[q].b = by_x[q] | (by_y[q] << 16)
        nodes[q].c = by_z[q] | (0x10000 if foreign(nodes[q]) else 0)
    return nodes, edges


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith('--')]
    opts = dict(zip(argv[1:], argv[2:]))
    if len(args) < 5:
        print(__doc__)
        return 1
    cfg = replace(DEFAULTS, detail_features=False) if '--no-detail' in argv else DEFAULTS
    build(args[0], args[1] if args[1] != '-' else None, int(args[2]), int(args[3]), args[4],
          float(opts.get('--step', 0.28)), dynamic_path=opts.get('--dynamic'), high_path=opts.get('--high'),
          narrow_path=opts.get('--narrow'), stairs_path=opts.get('--stairs'), exclude_path=opts.get('--exclude'),
          exclude_foot_path=opts.get('--exclude-foot'), cfg=cfg)
    return 0


if __name__ == '__main__':
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('navbuild', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
