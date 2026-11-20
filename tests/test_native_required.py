r"""The Rust core is required for texture conversion and NavKit's grid walks (their Python retired on 2026-11-17), for
the sound, mesh, physics and Granny conversions (2026-11-18) and for the core format readers and writers - DBX, bundles,
terrain, water, the collision index, the level loader (2026-11-18, round F1a) -, and a module built from other sources
is never used silently (core/bfbc2core/native.py; README.md, "The Rust core").

A copy of the suite's Python and Rust sources in a temporary folder (no game data, no cargo), three ways:
  - no module: texture.convert, navbuild.build, navwalk.walkable, audio's encoder, mesh.convert,
    havok.convert_packfile, granny.to_pc, dbx.parse / to_xml / write, fbrb.entries, terrain.parse, water.parse_mesh and
    collision.havok_index raise NativeMissing at once (before reading any input), saying how to get it
    (build_native.py --download, or build);
  - a stale module (the copy's Rust sources changed after it was built): the same error naming the mismatch, and one
    RuntimeWarning (from a reader that still has a Python twin, Granny's, found at import);
  - the matching module: they run, BFBC2_PURE_PYTHON=1 does not turn them off.
And the source fingerprint: the same with CRLF and LF line endings, different when a source changes or a crate is
added; build_native.py --download against a stand-in for GitHub (its fetch replaced; nothing goes on the network):
the release of the checkout's fingerprint, installed with its stamp only when its SHA-256 and its own fingerprint
check out. The module checks are skipped when it is not built here (python rust\build_native.py).

  python tests\test_native_required.py
"""
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(SUITE, 'core'))
from bfbc2core import native  # noqa: E402

FAILS: list[str] = []
PYD = os.path.join(SUITE, 'core', 'bfbc2core', 'bc2_native.pyd' if os.name == 'nt' else 'bc2_native.so')

# run inside the copy: prints one line per check, 'name: outcome'
PROBE = r'''
import os, sys, warnings
sys.path.insert(0, sys.argv[1])
import suite_paths
suite_paths.use('core', 'console-convert', 'navkit-tools', 'anim-tools')
warnings.simplefilter('always')
caught = []
warnings.showwarning = lambda m, *a, **k: caught.append(str(m))
from bfbc2core import collision, dbx, fbrb, native, terrain, water
import audio, granny, havok, mesh, navbuild, navwalk, texture
import numpy as np
print('warnings:', len(caught), caught[0][:80] if caught else '')
def attempt(name, fn):
    try:
        r = fn()
        print(f'{name}: ok {r!r:.40}')
    except native.NativeMissing as e:
        print(f'{name}: NativeMissing {e}')
    except Exception as e:
        print(f'{name}: {type(e).__name__} {e}')
import struct
tex = struct.pack('>8I', 3, 0, 10, 0, 8, 8, 0, 1) + bytes(60) + bytes(4096)
attempt('texture', lambda: len(texture.convert(tex)))
attempt('navbuild', lambda: navbuild.build('no such samples.bin', None, 0, 0, 'out.hpd', log=lambda *a: None))
attempt('walkable', lambda: navwalk.walkable(None))
attempt('audio', lambda: audio.xas_encode_frames(np.zeros((128, 1), np.int16)).shape)
attempt('mesh', lambda: mesh.convert(b'', []))
attempt('havok', lambda: havok.convert_packfile(bytes(64), 0, {}))
attempt('granny', lambda: granny.to_pc(bytes(16)))
attempt('dbx', lambda: len(dbx.to_xml(b'{binary}' + bytes(40))))
attempt('dbx write', lambda: len(dbx.write(dbx.DbxFile([], []))))
attempt('fbrb', lambda: fbrb.entries('no such bundle.fbrb'))
attempt('terrain', lambda: terrain.parse(b''))
attempt('water', lambda: water.parse_mesh(b''))
attempt('collision', lambda: collision.havok_index([]))
'''
RETIRED = ('texture', 'navbuild', 'walkable', 'audio', 'mesh', 'havok', 'granny', 'dbx', 'dbx write', 'fbrb', 'terrain',
           'water', 'collision')


def check(cond: bool, msg: str) -> None:
    if not cond:
        FAILS.append(msg)
        print('FAIL', msg)
    else:
        print('ok  ', msg)


def copy_suite(dst: str, with_module: bool) -> None:
    skip = shutil.ignore_patterns('target', '__pycache__', '*.pyd', '*.so', 'bc2_native.json', 'bc2_native.pyd.*',
                                  'tests', 'work', '*.hpd')
    os.makedirs(dst)
    shutil.copy2(os.path.join(SUITE, 'suite_paths.py'), dst)
    shutil.copy2(os.path.join(SUITE, 'VERSION'), dst)        # one of the sources' fingerprint (round P1)
    for d in ('core', 'bfbc2-console-convert', os.path.join('bfbc2-auto-navkit', 'tools'), 'bfbc2-anim-tools', 'rust'):
        shutil.copytree(os.path.join(SUITE, d), os.path.join(dst, d), ignore=skip)
    if with_module:
        shutil.copy2(PYD, os.path.join(dst, 'core', 'bfbc2core', os.path.basename(PYD)))


def probe(root: str, **env: str) -> dict[str, str]:
    script = os.path.join(root, 'probe.py')
    with open(script, 'w', encoding='utf-8') as f:
        f.write(PROBE)
    e = {k: v for k, v in os.environ.items() if k not in ('BFBC2_PURE_PYTHON', 'PYTHONPATH')}
    e.update(env)
    r = subprocess.run([sys.executable, script, root], cwd=root, env=e, capture_output=True, text=True, timeout=300)
    out = {}
    for line in r.stdout.splitlines():
        k, _, v = line.partition(': ')
        out[k] = v
    if r.returncode:
        check(False, f'probe failed: {r.stderr[-800:]}')
    return out


def fingerprints() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        rust = os.path.join(tmp, 'rust')
        os.makedirs(os.path.join(rust, 'a', 'src', 'deep'))
        files = {'Cargo.toml': '[workspace]\nmembers = ["a"]\n', 'Cargo.lock': 'x\n', 'rust-toolchain.toml': 'y\n',
                 'a/Cargo.toml': '[package]\n', 'a/src/lib.rs': 'fn a() {}\nfn b() {}\n', 'a/src/deep/m.rs': '//\n'}

        def write(eol: str) -> str:
            for n, t in files.items():
                with open(os.path.join(rust, *n.split('/')), 'w', encoding='utf-8', newline='') as f:
                    f.write(t.replace('\n', eol))
            return native.fingerprint(rust)
        lf, crlf = write('\n'), write('\r\n')
        check(lf == crlf and len(lf) == 64, 'fingerprint: the same with LF and CRLF line endings')
        check(len(native.sources(rust)) == 6, f'fingerprint: the workspace and crate files ({len(native.sources(rust))})')
        files['a/src/lib.rs'] += '// changed\n'
        check(write('\n') != lf, 'fingerprint: a changed source changes it')
        files['a/src/lib.rs'] = files['a/src/lib.rs'].replace('// changed\n', '')
        os.makedirs(os.path.join(rust, 'b', 'src'))
        files.update({'b/Cargo.toml': '[package]\n', 'b/src/lib.rs': ''})
        with_b = write('\n')
        check(with_b != lf, 'fingerprint: a new crate changes it')
        # an application crate (native.APPLICATIONS: the map editor) is not the module's: it changes nothing
        app = native.APPLICATIONS[0]
        os.makedirs(os.path.join(rust, app, 'src'))
        files.update({f'{app}/Cargo.toml': '[package]\n', f'{app}/src/main.rs': 'fn main() {}\n'})
        check(write('\n') == with_b, 'fingerprint: an application crate leaves it as it is')
        files[f'{app}/src/main.rs'] += '// changed\n'
        check(write('\n') == with_b, 'fingerprint: a change to an application crate leaves it as it is')


def download() -> None:
    """build_native.py --download against a stand-in for GitHub (its fetch replaced): the module installed with its
    stamp when the SHA-256 and the module's fingerprint check out; nothing installed on a wrong SHA-256, a module of
    other sources, or a missing release."""
    import io
    import urllib.error
    sys.path.insert(0, os.path.join(SUITE, 'rust'))
    import build_native as bn
    data = open(PYD, 'rb').read()
    good_sha = hashlib.sha256(data).hexdigest()
    saved = (bn.TARGET, native.STAMP, bn._get, native.BIN)
    exe = native.cli_exe()                 # this checkout's bfbc2.exe, served as the release's (round RT1)
    exe_data = open(exe, 'rb').read() if exe else None
    with tempfile.TemporaryDirectory() as tmp:
        try:
            bn.TARGET = os.path.join(tmp, os.path.basename(PYD))
            native.STAMP = os.path.join(tmp, 'bc2_native.json')
            native.BIN = os.path.join(tmp, 'bin')
            asked: list[str] = []

            def serve(sha: str, body: bytes, exe_sha: str | None = None, exe_body: bytes | None = None):
                def get(url: str) -> bytes:
                    asked.append(url)
                    if '/bfbc2.exe' in url:
                        if exe_body is None:
                            raise urllib.error.HTTPError(url, 404, 'Not Found', {}, io.BytesIO())  # type: ignore[arg-type]
                        return f'{exe_sha}  bfbc2.exe\n'.encode() if url.endswith('.sha256') else exe_body
                    if url.endswith('.sha256'):
                        return f'{sha}  bc2_native.pyd\n'.encode()
                    return body
                return get
            bn._get = serve('0' * 64, data)
            check(bn.download() == 1 and not os.path.exists(bn.TARGET), 'download: a wrong SHA-256 installs nothing')
            other = data.replace((native.checkout_fingerprint() or '').encode(), b'f' * 64)   # the baked-in fingerprint
            bn._get = serve(hashlib.sha256(other).hexdigest(), other)
            check(bn.download() == 1 and not os.path.exists(bn.TARGET), 'download: a module of other sources installs nothing')

            def missing(url: str) -> bytes:
                raise urllib.error.HTTPError(url, 404, 'Not Found', {}, io.BytesIO())  # type: ignore[arg-type]
            bn._get = missing
            check(bn.download() == 1 and not os.path.exists(bn.TARGET), 'download: no release, nothing installed')
            asked.clear()
            bn._get = serve(good_sha, data)
            ok = bn.download() == 0 and open(bn.TARGET, 'rb').read() == data
            stamp = native.read_stamp()
            check(ok and stamp.get('origin') == 'download' and stamp.get('source_hash') == native.checkout_fingerprint(),
                  f'download: the module installed with its stamp ({stamp.get("origin")})')
            fp = native.checkout_fingerprint() or ''
            mod_urls = [u for u in asked if 'bfbc2.exe' not in u]
            check(all(f'/releases/download/native-{fp[:12]}/bc2_native.pyd' in u for u in mod_urls) and len(mod_urls) == 2,
                  f'download: the release of the checkout\'s fingerprint ({asked[:1]})')
            check(not os.path.exists(native.bin_path('bfbc2')),
                  'download: a release without bfbc2.exe (made before round RT1) installs the module only')
            if exe_data is None:
                print('skipped the bfbc2.exe download checks: bin\\bfbc2.exe is not built from this checkout')
                return
            bn._get = serve(good_sha, data, '0' * 64, exe_data)
            check(bn.download() == 1 and not os.path.exists(native.bin_path('bfbc2')),
                  'download: a bfbc2.exe with a wrong SHA-256 is not installed')
            junk = b'MZ not a program'
            bn._get = serve(good_sha, data, hashlib.sha256(junk).hexdigest(), junk)
            check(bn.download() == 1 and not os.path.exists(native.bin_path('bfbc2')),
                  'download: a bfbc2.exe that does not run is not installed')
            bn._get = serve(good_sha, data, hashlib.sha256(exe_data).hexdigest(), exe_data)
            ok = bn.download() == 0 and open(native.bin_path('bfbc2'), 'rb').read() == exe_data
            py_txt = os.path.join(native.BIN, 'python.txt')
            check(ok and bn.binary_hash(native.bin_path('bfbc2')) == fp and os.path.isfile(py_txt),
                  'download: bfbc2.exe installed in bin\\ with its source hash, bin\\python.txt written')
        finally:
            bn.TARGET, native.STAMP, bn._get, native.BIN = saved


def retired_cli() -> None:
    """Round RT1: a retired command line (native.RETIRED_CLI) has only bfbc2.exe - without it cli_command raises
    CliMissing saying how to get it (BFBC2_CLI_PYTHON=1 does not bring back a Python); a tool that still has its
    Python falls back to it."""
    saved_env = os.environ.get(native.CLI_PYTHON_ENV)
    try:
        native._cli['exe'] = None                       # as if bin\bfbc2.exe were missing
        for tool in sorted(native.RETIRED_CLI):
            check(not os.path.exists(native.CLI_TOOLS[tool]), f'retired: {tool}\'s Python command line is gone')
            for env in (None, '1'):
                if env is None:
                    os.environ.pop(native.CLI_PYTHON_ENV, None)
                else:
                    os.environ[native.CLI_PYTHON_ENV] = env
                try:
                    native.cli_command(tool)
                    v = 'no error'
                except native.CliMissing as e:
                    v = str(e)
                check('--download' in v and 'retired' in v and 'bfbc2.exe' in v,
                      f'retired: {tool} without bfbc2.exe ({native.CLI_PYTHON_ENV}={env}): CliMissing saying how to '
                      f'get it ({v[:80]})')
        os.environ.pop(native.CLI_PYTHON_ENV, None)
        check(native.cli_command('hpdview') == [sys.executable, native.CLI_TOOLS['hpdview']],
              'retired: a tool that keeps its Python falls back to it without bfbc2.exe')
    finally:
        if saved_env is None:
            os.environ.pop(native.CLI_PYTHON_ENV, None)
        else:
            os.environ[native.CLI_PYTHON_ENV] = saved_env
        native.reset()


def main() -> int:
    fingerprints()
    retired_cli()
    if not os.path.isfile(PYD):
        print('skipped the module checks: the Rust core is not built (python rust\\build_native.py)')
        return 1 if FAILS else 0
    download()
    with tempfile.TemporaryDirectory() as tmp:
        # no module
        root = os.path.join(tmp, 'none')
        copy_suite(root, with_module=False)
        r = probe(root)
        for k in RETIRED:
            v = r.get(k, '')
            check(v.startswith('NativeMissing') and 'not built or downloaded' in v and '--download' in v
                  and 'build_native.py' in v, f'no module: {k} raises NativeMissing saying how to get it ({v[:150]})')
        # the matching module (copied sources: new file times, the same contents), BFBC2_PURE_PYTHON set
        root = os.path.join(tmp, 'ok')
        copy_suite(root, with_module=True)
        r = probe(root, BFBC2_PURE_PYTHON='1')
        check(r.get('texture') == 'ok 156', f'matching module: texture converts under BFBC2_PURE_PYTHON=1 '
                                            f'({r.get("texture")})')
        check(r.get('navbuild', '').startswith(('FileNotFoundError', 'OSError')),
              f'matching module: navbuild.build gets past the check to its input ({r.get("navbuild", "")[:80]})')
        check(r.get('audio') == 'ok (1, 1, 76)', f'matching module: audio encodes under BFBC2_PURE_PYTHON=1 ({r.get("audio")})')
        check(r.get('dbx write') == 'ok 29' and r.get('collision') == 'ok {}' and r.get('fbrb', '').startswith(
            ('FileNotFoundError', 'OSError')), f'matching module: the DBX writer, the collision index and the bundle reader '
            f'run under BFBC2_PURE_PYTHON=1 ({r.get("dbx write")}, {r.get("collision")}, {r.get("fbrb", "")[:40]})')
        for k in ('mesh', 'havok', 'granny', 'dbx', 'terrain', 'water'):
            check(bool(r.get(k)) and not r.get(k, '').startswith('NativeMissing'),
                  f'matching module: {k} gets past the check to its input ({r.get(k)})')
        r = probe(root)
        check(r.get('warnings') == '0 ' and r.get('dbx write') == 'ok 29', f'matching module: used, no warning ({r})')
        # stale: the copy's sources changed after the module was built
        lib = os.path.join(root, 'rust', 'bc2-py', 'src', 'lib.rs')
        with open(lib, 'a', encoding='utf-8') as f:
            f.write('// a change the module was not built with\n')
        r = probe(root)
        check(r.get('warnings', '').startswith('1 ') and 'other Rust sources' in r.get('warnings', ''),
              f'stale module: one warning ({r.get("warnings")})')
        for k in RETIRED:
            v = r.get(k, '')
            check(v.startswith('NativeMissing') and 'other Rust sources' in v and '--download' in v,
                  f'stale module: {k} raises NativeMissing naming the mismatch ({v[:150]})')
    print('all passed' if not FAILS else f'{len(FAILS)} failures')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
