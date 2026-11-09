r"""The Navigation Studio's edit mode without the game: the editor (editing.py) driven as clicks would drive it on a
stand-in studio and a stand-in base file - every tool, an outline finished with Enter, a box, two-point tools, a
note without text refused, undo / redo, save and reload, the click mode, a DICE base refusing region edits, and
the drawn segments of each kind of edit. Also overlay.py's usage when neither --reference nor --anomalies is given.
test_editing.py runs the same on a real level (game tier).

  python test_editing_offline.py
"""
import contextlib
import io
import os
import shutil
import sys
import tempfile
from types import SimpleNamespace

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
STUDIO = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navigation-studio')
import editing  # noqa: E402
from bfbc2core import navtune as nt  # noqa: E402


class FakeViewer:
    def __init__(self):
        self.select_on_click = True
        self.overlay = []


class FakeStudio:
    """What the editor uses of the studio: its files, the click mode and the overlay."""
    def __init__(self, files):
        self.files = files
        self.editor = None

    def sync_click_mode(self, v):
        v.select_on_click = not (self.editor is not None and self.editor.active())

    def refresh_overlay(self, v):
        v.overlay = self.editor.segments() if self.editor is not None else []


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    tmp = tempfile.mkdtemp(prefix='editing_offline_')
    try:
        base = os.path.join(tmp, 'pathdata.hpd')
        open(base, 'wb').write(b'stand-in base file')
        st = FakeStudio({'NavKit': SimpleNamespace(path=base)})
        v = FakeViewer()
        ed = editing.Editor(st, tmp, 'test_level', 'Levels/TEST_LEVEL')
        st.editor = ed
        ed.path = os.path.join(tmp, 'test_level.navtune.json')
        check(ed.target == 'NavKit' and ed.tune.kind == 'navkit' and not ed.tune.operations,
              'a new side file on NavKit\'s build (a NavKit base)')
        p0 = np.array([100.0, 20.0, 200.0])
        pts = [p0 + d for d in ([0, 0, 0], [10, 0, 0], [10, 1, 10], [0, 1, 10])]

        ed.set_open(v, True)
        ed.tool = 'unwalkable'
        st.sync_click_mode(v)
        check(not v.select_on_click, 'with a tool on, clicks stop selecting')
        for p in pts[:2]:
            ed.click(v, p)
        ed.finish_polygon(v)
        check(not ed.tune.operations and len(ed.clicks) == 2, 'an outline of two clicks is not finished yet')
        for p in pts[2:]:
            ed.click(v, p)
        check(len(ed.segments()) > 4, 'the drawing in progress is drawn')
        ed.finish_polygon(v)
        o = ed.tune.operations[-1]
        lo, hi = o['region']['y']
        check(o['op'] == 'unwalkable' and len(o['region']['polygon']) == 4 and abs(lo - 18.0) < 1e-6 and abs(hi - 25.0) < 1e-6,
              'an outline of 4 clicks: an unwalkable region, 2 m below and 4 m above the clicks', str(o['region']))
        ed.tool, ed.shape = 'walkable', 'box'
        ed.click(v, pts[0])
        ed.click(v, pts[2])
        check(ed.tune.operations[-1]['op'] == 'walkable' and 'box' in ed.tune.operations[-1]['region'], 'two corners make a walkable box')
        ed.shape = 'polygon'
        for tool in ('seed', 'unseed'):
            ed.tool = tool
            ed.click(v, pts[1])
        ed.tool, ed.note_text = 'note', ''
        n = len(ed.tune.operations)
        ed.click(v, pts[1])
        check(len(ed.tune.operations) == n and ed.message == 'type the note first', 'a note without text is not added')
        ed.note_text = 'check the stairs'
        ed.click(v, pts[1])
        for tool in ('link-areas', 'route-edge add', 'route-edge remove'):
            ed.tool = tool
            ed.click(v, pts[0])
            ed.click(v, pts[3])
        ed.tool, ed.trav_kind, ed.two_way = 'traversal', 0, False
        ed.click(v, pts[0])
        ed.click(v, pts[0] + [0, 6, 0])
        names = [o['op'] + (' ' + o['action'] if 'action' in o else '') for o in ed.tune.operations]
        check(names == ['unwalkable', 'walkable', 'seed', 'unseed', 'note', 'link-areas', 'route-edge add', 'route-edge remove',
                        'traversal'], 'every tool adds its operation', str(names))
        t = ed.tune.operations[-1]
        check(t['kind'] == 'ladder' and t['two-way'] is False and t['top'][1] - t['bottom'][1] == 6.0, 'the traversal: a one-way ladder, 6 m')
        counts = [len(editing.op_segments(o)) for o in ed.tune.operations]
        check(counts[0] == 12 and counts[1] == 12 and counts[2] == 3 and counts[-1] == 5 and len(v.overlay) == sum(counts),
              'segments per edit: a 4-point region 12, a box 12, a seed 3, a traversal 5; all drawn', str(counts))
        removed = [s for s in editing.op_segments(ed.tune.operations[7]) if s[2] == editing.COLOURS['route-edge remove']]
        check(len(removed) == 3, 'a removed route edge is drawn in its own colour')
        ed.undo(v)
        ed.undo(v)
        check(len(ed.tune.operations) == 7 and ed.message.startswith('undone'), 'undo twice')
        ed.redo(v)
        check(len(ed.tune.operations) == 8, 'redo once')
        ed.tune.save(ed.path)
        check(nt.load(ed.path).operations == ed.tune.operations, 'saved and loaded back unchanged')
        again = editing.Editor(st, tmp, 'test_level', 'Levels/TEST_LEVEL')
        check(again.path == nt.default_path(tmp, 'test_level'), 'the side file\'s default place (navtune.default_path)')
        ed.set_open(v, False)
        check(v.select_on_click and ed.tool is None and not ed.clicks, 'closing the panel gives clicks back to selecting')
        check(ed.job_state() is None, 'no apply job before the button is pressed')

        dice_st = FakeStudio({'DICE': SimpleNamespace(path=base)})
        dice = editing.Editor(dice_st, os.path.join(tmp, 'other'), 'test_level', 'Levels/TEST_LEVEL')
        dice_st.editor = dice
        dice.open, dice.tool = True, 'seed'
        dice.click(v, pts[0])
        check(dice.target == 'DICE' and not dice.tune.operations and 'DICE base' in dice.message,
              'on a DICE base, region edits are refused', dice.message)
        dice.tool = 'link-areas'
        dice.click(v, pts[0])
        dice.click(v, pts[3])
        check(len(dice.tune.operations) == 1, 'and links are taken')

        # overlay.py: neither mode named -> the usage (it was a KeyError on --reference)
        import overlay
        buf = io.StringIO()
        try:
            with contextlib.redirect_stdout(buf):
                code = overlay.main(['overlay.py', 'sp_009'])
        except KeyError as e:
            code = f'KeyError {e}'
        check(code == 1 and 'overlay.py <target level>' in buf.getvalue(), 'overlay: no --reference or --anomalies prints the usage',
              str(code))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
