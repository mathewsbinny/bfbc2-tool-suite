"""The path test: a route between two points over a path-data file's route graph, or why there is none.

    r = route(nav, start, end)          # nav: navdata.NavData; start, end: world (x, y, z) on the ground
    r.found, r.nodes, r.cost, r.reason, r.start_island, r.end_island, r.gap

The points are joined to the graph at the nearest route node within 15 m horizontally and 4 m in height (as
the studio's spawn check does); the route is found over the file's own directed edges and their stored costs
(A*, straight-line distance as the estimate). The stored costs are the edge lengths rounded down to 1/16 m, so the
estimate can exceed the remaining cost by up to 1/16 m per edge: a found route is the cheapest or within that
much of it (a few centimetres; the harness's route measures were taken this way, so it is kept). Without a route, the result says which island
each point is on and where the two islands come closest. This plans over the graph only: the game also
smooths the route through the areas, which this does not do (DESIGN.md section 7, question 4).

The search runs in the suite's Rust core (round E3o: bc2_native.PathGraph, one per NavData); its Python twin was retired
on 2026-11-18 (round R1; the answers recorded in tests/navdata_golden.json) and the module is required. The estimate
is the straight-line distance as sqrt(fma(z, z, fma(y, y, x * x))): np.linalg.norm's bits on the PC the routes were
recorded on (NumPy 2 on OpenBLAS, which fuses the multiply-adds; 200,000 random vectors), so a route's ties break as
they did there - now on every PC. Route.straight is that same formula (distance(); round CI1: it was
np.linalg.norm, a BLAS dot whose last bits follow the CPU's OpenBLAS kernel - SkylakeX on the recording PC, Haswell /
Zen on GitHub's runner, where half the recorded routes differed in it). closest_between's answer is kept per island
pair and NavData.
"""
import weakref
from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Any

import numpy as np

from . import navdata

JOIN_XZ, JOIN_Y = 15.0, 4.0


@dataclass
class Route:
    found: bool
    nodes: list[int] = field(default_factory=list)        # node indices, start to end
    cost: float = 0.0                                     # sum of the stored edge costs (m)
    length: float = 0.0                                   # 3-D length of the node chain (m)
    straight: float = 0.0                                 # start to end, straight (m)
    start_node: int | None = None
    end_node: int | None = None
    start_join: float = 0.0                               # metres from the point to its node
    end_join: float = 0.0
    start_island: int | None = None
    end_island: int | None = None
    gap: tuple[int | None, int | None, float] | None = None  # (node in start island, node in end island, metres)
    reason: str = ''
    expanded: int = 0


def ground_nodes(nav: navdata.NavData) -> np.ndarray:
    return nav.node_pos - np.array([0, navdata.Y_OFFSET, 0])


def join(nav: navdata.NavData, p: Sequence[float] | np.ndarray) -> tuple[int | None, float]:
    """(node index, horizontal distance) nearest to p within the join limits, or (None, distance)."""
    if not len(nav.node_pos):
        return None, np.inf
    g = ground_nodes(nav)
    d = np.hypot(g[:, 0] - p[0], g[:, 2] - p[2])
    d = np.where(np.abs(g[:, 1] - p[1]) <= JOIN_Y, d, np.inf)
    k = int(np.argmin(d))
    return (k, float(d[k])) if d[k] <= JOIN_XZ else (None, float(d[k]))


def _adjacency(nav: navdata.NavData) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    cache = getattr(nav, '_adj', None)
    if cache is None:
        order = np.argsort(nav.edges[:, 0], kind='stable') if len(nav.edges) else np.zeros(0, int)
        src = nav.edges[order, 0] if len(order) else np.zeros(0, int)
        start = np.searchsorted(src, np.arange(len(nav.node_pos) + 1))
        cache = (start, nav.edges[order, 1] if len(order) else np.zeros(0, int), nav.edge_cost[order] if len(order) else np.zeros(0))
        nav._adj = cache  # type: ignore[attr-defined]   # cached on the NavData object (not a dataclass field)
    return cache


_PER_NAV: dict[int, dict[str, Any]] = {}       # id(NavData) -> its route graph in the core, closest pairs
NORM = 'fma'                                   # the estimate's formula in the core (see above)


def _native() -> Any:
    """The Rust core with PathGraph, or bfbc2core.native.NativeMissing saying how to get it."""
    from . import native
    return native.require('the path test (navpath.route)', 'PathGraph')


def distance(v: Sequence[float] | np.ndarray) -> float:
    """The length of the 3-vector v by the estimate's formula (NORM, in the core): np.linalg.norm's bits on the PC
    the routes were recorded on, the same on every machine (round CI1)."""
    b = np.ascontiguousarray(np.asarray(v, float).reshape(3), '<f8').tobytes()
    return float(np.frombuffer(_native().PathGraph.norms(b, NORM), '<f8')[0])


def _per_nav(nav: navdata.NavData) -> dict[str, Any]:
    """This NavData's entry (dropped with it), made at the first use."""
    k = id(nav)
    d = _PER_NAV.get(k)
    if d is None:
        d = _PER_NAV[k] = {'closest': {}}
        weakref.finalize(nav, _PER_NAV.pop, k, None)
    return d


def _path_graph(nav: navdata.NavData) -> Any:
    """The core's PathGraph of nav's route graph (made at the first route); ValueError for a graph it cannot search
    (a cost or position that is not a finite number, an edge to no node)."""
    d = _per_nav(nav)
    if 'graph' not in d:
        start, nbr, cost = _adjacency(nav)
        try:
            d['graph'] = _native().PathGraph(
                np.ascontiguousarray(start, '<i8').tobytes(), np.ascontiguousarray(nbr, '<i8').tobytes(),
                np.ascontiguousarray(cost, '<f8').tobytes(),
                np.ascontiguousarray(nav.node_pos, '<f8').reshape(-1, 3).tobytes())
        except (ValueError, TypeError) as e:
            raise ValueError(f'navpath: the route graph of {nav.path} cannot be searched ({e}): a cost or node '
                             f'position that is not a finite number, or an edge to no node') from e
    return d['graph']


def closest_between(nav: navdata.NavData, ia: int | None,
                    ib: int | None) -> tuple[int | None, int | None, float] | None:
    """The closest pair of nodes of two islands (horizontal metres; None when one has no node); kept per island pair
    and NavData (round E3o)."""
    memo = _per_nav(nav)['closest']
    key = (ia, ib)
    if key not in memo:
        memo[key] = _closest(nav, ia, ib)
    return memo[key]


def _closest(nav: navdata.NavData, ia: int | None,
             ib: int | None) -> tuple[int | None, int | None, float] | None:
    a = np.nonzero(nav.node_island == ia)[0]
    b = np.nonzero(nav.node_island == ib)[0]
    if not len(a) or not len(b):
        return None
    pa, pb = nav.node_pos[a][:, [0, 2]], nav.node_pos[b][:, [0, 2]]
    best: tuple[int | None, int | None, float] = (None, None, np.inf)
    for k in range(0, len(pa), 512):
        d = np.hypot(pa[k:k + 512, None, 0] - pb[None, :, 0], pa[k:k + 512, None, 1] - pb[None, :, 1])
        i, j = np.unravel_index(int(np.argmin(d)), d.shape)
        if d[i, j] < best[2]:
            best = (int(a[k + i]), int(b[j]), float(d[i, j]))
    return best


def route(nav: navdata.NavData, start: Sequence[float] | np.ndarray, end: Sequence[float] | np.ndarray) -> Route:
    """The route from start to end: A* over the directed edges and their stored costs from the start's node, the
    estimate the straight distance to the end's node; nodes compared as (f, g, node), a node expanded once."""
    start, end = np.asarray(start, float), np.asarray(end, float)
    r = Route(False, straight=distance(end - start))
    r.start_node, r.start_join = join(nav, start)
    r.end_node, r.end_join = join(nav, end)
    if r.start_node is None or r.end_node is None:
        which = 'start' if r.start_node is None else 'end'
        r.reason = f'no route node within {JOIN_XZ:.0f} m of the {which} point (nearest {min(r.start_join if which == "start" else r.end_join, 9999):.0f} m)'
        return r
    r.start_island, r.end_island = int(nav.node_island[r.start_node]), int(nav.node_island[r.end_node])
    r.expanded, nodes, cost = _path_graph(nav).route(r.start_node, r.end_node, NORM)
    if not nodes:
        _not_found(nav, r)
        return r
    pos = nav.node_pos
    r.nodes = nodes
    r.cost = cost
    r.length = float(np.sum(np.linalg.norm(np.diff(pos[r.nodes], axis=0), axis=1))) if len(r.nodes) > 1 else 0.0
    r.found = True
    return r


def _not_found(nav: navdata.NavData, r: Route) -> None:
    if r.start_island != r.end_island:
        r.gap = closest_between(nav, r.start_island, r.end_island)
        r.reason = (f'the points are on different islands ({r.start_island} and {r.end_island}); they come closest '
                    f'{r.gap[2]:.1f} m apart' if r.gap else f'the points are on different islands ({r.start_island}, {r.end_island})')
    else:
        r.reason = 'same island, but no route along the edges\' directions (one-way edges)'
