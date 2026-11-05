r"""soot.py - Xbox 360 soot meshes (SootMesh: a model's scorch overlay, one per LOD, <mesh>_lod<N>_Soot.res) in BFBC2's
PC format; BFBC2's own Xbox files and Battlefield 1943's.

    import soot
    pc = soot.soot_mesh(console_bytes)

Proven on BFBC2's Xbox / PC twins (tests\test_soot.py): all 30 pairs byte for byte. Battlefield 1943's 406 soot
meshes read the same way: 396 are the empty one (16 bytes), 10 carry geometry (its beach buildings); 8 convert, the
warehouse's two have 16-byte vertices no BFBC2 file has (refused: its BFBC2 twin brings DICE's own soot).

SootMesh (PC little-endian, Xbox big-endian; the same size on both):
    u32 version (18), u32 vertex size (12; BF1943's warehouse: 16), u32, u32, u32 n
                                                              empty: version 18 and 12 zero bytes, nothing more
    n x 6 f32                                                 a bounding box per part
    u32 1, u32 vertex bytes V, u32 index bytes I, u32 0
    V / 12 vertices: 4 x u16, u32
    I / 2 indices: u16
    256 bytes: 64 x u32
"""
import struct

import numpy as np

EMPTY = 16
TAIL = 256


class SootError(Exception):
    what_to_do = 'Check that the resource is a SootMesh from an Xbox 360 copy.'


def soot_mesh(x):
    """An Xbox SootMesh (BFBC2's or BF1943's) as BFBC2's PC file."""
    out = bytearray(len(x))

    def swap(dt, n, at):
        if at + n * np.dtype(dt).itemsize > len(x):
            raise SootError(f'soot mesh: {n} x {dt} at {at:#x} runs past the end ({len(x)} bytes)')
        out[at:at + n * np.dtype(dt).itemsize] = np.frombuffer(x, '>' + dt, n, at).astype('<' + dt).tobytes()
    if len(x) == EMPTY:
        swap('u4', 4, 0)
        return bytes(out)
    n = struct.unpack_from('>I', x, 0x10)[0]
    swap('u4', 5, 0)
    swap('f4', 6 * n, 0x14)
    h = 0x14 + 24 * n
    swap('u4', 4, h)
    _, vb, ib, _ = struct.unpack_from('>4I', x, h)
    o = h + 16
    stride = struct.unpack_from('>I', x, 4)[0]
    if vb % stride or ib % 2 or o + vb + ib + TAIL != len(x):
        raise SootError(f'soot mesh: {vb} vertex and {ib} index bytes do not fit a {len(x)}-byte file')
    if stride != 12:
        raise SootError(f'soot mesh: {stride}-byte vertices (only the 12-byte one is proven: every BFBC2 soot mesh has it;'
                        f' BF1943\'s warehouse has 16)')
    be = np.dtype([('h', '>u2', 4), ('w', '>u4')])
    le = np.dtype([('h', '<u2', 4), ('w', '<u4')])
    out[o:o + vb] = np.frombuffer(x, be, vb // 12, o).astype(le).tobytes()
    o += vb
    swap('u2', ib // 2, o)
    swap('u4', TAIL // 4, o + ib)
    return bytes(out)
