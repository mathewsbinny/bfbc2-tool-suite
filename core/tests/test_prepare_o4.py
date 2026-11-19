r"""Round O4: the editor's level prepare in Rust (bfbc2.exe prepare_level, rust\bc2-cli\src\navkit) against the Python it
ports (rust\bc2-editor\prepare_level.py: navkit.prepare and bfbc2core.level.heightfield_of), on real stock levels.

  python core\tests\test_prepare_o4.py --game [--levels sp_009,sp_004_b,mp_005] [--exe <bfbc2.exe>] [--ref <suite>]
                                       [--root <scratch folder>] [--keep]

For each side a stand-in Project OPFOR folder is made under --root (default: %TEMP%\o4prep): its game\ a junction to
the real game copy (read only: nothing is written there), its own empty work\, and its own LOCALAPPDATA (the day logs,
the collision cache). The levels are prepared one after another with prepare_level (the common bundles unpacked by the
first), then each again (everything already unpacked), then each with --terrain-only. Compared: every run's exit code
and stdout bytes (the stand-in folder named <root>), every file under work\ (path and sha256), and the day logs'
records (stamps taken out). The junctions are removed with rmdir before the folders are deleted.

Without the game copy (or with no --game) it is skipped: the quick tier's prepare cases are cli_o4_navkit.py's.
"""
from __future__ import annotations

import hashlib
import os
import re
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native, paths  # noqa: E402


def arg(argv: list[str], k: str, d: str | None = None) -> str | None:
    return argv[argv.index(k) + 1] if k in argv[:-1] else d


def tree(work: str) -> dict[str, str]:
    out = {}
    for d, _ds, fs in os.walk(work):
        for f in fs:
            p = os.path.join(d, f)
            with open(p, 'rb') as fh:
                out[os.path.relpath(p, work)] = hashlib.sha256(fh.read()).hexdigest()
    return out


def log_records(lad: str) -> str:
    logs = os.path.join(lad, 'bfbc2-tool-suite', 'logs')
    out = []
    if os.path.isdir(logs):
        for n in sorted(x for x in os.listdir(logs) if not x.startswith('.')):
            with open(os.path.join(logs, n), 'rb') as f:
                rows = re.findall(rb'(?m)^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d,\d{3} ([^\r\n]*)', f.read())
            out.append(re.sub(r'\d{4}-\d\d-\d\d', '<date>', n) + '\n' + b'\n'.join(rows).decode('latin-1'))
    return '\n'.join(out)


def stand_in(root: str, game: str) -> tuple[str, str]:
    base = root
    opfor = os.path.join(base, 'opfor')
    os.makedirs(os.path.join(opfor, 'work'))
    subprocess.run(['cmd', '/c', 'mklink', '/J', os.path.join(opfor, 'game'), game], check=True, capture_output=True)
    lad = os.path.join(base, 'lad')
    os.makedirs(lad)
    return opfor, lad


def remove(root: str) -> None:
    j = os.path.join(root, 'opfor', 'game')
    if os.path.lexists(j):
        subprocess.run(['cmd', '/c', 'rmdir', j], capture_output=True)
        if os.path.lexists(j):
            raise SystemExit(f'could not remove the junction {j}: stopped before deleting anything')
    for n in os.listdir(root) if os.path.isdir(root) else []:
        p = os.path.join(root, n)
        if os.path.isdir(p):
            shutil.rmtree(p, ignore_errors=True)
        else:
            os.remove(p)


def main(argv: list[str]) -> int:
    if '--game' not in argv:
        print('skipped: the prepare proof needs the game copy (--game)')
        return 0
    try:
        opfor_real = paths.find_opfor()
    except paths.PathsError:
        print('skipped: no Project OPFOR BFBC2 copy')
        return 0
    game = os.path.join(opfor_real, 'game')
    exe = arg(argv, '--exe') or native.find_binary('bfbc2')
    ref = os.path.abspath(arg(argv, '--ref') or os.environ.get('BFBC2_O2_REF') or SUITE)
    root = os.path.abspath(arg(argv, '--root') or os.path.join(os.environ.get('TEMP', '.'), 'o4prep'))
    levels = (arg(argv, '--levels') or 'sp_009,sp_004_b,mp_005').split(',')
    if not exe or not os.path.isfile(exe):
        print('skipped: bfbc2.exe is not built')
        return 0
    runs = [[lv] for lv in levels] + [[lv] for lv in levels] + [[lv, '--terrain-only'] for lv in levels]
    got: dict[str, list] = {}
    times: dict[str, list[float]] = {}
    try:
        for side in ('python', 'rust'):
            remove(root)
            opfor, lad = stand_in(root, game)
            env = {k: v for k, v in os.environ.items() if k.upper() not in ('BFBC2_CLI_PYTHON', 'BFBC2_PURE_PYTHON')}
            env.update(LOCALAPPDATA=lad, BFBC2_CLI_SUITE=ref, BFBC2_CLI_TRACE=os.path.join(root, 'trace.txt'))
            cmd = ([sys.executable, os.path.join(ref, 'rust', 'bc2-editor', 'prepare_level.py')] if side == 'python'
                   else [exe, 'prepare_level'])
            got[side], times[side] = [], []
            for r in runs:
                t = time.perf_counter()
                p = subprocess.run(cmd + r + ['--opfor', opfor], capture_output=True, env=env, cwd=SUITE)
                times[side].append(time.perf_counter() - t)
                out = p.stdout.replace(opfor.encode(), b'<root>')
                got[side].append((r, p.returncode, out, p.stderr.replace(opfor.encode(), b'<root>')))
                print(f'  {side:6s} {" ".join(r):24s} exit {p.returncode}  {times[side][-1]:7.1f} s', flush=True)
            got[side].append(('files', tree(os.path.join(opfor, 'work'))))
            got[side].append(('logs', log_records(lad)))
            if side == 'rust':
                with open(os.path.join(root, 'trace.txt'), encoding='utf-8') as f:
                    who = f.read().split()
                print(f'  who answered (Rust side): {who.count("rust")} rust, {who.count("python")} python')
            if side == 'python' and '--keep' not in argv:
                remove(root)          # the same folder for both sides: the same paths in every output
        bad = 0
        for a, b in zip(got['python'], got['rust']):
            if a != b:
                bad += 1
                if a[0] == 'files':
                    da, db = a[1], b[1]
                    only = sorted(set(da) ^ set(db))[:5]
                    diff = sorted(k for k in set(da) & set(db) if da[k] != db[k])[:5]
                    print(f'FAIL files: {len(da)} / {len(db)}; only one side: {only}; differ: {diff}')
                elif a[0] == 'logs':
                    print(f'FAIL logs:\n{a[1][-800:]}\n---\n{b[1][-800:]}')
                else:
                    print(f'FAIL {a[0]}: exit {a[1]} / {b[1]}\n{a[2][-600:]!r}\n{b[2][-600:]!r}\n{a[3][-300:]!r}\n{b[3][-300:]!r}')
        files = got['python'][-2][1]
        print(f'{"ok  " if not bad else "FAIL"} prepare_level, {len(runs)} runs on {", ".join(levels)}: output, exit codes, '
              f'{len(files)} files under work\\ (sha256) and the day logs identical both ways '
              f'(Python {sum(times["python"]):.1f} s, Rust {sum(times["rust"]):.1f} s)')
        for i, r in enumerate(runs):
            print(f'     {" ".join(r):24s} Python {times["python"][i]:7.1f} s  Rust {times["rust"][i]:7.1f} s')
        return 1 if bad else 0
    finally:
        if '--keep' not in argv:
            remove(root)


if __name__ == '__main__':
    sys.exit(main(sys.argv))
