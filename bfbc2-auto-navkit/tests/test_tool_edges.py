r"""Edge cases of NavKit's smaller tools, on data built here (no game data):

- ordercheck / navcompare on path data without route nodes (an empty or failed build): no group, 0 points on the
  main network - not a crash;
- navkit's detailed-sector cache (work\navkit\<lv>\detailed.json): reused while the samples and the sector offsets
  are the same, chosen again when the offsets change (a new --box);
- navdiff's sector pictures (pngs): one PNG per sector, of the expected size;
- the GUI's reader thread: output and the result through the queue only; Cancel ends the step's process tree.

  python test_tool_edges.py
"""
import os
import shutil
import struct
import sys
import tempfile
from collections import Counter
from types import SimpleNamespace

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 'tools'))
sys.path.insert(0, os.path.join(HERE, '..'))
import navcompare  # noqa: E402
import navdiff  # noqa: E402
import navkit  # noqa: E402
import ordercheck  # noqa: E402


def empty_path_data():
    """An ordercheck.PathData as its constructor leaves it for a file whose sectors have no route nodes."""
    pd = ordercheck.PathData.__new__(ordercheck.PathData)
    pd.areas, pd.nodes, pd.group, pd.group_size = [], [], {}, Counter()
    pd.pos, pd.ids = np.array([]), []
    return pd


def png_size(path):
    with open(path, 'rb') as f:
        head = f.read(24)
    return struct.unpack('>II', head[16:24]) if head[:8] == b'\x89PNG\r\n\x1a\n' else None


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    # ordercheck / navcompare without route nodes
    pd = empty_path_data()
    try:
        got = pd.nearest_group((1.0, 2.0, 3.0))
    except (ValueError, IndexError) as e:
        got = e
    check(got == (None, None), 'ordercheck: no route nodes - no nearest group (it raised)', repr(got))
    try:
        n = navcompare.main_network(pd, [('AreaOrder', 'a', (1.0, 2.0, 3.0))])
    except (ValueError, IndexError) as e:
        n = e
    check(n == 0, 'navcompare: no route nodes - 0 points on the main network (it raised)', repr(n))

    tmp = tempfile.mkdtemp(prefix='navkit_edges_')
    try:
        # the detailed-sector cache
        samples = os.path.join(tmp, 'samples08.bin')
        open(samples, 'wb').write(b'x')
        settings = {'xoff': 0, 'zoff': 0}
        calls = []
        pr = SimpleNamespace(samples08=lambda lv: samples, out_dir=lambda lv: tmp, work=tmp,
                             level_work=lambda lv: tmp)
        import detailed
        saved = navkit.ensure_settings, detailed.choose
        navkit.ensure_settings = lambda pr_, lv: dict(settings)
        detailed.choose = lambda *a: (calls.append(a[3:5]) or [[len(calls), 0]], ['why'])
        try:
            first = navkit.detailed_sectors(pr, 'lv')
            again = navkit.detailed_sectors(pr, 'lv')
            settings.update(xoff=-3, zoff=2)
            moved = navkit.detailed_sectors(pr, 'lv')
        finally:
            navkit.ensure_settings, detailed.choose = saved
        check(first == again == [(1, 0)] and len(calls) >= 1, 'detailed sectors: cached while samples and offsets are the same')
        check(moved == [(2, 0)] and calls[-1] == (-3, 2),
              'detailed sectors: chosen again when the sector offsets change (a new --box)', f'{moved} {calls}')

        # navdiff's pictures
        cells = [(i, j) for i in range(0, 125, 5) for j in range(0, 125, 5)]
        R = {0x0102: SimpleNamespace(cells=lambda: [cells], mesh=SimpleNamespace(pitch=0.2))}
        rows = [{'id': 0x0102, 'only_ref': [(1, 1), (2, 2)], 'only_cand': [(3, 3)]}]
        out = os.path.join(tmp, 'png')
        navdiff.pngs(rows, R, {}, out, top=5)
        p = os.path.join(out, 'sector_0102.png')
        size = png_size(p) if os.path.isfile(p) else None
        check(size is not None and 400 <= size[0] <= 1200 and 400 <= size[1] <= 1200,
              'navdiff: one PNG per sector, of a plausible size (8 x 8 inches at 110 dpi, trimmed)', str(size))

        # navbuild's settings: a value passed through the build (no module settings rewritten)
        import dataclasses
        import navbuild
        d = navbuild.DEFAULTS
        check((d.pitch, d.sector_samples, d.window, d.tile, d.tiles, d.resolution, d.link_step, d.kind) ==
              (0.8, 125, range(-1, 127), 50, 3, 0.025, 0.6, 'normal'),
              'navbuild: the default settings are the normal pitch-0.8 grid', str(d))
        dd = d.detailed()
        check((dd.pitch, dd.sector_samples, dd.window, dd.tile, dd.tiles, dd.resolution, dd.link_step, dd.kind) ==
              (0.2, 500, range(-1, 502), 100, 6, 0.05, navbuild.DETAILED_LINK_STEP, 'detailed') and
              dataclasses.replace(dd, **navbuild.grid_settings(0.8)) == d,
              'navbuild: the detailed variant is the pitch-0.2 grid (the old set_pitch(0.2)), the rest unchanged', str(dd))
        nd = dataclasses.replace(d, detail_features=False)
        check(not nd.detail_features and d.detail_features and navbuild.DEFAULTS == navbuild.BuildSettings(),
              'navbuild: replace gives a changed copy, the defaults stay')
        try:
            d.__setattr__('pitch', 0.2)
            frozen = False
        except dataclasses.FrozenInstanceError:
            frozen = True
        check(frozen and not hasattr(navbuild, 'set_pitch') and not hasattr(navbuild, 'saved_settings'),
              'navbuild: the settings are frozen; set_pitch / saved_settings are gone')
        # the detailed pass gets the detailed settings; a stage that fails leaves nothing behind
        a = {'sectors': {(0, 0)}, 'border_nodes': {}, 'next_id': 0,
             'level': SimpleNamespace(s=SimpleNamespace(min_x=0.0, min_z=0.0, nx=1, pitch=0.8), keep=np.zeros(0, bool),
                                      col_of=np.zeros(0, int), y=np.zeros(0))}
        real_stage = navbuild.stage
        got = {}

        def failing_stage(*a_, **k_):
            got.update(k_)
            raise RuntimeError('the detailed stage failed')
        navbuild.stage = failing_stage
        try:
            navbuild.merge_detailed(a, {'sectors': {(0, 0)}, 'samples': 'none'}, 0, 0, [], {}, lambda *m: None, nd)
        except RuntimeError:
            pass
        finally:
            navbuild.stage = real_stage
        check(got.get('cfg') == nd.detailed() and not got['cfg'].detail_features and navbuild.DEFAULTS.pitch == 0.8,
              'navbuild: merge_detailed runs stage b with cfg.detailed() (the other settings kept)',
              str(got.get('cfg')))
        navbuild.ORDER_STATS['added'] = 3
        navbuild.navwalk.STEEP_STATS['lips'] = 2
        navbuild.reset_stats()
        check(not navbuild.ORDER_STATS and not navbuild.navwalk.STEEP_STATS and not hasattr(navbuild, 'REPAIR_STATS'),
              'navbuild: reset_stats clears every per-build counter (REPAIR_STATS, never read, is gone)')
        # navkit.py --no-detail: build / apply get settings with detail features off; navbuild's defaults untouched
        seen = []
        real_project, real_build, real_apply = navkit.Project, navkit.build, navkit.apply

        class FakeProject:
            def __init__(self, root=None):
                pass

            def level(self, name):
                return name
        navkit.Project = FakeProject
        navkit.build = lambda pr, lv, log=print, cfg=None: seen.append(('build', cfg))
        navkit.apply = lambda pr, lv, *a_, cfg=None, **k_: seen.append(('apply', cfg))
        try:
            navkit.main(['navkit.py', 'build', 'lv', '--no-detail'])
            navkit.main(['navkit.py', 'apply', 'lv', '--no-detail'])
            navkit.main(['navkit.py', 'build', 'lv'])
        finally:
            navkit.Project, navkit.build, navkit.apply = real_project, real_build, real_apply
        check([(w, c is not None and not c.detail_features) for w, c in seen[:2]] == [('build', True), ('apply', True)]
              and seen[2] == ('build', None) and navbuild.DEFAULTS.detail_features,
              'navkit --no-detail: build / apply get BuildSettings with detail features off, the defaults stay on',
              str(seen))

        # the GUI's reader thread hands everything over through the queue; Cancel ends the step and its children
        from importlib.machinery import SourceFileLoader
        gui = SourceFileLoader('navkitGUI', os.path.join(HERE, '..', 'navkitGUI.pyw')).load_module()
        import queue
        import subprocess
        app = gui.App.__new__(gui.App)
        app.lines = queue.Queue()
        proc = subprocess.Popen([sys.executable, '-c', 'print("one"); print("two")'], stdout=subprocess.PIPE, text=True)
        app.reader(proc, 'check')
        items = []
        while not app.lines.empty():
            items.append(app.lines.get())
        check([i.strip() for i in items[:2]] == ['one', 'two'] and items[-1] == ('done', 'check', 0, ['one', 'two']) and not hasattr(app, 'tail'),
              'GUI reader: lines, then (done, step, exit code, last lines) through the queue - no shared attributes', str(items))
        parent = subprocess.Popen([sys.executable, '-c', 'import subprocess, sys, time; c = subprocess.Popen([sys.executable, '
                                   '"-c", "import time; time.sleep(60)"]); print(c.pid, flush=True); time.sleep(60)'],
                                  stdout=subprocess.PIPE, text=True)
        child = int(parent.stdout.readline())
        gui.kill_tree(parent)
        parent.wait(10)
        import time
        time.sleep(0.5)
        alive = subprocess.run(['tasklist', '/FI', f'PID eq {child}'], capture_output=True, text=True).stdout if os.name == 'nt' else ''
        check(parent.poll() is not None and str(child) not in alive, 'GUI Cancel: the step and the processes it started end')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
