"""Fewer triangles for a mesh (third-person LODs of a weapon): quadric edge collapses (Garland-Heckbert) onto
existing vertices, so every vertex left keeps its own texture coordinates, normal and bone exactly.

    import decimate
    pos, nrm, uv, tris, bone = decimate.decimate(pos, nrm, uv, tris, bone, target)

The vertices as given are 'wedges': a corner's position with its attributes, split where the texture or the normals
are (a seam). Wedges at one position with one bone are one point of the surface; a collapse moves a point onto a
neighbouring point, and each of its wedges onto the neighbour's wedge it shares a triangle with - a point with a
wedge that shares no triangle with any of the neighbour's (a seam's corner, or a collapse across a seam) is not
moved there, so seams stay closed and every texture area keeps its own coordinates. Points are never merged across
bones. max_cost (optional) stops short of the target once the cheapest collapse left costs more (the quadric error:
squared distance times area, m^4): where seams keep the cheap collapses from happening, the target would otherwise be
reached by collapsing large shapes. A collapse that would turn a triangle over (normal off by more than ~80 degrees) or leave a sliver is
refused. Open edges (the rims of open surfaces) weigh as planes across them, so outlines keep their shape.
"""
import heapq
from typing import Any

import numpy as np


def _plane_quadric(a, b, c):
    n = np.cross(b - a, c - a)
    area = np.linalg.norm(n)
    if area < 1e-14:
        return np.zeros((4, 4))
    n = n / area
    p = np.append(n, -np.dot(n, a))
    return np.outer(p, p) * area


def decimate(pos, nrm, uv, tris, bone, target, boundary_weight=10.0, max_cost=None):
    pos, nrm, uv = (np.asarray(x, float) for x in (pos, nrm, uv))
    tris = np.asarray(tris, np.int64).copy()
    bone = np.asarray(bone, np.int64)
    # points: wedges at one position with one bone
    key = np.unique(np.hstack([np.round(pos, 6), bone[:, None]]), axis=0, return_inverse=True)[1].ravel()
    npts = key.max() + 1
    ppos = np.zeros((npts, 3))
    ppos[key] = pos
    wedges: list[set[int]] = [set() for _ in range(npts)]
    for w, k in enumerate(key):
        wedges[k].add(w)
    alive = np.ones(len(tris), bool)
    vt: list[set[int]] = [set() for _ in range(len(pos))]                 # wedge -> triangles
    for t, (a, b, c) in enumerate(tris):
        vt[a].add(t)
        vt[b].add(t)
        vt[c].add(t)

    def ptris(p):
        out = set()
        for w in wedges[p]:
            out |= vt[w]
        return out

    # quadrics: triangle planes, and planes across open edges
    Q = np.zeros((npts, 4, 4))
    edges: dict[tuple[Any, Any], list[int]] = {}
    for t, (a, b, c) in enumerate(tris):
        q = _plane_quadric(pos[a], pos[b], pos[c])
        for w in (a, b, c):
            Q[key[w]] += q
        for u, v in ((a, b), (b, c), (c, a)):
            e = (min(key[u], key[v]), max(key[u], key[v]))
            edges.setdefault(e, []).append(t)
    for (p0, p1), ts in edges.items():
        if len(ts) == 1 and p0 != p1:
            a, b, c = tris[ts[0]]
            fn = np.cross(pos[b] - pos[a], pos[c] - pos[a])
            d = ppos[p1] - ppos[p0]
            side = np.cross(d, fn)
            n2 = np.linalg.norm(side)
            if n2 < 1e-14:
                continue
            side /= n2
            pl = np.append(side, -np.dot(side, ppos[p0]))
            q = np.outer(pl, pl) * boundary_weight * np.dot(d, d)
            Q[p0] += q
            Q[p1] += q
    nbr: list[set[Any]] = [set() for _ in range(npts)]
    for p0, p1 in edges:
        if p0 != p1:
            nbr[p0].add(p1)
            nbr[p1].add(p0)
    version = np.zeros(npts, np.int64)

    def cost(u, v):                                       # move point u onto point v
        x = np.append(ppos[v], 1.0)
        return float(x @ (Q[u] + Q[v]) @ x)

    heap: list[tuple[float, Any, Any, Any, Any]] = []

    def push(u):
        for v in nbr[u]:
            heapq.heappush(heap, (cost(u, v), u, v, version[u], version[v]))
            heapq.heappush(heap, (cost(v, u), v, u, version[v], version[u]))

    for u in range(npts):
        for v in nbr[u]:
            if u < v:
                heapq.heappush(heap, (cost(u, v), u, v, 0, 0))
                heapq.heappush(heap, (cost(v, u), v, u, 0, 0))
    count = len(tris)

    def tri_normal(t, moved=None):
        a, b, c = (ppos[key[w]] if moved is None or key[w] != moved[0] else ppos[moved[1]] for w in tris[t])
        return np.cross(b - a, c - a)

    while count > target and heap:
        c, u, v, vu, vv = heapq.heappop(heap)
        if vu != version[u] or vv != version[v] or not wedges[u] or not wedges[v] or v not in nbr[u]:
            continue
        if max_cost is not None and c > max_cost:
            break                                         # every collapse left would cost more: stop short of target
        tu = ptris(u)
        shared = {t for t in tu if any(key[w] == v for w in tris[t])}
        # each of u's wedges -> v's wedge it shares a triangle with
        remap = {}
        ok = True
        for w in wedges[u]:
            if not vt[w]:                                 # a wedge no triangle uses any more
                continue
            cand = {x for t in vt[w] & shared for x in tris[t] if key[x] == v}
            if len(cand) != 1:
                ok = False
                break
            remap[w] = cand.pop()
        if not ok:
            continue
        # no triangle turned over or squashed
        for t in tu - shared:
            n0 = tri_normal(t)
            n1 = tri_normal(t, (u, v))
            l0, l1 = np.linalg.norm(n0), np.linalg.norm(n1)
            if l1 < 1e-12 or (l0 > 1e-12 and np.dot(n0, n1) < 0.17 * l0 * l1):
                ok = False
                break
        if not ok:
            continue
        for t in shared:
            alive[t] = False
            count -= 1
            for w in tris[t]:
                vt[w].discard(t)
        for w, x in remap.items():
            for t in list(vt[w]):
                tris[t][tris[t] == w] = x
                vt[x].add(t)
            vt[w].clear()
            key[w] = v
        wedges[u] = set()
        Q[v] += Q[u]
        for x in nbr[u]:
            nbr[x].discard(u)
            if x != v:
                nbr[x].add(v)
                nbr[v].add(x)
        nbr[v].discard(u)
        nbr[v].discard(v)
        nbr[u] = set()
        version[v] += 1
        push(v)
    t = tris[alive]
    used = np.unique(t)
    ren = np.full(len(pos), -1, np.int64)
    ren[used] = np.arange(len(used))
    return pos[used], nrm[used], uv[used], ren[t], bone[used]
