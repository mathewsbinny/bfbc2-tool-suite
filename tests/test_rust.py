r"""The Rust core's own checks (rust\): formatting, clippy with warnings as errors, and every crate's tests.

  python tests\test_rust.py               all of it in turn: fmt, clippy, build, the quick group (by hand; CI's
                                          native.yml)
  python tests\test_rust.py fmt           cargo fmt --all --check
  python tests\test_rust.py clippy        cargo clippy --all-targets -D warnings on the crates not green (below)
  python tests\test_rust.py build         cargo test --workspace --no-run, then the doc tests; records the test
                                          binaries and the sources they were built from (binaries_file(), in the
                                          checkout's build folder)
  python tests\test_rust.py run           the quick group: every test that is not #[ignore]
  python tests\test_rust.py run --game    the game group: the tests marked #[ignore = "game tier: ..."] (the
                                          whole-level editor, levelwrite and render tests, round TS1); builds first
                                          when the recorded binaries are not this checkout's
  python tests\test_rust.py list          every test with the tier that runs it (quick / game / corpus); a test
                                          ignored for another reason runs in no tier and fails the list
  python tests\test_rust.py --corpus      game: the corpus tests in Rust, release build (every unpacked .dbx of the
                                          game copy against DbxEdit's fingerprints; every bundle of the game copy and
                                          work\ww2 extracted)
  --no-skip    run every crate, green or not (also BFBC2_RUST_NO_SKIP=1, which run_tests.py full sets, and CI)
  --jobs N     run: N test binaries side by side (default 2)

Skipped (exit 0) when Rust is not installed: the suite then runs on the Python readers.

Round TS1 (audit AU3 TS-1 to TS-7). run_tests.py runs fmt, clippy, build and the two groups as entries of their own,
so the test run overlaps the Python entries instead of one entry holding every CPU for most of the tier.

Green crates. A crate's fingerprint covers its folder (sources, tests, test data), the folders of the workspace crates
it depends on (its own dev-dependencies included), the files they include_str! / include_bytes! from outside them,
the workspace's manifests, lock, toolchain and lint settings, the suite's VERSION, this script and the build's
environment (RUSTFLAGS, CARGO_PROFILE_*, ...). After a green clippy or quick run each crate's fingerprint is recorded
for that step in RUST_GREEN (%LOCALAPPDATA%\bfbc2-tool-suite\rust_green.json, merged under a lock between processes
and replaced atomically: runners in several worktrees write it at once). The next run on this PC leaves out a crate
whose fingerprint is recorded green, and says so. Never on CI, never with --no-skip or in the full tier, and never for
the game group (it reads the game copy, which the fingerprint cannot see). Within one build folder cargo itself
recompiles only what changed; across worktrees the compile cache (README, "The Rust core") serves the rest.

Times. Every test's own time comes from libtest's JSON events (--report-time; stable libtest takes them with
RUSTC_BOOTSTRAP=1, set for the test processes only: cargo never sees it, so no build is invalidated) and is kept per
test in RUST_TIMES (%LOCALAPPDATA%\bfbc2-tool-suite\rust_test_times.json); the slowest are printed after each run. In
the game group the gated binaries (bc2-editor, bc2-levelwrite, bc2-render, whose gate() lets two tests run at once)
run with --test-threads 2, so a test's time is its own and not its wait; in the quick group a gated test's time
includes its wait for gate(). A binary still running after BINARY_LIMIT is ended, the tests that started and never
finished named.

Skips. Each test's output (--show-output) is read: a line saying "skipped" (a level or a graphics adapter not on this
PC) is a part skipped for want of data, counted and named per test (audit TS-7: the "<level>: skipped, ..." lines
escaped the old count). The map editor's evidence lines (test_editor.KEEP) are printed as test_editor.py printed them.
"""
from __future__ import annotations

import concurrent.futures
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib
from typing import Any

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('rust', 'core')
import build_native  # noqa: E402
import test_editor  # noqa: E402  (its KEEP: the editor's evidence lines)
from bfbc2core import atomicio, native  # noqa: E402

RUST = os.path.join(SUITE, 'rust')
STATE = os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite')
RUST_GREEN = os.path.join(STATE, 'rust_green.json')
RUST_TIMES = os.path.join(STATE, 'rust_test_times.json')
NO_SKIP_ENV = 'BFBC2_RUST_NO_SKIP'
GAME_TIER = 'game tier'                 # #[ignore = "game tier: <why>"]: run by `run --game` (run_tests.py game)
CORPUS = {('bc2-formats', 'corpus'), ('bc2-formats', 'fbrb_corpus')}     # their ignored tests: --corpus (release)
GATED = {'bc2-editor', 'bc2-levelwrite', 'bc2-render'}                   # integration binaries whose tests take gate()
# one test binary's time limit: a hung test ends its binary with the tests still running named, before run_tests.py's
# limit ends the whole entry without them
BINARY_LIMIT = {'quick': 900.0, 'game': 3 * 3600.0}
KEPT_PER_KEY = 40                      # green fingerprints kept per step and crate (several worktrees and branches)
# a part skipped for want of data: "skipped: no game copy", "mp_005: skipped, not prepared", "parity skipped: ...";
# not a count of none ("... identical, 0 skipped")
SKIP = re.compile(r'(?<!\b0 )\bskipped\b', re.I)
ROOT_FILES = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'clippy.toml', 'rustfmt.toml', 'app_icon.rs')
ENV_KEYS = ('RUSTFLAGS', 'RUSTDOCFLAGS', 'CARGO_BUILD_RUSTFLAGS')     # not the compile cache: it changes no output
_INCLUDE = re.compile(rb'include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)')


def binaries_file() -> str:
    """The build's record of its test binaries, in this checkout's build folder."""
    return os.path.join(native.target_dir(), 'bfbc2-test-binaries.json')


# --- the crates and their fingerprints ------------------------------------------------------------------------------

def _manifest(path: str) -> dict[str, Any]:
    try:
        with open(path, 'rb') as f:
            return tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError):
        return {}


def members(rust: str = RUST) -> list[str]:
    """The workspace's crate folders (names under rust\\)."""
    ws = _manifest(os.path.join(rust, 'Cargo.toml')).get('workspace') or {}
    return sorted(str(m) for m in ws.get('members', []))


def package_name(crate: str, rust: str = RUST) -> str:
    return str((_manifest(os.path.join(rust, crate, 'Cargo.toml')).get('package') or {}).get('name', crate))


def closure(crate: str, rust: str = RUST) -> list[str]:
    """crate and every workspace crate it depends on, directly or not: normal, build and target-specific
    dependencies, and the crate's own dev-dependencies (its tests link them; a dependency's dev-dependencies do not
    reach it)."""
    root = _manifest(os.path.join(rust, 'Cargo.toml'))
    ws = {name: str(v['path']) for name, v in ((root.get('workspace') or {}).get('dependencies') or {}).items()
          if isinstance(v, dict) and 'path' in v}
    seen: set[str] = set()
    todo = [(crate, True)]
    while todo:
        c, dev = todo.pop()
        if c in seen:
            continue
        seen.add(c)
        m = _manifest(os.path.join(rust, c, 'Cargo.toml'))
        tables = list(native._dep_tables(m))
        if dev:
            tables.append(m.get('dev-dependencies') or {})
            tables += [t.get('dev-dependencies') or {} for t in (m.get('target') or {}).values() if isinstance(t, dict)]
        for table in tables:
            for name, spec in table.items():
                if not isinstance(spec, dict):
                    continue
                if spec.get('workspace'):
                    path = ws.get(str(spec.get('package', name)))
                    full = os.path.join(rust, path) if path else None
                else:
                    full = os.path.join(rust, c, str(spec['path'])) if 'path' in spec else None
                if full:
                    rel = os.path.relpath(os.path.normpath(full), rust)
                    if os.sep not in rel and os.path.isfile(os.path.join(rust, rel, 'Cargo.toml')):
                        todo.append((rel, False))
    return sorted(seen)


def _files(folder: str) -> list[str]:
    out = []
    for top, dirs, names in os.walk(folder):
        dirs[:] = sorted(d for d in dirs if d not in ('target', '__pycache__'))
        out += [os.path.join(top, n) for n in sorted(names) if not n.endswith('.pyc')]
    return out


def crate_inputs(crate: str, rust: str = RUST) -> list[str]:
    """Every file whose change can change crate's clippy or test results (see the module's text)."""
    out = [os.path.join(rust, n) for n in ROOT_FILES]
    out.append(os.path.join(os.path.dirname(os.path.abspath(rust)), 'VERSION'))
    out.append(os.path.abspath(__file__))
    for c in closure(crate, rust):
        files = _files(os.path.join(rust, c))
        out += files
        for p in files:
            if not p.endswith('.rs'):
                continue
            try:
                with open(p, 'rb') as f:
                    text = f.read()
            except OSError:
                continue
            for m in _INCLUDE.finditer(text):
                target = os.path.normpath(os.path.join(os.path.dirname(p), m.group(1).decode('utf-8', 'replace')))
                out += _files(target) if os.path.isdir(target) else [target]
    return sorted({p for p in out if os.path.isfile(p)})


def crate_fingerprint(crate: str, step: str, rust: str = RUST, env: dict[str, str] | None = None) -> str:
    """crate's fingerprint for step ('clippy', 'tests'): crate_inputs (line endings normalised, as
    bfbc2core.native.fingerprint) and the build's environment."""
    env = dict(os.environ) if env is None else env
    h = hashlib.sha256(f'{step}\0'.encode())
    for k in sorted(k for k in env if k in ENV_KEYS or k.startswith('CARGO_PROFILE_')):
        h.update(f'{k}={env[k]}\0'.encode())
    base = os.path.dirname(os.path.abspath(rust))
    for p in crate_inputs(crate, rust):
        with open(p, 'rb') as f:
            data = f.read().replace(b'\r\n', b'\n')
        h.update(f'{os.path.relpath(p, base).replace(os.sep, "/")}\0{len(data)}\0'.encode())
        h.update(data)
    return h.hexdigest()


# --- green fingerprints ----------------------------------------------------------------------------------------------

def may_skip(argv: list[str]) -> bool:
    """Green crates may be left out: not with --no-skip, BFBC2_RUST_NO_SKIP=1 (run_tests.py full) or on CI."""
    return not ('--no-skip' in argv or os.environ.get(NO_SKIP_ENV, '') not in ('', '0') or os.environ.get('CI'))


def green_since(step: str, crate: str, fp: str, path: str | None = None) -> str | None:
    """When crate's fingerprint fp was last recorded green for step on this PC, or None."""
    got = atomicio.read_json(path or RUST_GREEN, {})
    when = (got.get(f'{step}:{crate}') or {}).get(fp) if isinstance(got, dict) else None
    return str(when) if when else None


def mark_green(step: str, fps: dict[str, str], path: str | None = None) -> None:
    """Records crate -> fingerprint green for step: read, merged and replaced under a lock between processes (runners
    in other worktrees write the same file), the newest KEPT_PER_KEY fingerprints per crate kept."""
    if not fps:
        return
    path = path or RUST_GREEN
    stamp = time.strftime('%Y-%m-%d %H:%M:%S')
    with atomicio.locked(path + '.lock'):
        got = atomicio.read_json(path, {})
        got = got if isinstance(got, dict) else {}
        for crate, fp in fps.items():
            seen = dict(got.get(f'{step}:{crate}') or {})
            seen[fp] = stamp
            got[f'{step}:{crate}'] = dict(sorted(seen.items(), key=lambda kv: str(kv[1]))[-KEPT_PER_KEY:])
        atomicio.write_json(path, got, indent=1, sort_keys=True)


# --- cargo ----------------------------------------------------------------------------------------------------------

def ascii_text(s: str) -> str:
    return s.encode('ascii', errors='backslashreplace').decode('ascii')


def cargo(env: dict[str, str], *args: str) -> subprocess.CompletedProcess[str]:
    exe = shutil.which('cargo', path=env.get('PATH')) or 'cargo'    # Windows looks it up on this process's PATH
    return subprocess.run([exe, *args], cwd=RUST, env=env, capture_output=True, text=True, encoding='utf-8',
                          errors='replace')


def tail(out: str, n: int = 40) -> None:
    print(ascii_text('\n'.join(out.strip().splitlines()[-n:])))


def step_fmt(env: dict[str, str]) -> int:
    r = cargo(env, 'fmt', '--all', '--check')
    print(f'{"ok  " if r.returncode == 0 else "FAIL"} cargo fmt --check')
    if r.returncode:
        tail(r.stdout + r.stderr)
    return r.returncode


def step_clippy(env: dict[str, str], argv: list[str]) -> int:
    crates = members()
    fps = {c: crate_fingerprint(c, 'clippy', env=env) for c in crates}      # before the run: an edit meanwhile is new
    skip = may_skip(argv)
    todo = [c for c in crates if not (skip and green_since('clippy', c, fps[c]))]
    if not todo:
        print(f'ok   cargo clippy --all-targets -D warnings: all {len(crates)} crates unchanged since their last green '
              f'clippy on this PC')
        return 0
    # -p for a subset: only bc2-cli has features of its own, so a crate is linted alike alone or in the workspace
    which = ['--workspace'] if len(todo) == len(crates) else [a for c in todo for a in ('-p', package_name(c))]
    t0 = time.time()
    r = cargo(env, 'clippy', *which, '--all-targets', '--', '-D', 'warnings')
    left = len(crates) - len(todo)
    note = f'; {left} unchanged since their last green clippy, left out' if left else ''
    print(f'{"ok  " if r.returncode == 0 else "FAIL"} cargo clippy --all-targets -D warnings: {len(todo)} crate(s) '
          f'({time.time() - t0:.0f} s{note})')
    if r.returncode:
        tail(r.stdout + r.stderr)
        return 1
    mark_green('clippy', {c: fps[c] for c in todo})
    return 0


def build(env: dict[str, str]) -> tuple[int, str]:
    """cargo test --workspace --no-run; the test binaries recorded in binaries_file() with each crate's fingerprint as
    built (taken before the build: an edit during it makes the record stale, not wrong). (exit code, cargo's
    messages)."""
    fps = {package_name(c): crate_fingerprint(c, 'tests', env=env) for c in members()}
    rec = binaries_file()
    if os.path.isfile(rec):
        os.remove(rec)                          # a failed build leaves no record: the run refuses old binaries
    r = cargo(env, 'test', '--workspace', '--no-run', '--message-format=json-render-diagnostics')
    if r.returncode:
        return r.returncode, r.stderr
    bins = []
    for line in r.stdout.splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if m.get('reason') == 'compiler-artifact' and m.get('executable') and (m.get('profile') or {}).get('test'):
            folder = os.path.dirname(m['manifest_path'])
            bins.append({'crate': package_name(os.path.basename(folder)), 'target': m['target']['name'],
                         'kind': m['target']['kind'][0], 'exe': m['executable'], 'cwd': folder})
    atomicio.write_json(rec, {'fingerprints': fps, 'binaries': bins}, indent=1)
    return 0, r.stderr


def step_build(env: dict[str, str], argv: list[str]) -> int:
    crates = {package_name(c): c for c in members()}
    if may_skip(argv):
        fps = {n: crate_fingerprint(f, 'tests', env=env) for n, f in crates.items()}
        if all(green_since('tests-quick', n, fp) for n, fp in fps.items()):
            print(f'ok   cargo test --no-run and the doc tests: all {len(crates)} crates unchanged since their last '
                  f'green run on this PC')
            return 0
    t0 = time.time()
    rc, err = build(env)
    print(f'{"ok  " if rc == 0 else "FAIL"} cargo test --workspace --no-run ({time.time() - t0:.0f} s)')
    if rc:
        tail(err)
        return 1
    t0 = time.time()
    r = cargo(env, 'test', '--workspace', '--doc')
    out = r.stdout + r.stderr
    passed = sum(int(m.group(1)) for m in re.finditer(r'test result: \w+\. (\d+) passed', out))
    print(f'{"ok  " if r.returncode == 0 else "FAIL"} cargo test --doc: {passed} doc tests passed '
          f'({time.time() - t0:.0f} s)')
    if r.returncode:
        tail(out)
        return 1
    return 0


# --- the test binaries ----------------------------------------------------------------------------------------------

class Result:
    """One test binary's run."""

    def __init__(self, b: dict[str, Any]):
        self.b = b
        self.passed: list[str] = []
        self.failed: list[tuple[str, str]] = []
        self.ignored: list[tuple[str, str]] = []
        self.times: dict[str, float] = {}
        self.outputs: dict[str, str] = {}
        self.rc = 0
        self.wall = 0.0
        self.trailer = ''

    @property
    def name(self) -> str:
        return f"{self.b['crate']}/{self.b['kind']}:{self.b['target']}"


def run_binary(b: dict[str, Any], game: bool, env: dict[str, str]) -> Result:
    res = Result(b)
    args = [b['exe'], '--format', 'json', '-Z', 'unstable-options', '--report-time', '--show-output']
    if game:
        args.append('--ignored')
    if game and b['kind'] == 'test' and b['crate'] in GATED:
        # the whole-level tests: two at a time (as gate() lets them), so each one's time is its own, not its wait.
        # Not in the quick group: there libtest's fixed order with two threads always pairs g3::sets_from_the_data
        # with g4::create_and_open_projects, which hang together (2 runs in 5, TS1; a round of its own finds why)
        args += ['--test-threads', '2']
    # what cargo test sets for a test process at run time
    child = dict(env, RUSTC_BOOTSTRAP='1', CARGO_MANIFEST_DIR=b['cwd'], CARGO_PKG_NAME=b['crate'])
    t0 = time.time()
    p = subprocess.Popen(args, cwd=b['cwd'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                         encoding='utf-8', errors='replace', env=child)
    limit = BINARY_LIMIT['game' if game else 'quick']
    try:
        stdout, stderr = p.communicate(timeout=limit)
    except subprocess.TimeoutExpired:
        p.kill()
        stdout, stderr = p.communicate()
        stderr += f'\ntest_rust: the binary was still running after {limit:.0f} s and was ended'
    res.wall, res.rc = time.time() - t0, p.returncode
    loose = []
    started: list[str] = []
    for line in stdout.splitlines():
        e: Any = None
        if line.startswith('{'):
            try:
                e = json.loads(line)
            except ValueError:
                e = None
        if not isinstance(e, dict) or e.get('type') not in ('test', 'suite'):
            loose.append(line)
            continue
        if e.get('type') != 'test':
            continue
        name, ev = str(e.get('name')), e.get('event')
        if ev == 'started':
            started.append(name)
        if ev in ('ok', 'failed'):
            res.outputs[name] = str(e.get('stdout') or '')
            if e.get('exec_time') is not None:
                res.times[name] = float(e['exec_time'])
        if ev == 'ok':
            res.passed.append(name)
        elif ev == 'failed':
            res.failed.append((name, str(e.get('stdout') or '')))
        elif ev == 'ignored':
            res.ignored.append((name, str(e.get('message') or '')))
    done = set(res.passed) | {n for n, _ in res.failed} | {n for n, _ in res.ignored}
    unfinished = [n for n in started if n not in done]
    res.trailer = '\n'.join(loose[-30:] + stderr.strip().splitlines()[-30:]
                            + [f'started, never finished: {n}' for n in unfinished])
    return res


def load_binaries() -> dict[str, Any]:
    got = atomicio.read_json(binaries_file(), {})
    return got if isinstance(got, dict) and 'binaries' in got else {}


def keep_times(results: list[Result], group: str) -> list[tuple[float, str]]:
    """Every test's time into RUST_TIMES (merged under a lock, replaced atomically); the run's tests by time."""
    day = time.strftime('%Y-%m-%d')
    rows = [(s, f'{r.name}::{t}') for r in results for t, s in r.times.items()]
    if rows:
        try:
            with atomicio.locked(RUST_TIMES + '.lock'):
                kept = atomicio.read_json(RUST_TIMES, {})
                kept = kept if isinstance(kept, dict) else {}
                for s, n in rows:
                    kept[n] = {'s': round(s, 3), 'group': group, 'when': day}
                atomicio.write_json(RUST_TIMES, kept, indent=1, sort_keys=True)
        except OSError:
            pass
    return sorted(rows, reverse=True)


def step_run(env: dict[str, str], argv: list[str]) -> int:
    game = '--game' in argv
    group = 'game' if game else 'quick'
    jobs = int(argv[argv.index('--jobs') + 1]) if '--jobs' in argv[:-1] else 2
    crates = {package_name(c): c for c in members()}
    fps = {n: crate_fingerprint(f, 'tests', env=env) for n, f in crates.items()}    # before the run, as clippy's
    skip = may_skip(argv) and not game
    green: dict[str, str] = {}
    for n in crates:
        when = green_since('tests-quick', n, fps[n]) if skip else None
        if when:
            green[n] = when
    todo = [n for n in crates if n not in green]
    if not todo:
        print(f'ok   cargo tests, quick group: all {len(crates)} crates unchanged since their last green run on '
              f'this PC')
        print('all passed')
        return 0
    rec = load_binaries()
    stale = [n for n in todo if rec.get('fingerprints', {}).get(n) != fps[n]]
    if stale and game:
        rc, err = build(env)
        if rc:
            print('FAIL cargo test --workspace --no-run (the game group builds the tests first)')
            tail(err)
            return 1
        rec = load_binaries()
        stale = [n for n in todo if rec.get('fingerprints', {}).get(n) != fps[n]]
    if stale:
        print(f'FAIL cargo tests, {group} group: the test binaries of {", ".join(stale)} were not built from these '
              f'sources (the build entry failed, or the sources changed since): python tests\\test_rust.py build')
        return 1
    bins = [b for b in rec['binaries'] if b['crate'] in todo and not (game and (b['crate'], b['target']) in CORPUS)]
    known = atomicio.read_json(RUST_TIMES, {}) or {}

    def expected(b: dict[str, Any]) -> float:
        pre = f"{b['crate']}/{b['kind']}:{b['target']}::"
        return sum(float(v.get('s', 0)) for k, v in known.items()
                   if k.startswith(pre) and isinstance(v, dict) and v.get('group') == group)

    bins.sort(key=expected, reverse=True)                 # the longest first: the run ends on the short ones
    t0 = time.time()
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, jobs)) as pool:
        results = list(pool.map(lambda b: run_binary(b, game, env), bins))
    wall = time.time() - t0
    ranked = keep_times(results, group)
    failed = [r for r in results if r.rc != 0 or r.failed]
    passed = sum(len(r.passed) for r in results)
    # an ignored test runs in the game group (its reason starts "game tier") or the corpus entry - or in no tier
    nowhere = [f'{r.name}::{n} ({why or "no reason"})' for r in results if not game for n, why in r.ignored
               if not why.lower().startswith(GAME_TIER) and (r.b['crate'], r.b['target']) not in CORPUS]
    for_game = sum(1 for r in results if not game for _, why in r.ignored if why.lower().startswith(GAME_TIER))
    parts: dict[str, list[str]] = {}
    for r in results:
        for t, out in r.outputs.items():
            lines = sorted({ascii_text(line.strip()) for line in out.splitlines() if SKIP.search(line)})
            if lines:
                parts[f'{r.name}::{t}'] = lines
    ok = not failed and not nowhere
    detail = f'{passed} tests passed in {len(results)} binaries ({wall:.0f} s)'
    if for_game:
        detail += f'; {for_game} left for the game tier'
    if green:
        detail += f'; {len(green)} crate(s) unchanged since their last green run, left out ({", ".join(sorted(green))})'
    if parts:
        detail += f'; {len(parts)} with parts skipped for want of data or a graphics adapter (below)'
    print(f'{"ok  " if ok else "FAIL"} cargo tests, {group} group: {detail}')
    for s, n in ranked[:8]:
        print(f'       {s:7.1f} s  {ascii_text(n)}')
    for r in results:
        for _t, out in sorted(r.outputs.items()):
            for line in out.splitlines():
                if any(k in line for k in test_editor.KEEP):
                    print('       ' + ascii_text(line.strip()))
    for k, lines in sorted(parts.items()):
        for line in lines[:4]:
            print(f'       skipped part: {ascii_text(k)}: {line}')
    for n in nowhere:
        print(f'FAIL   ignored, and no tier runs it: {ascii_text(n)} (mark it #[ignore = "game tier: ..."])')
    for r in failed:
        print(f'--- {r.name}: exit code {r.rc}, {len(r.failed)} failed')
        for name, out in r.failed:
            print(f'    {ascii_text(name)}')
            tail(out, 30)
        if not r.failed:
            tail(r.trailer)
    if ok and not game:
        mark_green('tests-quick', {n: fps[n] for n in todo})
    print('all passed' if ok else 'the cargo tests failed')
    return 0 if ok else 1


def step_list(env: dict[str, str]) -> int:
    """Every test of the workspace with its tier: quick (not ignored), game (#[ignore = "game tier: ..."]), game
    (--corpus) or NONE (ignored for another reason: a failure). The doc tests run in the build entry."""
    rec = load_binaries()
    if not rec:
        rc, err = build(env)
        if rc:
            tail(err)
            return 1
        rec = load_binaries()
    rows, nowhere = [], 0
    for b in rec['binaries']:
        p = subprocess.run([b['exe'], '--format', 'json', '-Z', 'unstable-options', '--list'], cwd=b['cwd'],
                           capture_output=True, text=True, env=dict(env, RUSTC_BOOTSTRAP='1'))
        for line in p.stdout.splitlines():
            try:
                e = json.loads(line)
            except ValueError:
                continue
            if not (isinstance(e, dict) and e.get('event') == 'discovered'):
                continue
            why = str(e.get('ignore_message') or '')
            if not e.get('ignore'):
                tier = 'quick'
            elif (b['crate'], b['target']) in CORPUS:
                tier = 'game (--corpus, release)'
            elif why.lower().startswith(GAME_TIER):
                tier = 'game'
            else:
                tier = 'NONE'
                nowhere += 1
            rows.append(f"{b['crate']}/{b['kind']}:{b['target']}::{e.get('name')}\t{tier}")
    r = cargo(env, 'test', '--workspace', '--doc', '--', '--list')
    rows += [f'doc:{line[:-6]}\tquick (build entry)' for line in r.stdout.splitlines() if line.endswith(': test')]
    for row in sorted(rows):
        print(ascii_text(row))
    counts: dict[str, int] = {}
    for row in rows:
        counts[row.split('\t')[1]] = counts.get(row.split('\t')[1], 0) + 1
    print(f'{len(rows)} tests: ' + ', '.join(f'{v} {k}' for k, v in sorted(counts.items())))
    return 1 if nowhere else 0


def step_corpus(env: dict[str, str]) -> int:
    failed = 0
    for name, test in (('DBX corpus test (release)', 'corpus'), ('FBRB corpus test (release)', 'fbrb_corpus')):
        r = cargo(env, 'test', '--release', '-p', 'bc2-formats', '--test', test, '--', '--ignored', '--nocapture')
        out = (r.stdout + r.stderr).strip()
        summary = [line for line in out.splitlines() if line.startswith('test result:')
                   or (line[:1].isdigit() and ('files (' in line or ' bundles, ' in line))]
        passed = sum(int(line.split(' passed')[0].rsplit(' ', 1)[-1]) for line in summary if ' passed' in line)
        print(f'{"ok  " if r.returncode == 0 else "FAIL"} {name}' + (f': {passed} tests passed' if r.returncode == 0
                                                                       else ''))
        for line in summary:
            if 'files (' in line or ' bundles, ' in line:
                print('       ' + line)
        if r.returncode:
            failed += 1
            tail(out)
    print('all passed' if not failed else f'{failed} step(s) failed')
    return 1 if failed else 0


STEPS = ('fmt', 'clippy', 'build', 'run', 'list')


def main(argv: list[str]) -> int:
    env = build_native.cargo_env()
    if env is None:
        print('skipped: Rust (cargo) is not installed - the suite runs on the Python readers')
        return 0
    args = argv[1:]
    if '--corpus' in args:
        return step_corpus(env)
    step = next((a for a in args if a in STEPS), None)
    if step == 'run':
        return step_run(env, args)
    if step == 'list':
        return step_list(env)
    if step == 'fmt':
        rc = step_fmt(env)
    elif step == 'clippy':
        rc = step_clippy(env, args)
    elif step == 'build':
        rc = step_build(env, args)
    else:
        rc = step_fmt(env) | step_clippy(env, args) | step_build(env, args)
        if rc == 0:
            return step_run(env, args)
    print('all passed' if rc == 0 else 'failed')
    return 1 if rc else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
