r"""Edge cases of the core package, without game data (quick tier); regression tests for the round-3 audit:

- terrain.find and water.read find a level's files whatever the case of the name given (an upper-case name found
  nothing);
- errors: a GUI tool's error raised in a worker thread is logged and printed but opens no Tk window from that
  thread (Tk is not thread-safe; it could hang the tool).

  python core\tests\test_core_edges.py
"""
import os
import shutil
import struct
import sys
import tempfile
import threading
import types
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import errors, terrain, water  # noqa: E402


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    tmp = tempfile.mkdtemp(prefix='core_edges_')
    try:
        d = os.path.join(tmp, 'terrain', 'Levels', 'MP_005')
        os.makedirs(d)
        open(os.path.join(d, 'mp_005.Heightfield-000.res'), 'wb').write(b'x')
        mesh = struct.pack('<I', 1) + struct.pack('<I', 3) + struct.pack('<9f', 0, 0, 0, 1, 0, 0, 0, 0, 1) + \
            struct.pack('<I', 3) + struct.pack('<3I', 0, 1, 2)
        open(os.path.join(d, 'mp_005.Water.Mesh.res'), 'wb').write(mesh)
        check(terrain.find(tmp, 'MP_005') is not None and terrain.find(tmp, 'mp_005') is not None,
              'terrain.find: the level name in any case')
        w = water.read(tmp, 'MP_005')
        check(w is not None and len(w.triangles) == 1, 'water.read: the terrain name in any case')

        calls: list[str] = []
        fake: Any = types.ModuleType('tkinter')
        fake._default_root = None

        def fake_tk() -> types.SimpleNamespace:
            calls.append('Tk')
            return types.SimpleNamespace(withdraw=lambda: None, destroy=lambda: None)
        fake.Tk = fake_tk
        fake.messagebox = types.SimpleNamespace(showerror=lambda *a, **k: calls.append('showerror'),
                                                showwarning=lambda *a, **k: calls.append('showwarning'))
        saved = sys.modules.get('tkinter'), sys.modules.get('tkinter.messagebox')
        sys.modules['tkinter'], sys.modules['tkinter.messagebox'] = fake, fake.messagebox
        try:
            t = threading.Thread(target=errors._window, args=('t', 'from a worker'))
            t.start()
            t.join()
            in_worker = list(calls)
            errors._window('t', 'from the main thread')
        finally:
            for name, mod in zip(('tkinter', 'tkinter.messagebox'), saved):
                if mod is None:
                    sys.modules.pop(name, None)
                else:
                    sys.modules[name] = mod
        check(in_worker == [] and 'showerror' in calls,
              'errors: no Tk window from a worker thread; the main thread still shows one', str(calls))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
