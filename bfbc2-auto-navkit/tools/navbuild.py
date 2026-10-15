"""NavKit generator: in-game collision samples -> Kynapse-format pathdata.hpd (infantry, pitch 0.8).

Pipeline (navkit/research/KYNAPSE_SPEC.md sections 3 and 4):
  1. walkable surfaces: navwalk.walkable (slope, capsule, ForbidPathfinding objects, StepMax, perimeter seeds)
  2. sectors: 100 m grid from the level's SectorOffsets; each sector uses the samples -1 .. 126 of its
     window (two-sample overlap with each neighbour)
  3. tiles: 3 x 3 per sector, 50 samples each (edges half a sample outside the samples: -1.2, 38.8,
     78.8, 101.2 m); areas = connected regions of a tile (8-connected, step rule), bisected at the middle
     of their extent while larger than 25 samples (20 m)
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
MAX_AREA = 25                   # samples (20 m) before an area is bisected
RADIUS = 0.4
RESOLUTION = 0.025
DIST_EDGE_MAX = 15.0
BORDER_NODE_SPACING = 10.0
Y_OFFSET = 0.95                 # outline / node height above the ground (EntityHeight / 2)
HEADER = (2, 2, 0, 1, 0, 0, 0, 0, 2, 0, 8, 0) + (0,) * 20 + (36, 12, 1, 0)


class Level:
    """Kept walkable surfaces on the global sample grid, with the step-rule connectivity."""

    def __init__(self, s, keep, step, gx=None, gz=None):
        self.s, self.keep, self.step = s, keep, step
        self.y = s.y.astype(float)
        ny = np.maximum(s.normal[:, 1], 0.05)
        self.gx = gx if gx is not None else -s.normal[:, 0] / ny
        self.gz = gz if gz is not None else -s.normal[:, 2] / ny
        self.col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))

    def surfaces(self, gi, gj):
        if not (0 <= gi < self.s.nx and 0 <= gj < self.s.nz):
            return range(0)
        c = gj * self.s.nx + gi
        return [k for k in range(self.s.start[c], self.s.start[c + 1]) if self.keep[k]]

    def connected(self, k, n, di, dj):
        pred = PITCH * ((self.gx[k] + self.gx[n]) / 2 * di + (self.gz[k] + self.gz[n]) / 2 * dj)
        return abs(self.y[n] - self.y[k] - pred) <= self.step

    def neighbour(self, k, gi, gj, di, dj, prefer=None):
        """The kept surface in column (gi+di, gj+dj) connected to surface k, or None. With prefer (a set of
        surfaces, e.g. those of k's own area), a connected surface in that set is returned first."""
        best, bd = None, 9e9
        for n in self.surfaces(gi + di, gj + dj):
            if self.connected(k, n, di, dj):
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


def build_areas(level, gi0, gj0):
    """Areas of one sector: list of dicts {tile: (tx, tz), cells: [(i, j, k)]} (local i, j)."""
    cells_by_tile = defaultdict(list)
    for j in WINDOW:
        for i in WINDOW:
            for k in level.surfaces(gi0 + i, gj0 + j):
                tx, tz = min((i + 1) // TILE, TILES - 1), min((j + 1) // TILE, TILES - 1)
                cells_by_tile[(tx, tz)].append((i, j, k))
    areas = []
    for tile in sorted(cells_by_tile):
        members = {k: (i, j) for i, j, k in cells_by_tile[tile]}
        for comp in components(level, members, gi0, gj0):
            for piece in bisect(level, comp, gi0, gj0):
                areas.append({'tile': tile, 'cells': piece})
    return areas


def components(level, members, gi0, gj0):
    """8-connected components (step rule) of the given surfaces {k: (i, j)}. An area is single-layer in plan: a
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
                n = level.neighbour(k, gi0 + i, gj0 + j, di, dj)
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
    """Boundary edges of every area -> merged runs. Returns per area: portals, borders, outlines."""
    h = PITCH / 2
    result = []
    for ai, area in enumerate(areas):
        edges = defaultdict(list)    # (kind, target, axis, line, dir) -> [(along, y0, y1)]
        own = {c[2] for c in area['cells']}
        for i, j, k in area['cells']:
            yk = level.y[k] + Y_OFFSET
            for di, dj in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                # a side is interior when any connected surface across it is in this area (two connected
                # surfaces in one column - stair edge, split-off layer - must not leave a stray portal inside)
                n = level.neighbour(k, gi0 + i, gj0 + j, di, dj, prefer=own)
                ni, nj = i + di, j + dj
                if n is not None and ni in WINDOW and nj in WINDOW:
                    t = area_of.get(n)
                    if t == ai:
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
          narrow_path=None, stairs_path=None, exclude_path=None, exclude_foot_path=None, seeds=None):
    t0 = time.time()
    f, _ = navsample.load(samples_path)
    s = navwalk.coarse(f) if abs(f.pitch - PITCH) > 1e-3 else f
    tris = np.load(tris_path) if tris_path else None
    dyn = np.load(dynamic_path) if dynamic_path else None
    hi = navsample.load(high_path)[0] if high_path else None
    narrow = navsample.load(narrow_path)[0] if narrow_path else None
    st = np.load(stairs_path) if stairs_path else None
    ex = np.load(exclude_path) if exclude_path else None
    exf = np.load(exclude_foot_path) if exclude_foot_path else None
    keep, gx, gz = navwalk.walkable(s, tris, step=step, dynamic_tris=dyn, high_capsule=hi, return_gradients=True,
                                    narrow_capsule=narrow, stair_tris=st, exclude_tris=ex, exclude_footprint_tris=exf,
                                    extra_seeds=seeds)
    level = Level(s, keep, step, gx, gz)
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
    for X, Y in sectors:
        gi0, gj0, ox, oz = sector_origin_index(level, X, Y, xoff, zoff)
        areas = build_areas(level, gi0, gj0)
        # order: tiles column-major, then layer = index within tile
        areas.sort(key=lambda a: (a['tile'][0], a['tile'][1], min(level.y[c[2]] for c in a['cells'])))
        per_tile = defaultdict(int)
        for a in areas:
            a['layer'] = per_tile[a['tile']]
            per_tile[a['tile']] += 1
        area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
        info[(X, Y)] = {'areas': areas, 'area_of': area_of, 'origin': (gi0, gj0, ox, oz)}
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
    for (X, Y), inf in sorted(info.items()):
        gi0, gj0, ox, oz = inf['origin']
        areas, area_of = inf['areas'], inf['area_of']
        traced = trace(level, (X, Y), areas, area_of, gi0, gj0, ox, oz)
        sid = ((X & 0xFF) << 8) | (Y & 0xFF)
        mesh_areas = []
        tiles = [[] for _ in range(TILES * TILES)]
        for ai, (a, (portals, borders, outline)) in enumerate(zip(areas, traced)):
            ys = [level.y[c[2]] for c in a['cells']]
            lo, hi = min(ys) - Y_OFFSET, max(ys) + Y_OFFSET + 0.6
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
        rect = (ox - PITCH, ox + 100 + 1.1 * PITCH, oz - PITCH, oz + 100 + 1.1 * PITCH)
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
    sys.exit(main(sys.argv))
