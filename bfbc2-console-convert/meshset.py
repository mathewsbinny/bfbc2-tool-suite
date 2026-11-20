r"""Xbox 360 -> PC conversion of mesh sets (RigidMeshSet, CompositeMeshSet, SkinnedMeshSet, TreeMeshSet resources: a
mesh's LODs, their shader names, bounds, MeshData sizes and, for a composite or tree mesh, its parts; a tree's
billboard shader). Format: FORMAT.md section 4.

    import meshset
    s = meshset.parse(console_bytes, 'CompositeMeshSet', big_endian=True)
    pc_bytes = meshset.convert(console_bytes, 'CompositeMeshSet', [each converted LOD's MeshData bytes])

A LOD's size field is its MeshData's vertex bytes + index bytes + 12 (all 2,463 console and 619 PC mp_common LODs),
not the file's length: lod_size().

parse and write run in the suite's Rust core (round F0: bc2_native.meshset_parse / meshset_write, the same dicts
and bytes). parse's Python (parse_reference) was retired in round RT2 against its recorded answers
(bfbc2-console-convert\\tests\\f0_meshdata_replay*.json): a set it does not read (cut short or damaged) raises a
ConvertError that is also a bfbc2core.native.Refused. write's Python (write_reference) stays the fallback: OPFOR
edits the dicts it writes (bounds, sizes, shader names) and values the core does not take go to it.
"""
import struct

KINDS = ('RigidMeshSet', 'CompositeMeshSet', 'SkinnedMeshSet', 'TreeMeshSet')
# A TreeMeshSet's LOD is a composite one (parts: the tree's levels - trunk, branches, fronds - and the shader masks)
# followed by the name of the shader that draws the tree as a distant billboard ('' when none) and a number (1 for
# the palms, 0 for the bush - its meaning not yet known; carried over). Battlefield 1943's and BFBC2's are the same
# layout (the palm's: 2,199 bytes in both).
COMPOSITE_LIKE = ('CompositeMeshSet', 'TreeMeshSet')


class ConvertError(Exception):
    pass


def _native():
    import mesh
    return mesh._native()


def parse(b, kind, big_endian, flag=None):
    """{a, z, lod_settings, lods: [{x, sets: [(name, [shader names])], bounds, size, extra, flag, parts, masks}]}.
    flag: whether a rigid set's LODs have the flag byte - BFBC2's do (PC and Xbox), Battlefield 1943's do not (its
    rigid sets are one byte a LOD shorter); None: BFBC2's rule. A set read without it gets flag 0, written as BFBC2's.
    In the Rust core (round F0)."""
    if kind not in KINDS:
        raise ConvertError(f'{kind}: only {", ".join(KINDS)} are converted')
    if isinstance(b, bytearray | memoryview):
        b = bytes(b)
    got = None
    if type(b) is bytes:
        got = _native().meshset_parse(b, kind, bool(big_endian), None if flag is None else bool(flag))
    if got is None:
        from bfbc2core import native
        raise native.refused(f'{kind}: a mesh set the reader does not read (cut short or damaged)', ConvertError)
    return got


def write(s, kind):
    """PC (little-endian) bytes: in the Rust core (round F0), else write_reference (values the core does not take)."""
    nat = _native() if isinstance(kind, str) else None
    got = nat.meshset_write(s, kind) if nat is not None else None
    return got if got is not None else write_reference(s, kind)


def write_reference(s, kind):
    """The Python writer (the reference)."""
    out = bytearray(struct.pack('<IB', s['a'], s['z']) + s['lod_settings'] + b'\0')
    out += struct.pack('<I', len(s['lods']))
    for L in s['lods']:
        out += bytes([L['x'], L['shaders'], len(L['sets'])])
        for name, shaders in L['sets']:
            out += name + b'\0' + b''.join(x + b'\0' for x in shaders)
        out += struct.pack('<6f2I', *L['bounds'], L['size'], L['extra'])
        if kind != 'SkinnedMeshSet':
            out += bytes([L['flag']])
        if kind in COMPOSITE_LIKE:
            out += struct.pack('<I', len(L['parts']))
            for h, name, bounds, transform in L['parts']:
                out += struct.pack('<I', h) + name + b'\0' + struct.pack('<6f12f', *bounds, *transform)
            for m in L['masks']:
                out += struct.pack('<3Q', *m)
        if kind == 'TreeMeshSet':
            out += L['billboard'] + b'\0' + struct.pack('<I', L['tree_value'])
        if kind == 'SkinnedMeshSet':
            out += struct.pack('<I', len(L['bones'])) + struct.pack('<%dI' % len(L['bones']), *L['bones'])
            out += struct.pack('<%dI' % len(L['bone_hashes']), *L['bone_hashes'])
    return bytes(out)


def lod_size(meshdata_bytes):
    """The size a mesh set records for a PC MeshData: vertex bytes + index bytes + 12."""
    import mesh
    m = mesh.parse(meshdata_bytes, False)
    return m['vertex_bytes'] + m['index_bytes'] + 12


def convert(b, kind, lods=None):
    """A console mesh set -> the PC one; lods: each LOD's converted (PC) MeshData bytes - their sizes are recorded
    (lod_size) - or the sizes themselves as numbers; None keeps the console's."""
    s = parse(b, kind, True)
    if lods is not None:
        if len(lods) != len(s['lods']):
            raise ConvertError(f'{len(s["lods"])} LODs, {len(lods)} given')
        for L, n in zip(s['lods'], lods):
            L['size'] = lod_size(n) if isinstance(n, (bytes, bytearray)) else n
    return write(s, kind)
