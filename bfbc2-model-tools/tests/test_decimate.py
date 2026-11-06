r"""decimate.py (unit tier): a UV sphere (its texture seam split into two wedges per point, as a game mesh has it)
taken from 1,984 triangles to 500:

- at most the target, at least half of it;
- closed: welded by position every edge has exactly two triangles - the seam did not open;
- every vertex left is an input vertex with its own position, texture coordinates, normal and bone;
- round: every triangle's middle within 6 % of the radius of the sphere;
- bones: a second sphere touching the first on another bone keeps all its points where they were (never merged in);
- max_cost: a cap below every collapse's cost leaves the sphere whole; a cap between stops short of the target.

  python test_decimate.py
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import decimate  # noqa: E402


def sphere(rings=32, segs=32, r=1.0, at=(0, 0, 0)):
    pos, uv, tris = [], [], []
    for i in range(rings + 1):
        th = np.pi * i / rings
        for j in range(segs + 1):                          # column segs repeats column 0: the seam's second wedge
            ph = 2 * np.pi * j / segs
            pos.append([r * np.sin(th) * np.cos(ph), r * np.cos(th), r * np.sin(th) * np.sin(ph)])
            uv.append([j / segs, i / rings])
    w = segs + 1
    for i in range(rings):
        for j in range(segs):
            a, b, c, d = i * w + j, i * w + j + 1, (i + 1) * w + j, (i + 1) * w + j + 1
            if i:
                tris.append([a, b, c])                     # no slivers at the poles
            if i < rings - 1:
                tris.append([b, d, c])
    pos = np.array(pos)
    pos[np.abs(pos) < 1e-12] = 0.0
    # pole rows: every column is the same point; keep them as wedges of one point
    return pos + at, pos / r, np.array(uv), np.array(tris)


def main():
    fails = []
    pos, nrm, uv, tris = sphere()
    bone = np.zeros(len(pos), int)
    p, n, u, t, b = decimate.decimate(pos, nrm, uv, tris, bone, 500)
    print(f'sphere: {len(tris)} -> {len(t)} triangles, {len(p)} vertices')
    if not 250 <= len(t) <= 500:
        fails.append(f'{len(t)} triangles for a target of 500')
    key = np.unique(np.round(p, 6), axis=0, return_inverse=True)[1].ravel()
    edges = {}
    for a, b_, c in key[t]:
        for e in ((a, b_), (b_, c), (c, a)):
            k = (min(e), max(e))
            edges[k] = edges.get(k, 0) + 1
    open_ = sum(1 for v in edges.values() if v != 2)
    if open_:
        fails.append(f'{open_} edges not on exactly two triangles (a crack)')
    ins = {tuple(np.round(np.hstack([pos[i], uv[i], nrm[i]]), 9)) for i in range(len(pos))}
    stray = sum(1 for i in range(len(p)) if tuple(np.round(np.hstack([p[i], u[i], n[i]]), 9)) not in ins)
    if stray:
        fails.append(f'{stray} vertices not among the input')
    mid = np.linalg.norm(p[t].mean(1), axis=1)
    if mid.min() < 0.94:
        fails.append(f'a triangle sinks to {mid.min():.3f} of the radius')
    print(f'closed: {not open_}; vertices from the input: {not stray}; triangle middles {mid.min():.3f} - 1')
    # two bones: a small sphere touching the big one, on bone 1
    p2, n2, uv2, t2 = sphere(8, 8, 0.2, (1.2, 0, 0))
    allp = np.vstack([pos, p2])
    allt = np.vstack([tris, t2 + len(pos)])
    bones = np.r_[np.zeros(len(pos), int), np.ones(len(p2), int)]
    p, n, u, t, b = decimate.decimate(allp, np.vstack([nrm, n2]), np.vstack([uv, uv2]), allt, bones, 600)
    small = {tuple(np.round(x, 9)) for x in p2}
    moved = sum(1 for i in range(len(p)) if b[i] == 1 and tuple(np.round(p[i], 9)) not in small)
    mixed = sum(1 for tri in t if len(set(b[tri])) > 1)
    if moved or mixed:
        fails.append(f'bones mixed: {moved} vertices of bone 1 moved off it, {mixed} triangles on two bones')
    print(f'two bones: {len(t)} triangles, none across bones: {not mixed}')
    # max_cost
    whole = decimate.decimate(pos, nrm, uv, tris, bone, 500, max_cost=1e-12)[3]
    some = decimate.decimate(pos, nrm, uv, tris, bone, 500, max_cost=1e-5)[3]
    if len(whole) != len(tris) or not 500 < len(some) < len(tris):
        fails.append(f'max_cost: {len(whole)} triangles with a tiny cap (want {len(tris)}), {len(some)} with 1e-5')
    print(f'max_cost: 1e-12 keeps {len(whole)} triangles, 1e-5 stops at {len(some)}')
    for f in fails:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
