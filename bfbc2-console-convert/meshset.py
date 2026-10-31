r"""Xbox 360 -> PC conversion of mesh sets (RigidMeshSet, CompositeMeshSet resources: a mesh's LODs, their shader
names, bounds, MeshData sizes and, for a composite mesh, its parts). Format: FORMAT.md section 4.

    import meshset
    s = meshset.parse(console_bytes, 'CompositeMeshSet', big_endian=True)
    pc_bytes = meshset.convert(console_bytes, 'CompositeMeshSet', [each converted LOD's MeshData bytes])

A LOD's size field is its MeshData's vertex bytes + index bytes + 12 (all 2,463 console and 619 PC mp_common LODs),
not the file's length: lod_size().
"""
import struct

KINDS = ('RigidMeshSet', 'CompositeMeshSet', 'SkinnedMeshSet')


class ConvertError(Exception):
    pass


class _Reader:
    def __init__(self, b, be):
        self.b, self.p, self.e = b, 0, '>' if be else '<'

    def num(self, fmt):
        v = struct.unpack_from(self.e + fmt, self.b, self.p)
        self.p += struct.calcsize(fmt)
        return v if len(v) > 1 else v[0]

    def u8(self):
        self.p += 1
        return self.b[self.p - 1]

    def cstr(self):
        e = self.b.index(b'\0', self.p)
        s = self.b[self.p:e]
        self.p = e + 1
        return s


def parse(b, kind, big_endian):
    """{a, z, lod_settings, lods: [{x, sets: [(name, [shader names])], bounds, size, extra, flag, parts, masks}]}"""
    if kind not in KINDS:
        raise ConvertError(f'{kind}: only {", ".join(KINDS)} are converted')
    r = _Reader(b, big_endian)
    s = {'a': r.num('I'), 'z': r.u8(), 'lod_settings': r.cstr(), 'lods': []}
    for _ in range(r.num('I')):
        x, n, sets = r.u8(), r.u8(), r.u8()
        L = {'x': x, 'shaders': n, 'sets': [(r.cstr(), [r.cstr() for _ in range(n)]) for _ in range(sets)]}
        L['bounds'] = r.num('6f')
        L['size'], L['extra'] = r.num('2I')
        L['flag'] = r.u8() if kind != 'SkinnedMeshSet' else None        # skinned sets: no flag byte
        if kind == 'CompositeMeshSet':
            # parts: hash, name, bounds (6 floats), transform (3 rotation rows and the position: 12 floats);
            # then per shader a 192-bit mask of the parts using it
            L['parts'] = [(r.num('I'), r.cstr(), r.num('6f'), r.num('12f')) for _ in range(r.num('I'))]
            L['masks'] = [r.num('3Q') for _ in range(n)]
        if kind == 'SkinnedMeshSet':
            # the skeleton bones the LOD's parts use, then their name hashes
            nb = r.num('I')
            L['bones'] = [r.num('I') for _ in range(nb)]
            L['bone_hashes'] = [r.num('I') for _ in range(nb)]
        s['lods'].append(L)
    if r.p != len(b):
        raise ConvertError(f'{kind}: {len(b) - r.p} bytes after the last LOD')
    return s


def write(s, kind):
    """PC (little-endian) bytes."""
    out = bytearray(struct.pack('<IB', s['a'], s['z']) + s['lod_settings'] + b'\0')
    out += struct.pack('<I', len(s['lods']))
    for L in s['lods']:
        out += bytes([L['x'], L['shaders'], len(L['sets'])])
        for name, shaders in L['sets']:
            out += name + b'\0' + b''.join(x + b'\0' for x in shaders)
        out += struct.pack('<6f2I', *L['bounds'], L['size'], L['extra'])
        if kind != 'SkinnedMeshSet':
            out += bytes([L['flag']])
        if kind == 'CompositeMeshSet':
            out += struct.pack('<I', len(L['parts']))
            for h, name, bounds, transform in L['parts']:
                out += struct.pack('<I', h) + name + b'\0' + struct.pack('<6f12f', *bounds, *transform)
            for m in L['masks']:
                out += struct.pack('<3Q', *m)
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
