r"""Builds the suite's Rust core for Python, or downloads the prebuilt one: rust\bc2-py with maturin (release), its
module put into core\bfbc2core\bc2_native.pyd, where bfbc2core finds it (README.md, "The Rust core").

  python rust\build_native.py              build and install into core\bfbc2core
  python rust\build_native.py --download   download the prebuilt module and bin\bfbc2.exe for this checkout's Rust
                                           sources from the suite's GitHub Releases instead (Windows, 64-bit Python
                                           3.12 or later)
  python rust\build_native.py --if-stale   only when the module is missing or was made from other sources: build it
                                           when Rust and maturin are there, else download it
  python rust\build_native.py --remove     delete the module (the module is required since round F1a: the core
                                           formats - dbx, fbrb, terrain, water, collision, level - texture conversion
                                           and NavKit's builds then stop with an error saying how to get it)
  python rust\build_native.py --fingerprint [ARTEFACT]   print this checkout's source fingerprint of the module (the
                                           release tag's), or of bfbc2, bfbc2-editor or mapexport
  python rust\build_native.py --editor     build the map editor (bfbc2-editor.exe) and mapexport.exe into bin\
  python rust\build_native.py --target-dir print cargo's build folder for this checkout
  python rust\build_native.py --cargo ARGS...   run cargo ARGS in rust\ with this checkout's build folder (use it
                                           instead of a bare cargo, which would build into rust\target)

A build also builds the command-line binary bfbc2.exe (round O1: the tools' command lines in Rust, "bfbc2 fbrbtool
...", "bfbc2 dbxedit ...") with its own fingerprint (round QW1: each artefact's over the crates it links,
bfbc2core.native.ARTEFACTS); --if-stale builds only what is stale; a download fetches it too (round RT1: the command lines whose
Python was retired - bfbc2core.native.RETIRED_CLI - have only it; a release made before RT1 has the module only).

Where things go (round D1, 2026-11-19): cargo builds in C:\Programming\Projects\rust-targets\<checkout folder name>
(bfbc2core.native.target_dir: CARGO_TARGET_DIR when set, BFBC2_RUST_TARGETS for another root), never in rust\target;
the binaries the tools run are copied to the suite's bin\ (bin\bfbc2.exe, bin\bfbc2-editor.exe, bin\mapexport.exe),
a place that does not depend on the build folder. Removing a checkout leaves its build folder: remove it with
python -m bfbc2core.caches --prune-targets (every folder of the root whose checkout is gone).

Building needs Rust (cargo: on PATH or in %USERPROFILE%\.cargo\bin) and maturin (pip install maturin). The module
carries the fingerprint of the Rust sources it was built from (bfbc2core.native: a SHA-256 over sources(), line
endings normalized), baked in through BC2_SOURCE_HASH; bc2_native.json beside it records it, and bfbc2core uses a
module only when it matches the checkout's.

The download: the release tagged native-<the fingerprint's first 12 hex digits> of the suite's repository
(.github/workflows/native.yml publishes one for each push to main that changes the Rust sources), its assets
bc2_native.pyd and bc2_native.pyd.sha256, bfbc2.exe and bfbc2.exe.sha256 (round RT1). The SHA-256 is checked, then
the module's own fingerprint (the binary's --source-hash), before it replaces anything. Plain HTTPS GETs with a generic User-Agent; nothing about the user or the PC is sent.
BFBC2_NATIVE_REPO=owner/name downloads from another repository (a fork).
"""
import contextlib
import glob
import hashlib
import json
import os
import platform
import shutil
import struct
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import zipfile

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
TARGET = os.path.join(SUITE, 'core', 'bfbc2core', 'bc2_native.pyd' if os.name == 'nt' else 'bc2_native.so')
REPO = os.environ.get('BFBC2_NATIVE_REPO') or 'mathewsbinny/bfbc2-tool-suite'
ASSET = 'bc2_native.pyd'
USER_AGENT = 'bfbc2-tool-suite-build-native'

if os.path.join(SUITE, 'core') not in sys.path:
    sys.path.insert(0, os.path.join(SUITE, 'core'))
from bfbc2core import atomicio, native  # noqa: E402  (never imports the module itself: a loaded .pyd cannot be replaced)


def sources() -> list[str]:
    """Every file whose change makes a new module (bfbc2core.native.sources)."""
    return native.sources(RUST)


def fingerprint(artefact: str = 'module') -> str:
    """This checkout's source fingerprint of `artefact` (bfbc2core.native.fingerprint over native.sources(artefact));
    the module's is the release tag's."""
    return native.fingerprint(RUST, native.sources(RUST, artefact))


def tag(fp: str) -> str:
    """The release tag of the prebuilt module for fingerprint fp."""
    return f'native-{fp[:12]}'


def stale() -> bool:
    """The module is missing, has no bc2_native.json, or was made from other sources than the checkout's."""
    if not os.path.isfile(TARGET):
        return True
    have = native.read_stamp().get('source_hash')
    return not have or have != native.checkout_fingerprint(RUST)


def cli_stale() -> bool:
    """bin\\bfbc2.exe is missing or was built from other sources than the checkout's fingerprint of bfbc2."""
    native.reset()
    return native.binary_status('bfbc2')[0] != 'ok'


def cargo_env() -> dict[str, str] | None:
    """The environment with cargo on PATH and CARGO_TARGET_DIR this checkout's build folder (native.target_dir),
    or None when Rust is not installed."""
    env = native.cargo_env()
    if shutil.which('cargo') is None:
        home = os.path.join(os.path.expanduser('~'), '.cargo', 'bin')
        if not os.path.isfile(os.path.join(home, 'cargo.exe' if os.name == 'nt' else 'cargo')):
            return None
        env['PATH'] = home + os.pathsep + env.get('PATH', '')
    env.setdefault('PYO3_PYTHON', sys.executable)
    return env


def have_maturin() -> bool:
    return subprocess.run([sys.executable, '-m', 'maturin', '--version'], capture_output=True).returncode == 0


def install(new: str, target: str | None = None) -> None:
    """new in target's place (TARGET, the module, by default - read at the call: tests point it elsewhere). A module
    a running Python has loaded (or a running exe) cannot be replaced or deleted on Windows, but it can be renamed:
    it moves aside (<name>.old-N, deleted by a later build once nothing holds it) and the running tools keep the old
    code until they restart."""
    target = target or TARGET
    for old in glob.glob(target + '.old-*'):
        with contextlib.suppress(OSError):
            os.remove(old)
    try:
        atomicio.replace(new, target)       # tried again while a scanner or indexer holds it for a moment
        return
    except OSError as e:
        if not atomicio._transient(e) or not os.path.exists(target):
            raise
    # still held: a running tool has it loaded. It can be renamed, not replaced: moved aside, the new one put in its
    # place, and put back if that fails, so the live file is never left moved aside (audit AU4 DF4)
    n = 0
    while os.path.exists(f'{target}.old-{n}'):
        n += 1
    aside = f'{target}.old-{n}'
    atomicio.replace(target, aside)
    try:
        atomicio.replace(new, target)
    except BaseException:
        os.replace(aside, target)
        raise
    print(f'build_native: {os.path.basename(target)} is in use by a running tool; moved aside')


def install_binary(name: str, profile: str = 'release') -> str:
    """cargo's newly built binary `name` copied to its stable place bin\\<name>.exe (native.bin_path; written beside
    it, then moved in: a running copy is moved aside). Returns the path."""
    src, dst = native.built_path(name, profile), native.bin_path(name)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    tmp = dst + '.new'
    shutil.copyfile(src, tmp)
    install(tmp, dst)
    return dst


def module_info(path: str) -> dict[str, str] | None:
    """{'source_hash', 'version'} of the module file at path, read by importing a copy in another Python process
    (this one must not load it: Windows would lock the file); None when it does not load."""
    with tempfile.TemporaryDirectory() as d:
        shutil.copyfile(path, os.path.join(d, os.path.basename(TARGET)))
        code = ('import json, sys; sys.path.insert(0, sys.argv[1]); import bc2_native as m; '
                'print(json.dumps({"source_hash": getattr(m, "__source_hash__", ""), '
                '"version": getattr(m, "__version__", "")}))')
        r = subprocess.run([sys.executable, '-I', '-c', code, d], capture_output=True, text=True)
    if r.returncode:
        return None
    try:
        return dict(json.loads(r.stdout.strip().splitlines()[-1]))
    except (ValueError, IndexError):
        return None


def build(env: dict[str, str]) -> int:
    fp = fingerprint()
    cli_env = env
    env = dict(env, **{native.SOURCE_ENV: fp})
    tmp = TARGET + '.new'
    with tempfile.TemporaryDirectory() as out:
        # quiet and plain: maturin's messages carry emoji a cp1252 console cannot print; its output (cargo's errors
        # on a failure) is shown only on a failure, escaped to ASCII
        r = subprocess.run([sys.executable, '-m', 'maturin', 'build', '--release', '--quiet', '--color', 'never',
                            '--interpreter', sys.executable,
                            '--manifest-path', os.path.join(RUST, 'bc2-py', 'Cargo.toml'), '--out', out], env=env,
                           capture_output=True, cwd=RUST)     # from rust/: rustup picks rust-toolchain.toml's toolchain
        if r.returncode:
            text = (r.stdout + r.stderr).decode('utf-8', errors='replace')
            print(text.encode('ascii', errors='backslashreplace').decode('ascii'))
            print(f'build_native: maturin failed (exit code {r.returncode})')
            return r.returncode
        wheels = glob.glob(os.path.join(out, '*.whl'))
        if len(wheels) != 1:
            print(f'build_native: expected one wheel, found {len(wheels)}')
            return 1
        with zipfile.ZipFile(wheels[0]) as z:
            mods = [n for n in z.namelist() if n.rsplit('/', 1)[-1].startswith('bc2_native') and n.endswith(('.pyd', '.so'))]
            if len(mods) != 1:
                print(f'build_native: expected one module in {os.path.basename(wheels[0])}, found {mods}')
                return 1
            with z.open(mods[0]) as src, open(tmp, 'wb') as dst:
                shutil.copyfileobj(src, dst)
    info = module_info(tmp)
    if info is None or info.get('source_hash') != fp:
        os.remove(tmp)
        print(f'build_native: the built module does not carry this checkout\'s fingerprint ({info}); not installed')
        return 1
    install(tmp)
    native.write_stamp(fp, 'build', info.get('version', ''), RUST)
    print(f'built {TARGET} (sources {fp[:12]})')
    return build_cli(cli_env)


def build_cli(env: dict[str, str]) -> int:
    """The command-line binary bfbc2.exe (round O1: the tools' command lines in Rust), with its fingerprint baked in
    (round QW1: the checkout's fingerprint of bfbc2, over the crates it links): bfbc2core.native.cli_exe uses it only
    while that matches. Built in the build folder, copied to bin\\bfbc2.exe. bin\\python.txt names this Python (round O3): the interpreter bfbc2.exe hands a command
    line to where the Rust refuses it, when BFBC2_PYTHON is not set (then python.exe on PATH)."""
    env = dict(env, **{native.SOURCE_ENV: fingerprint('bfbc2')})
    if _cargo_build(env, ['-p', 'bc2-cli'], 'the command-line binary'):
        return 1
    print(f'built {install_binary("bfbc2")}')
    with open(os.path.join(native.BIN, 'python.txt'), 'w', encoding='utf-8') as f:
        f.write(sys.executable + '\n')
    return 0


def _cargo_build(env: dict[str, str], args: list[str], what: str) -> int:
    r = subprocess.run(['cargo', 'build', '--release', '--quiet', '--color', 'never', *args], env=env,
                       capture_output=True, cwd=RUST)
    if r.returncode:
        text = (r.stdout + r.stderr).decode('utf-8', errors='replace')
        print(text.encode('ascii', errors='backslashreplace').decode('ascii'))
        print(f'build_native: {what} failed to build (exit code {r.returncode})')
    return r.returncode


def build_editor(env: dict[str, str]) -> int:
    """The map editor (rust\\bc2-editor: bfbc2-editor.exe) and mapexport.exe (bc2-levelwrite), release, copied to
    bin\\, each with its own fingerprint baked in (round QW1: bfbc2core.native.binary finds them only while it is the
    checkout's)."""
    for name, crate in (('bfbc2-editor', 'bc2-editor'), ('mapexport', 'bc2-levelwrite')):
        if _cargo_build(dict(env, **{native.SOURCE_ENV: fingerprint(name)}), ['-p', crate, '--bin', name], name):
            return 1
        print(f'built {install_binary(name)}')
    return 0


def run_cargo(env: dict[str, str], args: list[str]) -> int:
    """cargo args in rust\\ with this checkout's build folder (where a bare cargo would use rust\\target)."""
    cargo = shutil.which('cargo', path=env.get('PATH')) or 'cargo'
    return subprocess.run([cargo, *args], env=env, cwd=RUST).returncode


def _get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
    with urllib.request.urlopen(req, timeout=120) as r:
        return bytes(r.read())


def can_download() -> str | None:
    """Why this Python cannot use the prebuilt module, or None: it is built for 64-bit Windows and the stable ABI of
    Python 3.12 and later (abi3; the suite needs 3.12 anyway: pyproject.toml)."""
    if os.name != 'nt' or platform.machine().lower() not in ('amd64', 'x86_64'):
        return f'the prebuilt module is for 64-bit Windows (this is {platform.system()} {platform.machine()})'
    if struct.calcsize('P') != 8:
        return 'the prebuilt module needs a 64-bit Python'
    return None


def download() -> int:
    """The prebuilt module for this checkout's fingerprint, from the repository's GitHub Releases, checked and
    installed like a build."""
    why = can_download()
    if why:
        print(f'build_native: {why}; build it instead (README.md, "The Rust core")')
        return 1
    fp = fingerprint()
    base = f'https://github.com/{REPO}/releases/download/{tag(fp)}/'
    print(f'downloading the Rust core for sources {fp[:12]} ({base}{ASSET})')
    try:
        want = _get(base + ASSET + '.sha256').decode('ascii', errors='replace').split()[0].lower()
        data = _get(base + ASSET)
    except urllib.error.HTTPError as e:
        if e.code == 404:
            print(f'build_native: no prebuilt module for this checkout\'s Rust sources (release {tag(fp)} of {REPO} '
                  f'has no {ASSET}). The release workflow makes one for each push to main that changes rust\\; a '
                  f'checkout with local Rust changes, or a commit that was never the head of a push, has none. '
                  f'Build it: python rust\\build_native.py (needs Rust and maturin)')
        else:
            print(f'build_native: the download failed: HTTP {e.code} {e.reason}')
        return 1
    except (urllib.error.URLError, OSError, IndexError) as e:
        print(f'build_native: the download failed: {e}')
        return 1
    got = hashlib.sha256(data).hexdigest()
    if got != want:
        print(f'build_native: the downloaded module\'s SHA-256 is {got}, the release says {want}; not installed')
        return 1
    tmp = TARGET + '.new'
    with open(tmp, 'wb') as f:
        f.write(data)
    info = module_info(tmp)
    if info is None or info.get('source_hash') != fp:
        os.remove(tmp)
        print(f'build_native: the downloaded module does not load or is not for these sources ({info}); not installed')
        return 1
    install(tmp)
    native.write_stamp(fp, 'download', info.get('version', ''), RUST)
    print(f'downloaded {TARGET} (sources {fp[:12]}, sha256 {got[:12]})')
    return download_cli(base, fp)


CLI_ASSET = 'bfbc2.exe'


def binary_hash(path: str) -> str | None:
    """`<path> --source-hash` (the fingerprint the binary was built from), or None when it does not run."""
    try:
        r = subprocess.run([path, '--source-hash'], capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def download_cli(base: str, fp: str) -> int:
    """The release's command-line binary bfbc2.exe (round RT1: the command lines whose Python was retired have only
    it) into bin\\bfbc2.exe, checked like the module (its SHA-256, then its --source-hash: the checkout's fingerprint
    of bfbc2, round QW1; the release is found by the module's, fp), with bin\\python.txt
    naming this Python (the interpreter it hands the other tools' unusual command lines to). A release made before
    RT1 has none: said, and the module stays installed (exit code 0)."""
    try:
        want = _get(base + CLI_ASSET + '.sha256').decode('ascii', errors='replace').split()[0].lower()
        data = _get(base + CLI_ASSET)
    except urllib.error.HTTPError as e:
        if e.code == 404:
            print(f'build_native: this release has no {CLI_ASSET} (made before round RT1); the command lines whose '
                  f'Python was retired need it built: python rust\\build_native.py (needs Rust and maturin)')
            return 0
        print(f'build_native: the download of {CLI_ASSET} failed: HTTP {e.code} {e.reason}')
        return 1
    except (urllib.error.URLError, OSError, IndexError) as e:
        print(f'build_native: the download of {CLI_ASSET} failed: {e}')
        return 1
    got = hashlib.sha256(data).hexdigest()
    if got != want:
        print(f'build_native: the downloaded {CLI_ASSET}\'s SHA-256 is {got}, the release says {want}; not installed')
        return 1
    dst = native.bin_path('bfbc2')
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    tmp = dst + '.new'
    with open(tmp, 'wb') as f:
        f.write(data)
    have = binary_hash(tmp)
    if have != fingerprint('bfbc2'):
        os.remove(tmp)
        print(f'build_native: the downloaded {CLI_ASSET} does not run or is not for these sources ({have}); not '
              f'installed')
        return 1
    install(tmp, dst)
    with open(os.path.join(native.BIN, 'python.txt'), 'w', encoding='utf-8') as f:
        f.write(sys.executable + '\n')
    print(f'downloaded {dst} (sources {fp[:12]}, sha256 {got[:12]})')
    return 0


def main(argv: list[str]) -> int:
    if '--fingerprint' in argv:
        rest = argv[argv.index('--fingerprint') + 1:]
        artefact = rest[0] if rest else 'module'
        if artefact not in native.ARTEFACTS:
            print(f'build_native: no artefact {artefact!r} ({", ".join(native.ARTEFACTS)})')
            return 2
        print(fingerprint(artefact))
        return 0
    if '--remove' in argv:
        if os.path.isfile(TARGET):
            os.replace(TARGET, TARGET + '.old-removed')     # renaming works even while a tool has it loaded
            print(f'removed {TARGET}')
        with contextlib.suppress(OSError):
            os.remove(native.STAMP)
        for old in glob.glob(TARGET + '.old-*'):
            with contextlib.suppress(OSError):
                os.remove(old)
        return 0
    if '--download' in argv:
        return download()
    if '--target-dir' in argv:
        print(native.target_dir())
        return 0
    if '--cargo' in argv or '--editor' in argv:
        cenv = cargo_env()
        if cenv is None:
            print('build_native: Rust (cargo) is not installed')
            return 1
        if '--cargo' in argv:
            return run_cargo(cenv, argv[argv.index('--cargo') + 1:])
        return build_editor(cenv)
    if_stale = '--if-stale' in argv
    module_stale = stale()
    if if_stale and not module_stale and not cli_stale():
        print(f'up to date: {os.path.basename(TARGET)}, bfbc2.exe')
        return 0
    env = cargo_env()
    if env is None or not have_maturin():
        missing = 'Rust (cargo) is not installed' if env is None else 'maturin is not installed (pip install maturin)'
        if if_stale:
            print(f'build_native: {missing}; trying the prebuilt module')
            return download()
        print(f'build_native: {missing}. Download the prebuilt module instead: python rust\\build_native.py --download')
        return 1
    if if_stale and not module_stale:
        print(f'up to date: {os.path.basename(TARGET)}')
        return build_cli(env)                   # only bfbc2.exe's crates changed (round QW1)
    return build(env)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
