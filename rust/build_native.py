r"""Builds the suite's Rust core for Python: rust\bc2-py with maturin (release), its module put into
core\bfbc2core\bc2_native.pyd, where bfbc2core.dbx finds it (README.md, "The Rust core").

  python rust\build_native.py              build and install into core\bfbc2core
  python rust\build_native.py --if-stale   only when the module is missing or older than a Rust source
  python rust\build_native.py --remove     delete the built module (the tools fall back to Python)

Needs Rust (cargo: on PATH or in %USERPROFILE%\.cargo\bin) and maturin (pip install maturin). Without them it
says so and exits 0 under --if-stale (the suite then runs on the Python reader), 1 otherwise.
"""
import contextlib
import glob
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
TARGET = os.path.join(SUITE, 'core', 'bfbc2core', 'bc2_native.pyd' if os.name == 'nt' else 'bc2_native.so')


def sources() -> list[str]:
    """Every file whose change makes a new module: the Rust sources and the workspace / crate manifests."""
    out = [os.path.join(RUST, n) for n in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')]
    # every crate of the workspace (a folder of rust\ with a Cargo.toml): a new crate needs no entry here
    for crate in sorted(d for d in os.listdir(RUST) if os.path.isfile(os.path.join(RUST, d, 'Cargo.toml'))):
        out += [os.path.join(RUST, crate, 'Cargo.toml')]
        out += glob.glob(os.path.join(RUST, crate, 'src', '**', '*.rs'), recursive=True)
    return [p for p in out if os.path.isfile(p)]


def stale() -> bool:
    if not os.path.isfile(TARGET):
        return True
    built = os.path.getmtime(TARGET)
    return any(os.path.getmtime(s) > built for s in sources())


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


def build(env: dict[str, str]) -> int:
    with tempfile.TemporaryDirectory() as out:
        # quiet and plain: maturin's messages carry emoji a cp1252 console cannot print; its output (cargo's errors
        # on a failure) is shown only on a failure, escaped to ASCII
        r = subprocess.run([sys.executable, '-m', 'maturin', 'build', '--release', '--quiet', '--color', 'never',
                            '--interpreter', sys.executable,
                            '--manifest-path', os.path.join(RUST, 'bc2-py', 'Cargo.toml'), '--out', out], env=env,
                           capture_output=True)
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
            tmp = TARGET + '.new'
            with z.open(mods[0]) as src, open(tmp, 'wb') as dst:
                shutil.copyfileobj(src, dst)
            install(tmp)
    print(f'built {TARGET}')
    return 0


def main(argv: list[str]) -> int:
    if '--remove' in argv:
        if os.path.isfile(TARGET):
            os.replace(TARGET, TARGET + '.old-removed')     # renaming works even while a tool has it loaded
            print(f'removed {TARGET}')
        for old in glob.glob(TARGET + '.old-*'):
            with contextlib.suppress(OSError):
                os.remove(old)
        return 0
    if_stale = '--if-stale' in argv
    if if_stale and not stale():
        print(f'up to date: {os.path.basename(TARGET)}')
        return 0
    env = cargo_env()
    if env is None or not have_maturin():
        print('skipped: ' + ('Rust (cargo) is not installed' if env is None else 'maturin is not installed (pip install maturin)')
              + ' - the suite uses the Python DBX reader')
        return 0 if if_stale else 1
    return build(env)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
