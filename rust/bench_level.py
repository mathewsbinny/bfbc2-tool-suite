r"""Times level.load (rust/BENCHMARKS.md, "The level loader in Rust"): the whole load and its stages.

  python rust\bench_level.py [--runs N] [--opfor <folder>] [level ...]
                                     default: mp_005 sp_01, 3 runs (median), files cached; --opfor: another
                                     OPFOR-like folder (e.g. a temp prepare of a WW2 level: BENCHMARKS.md)

The Rust scene (bc2_native.scene_load) and every Rust reader: level.load since round E1a. The Python loader and
readers it was compared with (the "main" and "python" modes) were retired on 2026-11-18 (round F1a); their timings
are kept in BENCHMARKS.md. The stage times come from wrapping the functions level.load calls. Read only: a level that
level.load would prepare or unpack is skipped. Says "contended" when an OPFOR rebuild (rebuild_all.py,
ww2_level_build.py, wake_install.py) is running.
"""
import collections
import functools
import os
import statistics
import subprocess
import sys
import time
from typing import Any

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
sys.path.insert(0, os.path.join(SUITE, 'core'))
sys.path.insert(0, os.path.join(SUITE, 'core', 'tests'))
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit', 'navkit-tools')
from bfbc2core import level, native, paths  # noqa: E402
from test_scene_native import ready  # noqa: E402

STAGES: dict[str, float] = collections.defaultdict(float)


def timed(mod: Any, name: str, key: str) -> None:
    f = getattr(mod, name)

    @functools.wraps(f)
    def w(*a: Any, **k: Any) -> Any:
        t = time.perf_counter()
        try:
            return f(*a, **k)
        finally:
            STAGES[key] += time.perf_counter() - t
    setattr(mod, name, w)


def contended() -> bool:
    """An OPFOR rebuild is running (its CPU load makes timings noisy)."""
    if os.name != 'nt':
        return False
    r = subprocess.run(['powershell', '-NoProfile', '-Command',
                        "Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -match "
                        "'rebuild_all|ww2_level_build|wake_install' } | Measure-Object | % Count"],
                       capture_output=True, text=True)
    return r.stdout.strip() not in ('', '0')


def main(argv: list[str]) -> int:
    runs = int(argv[argv.index('--runs') + 1]) if '--runs' in argv else 3
    opfor = argv[argv.index('--opfor') + 1] if '--opfor' in argv else None
    levels = [a for a in argv[1:] if not a.startswith('--') and not a.isdigit() and a != opfor] or ['mp_005', 'sp_01']
    root = paths.find_opfor(opfor, start=SUITE)
    timed(level, 'read_many', 'DBX partitions (read_many)')
    timed(level, '_terrain_file', 'terrain unpack check')
    timed(native.require('the Rust scene', 'scene_load'), 'scene_load', 'Rust scene (scene_load)')
    print(f'{"CONTENDED: an OPFOR rebuild is running; " if contended() else ""}median of {runs} runs')
    for lv in levels:
        why = ready(root, lv)
        if why:
            print(f'skip {lv}: {why}')
            continue
        for geometry in (True, False):
            totals: list[float] = []
            stages: list[dict[str, float]] = []
            for _ in range(runs):
                STAGES.clear()
                t = time.perf_counter()
                level.load(lv, root, log=lambda *_: None, geometry=geometry)
                totals.append(time.perf_counter() - t)
                stages.append(dict(STAGES))
            med = statistics.median(totals)
            k = totals.index(med) if med in totals else 0
            what = 'whole load' if geometry else 'entities only'
            print(f'{lv} {what} {med:6.2f} s  ' +
                  ', '.join(f'{n} {v:.3f}' for n, v in sorted(stages[k].items(), key=lambda x: -x[1]) if v >= 0.001))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
