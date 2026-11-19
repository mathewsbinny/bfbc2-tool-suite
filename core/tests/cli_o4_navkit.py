r"""Round O4's navkit group: the command lines test_cli_o4.py replays for navkit.py, prepare_level.py, levelobjects,
ladders, ordercheck and navcompare.

cases(): the quick tier's (a fake Project OPFOR folder and synthetic path data made in the case folder; no game data).
opfor_calls(opfor): the --game tier's (Project OPFOR's real level partitions and path data, read in place or copied
into a fake OPFOR folder in the case folder; nothing written outside it). The prepare itself on real levels is
test_prepare_o4.py's.
"""
from __future__ import annotations

import json
import os
import random
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import cli_o2_nav as nav  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]

T = '1/0/0/0/0/1/0/0/0/0/1/0/{x}/{y}/{z}/1'


def _nodes(data: bytes) -> list[tuple[float, float, float]]:
    """The node positions of a path data file (the reference reader)."""
    hpdtool, _kymesh = nav._navkit()
    f = hpdtool.parse_reference(data)
    return [tuple(n.pos) for s in f.sectors for n in s.nodes]


def _finite_hpd(seed: int, nodes: int = 30) -> bytes:
    """A random path data file whose meshes and nodes hold no infinities (the core reads every one)."""
    for k in range(seed, seed + 200):
        data = nav._hpd_file(k, (1, 3), nodes)
        hpdtool, kymesh = nav._navkit()
        f = hpdtool.parse_reference(data)
        ok = all(all(abs(v) < 1e30 for n in s.nodes for v in n.pos) for s in f.sectors)
        try:
            for s in f.sectors:
                m = kymesh.parse_mesh(s.mesh)
                for a in m.areas:
                    pts = [p for line in a.outline + a.d_lines for p in line]
                    pts += [p for b in a.portals for p in (b.p0, b.p1)]
                    pts += [p for b in a.border_portals for p in (b.p0, b.p1, b.direction)]
                    pts += [p for c in a.cells for p in [c.direction, *c.points]]
                    ok &= all(abs(v) < 1e30 for p in pts for v in p)
        except Exception:
            ok = False
        if ok and f.sectors and any(s.nodes for s in f.sectors):
            return data
    raise RuntimeError('no finite file')


def _xml(points: list[tuple[str, str, tuple[float, float, float]]], extra: str = '') -> str:
    """A converted level partition with order instances at the points (and a patrol chain)."""
    rows = ['<?xml version="1.0" encoding="utf-8"?>', '<partition guid="p">']
    for g, (typ, name, (x, y, z)) in enumerate(points, 1):
        rows.append(f'<instance guid="{g:08x}-0000-0000-0000-000000000000" type="Entity.{typ}">'
                    f'<field name="Name">{name}</field><complex name="Transform">{T.format(x=x, y=y, z=z)}</complex>'
                    '</instance>')
    rows.append(extra)
    rows.append('</partition>')
    return '\n'.join(rows) + '\n'


def _patrol(pts: list[tuple[float, float, float]]) -> str:
    """A PatrolOrder referencing a chain of AreaGeometry waypoints (Next refs), one of them twice (a loop)."""
    ids = [f'abcdef{k:02x}-1111-2222-3333-444444444444' for k in range(len(pts))]
    rows = [f'<instance guid="ABCDEF99-1111-2222-3333-444444444444" type="Entity.PatrolOrderEntityData">'
            f'<field name="Name">Patrol 1</field><complex name="Transform">{T.format(x=0, y=0, z=0)}</complex>'
            f'<field ref="{ids[0]}" name="Waypoints" /></instance>']
    for k, (x, y, z) in enumerate(pts):
        nxt = ids[(k + 1) % len(ids)]
        rows.append(f'<instance guid="{ids[k].upper()}" type="Entity.AreaGeometryEntityData">'
                    f'<complex name="Transform">{T.format(x=x, y=y, z=z)}</complex>'
                    f'<field name="Next" ref="{nxt}" /></instance>')
    return '\n'.join(rows)


def _level_xml(data: bytes, seed: int) -> str:
    rng = random.Random(seed)
    pos = _nodes(data)
    kinds = ['AreaOrderEntityData', 'InterludeOrderEntityData', 'FireAtAreaOrderEntityData', 'PatrolOrderEntityData']
    pts = []
    for k in range(min(len(pos), 14)):
        x, y, z = pos[rng.randrange(len(pos))]
        pts.append((kinds[k % 4], f'Order {k}', (x + rng.uniform(-3, 3), y + 0.95, z + rng.uniform(-3, 3))))
    pts.append(('AreaOrderEntityData', 'Far away', (90000.0, 0.0, 90000.0)))
    pts.append(('AreaOrderEntityData', 'At the origin', (0.0, 0.0, 0.0)))
    way = [(p[0] + 1.0, p[1] + 0.95, p[2] - 1.0) for p in pos[:4]]
    return _xml(pts, _patrol(way) if way else '')


def _opfor(lv: str = 'sp_test', dice: bytes | None = None, built: bytes | None = None, xml: str | None = None,
           extra: dict[str, Any] | None = None) -> dict[str, Any]:
    """A fake Project OPFOR folder (opf\\) in the case folder: a game copy with three levels (and the folders that
    are not levels), DICE's path data, a prepared level."""
    f: dict[str, Any] = {
        'opf\\game\\BFBC2Game.exe': b'MZ not the game',
        f'opf\\game\\Dist\\win32\\levels\\{lv.upper()}\\level-00.fbrb': b'not a bundle',
        'opf\\game\\Dist\\win32\\levels\\MP_X\\level-00.fbrb': b'not a bundle',
        'opf\\game\\Dist\\win32\\levels\\mp_y\\level-00.fbrb': b'not a bundle',
        'opf\\game\\Dist\\win32\\levels\\sp_common\\level-00.fbrb': b'not a bundle',
        'opf\\game\\Dist\\win32\\levels\\mp_common\\level-00.fbrb': b'not a bundle either',
        'opf\\game\\Dist\\win32\\levels\\opfor_fill\\level-00.fbrb': b'x',
        'opf\\game\\Dist\\win32\\levels\\no_bundle': None,
        'opf\\work': None,
    }
    if dice is not None:
        f[f'opf\\game\\Output\\win32\\ai\\levels\\{lv}\\infantry\\pathdata.hpd'] = dice
    if built is not None:
        f[f'opf\\work\\navkit\\{lv}\\pathdata.hpd'] = (built, 1_700_000_000)    # status prints its time
    if xml is not None:
        f[f'opf\\work\\levels\\{lv}\\dist\\levels\\{lv.upper()}.xml'] = xml
        f[f'opf\\work\\navkit\\{lv}\\navkit.json'] = json.dumps(
            {'level': lv.upper(), 'box': [-100, -100, 100, 100], 'y': [-30, 150], 'box_source': 'given',
             'offsets_source': 'rule: box', 'xoff': -2, 'zoff': -2}, indent=1)
    f.update(extra or {})
    return f


def navkit_cases() -> list[Case]:
    dice, ours, third = _finite_hpd(11), _finite_hpd(40), _finite_hpd(80)
    xml = _level_xml(dice, 1)
    full = _opfor(dice=dice, built=ours, xml=xml)
    o = ['--opfor', '{d}\\opf']
    mp = 'opf\\game\\Output\\win32\\ai\\levels\\mp_x\\infantry\\pathdata.hpd'
    mpy = 'opf\\game\\Output\\win32\\ai\\levels\\mp_y\\infantry\\pathdata.hpd'
    sp = 'opf\\game\\Output\\win32\\ai\\levels\\sp_test\\infantry\\pathdata.hpd'
    marks = dict(full, **{mp: third, mp + '.opfor-new': b'', mpy: third, mpy + '.opfor-stockmp-orig': (ours, 1000),
                          mpy + '.opfor-orig': (dice, 2000), 'opf\\work\\navkit\\mp_y\\navkit.json': '{}'})
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('the usage', {}, [], {}),
        ('help', {}, ['help'], {}),
        ('-h with options', {}, ['--yes', '-h'], {}),
        ('levels', full, ['levels', *o], {}),
        ('levels with OPFOR_ROOT', full, ['levels'], {'OPFOR_ROOT': '{d}\\opf'}),
        ('levels, --root (the older name)', full, ['levels', '--root', '{d}\\opf'], {}),
        ('levels with markers and backups', marks, ['levels', *o], {}),
        ('levels, no game exe', {'opf\\game\\x': 'x'}, ['levels', *o], {}),
        ('levels, no levels folder', {'opf\\game\\BFBC2Game.exe': b'MZ'}, ['levels', *o], {}),
        ('prepare without a level', full, ['prepare', *o], {}),
        ('an unknown level', full, ['prepare', 'SP_NONE', *o], {}),
        ('a level by path', full, ['frobnicate', 'Levels/SP_TEST', *o], {}),
        ('an unknown command', full, ['frobnicate', 'sp_test', *o], {}),
        ('--box without a value', full, ['prepare', 'sp_test', '--box'], {}),
        ('--box not numbers', full, ['prepare', 'sp_test', '--box', '1,x,3,4', *o], {}),
        ('build (handed to the Python)', full, ['build', 'sp_test', *o], {}),
        ('status (handed to the Python)', full, ['status', 'sp_test', *o], {}),
        ('--no-detail (handed to the Python)', full, ['levels', '--no-detail', *o], {}),
        ('compare', full, ['compare', 'SP_TEST', *o], {}),
        ('compare, --reference and --candidate', full, ['compare', 'sp_test', '--reference', '{d}\\opf\\work\\navkit\\sp_test\\pathdata.hpd',
                                                         '--candidate', '{d}\\opf\\game\\Output\\win32\\ai\\levels\\sp_test\\infantry\\pathdata.hpd', *o], {}),
        ('compare, no reference', _opfor(built=ours, xml=xml), ['compare', 'sp_test', *o], {}),
        ('compare, no candidate', _opfor(dice=dice, xml=xml), ['compare', 'sp_test', *o], {}),
        ('compare, not prepared', _opfor(dice=dice, built=ours), ['compare', 'sp_test', *o], {}),
        ('compare, damaged settings', dict(full, **{'opf\\work\\navkit\\sp_test\\navkit.json': '{"level": '}),
         ['compare', 'sp_test', *o], {}),
        ('compare, settings with a BOM', dict(full, **{'opf\\work\\navkit\\sp_test\\navkit.json': b'\xef\xbb\xbf{}'}),
         ['compare', 'sp_test', *o], {}),
        ('compare, no level partition', _opfor(dice=dice, built=ours, xml=xml, extra={
            'opf\\work\\levels\\sp_test\\dist\\levels\\SP_TEST.xml': None}), ['compare', 'sp_test', *o], {}),
        ('check against DICE', full, ['check', 'sp_test', *o], {}),
        ('check, self-check', _opfor(built=ours, xml=xml), ['check', 'sp_test', *o], {}),
        ('check, not built', _opfor(dice=dice, xml=xml), ['check', 'sp_test', *o], {}),
        ('install over DICE', full, ['install', 'sp_test', *o], {}),
        ('install where there was nothing', _opfor(built=ours, xml=xml), ['install', 'sp_test', *o], {}),
        ('install --from', full, ['install', 'sp_test', '--from', '{d}\\other.hpd', *o], {}),
        ('install --from a missing file', full, ['install', 'sp_test', '--from', '{d}\\none.hpd', *o], {}),
        ('install with its traversal annotations', dict(full, **{
            'opf\\work\\navkit\\sp_test\\sp_test.traversal.json': json.dumps(
                {'pathdata_sha1': __import__('hashlib').sha1(ours).hexdigest(), 'links': []})}),
         ['install', 'sp_test', *o], {}),
        ('install, annotations of another file', dict(full, **{
            'opf\\work\\navkit\\sp_test\\sp_test.traversal.json': '{"pathdata_sha1": "00"}',
            'opf\\game\\Output\\win32\\ai\\levels\\sp_test\\infantry\\sp_test.traversal.json': '{}'}),
         ['install', 'sp_test', *o], {}),
        ('install, annotations not a dict', dict(full, **{
            'opf\\work\\navkit\\sp_test\\sp_test.traversal.json': '[1, 2]'}), ['install', 'sp_test', *o], {}),
        ('install again (backup kept once)', dict(full, **{sp + '.opfor-orig': third}), ['install', 'sp_test', *o], {}),
        ('install, not built', _opfor(dice=dice, xml=xml), ['install', 'sp_test', *o], {}),
        ('restore the original', dict(full, **{sp + '.opfor-orig': third, sp + '.opfor-new': b'',
                                              'opf\\game\\Output\\win32\\ai\\levels\\sp_test\\infantry\\sp_test.traversal.json': '{}'}),
         ['restore', 'sp_test', *o], {}),
        ('restore, the level had none', dict(full, **{sp + '.opfor-new': b''}), ['restore', 'sp_test', *o], {}),
        ('restore, nothing installed', full, ['restore', 'sp_test', *o], {}),
        ('prepare: the shared bundle is not a bundle', full, ['prepare', 'sp_test', *o], {}),
        ('prepare: no shared bundle', dict(full, **{'opf\\game\\Dist\\win32\\levels\\sp_common\\level-00.fbrb': None}),
         ['prepare', 'sp_test', *o], {}),
    ]
    out: list[Case] = [(f'navkit: {name}', 'navkit', dict(files, **{'other.hpd': third}), args, env, False)
                       for name, files, args, env in rows]
    return nav.with_variants(out, ['navkit: compare', 'navkit: levels with markers and backups',
                                   'navkit: an unknown level', 'navkit: check against DICE'])


def prepare_level_cases() -> list[Case]:
    full = _opfor()
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('no level', {}, []),
        ('two levels', full, ['a', 'b', '--opfor', '{d}\\opf']),
        ('--opfor without a folder', full, ['sp_test', '--opfor']),
        ('an unknown level', full, ['sp_none', '--opfor', '{d}\\opf']),
        ('no game copy', {'opf\\game': None}, ['sp_test', '--opfor', '{d}\\opf']),
        ('the shared bundle is not a bundle', full, ['sp_test', '--opfor', '{d}\\opf']),
        ('terrain only, no terrain bundle', full, ['SP_TEST', '--terrain-only', '--opfor', '{d}\\opf']),
        ('terrain only, a terrain bundle that is not one', dict(full, **{
            'opf\\game\\Dist\\win32\\levels\\SP_TEST\\terrain-00.fbrb': b'not a bundle'}),
         ['sp_test', '--opfor', '{d}\\opf', '--terrain-only']),
    ]
    # OPFOR_ROOT names the fake folder: without it the search walks up from the suite and finds whatever Project
    # OPFOR sits beside the checkout (the recording's frozen copy has none, a checkout beside OPFOR has one)
    env = {'OPFOR_ROOT': '{d}\\opf'}
    out: list[Case] = [(f'prepare_level: {n}', 'prepare_level', f, a, env, False) for n, f, a in rows]
    return nav.with_variants(out, ['prepare_level: an unknown level'])


def _blueprint(flags: dict[str, str], dynamic: bool = False, mesh: str | None = None) -> str:
    typ = 'GameSharedResources.DynamicModelEntityData' if dynamic else 'Entity.StaticModelEntityData'
    body = ''.join(f'<field name="{k}">{v}</field>' for k, v in flags.items())
    if mesh:
        body += f'<field ref="{mesh}/00000000-0000-0000-0000-000000000000" name="Mesh" />'
    return f'<partition><instance guid="x" type="{typ}">{body}</instance></partition>\n'


def levelobjects_cases() -> list[Case]:
    lv = 'SP_T'
    objs = [('objects/crate/crate_a', '1', '2', '3'), ('objects/fence/Fence_Long', '-10.5', '0', '7'),
            ('objects/ladder/Ladder_5m', '4', '0.5', '-4'), ('objects/stairs/Stairs_Wide', '0', '0', '0'),
            ('objects/barrier/Boom', '1e3', '1', '2'), ('objects/none/Nothing', '0.0001', '0', '0.0004')]
    body = [f'<instance guid="{k:08x}" type="Entity.ReferenceObjectData"><complex name="Transform">'
            f'{T.format(x=x, y=y, z=z)}</complex><field ref="{bp}/00000000-0000-0000-0000-000000000000" '
            f'name="ReferencedObject" /></instance>' for k, (bp, x, y, z) in enumerate(objs)]
    body += [f'<instance guid="s{k}" type="Entity.SoldierSpawnEntityData"><complex name="Transform">'
             f'{T.format(x=k * 3.25, y=1, z=-k)}</complex></instance>' for k in range(3)]
    body.append(f'<instance guid="g1" type="Entity.AreaGeometryEntityData"><complex name="Transform">'
                f'{T.format(x=5, y=6, z=7)}</complex></instance>')
    body.append(f'<instance guid="a1" type="Entity.AreaOrderEntityData"><complex name="Transform">'
                f'{T.format(x=0.1, y=0, z=0.1)}</complex></instance>')
    x = '<partition>\n' + '\n'.join(body) + '\n</partition>\n'
    group = ('<partition><instance guid="h"><array name="Transforms">' + '/'.join(['1', '0', '0', '0'] * 3 + ['9', '8', '7', '1'])
             + '/' + '/'.join(['1'] * 16) + '</array><array name="Assets"><item ref="objects/crate/crate_a_Havok/x" />'
             '<item ref="objects/rocks/Rock_HAVOK/y" /></array></instance></partition>\n')
    tree: dict[str, Any] = {
        f'lv\\dist\\levels\\{lv}.xml': x,
        f'lv\\dist\\levels\\{lv}\\StaticModelGroupEntity_HavokAsset.xml': group,
        f'lv\\dist\\levels\\{lv}\\Other_HavokAsset.xml': '<partition>no transforms</partition>',
        'lv\\pkg': None,
        'lv\\dist\\objects\\crate\\crate_a.xml': _blueprint({'ForbidPathfinding': 'true'}, dynamic=True),
        'lv\\dist\\objects\\fence\\fence_long.xml': _blueprint({'ExcludeFromPathfinding': 'true'}),
        'lv\\dist\\objects\\barrier\\boom.xml': _blueprint({'ForbidPathfinding': 'true',
                                                             'DisablePathfinding': 'PdtWhenAlive'}),
        'common\\objects\\ladder\\ladder_5m.xml': _blueprint({'ForbidPathfinding': 'false',
                                                              'AllowPathdataSeeds': 'true'}, mesh='objects/fence/fence_long_mesh'),
    }
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('the usage', {}, []),
        ('a level', tree, ['{d}\\lv', lv, '{d}\\common']),
        ('a level, no extra folders', tree, ['{d}\\lv', lv]),
        ('a level whose partition is missing', tree, ['{d}\\lv', 'SP_NONE']),
        ('a group asset without Assets', dict(tree, **{f'lv\\dist\\levels\\{lv}\\Bad_HavokAsset.xml':
                                                      '<array name="Transforms">1/2</array>'}), ['{d}\\lv', lv]),
        ('a Transform that is not numbers', dict(tree, **{f'lv\\dist\\levels\\{lv}.xml': x.replace('>1/0/0/0/', '>a/0/0/0/', 1)}),
         ['{d}\\lv', lv]),
    ]
    out: list[Case] = [(f'levelobjects: {n}', 'levelobjects', f, a, {}, False) for n, f, a in rows]
    lad = {'lv\\objects.json': json.dumps([{'bp': 'objects/ladder/ladder_5m', 'flags': None, 'm': [1.0] * 16},
                                           {'bp': 'objects/crate/crate_a', 'flags': None, 'm': [0.0] * 16}]),
           'lv\\dist': None}
    out += [('ladders: the usage', 'ladders', {}, [], {}, False),
            ('ladders: a level with no packfiles', 'ladders', lad, ['{d}\\lv'], {}, False),
            ('ladders: no objects.json', 'ladders', {'lv': None}, ['{d}\\lv'], {}, False)]
    return nav.with_variants(out, ['levelobjects: a level'])


def order_cases() -> list[Case]:
    a, b, c = _finite_hpd(11), _finite_hpd(40), _finite_hpd(80)
    inf = nav._hpd_file(3)
    xml = _level_xml(a, 1)
    # two folders deep: ordercheck prints the name of the folder two above each file
    f = {'l.xml': xml, 'levels\\x\\a.hpd': a, 'h\\k\\b.hpd': b, 'h\\k\\c.hpd': c, 'h\\k\\inf.hpd': inf,
         'u.xml': xml.replace('Order 1<', 'Ordre café Ж<')}
    rows: list[tuple[str, str, list[str]]] = [
        ('ordercheck', 'the usage', []),
        ('ordercheck', 'one file (self-check)', ['{d}\\l.xml', '{d}\\levels\\x\\a.hpd']),
        ('ordercheck', 'two files', ['{d}\\l.xml', '{d}\\levels\\x\\a.hpd', '{d}\\b.hpd']),
        ('ordercheck', 'the same file twice', ['{d}\\l.xml', '{d}\\b.hpd', '{d}\\b.hpd']),
        ('ordercheck', 'three files', ['{d}\\l.xml', '{d}\\levels\\x\\a.hpd', '{d}\\b.hpd', '{d}\\c.hpd']),
        ('ordercheck', 'a file with infinities', ['{d}\\l.xml', '{d}\\inf.hpd']),
        ('ordercheck', 'a missing file', ['{d}\\l.xml', '{d}\\none.hpd']),
        ('ordercheck', 'a missing partition', ['{d}\\none.xml', '{d}\\b.hpd']),
        ('ordercheck', 'names outside cp1252', ['{d}\\u.xml', '{d}\\levels\\x\\a.hpd', '{d}\\b.hpd']),
        ('navcompare', 'the usage', ['{d}\\l.xml']),
        ('navcompare', 'two files', ['{d}\\l.xml', '{d}\\levels\\x\\a.hpd', '{d}\\b.hpd']),
        ('navcompare', '--json', ['{d}\\l.xml', '{d}\\levels\\x\\a.hpd', '{d}\\c.hpd', '--json']),
        ('navcompare', 'the same file', ['--json', '{d}\\l.xml', '{d}\\b.hpd', '{d}\\b.hpd']),
        ('navcompare', 'a missing file', ['{d}\\l.xml', '{d}\\b.hpd', '{d}\\none.hpd']),
    ]
    deep = {'{d}\\b.hpd': '{d}\\h\\k\\b.hpd', '{d}\\c.hpd': '{d}\\h\\k\\c.hpd', '{d}\\inf.hpd': '{d}\\h\\k\\inf.hpd',
            '{d}\\none.hpd': '{d}\\h\\k\\none.hpd'}
    out: list[Case] = [(f'{t}: {n}', t, f, [deep.get(x, x) for x in a], {}, False) for t, n, a in rows]
    return nav.with_variants(out, ['ordercheck: two files', 'ordercheck: names outside cp1252', 'navcompare: --json'])


def cases() -> list[Case]:
    return navkit_cases() + prepare_level_cases() + levelobjects_cases() + order_cases()


def opfor_calls(opfor: str) -> list[Call]:
    """Project OPFOR's real level partitions and path data: ordercheck and navcompare read them in place; navkit's
    levels reads the real folder; compare / check / install / restore run in a fake OPFOR folder in the case folder
    holding copies of a prepared level's files (DICE's path data, NavKit's build, the partition, navkit.json)."""
    calls: list[Call] = []
    work = os.path.join(opfor, 'work')
    calls.append(('navkit levels (the real OPFOR folder)', 'navkit', ['levels', '--opfor', opfor], {}, {}, True))
    for lv in ('sp_009', 'sp_004_b', 'ons_mp_002', 'sp_01'):
        settings = os.path.join(work, 'navkit', lv, 'navkit.json')
        built = os.path.join(work, 'navkit', lv, 'pathdata.hpd')
        dice = os.path.join(opfor, 'game', 'Output', 'win32', 'ai', 'levels', lv, 'infantry', 'pathdata.hpd')
        orig = dice + '.opfor-orig' if os.path.isfile(dice + '.opfor-orig') else dice
        if not (os.path.isfile(settings) and os.path.isfile(built) and os.path.isfile(orig)):
            continue
        with open(settings, encoding='utf-8') as f:
            name = json.load(f)['level']
        xml = os.path.join(work, 'levels', lv, 'dist', 'levels', name + '.xml')
        if not os.path.isfile(xml):
            continue
        calls.append((f'ordercheck {lv} (DICE, NavKit)', 'ordercheck', [xml, orig, built], {}, {}, True))
        calls.append((f'ordercheck {lv} (NavKit alone)', 'ordercheck', [xml, built], {}, {}, True))
        calls.append((f'navcompare {lv}', 'navcompare', [xml, orig, built], {}, {}, True))
        calls.append((f'navcompare {lv} --json', 'navcompare', [xml, orig, built, '--json'], {}, {}, True))
        fake: dict[str, Any] = {
            'opf\\game\\BFBC2Game.exe': (b'MZ', 0),
            f'opf\\game\\Dist\\win32\\levels\\{lv.upper()}\\level-00.fbrb': (b'x', 0),
            f'opf\\game\\Output\\win32\\ai\\levels\\{lv}\\infantry\\pathdata.hpd': orig,
            f'opf\\work\\navkit\\{lv}\\navkit.json': settings,
            f'opf\\work\\navkit\\{lv}\\pathdata.hpd': built,
            f'opf\\work\\levels\\{lv}\\dist\\levels\\{name}.xml': xml,
        }
        o = ['--opfor', '{d}\\opf']
        for cmd in ('compare', 'check', 'install'):
            calls.append((f'navkit {cmd} {lv} (copies)', 'navkit', [cmd, lv, *o], fake, {}, True))
        inst = dict(fake, **{f'opf\\game\\Output\\win32\\ai\\levels\\{lv}\\infantry\\pathdata.hpd.opfor-orig': orig,
                             f'opf\\game\\Output\\win32\\ai\\levels\\{lv}\\infantry\\pathdata.hpd': built})
        calls.append((f'navkit restore {lv} (copies)', 'navkit', ['restore', lv, *o], inst, {}, True))
    return calls


if __name__ == '__main__':
    print(len(cases()), struct.calcsize('<I'))
