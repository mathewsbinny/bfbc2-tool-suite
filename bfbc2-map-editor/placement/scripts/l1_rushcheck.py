"""L1 check: per Rush / Squad Rush level, stage count, M-COMs per stage, order certainty, bases and areas per stage."""
import json
import sys
from collections import defaultdict

doc = json.load(open(sys.argv[1], encoding='utf-8'))
by = defaultdict(list)
for r in doc['records']:
    if r['mode'] in ('Rush', 'SR'):
        by[r['level']].append(r)
for lv, rs in by.items():
    get = lambda m: [r for r in rs if r['metric'].endswith(m)]  # noqa: E731
    sc = get('.stage_count')[0]
    mps = get('.mcoms_per_stage')[0]['value']
    ab = [(r['stage'], r.get('names')) for r in get('.attacker_bases')]
    db = [(r['stage'], r.get('names')) for r in get('.defender_bases')]
    aa = [(r['stage'], round(r['value'] / 1000)) for r in get('.attacker_area')]
    da = [(r['stage'], round(r['value'] / 1000)) for r in get('.defender_area')]
    print(lv, int(sc['value']), mps, 'uncertain' if sc.get('order_uncertain') else '')
    print('   att', ab)
    print('   def', db)
    print('   areas k m2 att', aa, 'def', da)
