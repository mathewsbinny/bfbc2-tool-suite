"""NavKit: the walkability edits of a navigation side file (`<level>.navtune.json`, DESIGN.md section 5.1) as
inputs to the generator - which sampled surfaces are forced walkable or unwalkable, extra seeds, and the
points whose islands are dropped.

  walkable / unwalkable  every sampled surface inside the region (its outline or box in x/z, and the height
                         range) is forced walkable (slope and clearance rules skipped) or unwalkable; later
                         operations win where regions overlap
  seed                   a seed at the point: its island is kept (navwalk's seeding, as soldier spawns are)
  unseed                 the island the point is on is dropped after seeding (even if it has other seeds)

Links, route edges and traversal annotations are not walkability: navkit apply handles them after the build.

The masks are the Rust core's (bc2_navbuild::walk::edit_masks, round NK4: matplotlib's Path.contains_points ported
as its crossing test, points on the outline as matplotlib answers them); their NumPy / matplotlib Python was retired
after the parity run (rust/BENCHMARKS.md "NK4"). navwalk.build hands the regions (regions_of) to the core with the
sample file.
"""
from typing import Any

import numpy as np

WALKABILITY = ('walkable', 'unwalkable', 'seed', 'unseed')


def _core() -> Any:
    import navwalk
    return navwalk.require_native()


def surface_positions(s):
    """(x, z, y) of every sampled surface."""
    col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start[:s.nx * s.nz + 1]))
    x = s.min_x + (col % s.nx) * s.pitch
    z = s.min_z + (col // s.nx) * s.pitch
    return x, z, s.y.astype(np.float64)


def region_of(region):
    """A side file's region as the Rust core takes it: ('box', [x0, y0, z0, x1, y1, z1]) or ('polygon', (lo, hi),
    [(x, z), ...]); KeyError / ValueError for a region of another shape, as the retired NumPy raised."""
    if 'box' in region:
        x0, y0, z0, x1, y1, z1 = region['box']
        return ('box', [float(v) for v in (x0, y0, z0, x1, y1, z1)])
    lo, hi = region['y']
    poly = np.asarray(region['polygon'], dtype=np.float64)
    if poly.ndim != 2 or poly.shape[1] != 2:
        raise ValueError(f"a region's polygon is a list of (x, z) points, not an array of shape {poly.shape}")
    if not len(poly):
        raise ValueError('a region whose polygon has no points')
    return ('polygon', (float(lo), float(hi)), [(float(a), float(b)) for a, b in poly])


def region_mask(x, z, y, region):
    """The points (x, z, y arrays) inside the region; in the Rust core."""
    b = [np.ascontiguousarray(v, '<f8').tobytes() for v in (x, z, y)]
    return np.frombuffer(_core().navkit_region_mask(*b, region_of(region)), np.bool_).copy()


def regions_of(operations):
    """The walkability operations of a side file: ([(walkable, region)] for the region edits in order, seeds,
    unseeds, counts per operation name in the order they first occur)."""
    ops, seeds, unseeds = [], [], []
    counts: dict[str, int] = {}
    for o in operations:
        name = o.get('op')
        if name not in WALKABILITY:
            continue
        counts[name] = counts.get(name, 0) + 1
        if name in ('walkable', 'unwalkable'):
            ops.append((name == 'walkable', region_of(o['region'])))
        elif name == 'seed':
            seeds.append(tuple(o['at']))
        else:
            unseeds.append(tuple(o['at']))
    return ops, seeds, unseeds, counts


def edits_for(s, operations):
    """dict(force_walkable, force_unwalkable: bool per surface; seeds, unseeds: lists of (x, y, z); counts)."""
    import navwalk
    ops, seeds, unseeds, counts = regions_of(operations)
    fw_b, fu_b = _core().navkit_edit_masks(navwalk.run_dict(s, normals=False), ops)
    fw, fu = np.frombuffer(fw_b, np.bool_).copy(), np.frombuffer(fu_b, np.bool_).copy()
    return {'force_walkable': fw, 'force_unwalkable': fu, 'seeds': seeds, 'unseeds': unseeds, 'counts': counts,
            'surfaces_walkable': int(fw.sum()), 'surfaces_unwalkable': int(fu.sum())}
