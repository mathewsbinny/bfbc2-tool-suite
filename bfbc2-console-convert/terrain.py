r"""terrain.py - Xbox 360 terrain resources in BFBC2's PC formats (BFBC2's own Xbox levels, Battlefield 1943's).

    import terrain
    pc = terrain.heightfield(console_bytes)                      # TerrainHeightfield
    pc = terrain.heightfield(bf1943_bytes, source='bf1943')
    pc = terrain.water_mesh(console_bytes)                       # WaterMesh

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
"""
import struct

import numpy as np

FIXED = {1024: 62474, 2048: 256012}         # the tail's second part, by size


class TerrainError(Exception):
    what_to_do = 'Check that the resource is a TerrainHeightfield / WaterMesh from an Xbox 360 copy.'


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
