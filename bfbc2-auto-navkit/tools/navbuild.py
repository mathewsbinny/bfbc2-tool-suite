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

Detail features D and F are not generated (counts 0, allowed by the format); detailed zones (pitch 0.2)
are generated at the normal pitch.

Usage: navbuild.py <samples.bin> <forbid_tris.npy> <XOffset> <ZOffset> <out.hpd> [--step 0.28]
                   [--dynamic dynamic_tris.npy] [--high samples_high_capsule.bin] [--narrow samples_narrow.bin]
                   [--stairs stair_tris.npy] [--exclude exclude_tris.npy] [--exclude-foot exclude_footprint_tris.npy]
"""
import math
import os
import struct
import sys
import time
from collections import defaultdict, deque

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hpdtool  # noqa: E402
import kymesh  # noqa: E402
import navsample  # noqa: E402
import navwalk  # noqa: E402

PITCH = 0.8
SECTOR_SAMPLES = 125            # 100 m / 0.8 m
WINDOW = range(-1, 127)         # sample indices of a sector window (local)
TILE = 50                       # samples per tile
TILES = 3
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
BORDER_NODE_SPACING = 10.0
MAX_WATER_DEPTH = 1.5           # ground deeper under water is not walkable (DICE's data: 99.9 % of its ground under
                                # water is within 1.45 m, median over 11 levels; navwater.py)


def water_tops(s, vertices, triangles):
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


def outside_bounds(s, bounds):
    """Surfaces whose column lies outside bounds (minX, minZ, maxX, maxZ) - bool per surface (CLIP_TO_REFERENCE)."""
    col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start[:s.nx * s.nz + 1]))
    x, z = s.min_x + (col % s.nx) * s.pitch, s.min_z + (col // s.nx) * s.pitch
    e = 1e-3                                             # edges inclusive (float noise of the sample grid)
    return (x < bounds[0] - e) | (x > bounds[2] + e) | (z < bounds[1] - e) | (z > bounds[3] + e)


def deep_water(s, water, limit=MAX_WATER_DEPTH):
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


class Level:
    """Kept walkable surfaces on the global sample grid, with the step-rule connectivity."""

    def __init__(self, s, keep, step, gx=None, gz=None, stair=None):
        self.s, self.keep, self.step = s, keep, step
        self.stair = stair if stair is not None else np.zeros(len(s.y), bool)
        self.y = s.y.astype(float)
        ny = np.maximum(s.normal[:, 1], 0.05)
        self.gx = gx if gx is not None else -s.normal[:, 0] / ny
        self.gz = gz if gz is not None else -s.normal[:, 2] / ny
        self.col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
        self.terrain = (s.hit[:, 0] == navwalk.TERRAIN_SHAPE_VTABLE) if s.hit is not None else np.ones(len(s.y), bool)

    def surfaces(self, gi, gj):
        if not (0 <= gi < self.s.nx and 0 <= gj < self.s.nz):
            return range(0)
        c = gj * self.s.nx + gi
        return [k for k in range(self.s.start[c], self.s.start[c + 1]) if self.keep[k]]

    def connected(self, k, n, di, dj, rule=None):
        """Surface n (direction di, dj from k) links to k by rule ('slope', 'raw', 'terrain'; LINK_RULE if None)."""
        rule = rule or LINK_RULE
        if rule == 'slope' or (rule == 'terrain' and not (self.terrain[k] and self.terrain[n])):
            return navwalk.step_fits(self.y[n] - self.y[k], (self.gx[k], self.gz[k]), (self.gx[n], self.gz[n]),
                                     self.stair[k], self.stair[n], di, dj, PITCH, self.step)
        y = self.y
        if not (di and dj):
            return abs(y[n] - y[k]) <= LINK_STEP
        # diagonal: through either side column, over any surface there (walkable or not)
        c = self.col_of[k]
        gi, gj = c % self.s.nx, c // self.s.nx
        for a, b in ((gi + di, gj), (gi, gj + dj)):
            if 0 <= a < self.s.nx and 0 <= b < self.s.nz:
                m0 = b * self.s.nx + a
                for m in range(self.s.start[m0], self.s.start[m0 + 1]):
                    if abs(y[m] - y[k]) <= LINK_STEP and abs(y[n] - y[m]) <= LINK_STEP:
                        return True
        return False

    def neighbour(self, k, gi, gj, di, dj, prefer=None, rule=None):
        """The kept surface in column (gi+di, gj+dj) connected to surface k, or None. With prefer (a set of
        surfaces, e.g. those of k's own area), a connected surface in that set is returned first."""
        best, bd = None, 9e9
        for n in self.surfaces(gi + di, gj + dj):
            if self.connected(k, n, di, dj, rule):
                if prefer is not None and n in prefer:
                    return n
                d = abs(self.y[n] - self.y[k])
                if d < bd:
                    best, bd = n, d
        return best


def sector_origin_index(level, X, Y, xoff, zoff):
    """Global sample index of the sector's local sample (0, 0)."""
    s = level.s
    ox, oz = (X + xoff) * 100.0, (Y + zoff) * 100.0
    return int(round((ox - s.min_x) / PITCH)), int(round((oz - s.min_z) / PITCH)), ox, oz


def build_areas(level, gi0, gj0, start=(-1, -1)):
    """Areas of one sector: list of dicts {tile: (tx, tz), cells: [(i, j, k)]} (local i, j). start: the local sample
    index where the tiles begin on x and z (-1, or 0 on a side with no neighbouring sector - EDGE_SECTOR_TILES)."""
    cells_by_tile = defaultdict(list)
    for j in WINDOW:
        for i in WINDOW:
            for k in level.surfaces(gi0 + i, gj0 + j):
                tx = min(max(i - start[0], 0) // TILE, TILES - 1)
                tz = min(max(j - start[1], 0) // TILE, TILES - 1)
                cells_by_tile[(tx, tz)].append((i, j, k))
    areas = []
    for tile in sorted(cells_by_tile):
        members = {k: (i, j) for i, j, k in cells_by_tile[tile]}
        for comp in components(level, members, gi0, gj0):
            for piece in (bisect(level, comp, gi0, gj0) if BISECT_AREAS else [comp]):
                areas.append({'tile': tile, 'cells': piece})
    return areas


def components(level, members, gi0, gj0):
    """8-connected components (AREA_LINK rule) of the given surfaces {k: (i, j)}. An area is single-layer in plan: a
    surface whose column the component already holds (a second floor reached round a corner) starts its own."""
    seen, out = set(), []
    for k0 in members:
        if k0 in seen:
            continue
        comp, q = [], deque([k0])
        seen.add(k0)
        cols = {members[k0]}
        while q:
            k = q.popleft()
            i, j = members[k]
            comp.append((i, j, k))
            for di, dj in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)):
                n = level.neighbour(k, gi0 + i, gj0 + j, di, dj, rule=AREA_LINK)
                if n is not None and n in members and n not in seen and members[n] == (i + di, j + dj) \
                        and members[n] not in cols:
                    seen.add(n)
                    cols.add(members[n])
                    q.append(n)
        out.append(comp)
    return out


def bisect(level, comp, gi0, gj0):
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


def clip_bridges(bsegs, bridges):
    """Outline of overlapping 45-degree bridges is the boundary of their union: where edges of two bridges
    cross (one cell with diagonal-only neighbours on both sides of an empty cell), split them at the crossing
    and drop the pieces lying inside the other bridge's strip (half-width 1/sqrt 2 sample about the diagonal)."""
    def inside(px, pz, b):
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


def trace(level, sector, areas, area_of, gi0, gj0, ox, oz):
    """Boundary edges of every area -> merged runs. Returns per area: portals, borders, outlines. One sector on its
    own: one-way portal pieces are kept ('keep') or become outline (otherwise) - build() decides them level-wide
    ('restore', restore_one_way)."""
    per_area, one_way = trace_edges(level, areas, area_of, gi0, gj0)
    drop_one_way(per_area, one_way, keep=set(range(len(one_way))) if ONE_WAY_PORTALS == 'keep' else set())
    return trace_emit(level, areas, per_area, gi0, gj0, ox, oz)


def trace_edges(level, areas, area_of, gi0, gj0):
    """Boundary edges of every area: (per area {(kind, target, axis, line, dir): [(along, y)]}, one-way candidates).
    Portals are symmetric (SYMMETRIC_PORTALS): every portal edge A -> B also stands as B -> A, replacing an outline
    edge B had there - neighbour() picks the nearest surface in height, so with two surfaces in a column the two
    sides could disagree and leave a one-way portal (22 % of them on sp_004_b before). Where the target's side is
    its own interior (a second surface of the target in the source's column) the piece cannot be mirrored: it is a
    one-way candidate (source area, target area, key, along) for drop_one_way."""
    per_area = []
    one_way = []
    mirror = defaultdict(dict)       # area -> {(source area, axis, line, dir, along): y of its own cell there}
    side_kind = []                   # per area: {(axis, line, dir, along): (kind 'I' interior / 'B' / 'C' / 'E', target)}
    for ai, area in enumerate(areas):
        edges = defaultdict(list)    # (kind, target, axis, line, dir) -> [(along, y0, y1)]
        sides = {}
        own = {c[2] for c in area['cells']}
        for i, j, k in area['cells']:
            yk = level.y[k] + Y_OFFSET
            for di, dj in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                side = ('x', i + di * 0.5, di, j) if di else ('z', j + dj * 0.5, dj, i)
                # a side is interior when any connected surface across it is in this area (two connected
                # surfaces in one column - stair edge, split-off layer - must not leave a stray portal inside)
                n = level.neighbour(k, gi0 + i, gj0 + j, di, dj, prefer=own)
                ni, nj = i + di, j + dj
                if n is not None and ni in WINDOW and nj in WINDOW:
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
                    mirror[target][(ai, axis, line, -(di or dj), along)] = level.y[n] + Y_OFFSET
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


def drop_one_way(per_area, one_way, keep):
    """One-way candidates (trace_edges) not in keep (indices) become outline on their source area."""
    for n, (src, t, key, along) in enumerate(one_way):
        if n in keep:
            continue
        se = per_area[src]
        ys = [r for r in se.get(key, []) if r[0] == along]
        se[key] = [r for r in se.get(key, []) if r[0] != along]
        se[('E', None, key[2], key[3], key[4])].extend(ys)


def restore_one_way(sectors, shared):
    """ONE_WAY_PORTALS 'restore': which one-way candidates to keep. sectors: {sector: (per_area, one_way)} from
    trace_edges; shared: pairs ((sector, area), (sector, area)) joined across sectors (one surface in both - the
    sectors' two-sample overlap). Areas are joined through every portal piece that is not a candidate and through
    shared; then, pair by pair, a pair's candidates are kept only when its two areas are not joined yet (keeping them
    joins them). Returns {sector: set of kept candidate indices}."""
    parent = {}

    def find(x):
        parent.setdefault(x, x)
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a, b):
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
        for n, (src, t, key, along) in enumerate(one_way):
            by_pair[(sec, src, t)].append(n)
    for (sec, src, t), ns in sorted(by_pair.items()):
        if find((sec, src)) != find((sec, t)):
            keep[sec].update(ns)
            union((sec, src), (sec, t))
    return keep


def trace_emit(level, areas, per_area, gi0, gj0, ox, oz):
    """Per area (portals, borders, outline) from its boundary edges: runs merged, 45-degree bridges."""
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
        drop = set()
        for i, j, di, dj in bridges:
            # unit outline edges around the corner point (i + di/2, j + dj/2)
            drop.update({('x', i + di * 0.5, j), ('z', j + dj * 0.5, i), ('x', i + di * 0.5, j + dj), ('z', j + dj * 0.5, i + di)})
        portals, borders, outline = [], [], []
        bsegs = []                                          # (bridge, (ax, az, ya), (bx, bz, yb)) local units
        for bi, (i, j, di, dj) in enumerate(bridges):
            cx, cz = i + di * 0.5, j + dj * 0.5             # shared corner (local sample units)
            ya = level.y[next(c[2] for c in area['cells'] if (c[0], c[1]) == (i, j))] + Y_OFFSET
            yb = level.y[next(c[2] for c in area['cells'] if (c[0], c[1]) == (i + di, j + dj))] + Y_OFFSET
            for (ax, az), (bx, bz) in (((cx - di, cz), (cx, cz + dj)), ((cx, cz - dj), (cx + di, cz))):
                bsegs.append((bi, (ax, az, ya), (bx, bz, yb)))
        for _bi, (ax, az, ya), (bx, bz, yb) in clip_bridges(bsegs, bridges):
            outline.append([(ox + ax * PITCH, ya, oz + az * PITCH), (ox + bx * PITCH, yb, oz + bz * PITCH)])
        for key in list(edges):
            if key[0] == 'E':
                axis, line = key[2], key[3]
                edges[key] = [r for r in edges[key] if (axis, line, r[0]) not in drop]
        for (kind, target, axis, line, d), runs in edges.items():
            runs.sort()
            start = 0
            for r in range(1, len(runs) + 1):
                if r == len(runs) or runs[r][0] != runs[r - 1][0] + 1:
                    a, b = runs[start][0] - 0.5, runs[r - 1][0] + 0.5
                    ya, yb = runs[start][1], runs[r - 1][1]
                    if axis == 'x':
                        p0 = (ox + line * PITCH, ya, oz + a * PITCH)
                        p1 = (ox + line * PITCH, yb, oz + b * PITCH)
                    else:
                        p0 = (ox + a * PITCH, ya, oz + line * PITCH)
                        p1 = (ox + b * PITCH, yb, oz + line * PITCH)
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


def build(samples_path, tris_path, xoff, zoff, out_path, step=0.28, log=print, dynamic_path=None, high_path=None,
          narrow_path=None, stairs_path=None, exclude_path=None, exclude_foot_path=None, seeds=None, edits=None,
          water=None, max_water_depth=MAX_WATER_DEPTH, bounds=None):
    """edits: the operations of a navigation side file (navedits.py applies the walkability ones).
    water: the level's water mesh (vertices, triangles; core/bfbc2core/water.py): ground more than max_water_depth
    under it is not walkable, as in DICE's data (hand edits forcing ground walkable still win).
    bounds: (minX, minZ, maxX, maxZ) of the reference path data (CLIP_TO_REFERENCE), or None."""
    t0 = time.time()
    f, _ = navsample.load(samples_path)
    s = navwalk.coarse(f) if abs(f.pitch - PITCH) > 1e-3 else f
    hand = {}
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
    if bounds is not None and CLIP_TO_REFERENCE:
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
                                    extra_seeds=seeds, soldier_points=soldiers, **hand)
    level = Level(s, keep, step, gx, gz, stair)
    log(f'{len(s.y)} surfaces, {int(keep.sum())} walkable kept ({time.time() - t0:.0f} s)')

    # Sectors that contain kept surfaces.
    sectors = set()
    for k in np.nonzero(keep)[0]:
        c = level.col_of[k]
        x, z = s.min_x + (c % s.nx) * PITCH, s.min_z + (c // s.nx) * PITCH
        sectors.add((int(math.floor(x / 100.0)) - xoff, int(math.floor(z / 100.0)) - zoff))
    sectors = sorted(sectors)
    log(f'{len(sectors)} sectors')

    # Pass 1: areas per sector, and which area each surface belongs to.
    info = {}
    have = set(sectors)
    min_x_sector, min_z_sector = min(X for X, _ in sectors), min(Y for _, Y in sectors)
    for X, Y in sectors:
        gi0, gj0, ox, oz = sector_origin_index(level, X, Y, xoff, zoff)
        if EDGE_SECTOR_RULE == 'grid_min':
            edge = (X == min_x_sector, Y == min_z_sector)
        else:
            edge = ((X - 1, Y) not in have, (X, Y - 1) not in have)
        start = tuple(0 if EDGE_SECTOR_TILES and e else -1 for e in edge)
        areas = build_areas(level, gi0, gj0, start)
        # order: tiles column-major, then layer = index within tile
        areas.sort(key=lambda a: (a['tile'][0], a['tile'][1], min(level.y[c[2]] for c in a['cells'])))
        per_tile = defaultdict(int)
        for a in areas:
            a['layer'] = per_tile[a['tile']]
            per_tile[a['tile']] += 1
        area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
        info[(X, Y)] = {'areas': areas, 'area_of': area_of, 'origin': (gi0, gj0, ox, oz), 'start': start}
    log(f'{sum(len(v["areas"]) for v in info.values())} areas ({time.time() - t0:.0f} s)')

    # Graph: border nodes shared between sector pairs (global ids), then inner nodes.
    next_id = [1]
    border_nodes = defaultdict(list)     # sector -> [(id, pos, k)]

    def new_id():
        next_id[0] += 1
        return next_id[0]

    for (X, Y), inf in info.items():
        gi0, gj0, ox, oz = inf['origin']
        for (dX, dY), line in (((1, 0), 126), ((0, 1), 126)):
            other = (X + dX, Y + dY)
            if other not in info:
                continue
            # every kept surface on the line (all layers: floors under roofs too), grouped into connected runs
            # along it, one run per layer
            runs, active = [], []
            for t in WINDOW:
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
                n = max(1, int(math.ceil(len(run) * PITCH / BORDER_NODE_SPACING)))
                for q in range(n):
                    t, k = run[int((q + 0.5) * len(run) / n)]
                    i, j = (line, t) if dX else (t, line)
                    pos = (ox + i * PITCH, float(level.y[k]) + Y_OFFSET, oz + j * PITCH)
                    nid = 0x80000000 | new_id()
                    border_nodes[(X, Y)].append((nid, pos, k))
                    border_nodes[other].append((nid, pos, k))

    hpd_sectors, graphs = [], []
    stamp = time.localtime()
    date = (stamp.tm_year % 100) * 10000 + (stamp.tm_mon - 1) * 100 + stamp.tm_mday
    clock = stamp.tm_hour * 1000000 + stamp.tm_min * 10000 + stamp.tm_sec * 100
    # boundary edges of every sector, then the one-way portal pieces decided level-wide
    edge_sets = {}
    for (X, Y), inf in sorted(info.items()):
        gi0, gj0, ox, oz = inf['origin']
        edge_sets[(X, Y)] = trace_edges(level, inf['areas'], inf['area_of'], gi0, gj0)
    n_cand = sum(len(v[1]) for v in edge_sets.values())
    if ONE_WAY_PORTALS == 'restore':
        owners = defaultdict(list)
        for sec, inf in info.items():
            for k, ai in inf['area_of'].items():
                owners[k].append((sec, ai))
        shared = [(o[0], x) for o in owners.values() if len(o) > 1 for x in o[1:]]
        keep = restore_one_way(edge_sets, shared)
    else:
        keep = {sec: set(range(len(v[1]))) if ONE_WAY_PORTALS == 'keep' else set() for sec, v in edge_sets.items()}
    for sec, (per_area, one_way) in edge_sets.items():
        drop_one_way(per_area, one_way, keep.get(sec, set()))
    log(f'one-way portal pieces: {n_cand}, kept one-way {sum(len(v) for v in keep.values())} ({ONE_WAY_PORTALS})')
    for (X, Y), inf in sorted(info.items()):
        gi0, gj0, ox, oz = inf['origin']
        areas, area_of = inf['areas'], inf['area_of']
        traced = trace_emit(level, areas, edge_sets[(X, Y)][0], gi0, gj0, ox, oz)
        sid = ((X & 0xFF) << 8) | (Y & 0xFF)
        mesh_areas = []
        tiles = [[] for _ in range(TILES * TILES)]
        for ai, (a, (portals, borders, outline)) in enumerate(zip(areas, traced)):
            ys = [level.y[c[2]] for c in a['cells']]
            # DICE's area height range: from its lowest ground to its highest ground + 1.52 m (all normal-pitch
            # areas of sp_004_b: median +0.000 / +1.520; NavKit had -0.95 / +1.55, a metre of extra room below
            # each area, so a lower floor's range reached the deck above it - 2026-10-19)
            lo, hi = min(ys) + AREA_Y_MIN, max(ys) + AREA_Y_MAX
            mesh_areas.append(kymesh.Area(sid, a['tile'][0], a['tile'][1], a['layer'], RADIUS, PITCH, hi, lo,
                                          portals, [b[0] for b in borders], [], outline, []))
            a['borders'] = borders
            tiles[a['tile'][0] * TILES + a['tile'][1]].append(kymesh.TileItem(ai, lo, hi))
        # neighbour links
        links = defaultdict(lambda: defaultdict(list))
        for ai, a in enumerate(areas):
            for ci, (bp, t0_, t1_, line) in enumerate(a['borders']):
                di, dj = int(bp.direction[0]), int(bp.direction[2])
                other = (X + di, Y + dj)
                if other not in info:
                    continue
                # a surface just across the border, in the other sector's areas
                tmid = (t0_ + t1_) // 2
                i, j = (int(line + 0.5 * di), tmid) if di else (tmid, int(line + 0.5 * dj))
                k_here = next((c[2] for c in a['cells'] if (c[0], c[1]) == (i - di, j - dj)), None)
                if k_here is None:
                    continue
                n = level.neighbour(k_here, gi0 + i - di, gj0 + j - dj, di, dj)
                oth = info[other]
                if n is None or n not in oth['area_of']:
                    continue
                ta = oth['areas'][oth['area_of'][n]]
                rel = 0x50 + len(mesh_areas[ai].portals) * 0x20 + ci * 0x2C
                links[other][(ta['tile'][0], ta['tile'][1], ta['layer'])].append((ai, rel))
        mesh_links = []
        for other in sorted(links):
            oid = ((other[0] & 0xFF) << 8) | (other[1] & 0xFF)
            mesh_links.append(kymesh.NeighbourLink(oid, [kymesh.LinkObject(h, refs) for h, refs in sorted(links[other].items())]))
        # the runtime's tile grid starts at the rectangle's minimum (DICE: tile lines 39.6 m past it on 3,748 sector
        # axes) - so an edge sector, whose tiles start at its own edge, starts its rectangle there too (DICE 0x101:
        # -800.0 instead of -800.8). The maximum side keeps the 1.1-pitch margin (DICE drops it at the grid's maximum).
        sx, sz = inf['start']
        rect = (ox + sx * PITCH, ox + 100 + 1.1 * PITCH, oz + sz * PITCH, oz + 100 + 1.1 * PITCH)
        mesh = kymesh.Mesh(2, 0, 0, sid, RESOLUTION, PITCH, rect, TILES, TILES, RADIUS, tiles, mesh_areas,
                           mesh_links, b'\0' * 4)
        pts, pairs = build_graph(level, areas, area_of, traced, border_nodes[(X, Y)], gi0, gj0, ox, oz, new_id)
        ys = [a.min_y for a in mesh_areas] + [a.max_y for a in mesh_areas]
        bounds = (rect[0], rect[1], min(ys), max(ys), rect[2], rect[3])
        graphs.append((sid, bounds, kymesh.write_mesh(mesh), pts, pairs))
    linked = {pts[i][0] for _sid, _b, _m, pts, pairs in graphs for a_, b_, _d in pairs for i in (a_, b_)}
    for sid, bounds, mesh_bytes, pts, pairs in graphs:
        nodes, edges = finalize_graph(pts, pairs, linked)
        hpd_sectors.append(hpdtool.Sector(sid, bounds, 0, date, clock, nodes, edges, mesh_bytes))
    out = hpdtool.HpdFile(HEADER, hpd_sectors)
    data = hpdtool.write(out)
    open(out_path, 'wb').write(data)
    log(f'wrote {out_path}: {len(hpd_sectors)} sectors, {sum(len(x.nodes) for x in hpd_sectors)} nodes, '
        f'{sum(len(x.edges) for x in hpd_sectors)} edges, {len(data)} bytes ({time.time() - t0:.0f} s)')


def line_walkable(level, k0, a, b, gi0, gj0, k1):
    """Straight line from local sample a to b (i, j) over connected kept surfaces, starting on k0, ending on k1."""
    (i0, j0), (i1, j1) = a, b
    n = max(abs(i1 - i0), abs(j1 - j0))
    k, ci, cj = k0, i0, j0
    for t in range(1, n + 1):
        ti = i0 + round((i1 - i0) * t / n)
        tj = j0 + round((j1 - j0) * t / n)
        di, dj = ti - ci, tj - cj
        if di or dj:
            nk = level.neighbour(k, gi0 + ci, gj0 + cj, di, dj)
        else:
            continue
        if nk is None:
            return False
        k, ci, cj = nk, ti, tj
    return k == k1 or abs(level.y[k] - level.y[k1]) <= 0.6


def build_graph(level, areas, area_of, traced, borders, gi0, gj0, ox, oz, new_id):
    pts = []    # (id, pos, k, local i, local j)
    for a in areas:
        if len(a['cells']) < 4:
            continue
        ci = sum(c[0] for c in a['cells']) / len(a['cells'])
        cj = sum(c[1] for c in a['cells']) / len(a['cells'])
        i, j, k = min(a['cells'], key=lambda c: (c[0] - ci) ** 2 + (c[1] - cj) ** 2)
        pts.append((new_id(), (ox + i * PITCH, float(level.y[k]) + Y_OFFSET, oz + j * PITCH), k, i, j))
    cell_of = {c[2]: (c[0], c[1]) for a in areas for c in a['cells']}
    for ai, (portals, _b, _o) in enumerate(traced):
        for p in portals:
            if p.target < ai:
                continue          # one node per portal pair
            mx, mz = (p.p0[0] + p.p1[0]) / 2, (p.p0[2] + p.p1[2]) / 2
            best = min(areas[ai]['cells'], key=lambda c: (ox + c[0] * PITCH - mx) ** 2 + (oz + c[1] * PITCH - mz) ** 2)
            i, j, k = best
            pts.append((new_id(), (ox + i * PITCH, float(level.y[k]) + Y_OFFSET, oz + j * PITCH), k, i, j))
    for nid, pos, k in borders:
        i, j = int(round((pos[0] - ox) / PITCH)), int(round((pos[2] - oz) / PITCH))
        pts.append((nid, pos, k, i, j))
    # edges: pairs within DIST_EDGE_MAX whose straight line is walkable
    cell = DIST_EDGE_MAX
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
                    if d <= DIST_EDGE_MAX and line_walkable(level, p[2], (p[3], p[4]), (pts[q][3], pts[q][4]), gi0, gj0, pts[q][2]):
                        pairs.append((idx, q, d))
    # Connect remaining groups: grid search (8-connected, step rule) from one group to the nearest node
    # of another, then string-pull the path into straight walkable legs with waypoint nodes.
    parent = list(range(len(pts)))

    def find(x):
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
            prev = {pts[m][2]: None for m in members}
            q = deque(pts[m][2] for m in members)
            target = None
            while q and target is None:
                k = q.popleft()
                i, j = cell_ij[k]
                for di in (-1, 0, 1):
                    for dj in (-1, 0, 1):
                        if not (di or dj) or (i + di) not in WINDOW or (j + dj) not in WINDOW:
                            continue
                        n = level.neighbour(k, gi0 + i, gj0 + j, di, dj)
                        if n is None or n in prev:
                            continue
                        prev[n] = k
                        cell_ij.setdefault(n, (i + di, j + dj))
                        if n in node_at and find(node_at[n]) != root:
                            target = n
                            break
                        q.append(n)
                    if target is not None:
                        break
            if target is None:
                continue
            path = [target]
            while prev[path[-1]] is not None:
                path.append(prev[path[-1]])
            path.reverse()                       # from a member's surface to the target's surface
            # string pulling
            way = [path[0]]
            a = 0
            while a < len(path) - 1:
                b = len(path) - 1
                while b > a + 1 and not (math.dist(cell_ij[path[a]], cell_ij[path[b]]) * PITCH <= DIST_EDGE_MAX and
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
                    pts.append((new_id(), (ox + i * PITCH, float(level.y[k]) + Y_OFFSET, oz + j * PITCH), k, i, j))
                    parent.append(len(pts) - 1)
                    node_at[k] = len(pts) - 1
                    chain.append(len(pts) - 1)
            for u, v in zip(chain, chain[1:]):
                pairs.append((u, v, math.dist(pts[u][1], pts[v][1])))
                parent[find(u)] = find(v)
            changed = True
    return pts, pairs


def finalize_graph(pts, pairs, linked_ids):
    """Nodes and edges of a sector. Nodes without an edge in any sector (lone nodes on slivers and tiny islands
    the group joining cannot reach; border ids count edges of both sectors) are dropped - DICE's data has none."""
    keep = [idx for idx, p in enumerate(pts) if p[0] in linked_ids]
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
    nodes, edges = [], []
    for q, idx in enumerate(keep):
        nid, pos = pts[idx][0], pts[idx][1]
        nodes.append(hpdtool.Node(nid, pos, 0x80000000, DIST_EDGE_MAX if q == 0 else 0.0, 0, 0, 0))
    for idx in range(len(nodes)):
        first = len(edges)
        for cost, dst in sorted(out_edges[idx], key=lambda e: e[1]):
            edges.append(hpdtool.Edge(cost, idx, dst))
        nodes[idx].a = first + 1 if out_edges[idx] else 0     # DICE: 0 = no outgoing edges
    n = len(nodes)
    by_x = sorted(range(n), key=lambda q: nodes[q].pos[0])
    by_y = sorted(range(n), key=lambda q: nodes[q].pos[1])
    by_z = sorted(range(n), key=lambda q: nodes[q].pos[2])
    for q in range(n):
        nodes[q].b = by_x[q] | (by_y[q] << 16)
        nodes[q].c = by_z[q] | (0x10000 if nodes[q].id & 0x80000000 else 0)
    return nodes, edges


def main(argv):
    args = [a for a in argv[1:] if not a.startswith('--')]
    opts = dict(zip(argv[1:], argv[2:]))
    if len(args) < 5:
        print(__doc__)
        return 1
    build(args[0], args[1] if args[1] != '-' else None, int(args[2]), int(args[3]), args[4],
          float(opts.get('--step', 0.28)), dynamic_path=opts.get('--dynamic'), high_path=opts.get('--high'),
          narrow_path=opts.get('--narrow'), stairs_path=opts.get('--stairs'), exclude_path=opts.get('--exclude'),
          exclude_foot_path=opts.get('--exclude-foot'))
    return 0


if __name__ == '__main__':
    sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'core'))
    from bfbc2core import errors
    errors.setup('navbuild', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
