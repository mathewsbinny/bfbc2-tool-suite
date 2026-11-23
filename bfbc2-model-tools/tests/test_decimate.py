r"""decimate.py (unit tier): a UV sphere (its texture seam split into two wedges per point, as a game mesh has it)
taken from 1,984 triangles to 500:

- at most the target, at least half of it;
- closed: welded by position every edge has exactly two triangles - the seam did not open;
- every vertex left is an input vertex with its own position, texture coordinates, normal and bone;
- round: every triangle's middle within 6 % of the radius of the sphere;
- bones: a second sphere touching the first on another bone keeps all its points where they were (never merged in);
- max_cost: a cap below every collapse's cost leaves the sphere whole; a cap between stops short of the target.

Round PR1b-2-rest2 (the collapses in the Rust core): on those meshes, solids built with shapes.py as the weapon tools
build them (lathes, extrusions, turned and joined, two bones) and jittered spheres, each at several targets and caps,
the core's arrays are compared with decimate_reference (the NumPy original) - array for array, bit for bit - where
NumPy's OpenBLAS runs the kernels the core writes out (core SkylakeX, the machine that recorded the WW2 oracle;
elsewhere the reference's own last bits are its processor's, so that comparison is skipped and said), and always
with the digests recorded from the reference (tests/decimate_golden.json; --record writes them from the reference,
on a SkylakeX machine only).

  python test_decimate.py [--record]
"""
import ctypes
import glob
import hashlib
import json
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('model-tools')
import decimate  # noqa: E402
import shapes  # noqa: E402

GOLDEN = os.path.join(HERE, 'decimate_golden.json')


def blas_core():
    """NumPy's OpenBLAS core ('SkylakeX' on the reference machine), or '' when it cannot be asked."""
    libs = os.path.join(os.path.dirname(np.__file__), '..', 'numpy.libs')
    for f in glob.glob(os.path.join(libs, '*openblas*.dll')) + glob.glob(os.path.join(libs, '*openblas*.so*')):
        try:
            lib = ctypes.CDLL(f)
        except OSError:
            continue
        for name in ('scipy_openblas_get_corename64_', 'openblas_get_corename64_', 'openblas_get_corename'):
            fn = getattr(lib, name, None)
            if fn is not None:
                fn.restype = ctypes.c_char_p
                return fn().decode()
    return ''


def rifle(seg=16):
    """Weapon-like parts as the WW2 tools build them with shapes.py: a lathed barrel and band (bone 0), an extruded
    stock and a bolt (bone 1) turned and moved, joined."""
    barrel = shapes.lathe([(0.0, 0.012), (0.4, 0.011), (0.42, 0.016), (0.6, 0.009)], (0.0, 0.02), (0, 0, 0.5, 0.25),
                          segments=seg)
    band = shapes.lathe([(0.3, 0.02), (0.33, 0.02)], (0.0, 0.02), (0.5, 0, 1, 0.25), segments=seg - 4, wraps=2, closed=True)
    stock = shapes.extrude([(-0.4, -0.02), (0.0, -0.02), (0.05, 0.03), (-0.1, 0.06), (-0.45, 0.04)], 0.018,
                           (0, 0.25, 0.5, 0.5), (-0.45, -0.02, 0.05, 0.06), (0.5, 0.25, 1, 0.5))
    bolt = shapes.lathe([(0.0, 0.006), (0.08, 0.006)], (0.0, 0.0), (0, 0.5, 0.5, 0.75), segments=seg // 2,
                        closed=True)
    bolt = bolt.moved(by=(0.02, 0.03, 0.01), rot=shapes.rot_y(0.3)).with_bone(1)
    g = barrel + band + shapes.fix_winding(stock) + bolt
    return g.pos, g.nrm, g.uv, g.tris, g.bone


def jittered(seed):
    pos, nrm, uv, tris = sphere(16, 20, 0.05)
    rng = np.random.default_rng(seed)
    pos = pos + rng.normal(0, 0.002, pos.shape) * (np.abs(pos[:, 1:2]) < 0.049)
    return pos, nrm, uv, tris, np.zeros(len(pos), int)


def cases():
    """(name, args, keyword arguments) of the parity cases."""
    out = []
    pos, nrm, uv, tris = sphere()
    s = (pos, nrm, uv, tris, np.zeros(len(pos), int))
    for target, cap in ((500, None), (200, None), (1200, None), (500, 1e-5), (100, 1e-4)):
        out.append((f'sphere {target} {cap}', s + (target,), {'max_cost': cap}))
    r = rifle()
    n = len(r[3])
    for f, cap in ((0.5, None), (0.25, None), (0.1, None), (0.05, None), (0.1, 2e-9), (0.02, 1e-7)):
        out.append((f'rifle {f} {cap}', r + (int(n * f),), {'max_cost': cap}))
    out.append(('rifle boundary 3', r + (int(n * 0.2),), {'boundary_weight': 3.0}))
    big = rifle(48)
    nb = len(big[3])
    for f, cap in ((0.3, None), (0.1, None), (0.05, 1e-8)):
        out.append((f'rifle 48 {f} {cap}', big + (int(nb * f),), {'max_cost': cap}))
    for seed in range(4):
        j = jittered(seed)
        out.append((f'jittered {seed}', j + (len(j[3]) // 4,), {}))
    return out


def digest(arrays):
    h = hashlib.sha256()
    for a in arrays:
        a = np.ascontiguousarray(a)
        h.update(str((a.dtype.str, a.shape)).encode())
        h.update(a.tobytes())
    return h.hexdigest()


def parity(record):
    """The core against the reference and the recorded digests: the failures."""
    fails = []
    core = blas_core()
    strict = core == 'SkylakeX'
    golden = json.load(open(GOLDEN, encoding='utf-8')) if os.path.isfile(GOLDEN) and not record else {}
    got = {}
    for name, args, kw in cases():
        new = decimate.decimate(*args, **kw)
        dn = digest(new)
        got[name] = digest(decimate.decimate_reference(*args, **kw)) if record else dn
        if strict:
            ref = decimate.decimate_reference(*args, **kw)
            if digest(ref) != dn:
                fails.append(f"{name}: the core's arrays are not the reference's ({len(ref[3])} / {len(new[3])} "
                             f"triangles)")
        if not record and golden.get(name) != dn:
            fails.append(f'{name}: not the recorded digest')
        print(f'{name}: {len(args[3])} -> {len(new[3])} triangles')
    if record:
        if not strict:
            return [f"--record needs the reference machine's OpenBLAS core (SkylakeX), not {core or 'unknown'}"]
        with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(got, f, indent=1, sort_keys=True)
            f.write('\n')
        print(f'recorded {len(got)} digests')
    against = 'yes' if strict else f"skipped (OpenBLAS core {core or 'unknown'}, not SkylakeX)"
    print(f'parity: {len(cases())} cases; against the reference: {against}')
    return fails


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
    pts = np.array(pos)
    pts[np.abs(pts) < 1e-12] = 0.0
    # pole rows: every column is the same point; keep them as wedges of one point
    return pts + at, pts / r, np.array(uv), np.array(tris)


def shapes_parity():
    """shapes.py's core (products, tangents, windings) against its NumPy originals on solids as the weapon tools
    build them: the failures (only where NumPy runs the reference machine's OpenBLAS kernels)."""
    if blas_core() != 'SkylakeX':
        print('shapes parity: skipped (OpenBLAS core not SkylakeX)')
        return []
    fails = []
    saved = shapes._facing
    geos = []
    try:
        for ref in (False, True):
            shapes._facing = shapes._facing_reference if ref else saved
            r = rifle()
            geos.append(shapes.Geo(*r[:4], r[4]))
    finally:
        shapes._facing = saved
    if digest([geos[0].pos, geos[0].tris]) != digest([geos[1].pos, geos[1].tris]):
        fails.append('extrude: the windings are not the reference\'s')
    g = geos[0]
    rng = np.random.default_rng(1)
    n = 0
    for k in range(40):
        a, b, c = rng.uniform(-3.2, 3.2, 3)
        rot = shapes.rot_x(a) @ shapes.rot_y(b) @ shapes.rot_z(c) if k % 2 else shapes.rot_y(a)
        about = tuple(rng.normal(0, 0.1, 3))
        by = tuple(rng.normal(0, 0.2, 3))
        sub = shapes.Geo(g.pos[: 1 + k * 7], g.nrm[: 1 + k * 7], g.uv[: 1 + k * 7], np.zeros((0, 3), int))
        for x, y in ((g.moved(by, rot, about), g.moved_reference(by, rot, about)),
                     (sub.moved(by, rot, about), sub.moved_reference(by, rot, about))):
            n += 1
            if digest([x.pos, x.nrm]) != digest([y.pos, y.nrm]):
                fails.append(f'moved {k}: not the reference\'s')
        m = g.moved(by, rot, about)
        for name, new, old in (('tangents', m.tangents(), m.tangents_reference()),
                               ('fix_winding', shapes.fix_winding(m).tris, shapes.fix_winding_reference(m).tris),
                               ('outward', shapes.outward(m, about).tris, shapes.outward_reference(m, about).tris),
                               ('facing', shapes._facing(m, np.array(by)).tris,
                                shapes._facing_reference(m, np.array(by)).tris)):
            n += 1
            if digest(new if isinstance(new, tuple) else [new]) != digest(old if isinstance(old, tuple) else [old]):
                fails.append(f'{name} {k}: not the reference\'s')
    print(f'shapes parity: {n} results of moved / tangents / windings against the NumPy originals')
    return fails


def main():
    fails = []
    pos, nrm, uv, tris = sphere()
    bone = np.zeros(len(pos), int)
    p, n, u, t, b = decimate.decimate(pos, nrm, uv, tris, bone, 500)
    print(f'sphere: {len(tris)} -> {len(t)} triangles, {len(p)} vertices')
    if not 250 <= len(t) <= 500:
        fails.append(f'{len(t)} triangles for a target of 500')
    key = np.unique(np.round(p, 6), axis=0, return_inverse=True)[1].ravel()
    edges: dict[tuple[int, int], int] = {}
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
    fails += parity('--record' in sys.argv)
    fails += shapes_parity()
    for f in fails:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
