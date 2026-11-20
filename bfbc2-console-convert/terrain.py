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
A sub-terrain's heightfield (the ring around the main block: Heightfield-001 ... -012 of BFBC2's mp_002 and BF1943's
Guadalcanal and Iwo Jima; 128 samples on the Xbox, 64 in DICE's PC copies) has neither offsets nor sections: its
header 3 x u32 is (n, log2(n) - 3, 0), 0x24 - 0x2D are 0 (A = 0), and after the heights comes a min / max quadtree of
16-sample cells - u16 pairs, 1 + 4 + ... + (n / 16)^2 nodes (85 at 128: 340 bytes on BFBC2's Xbox mp_002 and BF1943's
levels alike; 21 at 64 on DICE's PC mp_002). SUB_SIZES: the sizes read this way.

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

Every function here runs in the suite's Rust core (round F2a: bc2_convert::terrain through bc2_native.terrain_*;
NativeMissing, saying how to get it, without it). Their Python (the *_reference functions) was retired in round RT2
against its recorded answers (bfbc2-console-convert\\tests\\f2a_console_replay*.json): where it raised another error
than a TerrainError (a read past the data: a resource cut short or damaged), a TerrainError that is also a
bfbc2core.native.Refused says so. _Walk (the field walker) and ROAD_VERTEX stay: OPFOR's wake_install walks a
VisualTerrain with them.
"""
import os
import sys
from typing import Any

import numpy as np


def _native() -> Any:
    """The Rust core with the terrain functions (bfbc2core.native.require): NativeMissing, saying how to get it."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('terrain conversion (bfbc2-console-convert/terrain.py)', 'terrain_convert')


def _bytes(x: Any) -> Any:
    return bytes(x) if isinstance(x, bytearray | memoryview) else x


def _answer(got: Any, error: type = Exception, what: str = 'terrain resource') -> Any:
    """A native answer: (True, value) -> (value,); (False, message) -> error(message) raised; None (the retired
    reference raised another error there: a read past the data) -> a Refused error."""
    if got is None:
        from bfbc2core import native
        raise native.refused(f'{what}: cut short or damaged (a read past the data)', error)
    if not got[0]:
        raise error(got[1])
    return (got[1],)


def _run(name: str, x: Any) -> Any:
    """The Rust core's terrain_convert(name, x) as (value,)."""
    x = _bytes(x)
    if type(x) is not bytes:
        raise TypeError(f'{name.replace("_", " ")}: bytes needed, not {type(x).__name__}')
    return _answer(_native().terrain_convert(name, x), TerrainError, name.replace('_', ' '))


FIXED = {1024: 62474, 2048: 256012}         # the tail's second part, by size
SUB_SIZES = (64, 128)                       # sub-terrain heightfields: a min / max quadtree after the heights


def quadtree_nodes(n):
    """The nodes of a sub-terrain heightfield's min / max quadtree: 16-sample cells, every level up to one node."""
    return sum(4 ** k for k in range((n // 16).bit_length()))


class TerrainError(Exception):
    what_to_do = ('Check that the resource is a TerrainHeightfield / WaterMesh / TerrainMaterialMap / VisualTerrain / '
                  'VisualWater from an Xbox 360 copy (BFBC2 or Battlefield 1943).')


def heightfield(x, source='bfbc2'):
    """An Xbox TerrainHeightfield (BFBC2's, or source='bf1943': Battlefield 1943's) as BFBC2's PC file."""
    x = _bytes(x)
    if type(x) is not bytes:
        raise TypeError(f'heightfield: bytes needed, not {type(x).__name__}')
    return _answer(_native().terrain_heightfield(x, bool(source == 'bfbc2')), TerrainError, 'heightfield')[0]


def water_mesh(x):
    """An Xbox WaterMesh as BFBC2's PC file."""
    return _run('water_mesh', x)[0]


def material_map(x):
    """An Xbox TerrainMaterialMap (BFBC2's or BF1943's) as BFBC2's PC file."""
    return _run('material_map', x)[0]


class _Walk:
    """(OPFOR's wake_install walks a PC VisualTerrain with it.) Walks a resource field by field (e: '>' an Xbox file, '<' a PC file), writing each field little-endian into a
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


def visual_terrain(x):
    """An Xbox VisualTerrain (BFBC2's or BF1943's) as BFBC2's PC file (the same size)."""
    return _run('visual_terrain', x)[0]


ROAD_STRIDE = 28


def road_strips(pc):
    """A PC VisualTerrain's road blocks (each terrain block's full-detail roads, then each one's low-detail ones):
    [{'block': i, 'strips': [{'shader', 'second', 'mask', 'first', 'triangles', 'bounds', 'layer'}], 'vertices':
    bytes (V x 28: world position float3, texture coordinate float2, 4 x f16), 'indices': uint32 array}] - the baked
    road and decal strips, each drawn with its shader entry ('second': a second pass, e.g. "..._Zonly"); 'first' /
    'triangles' a range of the block's indices, 'bounds' 6 floats as stored."""
    pc = _bytes(pc)
    if type(pc) is not bytes:
        raise TypeError(f'visual terrain: bytes needed, not {type(pc).__name__}')
    got = _answer(_native().terrain_road_strips(pc), TerrainError, 'visual terrain')
    for b in got[0]:
        b['indices'] = np.frombuffer(b['indices'], np.uint32).copy()
    return got[0]


def road_positions(vertices):
    """(V, 3) float32 world positions of a road block's vertices."""
    return np.frombuffer(vertices, '<f4').reshape(-1, ROAD_STRIDE // 4)[:, :3].copy()


def visual_terrain_parts(pc):
    """A PC VisualTerrain's parts: [(name, bytes)] ('blocks', 'roads 0' ..., 'patch shaders', 'patch map 0', 'patch
    map 0 cells', 'patch map 0 names', ...)."""
    pc = _bytes(pc)
    if type(pc) is not bytes:
        raise TypeError(f'visual terrain: bytes needed, not {type(pc).__name__}')
    return _answer(_native().terrain_visual_terrain_parts(pc), TerrainError, 'visual terrain')[0]


WATER_VERTEX = {'>': [('pos', 'u2', 4), ('n', 'u4', 1), ('t', 'u4', 1), ('uv', 'u2', 6)],      # Xbox: 28 bytes
                 '<': [('pos', 'u2', 4), ('n', 'f2', 4), ('t', 'f2', 4), ('uv', 'u2', 6)]}      # PC: 36 bytes


def _water_vertex(e):
    return np.dtype([(k, e + t, c) if c > 1 else (k, e + t) for k, t, c in WATER_VERTEX[e]])


def visual_water(x):
    """An Xbox VisualWater (BFBC2's or BF1943's) as BFBC2's PC file: the vertices grow from 28 to 36 bytes (normal
    and tangent DEC3N -> 4 x f16); exact except those two vectors (the Xbox keeps 10 bits of each)."""
    return _run('visual_water', x)[0]


def visual_water_parts(pc):
    """A PC VisualWater's vertices (numpy: pos, n, t, uv) and other parts [(name, bytes)]: indices, materials, draw
    ranges."""
    pc = _bytes(pc)
    if type(pc) is not bytes:
        raise TypeError(f'visual water: bytes needed, not {type(pc).__name__}')
    n, parts = _answer(_native().terrain_visual_water_parts(pc), TerrainError, 'visual water')[0]
    return np.frombuffer(pc, _water_vertex('<'), n, 12), parts


def material_names(pc):
    """The material names of a PC TerrainMaterialMap."""
    pc = _bytes(pc)
    if type(pc) is not bytes:
        raise TypeError(f'material map: bytes needed, not {type(pc).__name__}')
    return _answer(_native().terrain_material_names(pc), TerrainError, 'material map')[0]


