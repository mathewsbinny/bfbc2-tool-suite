r"""overlay.py's headless exports without the game (the command lines kept when the Navigation Studio's window was
retired in round GR2; the map editor's AI workspace draws the same overlays): the usage when neither mode is named,
a level's AI layer read from a made-up partition XML (spawners in both attribute orders, an AreaOrder, a patrol's
waypoint chain, an objective, HasAI), SectorOffsets, an unknown source, and the anomalies of a made-up NavData.

  python bfbc2-navigation-studio\tests\test_overlay_offline.py
"""
import contextlib
import io
import os
import sys
import tempfile

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path.insert(0, SUITE)
sys.path.insert(0, os.path.dirname(HERE))
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import overlay  # noqa: E402
from bfbc2core import navdata  # noqa: E402

failed = 0


def check(ok: bool, what: str, detail: str = '') -> None:
    global failed
    print(('ok   ' if ok else 'FAIL ') + what + ('' if ok else f'  ({detail})'))
    failed += not ok


T = '1/0/0/0/0/1/0/0/0/0/1/0/{x}/{y}/{z}/1'
XML = f'''<partition>
<instance guid="aaaaaaaa-0000-0000-0000-000000000001" type="Entity.LevelData"><field name="HasAI">true</field></instance>
<instance guid="aaaaaaaa-0000-0000-0000-000000000002" type="Entity.SoldierSpawnEntityData">
 <complex name="Transform">{T.format(x=10, y=2, z=20)}</complex><field name="Name">bot</field>
 <field name="PlayerType">SSAiPlayer</field><field name="Enabled">true</field><field name="Team">Team1</field>
 <field name="SoldierSquadId">Squad1</field><array name="Squad"><item ref="x"/></array></instance>
<instance type="Entity.SoldierSpawnEntityData" guid="aaaaaaaa-0000-0000-0000-000000000003">
 <complex name="Transform">{T.format(x=-5, y=0, z=7.5)}</complex><field name="Enabled">false</field></instance>
<instance guid="aaaaaaaa-0000-0000-0000-000000000004" type="Entity.AreaOrderEntityData">
 <complex name="Transform">{T.format(x=1, y=0, z=1)}</complex><field name="AreaType">Defend</field></instance>
<instance guid="aaaaaaaa-0000-0000-0000-000000000005" type="Entity.PatrolOrderEntityData">
 <field name="Name">walk</field><field ref="aaaaaaaa-0000-0000-0000-000000000006" name="Geometry"/></instance>
<instance guid="aaaaaaaa-0000-0000-0000-000000000006" type="Entity.AreaGeometryEntityData">
 <complex name="Transform">{T.format(x=2, y=0, z=2)}</complex><field name="Next" ref="aaaaaaaa-0000-0000-0000-000000000007"/></instance>
<instance guid="aaaaaaaa-0000-0000-0000-000000000007" type="Entity.AreaGeometryEntityData">
 <complex name="Transform">{T.format(x=3, y=0, z=2)}</complex></instance>
<instance guid="aaaaaaaa-0000-0000-0000-000000000008" type="Entity.ReferenceObjectData">
 <field ref="objects/gameplay/mpobjective/aaaa" name="ReferencedObject"/><complex name="Transform">{T.format(x=4, y=1, z=4)}</complex></instance>
</partition>
'''


def main() -> int:
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        code = overlay.main(['overlay.py', 'sp_009'])
    check(code == 1 and 'overlay.py <target level>' in buf.getvalue(), 'no --reference or --anomalies prints the usage')
    with tempfile.TemporaryDirectory() as tmp:
        xml = os.path.join(tmp, 'test_lv.xml')
        open(xml, 'w', encoding='utf-8').write(XML)
        layer = overlay.read_ai_layer(xml)
        check(layer['has_ai'] is True, 'HasAI read')
        s = layer['spawners']
        check(len(s) == 2 and s[0]['pos'] == [10.0, 2.0, 20.0] and s[0]['player_type'] == 'SSAiPlayer'
              and s[0]['enabled'] and s[0]['in_squad_entity'] and not s[1]['enabled'] and s[1]['pos'] == [-5.0, 0.0, 7.5],
              'spawners, in both attribute orders', str(s))
        check(layer['area_orders'] == [{'pos': [1.0, 0.0, 1.0], 'name': '', 'area_type': 'Defend'}], 'the AreaOrder',
              str(layer['area_orders']))
        check(layer['patrols'] == [{'name': 'walk', 'points': [[2.0, 0.0, 2.0], [3.0, 0.0, 2.0]]}], 'the patrol chain',
              str(layer['patrols']))
        check(layer['objectives'] == [{'pos': [4.0, 1.0, 4.0], 'type': 'mpobjective'}], 'the objective')
        off = os.path.join(tmp, 'test_lv', 'Precomputed', 'Infantry')
        os.makedirs(off)
        open(os.path.join(off, 'SectorOffsets.xml'), 'w').write('<f name="XOffset">-3</f><f name="ZOffset">12</f>')
        label, x, hpd, off_xml = overlay.reference_source(xml, tmp)
        check(x == xml and hpd is None and overlay.sector_offsets(off_xml) == (-3, 12), 'a partition path as the source')
        try:
            overlay.reference_source('nothing:here', tmp)
            check(False, 'an unknown source is refused')
        except ValueError:
            check(True, 'an unknown source is refused')
    # anomalies of a made-up file: one small island, a lonely node, a sliver area, an order without a node
    a = navdata.NavArea(1, 0, (0, 0), 0, 0.8, 0.0, 0.0, cells=2, island=1)
    pos = np.array([[0.0, 0.95, 0.0], [5.0, 0.95, 0.0], [500.0, 0.95, 500.0]])
    nav = navdata.NavData('x.hpd', [a], np.array([[0.0, 0.0], [0.8, 0.0]]), np.zeros(2), np.zeros(2, int), pos,
                          np.ones(3, int), np.array([[0, 1]]), np.ones(1), np.array([0, 0, 1]),
                          [navdata.Island(0, 2, 0, pos[:2].mean(0)), navdata.Island(2, 1, 2, pos[2])])
    an = overlay.anomalies(nav, {'spawners': [], 'area_orders': [{'pos': [300.0, 0.0, 300.0], 'name': 'far'}], 'patrols': []})
    check([len(an[k]) for k in ('small_islands', 'orders_without_node', 'sliver_areas', 'lonely_nodes')] == [2, 1, 1, 1],
          'anomalies: small islands, an order without a node, a sliver, a lonely node', str({k: len(v) for k, v in an.items()}))
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
