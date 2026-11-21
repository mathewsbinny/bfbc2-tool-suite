"""L1 exploration: a Rush level's M-COMs, stage counters and what the counters' events reach (output not kept)."""
import sys
from collections import Counter

from l1_level import load

GLUE = {'MultipleTriggerEntityData', 'DelayTriggerEntityData', 'EventSplitterEntityData', 'EventGateEntityData',
        'SequenceData', 'TeamFilterEntityData'}


def blueprint(i):
    c = i.f('ReferencedObject')
    return (c.attr('ref') or '').lower() if c is not None else ''


def main(lv, depth=4):
    L = load(lv)
    mcoms = [g for g in L.order if L.insts[g].type == 'ReferenceObjectData'
             and any(L.insts.get(e.dst) and L.insts[e.dst].type == 'GameInteractionEntityData' for e in L.out(g))]
    print(lv, 'M-COMs', len(mcoms))
    stages = {}
    for m in mcoms:
        key = tuple(sorted(e.dst for e in L.out(m) if L.insts.get(e.dst) and
                           L.insts[e.dst].type == 'MultipleTriggerEntityData'))
        stages.setdefault(key, []).append(m)
    for k, ms in stages.items():
        print('STAGE key', k, [(blueprint(L.insts[m]).split('/')[-2], tuple(round(x) for x in L.insts[m].pos)) for m in ms])
        for m in ms:
            c = Counter((e.sev, e.tev, L.insts[e.dst].type if e.dst in L.insts else '?') for e in L.out(m))
            print('   mcom out', dict(c))
        seen = set()
        frontier = list(k)
        for d in range(depth):
            nxt = []
            for g in frontier:
                for e in L.out(g):
                    if (g, e.dst, e.tev) in seen:
                        continue
                    seen.add((g, e.dst, e.tev))
                    t = L.insts[e.dst].type if e.dst in L.insts else '?'
                    nm = L.insts[e.dst].name if e.dst in L.insts else ''
                    print('  ' * (d + 2) + f'{e.sev} -> {e.tev} {t} "{nm}"')
                    if t in GLUE:
                        nxt.append(e.dst)
            frontier = nxt


if __name__ == '__main__':
    main(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 4)
