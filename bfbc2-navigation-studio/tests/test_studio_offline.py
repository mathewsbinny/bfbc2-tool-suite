r"""The Navigation Studio's logic without a window or game data (studio.py): a stand-in level and two small path-data
files built in memory (navdata.NavData), the scene and its layers, the difference figures, every colour mode, the
spawn and island reports, picking (areas, spawns, reference markers, the path test, an edit tool taking the
clicks), the path test's routes and overlay, the compare overlay of a reference level, the anomaly layers, the file
choice (default_files), the JSON exports, the argument parsing, and main() driven with a stand-in viewer (layers
hidden at start, --island, --edit, --navtune, --path, --focus, --compare, no path data).

  python test_studio_offline.py
"""
import contextlib
import io
import json
import os
import shutil
import sys
import tempfile
from types import SimpleNamespace
from typing import Any, cast

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
STUDIO = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, STUDIO)
sys.path.insert(0, os.path.join(STUDIO, '..', 'core'))
import overlay  # noqa: E402
import studio  # noqa: E402
from bfbc2core import navdata, navpath, navtune, viewer  # noqa: E402
from imgui_bundle import imgui  # noqa: E402


@contextlib.contextmanager
def gui_frame():
    """One Dear ImGui frame without a window or renderer (the panels' widgets run; nothing is clicked)."""
    if imgui.get_current_context() is None:
        imgui.create_context()
        io = imgui.get_io()
        io.set_ini_filename(None)                  # no imgui.ini written into the current folder
        io.display_size = cast(Any, (1600, 900))
        io.delta_time = 1 / 60
        io.backend_flags |= imgui.BackendFlags_.renderer_has_textures.value
    imgui.new_frame()
    try:
        yield
    except Exception:
        imgui.destroy_context()                    # a panel that raised leaves its window open: a fresh context next time
        raise
    imgui.render()


def panels_run(st, v, panels):
    """Every panel drawn in a frame; the names of those that raised."""
    bad = []
    for name in panels:
        try:
            with gui_frame():
                getattr(st, name)(v)
        except Exception as e:                     # a panel that cannot draw this state
            bad.append(f'{name}: {type(e).__name__} {e}')
    return bad

Y = navdata.Y_OFFSET


def square(x, z, y=0.0, size=1.6):
    """A closed outline polyline (ground + 0.95 m, as stored)."""
    return np.array([[x, y + Y, z], [x + size, y + Y, z], [x + size, y + Y, z + size], [x, y + Y, z + size], [x, y + Y, z]])


def nav_navkit(path):
    """Three areas: two on the main island (the second reached only by a ladder link), a lone detailed sliver on an
    island of one node; four route nodes."""
    a0 = navdata.NavArea(0x0101, 0, (0, 0), 0, 0.8, 0.0, 0.0, [square(0, 0)], [(np.array([0.8, Y, 0]), np.array([0.8, Y, 1.6]), 1)],
                         [(np.array([0, Y, 0]), np.array([0, Y, 1.6]))], island=0, cells=3)
    a1 = navdata.NavArea(0x0101, 1, (0, 0), 0, 0.8, 0.0, 0.0, [square(0, 0.8)], [], [], island=0, cells=2, ladder=True)
    a2 = navdata.NavArea(0x0102, 0, (0, 0), 0, 0.2, 0.0, 0.0, [], [], [], island=1, cells=1)
    xz = np.array([[0.4, 0.4], [1.2, 0.4], [2.0, 0.4], [0.4, 1.2], [1.2, 1.2], [50.4, 50.4]])
    nodes = np.array([[0, Y, 0], [10, Y, 0], [20, Y, 0], [50, Y, 50]], float)
    edges = np.array([[0, 1], [1, 0], [1, 2], [2, 1]])
    return navdata.NavData(path, [a0, a1, a2], xz, np.zeros(6), np.array([0, 0, 0, 1, 1, 2]), nodes, np.array([0x0101] * 3 + [0x0102]),
                           edges, np.array([10.0, 10.0, 10.0, 10.0]), np.array([0, 0, 0, 1]),
                           [navdata.Island(0, 3, 5, np.array([10.0, Y, 0.0])), navdata.Island(3, 1, 1, np.array([50.0, Y, 50.0]))],
                           np.array([False, False, True, True]))


def nav_dice(path):
    """One area: two of NavKit's cells and one NavKit lacks; two nodes joined one way."""
    a0 = navdata.NavArea(0x0101, 0, (0, 0), 0, 0.8, 0.0, 0.0, [square(0, 0), square(4, 0)], [], [], island=0, cells=3)
    xz = np.array([[0.4, 0.4], [1.2, 0.4], [5.2, 0.4]])
    nodes = np.array([[0, Y, 0], [20, Y, 0]], float)
    return navdata.NavData(path, [a0], xz, np.zeros(3), np.zeros(3, int), nodes, np.array([0x0101] * 2), np.array([[0, 1]]),
                           np.array([25.0]), np.array([0, 0]), [navdata.Island(0, 2, 3, np.array([10.0, Y, 0.0]))], None)


class Water:
    def __init__(self):
        self.vertices = np.array([[0, -1, 0], [10, -1, 0], [0, -1, 10]], np.float32)
        self.triangles = np.array([[0, 1, 2]])

    def __len__(self):
        return len(self.triangles)


def spawn(k, pos):
    return SimpleNamespace(category='soldier spawn', position=np.array(pos, float), name=f'spawn {k}', short_type='SoldierSpawnEntityData',
                           guid=f'g{k}')


def fake_level(opfor):
    tri = np.array([[[0, 0, 0], [1, 0, 0], [0, 0, 1]]], float)
    ents = [spawn(0, (1, 0, 0)), spawn(1, (49, 0, 50)), spawn(2, (200, 0, 200)),
            SimpleNamespace(category='placed object', position=np.zeros(3)), SimpleNamespace(category='soldier spawn', position=None)]
    return SimpleNamespace(terrain=(np.array([[0, 0, 0], [5, 0, 0], [0, 0, 5]], float), np.array([[0, 1, 2]])), water=Water(),
                           meshes=[SimpleNamespace(triangles=tri)], entities=ents, heightfield=None, lv='test_lv', opfor=opfor,
                           name='TEST_LV')


class FakeViewer:
    """What the studio uses of viewer.Viewer, without a window."""
    on_pick: Any                                # set by the code under test

    def __init__(self, scene=None, title='', panels=(), on_pick=None, size=None):
        self.scene, self.title, self.panels = scene, title, list(panels)
        self.hidden: set[int] = set()
        self.overlay: list = []
        self.colors: list = []
        self.focused: list = []
        self.camera = SimpleNamespace(focus=lambda p, d=None: self.focused.append((np.asarray(p).tolist(), d)))
        self.select_on_click = True
        self.pick_point = None
        self.exit_after = None
        self.screenshot = None
        self.ran = False
        self.fps, self.renderer, self.hovered = 60.0, 'no renderer (test)', viewer.NO_ID
        self.camera.fly = False

    def layer_visible(self, i):
        return i not in self.hidden

    def set_layer_visible(self, i, on):
        (self.hidden.discard if on else self.hidden.add)(i)

    def set_overlay(self, segs):
        self.overlay = list(segs)

    def set_colors(self, batch, colors):
        self.colors.append((batch, len(colors)))

    def run(self):
        self.ran = True


class StubEditor:
    def __init__(self):
        self.on, self.tool, self.clicks = False, None, []

    def active(self):
        return self.on

    def click(self, v, p):
        self.clicks.append(p)

    def segments(self):
        return [('edit', 'segment', (1, 1, 1))]


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    tmp = tempfile.mkdtemp(prefix='studio_offline_')
    saved = []

    def patch(obj, name, value):
        saved.append((obj, name, getattr(obj, name)))
        setattr(obj, name, value)
    try:
        # --- small pure helpers ---------------------------------------------------------------------------
        e = studio.box_edges([0, 0, 0], 2.0)
        lengths = np.linalg.norm(e[:, 1] - e[:, 0], axis=1)
        check(e.shape == (12, 2, 3) and np.allclose(lengths, 2.0) and np.allclose(e.reshape(-1, 3).min(0), -1.0),
              'box_edges: the 12 edges of the box, each the box\'s size long')
        q = studio.cell_quads(np.array([[0.4, 0.4], [10.0, 10.0]]), np.array([0.0, 5.0]))
        check(q.shape == (4, 3, 3) and np.allclose(q[:2, :, 1], studio.LIFT) and np.allclose(q[2:, :, 1], 5.0 + studio.LIFT)
              and np.isclose(q[:2, :, 0].min(), 0.0) and np.isclose(q[:2, :, 0].max(), 0.8),
              'cell_quads: two triangles per 0.8 m cell, flat at its height plus the lift')
        hf = SimpleNamespace(height=lambda x, z: 0.5 * np.asarray(x))
        q = studio.cell_quads(np.array([[2.0, 0.0], [4.0, 0.0]]), np.array([1.0, 7.0]), hf, lift=0.0)
        check(np.allclose(sorted(set(np.round(q[:2, :, 1].ravel(), 3))), [0.8, 1.2]) and np.allclose(q[2:, :, 1], 7.0),
              'cell_quads: cells on the terrain follow it corner by corner, others (a roof) stay flat')
        check(all(0 <= c <= 1 for c in studio.hue(3)) and studio.hue(1) != studio.hue(2), 'hue: distinct colours per index')
        check(studio.parse_args(['studio.py', 'sp_01', '--edit', '--color', 'pitch', '--hide']) ==
              ({'--edit': True, '--color': 'pitch'}, ['sp_01', '--hide']),
              'parse_args: flags alone, options with their value, a dangling option as an argument')
        check(studio.hidden_at_start({}) >= {'objects', 'DICE ground', 'NavKit route graph'} and 'NavKit ground' not in studio.hidden_at_start({})
              and studio.hidden_at_start({'--hide': 'terrain,water'}) == {'terrain', 'water'},
              'hidden_at_start: objects and every line layer by default, NavKit\'s ground shown; --hide replaces the list')
        r_ok = navpath.Route(True, cost=110.0)
        check(studio.route_cost_change({'NavKit': r_ok, 'DICE': navpath.Route(True, cost=100.0)}) == 10.0
              and studio.route_cost_change({'NavKit': r_ok, 'DICE': navpath.Route(False)}) is None
              and studio.route_cost_change({'NavKit': r_ok}) is None,
              'route_cost_change: NavKit against DICE in per cent, only when both found a route')

        # --- default_files / save_json -----------------------------------------------------------------
        root = os.path.join(tmp, 'opfor')
        check(studio.default_files(root, 'lv') == (None, None), 'default_files: nothing built, no path data')
        builds = [os.path.join(root, 'work', 'navkit', 'lv', 'pathdata.hpd'), os.path.join(root, 'work', 'navkit', 'regress', 'lv.hpd'),
                  os.path.join(root, 'work', 'navbuild', 'lv_W.hpd')]
        for k, p in enumerate(builds):
            os.makedirs(os.path.dirname(p), exist_ok=True)
            open(p, 'wb').close()
            os.utime(p, (1_700_000_000 + 100 * ((k + 1) % 3), 1_700_000_000 + 100 * ((k + 1) % 3)))
        ref = os.path.join(root, 'game', 'Output', 'win32', 'ai', 'levels', 'lv', 'infantry', 'pathdata.hpd')
        os.makedirs(os.path.dirname(ref))
        open(ref, 'wb').close()
        check(studio.default_files(root, 'lv') == (builds[1], ref), 'default_files: NavKit\'s newest build and DICE\'s path data',
              str(studio.default_files(root, 'lv')))
        out = studio.save_json(root, 'x.json', {'a': np.arange(3), 'b': {1, 2} and 'text', 'c': np.float64(1.5)})
        check(out == os.path.join(root, 'work', 'navstudio', 'x.json') and json.load(open(out)) == {'a': [0, 1, 2], 'b': 'text', 'c': 1.5},
              'save_json: into work\\navstudio, numpy values as lists / numbers')

        # --- the scene ---------------------------------------------------------------------------------
        lvl = fake_level(root)
        nk, dc = nav_navkit(os.path.join(root, 'navkit.hpd')), nav_dice(os.path.join(root, 'dice.hpd'))
        st = studio.Studio(lvl, {'NavKit': nk, 'DICE': dc})
        names = list(st.layers)
        check({'terrain', 'water', 'objects', 'soldier spawns', 'NavKit ground', 'NavKit ladder ground', 'NavKit ladder links', 'DICE route graph',
               'DICE ground missing in NavKit', 'anomaly: sliver areas', 'anomaly: lonely nodes'} <= set(names)
              and 'DICE ladder ground' not in names and len(st.scene.layers) == len(names),
              'scene: the level\'s layers, each file\'s layers (ladder ones only where there are ladders), the difference and anomaly layers', str(names))
        d = st.diff
        check(d is not None and abs(d['reproduced'] - 200 / 3) < 1e-9 and abs(d['extra'] - 200 / 3) < 1e-9 and abs(d['ladder'] - 200 / 3) < 1e-9
              and d['dice cells'] == 3 and d['navkit cells'] == 6,
              'difference figures: DICE ground reproduced, NavKit extra and ladder ground, against DICE\'s cells', str(d))
        check(len(st.spawns) == 3 and [(isl, round(dd, 1) if dd is not None else None) for _e, isl, dd in st.spawn_report['NavKit']]
              == [(0, 1.0), (1, 1.0), (None, 212.1)] and [isl for _e, isl, _d in st.spawn_report['DICE']] == [0, None, None],
              'spawn report: the island of each soldier spawn\'s nearest node within 15 m (None beyond)', str(st.spawn_report['NavKit']))
        a = st.anomaly
        check(a is not None and [x['island'] for x in a['small_islands']] == [0, 1] and [x['area'] for x in a['sliver_areas']] == [1, 2]
              and [x['node'] for x in a['lonely_nodes']] == [3],
              'anomalies of NavKit\'s build: small islands, sliver areas, lonely nodes (no AI layer: no spawner or order rows)',
              str({k: len(v) for k, v in (a or {}).items()}))
        st_dice = studio.Studio(lvl, {'DICE': dc})
        check(st_dice.diff is None and st_dice.anomaly is None and st_dice.island_file == 'DICE', 'one file: no difference figures, no anomalies')

        # --- colours -----------------------------------------------------------------------------------
        cols = {}
        for m in studio.COLOR_MODES:
            st.color = m
            cols[m] = (st.cell_colors('NavKit'), st.cell_colors('DICE'))
        nk_i = cols['island'][0]
        check(np.allclose(nk_i[:5], studio.MAIN) and np.allclose(nk_i[5], studio.hue(1)), 'colour by island: the main network green, others by hue')
        check(len({tuple(c) for c in cols['sector'][0]}) == 2, 'colour by sector: one colour per sector')
        check(np.allclose(cols['pitch'][0][:5], studio.NORMAL) and np.allclose(cols['pitch'][0][5], studio.DETAILED), 'colour by pitch: detailed zones orange')
        check(np.allclose(cols['anomaly'][0][:5], studio.MAIN) and np.allclose(cols['anomaly'][0][5], studio.WARNING),
              'colour by anomaly: an island under 25 nodes in the warning colour')
        nk_d, dc_d = cols['difference']
        check(np.allclose(nk_d[:2], studio.SAME_C) and np.allclose(nk_d[2], studio.EXTRA) and np.allclose(nk_d[3:5], studio.LADDER)
              and np.allclose(nk_d[5], studio.EXTRA) and np.allclose(dc_d[2], studio.MISSING) and np.allclose(dc_d[0], (0.55, 0.58, 0.68)),
              'colour by difference: the same ground, NavKit\'s extra, its ladder ground, DICE\'s missing')
        st_dice.color = 'difference'
        check(np.allclose(st_dice.cell_colors('DICE'), studio.SAME_C), 'colour by difference with one file: all one colour')
        check(studio.Studio._ladder_cells(nk).tolist() == [False, False, False, True, True, False]
              and studio.Studio._ladder_cells(navdata.NavData('e', [], np.zeros((0, 2)), np.zeros(0), np.zeros(0, int), np.zeros((0, 3)),
                                                              np.zeros(0), np.zeros((0, 2), int), np.zeros(0), np.zeros(0, int), [])).shape == (0,),
              '_ladder_cells: per cell, ground reached only by ladder; empty files too')
        v = FakeViewer()
        st.color = 'pitch'
        st.apply_colors(v)
        st.apply_colors()
        batch = st.ground_batch['NavKit']
        before = st.scene.batches[batch][2].copy()
        st.color = 'island'
        st.apply_colors()
        after = st.scene.batches[batch][2]
        check(sorted(b for b, _n in v.colors) == sorted(st.ground_batch.values()) and [n for _b, n in v.colors] == [12, 6]
              and not np.array_equal(before, after) and (after[:, 3] == 255).all(),
              'apply_colors: through the viewer once it runs, into the scene\'s batches before')

        # --- islands, ids --------------------------------------------------------------------------------
        gaps = st.island_gaps('NavKit')
        check(len(gaps) == 2 and gaps[0] == 0.0 and abs(gaps[1] - float(np.hypot(30, 50))) < 1e-6 and st.island_gaps('NavKit') is gaps,
              'island_gaps: each island\'s distance to the main network, widening the search; computed once', str(gaps))
        check(st.island_gaps('DICE') == [0.0], 'island_gaps: a file with only its main network')
        check(st.area_of_id(studio.ID_BASE['NavKit'] + 2) == ('NavKit', 2) and st.area_of_id(studio.ID_BASE['DICE']) == ('DICE', 0)
              and st.area_of_id(studio.ID_BASE['DICE'] + 1) is None and st.area_of_id(studio.TERRAIN_ID) is None,
              'area_of_id: a picked id to (file, area); other ids none')

        # --- picking and the path test ---------------------------------------------------------------------
        v = FakeViewer()
        pick = st.on_pick(v)
        pick(studio.ID_BASE['NavKit'] + 1)
        check(st.selected == ('NavKit', 1), 'a click on ground selects its area')
        pick(0x100000 + 2)
        check(st.selected == ('spawn', 2), 'a click on a spawn marker selects the spawn')
        pick(None)
        check(st.selected is None, 'a click on the sky clears the selection')
        st.set_path_mode(v, True)
        check(not v.select_on_click, 'path mode (P): clicks stop selecting')
        v.pick_point = np.array([0.5, 0.0, 0.0])
        pick(studio.TERRAIN_ID)
        v.pick_point = np.array([19.5, 0.0, 0.0])
        pick(studio.TERRAIN_ID)
        r = st.routes
        check(r['NavKit'].found and r['NavKit'].nodes == [0, 1, 2] and r['DICE'].found and r['DICE'].nodes == [0, 1]
              and studio.route_cost_change(r) == 100.0 * (20 - 25) / 25,
              'path test: two clicks - a route in each file', str({k: (x.found, x.nodes) for k, x in r.items()}))
        cross = 2 * 3
        check(len(st.path_segs) == cross + (2 + 2) + (1 + 2) and v.overlay[:len(st.path_segs)] == st.path_segs,
              'path test overlay: crosses at both points, each route\'s chain and its joins', str(len(st.path_segs)))
        v.pick_point = None
        pick(studio.TERRAIN_ID)
        check(len(st.path_pts) == 2, 'path mode: a click on nothing adds no point')
        v.pick_point = np.array([0.0, 0.0, 0.0])
        pick(studio.TERRAIN_ID)
        check(len(st.path_pts) == 1 and st.routes == {}, 'path test: a third click starts again')
        v.pick_point = np.array([50.0, 0.0, 49.0])
        pick(studio.TERRAIN_ID)
        nkr = st.routes['NavKit']
        check(not nkr.found and nkr.gap is not None and nkr.gap[:2] == (2, 3) and 'different islands' in nkr.reason
              and sum(1 for s_ in st.path_segs if s_[2] == studio.MISSING) == 3,
              'path test across islands: no route, the closest gap drawn in red', nkr.reason)
        ed = StubEditor()
        st.editor = ed
        ed.on, ed.tool = True, 'seed'
        v.pick_point = np.array([1.0, 2.0, 3.0])
        pick(studio.TERRAIN_ID)
        st.sync_click_mode(v)
        check(ed.clicks and np.allclose(ed.clicks[-1], [1, 2, 3]) and not st.path_mode and not v.select_on_click,
              'an edit tool takes the clicks (and turns the path test off)')
        st.set_path_mode(v, True)
        check(ed.tool is None, 'path mode on: the edit tool off (one click mode at a time)')
        ed.on = False
        st.set_path_mode(v, False)
        check(v.select_on_click, 'both off: clicks select again')
        st.refresh_overlay(v)
        check(v.overlay[-1] == ('edit', 'segment', (1, 1, 1)) and len(v.overlay) == len(st.path_segs) + 1,
              'overlay: the path test\'s segments, then the edits')
        st.editor = None

        # --- the compare overlay --------------------------------------------------------------------------
        compare = {'report': {'spawners': [{'pos': [1.0, 0.0, 0.5], 'player_type': 'SSAiPlayer', 'enabled': True, 'name': 'ai',
                                            'team': 'Team1', 'squad_id': 'SquadNone', 'in_squad_entity': True},
                                           {'pos': [500.0, 50.0, 0.0], 'player_type': 'SSHumanPlayer', 'enabled': False, 'name': 'hp',
                                            'team': 'Team2', 'squad_id': 'Squad1', 'in_squad_entity': False}],
                              'area_orders': [{'pos': [10.0, 0.0, 30.0], 'name': 'hold', 'area_type': 'AreaHoldGround'}],
                              'objectives': [{'pos': [5.0, 0.0, 5.0], 'type': 'mpobjective'}],
                              'reference': 'ref_lv (OPFOR level copy)', 'reference_xml': 'x/ref_lv.xml', 'reference_has_ai': True,
                              'reference_sector_offsets': (-3, 2), 'target_navkit_build': nk.path,
                              'summary': {'spawners': 2, 'ai_spawners_enabled': 1, 'area_orders': 1, 'patrols': 1, 'objectives': 1}},
                   'layer': {'patrols': [{'name': 'walk', 'points': [[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [5.0, 0.0, 5.0]]},
                                         {'name': 'one point', 'points': [[1.0, 0.0, 1.0]]}]},
                   'pathdata': dc, 'slug': 'level_x'}
        sc = studio.Studio(lvl, {'NavKit': nk}, 'island', compare)
        kinds = [k for k, _e in sc.ref_items]
        check(kinds == ['spawner', 'spawner', 'area order', 'patrol', 'objective'] and 'reference path data outlines' in sc.layers
              and sc.compare_index is not None and sc.ref_items[3][1]['pos'] == [0.0, 0.0, 0.0],
              'compare overlay: a marker per spawner, order, patrol (two points or more) and objective; the reference outlines', str(kinds))
        v = FakeViewer()
        pick = sc.on_pick(v)
        pick(studio.REF_BASE + 0)
        check(sc.selected == ('ref', 0) and sc.ref_near is not None and sc.ref_near[1] == 0 and len(sc.ref_segs) == 1
              and sc.ref_segs[0][2] == (0.3, 1.0, 0.3) and v.overlay == sc.ref_segs,
              'a reference marker picked: a green line to the nearest NavKit node on its floor (within 15 m)', str(sc.ref_near))
        pick(studio.REF_BASE + 2)
        check(sc.ref_segs and sc.ref_segs[0][2] == studio.MISSING and sc.ref_near is not None and sc.ref_near[0] > 15,
              'a reference marker far from the route nodes: the line in red')
        pick(studio.REF_BASE + 1)
        check(sc.ref_segs == [] and sc.ref_near is None, 'a reference marker with no node on its floor: no line')
        pick(studio.REF_BASE + 0)
        pick(studio.ID_BASE['NavKit'])
        check(sc.ref_segs == [] and v.overlay == [] and sc.selected == ('NavKit', 0), 'another pick clears the reference line')
        sd = studio.Studio(lvl, {'DICE': dc}, 'island', compare)
        sd.select_reference(0)
        check(sd.compare_index is None and sd.ref_segs == [] and sd.ref_near is None, 'compare without a NavKit build: markers only, no lines')

        # --- main() with a stand-in viewer -------------------------------------------------------------------
        # --- the panels, drawn without a window ---------------------------------------------------------------
        all_panels = ['panel_main', 'panel_islands', 'panel_selection', 'panel_path', 'panel_compare', 'panel_anomaly', 'panel_status']
        st.triangles = sc.triangles = st.scene.triangle_count()
        v = FakeViewer()
        bad = []
        st.color = 'difference'
        for sel in (None, ('NavKit', 0), ('NavKit', 2), ('DICE', 0), ('spawn', 0), ('spawn', 2)):
            st.selected = sel
            v.hovered = studio.ID_BASE['NavKit'] + 1 if sel else viewer.NO_ID
            bad += panels_run(st, v, all_panels)
        st.set_path_mode(v, True)
        st.path_point(v, [0.5, 0.0, 0.0])
        st.path_point(v, [19.5, 0.0, 0.0])
        bad += panels_run(st, v, ['panel_path'])
        st.path_point(v, [0.5, 0.0, 0.0])
        st.path_point(v, [50.0, 0.0, 49.0])
        bad += panels_run(st, v, ['panel_path'])
        for k in range(len(sc.ref_items)):
            sc.select_reference(k)
            bad += panels_run(sc, v, all_panels)
        sd.selected = ('ref', 0)
        bad += panels_run(sd, v, ['panel_selection', 'panel_compare', 'panel_main', 'panel_islands'])
        check(not bad, 'every panel draws in each state (selections: areas, spawns, reference markers; path test; compare; anomalies)',
              '; '.join(bad[:3]))

        made: list[FakeViewer] = []

        def make_viewer(*a, **k):
            made.append(FakeViewer(*a, **k))
            return made[-1]
        files = {'cand': nk.path, 'ref': dc.path}
        patch(studio.viewer, 'Viewer', make_viewer)
        patch(studio.levelmod, 'load', lambda name: lvl)
        patch(studio, 'default_files', lambda opfor, lv: (files['cand'], files['ref']))
        patch(studio.navdata, 'load', lambda p, hf=None, cache=None: {nk.path: nk, dc.path: dc}[p])
        for p in (nk.path, dc.path):
            open(p, 'wb').close()
        side = os.path.join(tmp, 'side.navtune.json')
        navtune.NavTune.new('Levels/TEST_LV', nk.path).save(side)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = studio.main(['studio.py', 'test_lv', '--island', '1', '--navtune', side, '--path', '0.5,0,0;19.5,0,0',
                                '--focus', '1,2,3', '--distance', '10', '--screenshot', os.path.join(tmp, 'shot.png'), '--frames', '3'])
        v = made[-1]
        text = buf.getvalue()
        hidden = {n for n, i in st.layers.items() if i in v.hidden}
        check(code == 0 and v.ran and v.exit_after == 3 and 'objects' in hidden and 'NavKit outlines' in hidden and 'NavKit ground' not in hidden,
              'main: the window built and run, objects and line layers hidden at start', str(sorted(hidden)))
        check(len(v.panels) == 8 and v.focused[0] == ([50.0, Y, 50.0], 60.0) and v.focused[-1] == ([1.0, 2.0, 3.0], 10.0),
              'main: every panel (the editor\'s too); --island and --focus move the camera', str(v.focused))
        check('path NavKit: cost 20.0 m, 3 nodes' in text and 'path DICE: cost 25.0 m, 2 nodes' in text and 'NavKit: ' in text,
              'main --path: the routes printed', text)
        check(v.on_pick is not None and not os.path.isfile(os.path.join(tmp, 'shot.png')),
              'main: the pick handler set; no screenshot without a frame drawn')
        made.clear()
        with contextlib.redirect_stdout(io.StringIO()):
            studio.main(['studio.py', 'test_lv', '--edit', '--hide', 'terrain', '--island', '9'])
        v = made[-1]
        check({n for n, i in st.layers.items() if i in v.hidden} == {'terrain'} and not v.focused,
              'main --hide: only those layers hidden; an island past the last: the camera stays')
        files['cand'] = files['ref'] = None
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = studio.main(['studio.py', 'test_lv'])
        check(code == 1 and 'no path data for this level' in buf.getvalue(), 'main: no path data and no --compare - said, exit code 1')
        patch(overlay, 'reference_report', lambda lv, spec, opfor, cand: dict(compare['report'], reference='x',
                                                                               reference_pathdata=None))
        patch(overlay, 'read_ai_layer', lambda xml: compare['layer'])
        made.clear()
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = studio.main(['studio.py', 'test_lv', '--compare', 'level:SP 01'])
        check(code == 0 and made and made[-1].ran and 'reference: x' in buf.getvalue() and len(made[-1].panels) == 7,
              'main --compare without path data: the reference\'s layer alone (no editor)')
        from bfbc2core import pick as pickmod
        patch(pickmod, 'pick_level', lambda title: None)
        check(studio.main(['studio.py']) == 1, 'main without a level: the picker, cancelled - exit code 1')
    finally:
        for obj, name, value in reversed(saved):
            setattr(obj, name, value)
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
