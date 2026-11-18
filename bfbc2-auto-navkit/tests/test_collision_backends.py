r"""NavKit's tools that read collision through bfbc2core.collision (detailed.flagged_sectors / choose, ladders.extract,
navsample_offline.level_geometry, navwater_mesh.object_triangles) give the results they gave with collision.py's
retired Python path (NavKit's levelobjects functions; recorded before it was deleted, round F1a 2026-11-18:
collision_golden.json), bit for bit (game tier; needs the levels prepared and their 0.8 m samples). Read only:
nothing is written (object_triangles without its cache).

  python test_collision_backends.py [--opfor <folder>] [level ...]      default: sp_009 sp_004_b (the recorded ones)

A level whose inputs changed since the recording is skipped. Skipped (exit 0) when the OPFOR folder is not there;
needs the module.
"""
import glob
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
sys.path.insert(0, os.path.join(_ROOT, 'core', 'tests'))
import detailed  # noqa: E402
import goldens  # noqa: E402
import ladders  # noqa: E402
import navregress  # noqa: E402
import navsample_offline  # noqa: E402
import navwater_mesh  # noqa: E402
from bfbc2core import native, paths  # noqa: E402

DEFAULT = ['sp_009', 'sp_004_b']
GOLDEN = os.path.join(HERE, 'collision_golden.json')


def inputs(root: str, lv: str) -> str:
    """What the tools read, as counts and sizes per folder (the level's, the shared ones) and the samples' SHA-1."""
    L = os.path.join(root, 'work', 'levels', lv)
    bases = [os.path.join(L, 'pkg'), os.path.join(L, 'dist')] + sorted(glob.glob(os.path.join(root, 'work', 'navkit', 'common', '*')))
    out = []
    for b in bases:
        n = size = 0
        for d, _, names in os.walk(b):
            for x in names:
                if x.lower().endswith(('.xml', '.dbx', '.res')):
                    n += 1
                    size += os.path.getsize(os.path.join(d, x))
        out.append((os.path.relpath(b, root), n, size))
    samples = navregress.inputs(root, lv)[0]
    return goldens.rh((out, goldens.file_sha(samples) if isinstance(samples, str) and os.path.isfile(samples) else str(samples)), 16)


def canon(x: Any) -> Any:
    """x with arrays as (shape, dtype, the bytes' digest) and objects as their fields, order kept (bit for bit)."""
    if isinstance(x, np.ndarray):
        return ('array', x.shape, x.dtype.str, goldens.h(x.tobytes(), 16))
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


def digests(root: str, lv: str) -> dict[str, str]:
    return {k: goldens.rh(v, 16) for k, v in outcomes(root, lv).items()}


def main(argv: list[str]) -> int:
    native.require('the collision readers', 'collision_index')
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
    rec = goldens.load(GOLDEN)['levels']
    failed = 0
    for lv in levels or DEFAULT:
        try:
            navregress.inputs(root, lv)
        except RuntimeError as e:
            print(f'skip {lv}: {e}')
            continue
        if lv not in rec or rec[lv]['inputs'] != inputs(root, lv):
            print(f'skip {lv}: not in the recording, or its inputs changed since')
            continue
        t = time.perf_counter()
        got = digests(root, lv)
        for what, want in rec[lv]['outcomes'].items():
            ok = got.get(what) == want
            failed += not ok
            print(f'{"ok  " if ok else "FAIL"} {lv}: {what} as with the Python collision readers')
        print(f'     {lv}: {time.perf_counter() - t:.1f} s')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
