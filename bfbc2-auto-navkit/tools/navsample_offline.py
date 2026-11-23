"""NavKit: offline collision sampler - the in-game sampler's output (OPFOR's [dev] navsample, src\\dll\\navsample.cpp)
computed from the level's own data, without starting the game: the terrain height field plus the Havok collision of
every placed object and static-group member, as the game's physics world holds them.

Usage:
  navsample_offline.py sample  <level> [--pitch 0.8|0.2] [--box minX,minZ,maxX,maxZ] [--y bottom,top] [--bodies he]
                               both capsule runs (normal and high) of one pitch; at 0.2 over the level's detailed sectors
                               (as navkit.py sample does). Writes <work>\\navsample_offline\\<level>_navkit[_hi][_p02].bin
  navsample_offline.py like    <level> <in-game samples.bin> [--bodies he]
                               the same grid as an in-game file (for validation): <level>_like_<file name>
  navsample_offline.py validate <level> <in-game.bin> ...   'like' for several files (geometry read once), measures
                               saved as <level>_validate.json
  navsample_offline.py compare <in-game.bin> <offline.bin>   per column: heights, layer counts, walkable mask
  navsample_offline.py parity  <level> ... [--name offline]   offline samples on the in-game grids, NavKit built
                               from both, navdiff (offline build against the in-game build) and navparity (both
                               against DICE); work\\navkit_offline\\parity_<name>.json
  navsample_offline.py build   <level>                  NavKit build from the offline samples (navkit.py build's inputs)
                               into <work>\\navkit_offline\\<level>\\pathdata.hpd
  navsample_offline.py check   <level>                  hpdcheck + roundtrip + ordercheck on that build
  navsample_offline.py all     <level>                  prepare (offline; a map without path data: the box from
                               its combat areas) + sample 0.8 + 0.2 + build + check (stockmp on one level)
  navsample_offline.py stage   <stage folder>         an export's stage (the path-data step, round NK6): the staged
                               level prepared from its bundle, sampled at 0.8 m and over its detailed sectors at 0.2 m,
                               into <work>\\navkit_stage\\<level> (the base level prepared first)
  navsample_offline.py stockmp [level ...] [--steps prepare,sample,build,check]
                               the stock MP maps (STOCK_MP), one after another; work\\navkit_offline\\stockmp.json

The sampling itself is in the Rust core since round NKS-deep C (bc2_navbuild::sampler, sample_grid below; its
command line: bfbc2 navkit sample <level> --offline, which also samples DICE's detailed sectors and a vehicle-shaped
run). What an in-game sample is (navsample.cpp), and how it is reproduced:
  * a sphere cast (radius 0.05) straight down from yTop; after a hit at y the next cast starts at y - 0.1; a cast that
    hits within 0.07 of its start (it started inside geometry) is not recorded - the start steps down 0.5 m, at most
    10 m in a row. Offline: the ray's hits per column are found by rasterising every collision triangle onto the grid
    (vertical line through the column centre against the triangle, or the cast's sphere touching it), then that loop
    is replayed on the sorted hits. The height field is solid: a cast starting under it hits at once, so nothing under
    the terrain is ever recorded. Convex shapes are solid too (a start inside one hits at once); triangle meshes
    (MOPP) are not, and are hit from both sides (the normal facing the ray).
  * each recorded surface gets a capsule test: radius 0.4, from stepMax (0.28; the high run 0.6) to 1.9 m above it.
    Offline: for every triangle near the column, the height interval of the vertical line inside (triangle + 0.4 m
    ball) is computed exactly (face slab, edge cylinders, vertex spheres); a surface is blocked when its capsule axis
    (CAPSULE_AXIS) meets one. The height field: its height under the capsule's lower hemisphere, sampled on rings (0,
    0.1 .. 0.4 m, 16 directions).
  * hit info (the record's three words): the shape vtables the in-game file has - terrain TERRAIN_SHAPE_VTABLE,
    mesh MESH_SHAPE_VTABLE, convex / box CONVEX_SHAPE_VTABLE - then RAY_SHAPE_VTABLE, then 0. No soldiers are in this
    world (navwalk treats them as absent anyway).

Which Havok bodies are in the world: every blueprint's *_Havok.res holds one or two physics systems of rigid bodies
named 'h', 'e' or 'c' (one letter). Measured against the in-game samples (README, "Standalone sampling"): the AI ray
and capsule see bodies named BODIES (default 'h' and 'c'); 'e' bodies are another layer.

Clean room: only the level's data files are read (Havok packfiles through hkpack.py, the height field through
core's terrain.py); no game code runs.
"""
import json
import math
import os
import struct
import sys
import time
from typing import cast

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit', 'navkit-tools')
import hkpack  # noqa: E402
import navsample  # noqa: E402

# --- the in-game sampler's settings (navsample.cpp, navkit.py AGENTS) ----------------------------------------------
# (the cast's own settings - the 0.1 m gap, the 0.5 m steps up to 10 m, the 0.07 m 'at once', 32 layers, the
# sphere's reach past edges - are bc2_navbuild::sampler's constants)
RAY_RADIUS = 0.05
RADIUS, HEIGHT = 0.4, 1.9
CAP_BOTTOMS = {'': 0.28, '_hi': 0.6}     # normal and high-capsule runs
# The capsule as the in-game flags show it: heights of its lowest and highest sphere centres above the surface, per
# run. navsample.cpp asks for one from stepMax to 1.9 m (sphere centres 0.68 .. 1.5 m; high run 1.0 .. 1.5), but its
# blocked flags fit a capsule whose lowest centre is higher: fitted on sp_004_b's 0.8 m runs (agreement of the blocked
# flag on matched surfaces, terrain / objects): normal 0.68 -> 98.9 / 94.3 %, 0.95 -> 99.3 / 96.6 %; high 1.0 -> 98.9 /
# 94.9 %, 1.15 -> 99.2 / 96.3 % (the upper end hardly matters: 1.3 .. 1.9 within 0.1 point). Steep terrain shows it
# plainly: in game the normal run's capsule is blocked from a slope of ~64 deg, the high run's from ~69 deg.
CAPSULE_AXIS = {0.28: (0.95, 1.5), 0.6: (1.15, 1.5)}

TERRAIN_SHAPE_VTABLE = 0x1454D14
MESH_SHAPE_VTABLE = 0x1483B38    # MOPP-tree object hits in the in-game files (44,298 of sp_004_b's 0.8 m samples)
CONVEX_SHAPE_VTABLE = 0x14864B0  # the rare third kind in the in-game files (39 on sp_004_b)
RAY_SHAPE_VTABLE = 0x14852B8     # second word of every in-game record (the cast's own body)

BODIES = 'hc'                    # rigid-body names whose collision the AI queries see (measured, see the docstring)
CONVEX_RADIUS = True             # convex shapes and boxes grown by their convex radius (Havok's 0.05 m shell)
INTACT_ONLY = True               # leave out mesh parts an object shows only once damaged (destroyed_parts)

KIND_TERRAIN, KIND_MESH, KIND_CONVEX = 0, 1, 2
VTABLE_OF_KIND = np.array([TERRAIN_SHAPE_VTABLE, MESH_SHAPE_VTABLE, CONVEX_SHAPE_VTABLE], dtype=np.uint32)


# --- Havok assets ----------------------------------------------------------------------------------------------------
# Havok 5.5 packfiles (32-bit). These files carry no class descriptions, so the offsets below were read off the data
# (objects found through the packfile's virtual fixups, hkpack.py):
#   hkpPhysicsSystem       +0x08 rigid bodies (array of pointers)
#   hkpRigidBody           +0x10 shape, +0x70 name (one letter: h / e / c), +0xE0 transform (3 rotation columns,
#                          translation; row-vector convention as the level's matrices: p @ M[:3, :3] + M[3, :3])
#   hkpMoppBvTreeShape     +0x34 child shape (hkpStorageExtendedMeshShape or hkpListShape)
#   hkpStorageExtendedMeshShape  +0xC0 mesh storages (array of pointers; every one holds triangles)
#   ...MeshSubpartStorage  +0x08 vertices (vec4), +0x14 16-bit indices (3 corners + padding per triangle)
#   hkpListShape           +0x18 children (16 bytes each, shape pointer first)
#   hkpConvexTranslateShape +0x18 child, +0x20 translation
#   hkpConvexVerticesShape +0x10 convex radius, +0x40 vertices (FourVectors: x4 y4 z4), +0x4C vertex count,
#                          +0x50 plane equations (n, d: n . v + d = 0, n outward)
#   hkpBoxShape            +0x10 convex radius, +0x20 half extents
def _arr(pf, at):
    p = pf.deref(at)
    n = struct.unpack_from('<i', pf.d, at + 4)[0]
    return p, (n if p else 0)


def _hull_faces(verts, planes, radius):
    """Triangles of a convex hull given its vertices and outward plane equations: per plane, the vertices on it in
    order around it, fanned. Grown by radius along each face normal (CONVEX_RADIUS)."""
    tris = []
    scale = max(1.0, float(np.abs(verts).max()))
    for n in planes:
        nn = n[:3]
        ln = np.linalg.norm(nn)
        if ln < 1e-6:
            continue
        nn = nn / ln
        d = n[3] / ln
        on = verts[np.abs(verts @ nn + d) < 2e-3 * scale]
        if len(on) < 3:
            continue
        on = np.unique(np.round(on, 5), axis=0)
        if len(on) < 3:
            continue
        c = on.mean(0)
        u = np.cross(nn, [0, 1, 0] if abs(nn[1]) < 0.9 else [1, 0, 0])
        u /= np.linalg.norm(u)
        v = np.cross(nn, u)
        ang = np.arctan2((on - c) @ v, (on - c) @ u)
        on = on[np.argsort(ang)]
        off = nn * radius if CONVEX_RADIUS else 0
        for k in range(1, len(on) - 1):
            t = np.array([on[0], on[k], on[k + 1]]) + off
            if np.dot(np.cross(t[1] - t[0], t[2] - t[0]), nn) < 0:
                t = t[[0, 2, 1]]
            tris.append(t)
    return np.array(tris, dtype=np.float64).reshape(-1, 3, 3)


def _box_faces(h, radius):
    h = np.asarray(h[:3], float) + (radius if CONVEX_RADIUS else 0)
    c = np.array([[x, y, z] for x in (-h[0], h[0]) for y in (-h[1], h[1]) for z in (-h[2], h[2])])
    faces = [(0, 1, 3), (0, 3, 2), (4, 6, 7), (4, 7, 5), (0, 4, 5), (0, 5, 1), (2, 3, 7), (2, 7, 6),
             (0, 2, 6), (0, 6, 4), (1, 5, 7), (1, 7, 3)]
    t = c[np.array(faces)]
    # outward winding
    nrm = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
    flip = (nrm * t.mean(1)).sum(1) < 0
    t[flip] = t[flip][:, [0, 2, 1]]
    return t


def _shape_parts(pf, sh, offset, out, depth=0):
    """Appends (kind, local triangles) of a shape tree to out; offset: translation of convex-translate parents."""
    if not sh or depth > 8:
        return
    cls = pf.by_offset.get(sh)
    d = pf.d
    if cls == 'hkpMoppBvTreeShape':
        _shape_parts(pf, pf.deref(sh + 0x34), offset, out, depth + 1)
    elif cls == 'hkpStorageExtendedMeshShape':
        arr, n = _arr(pf, sh + 0xC0)
        sp, nsub = _arr(pf, sh + 0x50)
        for k in range(n):
            # the triangle subpart record (0x40 bytes) of storage k: +0x20 its part (the asset's PartInfoDatas index)
            part = struct.unpack_from('<i', d, sp + 0x40 * k + 0x20)[0] if sp and k < nsub else -1
            st = pf.deref(arr + 4 * k)
            if not st:
                continue
            p, nv = _arr(pf, st + 8)
            ip, ni = _arr(pf, st + 0x14)
            if not (p and nv > 0 and ip and ni >= 4):
                continue
            v = np.frombuffer(d, '<f4', nv * 4, p).reshape(-1, 4)[:, :3].astype(np.float64)
            idx = np.frombuffer(d, '<u2', ni - ni % 4, ip).reshape(-1, 4)[:, :3]
            if idx.max() >= nv:
                continue
            out.append((KIND_MESH, v[idx] + offset, part))
    elif cls == 'hkpListShape':
        arr, n = _arr(pf, sh + 0x18)
        for k in range(n):
            _shape_parts(pf, pf.deref(arr + 16 * k), offset, out, depth + 1)
    elif cls == 'hkpConvexTranslateShape':
        t = np.array(struct.unpack_from('<3f', d, sh + 0x20))
        _shape_parts(pf, pf.deref(sh + 0x18), offset + t, out, depth + 1)
    elif cls == 'hkpConvexVerticesShape':
        radius = struct.unpack_from('<f', d, sh + 0x10)[0]
        vp, nb = _arr(pf, sh + 0x40)
        nverts = struct.unpack_from('<i', d, sh + 0x4C)[0]
        pp, npl = _arr(pf, sh + 0x50)
        if not (vp and nb > 0 and pp and npl > 0) or nb > 100000:
            return
        fv = np.frombuffer(d, '<f4', nb * 12, vp).reshape(nb, 3, 4)
        verts = np.stack([fv[:, 0, :].ravel(), fv[:, 1, :].ravel(), fv[:, 2, :].ravel()], 1).astype(np.float64)
        verts = verts[:max(1, min(nverts, len(verts)))]
        planes = np.frombuffer(d, '<f4', npl * 4, pp).reshape(npl, 4).astype(np.float64)
        t = _hull_faces(verts, planes, radius)
        if len(t):
            out.append((KIND_CONVEX, t + offset, -1))
    elif cls == 'hkpBoxShape':
        radius = struct.unpack_from('<f', d, sh + 0x10)[0]
        h = struct.unpack_from('<3f', d, sh + 0x20)
        out.append((KIND_CONVEX, _box_faces(h, radius) + offset, -1))


def asset_bodies(path):
    """[(system index, body name, kind, triangles in the asset's space, part, root kind)] of a *_Havok.res (body
    transforms applied; part: the index into the asset's PartInfoDatas of a mesh piece, -1 for convex pieces; the last
    item: the kind of the body's root shape - what the in-game file's hit vtable names: a MOPP tree over convex pieces
    is a 'mesh' hit there). In the Rust core when it is in use (round F4: bc2_native.navkit_asset_bodies, NumPy's
    small linear algebra bit for bit: bc2_navbuild::hulls), else asset_bodies_reference; a file it does not take is
    the Python's."""
    try:
        with open(path, 'rb') as f:
            data = f.read()
    except OSError:
        return []
    return asset_bodies_of(data)


def asset_bodies_of(data):
    """asset_bodies of a file's bytes."""
    import navnative
    m = navnative.core('navkit_asset_bodies')
    got = m.navkit_asset_bodies(data) if m is not None and type(data) is bytes else None
    if got is None:
        return asset_bodies_of_reference(data)
    return [(si, name, kind, np.frombuffer(t, np.float64).reshape(-1, 3, 3), part, rk)
            for si, name, kind, t, part, rk in got]


def asset_bodies_reference(path):
    """asset_bodies in Python (the reference)."""
    try:
        with open(path, 'rb') as f:
            data = f.read()
    except OSError:
        return []
    return asset_bodies_of_reference(data)


def asset_bodies_of_reference(data):
    """asset_bodies_of in Python (the reference)."""
    try:
        pf = hkpack.Packfile(data)
    except (ValueError, OSError, struct.error):
        return []
    out = []
    for si, sy in enumerate(pf.objects_of('hkpPhysicsSystem')):
        arr, n = _arr(pf, sy + 8)
        for k in range(n):
            b = pf.deref(arr + 4 * k)
            if not b or pf.by_offset.get(b) != 'hkpRigidBody':
                continue
            np_ = pf.deref(b + 0x70)
            name = pf.cstr(np_) if np_ else ''
            m = np.frombuffer(pf.d, '<f4', 16, b + 0xE0).reshape(4, 4).astype(np.float64)
            parts: list[tuple[int, np.ndarray, int]] = []
            try:
                _shape_parts(pf, pf.deref(b + 0x10), np.zeros(3), parts)
            except (struct.error, ValueError, IndexError):
                continue
            root = pf.by_offset.get(pf.deref(b + 0x10))
            rk = KIND_MESH if root in ('hkpMoppBvTreeShape', 'hkpStorageExtendedMeshShape') else KIND_CONVEX
            for kind, t, part in parts:
                out.append((si, name, kind, t @ m[:3, :3] + m[3, :3], part, rk))
    return out


class Geometry:
    """World-space collision triangles of a level (not the terrain): tris (n, 3, 3) float64, kind (n,) uint8
    (KIND_MESH / KIND_CONVEX), solid (n,) int32 (convex: an id per placed convex piece, else -1), body (n,) uint8 (the
    body name's letter), inst (n,) int32 (index into names: the placed object's blueprint), off (n,) bool (a part
    that is not there while the object is intact: its destroyed state), vt (n,) uint8 (the kind of the body's root
    shape, for the hit vtable)."""

    def __init__(self, tris, kind, solid, body, inst, names, off=None, vt=None):
        self.tris, self.kind, self.solid, self.body, self.inst, self.names = tris, kind, solid, body, inst, names
        self.off = off if off is not None else np.zeros(len(tris), bool)
        self.vt = vt if vt is not None else kind.copy()

    def select(self, mask):
        return Geometry(self.tris[mask], self.kind[mask], self.solid[mask], self.body[mask], self.inst[mask], self.names,
                        self.off[mask], self.vt[mask])

    def bodies(self, letters):
        return self.select(np.isin(self.body, np.frombuffer(letters.encode(), np.uint8)))

    def intact(self):
        return self.select(~self.off)


def _xml_text(path):
    try:
        return open(path, encoding='utf-8', errors='replace').read()
    except OSError:
        return ''


def part_names(havok_res):
    """The asset's PartInfoDatas names, in order (from the converted <name>_Havok.xml next to the .res)."""
    t = _xml_text(havok_res[:-4] + '.xml')
    if '<array name="PartInfoDatas">' not in t:
        return []
    body = t.split('<array name="PartInfoDatas">', 1)[1].split('</array>', 1)[0]
    import re
    return [m.lower() for m in re.findall(r'<complex>\s*<field name="Name">([^<]*)</field>', body)]


def destroyed_parts(blueprint_xml):
    """Mesh parts a blueprint shows only once damaged: the MeshNames of its PartComponentData health states other than
    each component's full-health state (the highest Health), minus the full-health ones. (House_06_8x8: 'ruin_downed'
    - the rubble - is the 0-health state of the 'ruin' component.)"""
    import re
    t = _xml_text(blueprint_xml)
    if 'PartComponentData' not in t:
        return set()
    states = {}
    for g, body in re.findall(r'<instance guid="([^"]+)" type="GameSharedResources.HealthStateData">(.*?)</instance>', t, re.S):
        mn = re.search(r'<field name="MeshName">([^<]*)</field>', body)
        hp = re.search(r'<field name="Health">([-0-9.eE]+)</field>', body)
        states[g.lower()] = ((mn.group(1) if mn else '').lower(), float(hp.group(1)) if hp else 0.0)
    shown: set[str]
    later: set[str]
    shown, later = set(), set()
    for body in re.findall(r'type="GameSharedResources.PartComponentData">(.*?)</instance>', t, re.S):
        hs = re.search(r'<array name="HealthStates">(.*?)</array>', body, re.S)
        refs = re.findall(r'<item ref="([^"]+)"', hs.group(1)) if hs else []
        st = [states[r.lower()] for r in refs if r.lower() in states]
        if not st:
            continue
        top = max(st, key=lambda s: s[1])
        if top[0]:
            shown.add(top[0])
        later.update(s[0] for s in st if s is not top and s[0])
    return later - shown


def level_geometry(level_dir, extra_bases=(), log=print, box=None):
    """The collision of a prepared level (work\\levels\\<level>: objects.json, group_instances.json from
    levelobjects.py, the unpacked bundles for the blueprints' *_Havok.res). box (minX, minZ, maxX, maxZ): only
    pieces reaching into it (plus 2 m)."""
    from bfbc2core import collision
    bases = [os.path.join(level_dir, 'pkg'), os.path.join(level_dir, 'dist')] + list(extra_bases)
    # NavKit's levelobjects index and mesh aliases (the Rust core); aliases not read: the index alone
    havok, aliases = collision.index(bases)
    if isinstance(aliases, Exception) and not isinstance(aliases, OSError):
        raise aliases
    for bp, target in ({} if isinstance(aliases, Exception) else aliases).items():
        if bp not in havok and target in havok:
            havok[bp] = havok[target]
    objs = json.load(open(os.path.join(level_dir, 'objects.json')))
    groups = json.load(open(os.path.join(level_dir, 'group_instances.json')))
    cache = {}
    T, K, S, B, I, OFF, V = [], [], [], [], [], [], []
    names: list[str] = []
    name_id: dict[str, int] = {}
    missing = set()
    solid_id = 0
    for o in objs + groups:
        bp = o['bp']
        if bp not in cache:
            cache[bp] = asset_bodies(havok[bp]) if bp in havok else []
            if bp not in havok:
                missing.add(bp)
            elif cache[bp]:
                # parts shown only once the object is damaged (its rubble): marked, left out by intact()
                later = set()
                for base in bases:
                    if os.path.isfile(os.path.join(base, bp + '.xml')):
                        later = destroyed_parts(os.path.join(base, bp + '.xml'))
                        break
                names_ = part_names(havok[bp]) if later else []
                gone = {k for k, nm in enumerate(names_) if nm in later}
                cache[bp] = [(si, nm, kind, tt, part in gone, rk) for si, nm, kind, tt, part, rk in cache[bp]]
        if not cache[bp]:
            continue
        if len(cache[bp][0]) == 6 and not isinstance(cache[bp][0][4], bool):
            cache[bp] = [(si, nm, kind, tt, False, rk) for si, nm, kind, tt, _p, rk in cache[bp]]
        m = np.array(o['m'], dtype=np.float64).reshape(4, 4)
        if box is not None:
            p = m[3, :3]
            if not (box[0] - 300 <= p[0] <= box[2] + 300 and box[1] - 300 <= p[2] <= box[3] + 300):
                continue
        iid = name_id.setdefault(bp, len(names))
        if iid == len(names):
            names.append(bp)
        for _si, name, kind, t, gone, rk in cache[bp]:
            w = t @ m[:3, :3] + m[3, :3]
            if box is not None:
                keep = ((w[:, :, 0].max(1) >= box[0] - 2) & (w[:, :, 0].min(1) <= box[2] + 2) &
                        (w[:, :, 2].max(1) >= box[1] - 2) & (w[:, :, 2].min(1) <= box[3] + 2))
                w = w[keep]
                if not len(w):
                    continue
            T.append(w)
            K.append(np.full(len(w), kind, np.uint8))
            if kind == KIND_CONVEX:
                S.append(np.full(len(w), solid_id, np.int32))
                solid_id += 1
            else:
                S.append(np.full(len(w), -1, np.int32))
            B.append(np.full(len(w), ord(name[:1] or '?'), np.uint8))
            I.append(np.full(len(w), iid, np.int32))
            OFF.append(np.full(len(w), bool(gone)))
            V.append(np.full(len(w), rk, np.uint8))
    if not T:
        return Geometry(np.zeros((0, 3, 3)), np.zeros(0, np.uint8), np.zeros(0, np.int32), np.zeros(0, np.uint8),
                        np.zeros(0, np.int32), names)
    g = Geometry(np.concatenate(T), np.concatenate(K), np.concatenate(S), np.concatenate(B), np.concatenate(I), names,
                 np.concatenate(OFF), np.concatenate(V))
    log(f'  geometry: {len(objs)} objects, {len(groups)} group members, {len(g.tris):,} triangles '
        f'({int((g.kind == KIND_CONVEX).sum()):,} convex, {int(g.off.sum()):,} in destroyed-state parts); bodies '
        + ', '.join(f'{chr(c)} {int((g.body == c).sum()):,}' for c in np.unique(g.body))
        + f'; {len(missing)} blueprints without a collision asset')
    return g


# --- the grid --------------------------------------------------------------------------------------------------------
class Grid:
    def __init__(self, pitch, min_x, min_z, nx, nz, y_bottom, y_top, mask=None):
        self.pitch, self.min_x, self.min_z, self.nx, self.nz = float(pitch), float(min_x), float(min_z), int(nx), int(nz)
        self.y_bottom, self.y_top = float(y_bottom), float(y_top)
        self.mask = mask          # bool (nz, nx): columns to sample (None: all)

    @staticmethod
    def from_box(pitch, box, y, rects=None):
        """As navsample.cpp's Begin(): aligned to whole multiples of the pitch."""
        p = np.float32(pitch)
        min_x = np.float32(math.floor(np.float32(box[0]) / p)) * p
        min_z = np.float32(math.floor(np.float32(box[1]) / p)) * p
        nx = int(math.ceil((np.float32(box[2]) - min_x) / p)) + 1
        nz = int(math.ceil((np.float32(box[3]) - min_z) / p)) + 1
        g = Grid(p, min_x, min_z, nx, nz, y[0], y[1])
        if rects:
            xs = g.min_x + np.arange(nx) * g.pitch
            zs = g.min_z + np.arange(nz) * g.pitch
            # navsample.cpp's mask: a 1 m grid from the box's min; a column counts when its 1 m cell is in a rectangle
            w, h = int(math.ceil(box[2] - g.min_x)) + 2, int(math.ceil(box[3] - g.min_z)) + 2
            m1 = np.zeros((h, w), bool)
            for r in rects:
                i0, j0 = max(0, int(math.floor(r[0] - g.min_x))), max(0, int(math.floor(r[1] - g.min_z)))
                i1, j1 = min(w - 1, int(math.ceil(r[2] - g.min_x))), min(h - 1, int(math.ceil(r[3] - g.min_z)))
                m1[j0:j1 + 1, i0:i1 + 1] = True
            ii = np.floor(xs - g.min_x).astype(int)
            jj = np.floor(zs - g.min_z).astype(int)
            okx = (ii >= 0) & (ii < w)
            okz = (jj >= 0) & (jj < h)
            mask = np.zeros((nz, nx), bool)
            mask[np.ix_(okz, okx)] = m1[np.ix_(jj[okz], ii[okx])]
            g.mask = mask
        return g

    @staticmethod
    def like(s):
        """The grid of an in-game sample file (navsample.Samples); columns it left empty under navsample_rects stay
        sampled here (compare() looks at the columns both have)."""
        return Grid(s.pitch, s.min_x, s.min_z, s.nx, s.nz, s.y_bottom, s.y_top)

    @property
    def ncols(self):
        return self.nx * self.nz

    def xz(self, cols):
        return (self.min_x + (cols % self.nx) * self.pitch, self.min_z + (cols // self.nx) * self.pitch)


# --- terrain ---------------------------------------------------------------------------------------------------------
def _rings():
    pts = [(0.0, 0.0)]
    for r in (0.1, 0.2, 0.3, 0.35, 0.4):
        n = 8 if r < 0.25 else 16
        for k in range(n):
            a = 2 * math.pi * k / n
            pts.append((r * math.cos(a), r * math.sin(a)))
    return np.array(pts)


RINGS = _rings()


def capsule_axis(bottom, radius=RADIUS, height=HEIGHT):
    """Heights above the surface of the capsule's lowest and highest sphere centres: CAPSULE_AXIS for the two runs
    navkit.py makes, else navsample.cpp's nominal capsule (bottom + radius .. height - radius)."""
    return CAPSULE_AXIS.get(round(float(bottom), 2), (bottom + radius, height - radius))


def terrain_blocks(hf, x, z, y, bottom, radius=RADIUS, chunk=100_000, height=HEIGHT):
    """Capsule (radius, stepMax bottom: capsule_axis) overlaps the height field: the terrain under its lower hemisphere
    rises above the hemisphere's surface somewhere (sampled on RINGS). In the Rust core (round E4e: half of a 0.2 m
    run's time; bc2_native.navkit_terrain_blocks, every surface in parallel), which it needs (bfbc2core.native.
    NativeMissing says how to get it). A height field that is not a float32 size x size array raises
    bfbc2core.native.Refused. Its NumPy (terrain_blocks_reference) was retired in round RT2 against its recorded
    answers (bfbc2-auto-navkit/tests/e4e_navtools_replay.json)."""
    if hf is None or not len(y):
        return np.zeros(len(y), bool)
    import navnative
    from bfbc2core import native
    m = navnative.core('navkit_terrain_blocks') or native.require(
        'the offline sampler\'s terrain test (navsample_offline.terrain_blocks)', 'navkit_terrain_blocks')
    heights = getattr(hf, 'heights', None)
    if isinstance(heights, np.ndarray) and heights.dtype == np.float32 and heights.ndim == 2 \
            and heights.shape == (hf.size, hf.size):
        d = np.hypot(RINGS[:, 0], RINGS[:, 1])
        lift = capsule_axis(bottom, radius, height)[0] - np.sqrt(np.maximum(radius * radius - d * d, 0))
        cx, cz = hf.cell
        f8 = lambda v: np.ascontiguousarray(np.broadcast_to(np.asarray(v, np.float64), (len(y),)), '<f8').tobytes()
        got = m.navkit_terrain_blocks(np.ascontiguousarray(heights, '<f4').tobytes(), int(hf.size), float(hf.min[0]),
                                      float(hf.min[2]), float(cx), float(cz), f8(x), f8(z), f8(y),
                                      [float(v) for v in RINGS[:, 0]], [float(v) for v in RINGS[:, 1]],
                                      [float(v) for v in lift])
        if got is not None:
            return np.frombuffer(got, np.uint8).astype(bool)
    raise native.refused('terrain_blocks: a height field that is not a float32 size x size array, or positions of '
                         'different lengths')


# --- one sampling run ------------------------------------------------------------------------------------------------
def sample_grid(grid, geom, hf, log=print, bottoms=(0.28, 0.6)):
    """Samples a grid: {bottom: navsample.Samples} for each capsule bottom (one cast, one capsule test per bottom). In
    the Rust core (round NKS-deep C: bc2_native.navkit_offline_sample, bc2_navbuild::sampler - the rasterised casts,
    the cast loop, the capsule and terrain tests, bit for bit the NumPy they replaced: the 15 regression levels'
    files identical at 0.8 m, sp_009 / sp_004_b / ons_mp_004 at 0.2 m and on in-game grids), which it needs
    (bfbc2core.native.NativeMissing says how to get it)."""
    import navnative
    from bfbc2core import native
    m = navnative.core('navkit_offline_sample') or native.require(
        'the offline sampler (navsample_offline.sample_grid)', 'navkit_offline_sample')
    t0 = time.time()
    heights, size, hf_min, cell = None, 0, (0.0, 0.0), (1.0, 1.0)
    if hf is not None:
        heights = np.ascontiguousarray(hf.heights, '<f4').tobytes()
        size, hf_min, cell = int(hf.size), (float(hf.min[0]), float(hf.min[2])), (float(hf.cell[0]), float(hf.cell[1]))
    mask = np.ascontiguousarray(grid.mask, np.uint8).tobytes() if grid.mask is not None else None
    counts, y, nrm, kind, flags = m.navkit_offline_sample(
        np.ascontiguousarray(geom.tris, '<f8').tobytes(), np.ascontiguousarray(geom.kind, np.uint8).tobytes(),
        np.ascontiguousarray(geom.solid, '<i4').tobytes(), np.ascontiguousarray(geom.vt, np.uint8).tobytes(),
        heights, size, hf_min, cell,
        (float(grid.pitch), float(grid.min_x), float(grid.min_z), int(grid.nx), int(grid.nz), float(grid.y_bottom),
         float(grid.y_top)), mask, [float(b) for b in bottoms])
    start = np.zeros(grid.ncols + 1, np.int64)
    np.cumsum(np.frombuffer(counts, np.uint8), out=start[1:])
    Y = np.frombuffer(y, np.float32).copy()
    nq = (np.frombuffer(nrm, np.int8).reshape(-1, 3) / 127.0).astype(np.float32)
    hit = np.zeros((len(Y), 3), np.uint32)
    hit[:, 0] = VTABLE_OF_KIND[np.frombuffer(kind, np.uint8)]
    hit[:, 1] = RAY_SHAPE_VTABLE
    out = {}
    for b, fl in zip(bottoms, flags):
        # the file's float32 header values (Samples declares float)
        out[b] = navsample.Samples(cast(float, np.float32(grid.pitch)), cast(float, np.float32(grid.min_x)),
                                   cast(float, np.float32(grid.min_z)), grid.nx,
                                   grid.nz, grid.y_top, grid.y_bottom, RADIUS, b, HEIGHT, RAY_RADIUS, start,
                                   Y, nq, np.frombuffer(fl, np.uint8).copy(), hit)
    log(f'  sampled {grid.nx} x {grid.nz} columns at {grid.pitch:g} m: {len(Y):,} surfaces, '
        + ', '.join(f'blocked {int(out[b].flags.sum()):,} (capsule from {b})' for b in bottoms)
        + f' ({time.time() - t0:.0f} s)')
    return out


def write(path, s):
    """navsample.cpp's version-3 file (file_bytes), written through a temporary file."""
    os.makedirs(os.path.dirname(path) or '.', exist_ok=True)
    data = file_bytes(s)
    tmp = path + '.tmp'
    with open(tmp, 'wb') as f:
        f.write(data)
    os.replace(tmp, path)


def _write_args(s):
    """navkit_navsample_write's arguments for s, or None: an array of another dtype or shape (the reference casts or
    broadcasts it), or a header value float() / int() refuses (the reference raises its error)."""
    try:
        def arr(a, dt, shape):
            return (isinstance(a, np.ndarray) and a.dtype == np.dtype(dt) and a.shape == shape
                    and a.dtype.byteorder in '=<|')
        n = len(s.y) if isinstance(s.y, np.ndarray) and s.y.ndim == 1 else -1
        hit = s.hit if s.hit is not None and len(s.hit) else None
        if (n < 0 or not isinstance(s.start, np.ndarray) or s.start.ndim != 1 or not arr(s.start, np.int64, s.start.shape)
                or not arr(s.y, np.float32, (n,)) or not arr(s.normal, np.float32, (n, 3))
                or not arr(s.flags, np.uint8, (n,)) or (hit is not None and not arr(hit, np.uint32, (n, 3)))):
            return None
        head = (float(s.pitch), float(s.min_x), float(s.min_z), int(s.nx), int(s.nz), float(s.y_top),
                float(s.y_bottom), float(s.radius), float(s.cap_bottom), float(s.cap_top), float(s.ray_radius))
        if not all(-(1 << 63) <= v < (1 << 63) for v in head[3:5]):
            return None
        return (head, np.ascontiguousarray(s.start).tobytes(), np.ascontiguousarray(s.y).tobytes(),
                np.ascontiguousarray(s.normal).tobytes(), np.ascontiguousarray(s.flags).tobytes(),
                None if hit is None else np.ascontiguousarray(hit).tobytes())
    except Exception:                                   # noqa: BLE001 - the reference raises its own error
        return None


def file_bytes(s):
    """The version-3 file's bytes, written in the Rust core (round F3: navkit_navsample_write; its Python twin was
    retired in round RT1, its answers kept in tests\\f3_golden_navsample.json). Samples the writer does not take - a
    column of more than 255 surfaces, a NaN normal, an array of another dtype or shape (y, normal: float32; flags:
    uint8; start: int64; hit: uint32, n x 3), a header value out of range - raise ValueError."""
    from bfbc2core import native
    nat = native.require('writing navsample files (round F3)', 'navkit_navsample_write')
    start = getattr(s, 'start', None)
    if isinstance(start, np.ndarray) and len(start) > 1 and np.diff(start).max() > 255:
        raise ValueError('more than 255 surfaces in a column')
    args = _write_args(s)
    got = nat.navkit_navsample_write(*args) if args is not None else None
    if got is None:
        raise ValueError('the samples cannot be written: a NaN normal, an array of another dtype or shape (y, normal: '
                         'float32; flags: uint8; start: int64; hit: uint32, n x 3) or a header value out of range')
    return got


# --- comparison with an in-game file ---------------------------------------------------------------------------------
def compare(a, b, tol=0.1, slope=40.0, log=print, both_only=True):
    """Per column agreement of two sample files on the same grid (a: in game, b: offline). Columns empty in a (outside
    the in-game run's rectangles) are left out when both_only. Returns a dict of measures."""
    if (a.nx, a.nz) != (b.nx, b.nz) or abs(a.min_x - b.min_x) > 1e-3 or abs(a.min_z - b.min_z) > 1e-3:
        raise ValueError('not the same grid')
    na, nb = np.diff(a.start), np.diff(b.start)
    cols = np.nonzero(na > 0)[0] if both_only else np.arange(a.nx * a.nz)
    r: dict[str, float] = {'columns': int(len(cols)), 'surfaces_a': int(na[cols].sum()), 'surfaces_b': int(nb[cols].sum())}
    r['layers_equal'] = float(np.mean(na[cols] == nb[cols]) * 100)
    # top surface height
    both = cols[(na[cols] > 0) & (nb[cols] > 0)]
    dy = np.abs(a.y[a.start[both]] - b.y[b.start[both]])
    r['top_within_0.1'] = float(np.mean(dy <= tol) * 100)
    r['top_median_mm'] = float(np.median(dy) * 1000) if len(dy) else float('nan')
    r['top_missing_b'] = float(np.mean(nb[cols] == 0) * 100)
    # every surface of a: is there one in b's column within tol? (and vice versa)
    def matched(x, y_, cols_):
        ca = np.repeat(np.arange(x.nx * x.nz), np.diff(x.start))
        m = np.zeros(len(x.y), bool)
        sel = np.isin(ca, cols_)
        idx = np.nonzero(sel)[0]
        cc, yy = ca[idx], x.y[idx].astype(float)
        K, off = 4096.0, 1000.0
        cb = np.repeat(np.arange(y_.nx * y_.nz), np.diff(y_.start))
        kb = cb * K + (y_.y.astype(float) + off)
        o = np.argsort(kb)
        kb = kb[o]
        lo = np.searchsorted(kb, cc * K + (yy - tol + off), 'left')
        hi = np.searchsorted(kb, cc * K + (yy + tol + off), 'right')
        m[idx] = hi > lo
        partner = np.full(len(x.y), -1, np.int64)
        partner[idx[hi > lo]] = o[lo[hi > lo]]
        return m, partner, sel
    ma, pa, sa = matched(a, b, cols)
    mb, _, sb = matched(b, a, cols)
    r['surf_a_found'] = float(ma[sa].mean() * 100) if sa.any() else float('nan')
    r['surf_b_real'] = float(mb[sb].mean() * 100) if sb.any() else float('nan')
    wa = navsample.walkable(a, slope)
    wb = navsample.walkable(b, slope)
    # walkable mask: per in-game surface, walkable in both (b's matched surface)
    k = np.nonzero(sa & ma)[0]
    agree = wa[k] == wb[pa[k]]
    r['walk_agree_matched'] = float(agree.mean() * 100) if len(k) else float('nan')
    ka = np.nonzero(sa & wa)[0]
    r['walk_a_found_walk_b'] = float(np.mean(ma[ka] & wb[np.maximum(pa[ka], 0)]) * 100) if len(ka) else float('nan')
    kb_ = np.nonzero(sb & wb)[0]
    mbb, pbb, _ = matched(b, a, cols)
    r['walk_b_real_walk_a'] = float(np.mean(mbb[kb_] & wa[np.maximum(pbb[kb_], 0)]) * 100) if len(kb_) else float('nan')
    blk_a = (a.flags[k] & 1) > 0
    blk_b = (b.flags[pa[k]] & 1) > 0
    r['blocked_agree_matched'] = float(np.mean(blk_a == blk_b) * 100) if len(k) else float('nan')
    r['blocked_a'] = int(blk_a.sum())
    r['blocked_b'] = int(blk_b.sum())
    r['blocked_both'] = int((blk_a & blk_b).sum())
    terr_a = a.hit[:, 0] == TERRAIN_SHAPE_VTABLE if a.hit is not None else np.ones(len(a.y), bool)
    obj = sa & ~terr_a
    r['object_surf_a'] = int(obj.sum())
    r['object_surf_a_found'] = float(ma[obj].mean() * 100) if obj.any() else float('nan')
    for kk, v in r.items():
        log(f'  {kk:24s} {v:.2f}' if isinstance(v, float) else f'  {kk:24s} {v:,}')
    return r


# --- level driver ----------------------------------------------------------------------------------------------------
def out_dir(root):
    return os.path.join(root, 'work', 'navsample_offline')


def sample_path(root, lv, suffix=''):
    return os.path.join(out_dir(root), f'{lv}_navkit{suffix}.bin')


def common_bases(root):
    return [os.path.join(root, 'work', 'navkit', 'common', c) for c in ('sp_common', 'mp_common')]


def load_level(root, lv, box=None, log=print, bodies=None):
    from bfbc2core import level as levelmod
    L = os.path.join(root, 'work', 'levels', lv)
    geom = level_geometry(L, common_bases(root), log=log, box=box)
    geom = geom.bodies(bodies or BODIES)
    if INTACT_ONLY:
        geom = geom.intact()
    hf = levelmod.heightfield_of(root, lv, log=log)
    return geom, hf


def run_level(root, lv, pitch=0.8, box=None, y=None, log=print, bodies=None, rects=None, names=None, loaded=None):
    """Both capsule runs of one pitch for a prepared level, written as the in-game sampler names them (in the offline
    folder). loaded: (geometry, height field) already read. Returns the paths."""
    import navkit
    pr = navkit.Project(root)
    s = navkit.ensure_settings(pr, lv)
    box = box or s['box']
    y = y or s['y']
    grid = Grid.from_box(pitch, box, y, rects)
    log(f'== offline sample {lv}: box {box}, y {y}, pitch {pitch} -> {grid.nx} x {grid.nz} columns'
        + (f', {int(grid.mask.sum()):,} in {len(rects)} rectangles' if grid.mask is not None else ''))
    geom, hf = loaded or load_level(root, lv, box=box, log=log, bodies=bodies)
    res = sample_grid(grid, geom, hf, log=log, bottoms=tuple(CAP_BOTTOMS.values()))
    ps = '' if abs(pitch - 0.8) < 1e-6 else f'_p{int(round(pitch * 10)):02d}'
    paths = []
    for suf, b in CAP_BOTTOMS.items():
        p = (names or {}).get(suf) or sample_path(root, lv, suf + ps)
        write(p, res[b])
        paths.append(p)
        log(f'  wrote {p}')
    return paths


def detailed_rects(root, lv, ids, margin=1.0):
    """The detailed sectors' squares plus the sampling margin (navkit.detailed_rects) and their box."""
    import navkit
    st = navkit.ensure_settings(navkit.Project(root), lv)
    rects = [[(i + st['xoff']) * 100 - margin, (j + st['zoff']) * 100 - margin, (i + st['xoff']) * 100 + 100 + margin,
              (j + st['zoff']) * 100 + 100 + margin] for i, j in ids]
    box = [min(r[0] for r in rects), min(r[1] for r in rects), max(r[2] for r in rects), max(r[3] for r in rects)]
    return rects, box


def sample_level(root, lv, log=print, bodies=None):
    """What navkit.py sample does in game, offline: the 0.8 m pair over the level's box, then - when the level has
    detailed sectors (chosen from those samples, detailed.py) - the 0.2 m pair over their squares. The geometry is
    read once. Returns {'p08': paths, 'p02': paths or None, 'detailed': ids}."""
    import navkit
    s = navkit.ensure_settings(navkit.Project(root), lv)
    loaded = load_level(root, lv, box=s['box'], log=log, bodies=bodies)
    p08 = run_level(root, lv, 0.8, log=log, loaded=loaded)
    ids, _why = detailed_choice(root, lv, p08[0])
    out = {'p08': p08, 'p02': None, 'detailed': [list(i) for i in ids]}
    for suf in ('', '_hi'):                      # a stale 0.2 m pair would be built in by build_level
        old = sample_path(root, lv, suf + '_p02')
        if os.path.isfile(old):
            os.remove(old)
    if ids:
        rects, box = detailed_rects(root, lv, ids)
        # the geometry over the detailed box (NAVS-SMP-2: a detailed sector outside the level's box keeps its objects)
        out['p02'] = run_level(root, lv, 0.2, box=box, log=log, rects=rects)
    else:
        log(f'  {lv}: no detailed sectors - no 0.2 m runs')
    return out


# --- stock multiplayer maps ------------------------------------------------------------------------------------------
STOCK_MP = ['mp_001', 'mp_001sr', 'mp_002', 'mp_002sr', 'mp_003', 'mp_004', 'mp_004sdm', 'mp_005', 'mp_005sr', 'mp_006',
            'mp_006cq', 'mp_006sdm', 'mp_007', 'mp_007sdm', 'mp_008', 'mp_009cq', 'mp_009gr', 'mp_009sdm', 'mp_012gr',
            'mp_012sr']
COMBAT_MARGIN = 50.0     # sampling box of a map without path data: its combat areas' extent plus this (m)


def combat_box(root, lv, margin=COMBAT_MARGIN):
    """(minX, minZ, maxX, maxZ) of the level's combat areas (CombatAreaTriggerEntityData outlines, every team and
    stage) plus margin, on 10 m; None without combat areas. On a stock MP map the objects' extent (the box navkit.py
    prepare takes without path data) holds backdrop scenery kilometres away (MP_005: 4.7 x 4.0 km); the soldiers'
    ground is the combat area (MP_005: 0.77 x 0.77 km). ONS_MP_002 (Valparaiso, DICE's console path data): combat
    areas x -108..290, z -494..374; DICE's ground x -110..440, z -530..370 (150 m past the combat area on x)."""
    from bfbc2core import level as levelmod
    L = levelmod.load(lv, opfor=root, geometry=False, log=lambda *a: None)
    pts = [s.points for s in L.shapes if L.entities[s.entity].category == 'combat area' and s.kind == 'area']
    if not pts:
        return None
    P = np.concatenate(pts)
    return [math.floor((P[:, 0].min() - margin) / 10) * 10, math.floor((P[:, 2].min() - margin) / 10) * 10,
            math.ceil((P[:, 0].max() + margin) / 10) * 10, math.ceil((P[:, 2].max() + margin) / 10) * 10]


def package_overlay(pr, lv, log=print):
    """The game loads a level's Package bundle over its Dist bundle, file by file. Every stock MP map has its level
    partition and static-group assets in Package (game patches: MP_002 4,067,601 bytes there, 4,023,001 in Dist;
    MP_002SR's partition is in Package only), but navkit.py prepare and levelobjects.py read the Dist copy. This unpacks
    both (as prepare does) and copies Package's levels\\ files over the Dist copy in the work folder, so every NavKit
    step sees the level the game loads. The copied files are listed in work\\levels\\<lv>\\pkg_overlay.txt."""
    import shutil
    L = pr.level_work(lv)
    pr.unpack(os.path.join(pr.level_dirs()[lv], 'level-00.fbrb'), os.path.join(L, 'dist'), log)
    pkg = os.path.join(pr.game, 'Package', 'levels', os.path.basename(pr.level_dirs()[lv]), 'level-00.fbrb')
    if not os.path.isfile(pkg):
        return []
    pr.unpack(pkg, os.path.join(L, 'pkg'), log)
    src = os.path.join(L, 'pkg', 'levels')
    copied = []
    for dp, _, fs in os.walk(src):
        for f in fs:
            rel = os.path.relpath(os.path.join(dp, f), src)
            dst = os.path.join(L, 'dist', 'levels', rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.copyfile(os.path.join(dp, f), dst)
            copied.append(rel)
    open(os.path.join(L, 'pkg_overlay.txt'), 'w', encoding='utf-8').write('\n'.join(sorted(copied)) + '\n')
    log(f'  Package overlay: {len(copied)} level files copied over the Dist copy')
    return copied


def prepare_mp(root, lv, log=print):
    """navkit.py prepare (offline: unpack, convert, objects), then - on a level without path data - the box set to
    its combat areas (combat_box) and the sector offsets by navkit's rule for such a level (one sector below the
    box)."""
    import navkit
    pr = navkit.Project(root)
    if not pr.dice_pathdata(lv):
        package_overlay(pr, pr.level(lv), log=log)
    navkit.prepare(pr, pr.level(lv), log=log)
    st = navkit.ensure_settings(pr, lv)
    if pr.dice_pathdata(lv):
        return st
    box = combat_box(root, lv)
    if box:
        st['box'], st['box_source'] = box, f'combat areas + {COMBAT_MARGIN:g} m (navsample_offline.combat_box)'
        st['xoff'], st['zoff'] = int(math.floor(box[0] / 100.0)) - 1, int(math.floor(box[1] / 100.0)) - 1
        st['offsets_source'] = 'rule: box'
        json.dump(st, open(pr.settings_path(lv), 'w'), indent=1)
        log(f'  box {box} ({st["box_source"]}), sector offsets {st["xoff"]}, {st["zoff"]}')
    return st


ALL_STEPS = ('prepare', 'sample', 'build', 'check')


def stock_mp(root, levels=None, log=print, steps=ALL_STEPS):
    """The whole offline pipeline on the stock MP maps, one after another; progress and timings in
    work\\navkit_offline\\stockmp.json (kept between runs; a level's entry is replaced when all its steps run again,
    updated when some do)."""
    levels = levels or STOCK_MP
    rep_path = os.path.join(root, 'work', 'navkit_offline', 'stockmp.json')
    os.makedirs(os.path.dirname(rep_path), exist_ok=True)
    report = json.load(open(rep_path)) if os.path.isfile(rep_path) else {}
    for lv in levels:
        r = {} if tuple(steps) == ALL_STEPS else report.get(lv, {})
        r.setdefault('steps', {})
        r.pop('error', None)
        log(f'##### {lv} ({time.strftime("%H:%M:%S")})')
        try:
            for step in steps:
                t0 = time.time()
                if step == 'prepare':
                    st = prepare_mp(root, lv, log=log)
                    r['box'], r['offsets'] = st['box'], [st['xoff'], st['zoff']]
                elif step == 'sample':
                    out = sample_level(root, lv, log=log)
                    r['detailed_sectors'] = len(out['detailed'])
                elif step == 'build':
                    r['pathdata'] = build_level(root, lv, log=log)
                    r['size_mb'] = round(os.path.getsize(r['pathdata']) / 1e6, 2)
                elif step == 'check':
                    path = r.get('pathdata') or os.path.join(root, 'work', 'navkit_offline', lv, 'pathdata.hpd')
                    res = check_level(root, lv, path, log=log)
                    r['check'] = {k: [v[0], v[1]] for k, v in res.items()}
                r['steps'][step] = round(time.time() - t0, 1)
                report[lv] = r
                json.dump(report, open(rep_path, 'w'), indent=1)
            r['ok'] = True
        except Exception as e:           # one map's failure does not stop the batch
            import traceback
            r['ok'] = False
            r['error'] = f'{type(e).__name__}: {e}'
            log(traceback.format_exc())
        report[lv] = r
        json.dump(report, open(rep_path, 'w'), indent=1)
    return report


def ingame_files(root, lv):
    """The level's in-game sample files as navkit.py build takes them: {suffix: path} for '', '_hi', '_p02',
    '_hi_p02' ('' and '_hi': <lv>_navkit.bin or the older <lv>_full.bin)."""
    d = os.path.join(root, 'work', 'navsample')
    out = {}
    for base in (f'{lv}_navkit', f'{lv}_full'):
        if os.path.isfile(os.path.join(d, base + '.bin')) and os.path.isfile(os.path.join(d, base + '_hi.bin')):
            out[''], out['_hi'] = os.path.join(d, base + '.bin'), os.path.join(d, base + '_hi.bin')
            break
    for suf in ('_p02', '_hi_p02'):
        q = os.path.join(d, f'{lv}_navkit{suf}.bin')
        if os.path.isfile(q):
            out[suf] = q
    return out


def parity(root, levels, name='offline', log=print):
    """For levels with in-game samples (and DICE path data): offline samples on the in-game files' grids, NavKit
    built from each, and the measures: navdiff of the offline build against the in-game build (same code, only the
    samples differ), and navparity of both against DICE's file. Saved as work\\navkit_offline\\parity_<name>.json."""
    import navdiff
    import navparity
    import navregress
    out_path = os.path.join(root, 'work', 'navkit_offline', f'parity_{name}.json')
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    report = json.load(open(out_path)) if os.path.isfile(out_path) else {}
    for lv in levels:
        t0 = time.time()
        files = ingame_files(root, lv)
        if '' not in files:
            log(f'{lv}: no in-game samples - skipped')
            continue
        like = os.path.join(out_dir(root), 'like', lv)
        os.makedirs(like, exist_ok=True)
        geom = hf = None
        val = {}
        for suf, f in files.items():
            s, _ = navsample.load(f)
            grid = Grid.like(s)
            na = np.diff(s.start)
            if (na == 0).mean() > 0.5:
                grid.mask = (na > 0).reshape(grid.nz, grid.nx)
            if geom is None:
                geom, hf = load_level(root, lv, log=log)
            res = sample_grid(grid, geom, hf, log=log, bottoms=(float(s.cap_bottom),))[float(s.cap_bottom)]
            write(os.path.join(like, f'{lv}_navkit{suf}.bin'), res)
            log(f'  {lv}{suf}: offline against in game')
            val[suf or '_08'] = compare(s, res, log=lambda m: log('  ' + m))
        t_sample = time.time() - t0
        cdir = os.path.join(root, 'work', 'navkit_offline', 'compare')
        os.makedirs(cdir, exist_ok=True)
        off_hpd = os.path.join(cdir, f'{lv}_offline.hpd')
        ing_hpd = os.path.join(cdir, f'{lv}_ingame.hpd')
        t1 = time.time()
        build_level(root, lv, out=off_hpd, log=lambda *a: None, samples_dir=like)
        t_off = time.time() - t1
        t1 = time.time()
        navregress.build_level(root, lv, ing_hpd)
        t_ing = time.time() - t1
        d, _, _, _ = navdiff.diff(ing_hpd, off_hpd, log=lambda *a: None)
        row = {'validation': val, 'seconds': {'offline_samples': round(t_sample), 'build_offline': round(t_off),
                                             'build_ingame': round(t_ing)},
               'offline_vs_ingame': {'ground': d['cells']['parity'],
                                     'extra': round(100.0 * d['cells']['only_candidate'] / max(d['cells']['reference'], 1), 2),
                                     'areas_same': d['areas']['parity'], 'nodes_same': d['graph']['node_parity'],
                                     'edges_same': d['graph']['edge_parity']}}
        if navparity.reference_of(root, lv):
            row['dice_offline'] = navparity.run_level(root, lv, off_hpd, log)
            row['dice_ingame'] = navparity.run_level(root, lv, ing_hpd, log)
        report[lv] = row
        json.dump(report, open(out_path, 'w'), indent=1)
        log(f'{lv}: offline build against the in-game build: ' + ', '.join(f'{k} {v}' for k, v in row['offline_vs_ingame'].items()))
        for k in ('dice_offline', 'dice_ingame'):
            if k in row:
                r = row[k]
                log(f'  {k}: ground {r["ground"]} extra {r["extra_ground"]} connectivity {r["connectivity"]} '
                    f'over {r["over_connection"]} AI {r["ai_coverage"]} pairwise {r["ai_pairwise"]}')
    log(f'wrote {out_path}')
    return report


def detailed_choice(root, lv, s08):
    """navkit.detailed_sectors with the offline 0.8 m samples (tools\\detailed.py)."""
    import detailed
    import navkit
    pr = navkit.Project(root)
    st = navkit.ensure_settings(pr, lv)
    chosen, why = detailed.choose(pr.level_work(lv), common_bases(root)[::-1], s08, st['xoff'], st['zoff'])
    return [tuple(c) for c in chosen], why


def build_level(root, lv, out=None, log=print, samples_dir=None):
    """navkit.py build with the offline samples: the same inputs (level objects, spawns, water, orders, ladders), the
    0.8 m pair and, when present, the 0.2 m pair over the detailed sectors chosen from the offline 0.8 m samples."""
    import navkit
    import navregress
    import navwalk
    pr = navkit.Project(root)
    s = navkit.ensure_settings(pr, lv)
    L = pr.level_work(lv)
    d = samples_dir or out_dir(root)
    s08 = os.path.join(d, f'{lv}_navkit.bin')
    if not os.path.isfile(s08) or not os.path.isfile(s08[:-4] + '_hi.bin'):
        raise RuntimeError(f'{lv}: offline samples missing ({s08} and its _hi run)')
    out = out or os.path.join(root, 'work', 'navkit_offline', lv, 'pathdata.hpd')
    os.makedirs(os.path.dirname(out), exist_ok=True)
    detailed = None
    p02 = os.path.join(d, f'{lv}_navkit_p02.bin')
    if os.path.isfile(p02):
        ids, _ = detailed_choice(root, lv, s08)
        hi02 = os.path.join(d, f'{lv}_navkit_hi_p02.bin')
        detailed = dict(samples=p02, high=hi02 if os.path.isfile(hi02) else None, sectors=set(ids))
        log(f'  detailed zones: {len(ids)} sectors at pitch 0.2')
    t0 = time.time()
    lad = os.path.join(L, 'ladders.json')
    navwalk.build(s08, os.path.join(L, 'forbid_tris.npy'), s['xoff'], s['zoff'], out, detailed=detailed,
                   dynamic_path=os.path.join(L, 'dynamic_tris.npy'), high_path=s08[:-4] + '_hi.bin',
                   stairs_path=os.path.join(L, 'stair_tris.npy'),
                   seeds=json.load(open(os.path.join(L, 'soldier_spawns.json'))),
                   water=navregress.level_water(root, lv), bounds=navregress.build_bounds(root, lv),
                   order_points=navregress.order_points_of(root, lv), area_orders=navregress.area_orders_of(root, lv),
                   ladders=json.load(open(lad)) if os.path.isfile(lad) else None,
                   log=lambda *a: log('  ' + ' '.join(str(x) for x in a)))
    log(f'  built {out} in {time.time() - t0:.0f} s ({os.path.getsize(out) / 1e6:.1f} MB)')
    return out


def check_level(root, lv, path, log=print):
    """hpdcheck, byte-exact roundtrip / meshtrip, and ordercheck (self-check without DICE data) of a build."""
    import subprocess

    import navkit
    pr = navkit.Project(root)
    s = navkit.ensure_settings(pr, lv)
    res: dict[str, tuple[int, str | list[str]]] = {}
    for name, args in (('hpdcheck', [os.path.join(HERE, 'hpdcheck.py'), path]),
                       ('roundtrip', [os.path.join(HERE, 'hpdtool.py'), 'roundtrip', path]),
                       ('meshtrip', [os.path.join(HERE, 'hpdtool.py'), 'meshtrip', path])):
        r = subprocess.run([sys.executable] + args, capture_output=True, text=True, errors='replace')
        tail = (r.stdout + r.stderr).strip().splitlines()[-1:] or ['']
        res[name] = (r.returncode, tail[0])
        log(f'  {name}: exit {r.returncode}: {tail[0]}')
    xml = os.path.join(pr.level_work(lv), 'dist', 'levels', s['level'] + '.xml')
    ref = pr.dice_pathdata(lv)
    r = subprocess.run([sys.executable, os.path.join(HERE, 'ordercheck.py'), xml] + ([ref] if ref else []) + [path],
                       capture_output=True, text=True, errors='replace')
    out = (r.stdout + r.stderr).strip().splitlines()
    open(os.path.join(os.path.dirname(path), 'check.txt'), 'w', encoding='utf-8').write('\n'.join(out) + '\n')
    res['ordercheck'] = (r.returncode, [l for l in out if not l.startswith('  ')][-6:])
    for l in res['ordercheck'][1]:
        log('  ' + l)
    return res


# --- an export's stage --------------------------------------------------------------------------------------------
def stage_tree(root, lv):
    """A staged level's NavKit folder (the export's path-data step reads it), in OPFOR's work folder - a short path,
    so the unpacked bundle's names stay under Windows' 260 characters: <root>\\work\\navkit_stage\\<level>, an
    OPFOR-like tree whose work\\levels\\<level> is the staged level prepared, work\\navkit\\<level>\\navkit.json its
    settings and work\\navsample its samples."""
    return os.path.join(root, 'work', 'navkit_stage', lv.lower())


def stage_level(root, stage, log=print, bodies=None, level=None, base=None):
    """The offline sampler on an export's stage (the export's path-data step, round NK6): the staged level's bundle
    prepared as navkit.py prepare does (unpacked and converted; objects, group members, spawns, orders, ladders and
    the triangle lists from the staged partition and its group files, the base level's unpacks for the blueprints),
    the base level's settings (box, heights, sector offsets) and terrain, then both capsule runs at 0.8 m and - over
    the detailed sectors the stage's own samples and objects choose - at 0.2 m, written into stage_tree(root,
    level). The base level must be prepared (navkit.py prepare <base>). level / base: the staged level's id and its base level
    (the export passes them: its stage.json is written after the step); else read from stage.json. Returns the
    samples' folder."""
    import shutil

    import ladders
    import levelobjects
    import navkit
    if not (level and base):
        rec = json.load(open(os.path.join(stage, 'stage.json'), encoding='utf-8'))
        level, base = rec['level'], rec['base']
    lv, base = level.lower(), base.lower()
    pr = navkit.Project(root)
    Lb = pr.level_work(base)
    sb = navkit.ensure_settings(pr, base)
    tree = stage_tree(pr.root, lv)
    W = os.path.join(tree, 'work')
    L = os.path.join(W, 'levels', lv)
    bundle = os.path.join(stage, 'level-00.fbrb')
    if not os.path.isfile(bundle):
        raise navkit.NavKitError(f'{stage}: no staged level-00.fbrb')
    log(f'== offline sample of the stage {lv} (base {base})')
    pr.unpack(bundle, os.path.join(L, 'dist'), log)
    os.makedirs(os.path.join(L, 'pkg'), exist_ok=True)
    part = navkit.level_partition_name(L, lv)
    # the base's static-group files the stage does not carry (an export writes only the groups it edits)
    bpart = sb['level']
    src = os.path.join(Lb, 'dist', 'levels', bpart)
    dst = os.path.join(L, 'dist', 'levels', part)
    if os.path.isdir(src):
        os.makedirs(dst, exist_ok=True)
        for f in sorted(os.listdir(src)):
            if f.lower().endswith('_havokasset.xml') and not os.path.isfile(os.path.join(dst, f)):
                shutil.copyfile(os.path.join(src, f), os.path.join(dst, f))
    extra = [os.path.join(Lb, 'pkg'), os.path.join(Lb, 'dist')] + common_bases(root)[::-1]
    extra = [b for b in extra if os.path.isdir(b)]
    log('  extracting objects, collision and spawn points ...')
    if levelobjects.main(['levelobjects', L, part] + extra):
        raise navkit.NavKitError(f'{lv}: levelobjects failed on the stage')
    lad = ladders.extract(L, extra)
    open(os.path.join(L, 'ladders.json'), 'w', encoding='utf-8').write(json.dumps(lad, indent=1, sort_keys=True))
    st = dict(sb, level=part, box_source=f'the base level {base}', offsets_source=f'the base level {base}')
    d = os.path.join(W, 'navkit', lv)
    os.makedirs(d, exist_ok=True)
    json.dump(st, open(os.path.join(d, 'navkit.json'), 'w'), indent=1, sort_keys=True)
    # the base's terrain (an export does not sculpt it yet): its unpack copied, so the height field and water read
    tb = os.path.join(Lb, 'terrain')
    tt = os.path.join(L, 'terrain')
    if os.path.isdir(tb) and not os.path.isdir(tt):
        shutil.copytree(tb, tt)
    from bfbc2core import level as levelmod
    geom = level_geometry(L, extra, log=log, box=st['box'])
    geom = geom.bodies(bodies or BODIES)
    if INTACT_ONLY:
        geom = geom.intact()
    hf = levelmod.heightfield_of(tree, lv, log=log)
    out = os.path.join(W, 'navsample')
    os.makedirs(out, exist_ok=True)

    def run(pitch, box, rects=None):
        grid = Grid.from_box(pitch, box, st['y'], rects)
        log(f'  pitch {pitch}: {grid.nx} x {grid.nz} columns')
        g = geom
        if rects:       # NAVS-SMP-2: the geometry over the detailed box, not the level's alone
            g = level_geometry(L, extra, log=log, box=box).bodies(bodies or BODIES)
            g = g.intact() if INTACT_ONLY else g
        res = sample_grid(grid, g, hf, log=log, bottoms=tuple(CAP_BOTTOMS.values()))
        ps = '' if abs(pitch - 0.8) < 1e-6 else f'_p{int(round(pitch * 10)):02d}'
        paths = []
        for suf, b in CAP_BOTTOMS.items():
            p = os.path.join(out, f'{lv}_navkit{suf}{ps}.bin')
            write(p, res[b])
            paths.append(p)
            log(f'  wrote {p}')
        return paths
    p08 = run(0.8, st['box'])
    import detailed
    ids, _why = detailed.choose(L, common_bases(root)[::-1], p08[0], st['xoff'], st['zoff'])
    for suf in ('', '_hi'):
        old = os.path.join(out, f'{lv}_navkit{suf}_p02.bin')
        if os.path.isfile(old):
            os.remove(old)
    if ids:
        rects = [[(i + st['xoff']) * 100 - 1.0, (j + st['zoff']) * 100 - 1.0, (i + st['xoff']) * 100 + 101.0,
                  (j + st['zoff']) * 100 + 101.0] for i, j in ids]
        box = [min(r[0] for r in rects), min(r[1] for r in rects), max(r[2] for r in rects), max(r[3] for r in rects)]
        run(0.2, box, rects)
    else:
        log(f'  {lv}: no detailed sectors - no 0.2 m runs')
    return out


def _opt(args, name, conv=str):
    if name in args:
        i = args.index(name)
        v = args[i + 1]
        del args[i:i + 2]
        return conv(v)
    return None


def main(argv):
    args = list(argv[1:])
    if not args or args[0] in ('-h', '--help'):
        print(__doc__)
        return 0
    import navkit
    pitch = _opt(args, '--pitch', float) or 0.8
    box = _opt(args, '--box', lambda v: [float(a) for a in v.split(',')])
    yr = _opt(args, '--y', lambda v: [float(a) for a in v.split(',')])
    bodies = _opt(args, '--bodies')
    root = navkit.find_opfor(_opt(args, '--opfor'))
    cmd = args[0]
    if cmd == 'compare':
        a, _ = navsample.load(args[1])
        b, _ = navsample.load(args[2])
        compare(a, b)
        return 0
    if cmd == 'parity':
        parity(root, [a.lower() for a in args[1:]], name=_opt(args, '--name') or 'offline')
        return 0
    if cmd == 'stockmp':
        steps = _opt(args, '--steps') or ','.join(ALL_STEPS)
        sel = [a.lower() for a in args[1:]] or None
        stock_mp(root, sel, steps=tuple(steps.split(',')))
        return 0
    if cmd == 'stage':
        level, base = _opt(args, '--level'), _opt(args, '--base')
        stage_level(root, args[1], bodies=bodies, level=level, base=base)
        return 0
    lv = args[1].lower()
    if cmd == 'sample':
        rects = None
        if abs(pitch - 0.8) > 1e-6:
            ids, _ = detailed_choice(root, lv, sample_path(root, lv))
            if not ids:
                print(f'{lv}: no detailed sectors')
                return 0
            st = navkit.ensure_settings(navkit.Project(root), lv)
            rects = [[(i + st['xoff']) * 100 - 1, (j + st['zoff']) * 100 - 1, (i + st['xoff']) * 100 + 101,
                      (j + st['zoff']) * 100 + 101] for i, j in ids]
            box = box or [min(r[0] for r in rects), min(r[1] for r in rects), max(r[2] for r in rects),
                          max(r[3] for r in rects)]
        run_level(root, lv, pitch, box, yr, bodies=bodies, rects=rects)
    elif cmd in ('like', 'validate'):
        # validate: several in-game files of one level, the geometry read once; a JSON summary next to the outputs
        files = args[2:]
        geom = hf = None
        summary = {}
        for f in files:
            s, _ = navsample.load(f)
            grid = Grid.like(s)
            na = np.diff(s.start)
            if (na == 0).mean() > 0.5:          # a detailed-zone run: only the columns it sampled
                grid.mask = (na > 0).reshape(grid.nz, grid.nx)
            print(f'== offline sample {lv} on the grid of {os.path.basename(f)}: {grid.nx} x {grid.nz} at {grid.pitch:g}')
            if geom is None:
                geom, hf = load_level(root, lv, bodies=bodies)
            res = sample_grid(grid, geom, hf, bottoms=(float(s.cap_bottom),))
            out = os.path.join(out_dir(root), f'{lv}_like_{os.path.basename(f)}')
            write(out, res[float(s.cap_bottom)])
            print('wrote', out)
            summary[os.path.basename(f)] = compare(s, res[float(s.cap_bottom)])
        if cmd == 'validate':
            p = os.path.join(out_dir(root), f'{lv}_validate.json')
            json.dump(summary, open(p, 'w'), indent=1)
            print('wrote', p)
    elif cmd == 'build':
        build_level(root, lv)
    elif cmd == 'check':
        check_level(root, lv, os.path.join(root, 'work', 'navkit_offline', lv, 'pathdata.hpd'))
    elif cmd == 'all':
        stock_mp(root, [lv])
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == '__main__':
    from bfbc2core import errors
    errors.setup('navsample_offline', gui=False)
    with errors.guard():
        sys.exit(main(sys.argv))
