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

    import havokport
    pc_bytes = havokport.convert(bf1943_bytes, template_pc_bytes, havok.classes(exe))
"""
import os
import struct
import sys

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


AABB_W = {'hkListShape', 'hkStorageExtendedMeshShape'}


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


def shape_subparts_to_550(pc):
    """A PC packfile's extended mesh shapes with each shape subpart's placement in 5.5.0's order: 4.6.1 keeps the
    translation at 32 and the rotation quaternion at 48, 5.5.0 the rotation at 32 and the translation at 48. The
    model groups' physics (forest, middle growth, static and rigid groups: one subpart per placed object) carry them;
    left in 4.6.1's order, each object's position was read as its rotation - Wake's palms drew as sheets hundreds of
    metres long across the sky. Swapped only where the data shows it (a subpart whose 'rotation' is no unit
    quaternion and whose 'translation' is one). Returns (bytes, subparts swapped)."""
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
    swapped = 0
    for o in range(base + virt, base + exp - 11, 12):
        off, _, cn = struct.unpack_from('<3I', b, o)
        if off == 0xFFFFFFFF or names.get(cn) not in SHAPE_MESHES:
            continue
        arr, cnt = local.get(off + SUBPARTS_AT), struct.unpack_from('<I', b, base + off + SUBPARTS_AT + 4)[0]
        if arr is None:
            continue
        for k in range(cnt):
            r = base + arr + SUBPART_SIZE * k
            rot = np.frombuffer(bytes(b[r + 32:r + 48]), '<f4')
            tra = np.frombuffer(bytes(b[r + 48:r + 64]), '<f4')
            if abs(np.linalg.norm(rot) - 1) > 1e-2 and abs(np.linalg.norm(tra) - 1) < 1e-2:
                b[r + 32:r + 48], b[r + 48:r + 64] = bytes(tra), bytes(rot)
                swapped += 1
    return bytes(b), swapped


def convert(b, template, classes, table=None):
    """BF1943's physics resource b -> BFBC2's PC format (table: ufloat8_table, for velocity limits other than 200).
    The wrapper's material table is swapped too: BF1943 writes it big-endian, and BFBC2's loader, which reads its
    counts to find the packfile, otherwise lands short of it (a null version string at load, 0xC585D8). The model
    groups' shape subparts get 5.5.0's field order (shape_subparts_to_550)."""
    pc = _run_table_to_pc(havok.convert(restructure(b, template, classes, table), classes))
    return shape_subparts_to_550(pc)[0]
