r"""Round O4: the editor's level prepare in Rust (bfbc2.exe prepare_level, rust\bc2-cli\src\navkit) against the answers
of the Python it ported (rust\bc2-editor\prepare_level.py: navkit.prepare and bfbc2core.level.heightfield_of; retired
in round RT1), recorded in prepare_o4_golden.json, on real stock levels.

  python core\tests\test_prepare_o4.py --game [--levels sp_009,sp_004_b,mp_005] [--exe <bfbc2.exe>] [--root <folder>]
                                       [--keep]
  python core\tests\test_prepare_o4.py --game --record --ref <a suite checkout that still has prepare_level.py>

A stand-in Project OPFOR folder is made under --root (default: a new %TEMP%\o4prep_* of the run's own): its game\ a junction to the real game
copy (read only: nothing is written there), its own empty work\, and its own LOCALAPPDATA (the day logs, the
collision cache). The levels are prepared one after another with prepare_level (the common bundles unpacked by the
first), then each again (everything already unpacked), then each with --terrain-only. Compared with the recording:
every run's exit code and both streams' bytes (the stand-in folder named <root>; SHA-1), every file under work\ (path
and sha256: their digest, overall and per top folder), and the day logs' records (stamps taken out). The junctions
are removed with rmdir before the folders are deleted. Every run must be answered by Rust (BFBC2_CLI_TRACE).
A changed game copy changes the answers: record again with --ref a checkout from before the retirement (a git worktree of main 6a7dd68 with its module built).

Without the game copy (or with no --game) it is skipped: the quick tier's prepare cases are cli_o4_navkit.py's.
"""
from __future__ import annotations

import contextlib
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native, paths  # noqa: E402

GOLDEN = os.path.join(HERE, 'prepare_o4_golden.json')


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


def summary(rows: list) -> dict:
    """A side's answers as the golden keeps them: each run's arguments, exit code and the SHA-1 of both streams;
    the files under work\\ (count, a digest of the sorted 'path sha256' lines, and one per top folder, so a
    difference is placed); the day logs' records (digest)."""
    out: dict = {'runs': [], 'files': {}, 'logs': ''}
    for row in rows:
        if row[0] == 'files':
            files = row[1]
            lines = sorted(f'{k.replace(os.sep, "/")} {v}' for k, v in files.items())
            tops: dict[str, list[str]] = {}
            for ln in lines:
                tops.setdefault(ln.split('/', 1)[0] if '/' in ln.split(' ', 1)[0] else '.', []).append(ln)
            out['files'] = {'count': len(lines), 'all': hashlib.sha1('\n'.join(lines).encode()).hexdigest(),
                            'tops': {t: [len(v), hashlib.sha1('\n'.join(v).encode()).hexdigest()]
                                     for t, v in sorted(tops.items())}}
        elif row[0] == 'logs':
            out['logs'] = hashlib.sha1(row[1].encode('utf-8')).hexdigest()
        else:
            r, code, so, se = row
            out['runs'].append([' '.join(r), code, hashlib.sha1(so).hexdigest(), hashlib.sha1(se).hexdigest()])
    return out


def compare(want: dict, got: dict) -> list[str]:
    bad = []
    for a, b in zip(want['runs'], got['runs']):
        if a != b:
            bad.append(f'run {a[0]}: exit {a[1]} / {b[1]}, stdout {"same" if a[2] == b[2] else "differs"}, stderr '
                       f'{"same" if a[3] == b[3] else "differs"}')
    if len(want['runs']) != len(got['runs']):
        bad.append(f'{len(want["runs"])} runs recorded, {len(got["runs"])} now')
    if want['files'] != got['files']:
        wt, gt = want['files'].get('tops', {}), got['files'].get('tops', {})
        bad.append(f'files: {want["files"].get("count")} recorded, {got["files"].get("count")} now; folders that '
                   f'differ: {sorted(t for t in set(wt) | set(gt) if wt.get(t) != gt.get(t))[:8]}')
    if want['logs'] != got['logs']:
        bad.append('the day logs\' records differ')
    return bad


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
    given = arg(argv, '--root')
    levels = (arg(argv, '--levels') or 'sp_009,sp_004_b,mp_005').split(',')
    record = '--record' in argv
    script = os.path.join(ref, 'rust', 'bc2-editor', 'prepare_level.py')
    if record and not os.path.isfile(script):
        print(f'cannot record: {script} is not there (retired in round RT1; --ref a checkout before it)')
        return 1
    if not record and (not exe or not os.path.isfile(exe)):
        print('skipped: bfbc2.exe is not built')
        return 0
    runs = [[lv] for lv in levels] + [[lv] for lv in levels] + [[lv, '--terrain-only'] for lv in levels]
    goldens = {}
    if os.path.isfile(GOLDEN):
        with open(GOLDEN, encoding='utf-8') as f:
            goldens = json.load(f)
    key = ','.join(levels)
    if not record and key not in goldens:
        print(f'FAIL no recorded answers for --levels {key} (recorded: {sorted(goldens)})')
        return 1
    side = 'python' if record else 'rust'
    took: list[float] = []
    # a folder of this run's own unless --root names one (was %TEMP%\o4prep: two worktrees' runs at once emptied
    # each other's stand-in)
    root = os.path.abspath(given or tempfile.mkdtemp(prefix='o4prep_'))
    try:
        remove(root)
        opfor, lad = stand_in(root, game)
        env = {k: v for k, v in os.environ.items() if k.upper() not in ('BFBC2_CLI_PYTHON', 'BFBC2_PURE_PYTHON')}
        env.update(LOCALAPPDATA=lad, BFBC2_CLI_SUITE=ref, BFBC2_CLI_TRACE=os.path.join(root, 'trace.txt'))
        cmd = [sys.executable, script] if record else [str(exe), 'prepare_level']
        rows: list = []
        for r in runs:
            t = time.perf_counter()
            p = subprocess.run(cmd + r + ['--opfor', opfor], capture_output=True, env=env, cwd=SUITE)
            took.append(time.perf_counter() - t)
            rows.append((r, p.returncode, p.stdout.replace(opfor.encode(), b'<root>'),
                         p.stderr.replace(opfor.encode(), b'<root>')))
            last = (p.stdout.strip().splitlines() or [b''])[-1].decode('utf-8', 'replace')[:200]
            print(f'  {side:6s} {" ".join(r):24s} exit {p.returncode}  {took[-1]:7.1f} s' +
                  (f'  ({last})' if p.returncode else ''), flush=True)
        rows.append(('files', tree(os.path.join(opfor, 'work'))))
        rows.append(('logs', log_records(lad)))
        got = summary(rows)
        if record:
            goldens[key] = {**got, 'recorded': time.strftime('%Y-%m-%d'), 'by': 'prepare_level.py',
                            'seconds': [round(x, 1) for x in took]}
            with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
                json.dump(goldens, f, indent=1, sort_keys=True)
                f.write('\n')
            print(f'recorded {len(runs)} runs and {got["files"]["count"]} files of prepare_level.py into {GOLDEN}')
            return 0
        with open(os.path.join(root, 'trace.txt'), encoding='utf-8') as f:
            who = f.read().split()
        want = goldens[key]
        bad = compare(want, got)
        for b in bad:
            print(f'FAIL {b}')
        print(f'{"ok  " if not bad else "FAIL"} bfbc2.exe prepare_level, {len(runs)} runs on {", ".join(levels)}: '
              f'output, exit codes, {got["files"]["count"]} files under work\\ (sha256) and the day logs as the retired '
              f'prepare_level.py answered ({want["recorded"]}); answered by Rust {who.count("rust")} of {len(runs)} '
              f'(Rust {sum(took):.1f} s, the Python {sum(want["seconds"]):.1f} s when recorded)')
        for i, r in enumerate(runs):
            print(f'     {" ".join(r):24s} Rust {took[i]:7.1f} s  Python {want["seconds"][i]:7.1f} s (recorded)')
        return 1 if bad or who.count('rust') != len(runs) else 0
    finally:
        if '--keep' not in argv:
            remove(root)
            if not given:
                with contextlib.suppress(OSError):
                    os.rmdir(root)


if __name__ == '__main__':
    sys.exit(main(sys.argv))

