"""BC2 terrain height fields: Terrains/<Level>/<Level>.Heightfield-000.res (type TerrainHeightfield, in each
level's terrain-00.fbrb).

    hf = read(path)            # Heightfield: bounds, size, heights (size x size, metres); parse(data) for bytes
    hf.height(x, z)            # world height, grid squares split as the game splits them
    vertices, triangles = hf.mesh(stride=2)

Layout (all little-endian; checked against the in-game terrain samples of 13 campaign levels, see DESIGN.md
"Terrain height field"):

    0x00  6 x f32      world bounds: min x, min y, min z, max x, max y, max z
    0x18  u32          size: samples per side (1024 or 2048)
    0x1C  u32 x 2      log2(size) - 3, log2(size) - 2
    0x24  u16 x 2      0, size / 256
    0x28  u32, u8      0, 0 in every level seen
    0x2D  u32          the length in bytes of the tail's first section (bfbc2-console-convert terrain.py)
    0x31  u16 x size^2 heights, rows along z, columns along x: height = min y + value / 65535 * (max y - min y),
                       sample (i, j) at (min x + i * (max x - min x) / size, min z + j * (max z - min z) / size)
    then               u32 x size offsets (u16 units) into the tail's first section, then u16 values: the first
                       section, then a part of fixed length (62,474 bytes at 1024, 256,012 at 2048) - not decoded

parse / read run in the suite's Rust core (rust/, module bc2_native: bc2_formats::terrain) when it is built, else in
Python here; both give bit-identical heights and refuse the same files with ValueError
(core/tests/test_geometry_native.py: every height field of the game copy). BFBC2_PURE_PYTHON=1 (or use_native(False))
forces Python; backend() says which one runs. The Rust core also reads the tail's known layout (tail()).
"""
import os
import struct
from dataclasses import dataclass
from types import ModuleType
from typing import Any, cast

import numpy as np

from . import dbx as _dbx

HEADER = struct.Struct('<6f6IB')


@dataclass
class Heightfield:
    min: np.ndarray        # (x, y, z)
    max: np.ndarray
    size: int
    raw: np.ndarray        # (size, size) uint16 as stored
    heights: np.ndarray    # (size, size) float32 metres

    @property
    def cell(self) -> tuple[float, float]:
        return (self.max[0] - self.min[0]) / self.size, (self.max[2] - self.min[2]) / self.size

    def height(self, x: Any, z: Any) -> np.ndarray:
        """Heights at world (x, z), each grid square split along its (i, j)-(i+1, j+1) diagonal."""
        cx, cz = self.cell
        u = (np.asarray(x, float) - self.min[0]) / cx
        w = (np.asarray(z, float) - self.min[2]) / cz
        n = self.size
        i = np.clip(np.floor(u).astype(int), 0, n - 2)
        j = np.clip(np.floor(w).astype(int), 0, n - 2)
        fu, fw = u - i, w - j
        H = self.heights
        a, b, c, d = H[j, i], H[j, i + 1], H[j + 1, i], H[j + 1, i + 1]
        return np.where(fu > fw, a + (b - a) * fu + (d - b) * fw, a + (c - a) * fw + (d - c) * fu)

    def mesh(self, stride: int = 2) -> tuple[np.ndarray, np.ndarray]:
        """(vertices (n, 3) float32, triangles (m, 3) uint32) on every <stride>-th sample."""
        cx, cz = self.cell
        idx = np.arange(0, self.size, stride)
        h = self.heights[np.ix_(idx, idx)]
        X, Z = np.meshgrid(self.min[0] + idx * cx, self.min[2] + idx * cz)
        verts = np.stack([X.ravel(), h.ravel(), Z.ravel()], axis=1).astype(np.float32)
        n = len(idx)
        k = np.arange(n * n).reshape(n, n)
        a, b, c, d = k[:-1, :-1].ravel(), k[:-1, 1:].ravel(), k[1:, :-1].ravel(), k[1:, 1:].ravel()
        tris = np.concatenate([np.stack([a, c, d], 1), np.stack([a, d, b], 1)]).astype(np.uint32)
        return verts, tris


# ------------------------------------------------------------------------------------------------ backend

def _with_terrain(m: ModuleType | None) -> ModuleType | None:
    """m when it has the terrain reader (a module built before it was added does not)."""
    return m if m is not None and hasattr(m, 'terrain_heights') else None


def _load_native() -> ModuleType | None:
    """The Rust core (found as dbx finds it) unless BFBC2_PURE_PYTHON is set."""
    if os.environ.get(_dbx.PURE_PYTHON_ENV, '') not in ('', '0'):
        return None
    return _with_terrain(_dbx._import_native())


_native = _load_native()


def backend() -> str:
    """'rust' when height fields are read in the Rust core, else 'python'."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """The Rust core on (when it is built, whatever BFBC2_PURE_PYTHON says) or off for this process; returns
    backend()."""
    global _native
    _native = _with_terrain(_dbx._import_native()) if on else None
    return backend()


# ------------------------------------------------------------------------------------------------ the API

def parse(data: bytes) -> Heightfield:
    """A TerrainHeightfield's bytes as a Heightfield. ValueError when it is too short for its header or its size is
    0, not a power of two or larger than the file holds."""
    data = data if type(data) is bytes else bytes(data)
    if _native is not None:
        lo, hi, size, heights = _native.terrain_heights(data)
        raw = np.frombuffer(data, '<u2', size * size, HEADER.size).reshape(size, size)
        return Heightfield(np.array(lo, dtype=np.float64), np.array(hi, dtype=np.float64), size, raw,
                           np.frombuffer(heights, np.float32).reshape(size, size))
    return _py_parse(data)


def _py_parse(data: bytes) -> Heightfield:
    if len(data) < HEADER.size:
        raise ValueError(f'not a TerrainHeightfield (the file has {len(data)} bytes, the header {HEADER.size})')
    f = HEADER.unpack_from(data, 0)
    lo, hi, size = np.array(f[0:3]), np.array(f[3:6]), f[6]
    if size <= 0 or size & (size - 1) or HEADER.size + 2 * size * size > len(data):
        raise ValueError(f'not a TerrainHeightfield (size {size})')
    raw = np.frombuffer(data, '<u2', size * size, HEADER.size).reshape(size, size)
    heights = (lo[1] + raw * ((hi[1] - lo[1]) / 65535.0)).astype(np.float32)
    return Heightfield(lo, hi, size, raw, heights)


def read(path: str) -> Heightfield:
    with open(path, 'rb') as fh:
        data = fh.read()
    try:
        return parse(data)
    except ValueError as e:
        raise ValueError(f'{path}: {e}') from None


def tail(data: bytes) -> tuple[tuple[Any, ...], tuple[int, ...]]:
    """The bytes after the heights as far as their layout is known, with the Rust core only (bc2_formats::terrain::
    Tail): (('sections', offsets u32 bytes, first section u16 bytes, fixed part u16 bytes) | ('quadtree', u16 pairs)
    | ('unknown', bytes), (log2 - 3, log2 - 2 or 0, 0, size / 256, 0, 0, first section length))."""
    m = _with_terrain(_dbx._import_native())
    if m is None:
        raise RuntimeError('terrain.tail needs the Rust core (rust/build_native.py)')
    return cast('tuple[tuple[Any, ...], tuple[int, ...]]', tuple(m.terrain_tail(bytes(data))))


def find(folder: str, lv: str) -> str | None:
    """The level's .Heightfield-000.res under an unpacked terrain-00.fbrb, or None."""
    want = f'{lv}.heightfield-000.res'.lower()       # any case of the level name
    for root, _, files in os.walk(folder):
        for name in files:
            if name.lower() == want:
                return os.path.join(root, name)
    return None
