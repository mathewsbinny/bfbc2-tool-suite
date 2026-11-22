r"""The suite's window (BFBC2 Tool Suite.pyw) driven as clicks would drive it: it opens with its 8 tabs and the
game copy's levels; FbrbTool lists a bundle, bfbc2.exe leveldiff compares a level with itself, the crash reader reads a
report built here, the test runner lists a tier - each through its tab's button, its output in the pane. Needs a
display and Project OPFOR BFBC2's game copy. (The Python 3D tools' level picker, pick.py, was retired in round RT3.)

  python tests\test_hub.py [--shots <folder>]       --shots: a screenshot of every tab (for a look)
"""
import atexit
import importlib.util
import os
import shutil
import sys
import tempfile
import time
import types

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')


def load_hub() -> types.ModuleType:
    spec = importlib.util.spec_from_file_location('suite_hub', os.path.join(SUITE, 'BFBC2 Tool Suite.pyw'))
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def main(argv: list[str]) -> int:
    import tkinter as tk
    hubmod = load_hub()
    failed = 0

    def check(ok: object, what: str) -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)

    hubmod.dpi_aware()
    root = tk.Tk()
    root.geometry('1100x760+20+20')
    hub = hubmod.Hub(root)
    root.update()

    def wait(timeout: float = 300) -> None:
        t0 = time.time()
        while time.time() - t0 < timeout:
            root.update()
            if hub.proc is None or hub.proc.poll() is not None:
                for _ in range(10):                      # let the last lines and the 'done' line arrive
                    root.update()
                    time.sleep(0.05)
                return
            time.sleep(0.05)

    def output() -> str:
        return hub.text.get('1.0', 'end')

    tabs = [hub.tabs.tab(t, 'text') for t in hub.tabs.tabs()]
    check(tabs == ['Levels', 'Bundles', 'Level diff', 'Crash reader', 'Xbox files', 'Tests', 'Mods', 'More tools'],
          f'8 tabs: {", ".join(tabs)}')
    levels = hub.tree.get_children()
    check(len(levels) > 30 and 'sp_01' in levels and 'mp_005' in levels, f'the Levels tab lists the game copy\'s levels ({len(levels)})')
    check(hub.tree.set('sp_01', 'dice') == 'yes' and hub.tree.set('sp_01', 'prepared') == 'yes', 'with what each has (sp_01: prepared, DICE path data)')

    if '--shots' in argv:
        from PIL import ImageGrab
        folder = argv[argv.index('--shots') + 1]
        os.makedirs(folder, exist_ok=True)
        for k, t in enumerate(hub.tabs.tabs()):
            hub.tabs.select(t)
            for _ in range(5):
                root.update()
                time.sleep(0.05)
            x, y = root.winfo_rootx(), root.winfo_rooty()
            ImageGrab.grab((x, y, x + root.winfo_width(), y + root.winfo_height())).save(os.path.join(folder, f'hub_{k}.png'))

    bundle = os.path.join(hub.pr.game, 'Dist', 'win32', 'levels', 'sp_009', 'terrain-00.fbrb')
    hub.fb_file.set(bundle)
    hub.tabs.select(1)
    hub.run([hubmod.PYTHON, hubmod.SCRIPTS['fbrb'], 'list', hub.fb_file.get()], 'FbrbTool list')
    wait()
    check('entries, payload' in output() and 'Heightfield' in output(), 'Bundles: FbrbTool lists a terrain bundle')

    lvl = os.path.join(hub.pr.work, 'levels', 'mp_005', 'dist', 'levels', 'MP_005.dbx')
    hub.ld_a.set(lvl)
    hub.ld_b.set(lvl)
    hub.ld_summary.set(True)
    hub.diff()
    wait()
    check('changed: 0; only in A: 0; only in B: 0' in output(), 'Level diff: a level against itself has no differences')

    tmp = tempfile.mkdtemp(prefix='hub_test_')
    atexit.register(shutil.rmtree, tmp, True)           # deleted at the end (round D1)
    rep = os.path.join(tmp, 'crashreport.xml')
    open(rep, 'w').write('<?xml version="1.0"?><report><type>crash</type><buildsignature>795745</buildsignature>'
                         '<categoryid> 0x6EFF8A</categoryid><stack> 0x6EFF8A 0x570330</stack>'
                         '<contextdata>registers: Eax 00000000, Eip 006eff8a, Esp 001ae65c</contextdata></report>')
    hub.cr_file.set(rep)
    hub.cr_nocode.set(True)
    hub.crash()
    wait()
    check('Md5Buffer' in output() and 'DbxEdit check' in output(), 'Crash reader: names the crash site and gives the hint')

    hub.t_tier.set('quick')
    hub.run([hubmod.PYTHON, hubmod.SCRIPTS['tests'], 'quick', '--list'], 'Tests: list')
    wait()
    check('repository rules' in output(), 'Tests: lists what the quick tier runs')

    hub.tree.selection_set('sp_009')
    check(hub.chosen_level() == 'sp_009', 'a level chosen in the list is the one the 3D buttons open')

    # a tool that fails: the user is told, with what it said last; a stopped one is not a failure
    told = []
    hub.notify = lambda kind, title, text: told.append((kind, text))
    hub.run([hubmod.PYTHON, '-c', 'print("checking the input"); print("error: the bundle is damaged"); raise SystemExit(3)'],
            'A failing tool')
    wait()
    check(told and told[-1][0] == 'warning' and 'exit code 3' in told[-1][1] and 'the bundle is damaged' in told[-1][1],
          'a tool that fails: a warning with its exit code and its last lines')
    n = len(told)
    hub.run([hubmod.PYTHON, '-c', 'import time; time.sleep(30)'], 'A long tool')
    root.update()
    time.sleep(0.5)
    hub.stop()
    wait()
    check(len(told) == n, 'a tool the user stops: no failure warning')
    check(root.report_callback_exception is not None, 'errors in the window\'s own buttons are caught and reported')
    root.destroy()

    check(not hasattr(hub, 'open_3d') and hasattr(hub, 'open_navigation'),
          'Levels: the navigation opens in the map editor (the Navigation Studio was retired in round GR2)')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
