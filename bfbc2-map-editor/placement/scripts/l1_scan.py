"""L1 exploration across every level: event names reaching gameplay types, vehicle blueprint kinds, odd fields."""
from collections import Counter

from l1_level import load
from l1_levels import levels

TYPES = {'CapturePointEntityData', 'CombatAreaTriggerEntityData', 'VehicleSpawnEntityData', 'SoldierSpawnEntityData',
         'MultipleTriggerEntityData'}
tev = Counter()
kinds = Counter()
capr = Counter()
geo = Counter()
for lv, era, mode, _ in levels():
    try:
        L = load(lv)
    except Exception as e:  # noqa: BLE001
        print('FAIL', lv, e)
        continue
    for e in L.events:
        t = L.insts[e.dst].type if e.dst in L.insts else '?'
        if t in TYPES:
            tev[(t, e.tev, e.sev)] += 1
    for v in L.of('VehicleSpawnEntityData'):
        for r in v.extrefs('Vehicles') + v.extrefs('Vehicle'):
            p = r.lower().split('/')
            kinds['/'.join(p[1:4]) if len(p) > 4 else r.lower()] += 1
    for c in L.of('CapturePointEntityData'):
        gs = c.refs('Geometry')
        geo[(era, mode, tuple(sorted(L.insts[g].type for g in gs if g in L.insts)), c.num('CaptureRadius') != 0)] += 1
    print(lv, era, mode, len(L.insts), 'events', len(L.events), 'CA', len(L.of('CombatAreaTriggerEntityData')))
print()
for k, n in sorted(tev.items()):
    print(n, k)
print()
for k, n in sorted(kinds.items()):
    print(n, k)
print()
for k, n in sorted(geo.items()):
    print(n, k)
