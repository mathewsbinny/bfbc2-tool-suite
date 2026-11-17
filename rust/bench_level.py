r"""Times level.load (rust/BENCHMARKS.md, "The level loader in Rust"): the whole load and each stage, in three modes.

  python rust\bench_level.py [--runs N] [level ...]       default: mp_005 sp_01, 3 runs (median), files cached

  rust     the Rust scene (bc2_native.scene_load) and every Rust reader: today's level.load
  main     the Python loader on the Rust DBX / FBRB readers, collision, terrain and water in Python: main before E1a
  python   everything in Python (BFBC2_PURE_PYTHON=1)

The stage times come from wrapping the functions level.load calls (Python stages; in rust mode the scene's call is
one stage). Read only: a level that level.load would prepare or unpack is skipped. Says "contended" when an OPFOR
rebuild (rebuild_all.py, ww2_level_build.py, wake_install.py) is running.
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
from bfbc2core import collision, level, paths, terrain, water  # noqa: E402
from test_scene_native import READERS, ready  # noqa: E402

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


def setup(mode: str) -> None:
    level.use_native(mode == 'rust')
    for m in READERS:
        m.use_native(mode == 'rust' or (mode == 'main' and m.__name__.endswith(('.dbx', '.fbrb'))))


def main(argv: list[str]) -> int:
    runs = int(argv[argv.index('--runs') + 1]) if '--runs' in argv else 3
    levels = [a for a in argv[1:] if not a.startswith('--') and not a.isdigit()] or ['mp_005', 'sp_01']
    root = paths.find_opfor(None, start=SUITE)
    timed(level, 'read_many', 'DBX partitions (read_many)')
    timed(level, 'resolve_blueprints', 'blueprints')
    timed(level, 'assign_teams', 'teams')
    timed(level, 'shapes_of', 'shapes')
    timed(collision, 'index', 'collision index (havok_index + mesh_aliases)')
    timed(collision, 'triangles_many', 'collision triangles')
    timed(terrain, 'read', 'terrain read')
    timed(terrain.Heightfield, 'mesh', 'terrain mesh')
    timed(water, 'read', 'water')
    timed(level, '_terrain_file', 'terrain unpack check')
    native = level._native or level._load_native() or level._with_scene(level.dbx._import_native())
    if native is not None:
        timed(native, 'scene_load', 'Rust scene (scene_load)')
        level._native = native
    print(f'{"CONTENDED: an OPFOR rebuild is running; " if contended() else ""}median of {runs} runs')
    for lv in levels:
        why = ready(root, lv)
        if why:
            print(f'skip {lv}: {why}')
            continue
        for geometry in (True, False):
            for mode in ('python', 'main', 'rust'):
                setup(mode)
                if mode == 'rust' and level._native is None:
                    level._native = native
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
                print(f'{lv} {what} {mode:6s} {med:6.2f} s  ' +
                      ', '.join(f'{n} {v:.3f}' for n, v in sorted(stages[k].items(), key=lambda x: -x[1]) if v >= 0.001))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
