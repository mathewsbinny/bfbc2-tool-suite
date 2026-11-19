r"""Builds the suite's Rust core for Python, or downloads the prebuilt one: rust\bc2-py with maturin (release), its
module put into core\bfbc2core\bc2_native.pyd, where bfbc2core finds it (README.md, "The Rust core").

  python rust\build_native.py              build and install into core\bfbc2core
  python rust\build_native.py --download   download the prebuilt module for this checkout's Rust sources from the
                                           suite's GitHub Releases instead (Windows, 64-bit Python 3.12 or later)
  python rust\build_native.py --if-stale   only when the module is missing or was made from other sources: build it
                                           when Rust and maturin are there, else download it
  python rust\build_native.py --remove     delete the module (texture conversion and NavKit's builds then stop with
                                           an error saying how to get it; the readers run in Python)
  python rust\build_native.py --fingerprint   print this checkout's source fingerprint (the release tag's)

A build also builds the command-line binary rust\target\release\bfbc2.exe (round O1: the tools' command lines in
Rust, "bfbc2 fbrbtool ...", "bfbc2 dbxedit ...") with the same fingerprint; a download fetches the module only (the
Python command lines then answer, as before).

Building needs Rust (cargo: on PATH or in %USERPROFILE%\.cargo\bin) and maturin (pip install maturin). The module
carries the fingerprint of the Rust sources it was built from (bfbc2core.native: a SHA-256 over sources(), line
endings normalized), baked in through BC2_SOURCE_HASH; bc2_native.json beside it records it, and bfbc2core uses a
module only when it matches the checkout's.

The download: the release tagged native-<the fingerprint's first 12 hex digits> of the suite's repository
(.github/workflows/native.yml publishes one for each push to main that changes the Rust sources), its assets
bc2_native.pyd and bc2_native.pyd.sha256. The SHA-256 is checked, then the module's own fingerprint, before it
replaces anything. Plain HTTPS GETs with a generic User-Agent; nothing about the user or the PC is sent.
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
from bfbc2core import native  # noqa: E402  (never imports the module itself: a loaded .pyd cannot be replaced)


def sources() -> list[str]:
    """Every file whose change makes a new module (bfbc2core.native.sources)."""
    return native.sources(RUST)


def fingerprint() -> str:
    """This checkout's source fingerprint (bfbc2core.native.fingerprint)."""
    return native.fingerprint(RUST)


def tag(fp: str) -> str:
    """The release tag of the prebuilt module for fingerprint fp."""
    return f'native-{fp[:12]}'


def stale() -> bool:
    """The module is missing, has no bc2_native.json, or was made from other sources than the checkout's."""
    if not os.path.isfile(TARGET):
        return True
    have = native.read_stamp().get('source_hash')
    return not have or have != native.checkout_fingerprint(RUST)


def cargo_env() -> dict[str, str] | None:
    """The environment with cargo on PATH, or None when Rust is not installed."""
    env = dict(os.environ)
    if shutil.which('cargo') is None:
        home = os.path.join(os.path.expanduser('~'), '.cargo', 'bin')
        if not os.path.isfile(os.path.join(home, 'cargo.exe' if os.name == 'nt' else 'cargo')):
            return None
        env['PATH'] = home + os.pathsep + env.get('PATH', '')
    env.setdefault('PYO3_PYTHON', sys.executable)
    return env


def have_maturin() -> bool:
    return subprocess.run([sys.executable, '-m', 'maturin', '--version'], capture_output=True).returncode == 0


def install(new: str) -> None:
    """new in TARGET's place. A module a running Python has loaded cannot be replaced or deleted on Windows, but it
    can be renamed: it moves aside (bc2_native.pyd.old-N, deleted by a later build once nothing holds it) and the
    running tools keep the old code until they restart."""
    for old in glob.glob(TARGET + '.old-*'):
        with contextlib.suppress(OSError):
            os.remove(old)
    try:
        os.replace(new, TARGET)
        return
    except PermissionError:
        n = 0
        while os.path.exists(f'{TARGET}.old-{n}'):
            n += 1
        os.replace(TARGET, f'{TARGET}.old-{n}')
        print(f'build_native: {os.path.basename(TARGET)} is in use by a running tool; moved aside')
    os.replace(new, TARGET)


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
    return build_cli(env)


def build_cli(env: dict[str, str]) -> int:
    """The command-line binary rust\\target\\release\\bfbc2.exe (round O1: the tools' command lines in Rust), with the
    same fingerprint baked in (env carries it): bfbc2core.native.cli_exe uses it only while that matches."""
    r = subprocess.run(['cargo', 'build', '--release', '--quiet', '--color', 'never', '-p', 'bc2-cli'], env=env,
                       capture_output=True, cwd=RUST)
    if r.returncode:
        text = (r.stdout + r.stderr).decode('utf-8', errors='replace')
        print(text.encode('ascii', errors='backslashreplace').decode('ascii'))
        print(f'build_native: the command-line binary failed to build (exit code {r.returncode})')
        return r.returncode
    print(f'built {native.cli_path()}')
    return 0


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
    return 0


def main(argv: list[str]) -> int:
    if '--fingerprint' in argv:
        print(fingerprint())
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
    if_stale = '--if-stale' in argv
    if if_stale and not stale():
        print(f'up to date: {os.path.basename(TARGET)}')
        return 0
    env = cargo_env()
    if env is None or not have_maturin():
        missing = 'Rust (cargo) is not installed' if env is None else 'maturin is not installed (pip install maturin)'
        if if_stale:
            print(f'build_native: {missing}; trying the prebuilt module')
            return download()
        print(f'build_native: {missing}. Download the prebuilt module instead: python rust\\build_native.py --download')
        return 1
    return build(env)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
