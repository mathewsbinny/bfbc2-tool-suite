r"""terrain.py - Xbox 360 terrain resources in BFBC2's PC formats (BFBC2's own Xbox levels, Battlefield 1943's).

    import terrain
    pc = terrain.heightfield(console_bytes)                      # TerrainHeightfield
    pc = terrain.heightfield(bf1943_bytes, source='bf1943')
    pc = terrain.water_mesh(console_bytes)                       # WaterMesh
    pc = terrain.material_map(console_bytes)                     # TerrainMaterialMap
    pc = terrain.visual_terrain(console_bytes)                   # VisualTerrain (<lv>.visual.res)
    pc = terrain.visual_water(console_bytes)                     # VisualWater (<lv>.water.res)

Proven on BFBC2's Xbox / PC twins (tests\test_terrain.py): the converted Xbox heightfield equals the PC file byte for
byte on the 7 levels DICE did not edit after the Xbox build; on the other 13 the heights differ in places (PC terrain
edits), the structure is the same.

TerrainHeightfield (PC, little-endian):
    0x00  6 x f32   world bounds: min x, y, z, max x, y, z
    0x18  3 x u32   size n (samples per side: 1024 / 2048), log2(n) - 3, log2(n) - 2
    0x24  2 x u16   0, n / 256
    0x28  u32       0 in every BFBC2 level
    0x2C  u8        0 in every BFBC2 level
    0x2D  u32       A: the length in bytes of the tail's first section
    0x31  u16 x n*n heights
    then  u32 x n   offsets (in u16 units) into the tail's first section
          u16 ...   the first section (A bytes), then a part of fixed length (62,474 bytes at n = 1024, 256,012 at
                    2048; its last block 4,098 values in BFBC2's mp_005 and BF1943's Wake Island alike)
Battlefield 1943's header has no A (heights at 0x2D) and other values at 0x28 (707 on Wake Island) and 0x2C (0x40):
A is computed from the tail, those two written as BFBC2 has them.

WaterMesh: 4-byte values throughout (all levels).

TerrainMaterialMap (all 20 levels byte for byte; BF1943's Wake Island, Iwo Jima, Guadalcanal read the same way):
    u32 width (2048 / 4096), u16 blocks b (32 / 128)
    b x { u32 length, length bytes (the material data: bytes, as they are), u16 x 256 }
    then { u32 length, length bytes } to the end: the material names, NUL-separated ("defaultmaterial", "Grass",
    "Dirt", ...; BF1943's include DefaultGround, Water_solidground)

VisualTerrain (the same size on both; strings NUL-terminated):
    u32 N                   terrain blocks: 1 (mp_003 - mp_005, Wake Island, Coral Sea), or the main block and its
                            ring of sub-terrains (13: Heightfield-001 ... -012 around the main; mp_002 25)
    2N x roads              each block's full-detail roads, then each block's low-detail ones; mostly empty (16 bytes)
        u32 n; n x { shader, second shader (e.g. "..._Zonly", mostly empty), mask (NullUgMask), u32 first index,
                     u32 triangles, 6 x f32 bounds, u32 layer (0 - 7) }
        u32 V, u32 V * 28, u32 I; V x { 5 x f32, 4 x f16 }; I x u32 indices
    u32 m; m x patch shader { patch mask shader, undergrowth mask shader (may be empty), u32 layers (2 - 6), u32,
        u32 g; g x { u32 k; k x mesh { mesh name, u8 0, shader (may be empty), 12 x f32 (density, scale ... 55.0),
                                     u8 1, u8 0 } } }
    u32 N; N x patch map { 6 x f32 bounds, u32 66, u32 1, u32 (15 / 31), u32 (7 / 15), u32 W (cells per side: 128 or 64
        for the main block, 8 for a sub-terrain), W*W x 4 x u16, W*W x 4 x u8, u32 W, W*W x u8, the block's heightfield
        name, the mask tile atlas name }
    Byte for byte on mp_003, mp_005, mp_005sr; elsewhere every part is but DICE's PC edits: the roads of mp_001, mp_004,
    mp_006, mp_008, mp_012 rebuilt (patches reordered), mp_008's patch shaders, and the sub-terrain patch maps, 4 x 4 on
    the PC (its sub-heightfields have 64 samples, the Xbox's 128: the Xbox's 8 x 8 kept, matching the Xbox heightfields).

VisualWater (the PC's 15,216 bytes larger on mp_005: 8 bytes more per vertex, 1,902 vertices):
    u32 10 (vertex format), u32 stride (Xbox 28, PC 36), u32 vertex bytes
    vertices  Xbox { 4 x f16 position, u32 normal DEC3N, u32 tangent DEC3N, 6 x f16 }
              PC   { 4 x f16 position, 4 x f16 normal,   4 x f16 tangent,   6 x f16 }
              (DEC3N: x, y, z signed 10 bits / 511, w 2 bits; a zero vector is (-1.5, -1.5, -1.5, 0) on the PC)
    u32 index bytes, u32 indices; u32 4 materials x { shader, second shader (may be empty) }; u32 r draw ranges x
    { u32 material, u32 triangles, u32 first index, u32 index count }; f32 8192
    Everything but the normals / tangents byte for byte on all 20 levels (whole files on 6); those decoded from the
    Xbox's 10 bits: 91% of the components exact, the rest within 1 / 511 (the Xbox's rounding lost the PC's last bits).
"""
import struct

import numpy as np

FIXED = {1024: 62474, 2048: 256012}         # the tail's second part, by size


class TerrainError(Exception):
    what_to_do = ('Check that the resource is a TerrainHeightfield / WaterMesh / TerrainMaterialMap / VisualTerrain / '
                  'VisualWater from an Xbox 360 copy (BFBC2 or Battlefield 1943).')


def _swap(x, out, dt, count, at, to=None):
    to = at if to is None else to
    out[to:to + count * np.dtype(dt).itemsize] = np.frombuffer(x, '>' + dt, count, at).astype('<' + dt).tobytes()


def heightfield(x, source='bfbc2'):
    """An Xbox TerrainHeightfield (BFBC2's, or source='bf1943': Battlefield 1943's) as BFBC2's PC file."""
    n = struct.unpack_from('>I', x, 0x18)[0]
    if n not in FIXED:
        raise TerrainError(f'heightfield size {n}: only {sorted(FIXED)} are known')
    head = 0x31 if source == 'bfbc2' else 0x2D
    t = head + 2 * n * n
    region = len(x) - t - 4 * n
    A = region - FIXED[n]
    if A < 0 or region % 2:
        raise TerrainError(f'heightfield: a tail of {len(x) - t} bytes does not fit size {n}')
    out = bytearray(0x31 + 2 * n * n + 4 * n + region)
    _swap(x, out, 'f4', 6, 0)
    _swap(x, out, 'u4', 3, 0x18)
    _swap(x, out, 'u2', 2, 0x24)
    if source == 'bfbc2':
        _swap(x, out, 'u4', 1, 0x28)
        out[0x2C] = x[0x2C]
        if struct.unpack_from('>I', x, 0x2D)[0] != A:
            raise TerrainError(f'heightfield: the header says the first section is {struct.unpack_from(">I", x, 0x2D)[0]}'
                               f' bytes, the tail {A}')
    struct.pack_into('<I', out, 0x2D, A)
    T = 0x31 + 2 * n * n
    _swap(x, out, 'u2', n * n, head, 0x31)
    _swap(x, out, 'u4', n, t, T)
    _swap(x, out, 'u2', region // 2, t + 4 * n, T + 4 * n)
    return bytes(out)


def water_mesh(x):
    """An Xbox WaterMesh as BFBC2's PC file."""
    if len(x) % 4:
        raise TerrainError(f'water mesh: {len(x)} bytes, not 4-byte values')
    return np.frombuffer(x, '>u4').astype('<u4').tobytes()


def material_map(x):
    """An Xbox TerrainMaterialMap (BFBC2's or BF1943's) as BFBC2's PC file."""
    out = bytearray(x)
    w, b = struct.unpack_from('>IH', x, 0)
    struct.pack_into('<IH', out, 0, w, b)
    o = 6
    for _ in range(b):
        L = struct.unpack_from('>I', x, o)[0]
        struct.pack_into('<I', out, o, L)
        o += 4 + L
        if o + 512 > len(x):
            raise TerrainError(f'material map: block runs past the end ({o + 512} > {len(x)})')
        _swap(x, out, 'u2', 256, o)
        o += 512
    while o < len(x):
        L = struct.unpack_from('>I', x, o)[0]
        struct.pack_into('<I', out, o, L)
        o += 4 + L
    if o != len(x):
        raise TerrainError(f'material map: the names end at {o}, the file at {len(x)}')
    return bytes(out)


class _Walk:
    """Walks a resource field by field (e: '>' an Xbox file, '<' a PC file), writing each field little-endian into a
    copy of the same size; mark() names the parts walked (for comparing files part by part)."""

    def __init__(self, x, what, e='>'):
        self.x, self.out, self.o, self.what, self.e = x, bytearray(x), 0, what, e
        self.parts, self.at, self.name = [], 0, None

    def mark(self, name):
        if self.name is not None:
            self.parts.append((self.name, self.at, self.o))
        self.name, self.at = name, self.o

    def need(self, n):
        if self.o + n > len(self.x):
            raise TerrainError(f'{self.what}: runs past the end at {self.o:#x} (+{n} > {len(self.x)} bytes)')

    def arr(self, dt, count):
        """count values of numpy type dt ('u4', 'f4', 'u2', 'u1', or a structured list [(type, count), ...])."""
        be = np.dtype([(f'f{i}', self.e + t, c) for i, (t, c) in enumerate(dt)]) if isinstance(dt, list) \
            else np.dtype(self.e + dt)
        size = count * be.itemsize
        self.need(size)
        v = np.frombuffer(self.x, be, count, self.o)
        self.out[self.o:self.o + size] = v.astype(be.newbyteorder('<')).tobytes()
        self.o += size
        return v

    def u32(self):
        return int(self.arr('u4', 1)[0])

    def string(self):
        e = self.x.find(b'\0', self.o)
        if e < 0:
            raise TerrainError(f'{self.what}: a name at {self.o:#x} has no end')
        s = self.x[self.o:e].decode('latin-1')
        self.o = e + 1
        return s

    def done(self):
        self.mark(None)
        if self.o != len(self.x):
            raise TerrainError(f'{self.what}: the data ends at {self.o:#x}, the file at {len(self.x):#x}')
        return bytes(self.out)


ROAD_VERTEX = [('f4', 5), ('u2', 4)]        # 28 bytes: 5 x f32, 4 x f16 (half floats, swapped as u16)


def _visual_terrain(x, e):
    w = _Walk(x, 'visual terrain', e)
    w.mark('blocks')
    n = w.u32()
    if not 1 <= n <= 256:
        raise TerrainError(f'visual terrain: {n} terrain blocks - not a VisualTerrain?')
    for i in range(2 * n):                                       # roads: each block's full detail, then low detail
        w.mark(f'roads {i}')
        for _ in range(w.u32()):
            w.string(), w.string(), w.string()
            w.arr('u4', 2), w.arr('f4', 6), w.arr('u4', 1)
        V, B, I = (w.u32() for _ in range(3))
        if B != 28 * V:
            raise TerrainError(f'visual terrain: road vertices of {B} bytes for {V} (not 28 each)')
        w.arr(ROAD_VERTEX, V), w.arr('u4', I)
    w.mark('patch shaders')
    for _ in range(w.u32()):
        w.string(), w.string()
        w.arr('u4', 2)
        for _ in range(w.u32()):                                 # undergrowth groups
            for _ in range(w.u32()):                             # meshes
                w.string()
                w.arr('u1', 1)
                w.string()
                w.arr('f4', 12), w.arr('u1', 2)
    if w.u32() != n:
        raise TerrainError('visual terrain: the patch maps do not match the road blocks in number')
    for i in range(n):                                           # patch maps, one per terrain block
        w.mark(f'patch map {i}')
        w.arr('f4', 6)
        W = int(w.arr('u4', 5)[-1])
        w.mark(f'patch map {i} cells')
        w.arr('u2', 4 * W * W), w.arr('u1', 4 * W * W)
        W2 = w.u32()
        w.arr('u1', W2 * W2)
        w.mark(f'patch map {i} names')
        w.string(), w.string()
    return w


def visual_terrain(x):
    """An Xbox VisualTerrain (BFBC2's or BF1943's) as BFBC2's PC file (the same size)."""
    return _visual_terrain(x, '>').done()


def visual_terrain_parts(pc):
    """A PC VisualTerrain's parts: [(name, bytes)] ('blocks', 'roads 0' ..., 'patch shaders', 'patch map 0', 'patch
    map 0 cells', 'patch map 0 names', ...)."""
    w = _visual_terrain(pc, '<')
    w.done()
    return [(name, pc[a:b]) for name, a, b in w.parts]


def _dec3n(u):
    """Xbox DEC3N (3 x signed 10 bits, 2 bits w) as float32 x, y, z, w."""
    out = np.zeros((len(u), 4), np.float32)
    for i, sh in enumerate((0, 10, 20)):
        v = ((u >> sh) & 0x3FF).astype(np.int32)
        out[:, i] = np.where(v >= 512, v - 1024, v) / 511.0
    out[:, 3] = (u >> 30) & 3
    out[(u & 0x3FFFFFFF) == 0, :3] = -1.5                        # no vector: the PC files have -1.5 there
    return out


WATER_VERTEX = {'>': [('pos', 'u2', 4), ('n', 'u4', 1), ('t', 'u4', 1), ('uv', 'u2', 6)],      # Xbox: 28 bytes
                 '<': [('pos', 'u2', 4), ('n', 'f2', 4), ('t', 'f2', 4), ('uv', 'u2', 6)]}      # PC: 36 bytes


def _water_vertex(e):
    return np.dtype([(k, e + t, c) if c > 1 else (k, e + t) for k, t, c in WATER_VERTEX[e]])


def _visual_water(x, e):
    """(header, vertices, the walk of the rest) of a VisualWater (e '>': Xbox, '<': PC)."""
    stride = _water_vertex(e).itemsize
    if len(x) < 12:
        raise TerrainError(f'visual water: {len(x)} bytes')
    fmt, s, nb = struct.unpack_from(e + 'III', x, 0)
    if fmt != 10 or s != stride or nb % stride or 12 + nb > len(x):
        raise TerrainError(f'visual water: vertex format {fmt}, {s} bytes each, {nb} bytes - expected format 10, '
                           f'{stride} bytes ({"an Xbox 360" if e == ">" else "a PC"} file)')
    v = np.frombuffer(x, _water_vertex(e), nb // stride, 12)
    w = _Walk(x, 'visual water', e)
    w.o = 12 + nb
    w.mark('indices')
    ni = w.u32()
    if ni % 4:
        raise TerrainError(f'visual water: {ni} bytes of indices, not u32s')
    w.arr('u4', ni // 4)
    w.mark('materials')
    for _ in range(w.u32()):                                     # shader, second shader (may be empty)
        w.string(), w.string()
    w.mark('draw ranges')
    w.arr('u4', 4 * w.u32())                                     # material, triangles, first index, index count
    w.arr('f4', 1)
    w.done()
    return fmt, v, w


def visual_water(x):
    """An Xbox VisualWater (BFBC2's or BF1943's) as BFBC2's PC file: the vertices grow from 28 to 36 bytes (normal
    and tangent DEC3N -> 4 x f16); exact except those two vectors (the Xbox keeps 10 bits of each)."""
    fmt, v, w = _visual_water(x, '>')
    pc = np.zeros(len(v), _water_vertex('<'))
    pc['pos'], pc['uv'] = v['pos'], v['uv']
    pc['n'], pc['t'] = _dec3n(v['n']), _dec3n(v['t'])
    a = w.parts[0][1]
    return struct.pack('<III', fmt, pc.itemsize, pc.nbytes) + pc.tobytes() + bytes(w.out[a:])


def visual_water_parts(pc):
    """A PC VisualWater's vertices (numpy: pos, n, t, uv) and other parts [(name, bytes)]: indices, materials, draw
    ranges."""
    _, v, w = _visual_water(pc, '<')
    return v, [(name, pc[a:b]) for name, a, b in w.parts]


def material_names(pc):
    """The material names of a PC TerrainMaterialMap."""
    w, b = struct.unpack_from('<IH', pc, 0)
    o = 6
    for _ in range(b):
        o += 4 + struct.unpack_from('<I', pc, o)[0] + 512
    names = []
    while o < len(pc):
        L = struct.unpack_from('<I', pc, o)[0]
        names += [n.decode('latin-1') for n in pc[o + 4:o + 4 + L].split(b'\0') if n]
        o += 4 + L
    return names
