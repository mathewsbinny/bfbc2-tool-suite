r"""The Map Inspector's logic without a window or game data (inspector.py): a stand-in level built in memory
(bfbc2core.level.Level with entities, meshes, outlines, terrain and water), the scene and its layers, the counts,
the references between objects (and who refers to whom), the search, selection and its link lines, following a
reference and going back, measuring (points, figures, the water above a point), picking, every panel drawn in a
Dear ImGui frame without a window, and main() driven with a stand-in viewer (layers hidden at start, --select,
--measure, --screenshot, no level chosen).

  python test_inspector_offline.py
"""
import contextlib
import io
import os
import sys
from types import SimpleNamespace
from typing import Any, cast

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
TOOL = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402
suite_paths.use('core', 'map-inspector')
import inspector  # noqa: E402
from bfbc2core import dbx, level as levelmod, viewer  # noqa: E402
from imgui_bundle import imgui  # noqa: E402

G = lambda n: f'00000000-0000-4000-8000-{n:012d}'


def text(name, value, ref=None):
    attrs = [('name', name)] + ([('ref', ref)] if ref is not None else [])
    return dbx.Element('field', dbx.TEXT, attrs, text=value)


def element(guid, typ, fields):
    return dbx.Element('instance', dbx.CONTAINER, [('guid', guid), ('type', typ)], children=fields)


def matrix(pos, forward=(0.0, 0.0, 1.0)):
    m = np.eye(4)
    m[2, :3] = forward
    m[3, :3] = pos
    return m


def entity(k, typ, category, pos=None, fields=(), **kw):
    short = typ.split('.')[-1]
    return levelmod.Entity(k, G(k), typ, short, category, kw.pop('id', None), kw.pop('name', None), 'levels/test',
                           None if pos is None else matrix(pos), kw.pop('blueprint', None), element(G(k), typ, list(fields)), **kw)


class Water:
    """water.Water's fields the inspector reads."""

    def __init__(self, vertices, triangles, shader):
        self.vertices, self.triangles, self.shader = vertices, triangles, shader

    def __len__(self):
        return len(self.triangles)


def fake_level():
    """Entities: a level data list (references every object: not a link), spawns of both teams and none, a vehicle
    spawn, an objective referring to a spawn (and to itself, and to an unknown guid), a placed object with a mesh,
    an effect, a group member (no marker), an entity without a position; an area outline with a height, a route."""
    long_value = 'x' * 200
    ents = [
        entity(0, 'GameSharedResources.LevelData', 'other', None, [dbx.Element('array', dbx.CONTAINER, [('name', 'Objects')],
                                                                               children=[text('', '', G(k)) for k in range(1, 9)])]),
        entity(1, 'GameSharedResources.SoldierSpawnEntityData', 'soldier spawn', (10, 0, 0), [text('Name', 'us spawn')],
               team=1, ai=True, name='us spawn'),
        entity(2, 'GameSharedResources.SoldierSpawnEntityData', 'soldier spawn', (20, 0, 0), team=2, ai=False),
        entity(3, 'GameSharedResources.SoldierSpawnEntityData', 'soldier spawn', (30, 0, 0)),
        entity(4, 'GameSharedResources.VehicleSpawnEntityData', 'vehicle spawn', (40, 0, 0), team=0, blueprint='vehicles/tank'),
        entity(5, 'GameSharedResources.MpObjectiveEntityData', 'objective', (50, 5, 0), [
            text('Spawner', '', G(1)), text('Self', '', G(5)), text('Elsewhere', '', 'other/partition/' + G(77)),
            text('Missing', '', G(99)), text('Null', '', 'null'), text('Long', long_value),
            dbx.Element('complex', dbx.CONTAINER, [('name', 'Inner')], children=[text('Deep', '', G(2)), text('Value', 'v')]),
            dbx.Element('field', 6, [('name', 'Enabled')], raw=bytes([1, 1]))], id='obj A'),
        entity(6, 'Entity.ReferenceObjectData', 'placed object', (60, 0, 0), destructible=True, blueprint='objects/house',
               blueprint_type='StaticModelEntityData'),
        entity(7, 'Entity.EffectEntityData', 'effect', (70, 0, 0)),
        entity(8, 'Entity.ReferenceObjectData', 'group member', (80, 0, 0), group='static group'),
        entity(9, 'Entity.SomethingData', 'other', None),
    ]
    tri = np.array([[[59, 0, -1], [61, 0, -1], [60, 2, 1]]], float)
    water = Water(np.array([[0, 2, 0], [100, 2, 0], [0, 2, 100], [-100, 3, 0]], np.float32), np.array([[0, 1, 2], [0, 2, 3]]),
                  ['sea', 'pond'])
    shapes = [levelmod.Shape(5, 'area', np.array([[0, 0, 0], [10, 0, 0], [10, 0, 10]], float), True, 4.0, 'Geometry'),
              levelmod.Shape(1, 'route', np.array([[0, 0, 0], [5, 0, 5]], float), False, 0.0, 'Route')]
    return levelmod.Level('TEST_LV', 'test_lv', 'opfor', entities=ents, meshes=[levelmod.Mesh(6, tri)],
                          terrain=(np.array([[0, 0, 0], [100, 0, 0], [0, 0, 100]], float), np.array([[0, 1, 2]])),
                          heightfield=cast(Any, SimpleNamespace(height=lambda x, z: 0.5)), water=cast(Any, water),
                          shapes=shapes)


class FakeViewer:
    """What the inspector uses of viewer.Viewer, without a window."""
    on_pick: Any                                # set by the code under test

    def __init__(self, scene=None, title='', panels=(), on_pick=None, size=None):
        self.scene, self.title, self.panels = scene, title, list(panels)
        self.hidden: set[int] = set()
        self.overlay: list = []
        self.focused: list = []
        self.camera = SimpleNamespace(focus=lambda p, d=None: self.focused.append((np.asarray(p).tolist(), d)), fly=False)
        self.select_on_click = True
        self.selected = viewer.NO_ID
        self.pick_point = self.hover_point = None
        self.hovered = viewer.NO_ID
        self.fps, self.renderer = 60.0, 'no renderer (test)'
        self.exit_after = None
        self.screenshot = None
        self.ran = False

    def layer_visible(self, i):
        return i not in self.hidden

    def set_layer_visible(self, i, on):
        (self.hidden.discard if on else self.hidden.add)(i)

    def set_overlay(self, segs):
        self.overlay = list(segs)

    def run(self):
        self.ran = True


@contextlib.contextmanager
def gui_frame():
    """One Dear ImGui frame without a window or renderer (the panels' widgets run; nothing is clicked)."""
    if imgui.get_current_context() is None:
        imgui.create_context()
        io_ = imgui.get_io()
        io_.set_ini_filename(None)                 # no imgui.ini written into the current folder
        io_.display_size = cast(Any, (1600, 900))
        io_.delta_time = 1 / 60
        io_.backend_flags |= imgui.BackendFlags_.renderer_has_textures.value
    imgui.new_frame()
    try:
        yield
    except Exception:
        imgui.destroy_context()                    # a panel that raised leaves its window open: a fresh context next time
        raise
    imgui.render()


def panels_run(ins, v, panels):
    bad = []
    for name in panels:
        try:
            with gui_frame():
                getattr(ins, name)(v)
        except Exception as e:
            bad.append(f'{name}: {type(e).__name__} {e}')
    return bad


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    saved = []

    def patch(obj, name, value):
        saved.append((obj, name, getattr(obj, name)))
        setattr(obj, name, value)
    try:
        lvl = fake_level()
        E = lvl.entities
        # --- pure helpers -----------------------------------------------------------------------------------
        check([inspector.layer_name(e) for e in E[1:5]] == ['soldier spawn, team 1', 'soldier spawn, team 2', 'soldier spawn, no team',
                                                            'vehicle spawn, neutral'] and inspector.layer_name(E[5]) == 'objective',
              'layer_name: spawns by team, others by category')
        check(inspector.parse_args(['inspector.py', 'mp_005', '--select', 'g', '--frames']) == ({'--select': 'g'}, ['mp_005', '--frames']),
              'parse_args: options with their values; a trailing option is positional')
        check(inspector.search_results(E, 'x') == [] and [e.index for e in inspector.search_results(E, 'x', 'SoldierSpawnEntityData')] == [1, 2, 3],
              'search: under 2 characters lists the filtered type, nothing without a filter')
        check([e.index for e in inspector.search_results(E, ' US SPAWN ')] == [1] and [e.index for e in inspector.search_results(E, 'objects/hou')] == [6]
              and [e.index for e in inspector.search_results(E, G(4))] == [4] and [e.index for e in inspector.search_results(E, 'obj a')] == [5],
              'search: name, blueprint, GUID and id, ignoring case and spaces around')
        check(len(inspector.search_results(E, 'data', limit=3)) == 3 and inspector.search_results(E, 'spawn', 'EffectEntityData') == [],
              'search: at most the limit; the type filter applies to matches too')
        check(inspector.measure_figures([np.zeros(3)]) is None, 'measure figures: none under two points')
        f = inspector.measure_figures([np.array([0.0, 0, 0]), np.array([3.0, 0, 4]), np.array([3.0, 5, 4])])
        check(f is not None and f['segment'] == 5.0 and f['horizontal'] == 0.0 and f['height'] == 5.0 and abs(f['slope'] - 90.0) < 1e-6
              and f['total'] == 10.0, 'measure figures: the last segment (straight up: 90 degrees) and the path total', str(f))

        # --- the scene ------------------------------------------------------------------------------------
        ins = inspector.Inspector(lvl)
        check([n for n, _c in ins.scene.layers] == [n for n, _c, _s in inspector.LAYERS], 'scene: one layer per LAYERS entry, in order')
        check(ins.counts == {'other': 2, 'soldier spawn, team 1': 1, 'soldier spawn, team 2': 1, 'soldier spawn, no team': 1,
                             'vehicle spawn, neutral': 1, 'objective': 1, 'placed object': 1, 'effect': 1, 'group member': 1},
              'counts per layer', str(ins.counts))
        check(ins.water_shaders == ['sea', 'pond'] and len(ins.scene.line_batches) == 4 + 3 + 1,
              'water: one id per shader; lines: facing lines of 4 spawns, an area\'s outline (bottom, top, posts) and a route',
              f'{ins.water_shaders} {len(ins.scene.line_batches)}')
        tags = np.concatenate([b[1] for b in ins.scene.batches]) & 0xFFFFFF
        check(6 in tags and 8 not in tags and 9 not in tags and 7 in tags and inspector.TERRAIN_ID in tags and inspector.WATER_ID + 1 in tags,
              'markers: a mesh where there is one, a box for an effect; none for a group member or a positionless entity')
        check({k: [(f_, e.index) for f_, e in v] for k, v in ins.referrers.items()} == {1: [('Spawner', 5)], 2: [('Inner/Deep', 5)]},
              'referrers: who refers to whom (not LevelData\'s list, not oneself, not an unknown or another partition\'s guid)',
              str(ins.referrers))
        check(ins.types[0] == ('SoldierSpawnEntityData', 3) and ins.types[1][1] == 2, 'types: most common first')

        # --- selection and links ----------------------------------------------------------------------------
        v = FakeViewer()
        ins.select(v, E[5])
        cols = [c for _a, _b, c in v.overlay]
        check(v.selected == 5 and cols == [inspector.REF_TO, inspector.REF_TO] and not v.focused,
              'select: the objective\'s links to the two positioned spawns it refers to (yellow)', str(cols))
        ins.select(v, E[1], focus=True)
        check(v.selected == 1 and [c for _a, _b, c in v.overlay] == [inspector.REF_FROM] and v.focused == [([10.0, 0.0, 0.0], 40.0)],
              'select with focus: a cyan line from what refers to it; the camera flies there')
        ins.select(v, E[0], focus=True)
        check(v.overlay == [] and len(v.focused) == 1, 'select an entity without a position: no lines, no flight')
        ins.select(v, None)
        check(v.selected == viewer.NO_ID and ins.selected is None, 'select nothing')
        ins.select(v, E[5])
        ins.follow(v, E[1])
        check(ins.selected is E[1] and ins.history == [E[5]] and v.focused[-1][0] == [10.0, 0.0, 0.0], 'follow a reference: remembered for < back')

        # --- picking and measuring ----------------------------------------------------------------------------
        pick = ins.on_pick(v)
        pick(3)
        check(ins.selected is E[3], 'a click on an object selects it')
        pick(inspector.TERRAIN_ID)
        check(ins.selected is None, 'a click on the terrain (an id past the entities) clears the selection')
        pick(None)
        check(ins.selected is None, 'a click on the sky clears the selection')
        ins.set_measuring(v, True)
        check(not v.select_on_click, 'measuring: clicks stop selecting')
        for p in ([0, 0, 0], [3, 0, 4], None, [3, 5, 4]):
            v.pick_point = None if p is None else np.array(p, float)
            pick(inspector.TERRAIN_ID)
        check(len(ins.measure) == 3 and len(v.overlay) == 2 + 3 * 3 and all(c == inspector.MEASURE for _a, _b, c in v.overlay),
              'measuring: a point per click on something (none on the sky), segments between them and a cross at each')
        ins.set_measuring(v, False)
        check(v.select_on_click, 'measuring off: clicks select again')
        w1, w2 = ins.water_above(np.array([10.0, 0.5, 10.0])), ins.water_above(np.array([-10.0, 0.0, 1.0]))
        check(w1 is not None and abs(w1 - 1.5) < 1e-6 and w2 is not None and abs(w2 - 2.1) < 1e-6,
              'water_above: the surface height over a point, from the triangle it is in (a sloped one too)', f'{w1} {w2}')
        check(ins.water_above(np.array([500.0, 0.0, 500.0])) is None, 'water_above: no water over the point - None')
        dry = inspector.Inspector(levelmod.Level('DRY', 'dry', 'opfor', entities=E[:2]))
        check(dry.water_above(np.zeros(3)) is None and dry.water_shaders == [], 'water_above: a level without water - None')

        # --- panels -------------------------------------------------------------------------------------------
        all_panels = ['panel_layers', 'panel_selection', 'panel_measure', 'panel_status']
        ins.scene_triangles = dry.scene_triangles = ins.scene.triangle_count()
        bad = []
        for sel, hist, hov, measuring, pts in ((None, [], viewer.NO_ID, False, []),
                                               (E[5], [E[1]], 5, True, []),
                                               (E[1], [], inspector.TERRAIN_ID, True, [np.array([10.0, 1.0, 10.0])]),
                                               (E[6], [], inspector.WATER_ID + 1, False, [np.zeros(3), np.array([3.0, 0, 4])]),
                                               (E[0], [], 9999, False, []), (E[9], [], 2, False, [])):
            ins.selected, ins.history, v.hovered, ins.measuring, ins.measure = sel, list(hist), hov, measuring, pts
            v.hover_point = np.array([1.0, 2.0, 3.0]) if hov != viewer.NO_ID else None
            bad += panels_run(ins, v, all_panels)
        dry.measure = [np.zeros(3)]
        bad += panels_run(dry, v, all_panels)
        check(not bad, 'every panel draws (layers and search, selection with its property tree, measure, status and tooltips)', '; '.join(bad[:3]))

        # --- main() with a stand-in viewer ----------------------------------------------------------------------
        made: list[FakeViewer] = []

        def make_viewer(*a, **k):
            made.append(FakeViewer(*a, **k))
            return made[-1]
        patch(inspector.viewer, 'Viewer', make_viewer)
        patch(inspector.levelmod, 'load', lambda name: lvl)
        code = inspector.main(['inspector.py', 'test_lv', '--select', G(5).upper(), '--measure', '0,0,0;3,0,4', '--screenshot', 'unused.png',
                               '--frames', '2'])
        v = made[-1]
        hidden = {n for n, i in ins.layer_index.items() if i in v.hidden}
        check(code == 0 and v.ran and v.exit_after == 2 and hidden == inspector.HIDDEN_AT_START and len(v.panels) == 4 and v.on_pick is not None,
              'main: the window built and run, effects / area points / other hidden at start', str(hidden))
        check(v.selected == 5 and v.focused == [([50.0, 5.0, 0.0], 40.0)] and not v.select_on_click and sum(c == inspector.MEASURE for _a, _b, c in v.overlay) == 1 + 6,
              'main --select (any case) and --measure: the object selected and flown to, the measure points drawn')
        made.clear()
        inspector.main(['inspector.py', 'test_lv', '--select', G(404)])
        check(made[-1].selected == viewer.NO_ID and made[-1].select_on_click, 'main --select of an unknown guid: nothing selected')
        from bfbc2core import pick as pickmod
        patch(pickmod, 'pick_level', lambda title: None)
        check(inspector.main(['inspector.py']) == 1, 'main without a level: the picker, cancelled - exit code 1')
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            patch(pickmod, 'pick_level', lambda title: 'test_lv')
            check(inspector.main(['inspector.py']) == 0 and made[-1].title.endswith('TEST_LV'), 'main without a level: the picked one opens')
    finally:
        for obj, name, value in reversed(saved):
            setattr(obj, name, value)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
