"""BC2 water: Terrains/<Terrain>/<Terrain>.Water.Mesh.res (type WaterMesh) and .Water.res (type VisualWater), in each
level's terrain-00.fbrb.

    w = read(folder_or_mesh_path, name)    # Water: vertices, triangles, per-triangle shader (or None)
    w.vertices (n, 3) float32, w.triangles (m, 3) uint32, w.shader[k] -> shader path of triangle k
    parse_mesh(data), parse_visual(data)   # the two files' bytes

WaterMesh (little-endian; read on all 36 levels with a terrain bundle, each file consumed exactly):

    u32           bodies (1 in every level)
    per body:     u32 vertex count, vertex count x (f32 x, y, z)
                  u32 index count, index count x u32 (triangles; 0 vertices = no water: SP_006, SP_009)

Heights are per vertex: rivers slope (SP_02: 17.9-156.8 m), seas are flat (SP_01: 4.4 m).

VisualWater (the drawing copy; only the parts used here):

    u32 10, u32 vertex stride (36), u32 vertex bytes, vertices (half-float x, y, z first, then normal and texture
    coordinates), u32 index bytes, u32 indices (identical to the mesh's on 33 of 34 levels with water),
    u32 slot count (4) and the slots' shader paths (0-terminated, each followed by one 0 byte, then
    Systems/Water/WaterOceanFragment), u32 range count and per range (u32 slot, u32 triangle count,
    u32 first index, u32 index count), then u32 0 and the byte 0x46.

parse_mesh / parse_visual / read run in the suite's Rust core (rust/, module bc2_native: bc2_formats::water) when it
is built, else in Python here; both give identical vertices, triangles and shaders and refuse the same files with
ValueError (core/tests/test_geometry_native.py: every water file of the game copy). BFBC2_PURE_PYTHON=1 (or
use_native(False)) forces Python; backend() says which one runs.
"""
import os
import struct
from dataclasses import dataclass
from types import ModuleType

import numpy as np

from . import dbx as _dbx


@dataclass
class Water:
    vertices: np.ndarray                  # (n, 3) float32
    triangles: np.ndarray                 # (m, 3) uint32
    shader: list[str] | None = None       # per triangle, from the VisualWater ranges; None if they do not match
    shaders: list[str] | None = None      # the VisualWater slots

    def __len__(self) -> int:
        return len(self.triangles)


# ------------------------------------------------------------------------------------------------ backend

def _with_water(m: ModuleType | None) -> ModuleType | None:
    """m when it has the water reader (a module built before it was added does not)."""
    return m if m is not None and hasattr(m, 'water_mesh') else None


def _load_native() -> ModuleType | None:
    """The Rust core (found as dbx finds it) unless BFBC2_PURE_PYTHON is set."""
    if os.environ.get(_dbx.PURE_PYTHON_ENV, '') not in ('', '0'):
        return None
    return _with_water(_dbx._import_native())


_native = _load_native()


def backend() -> str:
    """'rust' when water is read in the Rust core, else 'python'."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """The Rust core on (when it is built, whatever BFBC2_PURE_PYTHON says) or off for this process; returns
    backend()."""
    global _native
    _native = _with_water(_dbx._import_native()) if on else None
    return backend()


# ------------------------------------------------------------------------------------------------ the API

def _bytes(data: bytes) -> bytes:
    return data if type(data) is bytes else bytes(data)


def parse_mesh(data: bytes) -> tuple[np.ndarray, np.ndarray]:
    """(vertices, triangles) of every body of a WaterMesh's bytes, bodies joined. ValueError when the file ends
    early, a body's indices are not whole triangles or bytes are left over."""
    data = _bytes(data)
    if _native is not None:
        v, t = _native.water_mesh(data)
        return np.frombuffer(v, '<f4').reshape(-1, 3), np.frombuffer(t, '<u4').reshape(-1, 3)
    try:
        return _py_parse_mesh(data)
    except struct.error as e:
        raise ValueError(f'damaged WaterMesh ({e})') from e


def _py_parse_mesh(d: bytes) -> tuple[np.ndarray, np.ndarray]:
    bodies = struct.unpack_from('<I', d, 0)[0]
    verts: list[np.ndarray]
    tris: list[np.ndarray]
    o, verts, tris = 4, [], []
    base = 0
    for _ in range(bodies):
        n = struct.unpack_from('<I', d, o)[0]
        v = np.frombuffer(d, '<f4', n * 3, o + 4).reshape(n, 3)
        o += 4 + 12 * n
        m = struct.unpack_from('<I', d, o)[0]
        i = np.frombuffer(d, '<u4', m, o + 4).reshape(-1, 3)
        o += 4 + 4 * m
        verts.append(v)
        tris.append(i.astype(np.uint32) + base)
        base += n
    if o != len(d):
        raise ValueError(f'{len(d) - o} unread bytes - not the WaterMesh layout')
    return (np.concatenate(verts).astype(np.float32) if verts else np.zeros((0, 3), np.float32),
            np.concatenate(tris) if tris else np.zeros((0, 3), np.uint32))


def parse_visual(data: bytes) -> tuple[np.ndarray, list[str], list[tuple[int, ...]]]:
    """(indices, slot shader paths, ranges [(slot, triangles, first index, index count)]) of a VisualWater's bytes.
    ValueError when the file ends early or a name has no end."""
    data = _bytes(data)
    if _native is not None:
        idx, slots, ranges = _native.water_visual(data)
        return np.frombuffer(idx, '<u4'), slots, ranges
    try:
        return _py_parse_visual(data)
    except struct.error as e:
        raise ValueError(f'damaged VisualWater ({e})') from e


def _py_parse_visual(d: bytes) -> tuple[np.ndarray, list[str], list[tuple[int, ...]]]:
    _, stride, size = struct.unpack_from('<3I', d, 0)
    o = 12 + size
    ib = struct.unpack_from('<I', d, o)[0]
    indices = np.frombuffer(d, '<u4', ib // 4, o + 4)
    o += 4 + ib
    slots = struct.unpack_from('<I', d, o)[0]
    o += 4
    names: list[str] = []
    for _ in range(slots + 1):                 # the slots, then the fragment shader
        e = d.index(b'\0', o)
        names.append(d[o:e].decode('ascii', 'replace'))
        o = e + 1
        if len(names) <= slots and d[o:o + 1] == b'\0':
            o += 1
    count = struct.unpack_from('<I', d, o)[0]
    ranges = [struct.unpack_from('<4I', d, o + 4 + 16 * k) for k in range(count)]
    return indices, names[:slots], ranges


def _read_file(path: str) -> bytes:
    with open(path, 'rb') as fh:
        return fh.read()


def read_mesh(path: str) -> tuple[np.ndarray, np.ndarray]:
    """(vertices, triangles) of every body in a .Water.Mesh.res, bodies joined."""
    try:
        return parse_mesh(_read_file(path))
    except ValueError as e:
        raise ValueError(f'{path}: {e}') from None


def read_visual(path: str) -> tuple[np.ndarray, list[str], list[tuple[int, ...]]]:
    """(indices, slot shader paths, ranges [(slot, triangles, first index, index count)]) of a .Water.res."""
    return parse_visual(_read_file(path))


def shaders_of(mesh: bytes, visual: bytes, triangles: np.ndarray) -> tuple[list[str], list[str]] | None:
    """(the shader of each triangle, the slots) from a VisualWater's ranges, or None when it cannot be read or its
    indices are not the mesh's (triangles: the mesh's, from parse_mesh)."""
    if _native is not None:
        r = _native.water_shaders(_bytes(mesh), _bytes(visual))
        return None if r is None else (r[0], r[1])
    try:
        indices, slots, ranges = parse_visual(visual)
    except (struct.error, ValueError, IndexError):
        return None
    if not np.array_equal(indices, triangles.ravel()):
        return None
    shader = [''] * len(triangles)
    for slot, ntri, first, _nidx in ranges:
        for k in range(first // 3, min(first // 3 + ntri, len(shader))):
            shader[k] = slots[slot] if slot < len(slots) else f'slot {slot}'
    return shader, slots


def read(folder: str, name: str) -> Water | None:
    """The water of terrain <name> under an unpacked terrain-00.fbrb (None if it has no water files)."""
    mesh: str | None
    visual: str | None
    mesh = visual = None
    name = name.lower()                          # any case of the terrain name
    for root, _, files in os.walk(folder):
        for f in files:
            low = f.lower()
            if low == f'{name}.water.mesh.res':
                mesh = os.path.join(root, f)
            elif low == f'{name}.water.res':
                visual = os.path.join(root, f)
    if mesh is None:
        return None
    mesh_data = _read_file(mesh)
    try:
        v, t = parse_mesh(mesh_data)
    except ValueError as e:
        raise ValueError(f'{mesh}: {e}') from None
    w = Water(v, t)
    if visual is not None and len(t):
        r = shaders_of(mesh_data, _read_file(visual), t)
        if r is not None:
            w.shader, w.shaders = r
    return w
