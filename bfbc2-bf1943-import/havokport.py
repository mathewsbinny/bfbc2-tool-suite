r"""havokport.py - Battlefield 1943's physics (HavokPhysicsData: Havok 4.6.1, Xbox 360) in BFBC2's PC format (Havok
5.5.0), in two stages:

1. restructure: the 4.6.1 packfile rewritten as a big-endian 5.5.0 one - the class-name section taken from a BFBC2
   file (5.5.0 names, signatures: BFBC2's loader knows those), the data section's objects kept where they are, the
   two classes whose layout changed (hkRigidBody, hkMoppBvTreeShape) rewritten in place in 5.5.0's layout, the 4.6.1-only
   deactivator objects (hkSpatialRigidBodyDeactivator) left out, class-name strings in the data (a root container's
   variants) renamed, the fixup tables rebuilt, the sections in 5.5.0's order (class names, types, data);
2. the console converter (`bfbc2-console-convert\havok.py`) turns that into the PC file, field by field from the
   class layouts in the user's BFBC2Game.exe - a field the layouts do not account for stops it.

What changed from 4.6.1 to 5.5.0, measured on the physics both games ship (the Kurogane, Zero and Corsair; the
research notes are Project OPFOR's `research\WW2_PORT_PLAN.md`): the physics classes gained a "p" (hkRigidBody ->
hkpRigidBody); every object kept its size; hkRigidBody's collidable gained a 40-byte cached bounding volume (written
marked invalid, as BFBC2's files hold it), lost the deactivator pointer, gained the SPU collision callback (BFBC2's
filters 3 / 1) and extended listeners, and its motion packs the two velocity limits into bytes after the damping;
hkMoppBvTreeShape moved its code pointer to 16 and its child to 52; hkMoppCode gained a build type (1: built without
chunk subdivision, as BFBC2's). The other classes the vehicles use have 5.5.0's layout already.

The MOPP code itself is kept as BF1943's 4.6.1 builder wrote it (it differs from DICE's rebuilt code in split choices
and jump offsets only): both versions use the same byte code, and decoded (mopp.py) on the 88 physics files both games
ship, every one of the 54,245 triangles of BF1943's 236 converted codes reaches its terminal, 11,800 rays cast
through them hit what brute force hits, and the trees are as tight as DICE's (a triangle's own box returns 28.3 % of
the keys, DICE's 27.7 %) - no rebuild is needed.

    import havokport
    pc_bytes = havokport.convert(bf1943_bytes, template_pc_bytes, havok.classes(exe))
"""
import os
import struct
import sys
from typing import Any, cast

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert')
import havok  # noqa: E402

MAGIC = havok.MAGIC
DROPPED = {'hkSpatialRigidBodyDeactivator', 'hkRigidBodyDeactivator'}

# hkRigidBody: (4.6.1 offset, 5.5.0 offset, bytes) copied; the rest of the 512 bytes is 5.5.0's own (CONST)
RIGID_COPY = [(0, 0, 48),            # object header, collidable up to the broad-phase filter info
              (48, 88, 52),          # allowedPenetrationDepth, collisionEntries, multiThreadCheck, name, properties,
                                     # material
              (100, 140, 44),        # breakOffPartsUtil, solverData, storage index, constraints
              (148, 184, 12),        # simulationIsland, autoRemoveLevel..., uid
              (160, 208, 16),        # the motion: vtable, size, type, deactivation counters
              (176, 224, 160),       # transform, swept transform, delta angle
              (336, 384, 4),         # objectRadius
              (348, 388, 8),         # linear, angular damping
              (368, 400, 96)]        # inertia and mass, velocities, deactivation reference, saved motion
RIGID_CONST = [(48, b'\0\0\0\x01'),  # cached bounding volume: min[0] > max[0] marks it invalid (rebuilt at load)
               (202, b'\x03\x01')]   # spuCollisionCallback eventFilter, userFilter (BFBC2's values)
RIGID_DROPPED_SLOTS = {144}           # the deactivator
VELOCITY_BYTE = {200.0: 0x7F}        # hkUFloat8 of the velocity limits without a table: the value BF1943's vehicles use


def ufloat8_table(exe):
    """Havok's hkUFloat8 decoding table (256 rising floats: 0, 0.01, ... about 1e6) found in the user's executable
    (bytes); None if absent. 5.5.0 stores a rigid body's velocity limits as an index into it."""
    import numpy as np
    for off in range(4):
        a = np.frombuffer(exe, '<f4', (len(exe) - off) // 4, off)
        for i in np.where(a == np.float32(0.01))[0]:
            t = a[i - 1:i + 255].astype(float)
            if i >= 1 and len(t) == 256 and t[0] == 0 and 9e5 < t[255] < 1.1e6 and (np.diff(t) > 0).all():
                return [float(v) for v in t]
    return None


def ufloat8(v, table):
    """v as an index into the table: the nearest value (200 -> 0x7F, 202.8 - the Kurogane's byte in BFBC2's own
    file; rounding up would give the same there, rounding down would not)."""
    return min(range(len(table)), key=lambda i: abs(table[i] - v))
MOPP_TREE_COPY = [(0, 0, 16), (24, 16, 4), (20, 52, 4)]   # header; code pointer; child shape pointer


AABB_W = {'hkListShape', 'hkStorageExtendedMeshShape', 'hkExtendedMeshShape'}


def _member_offset(classes, cls, name):
    k = classes[cls]
    while k:
        for m in k.members:
            if m.name == name:
                return m.offset
        k = classes.get(k.parent) if k.parent else None
    raise KeyError(f'{cls}.{name}')


class PortError(Exception):
    what_to_do = 'The file uses a Havok class or value this converter does not know yet (havokport.py).'


def _rename(name, classes):
    if name in classes or name in ('HavokMaterial', 'HavokMaterials'):
        return name
    if name.startswith('hk') and 'hkp' + name[2:] in classes:
        return 'hkp' + name[2:]
    return None


def _sections(d, p, e):
    out = []
    for i in range(struct.unpack_from(e + 'I', d, p + 20)[0]):
        h = p + 64 + 48 * i
        out.append((d[h:h + 20], struct.unpack_from(e + '7I', d, h + 20)))
    return out


def _table(d, start, end, size, e):
    out, o = [], start
    while o + size <= end and struct.unpack_from(e + 'I', d, o)[0] != 0xFFFFFFFF:
        out.append(struct.unpack_from(e + '%dI' % (size // 4), d, o))
        o += size
    return out


def _classnames(d, start, e):
    """[(offset of the name in the section, name, signature)]."""
    out, o = [], start
    while d[o + 4] == 0x09:
        q = d.index(b'\0', o + 5)
        out.append((o + 5 - start, d[o + 5:q].decode('latin-1'), struct.unpack_from(e + 'I', d, o)[0]))
        o = q + 1
    return out


def restructure(b, template, classes, table=None):
    """The 4.6.1 resource b as a big-endian 5.5.0 one (DICE's wrapper kept); template: a BFBC2 PC physics resource
    whose class-name section names every class b's objects need; table: ufloat8_table (else only VELOCITY_BYTE's
    velocity limits are known)."""
    p = b.find(MAGIC)
    if p < 0 or b'Havok-4.6.1' not in b[p + 40:p + 56]:
        raise PortError('not a Havok 4.6.1 packfile')
    secs = _sections(b, p, '>')
    tags = [t.split(b'\0')[0].decode('latin-1') for t, _ in secs]
    cn, data = tags.index('__classnames__'), tags.index('__data__')
    names = {off: n for off, n, _ in _classnames(b, p + secs[cn][1][0], '>')}
    # the template's class names (PC: little-endian), written big-endian
    tp = template.find(MAGIC)
    tsecs = _sections(template, tp, '<')
    ttags = [t.split(b'\0')[0].decode('latin-1') for t, _ in tsecs]
    tcn = tsecs[ttags.index('__classnames__')][1]
    tnames = _classnames(template, tp + tcn[0], '<')
    new_cn = bytearray()
    at = {}
    for _, n, sig in tnames:
        at[n] = len(new_cn) + 5
        new_cn += struct.pack('>I', sig) + b'\x09' + n.encode('latin-1') + b'\0'
    while len(new_cn) % 16:
        new_cn += b'\xff'
    # the data section: content, fixups
    start, loc, glob, virt, exp, imp, end = secs[data][1]
    base = p + start
    content = bytearray(b[base:base + loc])
    local = {s: t for s, t in _table(b, base + loc, base + glob, 8, '>')}
    gfix = _table(b, base + glob, base + virt, 12, '>')
    vfix = _table(b, base + virt, base + exp, 12, '>')
    objects = []
    for o, sec, noff in vfix:
        if sec != cn or noff not in names:
            raise PortError(f'object at {o:#x}: its class name is not in the class-name section')
        objects.append((o, names[noff]))
    dropped = [(o, n) for o, n in objects if n in DROPPED]
    kept = [(o, n) for o, n in objects if n not in DROPPED]
    unknown = sorted({n for _, n in kept if _rename(n, classes) is None})
    if unknown:
        raise PortError(f'classes with no 5.5.0 counterpart: {", ".join(unknown)}')
    missing = sorted({_rename(n, classes) for _, n in kept} - set(at))
    if missing:
        raise PortError(f'the template\'s class names lack {", ".join(missing)}: give a BFBC2 file that has them')
    starts = sorted(o for o, _ in objects)

    def span(o):
        nxt = [s for s in starts if s > o]
        return (nxt[0] if nxt else loc) - o

    for o, _n in dropped:                            # their own pointers go with them
        for s in [s for s in local if o <= s < o + span(o)]:
            del local[s]
    slot_map = {}                                   # old slot -> new slot (None: dropped), in rewritten objects
    for o, n in kept:
        if n == 'hkRigidBody':
            old = bytes(content[o:o + 512])
            new = bytearray(512)
            for src, dst, size in RIGID_COPY:
                new[dst:dst + size] = old[src:src + size]
                for k in range(0, size, 4):
                    slot_map[o + src + k] = o + dst + k
            for dst, val in RIGID_CONST:
                new[dst:dst + len(val)] = val
            for k, src in ((396, 340), (397, 344)):
                v = round(struct.unpack_from('>f', old, src)[0], 3)
                if table is not None:
                    new[k] = ufloat8(v, table)
                    continue
                if v not in VELOCITY_BYTE:
                    raise PortError(f'rigid body at {o:#x}: a velocity limit of {v} (only {sorted(VELOCITY_BYTE)} known;'
                                    f' pass ufloat8_table(exe))')
                new[k] = VELOCITY_BYTE[v]
            new[398] = old[357]                     # deactivationClass
            for s in RIGID_DROPPED_SLOTS:
                slot_map[o + s] = None
            content[o:o + 512] = new
        elif n == 'hkMoppBvTreeShape':
            old = bytes(content[o:o + 64])
            new = bytearray(64)
            for src, dst, size in MOPP_TREE_COPY:
                new[dst:dst + size] = old[src:src + size]
                for k in range(0, size, 4):
                    slot_map[o + src + k] = o + dst + k
            content[o:o + 64] = new
        elif n == 'hkMoppCode':
            content[o + 44] = 1                             # buildType (a 1-byte enum): built without chunk subdivision
        if n in AABB_W:                                     # the bounding box centre's w: 1.0, as BFBC2's exporter
            w = o + _member_offset(classes, _rename(n, classes), 'aabbCenter') + 12
            if content[w:w + 4] == b'\0\0\0\0':
                content[w:w + 4] = struct.pack('>f', 1.0)
    gone = {o for o, _ in dropped}
    new_local = {}
    for s, t in local.items():
        s2 = slot_map.get(s, s)
        if s2 is None or t in gone:
            continue
        new_local[s2] = t
    # class-name strings in the data (a root container's variants name their class): renamed, appended
    renames = {n: _rename(n, classes) for n in set(names.values())}
    renames = {k: v for k, v in renames.items() if v and v != k}
    appended = {}
    for s, t in list(new_local.items()):
        if 0 <= t < loc and content[t:t + 2] == b'hk':
            q = content.find(b'\0', t)
            text = content[t:q].decode('latin-1', 'replace')
            if text in renames:
                if text not in appended:
                    while len(content) % 16:
                        content += b'\0'
                    appended[text] = len(content)
                    content += renames[text].encode('latin-1') + b'\0'
                new_local[s] = appended[text]
    while len(content) % 16:
        content += b'\0'
    # the global fixups: class-name targets to the template's offsets
    new_glob = []
    for s, sec, off in gfix:
        s2 = slot_map.get(s, s)
        if s2 is None:
            continue
        if sec == cn:
            n = _rename(names[off], classes)
            new_glob.append((s2, 0, at[n]))
        elif sec == data:
            new_glob.append((s2, 2, off))
        else:
            raise PortError(f'a global fixup into section {tags[sec]}')
    new_virt = [(o, 0, at[_rename(n, classes)]) for o, n in kept]

    def pack_table(rows, size):
        out = bytearray()
        for r in rows:
            out += struct.pack('>%dI' % (size // 4), *r)
        while len(out) % 16:
            out += b'\xff'
        return out

    lt = pack_table(sorted(new_local.items()), 8)
    gt = pack_table(sorted(new_glob), 12)
    vt = pack_table(sorted(new_virt), 12)
    d_loc = len(content)
    d_glob = d_loc + len(lt)
    d_virt = d_glob + len(gt)
    d_end = d_virt + len(vt)
    data_bytes = bytes(content) + lt + gt + vt
    # header and section table
    head = bytearray(b[p:p + 64])
    struct.pack_into('>I', head, 12, 5)                         # file version
    root_name = names[struct.unpack_from('>I', b, p + 36)[0]]
    struct.pack_into('>5I', head, 20, 3, 2, 0, 0, at[_rename(root_name, classes)])
    head[40:56] = b'Havok-5.5.0-r1\0\xff'                       # as BFBC2's own files pad it (0xFF after the NUL)
    cn_start = 64 + 48 * 3
    data_start = cn_start + len(new_cn)
    st = bytearray()
    for tag, rel in ((b'__classnames__', (cn_start, len(new_cn), len(new_cn), len(new_cn), len(new_cn), len(new_cn),
                                          len(new_cn))),
                     (b'__types__', (data_start, 0, 0, 0, 0, 0, 0)),
                     (b'__data__', (data_start, d_loc, d_glob, d_virt, d_end, d_end, d_end))):
        st += tag + b'\0' * (19 - len(tag)) + b'\xff' + struct.pack('>7I', *rel)
    return b[:p] + bytes(head) + bytes(st) + bytes(new_cn) + data_bytes


def wrapper_run_table(b):
    """(offset, M, C, big-endian) of the material table that opens the wrapper's byte run (bfbc2-console-convert
    FORMAT.md 1.1): u16 M groups, u16 C material hashes, C u32 hashes, M u16 group ends (the last = C), then the
    per-part bytes. BFBC2 writes the table little-endian on both platforms; BF1943 big-endian. None: no table found
    in either byte order."""
    o = b.find(MAGIC)
    n = struct.unpack_from('<I', b, 0)[0]
    p = 8 + 4 * n
    if p + 2 > o:
        return None
    a = struct.unpack_from('<H', b, p)[0]
    p += 2 + 12 * a
    if p + 2 > o:
        return None
    c = struct.unpack_from('<H', b, p)[0]
    p += 2 + 24 * c
    if p + 4 > o:
        return None
    if struct.unpack_from('<H', b, p)[0] == 0:
        return p, 0, 0, False                  # no table (a single body's wrapper): the same in either byte order
    for e, big in (('<', False), ('>', True)):
        m, k = struct.unpack_from(e + '2H', b, p)
        end = p + 4 + 4 * k + 2 * m
        if end > o - 8:
            continue
        ends = struct.unpack_from(f'{e}{m}H', b, p + 4 + 4 * k) if m else ()
        if (m == 0 and k == 0) or (ends and ends[-1] == k and all(x <= y for x, y in zip(ends, ends[1:]))):
            return p, m, k, big
    return None


def _run_table_to_pc(b):
    """The wrapper's material table in BFBC2's byte order (little-endian)."""
    t = wrapper_run_table(b)
    if t is None:
        if b.find(MAGIC) >= 0 and struct.unpack_from('<I', b, 0)[0] != struct.unpack_from('<I', b, 4)[0]:
            return b                          # not the part-list wrapper (FORMAT.md 1.1): nothing to swap
        raise PortError('the wrapper\'s material table (the byte run\'s start) is in neither byte order')
    p, m, k, big = t
    if not big:
        return b
    out = bytearray(b)
    fmt = f'2H{k}I{m}H'
    out[p:p + struct.calcsize('<' + fmt)] = struct.pack('<' + fmt, *struct.unpack_from('>' + fmt, b, p))
    return bytes(out)


SHAPE_MESHES = {'hkpExtendedMeshShape', 'hkpStorageExtendedMeshShape'}
SUBPART_SIZE, SUBPARTS_AT = 64, 88       # a shape subpart's size; hkpExtendedMeshShape::shapesSubparts (pointer, count)
TRIANGLE_SUBPARTS_AT = 80                # hkpExtendedMeshShape::trianglesSubparts (pointer, count)
HALF_EXTENTS_AT, CENTER_AT = 48, 64      # hkpExtendedMeshShape::aabbHalfExtents, aabbCenter
CHILDREN_AT = 16                         # ShapesSubpart::childShapes (pointer, count)
# a convex child's local box: hkpConvexVerticesShape aabbHalfExtents / aabbCenter; hkpBoxShape halfExtents (centre 0)
CONVEX_BOX: dict[str, tuple[int, int | None]] = {'hkpConvexVerticesShape': (32, 48), 'hkpBoxShape': (32, None)}
CONVEX_RADIUS_AT = 16                    # hkpConvexShape::radius


def subpart_w_lanes(q):
    """The w lanes of the three rotation columns BFBC2's exporter builds from a shape subpart's quaternion (x, y, z,
    w), float32: (2(yz - xw), 2(yw + xz), 0) - column 0's w is column 2's y (R12), column 1's w column 2's x (R02).
    Not part of the rotation (Havok ignores the w lanes); they are what DICE's model groups' bounding boxes carry in w
    (aabb_w). Decoded from DICE's own files: with these lanes and aabb_w's subpart-by-subpart merge, aabbCenter.w and
    aabbHalfExtents.w are bit-identical to DICE's in all 299 model-group shapes of all BFBC2 levels (2026-11-09; the
    earlier sign of lane 0, 2(xw - yz), matched 146 - its error shows only on tilted placements of tall children)."""
    import numpy as np
    x, y, z, w = (np.float32(v) for v in q)
    two = np.float32(2)
    return np.array([two * (y * z - x * w), two * (y * w + x * z), np.float32(0)], np.float32)


def aabb_w(parts):
    """(aabbCenter.w, aabbHalfExtents.w) of an extended mesh shape built of shape subparts, as
    hkpExtendedMeshShape::addShapesSubpart leaves them in BFBC2's files: subpart by subpart in file order, the
    subpart's own box (each child's box centre turned into the subpart's frame plus the translation's w 1.0, its half
    extents turned with the absolute w lanes of the rotation columns (subpart_w_lanes) plus the convex radius; the
    min / max over its children) is merged into the box the stored centre / half extents give back (centre -+ half),
    and centre / half extents are stored again as (max + min) / 2, (max - min) / 2 in float32 - bit-identical to
    DICE's in all 299 model-group shapes (merging child by child: 296; one box over all children: rounding off).
    parts: [(quaternion, [(centre xyz, half extents xyz, radius)])]; None when no subpart has a child."""
    import numpy as np
    cen: Any = None
    half: Any = None
    one, h5 = np.float32(1), np.float32(0.5)
    for q, children in parts:
        if not children:
            continue
        cw = subpart_w_lanes(q)
        lo: Any = None
        hi: Any = None
        for c, h, r in children:
            cc = one + np.float32(np.dot(cw, np.asarray(c, np.float32)))
            he = np.float32(np.abs(cw * np.asarray(h, np.float32)).sum()) + np.float32(r)
            lo = cc - he if lo is None else min(lo, cc - he)
            hi = cc + he if hi is None else max(hi, cc + he)
        if cen is not None:
            lo, hi = min(cen - half, lo), max(cen + half, hi)
        cen, half = (hi + lo) * h5, (hi - lo) * h5
    return None if cen is None else (float(cen), float(half))


def shape_subparts_to_550(pc):
    """A PC packfile's extended mesh shapes with each shape subpart's placement in 5.5.0's order: 4.6.1 keeps the
    translation at 32 and the rotation quaternion at 48, 5.5.0 the rotation at 32 and the translation at 48. The
    model groups' physics (forest, middle growth, static and rigid groups: one subpart per placed object) carry them;
    left in 4.6.1's order, each object's position was read as its rotation - Wake's palms drew as sheets hundreds of
    metres long across the sky. Swapped only where the data shows it (a subpart whose 'rotation' is no unit
    quaternion and whose 'translation' is one).

    The w lanes as BFBC2's exporter writes them (4.6.1's are 0): every shape subpart with children has translation.w
    1.0 (all 277,950 such subparts of BFBC2's levels; the empty subpart Havok adds last keeps 0), and a shape built
    of shape subparts only gets aabbCenter.w / aabbHalfExtents.w from aabb_w. Returns (bytes, subparts swapped)."""
    import numpy as np
    b = bytearray(pc)
    p = bytes(b).find(MAGIC)
    if p < 0:
        return pc, 0
    nsec = struct.unpack_from('<I', b, p + 20)[0]
    secs = {}
    for i in range(nsec):
        h = p + 64 + 48 * i
        secs[bytes(b[h:h + 19]).split(b'\0')[0].decode('latin-1')] = struct.unpack_from('<7I', b, h + 20)
    if '__data__' not in secs or '__classnames__' not in secs:
        return pc, 0
    cstart, cloc = secs['__classnames__'][:2]
    names, o = {}, p + cstart
    while o + 5 < p + cstart + cloc:
        if struct.unpack_from('<I', b, o)[0] == 0xFFFFFFFF:
            break
        q = b.index(0, o + 5)
        names[o + 5 - (p + cstart)] = bytes(b[o + 5:q]).decode('latin-1')
        o = q + 1
    start, loc, glob, virt, exp = secs['__data__'][:5]
    base = p + start
    local = {}
    for o in range(base + loc, base + glob - 7, 8):
        s, d = struct.unpack_from('<2I', b, o)
        if s != 0xFFFFFFFF:
            local[s] = d
    objects = {}                                    # pointers to objects: the global fixups (into this section)
    for o in range(base + glob, base + virt - 11, 12):
        s, sec, d = struct.unpack_from('<3I', b, o)
        if s != 0xFFFFFFFF:
            objects[s] = d
    swapped = 0
    kinds: dict[int, str] = {}
    for o in range(base + virt, base + exp - 11, 12):
        off, _, cn = struct.unpack_from('<3I', b, o)
        if off != 0xFFFFFFFF:
            kinds[off] = names.get(cn, '')
    for off, cls in kinds.items():
        if cls not in SHAPE_MESHES:
            continue
        arr, cnt = local.get(off + SUBPARTS_AT), struct.unpack_from('<I', b, base + off + SUBPARTS_AT + 4)[0]
        if arr is None:
            continue
        parts: list[tuple[tuple[float, ...], list[tuple[tuple[float, ...], tuple[float, ...], float]]]] = []
        known = True
        for k in range(cnt):
            r = base + arr + SUBPART_SIZE * k
            rot = np.frombuffer(bytes(b[r + 32:r + 48]), '<f4')
            tra = np.frombuffer(bytes(b[r + 48:r + 64]), '<f4')
            if abs(np.linalg.norm(rot) - 1) > 1e-2 and abs(np.linalg.norm(tra) - 1) < 1e-2:
                b[r + 32:r + 48], b[r + 48:r + 64] = bytes(tra), bytes(rot)
                swapped += 1
            nch = struct.unpack_from('<I', b, r + CHILDREN_AT + 4)[0]
            if not nch:
                continue
            struct.pack_into('<f', b, r + 60, 1.0)                  # translation.w
            quat = struct.unpack_from('<4f', b, r + 32)
            children = []
            carr = local.get(arr + SUBPART_SIZE * k + CHILDREN_AT)
            if carr is None:
                known = False
            for i in range(nch if carr is not None else 0):
                c = objects.get(cast(int, carr) + 4 * i)
                box = CONVEX_BOX.get(kinds.get(c, '') if c is not None else '')
                if c is None or box is None:
                    known = False
                    break
                half = struct.unpack_from('<3f', b, base + c + box[0])
                cen = struct.unpack_from('<3f', b, base + c + box[1]) if box[1] is not None else (0.0, 0.0, 0.0)
                children.append((cen, half, struct.unpack_from('<f', b, base + c + CONVEX_RADIUS_AT)[0]))
            parts.append((quat, children))
        has_triangles = struct.unpack_from('<I', b, base + off + TRIANGLE_SUBPARTS_AT + 4)[0] > 0
        w = aabb_w(parts) if known and not has_triangles else None
        if w is not None:
            struct.pack_into('<f', b, base + off + CENTER_AT + 12, w[0])
            struct.pack_into('<f', b, base + off + HALF_EXTENTS_AT + 12, w[1])
    return bytes(b), swapped


def convert(b, template, classes, table=None):
    """BF1943's physics resource b -> BFBC2's PC format (table: ufloat8_table, for velocity limits other than 200).
    The wrapper's material table is swapped too: BF1943 writes it big-endian, and BFBC2's loader, which reads its
    counts to find the packfile, otherwise lands short of it (a null version string at load, 0xC585D8). The model
    groups' shape subparts get 5.5.0's field order (shape_subparts_to_550)."""
    pc = _run_table_to_pc(havok.convert(restructure(b, template, classes, table), classes))
    return shape_subparts_to_550(pc)[0]


# ------------------------------------------------------------------------------------------------ physics materials
# Where a PC physics resource keeps its materials (Project OPFOR research\MATERIAL_SLOTS_RE.md, the exe's readers):
#   part materials  the wrapper's byte run after the boxes: u16 0 (no material table: BF1943's files may carry one,
#                   DICE's never do), u8 the number of distinct materials the parts use, u8 the highest index, then one
#                   byte per part - an index into the material list - left out when every part uses the same one (that
#                   index is then the u8 before). All 991 object HavokAssets BFBC2 ships follow it, their part bytes
#                   naming PartInfoDatas[].MaterialName through MaterialMappings (test_havokport). The game reads a
#                   part's material for its damage (vtable+0xE0, per health state 0x6D84D0).
#   material list   the tail before the packfile: 1, k, k u32 hashes (the material grid's MaterialData.Hash), in the
#                   order of the dbx MaterialMappings
#   face materials  a storage mesh's hkpStorageExtendedMeshShapeMeshSubpartStorage.materials (u32 indices into the
#                   packfile's HavokMaterials list): its subpart's first hkpMeshMaterial, which the game reads as the
#                   hit material of a static model group member (0xAAEEE0 / 0xAADE10: the bullet's impact effect,
#                   sound, decal and penetration)

class MaterialError(ValueError):
    """A physics resource whose material slots are not the shape this module reads and writes."""


def _wrapper(b):
    """(parts n, the part run's start, the material list's start (its "1, k"), the list's hashes, the packfile's
    start) of a PC physics resource."""
    o = b.find(MAGIC)
    if o < 0 or len(b) < 8:
        raise MaterialError('no Havok packfile')
    n, n2 = struct.unpack_from('<2I', b, 0)
    if n != n2:
        raise MaterialError('not a part-list wrapper (its counts differ)')
    p = 8 + 4 * n
    p += 2 + 12 * struct.unpack_from('<H', b, p)[0]
    p += 2 + 24 * struct.unpack_from('<H', b, p)[0]
    for k in range(0, (o - p) // 4):
        q = o - 4 * k - 8
        if q < p:
            break
        one, kk = struct.unpack_from('<2I', b, q)
        if one == 1 and kk == k:
            return n, p, q, list(struct.unpack_from(f'<{k}I', b, q + 8)), o
    raise MaterialError('no material list (1, k, k hashes) before the packfile')


def part_materials(b):
    """(the material list's hashes, each part's index into it) of a PC physics resource."""
    n, p, q, hashes, _ = _wrapper(b)
    run = b[p:q]
    if n == 0:
        return hashes, []
    if len(run) < 4 or run[:2] != b'\0\0':
        raise MaterialError(f'the part run opens with a material table ({run[:4].hex(" ")}): not read here')
    distinct, top = run[2], run[3]
    idx = [top] * n if distinct == 1 else list(run[4:4 + n])
    if len(run) != (4 if distinct == 1 else 4 + n) or len(set(idx)) != distinct or max(idx) != top or \
            top >= len(hashes):
        raise MaterialError(f'the part run {run.hex(" ")} is not {n} parts\' indices into {len(hashes)} materials')
    return hashes, idx


def part_run(indices):
    """The part run of these per-part material indices (the format part_materials reads)."""
    if not indices:
        raise MaterialError('no parts')
    if max(indices) > 255 or min(indices) < 0:
        raise MaterialError('an index outside 0 - 255')
    distinct = len(set(indices))
    head = b'\0\0' + bytes([distinct, max(indices)])
    return head if distinct == 1 else head + bytes(indices)


def with_part_materials(b, indices, hashes):
    """b with part i on material hashes[indices[i]]; hashes is the new material list: the old one, new hashes only
    appended (the packfile's face indices keep their meaning)."""
    n, p, _, old, o = _wrapper(b)
    part_materials(b)                                         # the run is the shape this writes
    if len(indices) != n:
        raise MaterialError(f'{len(indices)} part materials for {n} parts')
    if list(hashes[:len(old)]) != old:
        raise MaterialError('the material list may only grow at its end')
    if max(indices) >= len(hashes):
        raise MaterialError('a part index past the material list')
    tail = struct.pack('<2I', 1, len(hashes)) + struct.pack(f'<{len(hashes)}I', *hashes)
    return b[:p] + part_run(list(indices)) + tail + b[o:]


def _data(b):
    """(the __data__ section's start, its local fixups {from: to}, global fixups {from: to}, objects {offset: class})
    of a PC packfile; offsets from the data section's start."""
    p = b.find(MAGIC)
    secs = {}
    for i in range(struct.unpack_from('<I', b, p + 20)[0]):
        h = p + 64 + 48 * i
        secs[b[h:h + 19].split(b'\0')[0].decode('latin-1')] = struct.unpack_from('<7I', b, h + 20)
    cstart, cloc = secs['__classnames__'][:2]
    names, o = {}, p + cstart
    while o + 5 < p + cstart + cloc and struct.unpack_from('<I', b, o)[0] != 0xFFFFFFFF:
        q = b.index(0, o + 5)
        names[o + 5 - (p + cstart)] = b[o + 5:q].decode('latin-1')
        o = q + 1
    start, loc, glob, virt, exp = secs['__data__'][:5]
    base = p + start
    local, glob_ = {}, {}
    for o in range(base + loc, base + glob - 7, 8):
        s, d = struct.unpack_from('<2I', b, o)
        if s != 0xFFFFFFFF:
            local[s] = d
    for o in range(base + glob, base + virt - 11, 12):
        s, _, d = struct.unpack_from('<3I', b, o)
        if s != 0xFFFFFFFF:
            glob_[s] = d
    kinds = {}
    for o in range(base + virt, base + exp - 11, 12):
        off, _, cn = struct.unpack_from('<3I', b, o)
        if off != 0xFFFFFFFF:
            kinds[off] = names.get(cn, '')
    return base, local, glob_, kinds


def _cstr(b, at):
    return b[at:b.index(0, at)].decode('latin-1')


def havok_materials(b):
    """The packfile's HavokMaterials list: the material names the face indices count in, in order."""
    base, local, glob_, kinds = _data(b)
    lists = sorted(o for o, c in kinds.items() if c == 'HavokMaterials')
    if not lists:
        return []
    arr, cnt = local.get(lists[0]), struct.unpack_from('<I', b, base + lists[0] + 4)[0]
    return [_cstr(b, base + local[glob_[cast(int, arr) + 4 * i]]) for i in range(cnt)]   # HavokMaterial.name at 0


def _fixups(b, start, end, size):
    out, o = [], start
    while o + size <= end:
        e = struct.unpack_from(f'<{size // 4}I', b, o)
        if e[0] != 0xFFFFFFFF:
            out.append(e)
        o += size
    return out


def _table_bytes(rows, size):
    t = b''.join(struct.pack(f'<{size // 4}I', *r) for r in rows)
    return t + b'\xff' * (-len(t) % 16)


def with_havok_materials(b, names):
    """b with the packfile's HavokMaterials list grown to names (the old list first; new HavokMaterial objects, name
    0, as DICE's exporter lays them out: the list's objects end the data section - a 16-byte object, its name at the
    next 16 bytes, padded to 16). DICE keeps this list equal to the material list (MaterialMappings, the wrapper's
    hashes) in every file it ships."""
    p = b.find(MAGIC)
    old = havok_materials(b)
    if [n.lower() for n in names[:len(old)]] != [n.lower() for n in old]:
        raise MaterialError('the HavokMaterials list may only grow at its end')
    if len(names) == len(old):
        return b
    base, local, glob_, kinds = _data(b)
    lists = [o for o, c in kinds.items() if c == 'HavokMaterials']
    if len(lists) != 1:
        raise MaterialError(f'{len(lists)} HavokMaterials lists')
    hm = lists[0]
    arr = local[hm]
    if any(o > hm and c != 'HavokMaterial' for o, c in kinds.items()) or arr <= hm:
        raise MaterialError('the HavokMaterials list does not end the data section: not written here')
    h = p + 64 + 48 * 2
    if b[h:h + 8] != b'__data__':
        raise MaterialError('the data section is not the third')
    start, loc, glob, virt, exp, imp, end = struct.unpack_from('<7I', b, h + 20)
    if base + end != len(b) or not (exp == imp == end):
        raise MaterialError('the data section does not end the file, or has exports / imports')
    lo = _fixups(b, base + loc, base + glob, 8)
    gl = _fixups(b, base + glob, base + virt, 12)
    vi = _fixups(b, base + virt, base + exp, 12)
    mats = sorted(o for o, c in kinds.items() if c == 'HavokMaterial')
    sec = next(r[1] for r in gl if r[0] == arr)
    vrow = next(r for r in vi if r[0] == mats[0])
    # the region from the pointer array on, rebuilt: the array, then each material (the old ones' 16 bytes kept)
    old_objs = [b[base + o:base + o + 16] for o in (glob_[arr + 4 * i] for i in range(len(old)))]
    region, objs, strs = bytearray(), [], []
    n = len(names)
    region += bytes(4 * n) + bytes(-4 * n % 16)
    for i, name in enumerate(names):
        o = arr + len(region)
        region += old_objs[i] if i < len(old) else bytes(16)
        s = arr + len(region)
        raw = name.encode('latin-1') + b'\0'
        region += raw + bytes(-len(raw) % 16)
        objs.append(o)
        strs.append(s)
    data = bytearray(b[base:base + arr]) + region
    struct.pack_into('<I', data, hm + 4, n)                      # the array's size
    lo = [r for r in lo if r[0] < arr] + [(o, s) for o, s in zip(objs, strs)]
    gl = [r for r in gl if r[0] < arr] + [(arr + 4 * i, sec, o) for i, o in enumerate(objs)]
    vi = [r for r in vi if r[0] < arr] + [(o, vrow[1], vrow[2]) for o in objs]
    tl, tg, tv = _table_bytes(lo, 8), _table_bytes(gl, 12), _table_bytes(vi, 12)     # the old rows in their order
    loc2 = len(data)
    glob2, virt2 = loc2 + len(tl), loc2 + len(tl) + len(tg)
    end2 = virt2 + len(tv)
    head = bytearray(b[:base])
    struct.pack_into('<7I', head, h + 20, start, loc2, glob2, virt2, end2, end2, end2)
    return bytes(head) + bytes(data) + tl + tg + tv


STORAGE_MATERIALS_AT = 56                # hkpStorageExtendedMeshShapeMeshSubpartStorage::materials (u32 array)
BODY_NAME_AT, BODY_SHAPE_AT = 112, 16     # hkpWorldObject::name, collidable.shape
MOPP_CHILD_AT, MESH_STORAGE_AT = 52, 192  # hkpMoppBvTreeShape child.childShape, hkpStorageExtendedMeshShape::meshstorage


def _storages(b, base, local, glob_, kinds, body):
    """[(the materials array's offset, count)] of a rigid body's storage mesh (None: its shape is no storage mesh)."""
    shape = glob_.get(body + BODY_SHAPE_AT)
    if shape is not None and kinds.get(shape) == 'hkpMoppBvTreeShape':
        shape = glob_.get(shape + MOPP_CHILD_AT)
    if shape is None or kinds.get(shape) != 'hkpStorageExtendedMeshShape':
        return None
    arr, cnt = local.get(shape + MESH_STORAGE_AT), struct.unpack_from('<I', b, base + shape + MESH_STORAGE_AT + 4)[0]
    out = []
    for i in range(cnt):
        st = glob_[cast(int, arr) + 4 * i]
        k = struct.unpack_from('<I', b, base + st + STORAGE_MATERIALS_AT + 4)[0]
        out.append((local.get(st + STORAGE_MATERIALS_AT), k))
    return out


def bodies(b):
    """[(name, [the face materials of each subpart storage: indices into havok_materials] or None)] of the
    packfile's rigid bodies in data order (None: the body's shape is no storage mesh, e.g. a model group's
    collision body of convex children)."""
    base, local, glob_, kinds = _data(b)
    out = []
    for o in sorted(k for k, c in kinds.items() if c == 'hkpRigidBody'):
        name = _cstr(b, base + local[o + BODY_NAME_AT]) if o + BODY_NAME_AT in local else ''
        st = _storages(b, base, local, glob_, kinds, o)
        out.append((name, None if st is None else
                    [list(struct.unpack_from(f'<{k}I', b, base + m)) if k else [] for m, k in st]))
    return out


def with_face_materials(b, changes):
    """b with face materials rewritten in place: changes {body index (bodies' order): [per subpart storage: its
    indices into havok_materials]} - the counts as bodies() lists them; only values change."""
    base, local, glob_, kinds = _data(b)
    out = bytearray(b)
    n = len(havok_materials(b))
    order = sorted(k for k, c in kinds.items() if c == 'hkpRigidBody')
    for bi, new in changes.items():
        st = _storages(b, base, local, glob_, kinds, order[bi])
        if st is None:
            raise MaterialError(f'body {bi}: not a storage mesh')
        if len(new) != len(st):
            raise MaterialError(f'body {bi}: {len(new)} subpart lists for {len(st)} subpart storages')
        for i, ((m, k), vals) in enumerate(zip(st, new)):
            if len(vals) != k:
                raise MaterialError(f'body {bi} subpart {i}: {len(vals)} materials for {k}')
            if any(not 0 <= x < n for x in vals):
                raise MaterialError(f'body {bi} subpart {i}: an index past the {n} HavokMaterials')
            if k:
                struct.pack_into(f'<{k}I', out, base + m, *vals)
    return bytes(out)
