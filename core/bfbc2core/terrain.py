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

parse / read run in the suite's Rust core (rust/, module bc2_native: bc2_formats::terrain), which also reads the
tail's known layout (tail()). The Python reader was retired on 2026-11-18 (round F1a, rust/BENCHMARKS.md "Retired
Python"); its answers on every height field of the game copy are kept as recorded digests
(core/tests/geometry_golden.json, compared by test_geometry_native.py). Without the module these functions raise
bfbc2core.native.NativeMissing saying how to get it.
"""
import os
import struct
from dataclasses import dataclass
from types import ModuleType
from typing import Any, cast

import numpy as np

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


_native: ModuleType | None = None


def _rust() -> ModuleType:
    """The Rust core, or NativeMissing saying how to get it."""
    global _native
    if _native is None:
        from . import native
        _native = native.require('the terrain reader (bfbc2core.terrain)', 'terrain_heights')
    return _native


def parse(data: bytes) -> Heightfield:
    """A TerrainHeightfield's bytes as a Heightfield. ValueError when it is too short for its header or its size is
    0, not a power of two or larger than the file holds."""
    m = _rust()
    data = data if type(data) is bytes else bytes(data)
    lo, hi, size, heights = m.terrain_heights(data)
    raw = np.frombuffer(data, '<u2', size * size, HEADER.size).reshape(size, size)
    return Heightfield(np.array(lo, dtype=np.float64), np.array(hi, dtype=np.float64), size, raw,
                       np.frombuffer(heights, np.float32).reshape(size, size))


def read(path: str) -> Heightfield:
    with open(path, 'rb') as fh:
        data = fh.read()
    try:
        return parse(data)
    except ValueError as e:
        raise ValueError(f'{path}: {e}') from None


def tail(data: bytes) -> tuple[tuple[Any, ...], tuple[int, ...]]:
    """The bytes after the heights as far as their layout is known (bc2_formats::terrain::Tail): (('sections',
    offsets u32 bytes, first section u16 bytes, fixed part u16 bytes) | ('quadtree', u16 pairs) | ('unknown', bytes),
    (log2 - 3, log2 - 2 or 0, 0, size / 256, 0, 0, first section length))."""
    return cast('tuple[tuple[Any, ...], tuple[int, ...]]', tuple(_rust().terrain_tail(bytes(data))))


def find(folder: str, lv: str) -> str | None:
    """The level's .Heightfield-000.res under an unpacked terrain-00.fbrb, or None."""
    want = f'{lv}.heightfield-000.res'.lower()       # any case of the level name
    for root, _, files in os.walk(folder):
        for name in files:
            if name.lower() == want:
                return os.path.join(root, name)
    return None
