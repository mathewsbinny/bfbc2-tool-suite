"""L1 exploration: damage / area triggers of a level: names, fields, events in and out (output not kept)."""
import sys
from collections import Counter

from l1_level import load

TYPES = ('DamageAreaTriggerEntityData', 'AreaTriggerEntityData', 'ClearAreaTriggerEntityData')

for lv in sys.argv[1:]:
    L = load(lv)
    print('==', lv)
    for t in TYPES:
        for i in L.of(t)[:6]:
            fields = {c.attr('name'): (L.insts and None) for c in (i.el.children or [])}
            vals = []
            for c in i.el.children or []:
                n = c.attr('name')
                if c.children is None and n not in ('Components', 'Transform'):
                    vals.append(f'{n}={i.text(n)}')
                elif c.children is not None and n != 'Components':
                    vals.append(f'{n}[{len(c.children)}]')
            ins = Counter((e.sev, e.tev, L.insts[e.src].type if e.src in L.insts else '?') for e in L.inn(i.guid))
            outs = Counter((e.sev, e.tev, L.insts[e.dst].type if e.dst in L.insts else '?') for e in L.out(i.guid))
            print(t, tuple(round(x) for x in i.pos or (0, 0, 0)), ' '.join(vals))
            print('    in', dict(ins), 'out', dict(outs))
            geo = i.refs('Geometry') + i.refs('Volume')
            print('    geo', Counter(L.insts[g].type for g in geo if g in L.insts))
