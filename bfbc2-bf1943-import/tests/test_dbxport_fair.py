r"""dbxport's fair-port regression (game tier; --quick: a dozen partitions, the quick tier - skipped without game data).

Every partition BFBC2 ships under the same name as BF1943 (DICE's own conversion of it: the ground truth) is ported
from BF1943's copy with dbxport on a template pool that does NOT hold DICE's same-named partition (the callers'
pools: objects, vehicles, weapons; the catalogue's class defaults for the rest, never the partition's own), with the
catalogue, BFBC2's soldier skeleton and DICE's conventions. Asserted:

  - the port succeeds (no SchemaError: every shape difference has a converter), but for NOT_PORTABLE (what BFBC2
    changed beyond a converter: the 3p soldier anim trees' aim tables) - those must fail with a
    SchemaError; partitions with a type only DICE's same-named partition holds (no fair template: menus, awards,
    localization) are counted and left out;
  - no 'kind' or 'schema' differences from DICE's version: each instance both have holds DICE's fields, each leaf
    DICE's kind (text, reference, number size, bool);
  - primaryInstance names an instance of the partition; no empty array is written as a CONTAINER;
  - BoneFakePhysicsData.BoneId equals DICE's (24 of 24: BFBC2's skeleton reordered the bones);
  - the halo aimer: a reference into BF1943's objects/soldiers/common/beach/defaulthaloaimer keeps its guid, in the
    partition DICE moved it to;
  - recoil (FireLogic.Recoil, the 27 firings both games have) equals DICE's with DICE's version of the instance in the
    pool (the counterpart: the ranked choice takes it whenever BFBC2 ships the instance) - the template's recoil is
    DICE's convention; without it the fair donor's is reported.

  python test_dbxport_fair.py [--quick] [category ...]      categories: objects vehicles weapons effects sounds other
"""
import collections
import os
import re
import sys
import time
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import dbxparity  # noqa: E402
import dbxport  # noqa: E402
from bfbc2core import dbx  # noqa: E402

QUICK = ['objects/weapons/handheld/beach/thompson/thompson.dbx', 'objects/weapons/handheld/beach/thompson/thompson_firing.dbx',
         'objects/weapons/handheld/beach/m1garand/m1garand_firing.dbx',
         'objects/weapons/handheld/bu_sni_m95/bu_sni_m95.dbx',
         'objects/vehicles/beach/jap_zero_fighter/zero_firing.dbx', 'objects/vehicles/beach/kurogane/kurogane.dbx',
         'objects/vehicles/beach/stationary_aa_bofors/aa_bofors.dbx']
AIMER = 'objects/soldiers/common/beach/defaulthaloaimer'
# partitions dbxport refuses, rightly: BFBC2's shape is not BF1943's and no converter can make it (anim trees are
# not ported by design - BFBC2 ships them compiled; its AimDirections tables hold 104 numbers, BF1943's 40)
NOT_PORTABLE = re.compile(r'^characters/animtree/soldieranimtree3p')


def cat_of(k: str) -> str:
    if k.startswith('objects/weapons/'):
        return 'weapons'
    if k.startswith('objects/vehicles/'):
        return 'vehicles'
    if k.startswith('objects/'):
        return 'objects'
    if k.startswith(('effects/', 'fx/', 'emitters/')):
        return 'effects'
    if k.startswith('sound/'):
        return 'sounds'
    return 'other'


def kind(e: dbx.Element) -> str:
    if e.kind == dbx.CONTAINER:
        return 'array' if e.tag == 'array' else f'{e.tag}-container'
    if e.attr('ref') is not None:
        return 'ref'
    if e.kind == dbx.TEXT:
        return 'array' if e.tag == 'array' else 'text'
    if e.kind == dbx.NUMBERS:
        return f'num{dbx.numbers(e)[0]}'
    return 'byte'


def compare(o: dbx.Element, d: dbx.Element, path: str, note: Any) -> None:
    """Kind and schema differences of ours (o) against DICE's (d)."""
    ko, kd = kind(o), kind(d)
    if ko != kd:
        note('kind', f'{path}: {ko} vs DICE {kd}')
        return
    if o.kind != dbx.CONTAINER or d.kind != dbx.CONTAINER:
        return
    if o.tag == 'array':
        for a, b in zip(o.children or [], d.children or []):
            compare(a, b, f'{path}[]', note)
        return
    on = [c.attr('name') for c in o.children or ()]
    dn = [c.attr('name') for c in d.children or ()]
    if on != dn:
        note('schema', f'{path}: missing {[x for x in dn if x not in on][:4]} extra {[x for x in on if x not in dn][:4]}'
             if set(on) != set(dn) else f'{path}: order')
    for c in o.children or ():
        n = c.attr('name')
        if n is not None and n in dn:
            compare(c, d.child(n) or c, f'{path}.{n}', note)


def empty_containers(e: dbx.Element) -> int:
    n = int(e.tag == 'array' and e.kind == dbx.CONTAINER and not e.children)
    return n + sum(empty_containers(c) for c in e.children or ())


def recoil(f: dbx.DbxFile) -> dict[str, str]:
    out = {}
    for i in f.instances:
        if (i.attr('type') or '').endswith('FiringFunctionData'):
            r = (i.child('FireLogic') or dbx.Element('x', 0)).child('Recoil')
            if r is not None:
                out[(i.attr('guid') or '').lower()] = '/'.join(dbx.value(c) for c in r.children or ())
    return out


def main(argv: list[str]) -> int:
    quick = '--quick' in argv
    cats = [a for a in argv[1:] if not a.startswith('--')] or ['objects', 'vehicles', 'weapons', 'effects', 'sounds',
                                                               'other']
    # the modules under test and their helpers import outside the skip: a broken import is a failure, not a SKIP
    import bf1943import
    suite_paths.use('mesh-viewer', 'shader-tools')
    import dbxcatalogue
    import meshview
    import shaderdb
    try:                                                        # only missing game data skips
        root = shaderdb.find_opfor(None)
        folder = bf1943import.default_source()
        if not folder or not os.path.isdir(folder):
            raise bf1943import.Bf1943ImportError(f'no BF1943 source folder at {folder}')
        src = bf1943import.Source(folder)
    except (shaderdb.ShaderDbError, bf1943import.Bf1943ImportError) as e:
        print(f'SKIP no game data ({type(e).__name__}: {e})')
        return 0
    ix = meshview.Index(root)
    t0 = time.time()
    cat = dbxcatalogue.load(ix)
    bones = dbxport.BoneTables.ske01(src, ix)
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')}
    both = QUICK if quick else sorted(k for k in src.entries if k.endswith('.dbx') and k in ix.where and
                                      'build_overlay' not in ix.where[k][0].lower() and cat_of(k) in cats)
    fails: list[str] = []
    stats: collections.Counter[str] = collections.Counter()
    examples: dict[str, list[str]] = collections.defaultdict(list)
    tcache: dict[tuple, dbxport.Templates] = {}

    def note(cls: str, what: str) -> None:
        stats[cls] += 1
        if len(examples[cls]) < 12:
            examples[cls].append(what)

    def templates(pool: list[str], exclude: str) -> dbxport.Templates:
        key = tuple(p for p in pool if p != exclude)
        if key not in tcache:
            tcache[key] = dbxport.Templates.from_index(ix, key)
        return tcache[key]

    bone_ok = bone_all = aim_ok = aim_all = rec_ok = rec_all = rec_fair = 0
    n_done = 0
    for k in both:
        name = k[:-4]
        ct = cat_of(k)
        folder = name.rsplit('/', 1)[0]
        ported = {e[:-4] for e in src.entries if e.endswith('.dbx') and e.rsplit('/', 1)[0] == folder}
        ours = {p: dbxport.guids_of(src.data(p + '.dbx')) for p in ported}

        def resolve(p: str, ported: set = ported) -> bool:
            q = p.lower()
            return q in ported or q in bc2 or '/shaders/' in q or '/shader/' in q
        pool = dbxparity.fair_pool(k)
        fc = dbxparity.fair_catalogue(cat, name)
        T0 = templates(pool, name)
        lone = sorted({t for i in dbx.parse(src.data(k)).instances for t in [i.attr('type') or '']
                       if t not in T0 and t not in fc.defaults and t in cat.defaults and
                       t not in dbxport.NOT_IN_BFBC2})
        if lone:
            note('no fair template', f'{k}: {lone[:3]}')
            continue
        kw: dict[str, Any] = dict(drop_types=dbxport.NOT_IN_BFBC2, drop_untemplated=True, bones=bones, name=name,
                                  ours=ours)
        try:
            data, rep = dbxport.port(src.data(k), T0, resolve, catalogue=fc, **kw)
        except dbxport.SchemaError as e:
            note('refused (not portable)' if NOT_PORTABLE.search(k) else 'port failed', f'{k}: {e}'[:300])
            continue
        except dbxport.PortError as e:
            note('port failed', f'{k}: {e}'[:300])
            continue
        if NOT_PORTABLE.search(k):
            note('port failed', f'{k}: ported, but BFBC2\'s shape cannot be BF1943\'s')
        n_done += 1
        f, d, bf = dbx.parse(data), dbx.parse(ix.read(k)), dbx.parse(src.data(k))
        guids = {(i.attr('guid') or '').lower() for i in f.instances}
        if (f.partition.attr('primaryInstance') or '').lower() not in guids:
            note('primaryInstance missing', k)
        n = sum(empty_containers(i) for i in f.instances)
        if n:
            note('empty CONTAINER array', f'{k}: {n}')
        dice = {(i.attr('guid') or '').lower(): i for i in d.instances}
        for i in f.instances:
            di = dice.get((i.attr('guid') or '').lower())
            if di is None or di.attr('type') != i.attr('type'):
                continue
            compare(i, di, f'{k.rsplit("/", 1)[-1]}:{(i.attr("type") or "").rsplit(".", 1)[-1]}',
                    lambda cls, w: note(cls, w))
            if (i.attr('type') or '').endswith('BoneFakePhysicsData'):
                bone_all += 1
                bone_ok += dbx.value(i.child('BoneId') or i) == dbx.value(di.child('BoneId') or di)
        x = dbx.to_xml(src.data(k))
        if AIMER in x:
            for g in re.findall(AIMER + r'/([0-9a-fA-F-]{36})', x):
                aim_all += 1
                aim_ok += f'objects/soldiers/common/defaulthaloaimer/{g.lower()}' in dbx.to_xml(data).lower()
        if ct in ('weapons', 'vehicles') and b'FiringFunctionData' in src.data(k):
            dr = recoil(d)
            fr = recoil(f)
            # the counterpart: DICE's version of the instances in the pool, as the ranked choice takes it
            T = dbxport.Templates.from_index(ix, [name] + [p for p in pool if p != name])
            cdata, _ = dbxport.port(src.data(k), T, resolve, catalogue=cat, **kw)
            cr = recoil(dbx.parse(cdata))
            for g, v in dr.items():
                if g in {(i.attr('guid') or '').lower() for i in bf.instances}:
                    rec_all += 1
                    rec_ok += cr.get(g) == v
                    rec_fair += fr.get(g) == v
        if n_done % 100 == 0:
            print(f'  {n_done}/{len(both)} {time.time() - t0:.0f} s', flush=True)

    def check(ok: bool, what: str, cls: str | None = None) -> None:
        print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)
        if not ok:
            fails.append(what)
            for e in examples.get(cls or '', [])[:12]:
                print(f'       {e}')

    print(f'{n_done} of {len(both)} partitions ported ({time.time() - t0:.0f} s); {stats["no fair template"]} with a '
          f'type only DICE\'s same partition holds (not tested); {stats["refused (not portable)"]} refused as expected')
    check(not stats['port failed'], f'every port succeeds ({stats["port failed"]} failed)', 'port failed')
    check(not stats['kind'], f'no kind differences from DICE ({stats["kind"]})', 'kind')
    check(not stats['schema'], f'no schema differences from DICE ({stats["schema"]})', 'schema')
    check(not stats['primaryInstance missing'], 'primaryInstance present', 'primaryInstance missing')
    check(not stats['empty CONTAINER array'], 'no empty CONTAINER arrays', 'empty CONTAINER array')
    check(bone_ok == bone_all, f'BoneId as DICE\'s: {bone_ok} of {bone_all}')
    check(aim_ok == aim_all, f'halo aimer guid kept, relocated: {aim_ok} of {aim_all}')
    check(rec_ok == rec_all, f'recoil as DICE\'s (counterpart in the pool): {rec_ok} of {rec_all} firings; '
          f'the fair donor\'s: {rec_fair} of {rec_all}')
    print('all passed' if not fails else f'{len(fails)} check(s) failed')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
