"""L1 check: where soldier spawners stand relative to their capture point, and how far from their own alternates.

If a spawner's own transform sits on its capture point while its alternates are tens of metres away, the spawner is an
anchor rather than a place players appear (reported as a reading, not proven)."""
import math

import numpy as np

from l1_level import load
from l1_levels import levels

ds, nalt, alt_d, zero_alt, owndist = [], [], [], 0, []
for lv, era, mode, _ in levels():
    if mode == 'AS':
        continue
    L = load(lv)
    for c in L.of('CapturePointEntityData'):
        for s in c.refs('Spawners'):
            i = L.insts.get(s)
            if i is None or i.type != 'SoldierSpawnEntityData' or not i.pos or not c.pos:
                continue
            d = math.hypot(i.pos[0] - c.pos[0], i.pos[2] - c.pos[2])
            alts = [L.insts[a].pos for a in i.refs('AlternativeSpawnPoints') if a in L.insts and L.insts[a].pos]
            ds.append(d)
            nalt.append(len(alts))
            if not alts:
                zero_alt += 1
            else:
                nearest_alt = min(math.hypot(a[0] - i.pos[0], a[2] - i.pos[2]) for a in alts)
                alt_d.append(nearest_alt)
                owndist.append(min(math.hypot(a[0] - c.pos[0], a[2] - c.pos[2]) for a in alts))
ds = np.array(ds)
print('spawners on capture points', len(ds), 'anchor distance to CP: p10/50/90', np.percentile(ds, [10, 50, 90]).round(1),
      'share within 2 m', round(float((ds < 2).mean()) * 100, 1), '%')
print('alternates per spawner p10/50/90', np.percentile(nalt, [10, 50, 90]), 'spawners without alternates', zero_alt)
print('spawner to its nearest alternate p10/50/90', np.percentile(alt_d, [10, 50, 90]).round(1),
      'share < 0.5 m (spawner duplicates an alternate)', round(float((np.array(alt_d) < 0.5).mean()) * 100, 1), '%')
print('CP to nearest alternate p10/50/90', np.percentile(owndist, [10, 50, 90]).round(1))
