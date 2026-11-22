"""L1 check: AABB capture areas - the box's transform against its flag, its Extents, and whether the flag sits at the
box's centre (Extents = half sizes about the transform) or near a corner (Extents = sizes from a corner)."""
import math

from l1_level import load, polygons
from l1_levels import levels

for lv, _era, mode, _ in levels():
    if mode not in ('CQ', 'ONS'):
        continue
    L = load(lv)
    for c in L.of('CapturePointEntityData'):
        for g in polygons(L, c, 'Geometry'):
            if g['kind'] != 'aabb':
                continue
            m = g['matrix']
            e = g['extents']
            p = c.pos
            dx, dy, dz = p[0] - m[12], p[1] - m[13], p[2] - m[14]
            # the flag in the box's frame (rows right, up, forward)
            lx = dx * m[0] + dy * m[1] + dz * m[2]
            ly = dx * m[4] + dy * m[5] + dz * m[6]
            lz = dx * m[8] + dy * m[9] + dz * m[10]
            yaw = math.degrees(math.atan2(m[2], m[0]))
            print(f'{lv:16} {c.text("LocationName")[:18]:18} ext=({e[0]:5.1f},{e[1]:5.1f},{e[2]:5.1f}) '
                  f'flag in box frame=({lx:6.1f},{ly:6.1f},{lz:6.1f}) yaw={yaw:6.1f} capR={c.num("CaptureRadius")}')
