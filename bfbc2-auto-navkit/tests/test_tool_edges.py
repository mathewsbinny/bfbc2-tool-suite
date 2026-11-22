r"""Edge cases of NavKit's smaller tools, on data built here (no game data):

- ordercheck / navcompare on path data without route nodes (an empty or failed build): no group, 0 points on the
  main network - not a crash;
- navkit's detailed-sector cache (work\navkit\<lv>\detailed.json): reused while the samples and the sector offsets
  are the same, chosen again when the offsets change (a new --box);
- navdiff's sector pictures (pngs): one PNG per sector, of the expected size;
- navkit.py --no-detail: build / apply get the build setting detail_features off (the Rust defaults otherwise);
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
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit', 'navkit-tools')
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
        calls: list[tuple[Any, ...]] = []
        pr: Any = SimpleNamespace(samples08=lambda lv: samples, out_dir=lambda lv: tmp, work=tmp,   # a Project stand-in
                                  level_work=lambda lv: tmp)
        import detailed
        saved = navkit.ensure_settings, detailed.choose
        nk: Any = navkit                    # monkeypatched below (the stand-ins are not typed as the real ones)

        def choose(*a: Any) -> tuple[list[list[int]], list[str]]:
            calls.append(a[3:5])
            return [[len(calls), 0]], ['why']
        nk.ensure_settings = lambda pr_, lv: dict(settings)
        detailed.choose = choose
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

        # navkit.py --no-detail: build / apply get the setting detail_features off (the build's settings are the
        # Rust defaults since round NK5-rest: rust/bc2-navbuild/src/stage_tests.rs checks them)        # navkit.py --no-detail: build / apply get settings with detail features off; navbuild's defaults untouched
        seen = []
        real_project, real_build, real_apply = navkit.Project, navkit.build, navkit.apply

        class FakeProject:
            def __init__(self, root=None):
                pass

            def level(self, name):
                return name
        nk.Project = FakeProject
        nk.build = lambda pr, lv, log=print, cfg=None: seen.append(('build', cfg))
        nk.apply = lambda pr, lv, *a_, cfg=None, **k_: seen.append(('apply', cfg))
        try:
            navkit.main(['navkit.py', 'build', 'lv', '--no-detail'])
            navkit.main(['navkit.py', 'apply', 'lv', '--no-detail'])
            navkit.main(['navkit.py', 'build', 'lv'])
        finally:
            nk.Project, nk.build, nk.apply = real_project, real_build, real_apply
        check([(w, c == {'detail_features': False}) for w, c in seen[:2]] == [('build', True), ('apply', True)]
              and seen[2] == ('build', None),
              'navkit --no-detail: build / apply get the setting detail_features off, otherwise the Rust defaults',
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
        assert parent.stdout is not None
        child = int(parent.stdout.readline())
        gui.kill_tree(parent)
        parent.wait(10)
        import time
        # the child's end is waited for (up to 15 s), not assumed after half a second: on a busy PC (tests side by
        # side, run_tests.py -j) it can take longer to go (2026-11-17)
        alive = ''
        for _ in range(60):
            time.sleep(0.25)
            alive = subprocess.run(['tasklist', '/FI', f'PID eq {child}'], capture_output=True, text=True).stdout if os.name == 'nt' else ''
            if str(child) not in alive:
                break
        check(parent.poll() is not None and str(child) not in alive, 'GUI Cancel: the step and the processes it started end')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
