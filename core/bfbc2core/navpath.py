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

Round E3o: the search runs in the suite's Rust core when it is built (bc2_native.PathGraph, one per NavData: the same
pops, relaxations and estimate bits as route_reference, which stays as the reference and the fallback). The estimate
is np.linalg.norm, whose last bits depend on the BLAS (OpenBLAS fuses the multiply-adds): the formula the core uses is
the one that gives NumPy's bits on 20,000 test vectors on this machine, or the reference runs (norm_formula()).
closest_between's answer is kept per island pair and NavData. BFBC2_PURE_PYTHON=1 or use_native(False): the reference.
"""
import heapq
import os
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
_NATIVE: list[Any] = []                        # [module or None, the norm formula or None]


def _native() -> tuple[Any, str | None]:
    """(the Rust core with PathGraph, the norm formula that matches np.linalg.norm here), or (None, None)."""
    if not _NATIVE:
        m = None
        try:
            from . import dbx as _dbx
            if os.environ.get(_dbx.PURE_PYTHON_ENV, '') in ('', '0'):
                m = _dbx._import_native()
        except ImportError:
            m = None
        m = m if m is not None and hasattr(m, 'PathGraph') else None
        _NATIVE[:] = [m, norm_formula(m) if m is not None else None]
    return _NATIVE[0], _NATIVE[1]


def use_native(on: bool) -> bool:
    """The Rust search on (when built and its norm matches, whatever BFBC2_PURE_PYTHON says) or off for this process;
    True when it runs."""
    _NATIVE.clear()
    _PER_NAV.clear()
    if not on:
        _NATIVE[:] = [None, None]
        return False
    saved = os.environ.pop('BFBC2_PURE_PYTHON', None)
    try:
        m, f = _native()
        return m is not None and f is not None
    finally:
        if saved is not None:
            os.environ['BFBC2_PURE_PYTHON'] = saved


def norm_formula(m: Any) -> str | None:
    """'fma' or 'plain': the core's formula whose values are np.linalg.norm's, bit for bit, on 20,000 seeded vectors
    (three values each, scales from millimetres to kilometres, as pos[m] - pos[goal] is), or None."""
    rng = np.random.default_rng(20261117)
    v = rng.normal(size=(20000, 3)) * rng.choice([1e-3, 0.1, 1.0, 37.0, 900.0, 4000.0], size=(20000, 1))
    want = np.array([np.linalg.norm(x) for x in v], dtype=np.float64)
    for name in ('fma', 'plain'):
        got = np.frombuffer(m.PathGraph.norms(np.ascontiguousarray(v, '<f8').tobytes(), name), '<f8')
        if got.tobytes() == want.tobytes():
            return name
    return None


def _per_nav(nav: navdata.NavData) -> dict[str, Any]:
    """This NavData's entry (dropped with it), made at the first use."""
    k = id(nav)
    d = _PER_NAV.get(k)
    if d is None:
        d = _PER_NAV[k] = {'closest': {}}
        weakref.finalize(nav, _PER_NAV.pop, k, None)
    return d


def _path_graph(nav: navdata.NavData) -> tuple[Any, str] | None:
    """(the core's PathGraph of nav's route graph, the norm formula), or None: the reference searches."""
    m, formula = _native()
    if m is None or formula is None:
        return None
    d = _per_nav(nav)
    if 'graph' not in d:
        start, nbr, cost = _adjacency(nav)
        try:
            d['graph'] = m.PathGraph(np.ascontiguousarray(start, '<i8').tobytes(), np.ascontiguousarray(nbr, '<i8').tobytes(),
                                     np.ascontiguousarray(cost, '<f8').tobytes(),
                                     np.ascontiguousarray(nav.node_pos, '<f8').reshape(-1, 3).tobytes())
        except (ValueError, TypeError):
            d['graph'] = None                  # a graph the core does not answer as the reference: the reference
    return None if d['graph'] is None else (d['graph'], formula)


def closest_between(nav: navdata.NavData, ia: int | None,
                    ib: int | None) -> tuple[int | None, int | None, float] | None:
    """The closest pair of nodes of two islands (horizontal metres); kept per island pair (round E3o)."""
    memo = _per_nav(nav)['closest']
    key = (ia, ib)
    if key not in memo:
        memo[key] = closest_between_reference(nav, ia, ib)
    return memo[key]


def closest_between_reference(nav: navdata.NavData, ia: int | None,
                              ib: int | None) -> tuple[int | None, int | None, float] | None:
    """closest_between computed (the reference)."""
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
    """The route from start to end (round E3o: the search in the Rust core when built; route_reference's result)."""
    pg = _path_graph(nav)
    if pg is None:
        return route_reference(nav, start, end, closest_between)
    start, end = np.asarray(start, float), np.asarray(end, float)
    r = Route(False, straight=float(np.linalg.norm(end - start)))
    r.start_node, r.start_join = join(nav, start)
    r.end_node, r.end_join = join(nav, end)
    if r.start_node is None or r.end_node is None:
        which = 'start' if r.start_node is None else 'end'
        r.reason = f'no route node within {JOIN_XZ:.0f} m of the {which} point (nearest {min(r.start_join if which == "start" else r.end_join, 9999):.0f} m)'
        return r
    r.start_island, r.end_island = int(nav.node_island[r.start_node]), int(nav.node_island[r.end_node])
    graph, formula = pg
    r.expanded, nodes, cost = graph.route(r.start_node, r.end_node, formula)
    if not nodes:
        _not_found(nav, r)
        return r
    pos = nav.node_pos
    r.nodes = nodes
    r.cost = cost
    r.length = float(np.sum(np.linalg.norm(np.diff(pos[r.nodes], axis=0), axis=1))) if len(r.nodes) > 1 else 0.0
    r.found = True
    return r


def _not_found(nav: navdata.NavData, r: Route, closest: Any = None) -> None:
    if r.start_island != r.end_island:
        r.gap = (closest or closest_between)(nav, r.start_island, r.end_island)
        r.reason = (f'the points are on different islands ({r.start_island} and {r.end_island}); they come closest '
                    f'{r.gap[2]:.1f} m apart' if r.gap else f'the points are on different islands ({r.start_island}, {r.end_island})')
    else:
        r.reason = 'same island, but no route along the edges\' directions (one-way edges)'


def route_reference(nav: navdata.NavData, start: Sequence[float] | np.ndarray,
                    end: Sequence[float] | np.ndarray, closest: Any = None) -> Route:
    """The search in Python (the reference; closest: the islands' closest pair, closest_between_reference unless
    given)."""
    start, end = np.asarray(start, float), np.asarray(end, float)
    r = Route(False, straight=float(np.linalg.norm(end - start)))
    r.start_node, r.start_join = join(nav, start)
    r.end_node, r.end_join = join(nav, end)
    if r.start_node is None or r.end_node is None:
        which = 'start' if r.start_node is None else 'end'
        r.reason = f'no route node within {JOIN_XZ:.0f} m of the {which} point (nearest {min(r.start_join if which == "start" else r.end_join, 9999):.0f} m)'
        return r
    r.start_island, r.end_island = int(nav.node_island[r.start_node]), int(nav.node_island[r.end_node])
    start_idx, nbr, cost = _adjacency(nav)
    pos = nav.node_pos
    goal = r.end_node
    best = {r.start_node: 0.0}
    came: dict[int, int] = {}
    heap = [(float(np.linalg.norm(pos[r.start_node] - pos[goal])), 0.0, r.start_node)]
    done: set[int] = set()
    while heap:
        _, g, n = heapq.heappop(heap)
        if n in done:
            continue
        done.add(n)
        if n == goal:
            break
        for k in range(start_idx[n], start_idx[n + 1]):
            m = int(nbr[k])
            ng = g + float(cost[k])
            if ng < best.get(m, np.inf):
                best[m] = ng
                came[m] = n
                heapq.heappush(heap, (ng + float(np.linalg.norm(pos[m] - pos[goal])), ng, m))
    r.expanded = len(done)
    if goal not in done:
        _not_found(nav, r, closest or closest_between_reference)
        return r
    chain = [goal]
    while chain[-1] != r.start_node:
        chain.append(came[chain[-1]])
    r.nodes = chain[::-1]
    r.cost = best[goal]
    r.length = float(np.sum(np.linalg.norm(np.diff(pos[r.nodes], axis=0), axis=1))) if len(r.nodes) > 1 else 0.0
    r.found = True
    return r
