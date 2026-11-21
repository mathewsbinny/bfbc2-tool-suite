"""L1 exploration: combat-area outlines (Geometry vs SurroundingGeometry) of some levels (output not kept)."""
import sys

from l1_level import load, polygons


def shoelace(pts):
    a = 0.0
    for k in range(len(pts)):
        x1, _, z1 = pts[k]
        x2, _, z2 = pts[(k + 1) % len(pts)]
        a += x1 * z2 - x2 * z1
    return abs(a) / 2


for lv in sys.argv[1:]:
    L = load(lv)
    for c in L.of('CombatAreaTriggerEntityData'):
        for fld in ('Geometry', 'SurroundingGeometry'):
            for p in polygons(L, c, fld):
                if p['kind'] == 'polygon':
                    xs = [q[0] for q in p['points']]
                    zs = [q[2] for q in p['points']]
                    print(lv, c.name, c.text('Team'), fld, len(p['points']), 'pts area', round(shoelace(p['points'])),
                          'box', round(min(xs)), round(min(zs)), round(max(xs)), round(max(zs)), 'h', p['height'])
                else:
                    print(lv, c.name, fld, p['kind'])
