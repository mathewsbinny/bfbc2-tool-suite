r"""The suite's Rust core (the module bc2_native): finding it, checking that it was built from this checkout's Rust
sources, and the error a tool raises when it needs it (README.md, "The Rust core").

    from bfbc2core import native
    m = native.load()                   # the module, or None (not there, or built from other sources: warns once)
    m = native.require('texture conversion')    # the module, or NativeMissing saying how to get it

The source fingerprint. fingerprint() is a SHA-256 over every file whose change makes a new artefact (sources(): the
workspace manifest, the lock file, the toolchain file, the suite's VERSION file, which the binaries read at build
time, and for each crate the artefact links - crates(), its crate's closure of workspace dependencies, round QW1 -
its manifest, build.rs, src\**\*.rs and the files they include), each as
its path relative to rust\ with forward slashes, its length and its bytes with CRLF turned into LF, in path order:
a Windows checkout with autocrlf, a Linux one and the release workflow's all give the same value. build_native.py
bakes it into the module (the environment variable BC2_SOURCE_HASH, read at compile time: bc2_native.__source_hash__,
next to __version__, the crates' version) and writes bc2_native.json beside the module. Each artefact has its own
fingerprint (ARTEFACTS: the module, bfbc2.exe, bfbc2-editor.exe, mapexport.exe), baked into it the same way (the
binaries print it with --source-hash), so an edit to the editor does not make the module or bfbc2.exe stale.

The check, on the first import in a process: the module's __source_hash__ against this checkout's fingerprint. The
checkout's fingerprint is not hashed on every run: bc2_native.json keeps it with each source file's size and time
(nanoseconds) when it was last computed, per artefact, and while every file still has them it is taken from there
(a stat of 140-300 files). A changed time (an edit, a git checkout, a rebuild of nothing) hashes the files again, a few
milliseconds, and refreshes bc2_native.json when the module matches. A checkout without rust\ (only core\
copied somewhere) has no fingerprint and any module is taken as it is.

A module whose fingerprint differs (or has none: built before 2026-11-17, or by cargo / pip without
BC2_SOURCE_HASH) is never used silently: load() returns None and warns once (the pieces with a Python twin run in
Python), require() raises NativeMissing. BFBC2_PURE_PYTHON=1 turns off only the optional pieces (their use_native);
the pieces whose Python was retired (texture conversion, NavKit's grid walks: 2026-11-17; the conversions, the rest
of NavKit, and the core formats - dbx, fbrb, terrain, water, collision, level, culltree's reader -: 2026-11-18)
always need the module.

The binaries (round QW1, audit AU2 A6 / A19): binary(name) is bfbc2.exe, bfbc2-editor.exe or mapexport.exe only
while its --source-hash is this checkout's fingerprint of it. cli_command falls back to a tool's Python when
bfbc2.exe is stale, with one RuntimeWarning per process on stderr naming the fix (python rustuild_native.py);
BFBC2_STRICT_NATIVE=1 makes a stale or missing bfbc2.exe an error (NativeStale) instead. BFBC2_CLI_PYTHON=1 still
asks for the Python, with no warning.
"""
import contextlib
import hashlib
import importlib
import json
import os
import re
import subprocess
import sys
import tomllib
import warnings
from collections.abc import Iterator
from types import ModuleType
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
RUST = os.path.join(SUITE, 'rust')
STAMP = os.path.join(HERE, 'bc2_native.json')
BUILD = os.path.join(RUST, 'build_native.py')
SOURCE_ENV = 'BC2_SOURCE_HASH'           # what build_native.py passes to cargo; the artefacts bake it in
# Round QW1 (audit AU2 A7): one fingerprint per built artefact, over the crates it links. artefact -> the crate that
# builds it; its sources are the workspace files and that crate's closure of workspace path dependencies (normal,
# build and target-specific ones, optional ones included; dev-dependencies are the tests'), so an edit to a crate an
# artefact does not link (the map editor's, the renderer's) leaves that artefact up to date.
ARTEFACTS = {
    'module': 'bc2-py',             # bc2_native.pyd (maturin); its release tag is this fingerprint's
    'bfbc2': 'bc2-cli',             # bin\bfbc2.exe
    'bfbc2-editor': 'bc2-editor',   # bin\bfbc2-editor.exe (build_native.py --editor)
    'mapexport': 'bc2-levelwrite',  # bin\mapexport.exe: a binary of the levelwrite crate (src\bin\mapexport.rs)
}
# Before round QW1 the module's sources were every crate but these (one fingerprint served the module and bfbc2.exe).
APPLICATIONS = ('bc2-editor',)
STRICT_ENV = 'BFBC2_STRICT_NATIVE'      # 1: a stale or missing bfbc2.exe is an error, not a fall-back to the Python
_INCLUDE = re.compile(rb'include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)')


def _manifest(path: str) -> dict[str, Any]:
    try:
        with open(path, 'rb') as f:
            return tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError):
        return {}


def _dep_tables(manifest: dict[str, Any]) -> Iterator[dict[str, Any]]:
    for key in ('dependencies', 'build-dependencies'):
        yield manifest.get(key) or {}
    for target in (manifest.get('target') or {}).values():
        if isinstance(target, dict):
            for key in ('dependencies', 'build-dependencies'):
                yield target.get(key) or {}


def crates(rust: str = RUST, artefact: str = 'module') -> list[str]:
    r"""The crate folders (names under rust\) whose sources make `artefact`: its crate (ARTEFACTS) and every workspace
    crate it depends on, directly or not. A workspace without the artefact's crate (a stand-in in a test, a copied
    rust\ of an older layout): every crate but APPLICATIONS, the rule before round QW1."""
    root = _manifest(os.path.join(rust, 'Cargo.toml'))
    ws_paths = {name: str(v['path']) for name, v in ((root.get('workspace') or {}).get('dependencies') or {}).items()
                if isinstance(v, dict) and 'path' in v}
    start = ARTEFACTS[artefact]
    if not os.path.isfile(os.path.join(rust, start, 'Cargo.toml')):
        return sorted(d for d in os.listdir(rust) if os.path.isfile(os.path.join(rust, d, 'Cargo.toml'))
                      and d not in APPLICATIONS) if os.path.isdir(rust) else []
    seen: set[str] = set()
    todo = [start]
    while todo:
        crate = todo.pop()
        if crate in seen:
            continue
        seen.add(crate)
        base = os.path.join(rust, crate)
        for table in _dep_tables(_manifest(os.path.join(base, 'Cargo.toml'))):
            for name, spec in table.items():
                path = None
                if isinstance(spec, dict) and spec.get('workspace'):
                    path = ws_paths.get(str(spec.get('package', name)))
                    path = os.path.join(rust, path) if path else None
                elif isinstance(spec, dict) and 'path' in spec:
                    path = os.path.join(base, str(spec['path']))
                if path:
                    rel = os.path.relpath(os.path.normpath(path), rust)
                    if os.sep not in rel and os.path.isfile(os.path.join(rust, rel, 'Cargo.toml')):
                        todo.append(rel)
    return sorted(seen)


def sources(rust: str = RUST, artefact: str = 'module') -> list[str]:
    r"""Every file whose change makes a new `artefact` (ARTEFACTS; the module by default): the workspace's manifests,
    lock and toolchain files, the suite's VERSION file beside rust\ (round P1: the binaries read it at build time, so
    a new version is a new build and release), and for each crate of crates(): its Cargo.toml, build.rs, the Python
    package file maturin reads (pyproject.toml), src\**\*.rs and the files its sources and build script include
    (include_str! / include_bytes!: the Python docstrings bfbc2.exe prints, the editor's fonts, the icon's
    app_icon.rs)."""
    out = [os.path.join(rust, n) for n in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')]
    out.append(os.path.join(os.path.dirname(os.path.abspath(rust)), 'VERSION'))
    if os.path.isdir(rust):
        for crate in crates(rust, artefact):
            base = os.path.join(rust, crate)
            out += [os.path.join(base, n) for n in ('Cargo.toml', 'build.rs', 'pyproject.toml')]
            code = [os.path.join(base, 'build.rs')]
            for top, dirs, files in os.walk(os.path.join(base, 'src')):
                dirs.sort()
                code += [os.path.join(top, f) for f in sorted(files) if f.endswith('.rs')]
            out += code[1:]
            for p in code:
                try:
                    with open(p, 'rb') as f:
                        text = f.read()
                except OSError:
                    continue
                for m in _INCLUDE.finditer(text):
                    out.append(os.path.normpath(os.path.join(os.path.dirname(p), m.group(1).decode('utf-8', 'replace'))))
    return sorted({p for p in out if os.path.isfile(p)}, key=lambda p: _rel(p, rust))


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
    checkout's fingerprint per artefact with the files' sizes and times (the cache checkout_fingerprint reads)."""
    checkouts = {}
    for artefact in ARTEFACTS:
        paths = sources(rust, artefact)
        checkouts[artefact] = {'fingerprint': fingerprint(rust, paths), 'files': _stats(paths, rust)}
    d = {'source_hash': module_hash, 'version': version, 'origin': origin, 'checkouts': checkouts}
    tmp = STAMP + '.tmp'
    with open(tmp, 'w', encoding='utf-8') as f:
        json.dump(d, f, indent=1, sort_keys=True)
    os.replace(tmp, STAMP)


def _checkout(rust: str = RUST, artefact: str = 'module') -> tuple[str | None, bool]:
    r"""(this checkout's fingerprint of `artefact`, whether it came from bc2_native.json's cache): the cache while
    every source file has the size and time it recorded, else hashed now; None when there is no rust\ folder (no
    sources to compare with)."""
    if not os.path.isfile(os.path.join(rust, 'Cargo.toml')):
        return None, False
    paths = sources(rust, artefact)
    cached = (read_stamp().get('checkouts') or {}).get(artefact) or {}
    if cached.get('files') == _stats(paths, rust) and isinstance(cached.get('fingerprint'), str):
        return str(cached['fingerprint']), True
    return fingerprint(rust, paths), False


def checkout_fingerprint(rust: str = RUST, artefact: str = 'module') -> str | None:
    r"""This checkout's fingerprint of `artefact` (ARTEFACTS; None without a rust\ folder), from the cache when it
    holds."""
    return _checkout(rust, artefact)[0]


def _refresh_cache(fp: str, rust: str = RUST, artefact: str = 'module') -> None:
    """The stamp's checkout cache of `artefact` brought up to date (the files' times changed, their contents give
    the built artefact's fingerprint)."""
    d = read_stamp()
    if not d:
        return
    try:
        got = d.get('checkouts')
        checkouts: dict[str, Any] = got if isinstance(got, dict) else {}
        checkouts[artefact] = {'fingerprint': fp, 'files': _stats(sources(rust, artefact), rust)}
        d['checkouts'] = checkouts
        tmp = f'{STAMP}.tmp-{os.getpid()}'
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
    return (f'Get the right one with "{py} {BUILD} --download" (the prebuilt module and bfbc2.exe for this '
            f'checkout: Windows, 64-bit Python 3.12 or later) or build it with "{py} {BUILD}" (needs Rust and '
            f'maturin: README.md, "The Rust core").')


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
    _binaries.clear()


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


COMPILE_CACHE_ENV = 'BFBC2_COMPILE_CACHE'     # 1: the suite's cargo runs go through sccache (off by default)
COMPILE_CACHE_GB = 'BFBC2_COMPILE_CACHE_GB'   # its cap (default 6)


def compile_cache() -> str | None:
    """sccache's path when the suite's cargo runs go through it (round TS1): BFBC2_COMPILE_CACHE=1, sccache installed
    (on PATH or in %USERPROFILE%\\.cargo\\bin) and no RUSTC_WRAPPER of the user's. Off by default: measured in TS1
    (rust/BENCHMARKS.md), a second build folder took 2 of 208 compiles from the cache even with SCCACHE_BASEDIRS -
    cargo gives every crate with a build script an OUT_DIR inside the build folder, which sccache 0.19 keeps in the
    key, and every crate above it misses in turn - so the cache serves only rebuilds of the same folder."""
    if os.environ.get(COMPILE_CACHE_ENV, '').strip() != '1' or os.environ.get('RUSTC_WRAPPER'):
        return None
    import shutil
    found = shutil.which('sccache')
    if found:
        return found
    home = os.path.join(os.path.expanduser('~'), '.cargo', 'bin', 'sccache' + EXE)
    return home if os.path.isfile(home) else None


def compile_cache_dir() -> str:
    """The shared compile cache's folder: %LOCALAPPDATA%\\bfbc2-tool-suite\\sccache."""
    base = os.environ.get('LOCALAPPDATA') or os.path.expanduser('~')
    return os.path.join(base, 'bfbc2-tool-suite', 'sccache')


def cargo_env(env: dict[str, str] | None = None) -> dict[str, str]:
    """env (default: this process's) with CARGO_TARGET_DIR set to target_dir(): every cargo the suite runs builds
    there. With the compile cache (compile_cache()): RUSTC_WRAPPER is sccache, its folder and cap the suite's, and
    CARGO_INCREMENTAL 0 (sccache caches no incremental build)."""
    out = dict(os.environ if env is None else env)
    out['CARGO_TARGET_DIR'] = target_dir()
    wrapper = compile_cache()
    if wrapper and not out.get('RUSTC_WRAPPER'):
        out['RUSTC_WRAPPER'] = wrapper
        out.setdefault('SCCACHE_DIR', compile_cache_dir())
        out.setdefault('SCCACHE_CACHE_SIZE', f"{os.environ.get(COMPILE_CACHE_GB, '').strip() or '6'}G")
        out.setdefault('SCCACHE_BASEDIRS', compile_cache_basedirs())
        out['CARGO_INCREMENTAL'] = '0'
    return out


def compile_cache_basedirs() -> str:
    """SCCACHE_BASEDIRS: every build folder under targets_root() and this checkout's, ';'-separated. cargo gives each
    compile its build folder's absolute paths (-L dependency=..., --extern ...), so without these a dependency
    compiled for one worktree's folder never served another's (round TS1: 0 hits of 207 on a second folder); sccache
    takes them relative to the build folder. Only build folders: a checkout's own path stays in the key, so a
    workspace crate (whose env!("CARGO_MANIFEST_DIR") is baked in) is never taken from another worktree. The server
    reads this when it starts: a build folder made while it runs shares from its next start (it stops after 10 idle
    minutes)."""
    folders = {os.path.normpath(target_dir())}
    root = targets_root()
    if os.path.isdir(root):
        folders |= {os.path.join(root, d) for d in os.listdir(root) if os.path.isdir(os.path.join(root, d))}
    return ';'.join(sorted(folders))


def start_compile_cache() -> None:
    """Starts sccache's server now, from the caller (run_tests.py, before any test's job object exists): a server
    started inside a test's job would end with that test (the jobs are KILL_ON_JOB_CLOSE) under another worktree's
    build. Nothing without the compile cache; errors are ignored (the first compile starts it then)."""
    env = cargo_env()
    wrapper = env.get('RUSTC_WRAPPER')
    if not wrapper or 'sccache' not in os.path.basename(wrapper).lower():
        return
    with contextlib.suppress(OSError, subprocess.SubprocessError):
        subprocess.run([wrapper, '--start-server'], env=env, capture_output=True, timeout=60)


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
    # round O5: the shader tools' command lines
    'shaderdb': os.path.join(SUITE, 'bfbc2-shader-tools', 'shaderdb.py'),
    'renderqa': os.path.join(SUITE, 'bfbc2-shader-tools', 'renderqa.py'),
    'xdiff': os.path.join(SUITE, 'bfbc2-shader-tools', 'xdiff.py'),
    # round O5: the console readers, the BF1943 importer
    'apt': os.path.join(SUITE, 'bfbc2-console-readers', 'apt', 'apt.py'),
    'stfs': os.path.join(SUITE, 'bfbc2-console-readers', 'stfs', 'stfs.py'),
    'xiso': os.path.join(SUITE, 'bfbc2-console-readers', 'xiso', 'xiso.py'),
    'bf1943import': os.path.join(SUITE, 'bfbc2-bf1943-import', 'bf1943import.py'),
    # round EX1: LevelDiff's command line (LevelDiff.exe, C#, retired; the path is the record of the source it ported)
    'leveldiff': os.path.join(SUITE, 'bfbc2-diff-workbench', 'level-diff', 'LevelDiff.cs'),
}
_cli: dict[str, str | None] = {}
_binaries: dict[str, tuple[str, str | None, str]] = {}


def cli_path(profile: str | None = None) -> str:
    """The command-line binary (rust\\bc2-cli: bfbc2.exe): its stable place bin\\bfbc2.exe (build_native.py copies
    it there), or with a profile where cargo builds it."""
    return bin_path('bfbc2') if profile is None else built_path('bfbc2', profile)


def _candidates_of(name: str) -> list[str]:
    return [p for p in (bin_path(name), built_path(name, 'release'), built_path(name, 'debug'),
                        os.path.join(RUST, 'target', 'release', name + EXE),
                        os.path.join(RUST, 'target', 'debug', name + EXE)) if os.path.isfile(p)]


def binary_status(name: str) -> tuple[str, str | None, str]:
    """('ok', path, '') for the first of find_binary's places whose binary was built from this checkout's Rust
    sources (`<binary> --source-hash` is checkout_fingerprint(artefact=name)); ('stale', the first found, why) when
    every copy there was built from other sources (or by cargo alone, without a fingerprint); ('missing', None, why)
    when none is there. Asked once per process and name (reset() forgets)."""
    if name not in _binaries:
        found = _candidates_of(name)
        want = checkout_fingerprint(artefact=name) if name in ARTEFACTS else None
        status: tuple[str, str | None, str] = ('missing', None, f'{bin_path(name)} is not built or downloaded')
        for exe in found:
            try:
                r = subprocess.run([exe, '--source-hash'], capture_output=True, text=True, timeout=30)
                have = r.stdout.strip() if r.returncode == 0 else None
            except (OSError, subprocess.SubprocessError):
                have = None
            if have is not None and (want is None or have == want):
                status = ('ok', exe, '')
                break
            if status[0] == 'missing':
                status = ('stale', exe, f'{exe} was built from other Rust sources than this checkout has (binary '
                                        f'{(have or "without a fingerprint")[:12]}, checkout {(want or "")[:12]})')
        _binaries[name] = status
    return _binaries[name]


def binary(name: str) -> str | None:
    """The binary `name` (bfbc2, bfbc2-editor, mapexport) when it is built from this checkout's Rust sources, else
    None (binary_status says why)."""
    status, path, _ = binary_status(name)
    return path if status == 'ok' else None


def cli_exe() -> str | None:
    """The command-line binary when it is built from this checkout's Rust sources (its --source-hash is the
    checkout's fingerprint of bfbc2: build_native.py builds it with the module), else None. Asked once per
    process."""
    return binary('bfbc2')


# Round RT1 (2026-11-20): the command lines whose Python was retired - bfbc2.exe answers them alone, and the
# lines its port does not read (damaged or unusual input) end with an error instead of the hand-off to the Python.
# Their CLI_TOOLS paths stay as the record of the file each one ported (it no longer exists).
# Round EX1: leveldiff, whose C# LevelDiff.exe was retired (it never had a Python): only the binary as well.
# Round RT3: aptrefs, xiso, culltree, stockmp, crashreader - their libraries moved to <tool>lib.py (aptrefslib,
# xisolib, culltreelib, stockmplib, crashreaderlib), their .py entry points deleted.
# The rule when the Rust port cannot answer (round RT3, audit AU2 A17): while a tool's Python is alive it answers
# (bfbc2.exe hands the command line to it); once retired, a plain error, never a fallback. RETIRED_ROUND is the
# round each was retired in (bfbc2.exe's own list, rust\bc2-cli\src\pyproc.rs RETIRED, must agree:
# tests\test_native_required.py checks it; leveldiff is not a Python port, so it is not in pyproc's list). A module's
# binding follows the same rule: None (its Python reference answers) while the reference is alive, Refused after.
RETIRED_ROUND = {'graphstats': 'RT1', 'prepare_level': 'RT1', 'leveldiff': 'EX1', 'aptrefs': 'RT3', 'xiso': 'RT3',
                 'culltree': 'RT3', 'stockmp': 'RT3', 'crashreader': 'RT3'}
RETIRED_CLI = frozenset(RETIRED_ROUND)


class Refused(ValueError):
    """Round RT2: an input the Rust core does not read, given to a function whose Python reference (its fallback) was
    retired. The reference answered or raised there; the function now raises this (a ValueError, and a subclass of
    the module's own error where it has one: refused())."""


_refused_types: dict[type, type] = {}


def refused(message: str, base: type[Exception] = ValueError) -> Exception:
    """The error a retired reference's function raises for an input the Rust core refuses (a Refused that is also a
    `base`, so `except <the module's error>` still catches it); message: what is wrong, as far as the caller knows."""
    cls = Refused if base in (ValueError, Exception) else _refused_types.get(base)
    if cls is None:
        cls = _refused_types[base] = type('Refused', (Refused, base), {'__module__': __name__})
    return cls(message)


class CliMissing(NativeMissing):
    """A retired command line needs bin\\bfbc2.exe built (or downloaded) from this checkout's Rust sources."""


def _cli_why() -> str:
    status, _, why = binary_status('bfbc2')
    return why if status != 'ok' else ''


class NativeStale(NativeMissing):
    """BFBC2_STRICT_NATIVE=1: a ported tool's command line found bfbc2.exe stale or missing instead of falling back to
    its Python."""


def _cli_fix() -> str:
    py = os.path.basename(sys.executable).removesuffix('.exe') or 'python'
    return f'Build it again with "{py} {BUILD}" (or "{py} {BUILD} --download" for a prebuilt one).'


def _cli_fallback(tool: str) -> None:
    """A ported tool's command line falls back to its Python because bfbc2.exe is stale (or missing): one warning
    per process on stderr when it is stale, naming the fix (round QW1, audit AU2 A6; the Python is 1.3 to 18 times
    slower); BFBC2_STRICT_NATIVE=1 makes a stale or missing binary an error (NativeStale)."""
    status, _, why = binary_status('bfbc2')
    if os.environ.get(STRICT_ENV, '').strip() == '1':
        raise NativeStale(f'bfbc2 {tool}: {why} ({STRICT_ENV}=1 refuses the Python fall-back). {_cli_fix()}')
    if status == 'stale' and not _cli.get('warned'):
        _cli['warned'] = 'yes'
        warnings.warn(f'bfbc2core: {why}; the tools\' command lines run in Python instead (slower). {_cli_fix()}',
                      RuntimeWarning, stacklevel=3)


def cli_command(tool: str) -> list[str]:
    """The command line that runs a ported tool (CLI_TOOLS): [bfbc2.exe, tool] when the binary is built from this
    checkout, else (or with BFBC2_CLI_PYTHON=1) [python, the tool's .py] - the same arguments, output and exit
    codes either way (rust/BENCHMARKS.md, "O1"). A retired tool (RETIRED_CLI: its .py deleted in round RT1) has
    only the binary: without it CliMissing says how to get it (BFBC2_CLI_PYTHON does not apply)."""
    script = CLI_TOOLS[tool]
    if tool in RETIRED_CLI:
        exe = cli_exe()
        if exe is None:
            raise CliMissing(f'bfbc2 {tool} needs the suite\'s command-line binary (its Python command line was '
                             f'retired): {_cli_why()}. {_fix()}')
        return [exe, tool]
    if os.environ.get(CLI_PYTHON_ENV, '').strip() == '1':
        return [sys.executable, script]
    exe = cli_exe()
    if exe is None:
        _cli_fallback(tool)
    return [exe, tool] if exe else [sys.executable, script]
