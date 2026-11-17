r"""NavKit's tools that read collision through bfbc2core.collision (detailed.flagged_sectors / choose, ladders.extract,
navsample_offline.level_geometry, navwater_mesh.object_triangles) give the same results on the Rust core and in
Python (NavKit's levelobjects functions), bit for bit (game tier; needs the levels prepared and their 0.8 m samples).
Read only: nothing is written (object_triangles without its cache).

  python test_collision_backends.py [--opfor <folder>] [level ...]      default: sp_009 sp_004_b

Skipped (exit 0) when the Rust core is not built or the OPFOR folder is not there.
"""
import os
import sys
import time
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import detailed  # noqa: E402
import ladders  # noqa: E402
import navregress  # noqa: E402
import navsample_offline  # noqa: E402
import navwater_mesh  # noqa: E402
from bfbc2core import collision, dbx, paths  # noqa: E402

DEFAULT = ['sp_009', 'sp_004_b']


def canon(x: Any) -> Any:
    """x with arrays as (shape, dtype, bytes) and objects as their fields, order kept (bit for bit comparisons)."""
    if isinstance(x, np.ndarray):
        return ('array', x.shape, x.dtype.str, x.tobytes())
    if isinstance(x, np.generic):
        return ('scalar', x.dtype.str, x.tobytes())
    if isinstance(x, dict):
        return [(canon(k), canon(v)) for k, v in x.items()]
    if isinstance(x, (list, tuple)):
        return [canon(v) for v in x]
    if isinstance(x, set):
        return sorted(canon(v) for v in x)
    if hasattr(x, '__dict__'):
        return (type(x).__name__, canon(vars(x)))
    return x


def outcomes(root: str, lv: str) -> dict[str, Any]:
    L = os.path.join(root, 'work', 'levels', lv)
    common = [os.path.join(root, 'work', 'navkit', 'common', c) for c in ('mp_common', 'sp_common')]
    samples, xoff, zoff, _f = navregress.inputs(root, lv)
    quiet = lambda *a: None  # noqa: E731
    return {
        'detailed sectors': canon(detailed.choose(L, common, samples, xoff, zoff)),
        'ladders': canon(ladders.extract(L, common)),
        'offline sampler geometry': canon(navsample_offline.level_geometry(L, navsample_offline.common_bases(root),
                                                                           log=quiet)),
        'boat mesh object triangles': canon(navwater_mesh.object_triangles(root, lv, log=quiet)),
    }


def main(argv: list[str]) -> int:
    if collision._with_collision(dbx._import_native()) is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    args = argv[1:]
    opfor = None
    if '--opfor' in args:
        k = args.index('--opfor')
        opfor = args[k + 1] if k + 1 < len(args) else None
        del args[k:k + 2]
    try:
        root = paths.find_opfor(opfor, start=_ROOT)
    except Exception:
        print('skipped: Project OPFOR BFBC2 not found')
        return 0
    levels = [a.lower() for a in args]
    failed = 0
    for lv in levels or DEFAULT:
        try:
            navregress.inputs(root, lv)
        except RuntimeError as e:
            print(f'skip {lv}: {e}')
            continue
        times = []
        results = []
        for native in (False, True):
            collision.use_native(native)
            t = time.perf_counter()
            results.append(outcomes(root, lv))
            times.append(time.perf_counter() - t)
        collision.use_native(True)
        py, rs = results
        for what in py:
            ok = py[what] == rs[what]
            failed += not ok
            print(f'{"ok  " if ok else "FAIL"} {lv}: {what} the same in both')
        print(f'     {lv}: Python {times[0]:.1f} s, Rust core {times[1]:.1f} s')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
