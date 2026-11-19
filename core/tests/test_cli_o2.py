r"""Round O2's command lines in Rust (rust\bc2-cli, bfbc2.exe: aptcheck, aptrefs, bc2loc; exeref, crashreader;
hpdtool, hkpack, convert, aimanim) against the Python command lines they port, as processes: the same exit codes,
the same bytes on stdout and stderr (each alone, and both on one pipe), the same files and the same log records
(rust/BENCHMARKS.md, "O2"). The method is round O1's (test_cli_o1.py).

  python core\tests\test_cli_o2.py            quick: every recorded command line through bfbc2.exe, against what the
                                              Python command lines answered (cli_o2_<group>_golden*.json)
  python core\tests\test_cli_o2.py --live     + the Python command lines run again beside it, both ways compared
  python core\tests\test_cli_o2.py --game     + Project OPFOR's real calls replayed read-only (inputs copied to a
                                              temporary folder), both ways, timed
  python core\tests\test_cli_o2.py --record   record the goldens from the Python command lines (before the Python is
                                              retired: the oracle afterwards)
     --group menu|crash|nav   one group only (default: all three)
     --who                    list the lines bfbc2.exe handed to the Python
     --exe <bfbc2.exe>        the binary (default: native.find_binary - bin\bfbc2.exe, else the build folder)
     --ref <suite folder>     the suite copy whose Python command lines answer (default: BFBC2_O2_REF, else this
                              checkout): while the Rust sources change, a copy of the checkout before them keeps a
                              Rust core module that matches its sources

Who answered: bfbc2.exe writes "<tool> rust" or "<tool> python" (a hand-off to the Python command line, where the
Rust core refuses) to BFBC2_CLI_TRACE; each line's answerer is counted.

The cases come from cli_o2_menu.py, cli_o2_crash.py and cli_o2_nav.py (beside this file): each has cases() (the
quick tier's command lines, on synthetic inputs made in the case folder: no game data) and opfor_calls(opfor) (the
--game tier's). Compared: the exit code; stdout and stderr with the case folder, the log folder and the suite named
(<dir>, <logs>, <cwd>); a Python traceback cut to its first and last lines; the case folder's files; the log records'
first lines with their stamps and the start line's "(Python 3.x, win32)" / "(bfbc2.exe, win32)" taken out (the
traceback under an error record is Python's own).

Skipped (exit 0) without the binary or without the Rust core module (the Python command lines need it).
"""
from __future__ import annotations

import datetime
import hashlib
import importlib
import json
import os
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

GROUPS: tuple[str, ...] = ('menu', 'crash', 'nav')
ROUND = 'o2'                            # the case modules cli_<ROUND>_<group>.py, goldens cli_<ROUND>_<group>_golden*
_ref_arg = sys.argv[sys.argv.index('--ref') + 1] if '--ref' in sys.argv[:-1] else None
REF = os.path.abspath(_ref_arg or os.environ.get('BFBC2_O2_REF') or SUITE)
SCRIPTS = native.CLI_TOOLS
# the environment Python's stdio reads, and the switches that pick the Python, left out unless a case sets them
PY_ENV = ('PYTHONUTF8', 'PYTHONIOENCODING', 'PYTHONUNBUFFERED', 'PYTHONLEGACYWINDOWSSTDIO', 'BFBC2_PURE_PYTHON',
          'BFBC2_CLI_PYTHON', 'BFBC2_CLI_TRACE', 'BFBC2_CRASHREADER_REFERENCE')
WORKERS = int(os.environ.get('BFBC2_CLI_WORKERS') or 4)     # fewer on a busy PC (round O3)
SHARD = 900_000                        # bytes of JSON per golden file at most
# key, tool, files ({name: bytes | str | None (a folder)}), args ({d}: the case folder), env ({d} too), merged
Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]


def group_module(group: str) -> Any:
    return importlib.import_module(f'cli_{ROUND}_{group}')


# ------------------------------------------------------------------------------------------- running one

class Place:
    """One worker's folders: the case folder (64 characters long, as O1's), its own LOCALAPPDATA (each run's log is
    its alone) and the trace file."""

    def __init__(self, k: int, dir_len: int = 64) -> None:
        self.root, self.d = cg.case_root(dir_len, f'cli_{ROUND}_{k}_')
        self.lad = os.path.join(self.root, 'lad')
        self.logs = os.path.join(self.lad, 'bfbc2-tool-suite', 'logs')
        self.trace = os.path.join(self.root, 'trace.txt')

    def norm(self, data: bytes) -> str:
        """Printed bytes with the folders named (in every spelling Python writes them: as they are, in a repr's
        doubled backslashes), the log file's name and temporary files' numbers taken out; as Latin-1 text."""
        places = sorted(((self.logs, '<logs>'), (self.d, '<dir>'), (self.lad, '<lad>'), (SUITE, '<cwd>'),
                         (REF, '<cwd>')),
                        key=lambda p: -len(p[0]))
        for p, name in places:
            spellings = set()
            for q in (p, p.replace('\\', '\\\\'), p.replace('\\', '/')):
                spellings |= {q.encode('utf-8'), q.encode('cp1252', 'backslashreplace')}
            for s in sorted(spellings, key=len, reverse=True):
                data = data.replace(s, name.encode('ascii'))
        data = re.sub(rb'<logs>\\[^\s\\]+\.log', b'<log>', data)
        data = re.sub(rb'\.tmp-\d+(-\d+)?', b'.tmp-N', data)
        return data.decode('latin-1')

    def files(self) -> str:
        """cg.tree_digest of the case folder with each file's bytes normalized as norm() does first (outputs that
        name the case folder, e.g. aptrefs' JSON, do not depend on the temporary folder; a file that names none
        digests as before)."""
        snap: dict[str, str] = {}
        for d, ds, fs in os.walk(self.d):
            if not ds and not fs and d != self.d:
                snap[os.path.relpath(d, self.d) + os.sep] = '<dir>'
            for f in fs:
                with open(os.path.join(d, f), 'rb') as fh:
                    data = fh.read()
                text = self.norm(data)
                data = text.encode('latin-1') if '<dir>' in text or '<cwd>' in text or '<lad>' in text else data
                rel = os.path.relpath(os.path.join(d, f), self.d)
                # round O3: a group's LOOSE files (a cache whose hash names the case folder) by size only
                loose = any(re.search(p, rel.replace(os.sep, '/')) for p in LOOSE)
                snap[rel] = f'{len(data)} bytes' if loose else hashlib.sha1(data).hexdigest()
        return cg.digest(''.join(f'{k}\t{v}\n' for k, v in sorted(snap.items())))

    def log(self) -> str:
        """The log records the run appended: each record's first line, the stamp and the start line's runtime taken
        out; its files' names by tool and part."""
        out = []
        if os.path.isdir(self.logs):
            # left out: the start-up pruning's lock file (caches.prune_logs: the Python tools prune, bfbc2.exe leaves
            # it to them) and the ResourceWarnings errors.setup logs for files Python itself left unclosed
            for n in sorted(x for x in os.listdir(self.logs) if not x.startswith('.')):
                with open(os.path.join(self.logs, n), 'rb') as f:
                    text = f.read()
                rows = re.findall(rb'(?m)^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d,\d{3} ([^\r\n]*)', text)
                rows = [r for r in rows if not (r.startswith(b'WARNING ') and b'ResourceWarning: unclosed file' in r)]
                rows = [re.sub(rb' started \((Python [^,]*|bfbc2\.exe), win32\) ---$', b' started (<who>) ---', r)
                        for r in rows]
                name = re.sub(r'\d{4}-\d\d-\d\d', '<date>', n)
                out.append(f'[{name}]\n' + self.norm(b'\n'.join(rows) + b'\n'))
        return ''.join(out)

    def lad_state(self) -> str:
        """Round O3: what the run left in its local app data besides the log records (log() compares those): every
        file's name (a log by name only, the day's date as <date>; an archive of pruned logs by its members: name,
        size, CRC, time - the deflate bytes are zlib's in Python and zlib-rs' in Rust; its name's stamp as <stamp>),
        other files by their bytes."""
        import zipfile
        rows = []
        for d, _ds, fs in os.walk(self.lad):
            for f in fs:
                full = os.path.join(d, f)
                rel = os.path.relpath(full, self.lad).replace(os.sep, '/')
                rel = re.sub(r'logs-\d{4}-\d\d-\d\d_\d{6}(-\d+)?\.zip$', 'logs-<stamp>.zip', rel)
                rel = re.sub(r'\.tmp-\d+$', '.tmp-N', rel)
                if f.lower().endswith('.log'):
                    rows.append(re.sub(datetime.date.today().isoformat(), '<date>', rel))
                elif f.lower().endswith('.zip'):
                    try:
                        with zipfile.ZipFile(full) as z:
                            members = [f'{i.filename}:{i.file_size}:{i.CRC:08x}:{i.date_time}' for i in z.infolist()]
                        rows.append(f'{rel} [' + ', '.join(members) + ']')
                    except (OSError, zipfile.BadZipFile) as e:
                        rows.append(f'{rel} (unreadable: {type(e).__name__})')
                else:
                    with open(full, 'rb') as fh:
                        rows.append(f'{rel} {hashlib.sha1(fh.read()).hexdigest()}')
        return '\n'.join(sorted(rows))

    def who(self) -> str:
        """Who answered the last run: 'rust', 'python' (bfbc2.exe handed it off) or '' (a Python command line)."""
        try:
            with open(self.trace, encoding='utf-8') as f:
                lines = f.read().split()
            return lines[-1] if lines else ''
        except OSError:
            return ''


def setup(pl: Place, files: dict[str, Any]) -> None:
    cg.fresh(pl.d)
    shutil.rmtree(pl.lad, ignore_errors=True)
    if os.path.exists(pl.trace):
        os.remove(pl.trace)
    for name, data in files.items():
        # round O3: '{lad}/...' is a file of the run's local app data (old logs, caches), not of the case folder
        p = os.path.join(pl.lad, name[6:]) if name.startswith('{lad}/') else os.path.join(pl.d, name.replace('{d}', pl.d))
        if data is None:
            os.makedirs(p, exist_ok=True)
        else:
            mtime = None
            if isinstance(data, tuple):             # (contents, modification time): os.utime after writing
                data, mtime = data
            if callable(data):                      # round O3: contents made from the case folder (a cache naming it)
                data = data(pl.d)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, 'wb') as f:
                f.write(data.encode('utf-8') if isinstance(data, str) else data)
            if mtime is not None:
                os.utime(p, (mtime, mtime))


def base_env(extra: dict[str, str], pl: Place) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k.upper() not in PY_ENV}
    env.update({k: v.replace('{d}', pl.d).replace('{lad}', pl.lad) for k, v in extra.items()})
    env['LOCALAPPDATA'] = pl.lad
    env['BFBC2_CLI_TRACE'] = pl.trace
    env['BFBC2_CLI_SUITE'] = REF                # a hand-off runs the reference's .py
    return env


def traceback_tail(err: str) -> str:
    """Python's traceback cut to its first and last lines (its frames name the Python files; bfbc2 prints the
    same exception without them)."""
    header = 'Traceback (most recent call last):'
    if header not in err:
        return err
    head, _, tb = err.partition(header)
    lines = tb.split('\r\n')[1:]
    chain = ('The above exception was the direct cause of the following exception:',
             'During handling of the above exception, another exception occurred:')
    i = 0
    while True:
        while i < len(lines) and lines[i].startswith(' '):
            i += 1
        last = lines[i] if i < len(lines) else ''
        i += 1
        if i + 3 < len(lines) and lines[i] == '' and lines[i + 1] in chain and lines[i + 2] == '' \
                and lines[i + 3] == header:
            i += 4
            continue
        break
    return head + header + '\r\n' + last + '\r\n' + '\r\n'.join(lines[i:])


# round O3: a group's own (pattern, replacement) pairs over both streams (its NORMS: a time the tool prints)
NORMS: list[tuple[str, str]] = []
# round O3: a group's files compared by size only (its LOOSE: path patterns)
LOOSE: list[str] = []


def normed(text: str) -> str:
    for pat, rep in NORMS:
        text = re.sub(pat, rep, text)
    return text


def run(cmd: list[str], pl: Place, files: dict[str, Any], args: list[str], env: dict[str, str],
        merged: bool) -> tuple[dict[str, Any], float, str]:
    """One command line in a fresh case folder: ({code, out, err, files, log}, its wall time, who answered)."""
    setup(pl, files)
    argv = [a.replace('{d}', pl.d).replace('{lad}', pl.lad) for a in args]
    t = time.perf_counter()
    p = subprocess.run(cmd + argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT if merged else subprocess.PIPE,
                       env=base_env(env, pl), cwd=SUITE)
    took = time.perf_counter() - t
    return {'code': p.returncode, 'out': normed(traceback_tail(pl.norm(p.stdout))),
            'err': normed(traceback_tail(pl.norm(p.stderr or b''))), 'files': pl.files(), 'log': pl.log(),
            **({'lad': pl.lad_state()} if ROUND != 'o2' else {})}, \
        took, pl.who()


def commands(exe: str) -> dict[str, Callable[[str], list[str]]]:
    return {'rust': lambda tool: [exe, tool],
            'python': lambda tool: [sys.executable, os.path.join(REF, os.path.relpath(SCRIPTS[tool], SUITE))]}


def run_all(rows: list[Case], sides: list[str], exe: str
            ) -> tuple[dict[str, dict[str, Any]], dict[str, float], dict[str, str]]:
    """Every case on each side, on WORKERS threads: {side|key: result}, time per side, {key: who answered (Rust's
    side)}."""
    places = [Place(k) for k in range(WORKERS)]
    cmd = commands(exe)
    chunks = [rows[i::WORKERS] for i in range(WORKERS)]
    results: dict[str, dict[str, Any]] = {}
    times = dict.fromkeys(sides, 0.0)
    who: dict[str, str] = {}

    def worker(k: int) -> None:
        for key, tool, files, args, env, merged in chunks[k]:
            for side in sides:
                r, took, w = run(cmd[side](tool), places[k], files, args, env, merged)
                results[side + '|' + key] = r
                times[side] += took
                if side == 'rust':
                    who[key] = w
    try:
        with ThreadPoolExecutor(WORKERS) as pool:
            list(pool.map(worker, range(WORKERS)))
    finally:
        for pl in places:
            shutil.rmtree(pl.root, ignore_errors=True)
    return results, times, who


def diff(want: dict[str, Any], got: dict[str, Any]) -> str:
    for k in ('code', 'out', 'err', 'files', 'log', 'lad'):
        if want.get(k) != got.get(k):
            return f'{k}: {str(want.get(k))[-300:]!r} / {str(got.get(k))[-300:]!r}'
    return ''


# ------------------------------------------------------------------------------------------- goldens (sharded)

def golden_names(group: str) -> list[str]:
    base = f'cli_{ROUND}_{group}_golden'
    names = sorted(n for n in os.listdir(HERE) if re.fullmatch(base + r'(_\d+)?\.json', n))
    return names


def load_golden(group: str) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for n in golden_names(group):
        out.update(cg.load(n).get('cases', {}))
    return out


def save_golden(group: str, cases: dict[str, Any], meta: dict[str, Any]) -> list[str]:
    for old in golden_names(group):
        os.remove(cg.path(old))
    shards: list[dict[str, Any]] = [{}]
    size = 0
    for k, v in cases.items():
        n = len(json.dumps(k)) + len(json.dumps(v, ensure_ascii=False).encode('utf-8')) + 4
        if size + n > SHARD and shards[-1]:
            shards.append({})
            size = 0
        shards[-1][k] = v
        size += n
    names = []
    for i, part in enumerate(shards):
        name = f'cli_{ROUND}_{group}_golden.json' if len(shards) == 1 else f'cli_{ROUND}_{group}_golden_{i}.json'
        cg.save(name, {'meta': dict(meta, shard=i, shards=len(shards)), 'cases': part})
        names.append(name)
    return names


# ------------------------------------------------------------------------------------------- the game (--game)

def game(check: Callable[..., None], exe: str, group: str, mod: Any) -> dict[str, Any]:
    """Project OPFOR's real calls of the group, read-only: each a (label, tool, args, files, env, must_succeed) of
    the group's opfor_calls (files: {name in the case folder: a source file copied there}); each alone and on one
    pipe, both ways."""
    try:
        opfor: str | None = paths.find_opfor()
    except paths.PathsError:
        opfor = None
    if not opfor:
        print('     (no Project OPFOR BFBC2 copy: the game tier skipped)')
        return {}
    pl = Place(9 + GROUPS.index(group))
    cmd = commands(exe)
    bad: list[str] = []
    times = {'rust': 0.0, 'python': 0.0}
    runs = 0
    answered = {'rust': 0, 'python': 0}
    try:
        calls = mod.opfor_calls(opfor)
        for label, tool, args, sources, env, must in calls:
            files = {}
            for name, src in sources.items():
                if isinstance(src, tuple):              # (contents, modification time) made by the group
                    files[name] = src
                    continue
                with open(src, 'rb') as f:
                    files[name] = (f.read(), os.path.getmtime(src))     # the copy keeps the time (crashreader)
            got: dict[str, Any] = {}
            for side in ('python', 'rust'):
                for merged in (False, True):
                    r, took, w = run(cmd[side](tool), pl, files, args, env, merged)
                    times[side] += took
                    runs += 1
                    got[f'{side} {merged}'] = r
                    if side == 'rust' and not merged:
                        answered[w or 'python'] += 1
            if must and got['python False']['code'] != 0:
                bad.append(f'{label}: the Python command line failed here (exit code {got["python False"]["code"]}: '
                           f'{(got["python False"]["err"] or got["python False"]["out"])[:160]!r}), not a replay of '
                           'a working call')
            for merged in (False, True):
                why = diff(got[f'python {merged}'], got[f'rust {merged}'])
                if why:
                    bad.append(f'{label}{" (merged)" if merged else ""}: {why}')
    finally:
        shutil.rmtree(pl.root, ignore_errors=True)
    print(f'     {group}: OPFOR\'s calls: {runs} runs, Python {times["python"]:.1f} s, Rust {times["rust"]:.1f} s; '
          f'answered by Rust {answered["rust"]}, handed to Python {answered["python"]}')
    check(not bad, f'{group}: Project OPFOR\'s real calls, read-only ({len(calls)} command lines), each alone and on '
          f'one pipe: Rust answered every one as Python did', ' | '.join(bad[:3]))
    return {'calls': len(calls), 'times': times, 'answered': answered}


# ------------------------------------------------------------------------------------------- main

def find_exe(argv: list[str]) -> str | None:
    if '--exe' in argv:
        return argv[argv.index('--exe') + 1]
    return native.find_binary('bfbc2')


def main(argv: list[str]) -> int:
    exe = find_exe(argv)
    if exe is None or not os.path.isfile(exe):
        print('skipped: bfbc2.exe is not built (python rust\\build_native.py)')
        return 0
    if REF == SUITE and native.load() is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    groups = [argv[argv.index('--group') + 1]] if '--group' in argv else list(GROUPS)
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    t = time.time()
    for group in groups:
        mod = group_module(group)
        NORMS[:] = list(getattr(mod, 'NORMS', ()))
        LOOSE[:] = list(getattr(mod, 'LOOSE', ()))
        rows: list[Case] = mod.cases()
        keys = [r[0] for r in rows]
        if len(set(keys)) != len(keys):
            check(False, f'{group}: case keys are unique', str([k for k in keys if keys.count(k) > 1][:3]))
            continue
        if '--record' in argv:
            results, times, _who = run_all(rows, ['python'], exe)
            names = save_golden(group, {k: results['python|' + k] for k in keys},
                                {'recorded': datetime.date.today().isoformat(), 'by': 'the Python command lines',
                                 'python': sys.version.split()[0], 'cases': len(rows)})
            print(f'{group}: recorded {len(rows)} command lines from the Python ({times["python"]:.0f} s): '
                  f'{", ".join(names)}')
            continue
        sides = ['rust', 'python'] if '--live' in argv else ['rust']
        results, times, who = run_all(rows, sides, exe)
        golden = load_golden(group)
        bad, live_bad, missing = [], [], []
        for key in keys:
            got = results['rust|' + key]
            if key not in golden:
                missing.append(key)
            elif why := diff(golden[key], got):
                bad.append(f'{key}: {why}')
            if 'python' in sides and (why := diff(results['python|' + key], got)):
                live_bad.append(f'{key}: {why}')
        tools = sorted({r[1] for r in rows})
        per_tool = []
        for tool in tools:
            ks = [r[0] for r in rows if r[1] == tool]
            r_n = sum(who.get(k) == 'rust' for k in ks)
            per_tool.append(f'{tool} {len(ks)} (Rust {r_n}, Python {len(ks) - r_n})')
            if '--who' in argv:                          # the lines bfbc2.exe handed to the Python
                for k in ks:
                    if who.get(k) != 'rust':
                        print(f'     handed to Python: {k}')
        check(not bad and not missing, f'{group}: {len(rows)} command lines through bfbc2.exe ({"; ".join(per_tool)}): '
              'exit codes, both streams\' bytes, files and log records as the Python command lines answered '
              f'({len(rows) - len(bad) - len(missing)} of {len(rows)} as recorded)',
              ' | '.join(bad[:3] + [f'not recorded: {m}' for m in missing[:3]]))
        if 'python' in sides:
            check(not live_bad, f'{group}: the same {len(rows)} through the Python command lines now, both ways: the '
                  f'same answers (Python {times["python"]:.0f} s, Rust {times["rust"]:.0f} s of process time)',
                  ' | '.join(live_bad[:3]))
        if '--game' in argv:
            game(check, exe, group, mod)
    print(f'\n{"ALL PASSED" if not failed else f"{failed} FAILED"}  ({time.time() - t:.0f} s)')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
