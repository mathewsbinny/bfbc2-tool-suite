"""A Kynapse path-data file (pathdata.hpd) as the suite's 3D tools draw it: walkable cells, area boundaries, the
route graph and its islands. Read with NavKit's byte-exact reader (bfbc2-auto-navkit/tools: hpdtool, kymesh,
navdiff); the format is in bfbc2-auto-navkit/research/KYNAPSE_SPEC.md.

    nav = load(path, heightfield=None, cache_dir=None)
    nav.areas[k]                 NavArea: sector, tile, layer, pitch, boundary pieces, island
    nav.cell_xz, nav.cell_y      every walkable cell on the 0.8 m grid (navdiff's cells), its ground height
    nav.cell_area                the area each cell belongs to
    nav.node_pos, nav.edges      route graph: node positions (ground + 0.95 m, as stored), edges as index pairs
    nav.edge_cost                each edge's cost in metres, as stored
    nav.node_island, nav.islands islands (connected parts of the graph), largest first

Cells come from navdiff.area_cells (samples inside an area's boundary, height from the nearest boundary point);
where a cell lies on terrain, the terrain height field's height is used instead. The whole file is cached per file
(size and date) when cache_dir is given.

The areas, cells and the route graph's union-finds are made in the suite's Rust core (round E3o: navkit_navdata_areas,
navkit_mesh_cells, navkit_union_roots); their Python twins were retired on 2026-11-18 (round R1; the answers recorded
in tests/navdata_golden.json). The module is required.
"""
import contextlib
import hashlib
import json
import os
import pickle
import sys
from collections import Counter
from dataclasses import dataclass, field
from typing import Any

import numpy as np

from . import atomicio, caches

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools')
import hpdtool  # noqa: E402
import navdiff  # noqa: E402

GRID = navdiff.GRID
Y_OFFSET = navdiff.Y_OFFSET                 # boundaries and nodes are stored 0.95 m above the ground
DETAILED = 0.5                              # areas with a smaller pitch are detailed zones (DICE: 0.2 m)
CACHE_VERSION = 3                           # 3: ladder links and ladder ground
LADDER_DY = 2.5                             # a ladder link: a route edge climbing more than this (m) ...
LADDER_SLOPE = 1.5                          # ... and more than this many metres per metre across - no walkable link is
                                            # that steep (DICE's files have none; NavKit's ladder links, ladders.py)


@dataclass
class NavArea:
    sector: int
    index: int                              # within its sector
    tile: tuple[int, int]
    layer: int
    pitch: float
    min_y: float
    max_y: float
    outline: list[np.ndarray] = field(default_factory=list)      # polylines (ground + 0.95 m)
    portals: list[tuple] = field(default_factory=list)           # (p0, p1, target area index in the sector)
    borders: list[tuple] = field(default_factory=list)           # (p0, p1)
    island: int = -1                                             # index into NavData.islands
    cells: int = 0
    ladder: bool = False                                         # ground reached only through a ladder link

    @property
    def detailed(self) -> bool:
        return self.pitch < DETAILED


@dataclass
class Island:
    group: int                              # graph group id (a node id of the group)
    nodes: int
    cells: int
    centre: np.ndarray


@dataclass
class NavData:
    path: str
    areas: list[NavArea]
    cell_xz: np.ndarray                     # (n, 2) cell centres
    cell_y: np.ndarray                      # (n,) ground height
    cell_area: np.ndarray                   # (n,) index into areas
    node_pos: np.ndarray                    # (m, 3)
    node_sector: np.ndarray                 # (m,)
    edges: np.ndarray                       # (k, 2) node indices, source -> destination
    edge_cost: np.ndarray                   # (k,) metres, as stored (the tag's floor(distance * 16) / 16)
    node_island: np.ndarray                 # (m,) index into islands
    islands: list[Island]
    ladder_edge: np.ndarray | None = None   # (k,) bool: the edge is a ladder link (LADDER_DY / LADDER_SLOPE)

    @property
    def sectors(self) -> list[int]:
        return sorted({a.sector for a in self.areas})


def _terrain_id(heightfield: Any) -> str:
    """A fingerprint of a height field's content (its bounds and samples), kept on the object."""
    if heightfield is None:
        return 'est'
    fp = getattr(heightfield, '_navdata_id', None)
    if fp is None:
        h = hashlib.sha1(np.asarray(heightfield.min, np.float64).tobytes() + np.asarray(heightfield.max, np.float64).tobytes())
        h.update(np.ascontiguousarray(heightfield.raw).tobytes())
        fp = 'hf-' + h.hexdigest()[:16]
        with contextlib.suppress(AttributeError):
            heightfield._navdata_id = fp
    return fp


_CODE_ID: str | None = None


def _code_id() -> str:
    """A digest of the code that makes the cached objects: this module and the navigation tools it builds them with
    (navdiff, kymesh; found, not imported, so the digest is the same before and after they load) - a change to any of
    them is a new key even when CACHE_VERSION was not raised (suite deep scan 2026-11-15, S2)."""
    global _CODE_ID
    if _CODE_ID is None:
        import importlib.util
        h = hashlib.sha1()
        files: list[str | None] = [__file__]
        for m in ('navdiff', 'kymesh'):
            try:
                spec = importlib.util.find_spec(m)
            except (ImportError, ValueError):
                spec = None
            files.append(spec.origin if spec else None)
        for f in files:
            if f and os.path.isfile(f):
                with open(f, 'rb') as fh:
                    h.update(fh.read())
        _CODE_ID = h.hexdigest()[:12]
    return _CODE_ID


def _key(path: str, heightfield: Any = None) -> str:
    st = os.stat(path)
    hf = _terrain_id(heightfield)
    return hashlib.sha1(f'{CACHE_VERSION}|{_code_id()}|{os.path.abspath(path).lower()}|{st.st_size}|{st.st_mtime_ns}|{hf}'
                        .encode()).hexdigest()[:20]


def _ground(xz: np.ndarray, est: np.ndarray, heightfield: Any) -> np.ndarray:
    """Cell heights: the terrain height field's where the cell lies on terrain (within 1.5 m of navdiff's
    estimate), else the estimate."""
    if heightfield is None or not len(xz):
        return est
    t = heightfield.height(xz[:, 0], xz[:, 1])
    return np.where(np.abs(t - est) < 1.5, t, est)


def _native() -> Any:
    """The Rust core (round E3o's navkit_mesh_cells, navkit_navdata_areas, navkit_union_roots), or
    bfbc2core.native.NativeMissing saying how to get it."""
    from . import native
    return native.require('navdata (path data for the 3D tools)', 'navkit_navdata_areas')


def _meshes(f: Any) -> list[bytes]:
    return [bytes(s.mesh) for s in f.sectors]


def _mesh_cells(f: Any) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """(xz, est, area) of every area of the file in the Rust core (round E3o: the meshes parsed, each area's boundary
    sampled by navdiff.area_cells' rule, cell after cell); ValueError for a mesh it cannot read or a boundary value
    that is not finite (none in DICE's or NavKit's path data)."""
    got = _native().navkit_mesh_cells(_meshes(f), navdiff.GRID, navdiff.Y_OFFSET)
    if got is None:
        raise ValueError('navdata: a sector mesh the Rust core cannot read, or a boundary value that is not finite '
                         '(kymesh.parse_mesh_reference reads the record: check it with hpdview / kymesh)')
    ij, ys, area = got
    xz = np.frombuffer(ij, '<i8').reshape(-1, 2).astype(np.float64) * GRID
    return xz, np.frombuffer(ys, '<f8').astype(np.float64), np.frombuffer(area, '<i4').astype(np.int32)


def _cells(f: Any, heightfield: Any, cache_dir: str | None,
           path: str) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    xz: Any                                     # cached cell centres, or a list of arrays joined into one
    if cache_dir:
        cp = os.path.join(cache_dir, _key(path) + '.npz')
        if os.path.isfile(cp):
            try:
                with np.load(cp) as z:                           # closed again: no lock left on the file
                    xz, area, est = z['xz'], z['area'], z['est']
                caches.touch(cp)
                return xz, _ground(xz, est, heightfield), area, est
            except Exception:
                pass                                             # a damaged cache entry is rebuilt
    xz, est, area = _mesh_cells(f)
    y = _ground(xz, est, heightfield)
    if cache_dir:
        os.makedirs(cache_dir, exist_ok=True)
        cp = os.path.join(cache_dir, _key(path) + '.npz')
        tmp = f'{cp}.{os.getpid()}.tmp'                         # per process: two tools writing at once do not collide
        with open(tmp, 'wb') as fh:                      # a file object: numpy adds no .npz of its own
            np.savez(fh, xz=xz, y=y, area=area, est=est)
        atomicio.replace(tmp, cp)                        # an interrupted write leaves no half file
    return xz, y, area, est


def _areas(f: Any) -> list[NavArea]:
    """The file's NavAreas from the Rust core's flat arrays (round E3o: navkit_navdata_areas; each outline line of 2+
    points as float32, portal and border ends as float64, each a view of one array of the file); ValueError for a mesh
    it cannot read."""
    got = _native().navkit_navdata_areas(_meshes(f))
    if got is None:
        raise ValueError('navdata: a sector mesh the Rust core cannot read (a short record, a NaN: '
                         'kymesh.parse_mesh_reference reads the record - check it with hpdview / kymesh)')
    ints, floats, points, line_start, area_lines, portals, targets, area_portals, borders, area_borders = got
    head = np.frombuffer(ints, '<i8').reshape(-1, 5).tolist()
    vals = np.frombuffer(floats, '<f8').reshape(-1, 3).tolist()
    pts = np.frombuffer(points, '<f4').reshape(-1, 3).astype(np.float32)          # one writable array
    ls = np.frombuffer(line_start, '<i8').tolist()
    al = np.frombuffer(area_lines, '<i8').tolist()
    pp = np.frombuffer(portals, '<f8').reshape(-1, 2, 3).astype(np.float64)
    tg = np.frombuffer(targets, '<i8').tolist()
    ap = np.frombuffer(area_portals, '<i8').tolist()
    bb = np.frombuffer(borders, '<f8').reshape(-1, 2, 3).astype(np.float64)
    ab = np.frombuffer(area_borders, '<i8').tolist()
    sid = [s.id for s in f.sectors]
    out = []
    for k, ((si, n, tx, tz, layer), (pitch, min_y, max_y)) in enumerate(zip(head, vals)):
        out.append(NavArea(sid[si], n, (tx, tz), layer, pitch, min_y, max_y,
                           [pts[ls[j]:ls[j + 1]] for j in range(al[k], al[k + 1])],
                           [(pp[j, 0], pp[j, 1], tg[j]) for j in range(ap[k], ap[k + 1])],
                           [(bb[j, 0], bb[j, 1]) for j in range(ab[k], ab[k + 1])]))
    return out


def _union_roots(n: int, edges: np.ndarray, skip: np.ndarray | None = None) -> np.ndarray:
    """Each node's root after joining the ends of every edge (but those marked in skip) in order, by a union-find with
    path halving (parent[root of a] = root of b; then find(k) for every k in order), in the Rust core (round E3o).
    ValueError for a node out of range."""
    if not n:
        if len(edges):
            raise ValueError('navdata: route edges without nodes')
        return np.zeros(0, np.int64)
    e = np.ascontiguousarray(edges, dtype='<i8')
    sk = None if skip is None else np.ascontiguousarray(skip, dtype=bool).view(np.uint8).tobytes()
    return np.frombuffer(_native().navkit_union_roots(n, e.tobytes(), sk), '<i8').astype(np.int64)


def load(path: str, heightfield: Any = None, cache_dir: str | None = None) -> NavData:
    """The file as NavData. With cache_dir, the whole result is kept there per file (and per height source), so
    opening a level again takes about a second."""
    if cache_dir:
        pk = os.path.join(cache_dir, _key(path, heightfield) + '.navdata.pkl')
        if os.path.isfile(pk):
            try:
                with open(pk, 'rb') as fh:
                    got = pickle.load(fh)
                caches.touch(pk)                             # used: the pruning deletes the least recently used
                return got
            except Exception:
                pass                                         # an unreadable cache entry is rebuilt
        nav = _load(path, heightfield, cache_dir)
        os.makedirs(cache_dir, exist_ok=True)
        tmp = f'{pk}.{os.getpid()}.tmp'
        with open(tmp, 'wb') as fh:
            pickle.dump(nav, fh, protocol=pickle.HIGHEST_PROTOCOL)
        atomicio.replace(tmp, pk)
        caches.prune_project_cache(cache_dir)                # a project cache (work\navstudio\cache, ...) to its cap
        return nav
    return _load(path, heightfield, None)


def _load(path: str, heightfield: Any, cache_dir: str | None) -> NavData:
    f = hpdtool.parse(open(path, 'rb').read())
    areas = _areas(f)
    xz, y, cell_area, _ = _cells(f, heightfield, cache_dir, path)
    counts = np.bincount(cell_area, minlength=len(areas)) if len(cell_area) else np.zeros(len(areas), int)
    for a, cnt in zip(areas, counts):
        a.cells = int(cnt)

    # route graph: border nodes repeat in both sectors with the same id - one node per id
    index: dict[int, int]
    pos: Any                                    # a list of positions, then an array
    sec: list[int]
    edges: Any                                  # a list of index pairs, then an array
    costs: list[float]
    index, pos, sec, edges, costs = {}, [], [], [], []
    for s in f.sectors:
        local = []
        for nd in s.nodes:
            if nd.id not in index:
                index[nd.id] = len(pos)
                pos.append(nd.pos)
                sec.append(s.id)
            local.append(index[nd.id])
        for e in s.edges:
            edges.append((local[e.src], local[e.dst]))
            costs.append((e.tag & 0x7FFFFFFF) / 16.0)
    pos = np.array(pos, dtype=np.float64).reshape(-1, 3)
    edges = np.array(edges, dtype=np.int64).reshape(-1, 2)
    roots = _union_roots(len(pos), edges)
    # ladder links, and the walking parts of each island (its components without them): ground on any walking
    # part but the island's largest is reached through a ladder - ladder ground
    if len(edges):
        d = pos[edges[:, 1]] - pos[edges[:, 0]]
        dy, h = np.abs(d[:, 1]), np.hypot(d[:, 0], d[:, 2])
        ladder_edge = (dy > LADDER_DY) & (dy > LADDER_SLOPE * h)
    else:
        ladder_edge = np.zeros(0, bool)
    walk_root = _union_roots(len(pos), edges, ladder_edge)
    base: dict[Any, Any] = {}
    if ladder_edge.any():
        wsize = Counter(walk_root.tolist())
        for k in range(len(pos)):
            r, w = roots[k], walk_root[k]
            if r not in base or wsize[w] > wsize[base[r]]:
                base[r] = w
    sizes = Counter(roots.tolist())
    order = [r for r, _ in sizes.most_common()]
    rank = {r: k for k, r in enumerate(order)}
    node_island = np.array([rank[r] for r in roots], dtype=np.int32)

    # each area's island: the graph node of its sector nearest to the area's cells (an area has its own node,
    # near its centre; tiny areas without one take their neighbour's)
    sec_nodes: dict[int, Any] = {}
    for k, sid in enumerate(sec):
        sec_nodes.setdefault(sid, []).append(k)
    sec_nodes = {s: np.array(v) for s, v in sec_nodes.items()}
    centre = np.zeros((len(areas), 3))
    if len(cell_area):
        for d in range(2):
            centre[:, [0, 2][d]] = np.bincount(cell_area, weights=xz[:, d], minlength=len(areas)) / np.maximum(counts, 1)
        centre[:, 1] = np.bincount(cell_area, weights=y, minlength=len(areas)) / np.maximum(counts, 1)
    for k, a in enumerate(areas):
        cand = sec_nodes.get(a.sector)
        if cand is None or not len(cand):
            continue
        p = pos[cand]
        d = np.hypot(p[:, 0] - centre[k, 0], p[:, 2] - centre[k, 2]) + 3.0 * np.abs(p[:, 1] - Y_OFFSET - centre[k, 1])
        kn = cand[int(np.argmin(d))]
        a.island = int(node_island[kn])
        a.ladder = bool(base) and walk_root[kn] != base[roots[kn]]
    island_cells: Counter[int] = Counter()
    for a in areas:
        island_cells[a.island] += a.cells
    islands = []
    for k, r in enumerate(order):
        sel = node_island == k
        islands.append(Island(int(r), int(sel.sum()), int(island_cells.get(k, 0)), pos[sel].mean(axis=0)))
    return NavData(path, areas, xz, y, cell_area, pos, np.array(sec), edges, np.array(costs, dtype=np.float64),
                   node_island, islands, ladder_edge)


def compare_cells(cand: NavData, ref: NavData) -> tuple[np.ndarray, np.ndarray]:
    """Per cell of each file: True where the other file has the same ground (navdiff's rule: the same 0.8 m
    cell; one floor on each side is the same ground, stacked floors pair by height within 4 m)."""
    def keyed(nav: NavData) -> np.ndarray:
        ij = np.round(nav.cell_xz / GRID).astype(np.int64)
        return (ij[:, 0] + 50_000) * 100_000 + (ij[:, 1] + 50_000)

    kc, kr = keyed(cand), keyed(ref)
    yc, yr = np.asarray(cand.cell_y, np.float64), np.asarray(ref.cell_y, np.float64)

    def same(keys: np.ndarray, y: np.ndarray, okeys: np.ndarray, oy: np.ndarray) -> np.ndarray:
        """For each (key, y): does the other file have the key, and (one floor each) or a floor within STACK_TOL?"""
        uk, counts = np.unique(keys, return_counts=True)
        mine_n = counts[np.searchsorted(uk, keys)]
        order = np.lexsort((oy, okeys))
        sk, sy = okeys[order], oy[order]
        lo, hi = np.searchsorted(sk, keys, 'left'), np.searchsorted(sk, keys, 'right')
        other_n = hi - lo
        # the other file's nearest floor at the same key: composite (key, height) values, sorted - the
        # neighbours of (key, y) in that order are the nearest floors below and above
        comp_o = sk.astype(np.float64) * 1e5 + (sy + 2e4)
        comp_m = keys.astype(np.float64) * 1e5 + (y + 2e4)
        p = np.searchsorted(comp_o, comp_m)
        best = np.full(len(keys), np.inf)
        for q in (p - 1, p):
            ok = (q >= 0) & (q < len(sk))
            qq = np.clip(q, 0, max(len(sk) - 1, 0))
            hit = ok & (sk[qq] == keys) if len(sk) else np.zeros(len(keys), bool)
            best = np.where(hit, np.minimum(best, np.abs(sy[qq] - y) if len(sk) else best), best)
        return (other_n > 0) & (((other_n == 1) & (mine_n == 1)) | (best <= navdiff.STACK_TOL))
    return same(kc, yc, kr, yr), same(kr, yr, kc, yc)


def summary(nav: NavData) -> dict[str, int]:
    return {'areas': len(nav.areas), 'detailed areas': sum(a.detailed for a in nav.areas), 'cells': int(len(nav.cell_xz)),
            'nodes': int(len(nav.node_pos)), 'edges': int(len(nav.edges)), 'islands': len(nav.islands),
            'large islands (25+ nodes)': sum(i.nodes >= 25 for i in nav.islands), 'sectors': len(nav.sectors),
            'ladder links': int(nav.ladder_edge.sum() // 2) if nav.ladder_edge is not None else 0,
            'ladder ground cells': sum(a.cells for a in nav.areas if a.ladder)}


if __name__ == '__main__':
    nav = load(sys.argv[1])
    print(json.dumps(summary(nav), indent=1))
