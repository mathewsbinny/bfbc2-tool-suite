r"""dbxparity.py - BF1943's data partitions against DICE's own versions, field by field: the ground truth for dbxport.
BFBC2 ships many partitions Battlefield 1943 also has, under the same names and with the same instance GUIDs (the
campaign prologue's Zero and Corsair, the Kurogane, the Chi-Ha, the Bofors, weapons, props, trees, effects). For each,
every instance present in both (same GUID and type) is compared field by field, nested fields and array items
included, and every difference is classified over all partitions:

  enum            BF1943 stores a number where DICE stores an enum name (RotationInput 4 / EiaYaw): the mapping
  kind            the field's stored kind differs (text / numbers / byte) with no number-name pairing
  nested          a BF1943-only field DICE keeps one level deeper under the same name (FireLogic.MaxRecoilAngleX ->
                  FireLogic.Recoil.MaxRecoilAngleX)
  renamed         a BF1943-only field whose value DICE holds in a DICE-only field of the same instance
                  (AngularMomentumDamping -> AngularMomentumDampingAcc + ...Deacc)
  bf1943-only     fields BFBC2 does not have
  dice-only       fields BF1943 does not have (dbxport fills them from a template)
  value           the same field, another value (DICE's edits, or a format difference)

    python dbxparity.py [name part ...] [-o report.txt]      (default: every partition both games have)

The report lists, per (type, field), how often each class occurred and examples; a JSON beside it has the enum
pairings. dbxport's value translations are checked against it (dbxport.hint_value and the enum maps).
"""
import argparse
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
for p in ('core', 'bfbc2-shader-tools'):
    sys.path.insert(0, os.path.join(SUITE, p))
sys.path.insert(0, HERE)
from bfbc2core import dbx  # noqa: E402

NAME = re.compile(r'[A-Za-z_][A-Za-z0-9_]*$')
INT = re.compile(r'-?\d+$')


def flatten(e, path, out):
    """{field path: (kind, value)} of an instance's fields; arrays / complexes nested (items by index)."""
    kids = e.children or []
    if e.kind == dbx.CONTAINER or e.tag in ('array', 'complex', 'instance') and kids:
        for i, c in enumerate(kids):
            n = c.attr('name')
            sub = f'{path}.{n}' if n is not None else f'{path}[{i}]'
            flatten(c, sub, out)
        if e.tag == 'array':
            out[path + '#len'] = ('len', str(len(kids)))
        return
    ref = e.attr('ref')
    if ref is not None:
        out[path] = ('ref', ref.split('/')[-1].lower() if ref != 'null' else 'null')
        return
    try:
        out[path] = (e.kind, dbx.value(e).strip())
    except Exception:                                               # noqa: BLE001 - an unreadable value
        out[path] = (e.kind, '?')


def instances(data):
    f = dbx.parse(data)
    out = {}
    for inst in f.instances:
        fields = {}
        flatten(inst, '', fields)
        out[(inst.attr('guid') or '').upper()] = (inst.attr('type'), fields)
    return out


def compare(src, ix, names, log=print):
    stats = collections.defaultdict(collections.Counter)            # (type, field) -> class counts
    examples = collections.defaultdict(list)
    enums = collections.defaultdict(collections.Counter)            # (type, field) -> (number, name) counts
    partitions = matched = 0
    for k in names:
        try:
            a, b = instances(src.data(k)), instances(ix.read(k))
        except Exception as e:                                      # noqa: BLE001
            log(f'  {k}: not compared ({e})')
            continue
        partitions += 1
        for g, (t, fa) in a.items():
            if g not in b or b[g][0] != t:
                continue
            matched += 1
            fb = b[g][1]
            tshort = t.split('.')[-1]
            only_a = {p: v for p, v in fa.items() if p not in fb}
            only_b = {p: v for p, v in fb.items() if p not in fa}
            for p, (ka, va) in fa.items():
                if p not in fb:
                    continue
                kb, vb = fb[p]
                key = (tshort, re.sub(r'\[\d+\]', '[]', p))
                if (ka, va) == (kb, vb):
                    stats[key]['same'] += 1
                elif INT.match(va) and NAME.match(vb) and vb not in ('true', 'false', 'null'):
                    stats[key]['enum'] += 1
                    enums[key][(va, vb)] += 1
                elif ka != kb:
                    stats[key]['kind'] += 1
                    if len(examples[key]) < 3:
                        examples[key].append(f'{k}: {ka}:{va[:30]} / DICE {kb}:{vb[:30]}')
                else:
                    stats[key]['value'] += 1
                    if len(examples[key]) < 3:
                        examples[key].append(f'{k}: {va[:30]} / DICE {vb[:30]}')
            dice_values = collections.defaultdict(list)
            for p, (_kb, vb) in only_b.items():
                dice_values[vb].append(p)
            deeper = collections.defaultdict(list)                  # field name -> DICE-only paths ending in it
            for p in only_b:
                deeper[p.rsplit('.', 1)[-1]].append(p)
            for p, (_ka, va) in only_a.items():
                key = (tshort, re.sub(r'\[\d+\]', '[]', p))
                head, _, leaf = p.rpartition('.')
                nest = [q for q in deeper.get(leaf, []) if q.startswith(head + '.') and q.count('.') > p.count('.')]
                hits = dice_values.get(va, [])
                if nest:
                    stats[key]['nested'] += 1
                    ex = f'-> {nest[0]}'
                    if ex not in examples[key]:
                        examples[key].append(ex)
                elif hits and va not in ('0', 'false', 'null', '', '1', 'true'):
                    stats[key]['renamed'] += 1
                    ex = f"-> {', '.join(re.sub(r'.*\.', '', h) for h in hits[:3])}"
                    if ex not in examples[key]:
                        examples[key].append(ex)
                else:
                    stats[key]['bf1943-only'] += 1
            for p in only_b:
                stats[(tshort, re.sub(r'\[\d+\]', '[]', p))]['dice-only'] += 1
    return partitions, matched, stats, examples, enums


def main(argv=None):
    import bf1943import
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-mesh-viewer'))
    import meshview
    import shaderdb
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('names', nargs='*')
    ap.add_argument('-o', default='dbxparity.txt')
    a = ap.parse_args(argv)
    src = bf1943import.Source(bf1943import.default_source())
    ix = meshview.Index(shaderdb.find_opfor(None))
    both = sorted(k for k in src.entries if k.endswith('.dbx') and k in ix.where and
                  (not a.names or any(x.lower() in k for x in a.names)))
    partitions, matched, stats, examples, enums = compare(src, ix, both)
    classes = collections.Counter()
    for c in stats.values():
        classes.update(c)
    with open(a.o, 'w', encoding='utf-8') as f:
        f.write(f'{partitions} partitions in both games, {matched} instances matched by GUID and type; field '
                f'comparisons: {dict(classes)}\n\n')
        for cls in ('enum', 'kind', 'nested', 'renamed', 'value', 'bf1943-only', 'dice-only'):
            rows = sorted(((k, c) for k, c in stats.items() if c[cls]), key=lambda kc: -kc[1][cls])
            f.write(f'== {cls}: {len(rows)} fields\n')
            for (t, p), c in rows:
                extra = ''
                if cls == 'enum':
                    extra = ' ' + ', '.join(f'{n}={v}' for (n, v), _ in sorted(enums[(t, p)].items()))
                elif examples[(t, p)] and cls in ('kind', 'nested', 'renamed', 'value'):
                    extra = ' | ' + ' ; '.join(examples[(t, p)][:2])
                f.write(f'  {c[cls]:5d} {t}{p}{extra}\n')
            f.write('\n')
    json.dump({f'{t}{p}': {f'{n}': v for (n, v) in sorted(c)} for (t, p), c in enums.items()},
              open(os.path.splitext(a.o)[0] + '_enums.json', 'w'), indent=1)
    print(f'{partitions} partitions, {matched} instances; {dict(classes)} -> {a.o}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
