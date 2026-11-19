r"""The suite's command lines in Rust (round O1: rust\bc2-cli, bfbc2.exe) against the Python command lines they port
(bfbc2-fbrb-tool\fbrbtool.py, bfbc2-dbx-tools\dbxedit.py), as processes: the same exit codes, the same bytes on
stdout and stderr (each alone, and both on one pipe: the order Python's buffering gives), the same files and the same
log lines (rust/BENCHMARKS.md, "O1").

  python core\tests\test_cli_o1.py            quick: every recorded command line (test_fbrbtool's 228, test_dbxedit's
                                              82, the encoding / buffering / environment cases) through bfbc2.exe,
                                              against what the Python command lines answered (cli_o1_golden.json)
  python core\tests\test_cli_o1.py --live     + the Python command lines run again beside it, both ways compared
  python core\tests\test_cli_o1.py --game     + Project OPFOR's real calls replayed read-only (build_menus.bat's,
                                              merge_startup.ps1's and campaign_fill.ps1's FbrbTool lines, the logged
                                              FbrbTool invocations' bundles, a DbxEdit corpus sample copied out), both
                                              ways, timed
  python core\tests\test_cli_o1.py --record   record cli_o1_golden.json from the Python command lines (before the
                                              Python is retired: the oracle afterwards)
     --exe <bfbc2.exe>   the binary (default: rust\target\release\bfbc2.exe, else debug)
     --sample N          logged FbrbTool invocations' bundles in --game (default 12)

Skipped (exit 0) without the binary or without the Rust core module (the Python command lines need it).
"""
from __future__ import annotations

import datetime
import os
import random
import re
import shutil
import subprocess
import sys
import time
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native, paths  # noqa: E402

if HERE not in sys.path:
    sys.path.insert(0, HERE)
import csharp_golden as cg  # noqa: E402
import test_dbxedit as tdx  # noqa: E402
import test_fbrbtool as tfb  # noqa: E402

GOLDEN = 'cli_o1_golden.json'
SCRIPTS = native.CLI_TOOLS
# the environment Python's stdio reads, left out unless a case sets it
PY_ENV = ('PYTHONUTF8', 'PYTHONIOENCODING', 'PYTHONUNBUFFERED', 'PYTHONLEGACYWINDOWSSTDIO')
WORKERS = 4
Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]   # key, tool, files, args, env, merged


# ------------------------------------------------------------------------------------------- the cases

def big_list_bundle(n: int) -> bytes:
    """A bundle whose `list` prints more than Python's 8,192-byte buffer (stdout then leaves before stderr)."""
    return tfb.bundle([(f'Levels/Big/Entry{i:05d}.dbx', 'T', 0x10000, bytes([i % 251]) * 3) for i in range(n)])


def damaged_big(n: int) -> bytes:
    """A BIG archive of n files whose last one's data runs past the end: `big` prints each file it extracts on
    stdout, then fails on stderr."""
    files = [(f'data/file{i:04d}.bin', b'x' * 4) for i in range(n)] + [('z.bin', b'y')]
    return tfb.big_archive(files, fix=lambda name, o, s: (o, s + 999) if name == 'z.bin' else (o, s))


def cases() -> list[Case]:
    rows: list[Case] = []
    for name, files, args in tfb.cases():
        rows.append((f'fbrbtool: {name}', 'fbrbtool', files, args, {}, False))
    for name, files, args, _ in tdx.cases():
        rows.append((f'dbxedit: {name}', 'dbxedit', files, args, {}, False))
    basic = {'b.fbrb': tfb.bundle(tfb.BASIC)}
    latin = {'b.fbrb': tfb.bundle([('Levels/Caf\u00e9/Men\u00fc.dbx', 'T', 0x10000, b'x' * 9)])}
    extra: list[tuple[str, str, dict[str, Any], list[str]]] = [
        ('a path in cp1252 (stderr)', 'fbrbtool', {}, ['list', '{d}\\caf\u00e9.fbrb']),
        ('a path outside cp1252 (stderr: backslashreplace)', 'fbrbtool', {}, ['list', '{d}\\\u0416\u0417.fbrb']),
        ('a path outside cp1252 (stdout: UnicodeEncodeError)', 'fbrbtool', basic, ['unpack', '{d}\\b.fbrb', '{d}\\\u0416']),
        ('an emoji path (stdout)', 'fbrbtool', basic, ['unpack', '{d}\\b.fbrb', '{d}\\\U0001F600 x']),
        ('entry names in Latin-1 (stdout)', 'fbrbtool', latin, ['list', '{d}\\b.fbrb']),
        ('entry names in Latin-1, unpacked', 'fbrbtool', latin, ['unpack', '{d}\\b.fbrb', '{d}\\caf\u00e9']),
        ('a list over 8,192 bytes', 'fbrbtool', {'b.fbrb': big_list_bundle(400)}, ['list', '{d}\\b.fbrb']),
        ('a list of 8,192 bytes or less', 'fbrbtool', {'b.fbrb': big_list_bundle(60)}, ['list', '{d}\\b.fbrb']),
        # both streams: stdout over / at most 8,192 bytes, then an error on stderr (the order on one pipe flips)
        ('big: stdout over 8,192 bytes, then an error', 'fbrbtool', {'x.big': damaged_big(600)},
         ['big', '{d}\\x.big', '{d}\\out']),
        ('big: stdout under 8,192 bytes, then an error', 'fbrbtool', {'x.big': damaged_big(100)},
         ['big', '{d}\\x.big', '{d}\\out']),
        ('xml of a folder named outside cp1252', 'dbxedit', {'\u0416\\a.dbx': tdx.sample().bytes()}, ['xml', '{d}\\\u0416']),
        ('xml of a folder named in cp1252', 'dbxedit', {'\u00e9t\u00e9\\a.dbx': tdx.sample().bytes()}, ['xml', '{d}\\\u00e9t\u00e9']),
        ('usage', 'dbxedit', {}, []),
        ('an empty argument', 'fbrbtool', {}, ['list', '']),
        ('arguments with spaces and quotes', 'fbrbtool', {}, ['list', '{d}\\a "b" c.fbrb']),
    ]
    for name, tool, files, args in extra:
        rows.append((f'{tool}: {name}', tool, files, args, {}, False))
    # one pipe for both streams: every case with both outputs, and the buffering cases
    rows += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in list(rows)]
    # Python's stdio switches
    variants = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
                'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}
    picked = [r for r in rows if r[0].startswith(('fbrbtool: a path', 'fbrbtool: an emoji', 'fbrbtool: entry names',
                                                   'fbrbtool: a list', 'fbrbtool: big: stdout',
                                                   'dbxedit: xml of a folder'))]
    for label, env in variants.items():
        rows += [(f'{k} [{label}]', t, f, a, env, m) for k, t, f, a, _, m in picked]
    return rows


# ------------------------------------------------------------------------------------------- running one

class Place:
    """One worker's folders: the case folder (as long as the recorded one: FbrbTool's path rules) and its own
    LOCALAPPDATA, so each run's log is its alone."""

    def __init__(self, k: int, dir_len: int) -> None:
        self.root, self.d = cg.case_root(dir_len, f'cli_o1_{k}_')
        self.lad = os.path.join(self.root, 'lad')
        self.logs = os.path.join(self.lad, 'bfbc2-tool-suite', 'logs')

    def norm(self, data: bytes) -> str:
        """Printed bytes with the folders named (in every spelling Python writes them), the log's day and the
        temporary files' numbers taken out; as Latin-1 text (one character per byte)."""
        places = sorted(((self.logs, '<logs>'), (self.d, '<dir>'), (self.lad, '<lad>'), (SUITE, '<cwd>')),
                        key=lambda p: -len(p[0]))
        for p, name in places:
            spellings = {p.encode('utf-8'), p.encode('cp1252', 'backslashreplace')}
            for s in sorted(spellings, key=len, reverse=True):
                data = data.replace(s, name.encode('ascii'))
        data = re.sub(rb'<logs>\\[^\s\\]+\.log', b'<log>', data)
        data = re.sub(rb'\.tmp-\d+(-\d+)?', b'.tmp-N', data)
        return data.decode('latin-1')

    def log(self) -> str:
        """The log lines the run appended: stamps as <stamp>, its files' names by tool and part."""
        out = []
        if os.path.isdir(self.logs):
            for n in sorted(os.listdir(self.logs)):
                with open(os.path.join(self.logs, n), 'rb') as f:
                    text = re.sub(rb'(?m)^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d,\d{3} ', b'<stamp> ', f.read())
                name = re.sub(r'\d{4}-\d\d-\d\d', '<date>', n)
                out.append(f'[{name}]\n' + self.norm(text))
        return ''.join(out)


def setup(pl: Place, files: dict[str, Any]) -> None:
    cg.fresh(pl.d)
    shutil.rmtree(pl.lad, ignore_errors=True)
    for name, data in files.items():
        p = os.path.join(pl.d, name)
        if data is None:
            os.makedirs(p, exist_ok=True)
        else:
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, 'wb') as f:
                f.write(data.encode('utf-8') if isinstance(data, str) else data)


def base_env(extra: dict[str, str], lad: str) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k.upper() not in PY_ENV}
    env.update(extra)
    env['LOCALAPPDATA'] = lad
    return env


def traceback_tail(err: str) -> str:
    """Python's traceback cut to its first and last lines (its frames name the Python files; bfbc2 prints the
    same exception without them: BENCHMARKS.md "O1")."""
    if err.startswith('Traceback (most recent call last):'):
        lines = err.split('\r\n')
        tail = [ln for ln in lines if ln and not ln.startswith(' ')]
        return '\r\n'.join(tail) + '\r\n'
    return err


def run(cmd: list[str], pl: Place, files: dict[str, Any], args: list[str], env: dict[str, str],
        merged: bool) -> tuple[dict[str, Any], float]:
    """One command line in a fresh case folder: {code, out, err, files, log} and its wall time."""
    setup(pl, files)
    argv = [a.replace('{d}', pl.d) for a in args]
    t = time.perf_counter()
    p = subprocess.run(cmd + argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT if merged else subprocess.PIPE,
                       env=base_env(env, pl.lad), cwd=SUITE)
    took = time.perf_counter() - t
    return {'code': p.returncode, 'out': traceback_tail(pl.norm(p.stdout)),
            'err': traceback_tail(pl.norm(p.stderr or b'')), 'files': cg.tree_digest(pl.d), 'log': pl.log()}, took


def commands(exe: str) -> dict[str, Callable[[str], list[str]]]:
    return {'rust': lambda tool: [exe, tool], 'python': lambda tool: [sys.executable, SCRIPTS[tool]]}


def run_all(rows: list[Case], sides: list[str], exe: str) -> tuple[dict[str, dict[str, Any]], dict[str, float]]:
    """Every case on each side, on WORKERS threads (each with its own folders): {side + key: result}, time per side."""
    places = [Place(k, 64) for k in range(WORKERS)]
    cmd = commands(exe)
    chunks = [rows[i::WORKERS] for i in range(WORKERS)]
    results: dict[str, dict[str, Any]] = {}
    times = dict.fromkeys(sides, 0.0)

    def worker(k: int) -> None:
        for key, tool, files, args, env, merged in chunks[k]:
            for side in sides:
                r, took = run(cmd[side](tool), places[k], files, args, env, merged)
                results[side + '|' + key] = r
                times[side] += took
    try:
        with ThreadPoolExecutor(WORKERS) as pool:
            list(pool.map(worker, range(WORKERS)))
    finally:
        for pl in places:
            shutil.rmtree(pl.root, ignore_errors=True)
    return results, times


def diff(want: dict[str, Any], got: dict[str, Any]) -> str:
    for k in ('code', 'out', 'err', 'files', 'log'):
        if want.get(k) != got.get(k):
            return f'{k}: {str(want.get(k))[-200:]!r} / {str(got.get(k))[-200:]!r}'
    return ''


# ------------------------------------------------------------------------------------------- the game (--game)

def opfor_calls(opfor: str, sample: int) -> list[tuple[str, str, list[str], str]]:
    """Project OPFOR's real FbrbTool command lines, read-only: (label, tool, args with {o} the output folder, what
    reads them). Outputs go to the scratch folder."""
    game = os.path.join(opfor, 'game')
    out: list[tuple[str, str, list[str], str]] = []
    menu = os.path.join(opfor, 'original', 'Package', 'menu', 'main', 'mainmenu-00.fbrb')
    if os.path.isfile(menu):         # build_menus.bat: unpack the shipped main menu, pack it again
        out.append(('build_menus.bat unpack', 'fbrbtool', ['unpack', menu, '{o}\\mainmenu'], 'pack'))
    startup = os.path.join(game, 'Package', 'async', 'startup-00.fbrb')
    if os.path.isfile(startup):      # merge_startup.ps1: unpack OPFOR's startup bundle, pack it again
        out.append(('merge_startup.ps1 unpack', 'fbrbtool', ['unpack', startup, '{o}\\ours'], 'pack'))
    for top in (('Package', 'levels'), ('Dist', 'win32', 'levels')):   # campaign_fill.ps1: sp_005's two bundles
        b = os.path.join(game, *top, 'sp_005', 'level-00.fbrb')
        if os.path.isfile(b):
            out.append((f'campaign_fill.ps1 unpack {top[0]}', 'fbrbtool', ['unpack', b, '{o}\\u'], ''))
    for b in tfb.logged_bundles(120, sample):        # the logged invocations (today's and yesterday's log)
        out.append((f'logged list {os.path.basename(os.path.dirname(b))}', 'fbrbtool', ['list', b], ''))
        out.append((f'logged unpack {os.path.basename(os.path.dirname(b))}', 'fbrbtool', ['unpack', b, '{o}\\u'], ''))
    return out


def game(check: Any, exe: str, sample: int) -> None:
    try:
        opfor: str | None = paths.find_opfor()
    except paths.PathsError:
        opfor = None
    if not opfor:
        print('     (no Project OPFOR BFBC2 copy: the game tier skipped)')
        return
    pl = Place(9, 64)
    cmd = commands(exe)
    bad: list[str] = []
    times = {'rust': 0.0, 'python': 0.0}
    runs = 0
    try:
        calls = opfor_calls(opfor, sample)
        for label, tool, args, then in calls:
            got: dict[str, Any] = {}
            for side in ('python', 'rust'):
                for merged in (False, True):
                    setup(pl, {})
                    argv = [a.replace('{o}', pl.d) for a in args]
                    t = time.perf_counter()
                    p = subprocess.run(cmd[side](tool) + argv, stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT if merged else subprocess.PIPE,
                                       env=base_env({}, pl.lad), cwd=SUITE)
                    times[side] += time.perf_counter() - t
                    r = {'code': p.returncode, 'out': pl.norm(p.stdout), 'err': pl.norm(p.stderr or b''),
                         'files': cg.tree_digest(pl.d), 'log': pl.log()}
                    if then == 'pack' and p.returncode == 0:      # the script's pack of the unpacked folder
                        folder = argv[2]
                        t = time.perf_counter()
                        q = subprocess.run(cmd[side](tool) + ['pack', folder, folder + '.fbrb'], capture_output=True,
                                           env=base_env({}, pl.lad), cwd=SUITE)
                        times[side] += time.perf_counter() - t
                        r['pack'] = [q.returncode, pl.norm(q.stdout), pl.norm(q.stderr), cg.sha1_file(folder + '.fbrb')
                                     if os.path.isfile(folder + '.fbrb') else 'none', pl.log()]
                    got[f'{side} {merged}'] = r
                    runs += 1 + ('pack' in r)
            if not label.startswith('logged') and got['python False']['code'] != 0:
                # OPFOR's scripts stop on a failed call: the replay must run as theirs do (a short temporary folder)
                bad.append(f'{label}: the Python command line failed here (exit code {got["python False"]["code"]}: '
                           f'{got["python False"]["err"][:160]!r}), not a replay of a working call')
            for merged in (False, True):
                why = diff(got[f'python {merged}'], got[f'rust {merged}'])
                if not why and got[f'python {merged}'].get('pack') != got[f'rust {merged}'].get('pack'):
                    why = 'the pack differs'
                if why:
                    bad.append(f'{label}{" (merged)" if merged else ""}: {why}')
        # DbxEdit on a corpus sample, copied out of OPFOR's work folders (the commands write beside their input)
        files = tdx.corpus_files(opfor)
        rnd = random.Random(11)
        picked = sorted(rnd.sample(files, min(12, len(files))))
        for f in picked:
            with open(f, 'rb') as fh:
                data = fh.read()
            for label, args in (('xml', ['xml', '{d}\\f.dbx']), ('roundtrip', ['roundtrip', '{d}\\f.dbx']),
                                ('check', ['check', '{d}\\f.dbx'])):
                res = {}
                for side in ('python', 'rust'):
                    r, took = run(cmd[side]('dbxedit'), pl, {'f.dbx': data}, args, {}, False)
                    times[side] += took
                    res[side] = r
                    runs += 1
                why = diff(res['python'], res['rust'])
                if why:
                    bad.append(f'dbxedit {label} {f}: {why}')
    finally:
        shutil.rmtree(pl.root, ignore_errors=True)
    print(f'     OPFOR\'s calls: {runs} runs, Python {times["python"]:.1f} s, Rust {times["rust"]:.1f} s')
    check(not bad, f'Project OPFOR\'s real calls, read-only ({len(calls)} FbrbTool lines: build_menus.bat, '
          f'merge_startup.ps1, campaign_fill.ps1, the logged invocations; 36 DbxEdit runs on corpus copies), each '
          f'alone and on one pipe: Rust answered every one as Python did', ' | '.join(bad[:3]))


# ------------------------------------------------------------------------------------------- main

def find_exe(argv: list[str]) -> str | None:
    if '--exe' in argv:
        return argv[argv.index('--exe') + 1]
    for profile in ('release', 'debug'):
        p = native.cli_path(profile)
        if os.path.isfile(p):
            return p
    return None


def main(argv: list[str]) -> int:
    exe = find_exe(argv)
    if exe is None or not os.path.isfile(exe):
        print('skipped: bfbc2.exe is not built (python rust\\build_native.py)')
        return 0
    if native.load() is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    rows = cases()
    t = time.time()
    if '--record' in argv:
        results, times = run_all(rows, ['python'], exe)
        cg.save(GOLDEN, {'meta': {'recorded': datetime.date.today().isoformat(), 'by': 'the Python command lines',
                                  'python': sys.version.split()[0], 'cases': len(rows)},
                         'cases': {k: results['python|' + k] for k, *_ in rows}})
        print(f'recorded {len(rows)} command lines from the Python ({times["python"]:.0f} s): {GOLDEN}')
        return 0
    sides = ['rust', 'python'] if '--live' in argv else ['rust']
    results, times = run_all(rows, sides, exe)
    golden = cg.load(GOLDEN).get('cases', {})
    bad, live_bad, missing = [], [], []
    for key, *_ in rows:
        got = results['rust|' + key]
        if key not in golden:
            missing.append(key)
        elif why := diff(golden[key], got):
            bad.append(f'{key}: {why}')
        if 'python' in sides and (why := diff(results['python|' + key], got)):
            live_bad.append(f'{key}: {why}')
    who = f'Rust answered {len(rows) - len(bad) - len(missing)} of {len(rows)} as recorded'
    check(not bad and not missing, f'{len(rows)} command lines through bfbc2.exe (FbrbTool\'s 228, DbxEdit\'s 82, the '
          f'encoding, buffering and environment cases; each alone and on one pipe): exit codes, both streams\' bytes, '
          f'files and log lines as the Python command lines answered ({GOLDEN}; {who})',
          ' | '.join(bad[:3] + [f'not recorded: {m}' for m in missing[:3]]))
    if 'python' in sides:
        check(not live_bad, f'the same {len(rows)} through the Python command lines now, both ways: the same answers '
              f'(Python {times["python"]:.0f} s, Rust {times["rust"]:.0f} s of process time)', ' | '.join(live_bad[:3]))
    if '--game' in argv:
        sample = int(argv[argv.index('--sample') + 1]) if '--sample' in argv else 12
        game(check, exe, sample)
    print(f'\n{"ALL PASSED" if not failed else f"{failed} FAILED"}  ({time.time() - t:.0f} s)')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
