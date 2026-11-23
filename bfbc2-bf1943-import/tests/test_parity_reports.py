r"""The parity tools' reports on the user's files (game tier; skipped without the Rising Sun workspace, the game copy
or bfbc2.exe): `bfbc2 dbxparity` (the field-by-field comparison and `--predict`) and `bfbc2 portparity`, run in an
empty folder, must write the reports, the enum and model JSON and the console lines whose SHA-256 the retired Python
command lines gave (dbxparity.py / portparity.py at 0434c22, recorded in parity_reports_golden.json on 2026-11-22;
round PR1b-1d, rust/BENCHMARKS.md "PR1b-1d"), every run answered by the Rust (BFBC2_CLI_TRACE). Only digests are
kept: the reports name EA's assets and values.

The digests are the user's game copy's and BF1943 copy's: a change to either (a bundle replaced) changes the reports.
Record again from the last commit with the Python (`git worktree add <dir> 0434c22`, build its module, run its
dbxparity.py / portparity.py as RUNS does in an empty folder) - never from bfbc2.exe itself.

  python test_parity_reports.py
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import bf1943import as bi  # noqa: E402
from bfbc2core import native  # noqa: E402

GOLDEN = os.path.join(HERE, 'parity_reports_golden.json')
# (key, arguments, the files written in the run's folder)
RUNS = [('dbxparity', ['dbxparity'], ['dbxparity.txt', 'dbxparity_enums.json']),
        ('predict', ['dbxparity', '--predict', '-o', 'predict.txt'], ['predict.txt']),
        ('portparity', ['portparity'], ['portparity.txt', 'portparity.json'])]


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def main() -> int:
    exe = native.cli_exe()
    if exe is None:
        print('SKIP: bfbc2.exe is not built from this checkout (python rust\\build_native.py)')
        return 0
    if not os.path.isdir(bi.default_source()):
        print(f'SKIP: no BF1943 source at {bi.default_source()}')
        return 0
    with open(GOLDEN, encoding='utf-8') as f:
        golden = json.load(f)['runs']
    fails = []
    tmp = tempfile.mkdtemp(prefix='parity_reports_')
    try:
        # the catalogue's cache made current first (round PR1b-1e: --predict reads it; the Python's recording read
        # its pickle, so a build's lines on stderr are not the recorded run's)
        warm = subprocess.run([exe, 'dbxcatalogue'], cwd=tmp, capture_output=True, timeout=1800)
        if warm.returncode != 0 and b'Project OPFOR BFBC2 not found' in warm.stderr:
            print('SKIP: no Project OPFOR BFBC2 game copy')
            return 0
        for key, args, files in RUNS:
            d = os.path.join(tmp, key)
            os.makedirs(d)
            trace = os.path.join(tmp, f'{key}.trace')
            env = dict(os.environ, BFBC2_CLI_TRACE=trace)
            env.pop('COLUMNS', None)
            r = subprocess.run([exe, *args], cwd=d, capture_output=True, env=env, timeout=1800)
            if r.returncode != 0 and b'Project OPFOR BFBC2 not found' in r.stderr:
                print('SKIP: no Project OPFOR BFBC2 game copy')
                return 0
            with open(trace, encoding='utf-8') as f:
                who = f.read().split()[-1:]
            got = {'exit': r.returncode, 'stdout': sha(r.stdout), 'stderr': sha(r.stderr)}
            for name in files:
                p = os.path.join(d, name)
                got[name] = sha(open(p, 'rb').read()) if os.path.isfile(p) else None
            ok = got == golden[key] and who == ['rust']
            print(f'{"ok  " if ok else "FAIL"} {" ".join(args)}: the retired Python\'s reports, byte for byte '
                  f'(answered by {" ".join(who) or "?"})', flush=True)
            if not ok:
                fails.append(key)
                for k, v in got.items():
                    if golden[key].get(k) != v:
                        print(f'     {k}: {v} / recorded {golden[key].get(k)}')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not fails else f'{len(fails)} failed')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
