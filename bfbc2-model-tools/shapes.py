"""Simple solids for weapon parts, with texture coordinates placed on a texture atlas: surfaces of revolution (lathe),
extruded outlines (extrude), and helpers to move, turn and join them. A Geo holds triangles with per-corner data;
every vertex belongs to one skeleton bone (Geo.bone).

Units are metres in the model's space (Bad Company 2 weapons: +z along the barrel to the muzzle, +y up). A UV rect
is (u0, v0, u1, v1) on the texture; where an axis maps to a rect, the first value goes to the rect's first edge.
"""
import math

import numpy as np


class Geo:
    def __init__(self, pos=None, nrm=None, uv=None, tris=None, bone=None):
        self.pos = np.zeros((0, 3)) if pos is None else np.asarray(pos, float)
        self.nrm = np.zeros((0, 3)) if nrm is None else np.asarray(nrm, float)
        self.uv = np.zeros((0, 2)) if uv is None else np.asarray(uv, float)
        self.tris = np.zeros((0, 3), int) if tris is None else np.asarray(tris, int)
        n = len(self.pos)
        self.bone = np.zeros(n, int) if bone is None else (np.full(n, bone, int) if np.isscalar(bone) else np.asarray(bone, int))

    def __add__(self, o):
        return Geo(np.vstack([self.pos, o.pos]), np.vstack([self.nrm, o.nrm]), np.vstack([self.uv, o.uv]),
                   np.vstack([self.tris, o.tris + len(self.pos)]), np.concatenate([self.bone, o.bone]))

    def moved(self, by=(0, 0, 0), rot=None, about=(0, 0, 0)):
        """Rotated (3x3, about a point) and then moved."""
        p, n = self.pos.copy(), self.nrm.copy()
        if rot is not None:
            p = (p - about) @ np.asarray(rot).T + about
            n = n @ np.asarray(rot).T
        return Geo(p + np.asarray(by), n, self.uv.copy(), self.tris.copy(), self.bone.copy())

    def with_bone(self, bone):
        return Geo(self.pos, self.nrm, self.uv, self.tris, bone)

    def flipped(self):
        return Geo(self.pos, -self.nrm, self.uv, self.tris[:, ::-1], self.bone)

    def tangents(self):
        """(tangent, binormal) per vertex: tangent along +u on the surface; binormal across normal and tangent,
        -(normal x tangent) or +(normal x tangent) - whichever points along +v (the game stores both: a face whose
        texture is mirrored has the other sign)."""
        t = np.zeros_like(self.pos)
        bv = np.zeros_like(self.pos)
        for a, b, c in self.tris:
            e1, e2 = self.pos[b] - self.pos[a], self.pos[c] - self.pos[a]
            d1, d2 = self.uv[b] - self.uv[a], self.uv[c] - self.uv[a]
            r = d1[0] * d2[1] - d2[0] * d1[1]
            if abs(r) < 1e-12:
                continue
            t[[a, b, c]] += (e1 * d2[1] - e2 * d1[1]) / r
            bv[[a, b, c]] += (e2 * d1[0] - e1 * d2[0]) / r
        n = self.nrm / np.maximum(np.linalg.norm(self.nrm, axis=1, keepdims=True), 1e-12)
        t = t - n * np.sum(t * n, axis=1, keepdims=True)
        bad = np.linalg.norm(t, axis=1) < 1e-9
        if bad.any():                                   # no usable texture gradient: any direction across the normal
            alt = np.where(np.abs(n[bad, 0:1]) < 0.9, [[1, 0, 0]], [[0, 1, 0]])
            t[bad] = np.cross(n[bad], alt)
        t /= np.linalg.norm(t, axis=1, keepdims=True)
        b = -np.cross(n, t)
        flip = np.sum(b * bv, axis=1) < 0                 # mirrored texture: +v runs the other way
        b[flip] = -b[flip]
        return t, b


def rot_z(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])


def rot_y(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])


def rot_x(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[1, 0, 0], [0, c, -s], [0, s, c]])


def _lerp(a, b, f):
    return a + (b - a) * f


def lathe(profile, center, rect, segments=16, wraps=1.0, start=0.0, closed=False):
    """A surface of revolution around the line parallel to z through center (x, y): profile [(z, r)] in order,
    turned through 360 degrees. u runs around (rect u0 -> u1, `wraps` times: a whole number dividing `segments`,
    each wrap its own seam), v along the profile by length (v0 -> v1). Normals point away from the axis on an outer
    surface whose profile runs to +z (reverse the profile for an inner one). closed: join the profile's last point
    to its first."""
    if wraps != int(wraps) or wraps < 1 or segments % int(wraps):
        raise ValueError(f'lathe: wraps must be a whole number dividing segments ({wraps}, {segments})')
    per = segments // int(wraps)                         # segments per wrap
    prof = list(profile) + ([profile[0]] if closed else [])
    seg_len = [0.0]
    for (z0, r0), (z1, r1) in zip(prof, prof[1:]):
        seg_len.append(seg_len[-1] + math.hypot(z1 - z0, r1 - r0))
    total = seg_len[-1] or 1.0
    u0, v0, u1, v1 = rect
    pos, nrm, uv, tris = [], [], [], []
    cx, cy = center
    # one ring of vertices per profile segment end (hard edges between segments)
    for k in range(len(prof) - 1):
        (za, ra), (zb, rb) = prof[k], prof[k + 1]
        dz, dr = zb - za, rb - ra
        ln = math.hypot(dz, dr) or 1.0
        nz, nr = -dr / ln, dz / ln                       # the profile turned a quarter: outward for +z
        for w in range(int(wraps)):                       # each wrap: its own columns, u from u0 to u1
            base = len(pos)
            for j in range(per + 1):
                a = start + 2 * math.pi * (w * per + j) / segments
                ca, sa = math.cos(a), math.sin(a)
                for z, r, sl in ((za, ra, seg_len[k]), (zb, rb, seg_len[k + 1])):
                    pos.append((cx + r * ca, cy + r * sa, z))
                    nrm.append((nr * ca, nr * sa, nz))
                    uv.append((_lerp(u0, u1, j / per), _lerp(v0, v1, sl / total)))
            for j in range(per):
                a, b = base + 2 * j, base + 2 * j + 1
                c, d = base + 2 * (j + 1), base + 2 * (j + 1) + 1
                tris += [(a, c, b), (b, c, d)]
    return Geo(pos, nrm, uv, tris)


def _triangulate(poly):
    """Ear clipping of a simple polygon [(a, b)] (either winding) -> [(i, j, k)] counter-clockwise."""
    pts = [tuple(p) for p in poly]
    idx = list(range(len(pts)))
    area = sum(pts[i][0] * pts[(i + 1) % len(pts)][1] - pts[(i + 1) % len(pts)][0] * pts[i][1] for i in idx)
    if area < 0:
        idx.reverse()

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    out, guard = [], 0
    while len(idx) > 3 and guard < 10000:
        guard += 1
        for k in range(len(idx)):
            i, j, l = idx[k - 1], idx[k], idx[(k + 1) % len(idx)]
            if cross(pts[i], pts[j], pts[l]) <= 1e-15:
                continue
            if any(cross(pts[i], pts[j], pts[m]) >= 0 and cross(pts[j], pts[l], pts[m]) >= 0 and
                   cross(pts[l], pts[i], pts[m]) >= 0 for m in idx if m not in (i, j, l)):
                continue
            out.append((i, j, l))
            idx.pop(k)
            break
        else:
            break
    if len(idx) == 3:
        out.append(tuple(idx))
    return out


def extrude(outline, half, side_rect, side_box, edge_rect, axis='x', center=0.0):
    """A slab: the outline [(a, b)] in the plane across `axis` (axis x: a = z, b = y; axis z: a = x, b = y),
    thickness 2 * half(a, b) (a number or a function) about `center` on that axis. Both faces are textured with
    side_rect, the outline's box side_box (a0, b0, a1, b1) mapping a0 -> u0, a1 -> u1, b0 -> v0, b1 -> v1; the rim
    runs around the outline with edge_rect (u by length along the rim, v across)."""
    hf = half if callable(half) else (lambda a, b, h=half: h)
    u0, v0, u1, v1 = side_rect
    a0, b0, a1, b1 = side_box
    tri = _triangulate(outline)

    def place(a, b, s):
        if axis == 'x':
            return (center + s, b, a)
        return (a, b, center + s)

    def face_uv(a, b):
        return (_lerp(u0, u1, (a - a0) / (a1 - a0)), _lerp(v0, v1, (b - b0) / (b1 - b0)))

    nax = np.array([1.0, 0, 0]) if axis == 'x' else np.array([0, 0, 1.0])
    g = Geo()
    for sgn in (1, -1):
        pos = [place(a, b, sgn * hf(a, b)) for a, b in outline]
        uv = [face_uv(a, b) for a, b in outline]
        # the outline's (a, b) frame: axis x -> (z, y), whose normal (z x y) is -x; axis z -> (x, y): +z
        frame = -nax if axis == 'x' else nax
        tris = [t if sgn * 1 == (1 if (frame @ nax) > 0 else -1) else t[::-1] for t in tri]
        g = g + Geo(pos, [tuple(sgn * nax)] * len(pos), uv, tris)
    # rim
    n = len(outline)
    per = [0.0]
    for k in range(n):
        a, b = outline[k], outline[(k + 1) % n]
        per.append(per[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
    total = per[-1] or 1.0
    eu0, ev0, eu1, ev1 = edge_rect
    ccw = sum(outline[k][0] * outline[(k + 1) % n][1] - outline[(k + 1) % n][0] * outline[k][1] for k in range(n)) > 0
    for k in range(n):
        (pa, pb), (qa, qb) = outline[k], outline[(k + 1) % n]
        da, db = qa - pa, qb - pb
        ln = math.hypot(da, db) or 1.0
        oa, ob = (db / ln, -da / ln) if ccw else (-db / ln, da / ln)        # outward in the outline's plane
        nv = place(oa, ob, 0.0)
        nv = (nv[0] - (center if axis == 'x' else 0), nv[1], nv[2] - (center if axis == 'z' else 0))
        quad = [place(pa, pb, hf(pa, pb)), place(qa, qb, hf(qa, qb)), place(qa, qb, -hf(qa, qb)), place(pa, pb, -hf(pa, pb))]
        uv = [(_lerp(eu0, eu1, per[k] / total), ev0), (_lerp(eu0, eu1, per[k + 1] / total), ev0),
              (_lerp(eu0, eu1, per[k + 1] / total), ev1), (_lerp(eu0, eu1, per[k] / total), ev1)]
        q = Geo(quad, [nv] * 4, uv, [(0, 1, 2), (0, 2, 3)])
        g = g + _facing(q, np.array(nv))
    return g


def _facing(g, normal):
    """g with every triangle wound so that its geometric normal agrees with `normal`."""
    tris = []
    for t in g.tris:
        a, b, c = g.pos[t]
        if np.cross(b - a, c - a) @ normal < 0:
            t = t[::-1]
        tris.append(t)
    return Geo(g.pos, g.nrm, g.uv, tris, g.bone)


def outward(g, inside_point):
    """Each triangle wound so it faces away from inside_point (closed convex-ish solids)."""
    tris = []
    for t in g.tris:
        a, b, c = g.pos[t]
        if np.cross(b - a, c - a) @ ((a + b + c) / 3 - inside_point) < 0:
            t = t[::-1]
        tris.append(t)
    return Geo(g.pos, g.nrm, g.uv, tris, g.bone)


def fix_winding(g):
    """Each triangle wound to agree with its vertices' normals (the convention: counter-clockwise seen from the
    side the normal points to)."""
    tris = []
    for t in g.tris:
        a, b, c = g.pos[t]
        if np.cross(b - a, c - a) @ g.nrm[t].sum(0) < 0:
            t = t[::-1]
        tris.append(t)
    return Geo(g.pos, g.nrm, g.uv, tris, g.bone)
