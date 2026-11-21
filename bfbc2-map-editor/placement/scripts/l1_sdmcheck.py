"""L1 check: a Squad Deathmatch level's spawn points outside its combat area (which spawner, which squad)."""
import sys

import numpy as np

from l1_geom import boundary_samples, signed_margin
from l1_level import load
from l1_measure import ca_union, cp_spawners, spawn_points

lv = sys.argv[1]
L = load(lv)
polys = [p for c in L.of('CombatAreaTriggerEntityData') for p in ca_union(L, c)]
bnd = boundary_samples(polys)
for c in L.of('CapturePointEntityData'):
    sol, _ = cp_spawners(L, c)
    for s in sol:
        for g, p in spawn_points(L, s):
            m = signed_margin(p, polys, bnd)
            if m is not None and m < 0:
                print(c.name, L.insts[s].name, g, L.insts[g].type, np.round(p, 1), round(m, 1))
