"""NavKit: walkable surfaces from in-game collision samples, and scoring against an existing pathdata.hpd.

Rules (navkit/research/KYNAPSE_SPEC.md section 3):
  1. a sampled surface is a candidate when the soldier capsule fits and its normal is within
     GroundSlopeMax (40 deg) of vertical;
  2. surfaces lying on a ForbidPathfinding object (forbid_tris.npy from levelobjects.py) are removed;
  3. regions grow 8-connected between neighbouring samples when the height step, relative to the local
     ground plane (from both normals), is at most StepMax (0.28 m);
  4. regions are seeded on the sector perimeter lines (1.6 m inside every 100 m sector edge), on terrain
     only (objects do not allow path-data seeds by default).

Usage:
  navwalk.py score <samples.bin> <pathdata.hpd> [forbid_tris.npy] [--step 0.28] [--slope 40]
                   scores at the 0.8 m sample grid (a 0.2 m run is subsampled)
"""
import math
import os
import sys
from collections import deque

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import navsample  # noqa: E402

TERRAIN_SHAPE_VTABLE = 0x1454D14   # shape class of terrain hits (navsample version 2 hit info), build 795745
OUTLINE_Y_OFFSET = 0.95            # outline heights in pathdata.hpd = ground + EntityHeight / 2
PITCH = 0.8
SECTOR = 100.0


def coarse(f):
    """Every (PITCH / f.pitch)-th column of a fine run, keeping all surfaces of the column."""
    step = int(round(PITCH / f.pitch))
    nx, nz = (f.nx - 1) // step + 1, (f.nz - 1) // step + 1
    start, idx = [0], []
    for j in range(nz):
        for i in range(nx):
            c = (j * step) * f.nx + i * step
            idx.extend(range(f.start[c], f.start[c + 1]))
            start.append(len(idx))
    idx = np.array(idx, dtype=np.int64)
    hit = f.hit[idx] if f.hit is not None and len(f.hit) else None
    return navsample.Samples(PITCH, f.min_x, f.min_z, nx, nz, f.y_top, f.y_bottom, f.radius, f.cap_bottom,
                             f.cap_top, f.ray_radius, np.array(start), f.y[idx], f.normal[idx], f.flags[idx], hit)


def on_objects(s, tris, tol=0.2):
    """Surfaces lying on one of the given world-space triangles (within tol metres vertically)."""
    out = np.zeros(len(s.y), bool)
    if tris is None or not len(tris):
        return out
    x0, x1 = s.min_x - 2, s.min_x + s.nx * s.pitch + 2
    z0, z1 = s.min_z - 2, s.min_z + s.nz * s.pitch + 2
    t = tris[(tris[:, :, 0].max(1) >= x0) & (tris[:, :, 0].min(1) <= x1) &
             (tris[:, :, 2].max(1) >= z0) & (tris[:, :, 2].min(1) <= z1)].astype(float)
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


def walkable(s, forbid_tris=None, slope=40.0, step=0.28):
    """Kept walkable surfaces (bool per surface) by rules 1-4."""
    y = s.y.astype(float)
    ny = np.maximum(s.normal[:, 1], 0.05)
    gx_all, gz_all = -s.normal[:, 0] / ny, -s.normal[:, 2] / ny
    ok = ((s.flags & 3) == 0) & (s.normal[:, 1] >= math.cos(math.radians(slope)) - 0.01)
    ok &= ~on_objects(s, forbid_tris)
    terrain = (s.hit[:, 0] == TERRAIN_SHAPE_VTABLE) if s.hit is not None else np.ones(len(y), bool)
    col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
    per_i = {i for i in range(s.nx) if round((s.min_x + i * s.pitch) % SECTOR, 1) in (1.6, 98.4)}
    per_j = {j for j in range(s.nz) if round((s.min_z + j * s.pitch) % SECTOR, 1) in (1.6, 98.4)}
    keep = np.zeros(len(y), bool)
    q = deque()
    for k in np.nonzero(ok & terrain)[0]:
        c = col_of[k]
        if c % s.nx in per_i or c // s.nx in per_j:
            keep[k] = True
            q.append(k)
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
                    if keep[n] or not ok[n]:
                        continue
                    pred = s.pitch * ((gx_all[k] + gx_all[n]) / 2 * di + (gz_all[k] + gz_all[n]) / 2 * dj)
                    if abs(y[n] - y[k] - pred) <= step:
                        keep[n] = True
                        q.append(n)
    return keep


def score(s, keep, hpd, tol=0.6):
    """Against the reference: DICE layers (cells fully inside an area) found exactly / within one cell, and
    kept surfaces with no reference layer at that height in the cell or its neighbours."""
    ref = {c: [(lo - OUTLINE_Y_OFFSET, hi - OUTLINE_Y_OFFSET, cv) for lo, hi, cv in layers]
           for c, layers in navsample.reference_cells(s, hpd).items()}
    y = s.y.astype(float)
    col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))

    def kept_near(c, lo, hi, r):
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


def main(argv):
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
    sys.exit(main(sys.argv))
