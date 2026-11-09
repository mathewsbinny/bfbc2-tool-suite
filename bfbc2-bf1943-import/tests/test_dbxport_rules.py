r"""dbxport's rules on crafted partitions (quick tier: no game data). The 19 edge cases of the deep analysis
(research DBXPORT_DEEP_AUDIT.md, its synth.py) with the expectations the rewrite meets, and a test per rule:
ranked templates, the reference resolver, drop_untemplated, the schema gate and its converters, the value passes
(EIA spellings, bones, DICE's conventions), empty arrays as TEXT, structured reports.

  python test_dbxport_rules.py
"""
import os
import struct
import sys
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import dbxcatalogue  # noqa: E402
import dbxport  # noqa: E402
from bfbc2core import dbx  # noqa: E402

E = dbx.Element
T, N, C = dbx.TEXT, dbx.NUMBERS, dbx.CONTAINER
A = 'aaaaaaaa-0000-0000-0000-00000000000'
B = 'bbbbbbbb-0000-0000-0000-00000000000'
G1 = 'aaaaaaaa-1111-1111-1111-111111111111'
H1 = 'bbbbbbbb-1111-1111-1111-111111111111'


def num(name: str | None, vals: list[float], fmt: str = '>f', tag: str = 'field') -> dbx.Element:
    raw = bytearray()
    dbx._put_varint(raw, len(vals))
    dbx._put_varint(raw, struct.calcsize(fmt))
    for v in vals:
        raw += struct.pack(fmt, v)
    return E(tag, N, [('name', name)] if name is not None else [], '', bytes(raw))


def boolf(name: str, v: bool | int) -> dbx.Element:
    return E('field', 0x6, [('name', name)], '', bytes([0, int(v)]))


def txt(name: str, v: str, tag: str = 'field') -> dbx.Element:
    return E(tag, T, [('name', name)], v)


def ref(name: str, r: str, tag: str = 'field') -> dbx.Element:
    return E(tag, T, [('ref', r), ('name', name)], '')


def item(r: str) -> dbx.Element:
    return E('item', T, [('ref', r)], '')


def arr(name: str, kids: list[dbx.Element]) -> dbx.Element:
    if not kids:
        return E('array', T, [('name', name)], '')
    return E('array', C, [('name', name)], '', b'', kids)


def cx(name: str, kids: list[dbx.Element]) -> dbx.Element:
    return E('complex', C, [('name', name)], '', b'', kids)


def inst(guid: str, typ: str, kids: list[dbx.Element], **kw: str) -> dbx.Element:
    return E('instance', C, [('guid', guid), ('type', typ)] + list(kw.items()), '', b'', kids)


def part(primary: str, insts: list[dbx.Element], guid: str = '11111111-1111-1111-1111-111111111111') -> bytes:
    p = E('partition', C, [('guid', guid), ('primaryInstance', primary), ('exportMode', 'All')], '', b'', insts)
    return dbx.write(dbx.DbxFile([], [p]))


def tmpl(*insts: dbx.Element, name: str = 'tpl', catalogue: Any = None) -> dbxport.Templates:
    return dbxport.Templates([(name, part(insts[0].attr('guid') or '', list(insts),
                                          '22222222-2222-2222-2222-222222222222'))], catalogue)


def parsed(data: bytes) -> Any:
    return dbx.parse(data)


def field(e: Any, *names: str) -> Any:
    for n in names:
        e = e.child(n) if e is not None else None
    return e


def catalogue(**kw: Any) -> dbxcatalogue.Catalogue:
    c = dbxcatalogue.Catalogue()
    for k, v in kw.items():
        setattr(c, k, v)
    return c


def main() -> int:
    failed = []

    def check(ok: Any, what: str, detail: Any = '') -> None:
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            failed.append(what)

    def raises(fn: Any, exc: type[BaseException]) -> bool:
        try:
            fn()
        except exc:
            return True
        return False

    yes = lambda p: True                                        # noqa: E731

    # ---------------------------------------------------------------- the 19 cases of the deep analysis
    # 1. the template's array is empty: the item takes another instance's item schema (no BF1943 schema kept)
    t = dbxport.Templates([('tpl', part(B + '1', [inst(B + '1', 'X.Holder', [arr('Items', []), txt('Name', 'tpl')]),
                                                  inst(B + '2', 'X.Holder', [arr('Items', [cx('', [num('Val', [0.5]),
                                                                                                num('New', [7.0])])]),
                                                                             txt('Name', 'other')])]))])
    bf = part(A + '1', [inst(A + '1', 'X.Holder', [arr('Items', [cx('', [txt('OldField', 'x'), num('Val', [1.0])])]),
                                                   txt('Name', 'bf')])])
    data, rep = dbxport.port(bf, t)
    it = field(parsed(data).instances[0], 'Items').children[0]
    check([c.attr('name') for c in it.children] == ['Val', 'New'] and dbx.value(it.child('Val')) == '1' and
          any(r.action == 'left out' and r.path.endswith('OldField') for r in rep.records),
          '1 an empty template array: items aligned on another instance\'s item (BF1943\'s value kept)',
          [c.attr('name') for c in it.children])

    # 2. ONE_TO_ARRAY with the template's array empty: BF1943's mesh becomes the one item
    t = tmpl(inst(B + '1', 'X.State', [arr('Meshes1p', []), txt('Name', 'tpl')]))
    bf = part(A + '1', [inst(A + '1', 'X.State', [ref('Mesh1p', f'objects/x/mesh/{G1}'), txt('Name', 'bf')])])
    data, rep = dbxport.port(bf, t, yes)
    m = field(parsed(data).instances[0], 'Meshes1p')
    check(m.children and m.children[0].attr('ref') == f'objects/x/mesh/{G1}' and rep.of('converted'),
          '2 ONE_TO_ARRAY, the template\'s array empty: BF1943\'s mesh is the one item')

    # 3. ONE_TO_ARRAY, BF1943's single reference null: an empty array (TEXT), not one null item
    t = tmpl(inst(B + '1', 'X.State', [arr('Meshes1p', [item(f'objects/t/m/{H1}')])]))
    bf = part(A + '1', [inst(A + '1', 'X.State', [ref('Mesh1p', 'null')])])
    m = field(parsed(dbxport.port(bf, t, yes)[0]).instances[0], 'Meshes1p')
    check(m.kind == T and not m.children, '3 ONE_TO_ARRAY, BF1943 null: an empty array', (m.kind, m.children))

    # 4. no hoist by name: a BFBC2-only struct with a child named like a BF1943 field takes the template's value
    t = tmpl(inst(B + '1', 'X.Obj', [txt('Name', 'tpl'), cx('Preview', [txt('Name', 'tplpreview'), num('Scale', [1.0])])]))
    bf = part(A + '1', [inst(A + '1', 'X.Obj', [txt('Name', 'MyObject')])])
    data, rep = dbxport.port(bf, t)
    i = parsed(data).instances[0]
    check(field(i, 'Preview', 'Name').text == 'tplpreview' and field(i, 'Name').text == 'MyObject' and
          not rep.of('nested'), '4 nested only by the explicit map: Preview.Name keeps the template\'s value')

    # 5. pruned to nothing: written as an empty TEXT array (DICE never writes an empty CONTAINER)
    t = tmpl(inst(B + '1', 'X.Obj', [arr('Effects', [item(f'fx/a/{H1}')])]))
    bf = part(A + '1', [inst(A + '1', 'X.Obj', [arr('Effects', [item(f'fx/gone/{G1}')])])])
    data, rep = dbxport.port(bf, t, lambda p: p != 'fx/gone')
    e = parsed(data).instances[0].children[0]
    check(e.kind == T and rep.of('pruned'), '5 pruned to nothing: an empty array as TEXT', e.kind)

    # 6. the primary instance left out: primaryInstance re-pointed to a kept instance
    t = tmpl(inst(B + '2', 'X.Other', [txt('Name', 'tpl')]))
    bf = part(A + '1', [inst(A + '1', 'GameSharedResources.CarHornComponentData', [txt('Name', 'horn')]),
                        inst(A + '2', 'X.Other', [ref('Horn', A + '1')])])
    data, rep = dbxport.port(bf, t, drop_types=dbxport.NOT_IN_BFBC2)
    f = parsed(data)
    check(f.partition.attr('primaryInstance') == A + '2' and rep.of('primary'),
          '6 primary instance left out: primaryInstance re-pointed', f.partition.attr('primaryInstance'))

    # 7. an instance without a template: its references to a left-out instance go too
    t = tmpl(inst(B + '9', 'X.Templated', [txt('Name', 'tpl')]))
    bf = part(A + '2', [inst(A + '1', 'GameSharedResources.CarHornComponentData', [txt('Name', 'horn')]),
                        inst(A + '2', 'X.Untemplated', [ref('Horn', A + '1'), arr('List', [item(A + '1')])])])
    data, rep = dbxport.port(bf, t, yes, drop_types=dbxport.NOT_IN_BFBC2)
    i = parsed(data).instances[0]
    check(field(i, 'Horn').attr('ref') == 'null' and field(i, 'List').kind == T and rep.of('untemplated'),
          '7 untemplated instance: references to the left-out instance nulled / pruned')

    # 8. a BF1943 number where BFBC2 has an enum name (no map) and an empty curve: the first an error, the second
    #    the template's (a converter)
    t = tmpl(inst(B + '1', 'X.Cam', [txt('PitchInputAction', 'EiaPitch'), num('Curve', [0, 0, 0, 0, 0.1, 0, 0, 0])]))
    bf = part(A + '1', [inst(A + '1', 'X.Cam', [num('PitchInputAction', [5], '>i'), num('Curve', [])])])
    check(raises(lambda: dbxport.port(bf, t), dbxport.SchemaError), '8a an unmapped number for an enum: SchemaError')
    data, rep = dbxport.port(bf, t, schema='report')
    i = parsed(data).instances[0]
    check(rep.of('schema') and dbx.value(field(i, 'Curve')).startswith('0/0/0/0/0.1'),
          '8b schema=report: reported; the empty curve takes the template\'s values')

    # 9. bool -> 4-byte integer: an integer 1, not the float's bits
    t = tmpl(inst(B + '1', 'X.Cfg', [num('MaxCount', [5], '>i')]))
    bf = part(A + '1', [inst(A + '1', 'X.Cfg', [boolf('MaxCount', True)])])
    v = dbx.numbers(parsed(dbxport.port(bf, t)[0]).instances[0].children[0])[1][0]
    check(struct.unpack('>i', v)[0] == 1, '9 bool -> integer field: 1', v.hex())
    # ... and into a float: 1.0; the catalogue's evidence decides when the template's value is 0
    t = tmpl(inst(B + '1', 'X.Cfg', [num('Damp', [0.0])]))
    bf = part(A + '1', [inst(A + '1', 'X.Cfg', [boolf('Damp', True)])])
    v = dbx.numbers(parsed(dbxport.port(bf, t)[0]).instances[0].children[0])[1][0]
    check(struct.unpack('>f', v)[0] == 1.0, '9b bool -> float field (template 0): 1.0', v.hex())
    cat = catalogue(numeric={('X.Cfg', '.Damp'): {'whole': 3}})
    v = dbx.numbers(parsed(dbxport.port(bf, t, catalogue=cat)[0]).instances[0].children[0])[1][0]
    check(struct.unpack('>i', v)[0] == 1, '9c ... the catalogue says DICE writes whole numbers there: 1', v.hex())

    # 10. a byte 3 into a float: 3.0, reported as 3 (not 'true')
    t = tmpl(inst(B + '1', 'X.Cfg', [num('Level', [0.5])]))
    bf = part(A + '1', [inst(A + '1', 'X.Cfg', [E('field', 0x5, [('name', 'Level')], '', bytes([0, 3]))])])
    data, rep = dbxport.port(bf, t)
    check(dbx.value(parsed(data).instances[0].children[0]) == '3' and rep.of('value')[0].old == '3',
          '10 byte 3 -> float 3', rep)

    # 11. prune: a struct item whose nested reference will not resolve is left out (any depth)
    t = tmpl(inst(B + '1', 'X.Lvl', [arr('Conns', [cx('', [ref('Source', B + '1'),
                                                            cx('Inner', [ref('Target', f'objects/t/{H1}')])])])]))
    bf = part(A + '1', [inst(A + '1', 'X.Lvl', [arr('Conns', [cx('', [ref('Source', A + '1'),
                                                                      cx('Inner', [ref('Target', f'objects/gone/{G1}')])])])])])
    data, rep = dbxport.port(bf, t, lambda p: p != 'objects/gone', prune=True)
    check(parsed(data).instances[0].child('Conns').kind == T and rep.of('pruned') and not rep.of('substituted'),
          '11 prune: an item with a nested unresolved reference is left out, nothing substituted')
    data, rep = dbxport.port(bf, t, lambda p: p != 'objects/gone')
    tgt = field(parsed(data).instances[0].child('Conns').children[0], 'Inner', 'Target')
    check(tgt.attr('ref') == 'null' and rep.of('nulled'), '11b without prune: the nested reference is nulled')

    # 12. the guid is checked, not just the partition: a reference into a partition DICE moved is relocated
    cat = catalogue(guids={G1: 'objects/common/aimer', H1: 'objects/common/other'},
                    primary={'objects/common/aimer': G1, 'objects/common/other': H1})
    t = tmpl(inst(B + '1', 'X.W', [ref('HaloAimer', f'objects/common/other/{H1}')]))
    bf = part(A + '1', [inst(A + '1', 'X.W', [ref('HaloAimer', f'objects/beach/aimer/{G1}')])])
    data, rep = dbxport.port(bf, t, lambda p: False, catalogue=cat)
    check(field(parsed(data).instances[0], 'HaloAimer').attr('ref') == f'objects/common/aimer/{G1}' and
          rep.of('relocated'), '12 a guid DICE moved: the reference rewritten to its BFBC2 partition')

    # 13. an added field's cross-partition reference is resolved: one to a partition that will not exist is nulled
    t = tmpl(inst(B + '1', 'X.Skel', [txt('Name', 'tpl'), ref('LODSettings', f'characters/skeletons/ske01_lodsettings/{H1}')]))
    bf = part(A + '1', [inst(A + '1', 'X.Skel', [txt('Name', 'bf')])])
    data, rep = dbxport.port(bf, t, lambda p: 'lodsettings' not in p)
    check(field(parsed(data).instances[0], 'LODSettings').attr('ref') == 'null' and rep.of('nulled'),
          '13 an added field\'s reference that will not exist: null')

    # 14. an upper-case local guid naming a left-out instance: null, no template value
    t = tmpl(inst(B + '1', 'X.V', [ref('Horn', f'objects/t/{H1}')]))
    bf = part(A + '2', [inst((A + '1').upper(), 'GameSharedResources.CarHornComponentData', []),
                        inst(A + '2', 'X.V', [ref('Horn', (A + '1').upper())])])
    data, rep = dbxport.port(bf, t, drop_types=dbxport.NOT_IN_BFBC2)
    check(field(parsed(data).instances[0], 'Horn').attr('ref') == 'null' and not rep.of('substituted'),
          '14 upper-case local guid to a left-out instance: null (the template\'s horn not substituted)')

    # 15. array items aligned by position: item 2 takes the template's item 2 (zoom level 2's -1)
    t = tmpl(inst(B + '1', 'X.Zoom', [arr('Levels', [cx('', [num('Fov', [55.0]), num('Impulse', [1.0])]),
                                                     cx('', [num('Fov', [20.0]), num('Impulse', [-1.0])])])]))
    bf = part(A + '1', [inst(A + '1', 'X.Zoom', [arr('Levels', [cx('', [num('Fov', [50.0])]), cx('', [num('Fov', [25.0])])])])])
    lv = parsed(dbxport.port(bf, t)[0]).instances[0].child('Levels').children
    check([dbx.value(x.child('Impulse')) for x in lv] == ['1', '-1'] and dbx.value(lv[1].child('Fov')) == '25',
          '15 array items aligned by position', [dbx.value(x.child('Impulse')) for x in lv])

    # 16. TYPE_AS before drop_untemplated: the pilot seat is written as a player entry, not left out
    t = tmpl(inst(B + '1', 'GameSharedResources.PlayerEntryComponentData', [txt('Name', 'tpl')]))
    bf = part(A + '1', [inst(A + '1', 'GameSharedResources.LandingGearEntryComponentData', [txt('Name', 'pilot')])])
    cat = catalogue(classes={'GameSharedResources.PlayerEntryComponentData'})
    f = parsed(dbxport.port(bf, t, drop_untemplated=True, catalogue=cat)[0])
    check(len(f.instances) == 1 and f.instances[0].attr('type') == 'GameSharedResources.PlayerEntryComponentData',
          '16 TYPE_AS runs before drop_untemplated')

    # 17. a duplicate field: reported, the first kept
    t = tmpl(inst(B + '1', 'X.D', [txt('Name', 'tpl')]))
    bf = part(A + '1', [inst(A + '1', 'X.D', [txt('Name', 'first'), txt('Name', 'second')])])
    data, rep = dbxport.port(bf, t)
    check(field(parsed(data).instances[0], 'Name').text == 'first' and rep.of('duplicate'),
          '17 duplicate field names: the second reported')

    # 18. Vec3 vs Vec4: an error (no converter claims it)
    t = tmpl(inst(B + '1', 'X.P', [num('Offset', [0, 0, 0, 0], tag='complex')]))
    bf = part(A + '1', [inst(A + '1', 'X.P', [num('Offset', [1, 2, 3], tag='complex')])])
    check(raises(lambda: dbxport.port(bf, t), dbxport.SchemaError), '18 number count differs (Vec3 / Vec4): SchemaError')

    # 19. container vs leaf both ways: errors
    t = tmpl(inst(B + '1', 'X.S', [num('Range', [1.0]), cx('Curve', [num('A', [1.0])])]))
    bf = part(A + '1', [inst(A + '1', 'X.S', [cx('Range', [num('Min', [0.0]), num('Max', [2.0])]), num('Curve', [3.0])])])
    try:
        dbxport.port(bf, t)
        n = 0
    except dbxport.SchemaError as ex:
        n = len(ex.problems)
    check(n == 2, '19 container vs leaf: SchemaError naming both fields', n)

    # ---------------------------------------------------------------- ranked template choice
    pool = dbxport.Templates([
        ('veh/a', part(B + '1', [inst(B + '1', 'X.Seat', [txt('InputAction', 'EiaFire'), num('V', [1.0])]),
                                 inst(B + '2', 'X.Seat', [txt('InputAction', 'EiaFireSecondary'), num('V', [2.0])])])),
        ('veh/b', part(B + '3', [inst(B + '3', 'X.Seat', [txt('InputAction', 'EiaFire'), num('V', [3.0])]),
                                 inst(G1, 'X.Seat', [txt('InputAction', 'EiaFire'), num('V', [4.0])])])),
    ])
    bf = part(A + '1', [inst(A + '1', 'X.Seat', [txt('InputAction', 'EIAFireSecondary')]),
                        inst(G1, 'X.Seat', [txt('InputAction', 'EIAFire')]),
                        inst(A + '3', 'X.Seat', [txt('InputAction', 'EIAFire')])])
    data, rep = dbxport.port(bf, pool, donors=['veh/b'])
    steps = [(r.step, r.source) for r in rep.of('template')]
    check(steps == [('donor', 'veh/b'), ('counterpart', 'veh/b'), ('donor', 'veh/b')],
          'ranked: counterpart first, then the paired donor', steps)
    data, rep = dbxport.port(bf, pool)
    f = parsed(data)
    vals = [dbx.value(i.child('V')) for i in f.instances]
    check(vals == ['2', '4', '1'] and [r.step for r in rep.of('template')] == ['role', 'counterpart', 'role'],
          'ranked: the role (input action) picks the pool\'s candidate', (vals, [r.step for r in rep.of('template')]))
    check([i.child('InputAction').text for i in f.instances] == ['EiaFireSecondary', 'EiaFire', 'EiaFire'],
          'EIA spellings as BFBC2\'s in every input-action field')
    # the zoom index: the instance a referrer's array names second takes the template's second
    zt = dbxport.Templates([('w/m24', part(B + '1', [
        inst(B + '1', 'X.Aim', [arr('ZoomLevels', [item(B + '2'), item(B + '3')])]),
        inst(B + '2', 'X.ZoomLevel', [num('Fov', [55.0]), num('Impulse', [1.0])]),
        inst(B + '3', 'X.ZoomLevel', [num('Fov', [20.0]), num('Impulse', [-1.0])])]))])
    bf = part(A + '1', [inst(A + '1', 'X.Aim', [arr('ZoomLevels', [item(A + '2'), item(A + '3')])]),
                        inst(A + '3', 'X.ZoomLevel', [num('Fov', [25.0])]),
                        inst(A + '2', 'X.ZoomLevel', [num('Fov', [50.0])])])
    f = parsed(dbxport.port(bf, zt)[0])
    imp = {i.attr('guid'): dbx.value(i.child('Impulse')) for i in f.instances if i.child('Impulse') is not None}
    check(imp == {A + '2': '1', A + '3': '-1'}, 'ranked: zoom level n takes the template\'s zoom level n', imp)
    # missing pool entries fail; the glob is sorted
    check(raises(lambda: dbxport.Templates([('objects/vehicles/vietnam/m48pattontank/m48pattontank', None)]),
                 dbxport.MissingTemplate), 'a missing pool entry fails (MissingTemplate)')
    # the class default: the catalogue's DICE instance of a type the pool lacks
    cat = catalogue(defaults={'X.Rare': ('dice/rare', inst(B + '7', 'X.Rare', [num('A', [1.0]), num('B', [9.0]),
                                                                               ref('Snd', f'sound/x/{H1}')]))})
    bf = part(A + '1', [inst(A + '1', 'X.Rare', [num('A', [2.0])])])
    data, rep = dbxport.port(bf, tmpl(inst(B + '1', 'X.Other', [])), catalogue=cat)
    i = parsed(data).instances[0]
    check([c.attr('name') for c in i.children] == ['A', 'B', 'Snd'] and dbx.value(i.child('A')) == '2' and
          i.child('Snd').attr('ref') == 'null' and rep.of('template')[0].step == 'default',
          'class default: the catalogue\'s instance, its references (another object\'s) nulled')

    # ---------------------------------------------------------------- the reference resolver
    cat = catalogue(guids={H1: 'sound/weapons/m24_fire', B + '5': 'persistence/weapons'},
                    primary={'sound/weapons/m24_fire': H1, 'sound/x/horn': B + '6', 'persistence/weapons': B + '5'})
    cat.guids[B + '6'] = 'sound/x/horn'
    tw = tmpl(inst(B + '1', 'GameSharedResources.FiringFunctionData', [ref('FireSound', f'sound/x/horn/{B}6'), ref('Persistence', B + '9')]),
              inst(B + '9', 'X.Persist', []), name='w/m24_firing')
    bf = part(A + '1', [inst(A + '1', 'GameSharedResources.FiringFunctionData', [ref('FireSound', f'sound/bf/rifle_fire/{G1}'),
                                                   ref('Persistence', A + '2')]),
                        inst(A + '2', 'GameSharedResources.PersistentWeapon', [])])
    data, rep = dbxport.port(bf, tw, lambda p: False, catalogue=cat)
    i = parsed(data).instances[0]
    check(i.child('FireSound').attr('ref') == 'null' and i.child('Persistence').attr('ref') == 'null' and
          len(parsed(data).instances) == 1 and not rep.of('substituted'),
          'resolver: no cross-role substitution - unresolved references are null (the horn is not written)')
    si = {'sound/bf/rifle_fire': 'sound/weapons/m24_fire', ('Persistence', '*'): f'persistence/weapons/{B}5'}
    data, rep = dbxport.port(bf, tw, lambda p: False, catalogue=cat, stand_ins=si)
    i = parsed(data).instances[0]
    check(i.child('FireSound').attr('ref') == f'sound/weapons/m24_fire/{H1}' and
          i.child('Persistence').attr('ref') == f'persistence/weapons/{B}5' and len(rep.of('substituted')) == 2,
          'resolver: STAND_INS (by partition: its primary instance; by field) stand in')
    data, rep = dbxport.port(bf, tw, lambda p: p.startswith('sound/x'), catalogue=cat,
                             stand_ins={'sound/bf/rifle_fire': dbxport.TEMPLATE})
    check(parsed(data).instances[0].child('FireSound').attr('ref') == 'null' and rep.of('guarded'),
          'role guard: the template\'s horn refused as a weapon sound (TEMPLATE stand-in)')
    check(raises(lambda: dbxport.port(bf, tw, lambda p: False, catalogue=cat, strict=True), dbxport.PortError),
          'strict: an unresolved reference fails the port')
    # the counterpart's reference
    ct = dbxport.Templates([('veh/zero', part(A + '1', [inst(A + '1', 'X.Fx', [ref('Effect', f'fx/dice/{H1}')])]))])
    bf = part(A + '1', [inst(A + '1', 'X.Fx', [ref('Effect', f'effects/bf/{G1}')])])
    cat = catalogue(guids={H1: 'fx/dice'}, primary={'fx/dice': H1})
    data, rep = dbxport.port(bf, ct, lambda p: False, catalogue=cat)
    check(parsed(data).instances[0].child('Effect').attr('ref') == f'fx/dice/{H1}' and rep.of('substituted')[0].note
          == 'the counterpart\'s', 'resolver: the counterpart\'s reference (DICE\'s version of the instance)')
    # ours: a ported partition's guids are checked
    bf = part(A + '1', [inst(A + '1', 'X.V', [ref('A', f'objects/ours/{G1}'), ref('B', f'objects/ours/{H1}')])])
    data, rep = dbxport.port(bf, tmpl(inst(B + '1', 'X.V', [ref('A', 'null'), ref('B', 'null')])),
                             ours={'objects/ours': {G1}}, catalogue=catalogue())
    i = parsed(data).instances[0]
    check(i.child('A').attr('ref') == f'objects/ours/{G1}' and i.child('B').attr('ref') == 'null',
          'resolver: ours checks the guid in a ported partition')
    # required: a placement of an object that will not exist is left out
    bf = part(A + '1', [inst(A + '1', 'Entity.ReferenceObjectData', [ref('ReferencedObject', f'objects/gone/{G1}')]),
                        inst(A + '2', 'X.List', [arr('Objects', [item(A + '1')])])])
    t = dbxport.Templates([('lv', part(B + '1', [inst(B + '1', 'Entity.ReferenceObjectData', [ref('ReferencedObject', 'null')]),
                                                  inst(B + '2', 'X.List', [arr('Objects', [])])]))])
    f = parsed(dbxport.port(bf, t, lambda p: False, required=dbxport.REQUIRED)[0])
    check(len(f.instances) == 1 and f.instances[0].child('Objects').kind == T,
          'required: the placement left out, the list pruned')

    # ---------------------------------------------------------------- drop_untemplated by class
    cat = catalogue(classes={'GameSharedResources.ExplosionEntityData'}, exe_names=frozenset({'MeshShaderSetAsset'}))
    bf = part(A + '1', [inst(A + '1', 'GameSharedResources.ExplosionEntityData', [num('R', [1.0])]),
                        inst(A + '2', 'Render.MeshShaderSetAsset', []),
                        inst(A + '3', 'GameSharedResources.CarHornComponentData', [])])
    f = parsed(dbxport.port(bf, tmpl(inst(B + '1', 'X.Other', [])), drop_untemplated=True, catalogue=cat)[0])
    check([i.attr('type').split('.')[-1] for i in f.instances] == ['ExplosionEntityData', 'MeshShaderSetAsset'],
          'drop_untemplated: only classes BFBC2 lacks (catalogue / executable names) are left out')
    check(raises(lambda: dbxport.port(bf, tmpl(inst(B + '1', 'X.Other', [])), drop_untemplated=True),
                 dbxport.PortError), 'drop_untemplated without a catalogue: an error')

    # ---------------------------------------------------------------- converters without a template, the gate
    bf = part(A + '1', [inst(A + '1', 'GameSharedResources.ChildMovingBodyData', [num('InputAction', [0], '>i')])])
    i = parsed(dbxport.port(bf, tmpl(inst(B + '1', 'X.Other', [])))[0]).instances[0]
    check(i.child('InputAction').text == 'EiaFire', 'enum map without a template: ChildMovingBodyData 0 -> EiaFire')
    cat = catalogue(order={'X.E': ['Kind']}, texts={('X.E', '.Kind'): {'ValA': 4, 'ValB': 2}},
                    exe_names=frozenset({'ValA', 'ValB'}))
    bf = part(A + '1', [inst(A + '1', 'X.E', [txt('Kind', 'ValBombRaid')])])
    data, rep = dbxport.port(bf, tmpl(inst(B + '1', 'X.E', [txt('Kind', 'ValA')])), catalogue=cat)
    check(rep.of('enum'), 'gate: an enum name the executable lacks is reported')
    check(raises(lambda: dbxport.port(bf, tmpl(inst(B + '1', 'X.E', [txt('Kind', 'ValA')])), catalogue=cat,
                                      strict=True), dbxport.PortError), 'gate: ... and fails a strict port')

    # ---------------------------------------------------------------- value passes
    bf = part(A + '1', [inst(A + '1', 'X.Hud', [txt('HintAction1', 'EIAFireCountermeasure'), txt('HintAction2', 'EIUndefined'),
                                                txt('HintLine1', 'VH_SWITCH'),
                                                arr('Suppress', [cx('', [txt('ActionToSuppress', 'EIAPitch')])])])])
    t = tmpl(inst(B + '1', 'X.Hud', [txt('HintAction1', 'EiaUndefined'), txt('HintAction2', 'EiaUndefined'),
                                     txt('HintLine1', 'x'), arr('Suppress', [cx('', [txt('ActionToSuppress', 'EiaThrottle')])])]))
    i = parsed(dbxport.port(bf, t)[0]).instances[0]
    check([i.child('HintAction1').text, i.child('HintAction2').text, i.child('HintLine1').text,
           i.child('Suppress').children[0].child('ActionToSuppress').text] ==
          ['EiaFireCountermeasure', 'EiaUndefined', 'VEHICLE_HINT_CHANGE_POSITION', 'EiaPitch'],
          'EIA -> Eia in every field (countermeasure is BFBC2\'s EiaFireCountermeasure)')
    # bones
    B_ = dbxport.BoneTables(['root', 'Hips', 'jntWpn_7', 'Head'], ['root', 'Head', 'Hips', 'x', 'jntWpn_7'])
    bf = part(A + '1', [inst(A + '1', 'GameSharedResources.BoneFakePhysicsData', [txt('BoneName', 'jntWpn_7'),
                                                                                  num('BoneId', [2], '>i')]),
                        inst(A + '2', 'GameSharedResources.SoldierIKData', [num('WeaponBoneIndex', [1], '>i')])])
    t = tmpl(inst(B + '1', 'GameSharedResources.BoneFakePhysicsData', [txt('BoneName', 'x'), num('BoneId', [0], '>i')]),
             inst(B + '2', 'GameSharedResources.SoldierIKData', [num('WeaponBoneIndex', [0], '>i')]))
    data, rep = dbxport.port(bf, t, bones=B_)
    f = parsed(data)
    check([_int(f.instances[0].child('BoneId')), _int(f.instances[1].child('WeaponBoneIndex'))] == [4, 2] and
          len(rep.of('bones')) == 2, 'bones: BoneId by its BoneName, WeaponBoneIndex by name on BFBC2\'s skeleton')
    # DICE's conventions
    wt = dbxport.Templates([('objects/weapons/m24/m24_firing', part(B + '1', [
        inst(B + '1', 'GameSharedResources.FiringFunctionData', [
            cx('Shot', [num('InitialSpeed', [0, 0, 600, 0], tag='complex'), ref('ProjectileData', B + '2')]),
            cx('FireLogic', [num('RateOfFire', [60.0]), cx('Recoil', [num('MaxRecoilAngleX', [0.0]),
                                                                      num('MaxRecoilFov', [0.5])])]),
            cx('Ammo', [num('NumberOfMagazines', [5], '>i'), num('AmmoPickupMinAmount', [3], '>i'),
                        num('AmmoPickupMaxAmount', [8], '>i')])]),
        inst(B + '2', 'GameSharedResources.BulletEntityData', [num('Gravity', [-9.81])]),
        inst(B + '3', 'GameSharedResources.WeaponFiringData', [boolf('UseAutoAiming', False)])]))])
    bf = part(A + '1', [
        inst(A + '1', 'GameSharedResources.FiringFunctionData', [
            cx('Shot', [num('InitialSpeed', [0, 0, 400, 0], tag='complex'), ref('ProjectileData', A + '2')]),
            cx('FireLogic', [num('RateOfFire', [50.0]), num('MaxRecoilAngleX', [4.0]), num('MaxRecoilFov', [0.5])]),
            cx('Ammo', [num('NumberOfMagazines', [-1], '>i'), num('AmmoPickupMinAmount', [0], '>i'),
                        num('AmmoPickupMaxAmount', [0], '>i')])]),
        inst(A + '2', 'GameSharedResources.BulletEntityData', [num('Gravity', [0.0])]),
        inst(A + '3', 'GameSharedResources.WeaponFiringData', [boolf('UseAutoAiming', True)])])
    data, rep = dbxport.port(bf, wt, name='objects/weapons/beach/k98/k98_firing')
    f = parsed(data)
    ff, bl, wf = f.instances
    got = [dbx.value(field(ff, 'Shot', 'InitialSpeed')), dbx.value(bl.child('Gravity')),
           dbx.value(field(ff, 'Ammo', 'NumberOfMagazines')), dbx.value(field(ff, 'Ammo', 'AmmoPickupMaxAmount')),
           dbx.value(wf.child('UseAutoAiming')), dbx.value(field(ff, 'FireLogic', 'Recoil', 'MaxRecoilAngleX')),
           dbx.value(field(ff, 'FireLogic', 'RateOfFire'))]
    check(got == ['0/0/600/0', '-9.81', '5', '8', 'false', '0', '50'],
          'DICE conventions: 600 m/s, bullet drop, finite ammo and pickups, no auto-aim, the template\'s recoil; '
          'BF1943\'s rate of fire kept', got)
    data, rep = dbxport.port(bf, wt, name='objects/weapons/beach/k98/k98_firing', conventions=False)
    ff = parsed(data).instances[0]
    check(dbx.value(field(ff, 'FireLogic', 'Recoil', 'MaxRecoilAngleX')) == '4' and
          dbx.value(field(ff, 'Shot', 'InitialSpeed')) == '0/0/400/0',
          'conventions=False: BF1943\'s values (the nested recoil moved, as before)')

    # ---------------------------------------------------------------- reports
    t = tmpl(inst(B + '1', 'X.R', [txt('Name', 'tpl'), num('New', [1.0])]))
    bf = part(A + '1', [inst(A + '1', 'X.R', [txt('Name', 'bf'), txt('Old', 'o')])])
    data, rep = dbxport.port(bf, t)
    check(isinstance(rep, list) and all(isinstance(x, str) for x in rep) and len(rep) == len(rep.records) and
          any(x.startswith('added X.R.New') for x in rep) and rep.of('added')[0].source == 'tpl' and
          rep.of('template')[0].step == 'pool', 'reports: text lines from records (path, action, values, source, step)')

    print('all passed' if not failed else f'{len(failed)} check(s) failed')
    return 1 if failed else 0


def _int(e: Any) -> int:
    return struct.unpack('>i', dbx.numbers(e)[1][0])[0]


if __name__ == '__main__':
    sys.exit(main())
