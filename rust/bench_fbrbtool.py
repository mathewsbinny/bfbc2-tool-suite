r"""FbrbTool.exe against FbrbTool's commands in process (bfbc2core.fbrbtool), per command, the exe's start included.

  python rust\bench_fbrbtool.py [--runs N] [bundle ...]

Each bundle (default: a small, a medium and a large zipped bundle and a stored one of Project OPFOR BFBC2's game
copy): list, unpack into a fresh folder, pack of that folder - each way N times (default 7), alternating; best and
median printed. Outputs go to a temporary folder. Run it at below-normal priority on a quiet PC.
"""
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import fbrbtool  # noqa: E402

DEFAULT = [r'Dist\win32\async\weapon\bu_shg_f2000-00.fbrb', r'Dist\win32\levels\mp_007\ingame-00.fbrb',
           r'Dist\win32\levels\mp_006sdm\level-00.fbrb', r'Dist\win32\streaming_sounds-00.fbrb']


def timed(fn) -> float:
    t = time.perf_counter()
    fn()
    return time.perf_counter() - t


def main(argv: list[str]) -> int:
    runs = int(argv[argv.index('--runs') + 1]) if '--runs' in argv else 7
    if fbrbtool.use_exe():
        print('the Rust core is not built for this checkout (python rust\build_native.py): nothing to compare')
        return 1
    bundles = [a for a in argv[1:] if a.lower().endswith('.fbrb')]
    if not bundles:
        suite_paths.use('navkit')
        import navkit
        game = os.path.join(navkit.find_opfor(None), 'game')
        bundles = [os.path.join(game, b) for b in DEFAULT if os.path.isfile(os.path.join(game, b))]
    work = tempfile.mkdtemp(prefix='bench_fbrbtool_')
    print(f'{"command":<8} {"bundle":<40} {"MB":>7} {"FbrbTool.exe best / median":>28} {"in process best / median":>26} '
          f'{"faster":>7}')
    try:
        for b in bundles:
            src = os.path.join(work, 'src')
            shutil.rmtree(src, ignore_errors=True)
            subprocess.run([fbrbtool.EXE, 'unpack', b, src], check=True, capture_output=True)
            for cmd in ('list', 'unpack', 'pack'):
                times: dict[str, list[float]] = {'exe': [], 'rust': []}
                for _ in range(runs):
                    for way in ('exe', 'rust'):
                        out = os.path.join(work, way)
                        shutil.rmtree(out, ignore_errors=True)
                        args = {'list': ['list', b], 'unpack': ['unpack', b, out],
                                'pack': ['pack', src, out + '.fbrb']}[cmd]
                        if way == 'exe':
                            dt = timed(lambda a=args: subprocess.run([fbrbtool.EXE, *a], check=True, capture_output=True))
                        else:
                            dt = timed(lambda a=args: fbrbtool.run(a, check=True))
                        times[way].append(dt)
                e, r = times['exe'], times['rust']
                fmt = lambda v: f'{min(v) * 1000:,.0f} / {statistics.median(v) * 1000:,.0f} ms'  # noqa: E731
                print(f'{cmd:<8} {os.path.basename(b)[:40]:<40} {os.path.getsize(b) / 1e6:>7.1f} {fmt(e):>28} '
                      f'{fmt(r):>26} {statistics.median(e) / statistics.median(r):>6.1f}x', flush=True)
            shutil.rmtree(src, ignore_errors=True)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
