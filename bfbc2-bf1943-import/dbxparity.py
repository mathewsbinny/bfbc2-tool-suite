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
    python dbxparity.py --predict [name part ...] [-o report.txt]

--predict scores dbxport's value passes and DICE's conventions (dbxport.DICE_CONVENTIONS, the bone remap, the EIA
spellings): each weapon and vehicle partition both games have is ported from BF1943's copy on a fair pool (FAIR_POOLS:
the callers' kind of donors, never DICE's same-named partition; the catalogue's class defaults, never the partition's
own) with the passes and without them, and each field a pass targets (PREDICTED) is compared with DICE's value:
how many DICE values the passes reproduce, against BF1943's value as it was. Rule-derived fields (gravity, speed,
auto-aim, sprint look, bones, enum spellings) are scored on their own; template-derived ones (recoil, magazines,
pickups, AlwaysAimHead, the donor's AnimBaseSet) depend on the donor - also scored with DICE's version in the pool
(the counterpart, which the ranked template choice takes whenever BFBC2 ships the instance).

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
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'shader-tools', 'core')
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
        fields: dict[str, tuple[int | str, str]] = {}
        flatten(inst, '', fields)
        out[(inst.attr('guid') or '').upper()] = (inst.attr('type'), fields)
    return out


def compare(src, ix, names, log=print):
    stats: collections.defaultdict[tuple[str, str], collections.Counter[str]] = \
        collections.defaultdict(collections.Counter)                # (type, field) -> class counts
    examples: collections.defaultdict[tuple[str, str], list[str]] = collections.defaultdict(list)
    enums: collections.defaultdict[tuple[str, str], collections.Counter[tuple[str, str]]] = \
        collections.defaultdict(collections.Counter)                # (type, field) -> (number, name) counts
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


# the fair pools: BFBC2 donors of the kind the callers use (OPFOR's ww2 builds), per partition family
_V, _VIET = 'objects/vehicles/beach', 'objects/vehicles/vietnam'
_COMMON = [f'{_V}/kurogane/kurogane_wreck', f'{_V}/kurogane/kurogane_mesh_mesh', f'{_V}/type97-chiha/chiha_mb_mesh',
           'systems/mesh/defaultmeshlodsettings', 'objects/weapons/handheld/beach/thompson/thompson_lodsettings',
           'objects/vehicles/sea/lcrs/lcrs', 'objects/vehicles/air/ah60/ah60']
_JEEP = [f'{_VIET}/m151a1/m151a1', f'{_V}/kurogane/kurogane', 'objects/vehicles/land/humvee/humvee']
_PLANE = [f'{_V}/us_corsair/corsair', f'{_V}/jap_zero_fighter/zero_fighter']
_M24 = 'objects/weapons/handheld/us_sni_m24'
FAIR_POOLS = {
    'objects': ['objects/objects/boat_house/boat_house', 'objects/objects/boat_house/boat_house_mesh',
                'objects/objects/boat_house/boat_house_havok',
                'objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01',
                'objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01_mesh',
                'objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01_havok',
                'systems/mesh/defaultmeshlodsettings', 'objects/vehicles/beach/kurogane/kurogane_wreck',
                'objects/buildings/barrack/barrack', 'objects/buildings/barrack/barrack_mesh',
                'objects/buildings/barrack/barrack_havok', 'objects/buildings/house_06_8x8/house_06_8x8'],
    'zero': [f'{_V}/jap_zero_fighter/zero_fighter', f'{_V}/jap_zero_fighter/zero_firing',
             f'{_V}/jap_zero_fighter/zero_bombs_firing'] + _PLANE + _COMMON,
    'corsair': [f'{_V}/us_corsair/corsair', f'{_V}/us_corsair/corsair_firing',
                f'{_V}/us_corsair/corsair_bombs_firing'] + _PLANE + _COMMON,
    'kurogane': _JEEP + _COMMON,
    'chiha': [f'{_V}/type97-chiha/type97-chiha', f'{_VIET}/m48pattontank/m48'] + _JEEP + _COMMON,
    'bofors': [f'{_V}/stationary_aa_bofors/aa_bofors', f'{_V}/stationary_aa_bofors/aa_bofors_firing'] + _COMMON,
    'vehicles': [f'{_V}/stationary_hmg_30cal/30cal_window'] + _COMMON,
    'weapons': [f'{_M24}/us_sni_m24', f'{_M24}/us_sni_m24_firing', f'{_M24}/us_sni_m24_mesh',
                'objects/weapons/handheld/beach/thompson/thompson_lodsettings', 'systems/mesh/defaultmeshlodsettings'],
}


def family(k):
    """A shared partition's family: weapons, vehicles (by vehicle), objects, effects, sounds, other."""
    if k.startswith('objects/weapons/'):
        return 'weapons'
    if k.startswith('objects/vehicles/'):
        return next((v for v in ('zero', 'corsair', 'kurogane', 'chiha', 'bofors') if v in k), 'vehicles')
    if k.startswith('objects/'):
        return 'objects'
    if k.startswith(('effects/', 'fx/', 'emitters/')):
        return 'effects'
    return 'sounds' if k.startswith('sound/') else 'other'


def fair_pool(k):
    """The fair pool of a shared partition (its own name left out): FAIR_POOLS, none for the families without one
    (the catalogue's class defaults)."""
    name = k[:-4] if k.endswith('.dbx') else k
    return [p for p in FAIR_POOLS.get(family(k), []) if p != name]


def fair_catalogue(cat, exclude):
    """The catalogue with class defaults that are not the partition's own (its second instance instead)."""
    import copy
    c = copy.copy(cat)
    c.defaults = {t: (cat.others[t] if v[0] == exclude and t in cat.others else v)
                  for t, v in cat.defaults.items() if v[0] != exclude or t in cat.others}
    return c


# the fields DICE's conventions and dbxport's value passes target: (type, field path) patterns; rule-derived (the
# value follows from BF1943's and the rule) or template-derived (the donor's)
PREDICTED = [
    ('rule', 'BulletEntityData', r'\.Gravity$'), ('rule', 'GrenadeEntityData', r'\.Gravity$'),
    ('rule', 'FiringFunctionData', r'\.Shot\.InitialSpeed$'), ('rule', 'WeaponFiringData', r'\.UseAutoAiming$'),
    ('rule', 'ZoomLevelData', r'\.SprintLookSpeedMultiplier$'), ('rule', 'BoneFakePhysicsData', r'\.BoneId$'),
    ('rule', '', r'(InputAction|ActionToSuppress|HintAction\d)$'),
    ('template', 'SoldierAimingSimulationData', r'\.ZoomType$'),
    ('template', 'SoldierWeaponData', r'\.AnimBaseSet$'),
    ('template', 'SoldierWeaponData', r'\.WeaponStates\[\d+\]\.AlwaysAimHead$'),
    ('template', 'FiringFunctionData', r'\.Ammo\.(NumberOfMagazines|AmmoPickupMinAmount|AmmoPickupMaxAmount)$'),
    ('template', 'FiringFunctionData', r'\.FireLogic\.Recoil\.\w+$'),
]


def predict(src, ix, names, cat, bones, log=print):
    """{(kind, type, field): Counter(dice, predicted, baseline, counterpart)}: how many of DICE's values each pass
    reproduces (see the module docstring)."""
    import dbxport
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')}
    out: collections.defaultdict[tuple, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    examples: collections.defaultdict[tuple, list[str]] = collections.defaultdict(list)
    tcache: dict = {}

    def templates(pool):
        key = tuple(pool)
        if key not in tcache:
            tcache[key] = dbxport.Templates.from_index(ix, key)
        return tcache[key]
    for k in names:
        name = k[:-4]
        folder = name.rsplit('/', 1)[0]
        ported = {e[:-4] for e in src.entries if e.endswith('.dbx') and e.rsplit('/', 1)[0] == folder}
        ours = {p: dbxport.guids_of(src.data(p + '.dbx')) for p in ported}

        def resolve(p, ported=ported):
            q = p.lower()
            return q in ported or q in bc2 or '/shaders/' in q or '/shader/' in q
        kw = dict(drop_types=dbxport.NOT_IN_BFBC2, drop_untemplated=True, name=name, ours=ours, schema='report')
        fc = fair_catalogue(cat, name)
        runs = {}
        try:
            runs['predicted'] = dbxport.port(src.data(k), templates(fair_pool(k)), resolve, catalogue=fc,
                                             bones=bones, **kw)[0]
            runs['baseline'] = dbxport.port(src.data(k), templates(fair_pool(k)), resolve, catalogue=fc,
                                            conventions=False, **kw)[0]
            runs['counterpart'] = dbxport.port(src.data(k), templates([name] + fair_pool(k)), resolve, catalogue=cat,
                                               bones=bones, **kw)[0]
        except dbxport.PortError as e:
            log(f'  {k}: not ported ({e})')
            continue
        dice = instances(ix.read(k))
        got = {r: instances(d) for r, d in runs.items()}
        for g, (t, fd) in dice.items():
            short = t.split('.')[-1]
            for p, (_kd, vd) in fd.items():
                for kind_, tp, pat in PREDICTED:
                    if tp and tp != short or not re.search(pat, p):
                        continue
                    key = (kind_, short, re.sub(r'\[\d+\]', '[]', p))
                    hits = {r: got[r].get(g, (None, {}))[1].get(p, (None, None))[1] for r in runs}
                    if hits['baseline'] is None:
                        continue
                    out[key]['fields'] += 1
                    for r, v in hits.items():
                        out[key][r] += v == vd
                    if hits['predicted'] != vd and len(examples[key]) < 3:
                        examples[key].append(f'{k.rsplit("/", 1)[-1]}: DICE {vd} predicted {hits["predicted"]} '
                                             f'BF1943 {hits["baseline"]}')
                    break
    return out, examples


def main(argv=None):
    import bf1943import
    suite_paths.use('mesh-viewer')
    import meshview
    import shaderdb
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('names', nargs='*')
    ap.add_argument('-o', default='dbxparity.txt')
    ap.add_argument('--predict', action='store_true', help='score dbxport\'s value passes and DICE\'s conventions')
    a = ap.parse_args(argv)
    src = bf1943import.Source(bf1943import.default_source())
    ix = meshview.Index(shaderdb.find_opfor(None))
    if a.predict:
        import dbxcatalogue
        import dbxport
        cat = dbxcatalogue.load(ix)
        bones = dbxport.BoneTables.ske01(src, ix)
        names = sorted(k for k in src.entries if k.endswith('.dbx') and k in ix.where and
                       family(k) not in ('objects', 'effects', 'sounds', 'other') and
                       (not a.names or any(x.lower() in k for x in a.names)))
        out, examples = predict(src, ix, names, cat, bones)
        tot: collections.Counter[str] = collections.Counter()
        with open(a.o, 'w', encoding='utf-8') as f:
            f.write(f'dbxport --predict on {len(names)} weapon / vehicle partitions both games have: of DICE\'s values, '
                    f'how many BF1943\'s value (baseline), the passes on a fair pool (predicted) and with DICE\'s '
                    f'version in the pool (counterpart) give\n\n')
            for kind_ in ('rule', 'template'):
                f.write(f'== {kind_}-derived\n')
                for key, c in sorted(out.items()):
                    if key[0] != kind_:
                        continue
                    tot.update({f'{kind_} {x}': v for x, v in c.items()})
                    f.write(f'  {key[1]}{key[2]}: {c["fields"]} fields; baseline {c["baseline"]}, predicted '
                            f'{c["predicted"]}, counterpart {c["counterpart"]}'
                            + (f' | {" ; ".join(examples[key])}' if c['predicted'] != c['fields'] else '') + '\n')
            f.write(f'\ntotals: {dict(tot)}\n')
        print(f'{len(names)} partitions; {dict(tot)} -> {a.o}')
        return 0
    both = sorted(k for k in src.entries if k.endswith('.dbx') and k in ix.where and
                  (not a.names or any(x.lower() in k for x in a.names)))
    partitions, matched, stats, examples, enums = compare(src, ix, both)
    classes: collections.Counter[str] = collections.Counter()
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
