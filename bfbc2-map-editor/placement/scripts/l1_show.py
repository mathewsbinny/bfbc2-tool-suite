"""L1 helper: print a measurement file's records compactly (optionally one level, metric prefix)."""
import json
import sys

doc = json.load(open(sys.argv[1], encoding='utf-8'))
lv = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] != '-' else None
pre = sys.argv[3] if len(sys.argv) > 3 else ''
for r in doc['records']:
    if lv and r['level'] != lv:
        continue
    if not r['metric'].startswith(pre) or r['metric'].startswith('all.'):
        continue
    extra = {k: v for k, v in r.items() if k not in ('level', 'era', 'mode', 'metric', 'value', 'unit', 'guids', 'note')}
    print(r['level'], r['metric'], r['value'], r['unit'], json.dumps(extra)[:220])
