"""L1: plane geometry on the XZ ground plane (numpy only): point in polygon, union area on a raster, distance to a
union's boundary, and the terrain height field from the game copy's bundles (read in memory, nothing unpacked)."""
import math
import os

import numpy as np
from l1_read import GAME, fbrb

# ------------------------------------------------------------------------------------------------ polygons


def xz(points) -> np.ndarray:
    a = np.asarray(points, dtype=np.float64)
    return a[:, [0, 2]] if a.shape[1] == 3 else a


def inside(pts: np.ndarray, poly: np.ndarray) -> np.ndarray:
    """Even-odd point in polygon for many points (pts (n, 2), poly (m, 2))."""
    x, z = pts[:, 0], pts[:, 1]
    res = np.zeros(len(pts), dtype=bool)
    m = len(poly)
    for k in range(m):
        x1, z1 = poly[k]
        x2, z2 = poly[(k + 1) % m]
        cond = (z1 > z) != (z2 > z)
        with np.errstate(divide='ignore', invalid='ignore'):
            xc = x1 + (z - z1) * (x2 - x1) / (z2 - z1)
        res ^= cond & (x < xc)
    return res


def inside_union(pts: np.ndarray, polys: list[np.ndarray]) -> np.ndarray:
    r = np.zeros(len(pts), dtype=bool)
    for p in polys:
        r |= inside(pts, p)
    return r


def shoelace(poly: np.ndarray) -> float:
    x, z = poly[:, 0], poly[:, 1]
    return float(abs(np.dot(x, np.roll(z, -1)) - np.dot(z, np.roll(x, -1))) / 2)


def boundary_samples(polys: list[np.ndarray], step: float = 2.0) -> np.ndarray:
    """Points every `step` m along each outline, without those strictly inside another outline of the union."""
    out = []
    for i, p in enumerate(polys):
        m = len(p)
        for k in range(m):
            a, b = p[k], p[(k + 1) % m]
            n = max(1, int(math.ceil(np.linalg.norm(b - a) / step)))
            t = np.arange(n)[:, None] / n
            s = a + (b - a) * t
            others = [q for j, q in enumerate(polys) if j != i]
            if others:
                s = s[~inside_union(s, others)]
            out.append(s)
    return np.concatenate(out) if out else np.zeros((0, 2))


def signed_margin(pt, polys: list[np.ndarray], bnd: np.ndarray) -> float | None:
    """Distance from a point to the union's boundary: positive inside, negative outside."""
    if len(bnd) == 0:
        return None
    p = np.asarray(pt, dtype=np.float64)[[0, 2]] if len(pt) == 3 else np.asarray(pt, dtype=np.float64)
    d = float(np.min(np.hypot(bnd[:, 0] - p[0], bnd[:, 1] - p[1])))
    ins = bool(inside_union(p[None, :], polys)[0])
    return d if ins else -d


class Grid:
    """A raster over a box, for union / intersection areas of outlines."""

    def __init__(self, box, max_cells: float = 3e6):
        x0, z0, x1, z1 = box
        w, h = max(x1 - x0, 1.0), max(z1 - z0, 1.0)
        self.cell = max(2.0, math.sqrt(w * h / max_cells))
        self.nx = int(math.ceil(w / self.cell)) + 1
        self.nz = int(math.ceil(h / self.cell)) + 1
        xs = x0 + (np.arange(self.nx) + 0.5) * self.cell
        zs = z0 + (np.arange(self.nz) + 0.5) * self.cell
        X, Z = np.meshgrid(xs, zs)
        self.pts = np.stack([X.ravel(), Z.ravel()], axis=1)

    def mask(self, polys: list[np.ndarray]) -> np.ndarray:
        m = np.zeros(len(self.pts), dtype=bool)
        for p in polys:
            x0, z0 = p.min(axis=0)
            x1, z1 = p.max(axis=0)
            sel = (self.pts[:, 0] >= x0) & (self.pts[:, 0] <= x1) & (self.pts[:, 1] >= z0) & (self.pts[:, 1] <= z1)
            idx = np.nonzero(sel)[0]
            if len(idx):
                m[idx] |= inside(self.pts[idx], p)
        return m

    def area(self, mask: np.ndarray) -> float:
        return float(mask.sum()) * self.cell * self.cell


def box_of(polys: list[np.ndarray]):
    a = np.concatenate(polys)
    return (float(a[:, 0].min()), float(a[:, 1].min()), float(a[:, 0].max()), float(a[:, 1].max()))


# ------------------------------------------------------------------------------------------------ terrain

_hf_cache: dict[str, object] = {}
_bundle_names: dict[str, dict] = {}


def _terrain_bundles(prefer: list[str]) -> list[str]:
    tops = [os.path.join(GAME, 'Package', 'levels'), os.path.join(GAME, 'Dist', 'win32', 'levels')]
    first, rest = [], []
    for top in tops:
        for d in sorted(os.listdir(top)):
            p = os.path.join(top, d, 'terrain-00.fbrb')
            if os.path.isfile(p):
                (first if d.lower() in prefer else rest).append(p)
    first.sort(key=lambda p: prefer.index(os.path.basename(os.path.dirname(p)).lower()))
    return first + rest


def heightfield(name: str, lv: str, common: str | None):
    """bfbc2core.terrain.Heightfield of terrain `name` (terrains/<name>/<name>.heightfield-000.res), searched in the
    level's terrain bundle, the base map's, the era's common one, then every other; None when none has it."""
    key = name.lower()
    if key in _hf_cache:
        return _hf_cache[key]
    from bfbc2core import terrain
    want = f'terrains/{key}/{key}.heightfield-000.res'
    prefer = [lv.lower(), key] + ([common] if common else [])
    found = None
    for b in _terrain_bundles(prefer):
        if b not in _bundle_names:
            try:
                _bundle_names[b] = fbrb.names(b)
            except Exception:  # noqa: BLE001
                _bundle_names[b] = {}
        ent = _bundle_names[b].get(want)
        if ent is not None:
            data = fbrb.read_uncached(b, [ent.name])[ent.name]
            found = (terrain.parse(data), b)
            break
    _hf_cache[key] = found
    return found
