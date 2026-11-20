r"""The suite's Rust core (the module bc2_native): finding it, checking that it was built from this checkout's Rust
sources, and the error a tool raises when it needs it (README.md, "The Rust core").

    from bfbc2core import native
    m = native.load()                   # the module, or None (not there, or built from other sources: warns once)
    m = native.require('texture conversion')    # the module, or NativeMissing saying how to get it

The source fingerprint. fingerprint() is a SHA-256 over every file whose change makes a new module (sources(): the
workspace and crate manifests, the lock file, the toolchain file, each crate's build.rs and src\**\*.rs), each as
its path relative to rust\ with forward slashes, its length and its bytes with CRLF turned into LF, in path order:
a Windows checkout with autocrlf, a Linux one and the release workflow's all give the same value. build_native.py
bakes it into the module (the environment variable BC2_SOURCE_HASH, read at compile time: bc2_native.__source_hash__,
next to __version__, the crates' version) and writes bc2_native.json beside the module.

The check, on the first import in a process: the module's __source_hash__ against this checkout's fingerprint. The
checkout's fingerprint is not hashed on every run: bc2_native.json keeps it with each source file's size and time
(nanoseconds) when it was last computed, and while every file still has them it is taken from there (a stat of
about 60 files). A changed time (an edit, a git checkout, a rebuild of nothing) hashes the files again, a few
milliseconds, and refreshes bc2_native.json when the module matches. A checkout without rust\ (only core\
copied somewhere) has no fingerprint and any module is taken as it is.

A module whose fingerprint differs (or has none: built before 2026-11-17, or by cargo / pip without
BC2_SOURCE_HASH) is never used silently: load() returns None and warns once (the pieces with a Python twin run in
Python), require() raises NativeMissing. BFBC2_PURE_PYTHON=1 turns off only the optional pieces (their use_native);
the pieces whose Python was retired (texture conversion, NavKit's grid walks: 2026-11-17; the conversions, the rest
of NavKit, and the core formats - dbx, fbrb, terrain, water, collision, level, culltree's reader -: 2026-11-18)
always need the module.
"""
import hashlib
import importlib
import json
import os
import subprocess
import sys
import warnings
from collections.abc import Iterator
from types import ModuleType
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
RUST = os.path.join(SUITE, 'rust')
STAMP = os.path.join(HERE, 'bc2_native.json')
BUILD = os.path.join(RUST, 'build_native.py')
SOURCE_ENV = 'BC2_SOURCE_HASH'           # what build_native.py passes to cargo; bc2-py's build.rs reads it
# The workspace's applications: crates the module never links (bc2-py does not depend on them), so their sources are
# not the module's and a change to them does not make a new module (the map editor, round G1, 2026-11-18). The
# workspace files (Cargo.toml, Cargo.lock) still count, as a dependency they add changes them.
APPLICATIONS = ('bc2-editor',)


def sources(rust: str = RUST) -> list[str]:
    """Every file whose change makes a new module: the workspace's manifests, lock and toolchain files, and each crate
    (a folder of rust\\ with a Cargo.toml: a new crate needs no entry here) with its build.rs, src\\**\\*.rs and the
    Python package file maturin reads (pyproject.toml); not the APPLICATIONS' crates."""
    out = [os.path.join(rust, n) for n in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')]
    if os.path.isdir(rust):
        for crate in sorted(d for d in os.listdir(rust)
                            if os.path.isfile(os.path.join(rust, d, 'Cargo.toml')) and d not in APPLICATIONS):
            base = os.path.join(rust, crate)
            out += [os.path.join(base, n) for n in ('Cargo.toml', 'build.rs', 'pyproject.toml')]
            for top, dirs, files in os.walk(os.path.join(base, 'src')):
                dirs.sort()
                out += [os.path.join(top, f) for f in sorted(files) if f.endswith('.rs')]
    return sorted((p for p in out if os.path.isfile(p)), key=lambda p: _rel(p, rust))


def _rel(path: str, rust: str) -> str:
    return os.path.relpath(path, rust).replace(os.sep, '/')


def fingerprint(rust: str = RUST, paths: list[str] | None = None) -> str:
    """The sources' fingerprint (hex SHA-256; see the module's text): the same on every checkout of the same commit,
    whatever its line endings."""
    h = hashlib.sha256()
    for p in paths if paths is not None else sources(rust):
        with open(p, 'rb') as f:
            data = f.read().replace(b'\r\n', b'\n')
        h.update(f'{_rel(p, rust)}\0{len(data)}\0'.encode())
        h.update(data)
    return h.hexdigest()


def _stats(paths: list[str], rust: str) -> dict[str, list[int]]:
    out = {}
    for p in paths:
        st = os.stat(p)
        out[_rel(p, rust)] = [st.st_size, st.st_mtime_ns]
    return out


def read_stamp() -> dict[str, Any]:
    """bc2_native.json ({} when it is missing or damaged)."""
    try:
        with open(STAMP, encoding='utf-8') as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def write_stamp(module_hash: str, origin: str, version: str = '', rust: str = RUST) -> None:
    """bc2_native.json for a module just built or downloaded: its fingerprint, where it came from, and this
    checkout's fingerprint with the files' sizes and times (the cache checkout_fingerprint reads)."""
    paths = sources(rust)
    d = {'source_hash': module_hash, 'version': version, 'origin': origin,
         'checkout': {'fingerprint': fingerprint(rust, paths), 'files': _stats(paths, rust)}}
    tmp = STAMP + '.tmp'
    with open(tmp, 'w', encoding='utf-8') as f:
        json.dump(d, f, indent=1, sort_keys=True)
    os.replace(tmp, STAMP)


def _checkout(rust: str = RUST) -> tuple[str | None, bool]:
    """(this checkout's fingerprint, whether it came from bc2_native.json's cache): the cache while every source
    file has the size and time it recorded, else hashed now; None when there is no rust\\ folder (no sources to
    compare with)."""
    if not os.path.isfile(os.path.join(rust, 'Cargo.toml')):
        return None, False
    paths = sources(rust)
    cached = read_stamp().get('checkout') or {}
    if cached.get('files') == _stats(paths, rust) and isinstance(cached.get('fingerprint'), str):
        return str(cached['fingerprint']), True
    return fingerprint(rust, paths), False


def checkout_fingerprint(rust: str = RUST) -> str | None:
    """This checkout's fingerprint (None without a rust\\ folder), from the cache when it holds."""
    return _checkout(rust)[0]


def _refresh_cache(fp: str, rust: str = RUST) -> None:
    """The stamp's checkout cache brought up to date (the files' times changed, their contents give the module's
    fingerprint)."""
    d = read_stamp()
    if not d:
        return
    try:
        d['checkout'] = {'fingerprint': fp, 'files': _stats(sources(rust), rust)}
        tmp = STAMP + '.tmp'
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump(d, f, indent=1, sort_keys=True)
        os.replace(tmp, STAMP)
    except OSError:
        pass                                 # a read-only checkout or a race with another process: only a cache


class NativeMissing(ImportError):
    """A tool needs the Rust core and it is not there, or was built from other sources than this checkout's."""


_state: dict[str, Any] = {}


def _candidates() -> Iterator[tuple[str, ModuleType]]:
    """The modules that can be imported, in the order they are preferred: built into this folder, then installed."""
    for where, name, pkg in (('core/bfbc2core', '.bc2_native', __package__), ('installed', 'bc2_native', None)):
        try:
            yield where, importlib.import_module(name, pkg)
        except ImportError:
            continue


def _probe() -> None:
    if _state:
        return
    _state.update(module=None, status='missing', detail='')
    want: str | None = None
    cached = False
    for n, (where, m) in enumerate(_candidates()):
        if n == 0:
            want, cached = _checkout()
        have = getattr(m, '__source_hash__', '') or ''
        if want is None or have == want:
            _state.update(module=m, status='ok', where=where)
            if want is not None and not cached and read_stamp().get('source_hash') == have:
                _refresh_cache(want)
            return
        if _state['status'] == 'missing':
            _state.update(status='stale', where=where,
                          detail=f'the module in {where} was built from other Rust sources than this checkout has '
                                 f'(module {have[:12] or "without a fingerprint"}, checkout {(want or "")[:12]})')


def load() -> ModuleType | None:
    """The Rust core, or None: not there, or built from other sources than this checkout's (then a warning, once
    per process: the pieces with a Python twin run in Python)."""
    _probe()
    if _state['status'] == 'stale' and not _state.get('warned'):
        _state['warned'] = True
        warnings.warn(f'bfbc2core: {_state["detail"]}; it is not used. {_fix()}', RuntimeWarning, stacklevel=2)
    return _state['module']


def status() -> str:
    """'ok', 'missing' or 'stale'."""
    _probe()
    return str(_state['status'])


def _fix() -> str:
    py = os.path.basename(sys.executable).removesuffix('.exe') or 'python'
    return (f'Get the right one with "{py} {BUILD} --download" (the prebuilt module for this checkout: Windows, '
            f'64-bit Python 3.12 or later) or build it with "{py} {BUILD}" (needs Rust and maturin: README.md, '
            f'"The Rust core").')


def require(what: str, attr: str | None = None) -> ModuleType:
    """The Rust core for `what` (a piece whose Python was retired), or NativeMissing saying why and how to get it.
    attr: a function or class the module must have (a module older than the piece lacks it)."""
    _probe()
    m = _state['module']
    if m is None:
        why = ('the module is not built or downloaded' if _state['status'] == 'missing' else _state['detail'])
        raise NativeMissing(f'{what} needs the suite\'s Rust core (bc2_native): {why}. {_fix()}')
    if attr is not None and not hasattr(m, attr):
        raise NativeMissing(f'{what} needs bc2_native.{attr}, which this module lacks. {_fix()}')
    return m


def reset() -> None:
    """Forget what the probe found (tests; after build_native.py replaced the module, a running process still
    holds the old one until it restarts)."""
    _state.clear()
    _cli.clear()


# ------------------------------------------------------------------------------- the command-line binary (round O1)

# ------------------------------------------------------------- where cargo builds, where the binaries live (round D1)
#
# cargo's build folder is outside the checkout (round D1, 2026-11-19: each worktree's own rust\target took 5-15 GB of
# the only drive): <root>\<checkout folder name>, root C:\Programming\Projects\rust-targets beside the checkouts
# (BFBC2_RUST_TARGETS overrides it), e.g. rust-targets\bfbc2-tool-suite, rust-targets\bfbc2-tool-suite-e2b - one
# folder per checkout, so two checkouts never wait on each other's cargo lock. An explicit CARGO_TARGET_DIR wins.
# The binaries the tools run are copied to the suite's bin\ (gitignored, built outputs only) by build_native.py:
# their path never depends on the build folder.

TARGETS_ENV = 'BFBC2_RUST_TARGETS'
BIN = os.path.join(SUITE, 'bin')
EXE = '.exe' if os.name == 'nt' else ''


def targets_root() -> str:
    """The folder holding every checkout's build folder: BFBC2_RUST_TARGETS, else rust-targets beside the
    checkout."""
    v = os.environ.get(TARGETS_ENV, '').strip()
    return os.path.abspath(v) if v else os.path.join(os.path.dirname(SUITE), 'rust-targets')


def target_dir(suite: str = SUITE) -> str:
    """cargo's build folder for this checkout: CARGO_TARGET_DIR when set (relative to rust\\, where cargo runs),
    else <targets_root()>\\<the checkout's folder name>."""
    v = os.environ.get('CARGO_TARGET_DIR', '').strip()
    if v:
        return os.path.normpath(os.path.join(suite, 'rust', v))
    return os.path.join(targets_root(), os.path.basename(os.path.normpath(suite)))


def cargo_env(env: dict[str, str] | None = None) -> dict[str, str]:
    """env (default: this process's) with CARGO_TARGET_DIR set to target_dir(): every cargo the suite runs builds
    there."""
    out = dict(os.environ if env is None else env)
    out['CARGO_TARGET_DIR'] = target_dir()
    return out


def built_path(name: str, profile: str = 'release') -> str:
    """Where cargo leaves the binary `name` (no .exe) of `profile`."""
    return os.path.join(target_dir(), profile, name + EXE)


def bin_path(name: str) -> str:
    """The binary's stable place: the suite's bin\\ (build_native.py copies it there after a build)."""
    return os.path.join(BIN, name + EXE)


def find_binary(name: str) -> str | None:
    """The binary `name`: bin\\ first, then this checkout's build folder (release, debug), then the old rust\\target
    (a checkout built before round D1); None when none is there."""
    for p in (bin_path(name), built_path(name, 'release'), built_path(name, 'debug'),
              os.path.join(RUST, 'target', 'release', name + EXE), os.path.join(RUST, 'target', 'debug', name + EXE)):
        if os.path.isfile(p):
            return p
    return None


CLI_PYTHON_ENV = 'BFBC2_CLI_PYTHON'      # 1: the Python command lines answer, the binary is not used
CLI_TOOLS = {                            # bfbc2 <tool>: the Python command line it is the port of
    'fbrbtool': os.path.join(SUITE, 'bfbc2-fbrb-tool', 'fbrbtool.py'),
    'dbxedit': os.path.join(SUITE, 'bfbc2-dbx-tools', 'dbxedit.py'),
    # round O2 (rust\bc2-cli\src\pyproc.rs: each hands the command line to its .py where the Rust core refuses)
    'aptcheck': os.path.join(SUITE, 'bfbc2-menu-tools', 'aptcheck.py'),
    'aptrefs': os.path.join(SUITE, 'bfbc2-menu-tools', 'aptrefs.py'),
    'bc2loc': os.path.join(SUITE, 'bfbc2-menu-tools', 'bc2loc.py'),
    'exeref': os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'exeref.py'),
    'crashreader': os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'crashreader.py'),
    'hpdtool': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdtool.py'),
    'hkpack': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hkpack.py'),
    'convert': os.path.join(SUITE, 'bfbc2-console-convert', 'convert.py'),
    'aimanim': os.path.join(SUITE, 'bfbc2-anim-tools', 'aimanim.py'),
    # round O3 (the same way; rust\bc2-cli\src\<group>)
    'caches': os.path.join(SUITE, 'core', 'bfbc2core', 'caches.py'),
    'stockmp': os.path.join(SUITE, 'bfbc2-stockmp-ai', 'stockmp.py'),
    'aptgraft': os.path.join(SUITE, 'bfbc2-menu-tools', 'aptgraft.py'),
    'audio': os.path.join(SUITE, 'bfbc2-console-convert', 'audio.py'),
    'dbxcatalogue': os.path.join(SUITE, 'bfbc2-bf1943-import', 'dbxcatalogue.py'),
    'dbxparity': os.path.join(SUITE, 'bfbc2-bf1943-import', 'dbxparity.py'),
    'portparity': os.path.join(SUITE, 'bfbc2-bf1943-import', 'portparity.py'),
    'meshview': os.path.join(SUITE, 'bfbc2-mesh-viewer', 'meshview.py'),
    'culltree': os.path.join(SUITE, 'bfbc2-culltree', 'culltree.py'),
    # round O4: NavKit's driver and tools, the editor's level prepare, the test harness, texqa
    'navkit': os.path.join(SUITE, 'bfbc2-auto-navkit', 'navkit.py'),
    'prepare_level': os.path.join(SUITE, 'rust', 'bc2-editor', 'prepare_level.py'),
    'levelobjects': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'levelobjects.py'),
    'ladders': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'ladders.py'),
    'ordercheck': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'ordercheck.py'),
    'navcompare': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navcompare.py'),
    'graphstats': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'graphstats.py'),
    'hpdcheck': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdcheck.py'),
    'hpdview': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'hpdview.py'),
    'detailed': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'detailed.py'),
    'navparity': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navparity.py'),
    'navregress': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'navregress.py'),
    'sample_all': os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools', 'sample_all.py'),
    'harness': os.path.join(SUITE, 'bfbc2-automated-map-and-bot-test-harness', 'harness.py'),
    'texqa': os.path.join(SUITE, 'bfbc2-console-convert', 'texqa.py'),
    # round O5: the console readers, the BF1943 importer
    'apt': os.path.join(SUITE, 'bfbc2-console-readers', 'apt', 'apt.py'),
    'stfs': os.path.join(SUITE, 'bfbc2-console-readers', 'stfs', 'stfs.py'),
    'xiso': os.path.join(SUITE, 'bfbc2-console-readers', 'xiso', 'xiso.py'),
    'bf1943import': os.path.join(SUITE, 'bfbc2-bf1943-import', 'bf1943import.py'),
}
_cli: dict[str, str | None] = {}


def cli_path(profile: str | None = None) -> str:
    """The command-line binary (rust\\bc2-cli: bfbc2.exe): its stable place bin\\bfbc2.exe (build_native.py copies
    it there), or with a profile where cargo builds it."""
    return bin_path('bfbc2') if profile is None else built_path('bfbc2', profile)


def cli_exe() -> str | None:
    """The command-line binary when it is built from this checkout's Rust sources (its --source-hash is the
    checkout's fingerprint: build_native.py builds it with the module), else None. Asked once per process."""
    if 'exe' not in _cli:
        exe, found = find_binary('bfbc2') or cli_path(), None
        if os.path.isfile(exe):
            try:
                r = subprocess.run([exe, '--source-hash'], capture_output=True, text=True, timeout=30)
                want = checkout_fingerprint()
                if r.returncode == 0 and (want is None or r.stdout.strip() == want):
                    found = exe
            except (OSError, subprocess.SubprocessError):
                pass
        _cli['exe'] = found
    return _cli['exe']


def cli_command(tool: str) -> list[str]:
    """The command line that runs a ported tool (CLI_TOOLS): [bfbc2.exe, tool] when the binary is built from this
    checkout, else (or with BFBC2_CLI_PYTHON=1) [python, the tool's .py] - the same arguments, output and exit
    codes either way (rust/BENCHMARKS.md, "O1")."""
    script = CLI_TOOLS[tool]
    exe = None if os.environ.get(CLI_PYTHON_ENV, '').strip() == '1' else cli_exe()
    return [exe, tool] if exe else [sys.executable, script]
