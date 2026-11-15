r"""Runs a tool's build.bat only when its .exe files are missing or older than their sources.

  python tests\build_if_stale.py <build.bat> <exe> [<exe> ...] -- <source> [<source> ...]

The quick test tier ran every build.bat on every run (QA 2026-11-15). The .NET Framework 4 compiler does not build
deterministically, so each run gave new bytes and a new modification time to tools a plan had run: OPFOR's install
plan fingerprint counts the suite's .exe files, so a test run between a plan and its apply threw the plan away. The
sources include the build.bat itself (a changed compiler line rebuilds).
"""
import os
import subprocess
import sys


def stale(exes, sources):
    if not all(os.path.isfile(e) for e in exes):
        return True
    built = min(os.path.getmtime(e) for e in exes)
    return any(os.path.getmtime(s) > built for s in sources)


def main(argv):
    if '--' not in argv or argv.index('--') < 2:
        print(__doc__)
        return 2
    i = argv.index('--')
    bat, exes, sources = argv[0], argv[1:i], [argv[0]] + argv[i + 1:]
    missing = [s for s in sources if not os.path.isfile(s)]
    if missing:
        print(f'source not found: {missing[0]}')
        return 1
    if not stale(exes, sources):
        print('up to date: ' + ', '.join(os.path.basename(e) for e in exes))
        return 0
    r = subprocess.run(['cmd', '/c', bat])
    return r.returncode


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
