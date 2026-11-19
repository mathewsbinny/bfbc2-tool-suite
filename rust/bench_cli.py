r"""Round O1's timings (rust/BENCHMARKS.md, "O1"): the Python command lines against bfbc2.exe, as processes, on
Project OPFOR's real calls (read only; outputs in a temporary folder). Best of --repeat runs per side, the sides
alternating.

    python rust\bench_cli.py [--repeat 3] [--exe rust\target\release\bfbc2.exe]
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native, paths  # noqa: E402


def timed(cmd: list[str]) -> float:
    t = time.perf_counter()
    r = subprocess.run(cmd, capture_output=True)
    took = time.perf_counter() - t
    if r.returncode not in (0, 2):
        raise SystemExit(f'failed ({r.returncode}): {cmd}\n{r.stderr.decode("utf-8", "replace")}')
    return took


def main(argv: list[str]) -> int:
    repeat = int(argv[argv.index('--repeat') + 1]) if '--repeat' in argv else 3
    exe = argv[argv.index('--exe') + 1] if '--exe' in argv else native.cli_path()
    opfor = paths.find_opfor()
    game = os.path.join(opfor, 'game')
    menu = os.path.join(opfor, 'original', 'Package', 'menu', 'main', 'mainmenu-00.fbrb')
    startup = os.path.join(game, 'Package', 'async', 'startup-00.fbrb')
    sp5 = [os.path.join(game, *top, 'sp_005', 'level-00.fbrb') for top in (('Package', 'levels'), ('Dist', 'win32', 'levels'))]
    tmp = tempfile.mkdtemp(prefix='bench_cli_')
    sample = os.path.join(tmp, 'f.dbx')
    src = next(os.path.join(d, f) for d, _, fs in os.walk(os.path.join(opfor, 'work', 'levels', 'mp_005'))
               for f in fs if f.endswith('.dbx') and os.path.getsize(os.path.join(d, f)) > 20000)
    shutil.copyfile(src, sample)
    side = {'python': lambda tool: [sys.executable, native.CLI_TOOLS[tool]], 'rust': lambda tool: [exe, tool]}

    def jobs(s: str) -> dict[str, list[list[str]]]:
        f, d = side[s]('fbrbtool'), side[s]('dbxedit')
        o = os.path.join(tmp, s)
        return {
            'usage (start-up alone)': [f],
            'list startup-00.fbrb': [f + ['list', startup]],
            'build_menus.bat: unpack + pack mainmenu-00': [f + ['unpack', menu, o + 'm'], f + ['pack', o + 'm', o + 'm.fbrb']],
            'merge_startup.ps1: unpack + pack startup-00': [f + ['unpack', startup, o + 's'], f + ['pack', o + 's', o + 's.fbrb']],
            'campaign_fill.ps1: unpack sp_005 x2': [f + ['unpack', sp5[0], o + 'p'], f + ['unpack', sp5[1], o + 'd']],
            'dbxedit xml + roundtrip + check, one MP_005 partition': [d + ['xml', sample], d + ['roundtrip', sample],
                                                                     d + ['check', sample]],
        }
    best: dict[tuple[str, str], float] = {}
    try:
        for _ in range(repeat):
            for s in ('python', 'rust'):
                for name, cmds in jobs(s).items():
                    for o in ('m', 's', 'p', 'd'):
                        shutil.rmtree(os.path.join(tmp, s + o), ignore_errors=True)
                    took = sum(timed(c) for c in cmds)
                    best[(name, s)] = min(best.get((name, s), took), took)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print(f'| job (best of {repeat}) | Python | Rust | |\n|---|---:|---:|---:|')
    for name in jobs('rust'):
        p, r = best[(name, 'python')], best[(name, 'rust')]
        print(f'| {name} | {p:.3f} s | {r:.3f} s | x{p / r:.1f} |')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
