r"""The stock-MP AI layer's offline validation (validate.py) on synthetic builds (no game data): each check_* on a
good build and on builds broken the way it guards against, then validate() over a whole synthetic build folder
(build_report.json, the original / authored partitions, the bundle folder, the packed bundle, mp_common) - the
results and validation.json. The levels come from test_stockmp.py's synthetic_level / synthetic_donor; DbxEdit.exe and
FbrbTool.exe (built in the suite) run on temp files only; the path data is a stand-in (cfgmod.Snapper, sector offsets).

  python test_validate.py
"""
import json
import os
import shutil
import sys
import tempfile
from collections import Counter

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
TOOL = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('stockmp', 'core')
sys.path.insert(0, HERE)                    # test_stockmp, beside this file (a tests folder has no short name)

import author  # noqa: E402
import config as cfgmod  # noqa: E402
import dbxbin  # noqa: E402
import levelmodel  # noqa: E402
import pipeline  # noqa: E402
import validate  # noqa: E402
from test_stockmp import KITS, B, G, synthetic_donor, synthetic_level  # noqa: E402

LV, LV_UPPER = 'test_01', 'TEST_01'


def authored(level_bytes, c, donor):
    d = dbxbin.parse(level_bytes)
    author.Author(d, dbxbin.parse(donor), c, levelmodel.Model(d)).build()
    return dbxbin.write(d)


def edit(data, fn):
    """The file with fn(dbx, builder bound to it) applied (the builder's elements intern into the file's strings)."""
    d = dbxbin.parse(data)
    b = B()
    b.d = d
    fn(d, b)
    return dbxbin.write(d)


def inst_of(d, guid):
    return next(e for e in d.instances() if (e.attr('guid') or '').upper() == guid.upper())


def save(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data)
    return path


def partition(guids, typ='Test.ObjectData'):
    b = B()
    insts = [b.inst(g, typ, [b.text('Name', 'x')]) for g in guids]
    return b.done(G(900 + len(guids)), insts)


def external_refs(orig, auth):
    """The references into other partitions the authored level adds (what check_external looks up)."""
    _f, oinst = validate._core_instances(orig)
    _f, inst = validate._core_instances(auth)
    old = {r.lower() for e in oinst.values() for _c, r in validate._refs(e) if '/' in r}
    new = {r.lower() for g, e in inst.items() if g not in oinst or (e.attr('type') or '').endswith('.LevelData')
           for _c, r in validate._refs(e) if '/' in r}
    return sorted(new - old)


def fill_partitions(folder, refs, skip=()):
    """Partitions holding the referenced instances, at their paths under folder."""
    by_part: dict[str, list[str]] = {}
    for r in refs:
        part, guid = r.rsplit('/', 1)
        by_part.setdefault(part, []).append(guid)
    for part, guids in by_part.items():
        if part not in skip:
            save(os.path.join(folder, *part.split('/')) + '.dbx', partition(guids))


def sector_donor(tmp):
    b = B()
    so = b.inst(G(800), 'GameSharedResources.PathdataSectorOffsets', [
        b.text('Name', 'Levels/SP_X/Precomputed/Infantry/SectorOffsets'), b.text('MemoryReportGroup', 'MemoryReportGroup_default'),
        b.num('XOffset', [-2], 'i'), b.num('ZOffset', [-5], 'i')])
    return save(os.path.join(tmp, 'so_donor.dbx'), b.done(G(801), [so]))


def build_folder(bdir, auth, so_donor, offsets, template_ref, extra_tail=()):
    """A bundle folder as pipeline.build leaves it: the authored level, the SectorOffsets assets and the packed
    template partition at the manifest's tail. Returns the added entry names."""
    save(os.path.join(bdir, 'Levels', LV_UPPER + '.dbx'), auth)
    lines = [f'Levels/{LV_UPPER}.dbx|<non-resource>|00010000|{len(auth)}']
    added = []
    for kind, _folder in pipeline.KINDS:
        raw, _pg, _ig = pipeline.sector_asset(so_donor, LV_UPPER, kind, LV, offsets[kind])
        name = f'Levels/{LV_UPPER}/Precomputed/{kind}/SectorOffsets.dbx'
        save(os.path.join(bdir, *name.split('/')), raw)
        added.append(name)
    part, guid = template_ref.rsplit('/', 1)
    tname = part + '.dbx'
    save(os.path.join(bdir, *tname.split('/')), partition([guid]))
    added.append(tname)
    for name in list(added) + list(extra_tail):
        size = os.path.getsize(os.path.join(bdir, *name.split('/'))) if name in added else 0
        lines.append(f'{name}|<non-resource>|00010000|{size}')
    with open(os.path.join(bdir, 'fbrb.manifest'), 'w', encoding='utf-8', newline='') as f:
        f.write('\n'.join(lines) + '\n')
    return added


class FakePathData:
    """ordercheck.PathData's fields check_navigation reads: route nodes (ground + 0.95 m), their groups, areas."""

    def __init__(self, nodes, groups, area_points=()):
        self.pos = np.array(nodes, float)
        self.ids = list(range(len(nodes)))
        self.group = dict(enumerate(groups))
        self.group_size = Counter(groups)
        self.areas = list(area_points)

    def on_area(self, p):
        return any(abs(p[0] - a[0]) < 1 and abs(p[2] - a[2]) < 1 for a in self.areas)


class FakeSnapper(cfgmod.Snapper):
    def __init__(self, pd):
        self.pd = pd
        self.biggest = max(pd.group_size.values())


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    have_tools = os.path.isfile(pipeline.DBX) and os.path.isfile(pipeline.FBRB)
    tmp = tempfile.mkdtemp(prefix='stockmp_validate_test_')
    saved = []

    def patch(obj, name, value):
        saved.append((obj, name, getattr(obj, name)))
        setattr(obj, name, value)
    try:
        lvl, donor = synthetic_level('conquest'), synthetic_donor()
        m = levelmodel.Model(dbxbin.parse(lvl))
        c = cfgmod.generate(m, LV, KITS, {'infantry': None, 'vehicle': None}, bots_per_team=3)
        auth = authored(lvl, c, donor)
        orig_p, auth_p = save(os.path.join(tmp, 'orig.dbx'), lvl), save(os.path.join(tmp, 'auth.dbx'), auth)

        # --- structure ---------------------------------------------------------------------------------
        ok, detail = validate.check_structure(orig_p, auth_p, c)
        check(ok and 'all in Objects' in detail and 'HasAI true' in detail, 'structure: the authored level passes', detail)
        ok, detail = validate.check_structure(orig_p, orig_p, c)
        check(not ok and 'HasAI is not true' in detail and 'PathfindingTypes' in detail,
              'structure: the stock level (no HasAI, no PathfindingTypes) fails', detail)

        def break_structure(d, b):
            lvd = inst_of(d, G(99))
            objs = lvd.child('Objects')
            objs.children.append(objs.children[0])                                  # listed twice
            new_sp = inst_of(d, dbxbin.new_guid(author.NS, LV, 'spawner', 't1_00'))
            new_sp.children.append(b.ref('Dangling', G(12345).lower()))              # a local reference to nothing
            objs.children[:] = [i for i in objs.children if (i.attr('ref') or '').upper() != new_sp.attr('guid').upper()]
            ev = lvd.child('EventConnections').children[0]
            ev.children[0] = b.ref('Source', G(23456).lower())                        # an event from nothing
        bad_p = save(os.path.join(tmp, 'bad_structure.dbx'), edit(auth, break_structure))
        ok, detail = validate.check_structure(orig_p, bad_p, c)
        check(not ok and 'listed twice' in detail and 'local reference' in detail and 'not in Objects' in detail and 'event Source' in detail,
              'structure: a double Objects entry, a dangling reference, an unlisted instance and an event from nothing are named', detail)

        def twice(d, b):                       # an instance whose GUID another one has too (a level authored twice)
            sp = inst_of(d, dbxbin.new_guid(author.NS, LV, 'spawner', 't1_00'))
            d.partition.children.append(d.clone(sp, d))
        dup_p = save(os.path.join(tmp, 'dup_guid.dbx'), edit(auth, twice))
        ok, detail = validate.check_structure(orig_p, dup_p, c)
        check(not ok and '1 GUIDs used by more than one instance' in detail,
              'structure: a GUID used by two instances is named (the GUID-keyed reading would hide it)', detail)

        # --- only intended changes ------------------------------------------------------------------------
        ok, detail = validate.check_changes(orig_p, auth_p, c, tmp)
        check(ok and "'PlayerType': 1" in detail and "'HasAI': 1" in detail,
              'changes: only HasAI and the stock spawner\'s PlayerType (stock_spawners=human)', detail)
        keep = dict(c, stock_spawners='keep')
        ok, detail = validate.check_changes(orig_p, auth_p, keep, tmp)
        check(not ok and 'PlayerType changed although' in detail, 'changes: PlayerType changed while stock_spawners keeps them', detail)

        def change_stock(d, b):
            sp = inst_of(d, G(1))
            d.set_text(sp.child('Name'), 'renamed')                                   # an existing instance changed
            lvd = inst_of(d, G(99))
            objs = lvd.child('Objects').children
            objs[0], objs[1] = objs[1], objs[0]                                      # the stock Objects reordered
            d.partition.children[:] = [i for i in d.partition.children if (i.attr('guid') or '').upper() != G(20)]   # removed
        bad_p = save(os.path.join(tmp, 'bad_changes.dbx'), edit(auth, change_stock))
        ok, detail = validate.check_changes(orig_p, bad_p, c, tmp)
        check(not ok and f'{G(20)} removed' in detail and 'Objects: existing items changed' in detail and '/Name[2]' in detail,
              'changes: a removed instance, reordered stock Objects and a changed stock field are named', detail)

        # --- references into other partitions -------------------------------------------------------------
        refs = external_refs(orig_p, auth_p)
        check(any(r.startswith('objects/soldiers/templates/') for r in refs) and any(r.startswith('ai/pathfinding/') for r in refs),
              'the authored level references the template and the pathfinding settings', str(refs))
        common = os.path.join(tmp, 'mp_common')
        fill_partitions(common, refs)
        ok, detail = validate.check_external(orig_p, auth_p, [os.path.join(tmp, 'nothing here'), common])
        check(ok and f'{len(refs)} new references' in detail, 'external: every new reference resolves in the folders', detail)
        missing_part = refs[0].rsplit('/', 1)[0]
        common2 = os.path.join(tmp, 'mp_common2')
        fill_partitions(common2, refs, skip=(missing_part,))
        save(os.path.join(common2, *refs[-1].rsplit('/', 1)[0].split('/')) + '.dbx', partition([G(4242)]))
        ok, detail = validate.check_external(orig_p, auth_p, [common2])
        check(not ok and f'{refs[0]}: partition not found' in detail and 'instance not in' in detail,
              'external: a missing partition and a missing instance are named', detail)

        # --- packed assets ---------------------------------------------------------------------------------
        so_donor = sector_donor(tmp)
        offsets = {'Infantry': (-9, -4), 'Vehicle': (-9, -3)}
        bdir = os.path.join(tmp, 'build', 'dist')
        added = build_folder(bdir, auth, so_donor, offsets, c['template'])
        rep = {'added_entries': added, 'sector_offsets_values': offsets, 'template_pack': [added[-1]]}
        ok, detail = validate.check_assets(bdir, rep, c, LV_UPPER)
        check(ok and 'Infantry -9,-4' in detail and '1 template partitions packed' in detail, 'assets: SectorOffsets and the template at the tail', detail)
        ok, detail = validate.check_assets(bdir, dict(rep, sector_offsets_values={'Infantry': (-9, -4), 'Vehicle': (1, 2)}), c, LV_UPPER)
        check(not ok and 'Vehicle XOffset is not the integer 1' in detail and 'Vehicle ZOffset is not the integer 2' in detail,
              'assets: offsets other than the path data\'s are named', detail)
        ok, detail = validate.check_assets(bdir, dict(rep, added_entries=added[:1] + added[2:] + added[1:2]), c, LV_UPPER)
        check(not ok and 'not the manifest tail' in detail, 'assets: added entries not in order at the tail', detail)
        ok, detail = validate.check_assets(bdir, dict(rep, template_pack=['Objects/None.dbx']), c, LV_UPPER)
        check(not ok and 'Objects/None.dbx not packed' in detail, 'assets: a template partition not in the folder', detail)
        ok, detail = validate.check_assets(bdir, rep, dict(c, template='objects/x/' + G(31337).lower()), LV_UPPER)
        check(not ok and 'template instance is not in' in detail, 'assets: the template instance not in its packed partition', detail)
        bdir2 = os.path.join(tmp, 'build2', 'dist')
        build_folder(bdir2, auth, so_donor, offsets, c['template'])
        os.remove(os.path.join(bdir2, 'Levels', LV_UPPER, 'Precomputed', 'Vehicle', 'SectorOffsets.dbx'))
        ok, detail = validate.check_assets(bdir2, dict(rep, added_entries=[], template_pack=[]), c, LV_UPPER)
        check(not ok and 'Vehicle SectorOffsets missing' in detail, 'assets: a missing SectorOffsets asset', detail)
        so_inf = os.path.join(bdir2, 'Levels', LV_UPPER, 'Precomputed', 'Infantry', 'SectorOffsets.dbx')
        save(so_inf, edit(open(so_inf, 'rb').read(), lambda d, b: d.set_text(d.instances()[0].child('Name'), 'Levels/OTHER/SectorOffsets')))
        ok, detail = validate.check_assets(bdir2, dict(rep, added_entries=[], template_pack=[]), c, LV_UPPER)
        check(not ok and 'Infantry: Name Levels/OTHER/SectorOffsets' in detail, 'assets: a SectorOffsets asset named for another level', detail)

        # --- Rush stages ---------------------------------------------------------------------------------
        ok, detail = validate.check_stages(auth_p, c)
        check(ok and detail == 'no stages (one-stage mode)', 'stages: Conquest has none', detail)
        rl = synthetic_level('rush')
        cr = cfgmod.generate(levelmodel.Model(dbxbin.parse(rl)), LV, KITS, {}, rush_bots=(2, 2))
        rush_p = save(os.path.join(tmp, 'rush.dbx'), authored(rl, cr, donor))
        ok, detail = validate.check_stages(rush_p, cr)
        check(ok and '1 stage switches' in detail, 'stages: Rush - later stages start disabled, the switch source exists', detail)
        later = next(s for s in cr['spawners'] if s['stage'] == 1)

        def enable_later(d, b):
            e = inst_of(d, dbxbin.new_guid(author.NS, LV, 'spawner', later['id']))
            e.children[e.children.index(e.child('Enabled'))] = b.bool('Enabled', True)
        bad_p = save(os.path.join(tmp, 'rush_bad.dbx'), edit(open(rush_p, 'rb').read(), enable_later))
        cr_bad = json.loads(json.dumps(cr))
        cr_bad['stages'] = [dict(cr['stages'][0], advance=dict(cr['stages'][0]['advance'], source=G(10))),
                            dict(cr['stages'][0], index=7, advance=dict(cr['stages'][0]['advance'], source=G(555)))]
        ok, detail = validate.check_stages(bad_p, cr_bad)
        check(not ok and f'{later["id"]} (stage 1) starts enabled' in detail and 'source is a GameSharedResources.CapturePointEntityData' in detail
              and f'stage 7: source {G(555)} missing' in detail,
              'stages: a later spawner enabled, a switch source of the wrong type or missing', detail)

        # --- navigation (a stand-in path data) --------------------------------------------------------------
        ok, detail = validate.check_navigation(c, os.path.join(tmp, 'none.hpd'))
        check(not ok and detail.startswith('no path data at'), 'navigation: no path data', detail)
        nodes = [list(s['pos']) for s in c['spawners']] + [list(o['pos']) for o in c['orders']]
        nodes = [[p[0], p[1] - cfgmod.SPAWN_LIFT + cfgmod.NODE_Y, p[2]] for p in nodes[:len(c['spawners'])]] + \
                [[p[0], p[1] + cfgmod.NODE_Y, p[2]] for p in nodes[len(c['spawners']):]]
        pd_good = FakePathData(nodes, [0] * len(nodes))
        patch(cfgmod, 'Snapper', lambda hpd: FakeSnapper(pd_good))
        ok, detail = validate.check_navigation(c, 'any.hpd')
        check(ok and f'{len(c["spawners"])} spawners on the navigation data' in detail and 'within 0.0 m' in detail,
              'navigation: every spawner on the route graph, every order with a node, all connected', detail)
        groups = [0] * len(nodes)
        groups[0] = 1                                                               # the first spawner's node on an island of its own
        pd_bad = FakePathData(nodes, groups)
        patch(cfgmod, 'Snapper', lambda hpd: FakeSnapper(pd_bad))
        c_bad = json.loads(json.dumps(c))
        c_bad['spawners'][-1]['pos'] = [300.0, 0.3, 300.0]                         # the last spawner far from every node
        c_bad['orders'][-1]['pos'] = [100.0, 0.0, 200.0]                           # the last order too
        ok, detail = validate.check_navigation(c_bad, 'any.hpd')
        sp0, sp_last = c['spawners'][0]['id'], c['spawners'][-1]['id']
        check(not ok and f'spawner {sp_last}: not on the navigation data' in detail and f'spawner {sp0}: on a small island (1 nodes)' in detail
              and f'order {c["orders"][-1]["id"]}: no route node within' in detail and f'spawner {sp0} is not route-connected' in detail,
              'navigation: off the data, on a small island, an order without a node, a spawner not connected are named', detail)
        c_one = json.loads(json.dumps(c))
        for o in c_one['orders']:
            o['soldiers'] = [x for x in o['soldiers'] if x != c['spawners'][-1]['id']]
        patch(cfgmod, 'Snapper', lambda hpd: FakeSnapper(pd_good))
        ok, detail = validate.check_navigation(c_one, 'any.hpd')
        check(not ok and detail == f"1 AI spawners in no order: ['{c['spawners'][-1]['id']}']", 'navigation: a spawner in no order', detail)
        for obj, name, value in reversed(saved):
            setattr(obj, name, value)
        saved.clear()

        # --- DbxEdit and FbrbTool on the temp files -------------------------------------------------------------
        if not have_tools:
            check(False, 'DbxEdit.exe / FbrbTool.exe not built (run build.bat in bfbc2-dbx-tools and bfbc2-fbrb-tool)')
        else:
            ok, detail = validate.check_roundtrip(auth_p, tmp)
            check(ok and detail == 'roundtrip identical; xml -> import identical', 'round trips: DbxEdit roundtrip and xml -> import', detail)
            junk = save(os.path.join(tmp, 'junk.dbx'), b'{binary}' + b'\x00' * 7)
            ok, detail = validate.check_roundtrip(junk, tmp)
            check(not ok and 'DIFFERENT' in detail, 'round trips: a file DbxEdit cannot read back fails', detail)
            src = os.path.join(tmp, 'src', 'dist')
            save(os.path.join(src, 'Levels', LV_UPPER + '.dbx'), lvl)
            ok, detail = validate.check_dbxedit(orig_p, auth_p, bdir, src, common)
            check(ok and detail == 'authored 1 error(s), 1 warning(s); stock 1 / 1',
                  'DbxEdit check: no more errors or warnings than the stock level (the stand-in kit error is the stock level\'s own)', detail)
            ok, detail = validate.check_dbxedit(orig_p, junk, bdir, src, common)
            check(not ok, 'DbxEdit check: a broken authored file fails', detail)
            out = os.path.join(tmp, 'out', 'level-00.fbrb')
            os.makedirs(os.path.dirname(out))
            pipeline.run(pipeline.FBRB, 'pack', bdir, out)
            ok, detail = validate.check_packed(out, bdir, tmp)
            check(ok and detail == f'{len(added) + 1} files unpack identical', 'packed bundle: unpacks to the built files', detail)
            save(os.path.join(bdir, 'Levels', LV_UPPER + '.dbx'), auth + b'changed')
            ok, detail = validate.check_packed(out, bdir, tmp)
            save(os.path.join(bdir, 'Levels', LV_UPPER + '.dbx'), auth)
            check(not ok and detail == f'1 of {len(added) + 1} files differ after packing', 'packed bundle: a file changed after packing', detail)

            # --- validate(): a whole synthetic build folder ------------------------------------------------------
            opfor = os.path.join(tmp, 'opfor')
            paths = pipeline.Paths(LV, opfor=opfor, work=os.path.join(opfor, 'work', 'stockmp'), game=os.path.join(opfor, 'game'))
            shutil.copytree(common, os.path.join(paths.common, 'mp_common'))
            save(os.path.join(paths.common, 'mp_common', 'fbrb.manifest'), b'')
            shutil.copytree(src, paths.src('dist'))
            shutil.copytree(bdir, os.path.join(paths.root, 'build', 'dist'))
            save(os.path.join(paths.root, 'build', f'{LV_UPPER}_dist.original.dbx'), lvl)
            save(os.path.join(paths.root, 'build', f'{LV_UPPER}_dist.authored.dbx'), auth)
            os.makedirs(os.path.dirname(paths.out_bundle('dist')))
            shutil.copyfile(out, paths.out_bundle('dist'))
            report = {'variants': {'dist': {'entry': f'Levels/{LV_UPPER}.dbx', 'added_entries': added}},
                      'sector_offsets': {k: {'x': v[0], 'z': v[1]} for k, v in offsets.items()}, 'template_pack': [added[-1]]}
            save(os.path.join(paths.root, 'build_report.json'), json.dumps(report).encode())
            patch(cfgmod, 'Snapper', lambda hpd: FakeSnapper(pd_good))
            staged = {'infantry': (-9, -4), 'vehicle': (-9, -3)}
            patch(pipeline, 'sector_offsets', lambda hpd: (staged[os.path.basename(os.path.dirname(hpd))], (1.0, 1.0)))
            said: list[str] = []
            res = validate.validate(paths, c, log=said.append)
            names = [r['check'] for r in res['results']]
            failing = [f'{r["check"]}: {r["detail"]}' for r in res['results'] if not r['ok']]
            check(res['passed'] and not failing and len(names) == 8 + 1 + 2 and names[0] == 'dist: DbxEdit check'
                  and names[-2:] == ['staged infantry path data', 'staged vehicle path data'],
                  'validate: every check passes on the synthetic build (8 per bundle, navigation, the staged path data)', '; '.join(failing))
            saved_json = json.load(open(os.path.join(paths.root, 'validation.json'), encoding='utf-8'))
            check(saved_json == res and len(said) == len(names) and all(s.startswith('ok  ') for s in said),
                  'validate: validation.json holds the results; one log line per check')
            staged['vehicle'] = (0, 0)
            patch(validate, 'check_assets', lambda *a: 1 / 0)
            res = validate.validate(paths, dict(c, layer='minimal'), log=lambda m: None)
            bad = {r['check']: r['detail'] for r in res['results'] if not r['ok']}
            check(not res['passed'] and set(bad) == {'dist: packed assets', 'staged vehicle path data'}
                  and bad['dist: packed assets'] == 'ZeroDivisionError: division by zero' and 'navigation' not in [r['check'] for r in res['results']],
                  'validate: a check that raises is a failed check; staged offsets other than the asset\'s fail; minimal: no navigation check',
                  str(bad))

            def offsets_or_fail(hpd):
                if os.path.basename(os.path.dirname(hpd)) == 'vehicle':
                    raise pipeline.BuildError('no sectors in the path data')
                return staged['infantry'], (1.0, 1.0)
            patch(pipeline, 'sector_offsets', offsets_or_fail)
            res = validate.validate(paths, dict(c, layer='minimal'), log=lambda m: None)
            bad = {r['check']: r['detail'] for r in res['results'] if not r['ok']}
            check(not res['passed'] and 'no sectors' in bad.get('staged vehicle path data', '')
                  and 'staged infantry path data' not in bad,
                  'validate: staged path data whose offsets cannot be read is a failed check, not a crash', str(bad))
    finally:
        for obj, name, value in reversed(saved):
            setattr(obj, name, value)
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
