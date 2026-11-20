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

soot_mesh runs in the suite's Rust core (round F2a: bc2_convert::terrain::soot_mesh, through terrain.py; the same
bytes and SootError messages). Its Python (soot_mesh_reference) was retired in round RT2 against its recorded answers
(bfbc2-console-convert\\tests\\f2a_console_replay*.json): where it raised another error (a read past the data) a
SootError that is also a bfbc2core.native.Refused says the mesh is cut short or damaged.
"""
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)
import terrain  # noqa: E402

EMPTY = 16
TAIL = 256


class SootError(Exception):
    what_to_do = 'Check that the resource is a SootMesh from an Xbox 360 copy.'


def soot_mesh(x):
    """An Xbox SootMesh (BFBC2's or BF1943's) as BFBC2's PC file."""
    x = terrain._bytes(x)
    if type(x) is not bytes:
        raise TypeError(f'soot mesh: bytes needed, not {type(x).__name__}')
    return terrain._answer(terrain._native().terrain_convert('soot_mesh', x), SootError, 'soot mesh')[0]
