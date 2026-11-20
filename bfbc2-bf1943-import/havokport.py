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

In the suite's Rust core (round F2b, rust/bc2-convert/src/havokport.rs, through importcore): restructure, the material
table's swap, shape_subparts_to_550, ufloat8_table and the physics materials' readers and writers run there when it is
built (the console converter's packfile walk already did: havok.convert), which they need (without it
bfbc2core.native.NativeMissing says how to get it; BFBC2_IMPORTER_PYTHON does not change that). Where the Python
would have raised anything but its own PortError / MaterialError (a short read, a missing table entry, a damaged
file), or in a case not reproduced (a velocity limit without a table), they raise a PortError / MaterialError that is
also a bfbc2core.native.Refused. Their Python (the *_reference functions) was retired in round RT2 against its
recorded answers (tests\f2b_havokport_replay*.json: 1,500 seeded 4.6.1 packfiles, 1,500 seeded PC resources, every
BF1943 HavokPhysicsData and BFBC2 physics resource of the game copy).
"""
import os
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import importcore  # noqa: E402

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core')
import havok  # noqa: E402
from bfbc2core import native  # noqa: E402

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


def _module():
    """The Rust core: importcore's, else bfbc2core.native.require's (NativeMissing saying how to get it)."""
    return importcore.native() or native.require('the BF1943 importer\'s physics port (havokport.py)',
                                                 'havokport_restructure')


def _call(name, *args, error=ValueError):
    """importcore.call(name, ...) on the Rust core (required): its `(result,)`, or None where it refuses."""
    if importcore.native() is not None:
        return importcore.call(name, *args, error=error)
    try:
        return getattr(_module(), name)(*args)
    except ValueError as e:
        raise error(str(e)) from None
    except (TypeError, OverflowError):
        return None


def _answer(r, what, error):
    """r[0], or the refusal (an `error` that is also a bfbc2core.native.Refused)."""
    if r is None:
        raise native.refused(f'{what}: the resource is damaged or of a layout this port does not read', error)
    return r[0]


def ufloat8_table(exe):
    """Havok's hkUFloat8 decoding table (256 rising floats: 0, 0.01, ... about 1e6) found in the user's executable
    (bytes); None if absent. 5.5.0 stores a rigid body's velocity limits as an index into it."""
    return _module().havokport_ufloat8_table(bytes(exe))


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


_layouts: list = [None, None]          # [the classes dict last handed to the Rust core, its PortLayouts]


def _port_layouts(classes):
    """The class layouts restructure asks, as the Rust core's PortLayouts (made once per dict, kept with it)."""
    n = _module()
    if _layouts[0] is not classes:
        aabb = {}
        for name in AABB_W:
            r = _rename(name, classes)
            if r is not None:
                try:
                    aabb[r] = _member_offset(classes, r, 'aabbCenter')
                except (KeyError, AttributeError):
                    aabb[r] = None
        try:
            _layouts[:] = [classes, n.PortLayouts(set(classes), aabb)]
        except (TypeError, OverflowError):
            _layouts[:] = [classes, None]
    return _layouts[1]


def restructure(b, template, classes, table=None):
    """The 4.6.1 resource b as a big-endian 5.5.0 one (DICE's wrapper kept); template: a BFBC2 PC physics resource
    whose class-name section names every class b's objects need; table: ufloat8_table (else only VELOCITY_BYTE's
    velocity limits are known)."""
    lay = _port_layouts(classes)
    if lay is None:
        raise native.refused('class layouts the port does not take', PortError)
    r = _call('havokport_restructure', bytes(b), bytes(template), lay, None if table is None else list(table),
              error=PortError)
    if r is None:
        raise native.refused('the 4.6.1 physics resource is damaged, or holds a value this port does not convert (a '
                             'velocity limit other than 200 without the hkUFloat8 table)', PortError)
    return r[0]


def wrapper_run_table(b):
    """(offset, M, C, big-endian) of the material table that opens the wrapper's byte run (bfbc2-console-convert
    FORMAT.md 1.1): u16 M groups, u16 C material hashes, C u32 hashes, M u16 group ends (the last = C), then the
    per-part bytes. BFBC2 writes the table little-endian on both platforms; BF1943 big-endian. None: no table found
    in either byte order."""
    return _answer(_call('havokport_run_table', bytes(b)), 'the wrapper\'s material table', PortError)


def _run_table_to_pc(b):
    """The wrapper's material table in BFBC2's byte order (little-endian)."""
    return _answer(_call('havokport_run_table_to_pc', bytes(b), error=PortError), 'the wrapper\'s material table',
                   PortError)


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
    return tuple(_answer(_call('havokport_subparts', bytes(pc)), 'the shape subparts', PortError))


def convert(b, template, classes, table=None):
    """BF1943's physics resource b -> BFBC2's PC format (table: ufloat8_table, for velocity limits other than 200).
    The wrapper's material table is swapped too: BF1943 writes it big-endian, and BFBC2's loader, which reads its
    counts to find the packfile, otherwise lands short of it (a null version string at load, 0xC585D8). The model
    groups' shape subparts get 5.5.0's field order (shape_subparts_to_550)."""
    pc = havok.convert(restructure(b, template, classes, table), classes)
    r = _call('havokport_finish', bytes(pc), error=PortError)
    return r[0] if r is not None else shape_subparts_to_550(_run_table_to_pc(pc))[0]


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
    return tuple(_answer(_call('havokport_wrapper', bytes(b), error=MaterialError), 'the wrapper', MaterialError))


def part_materials(b):
    """(the material list's hashes, each part's index into it) of a PC physics resource."""
    return tuple(_answer(_call('havokport_part_materials', bytes(b), error=MaterialError), 'the part materials',
                         MaterialError))


def part_run(indices):
    """The part run of these per-part material indices (the format part_materials reads)."""
    r = _call('havokport_part_run', indices, error=MaterialError)
    if r is None:
        raise native.refused('the part materials: indices that are not whole numbers', MaterialError)
    return r[0]


def with_part_materials(b, indices, hashes):
    """b with part i on material hashes[indices[i]]; hashes is the new material list: the old one, new hashes only
    appended (the packfile's face indices keep their meaning)."""
    return _answer(_call('havokport_with_part_materials', bytes(b), indices, hashes, error=MaterialError),
                   'the part materials', MaterialError)


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


def havok_materials(b):
    """The packfile's HavokMaterials list: the material names the face indices count in, in order."""
    return _answer(_call('havokport_havok_materials', bytes(b)), 'the HavokMaterials list', MaterialError)


def with_havok_materials(b, names):
    """b with the packfile's HavokMaterials list grown to names (the old list first; new HavokMaterial objects, name
    0, as DICE's exporter lays them out: the list's objects end the data section - a 16-byte object, its name at the
    next 16 bytes, padded to 16). DICE keeps this list equal to the material list (MaterialMappings, the wrapper's
    hashes) in every file it ships."""
    return _answer(_call('havokport_with_havok_materials', bytes(b), list(names), error=MaterialError),
                   'the HavokMaterials list', MaterialError)


STORAGE_MATERIALS_AT = 56                # hkpStorageExtendedMeshShapeMeshSubpartStorage::materials (u32 array)
BODY_NAME_AT, BODY_SHAPE_AT = 112, 16     # hkpWorldObject::name, collidable.shape
MOPP_CHILD_AT, MESH_STORAGE_AT = 52, 192  # hkpMoppBvTreeShape child.childShape, hkpStorageExtendedMeshShape::meshstorage


def bodies(b):
    """[(name, [the face materials of each subpart storage: indices into havok_materials] or None)] of the
    packfile's rigid bodies in data order (None: the body's shape is no storage mesh, e.g. a model group's
    collision body of convex children)."""
    return _answer(_call('havokport_bodies', bytes(b)), 'the rigid bodies', MaterialError)


def with_face_materials(b, changes):
    """b with face materials rewritten in place: changes {body index (bodies' order): [per subpart storage: its
    indices into havok_materials]} - the counts as bodies() lists them; only values change."""
    r = _call('havokport_with_face_materials', bytes(b), dict(changes), error=MaterialError)
    return _answer(r, 'the face materials', MaterialError)


