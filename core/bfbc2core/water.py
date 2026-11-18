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

parse_mesh / parse_visual / shaders_of / read run in the suite's Rust core (rust/, module bc2_native:
bc2_formats::water). The Python reader was retired on 2026-11-18 (round F1a, rust/BENCHMARKS.md "Retired Python");
its answers on every water file of the game copy are kept as recorded digests (core/tests/geometry_golden.json,
compared by test_geometry_native.py). Without the module these functions raise bfbc2core.native.NativeMissing saying
how to get it.
"""
import os
from dataclasses import dataclass
from types import ModuleType

import numpy as np


@dataclass
class Water:
    vertices: np.ndarray                  # (n, 3) float32
    triangles: np.ndarray                 # (m, 3) uint32
    shader: list[str] | None = None       # per triangle, from the VisualWater ranges; None if they do not match
    shaders: list[str] | None = None      # the VisualWater slots

    def __len__(self) -> int:
        return len(self.triangles)


_native: ModuleType | None = None


def _rust() -> ModuleType:
    """The Rust core, or NativeMissing saying how to get it."""
    global _native
    if _native is None:
        from . import native
        _native = native.require('the water reader (bfbc2core.water)', 'water_shaders')
    return _native


def _bytes(data: bytes) -> bytes:
    return data if type(data) is bytes else bytes(data)


def parse_mesh(data: bytes) -> tuple[np.ndarray, np.ndarray]:
    """(vertices, triangles) of every body of a WaterMesh's bytes, bodies joined. ValueError when the file ends
    early, a body's indices are not whole triangles or bytes are left over."""
    v, t = _rust().water_mesh(_bytes(data))
    return np.frombuffer(v, '<f4').reshape(-1, 3), np.frombuffer(t, '<u4').reshape(-1, 3)


def parse_visual(data: bytes) -> tuple[np.ndarray, list[str], list[tuple[int, ...]]]:
    """(indices, slot shader paths, ranges [(slot, triangles, first index, index count)]) of a VisualWater's bytes.
    ValueError when the file ends early or a name has no end."""
    idx, slots, ranges = _rust().water_visual(_bytes(data))
    return np.frombuffer(idx, '<u4'), slots, ranges


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
    indices are not the mesh's (mesh: the WaterMesh's bytes, whose triangles are given; triangles is not read: the
    mesh's own are)."""
    r = _rust().water_shaders(_bytes(mesh), _bytes(visual))
    return None if r is None else (r[0], r[1])


def find_files(folder: str, name: str) -> tuple[str | None, str | None]:
    """The .Water.Mesh.res and .Water.res of terrain <name> under an unpacked terrain-00.fbrb (the last of each in
    walk order), None for one that is not there."""
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
    return mesh, visual


def read(folder: str, name: str) -> Water | None:
    """The water of terrain <name> under an unpacked terrain-00.fbrb (None if it has no water files)."""
    mesh, visual = find_files(folder, name)
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
