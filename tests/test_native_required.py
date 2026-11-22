r"""The Rust core is required for texture conversion and NavKit's grid walks (their Python retired on 2026-11-17), for
the sound, mesh, physics and Granny conversions (2026-11-18) and for the core format readers and writers - DBX, bundles,
terrain, water, the collision index, the level loader (2026-11-18, round F1a) -, and a module built from other sources
is never used silently (core/bfbc2core/native.py; README.md, "The Rust core").

A copy of the suite's Python and Rust sources in a temporary folder (no game data, no cargo), three ways:
  - no module: texture.convert, navwalk.build, navwalk.walkable, audio's encoder, mesh.convert,
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
import audio, granny, havok, mesh, navwalk, texture
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
attempt('navkit build', lambda: navwalk.build('no such samples.bin', None, 0, 0, 'out.hpd', log=lambda *a: None))
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
RETIRED = ('texture', 'navkit build', 'walkable', 'audio', 'mesh', 'havok', 'granny', 'dbx', 'dbx write', 'fbrb', 'terrain',
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
    artefacts()


def artefacts() -> None:
    """Round QW1: one fingerprint per artefact over the crates it links (native.crates: the artefact's crate and its
    workspace path dependencies, normal / build / target-specific, not dev-dependencies), with the files they
    include."""
    with tempfile.TemporaryDirectory() as tmp:
        rust = os.path.join(tmp, 'rust')
        files = {
            'Cargo.toml': '[workspace]\nmembers = ["bc2-py", "bc2-cli", "bc2-formats", "bc2-render", "bc2-editor", '
                          '"bc2-levelwrite"]\n[workspace.dependencies]\nbc2-formats = { path = "bc2-formats" }\n'
                          'bc2-render = { path = "bc2-render" }\nbc2-levelwrite = { path = "bc2-levelwrite" }\n',
            'Cargo.lock': '', 'rust-toolchain.toml': '',
            'bc2-formats/Cargo.toml': '[package]\nname = "bc2-formats"\n',
            'bc2-formats/src/lib.rs': 'pub fn f() {}\n',
            'bc2-render/Cargo.toml': '[package]\nname = "bc2-render"\n',
            'bc2-render/src/lib.rs': '',
            'bc2-levelwrite/Cargo.toml': '[package]\nname = "bc2-levelwrite"\n[dependencies]\nbc2-formats.workspace = true\n',
            'bc2-levelwrite/src/lib.rs': '', 'bc2-levelwrite/src/bin/mapexport.rs': 'fn main() {}\n',
            'bc2-py/Cargo.toml': '[package]\nname = "bc2-py"\n[dependencies]\nbc2-formats.workspace = true\n'
                                 '[dev-dependencies]\nbc2-render.workspace = true\n',
            'bc2-py/src/lib.rs': '',
            'bc2-cli/Cargo.toml': '[package]\nname = "bc2-cli"\n[dependencies]\nbc2-formats.workspace = true\n'
                                  'bc2-levelwrite = { workspace = true, optional = true }\n',
            'bc2-cli/src/main.rs': 'const DOC: &str = include_str!(\n    "../../../tool.py"\n);\n',
            'bc2-editor/Cargo.toml': '[package]\nname = "bc2-editor"\n[dependencies]\nbc2-levelwrite.workspace = true\n'
                                     "[target.'cfg(windows)'.dependencies]\nbc2-render = { path = \"../bc2-render\" }\n",
            'bc2-editor/src/main.rs': '',
            '../tool.py': '\"\"\"doc\"\"\"\n', '../VERSION': '0.9.0\n',
        }

        def write() -> dict[str, str]:
            for n, t in files.items():
                p = os.path.normpath(os.path.join(rust, *n.split('/')))
                os.makedirs(os.path.dirname(p), exist_ok=True)
                with open(p, 'w', encoding='utf-8', newline='') as f:
                    f.write(t)
            return {a: native.fingerprint(rust, native.sources(rust, a)) for a in native.ARTEFACTS}
        base = write()
        check(native.crates(rust, 'module') == ['bc2-formats', 'bc2-py'],
              f'artefacts: the module links bc2-py and bc2-formats, not its dev-dependency ({native.crates(rust)})')
        check(native.crates(rust, 'bfbc2') == ['bc2-cli', 'bc2-formats', 'bc2-levelwrite'],
              f'artefacts: bfbc2 with its optional dependency ({native.crates(rust, "bfbc2")})')
        check(native.crates(rust, 'bfbc2-editor') == ['bc2-editor', 'bc2-formats', 'bc2-levelwrite', 'bc2-render'],
              f'artefacts: the editor with a target-specific path dependency ({native.crates(rust, "bfbc2-editor")})')
        check(native.crates(rust, 'mapexport') == ['bc2-formats', 'bc2-levelwrite'],
              f'artefacts: mapexport, a binary of levelwrite ({native.crates(rust, "mapexport")})')
        check(len(set(base.values())) == 4, 'artefacts: four different fingerprints')
        check(os.path.join(tmp, 'tool.py') in native.sources(rust, 'bfbc2'),
              'artefacts: a file bfbc2 includes (include_str!) is one of its sources')

        def changed(name: str, text: str) -> set[str]:
            old = files[name]
            files[name] = text
            now = write()
            files[name] = old
            write()
            return {a for a in now if now[a] != base[a]}
        got = changed('bc2-render/src/lib.rs', '// edited\n')
        check(got == {'bfbc2-editor'}, f'artefacts: an edit to the renderer makes only the editor stale ({got})')
        got = changed('bc2-editor/src/main.rs', '// edited\n')
        check(got == {'bfbc2-editor'}, f'artefacts: an edit to the editor makes only the editor stale ({got})')
        got = changed('bc2-levelwrite/src/bin/mapexport.rs', '// edited\n')
        check(got == {'bfbc2', 'bfbc2-editor', 'mapexport'}, f'artefacts: levelwrite reaches its users ({got})')
        got = changed('bc2-formats/src/lib.rs', '// edited\n')
        check(got == set(native.ARTEFACTS), f'artefacts: the formats crate is in every one ({got})')
        got = changed('../tool.py', '\"\"\"new doc\"\"\"\n')
        check(got == {'bfbc2'}, f'artefacts: an included docstring makes bfbc2 stale ({got})')
        got = changed('../VERSION', '0.9.1\n')
        check(got == set(native.ARTEFACTS), f'artefacts: VERSION is in every one ({got})')


def stale_cli() -> None:
    """Round QW1 (audit AU2 A6 / A19): a stale bfbc2.exe - cli_command falls back to the Python with one
    RuntimeWarning per process naming the fix; BFBC2_STRICT_NATIVE=1 makes it an error; BFBC2_CLI_PYTHON=1 asks for
    the Python with no warning. binary() finds a binary only while it is the checkout's."""
    import warnings
    saved = (os.environ.get(native.CLI_PYTHON_ENV), os.environ.get(native.STRICT_ENV))
    try:
        for k in (native.CLI_PYTHON_ENV, native.STRICT_ENV):
            os.environ.pop(k, None)
        native.reset()
        native._binaries['bfbc2'] = ('stale', 'C:\\stand-in\\bfbc2.exe', 'C:\\stand-in\\bfbc2.exe was built from '
                                     'other Rust sources than this checkout has')
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter('always')
            a = native.cli_command('hpdview')
            b = native.cli_command('navkit')        # (culltree until round RT3 retired its Python command line)
        texts = [str(w.message) for w in caught if issubclass(w.category, RuntimeWarning)]
        check(a == [sys.executable, native.CLI_TOOLS['hpdview']] and b[0] == sys.executable,
              'stale bfbc2.exe: the tools fall back to their Python')
        check(len(texts) == 1 and 'other Rust sources' in texts[0] and 'build_native.py' in texts[0],
              f'stale bfbc2.exe: one warning per process naming the fix ({texts})')
        os.environ[native.STRICT_ENV] = '1'
        try:
            native.cli_command('hpdview')
            v = 'no error'
        except native.NativeStale as e:
            v = str(e)
        check('BFBC2_STRICT_NATIVE' in v and 'build_native.py' in v, f'stale bfbc2.exe, strict: an error ({v[:90]})')
        native._binaries['bfbc2'] = ('missing', None, 'bin\\bfbc2.exe is not built or downloaded')
        try:
            native.cli_command('hpdview')
            v = 'no error'
        except native.NativeStale as e:
            v = str(e)
        check('not built' in v, f'missing bfbc2.exe, strict: an error ({v[:90]})')
        os.environ[native.CLI_PYTHON_ENV] = '1'
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter('always')
            a = native.cli_command('hpdview')
        check(a == [sys.executable, native.CLI_TOOLS['hpdview']] and not caught,
              'BFBC2_CLI_PYTHON=1: the Python, no warning and no error even when strict')
        # the real binaries against this checkout's fingerprints (a changed fingerprint makes each one stale)
        native.reset()
        os.environ.pop(native.CLI_PYTHON_ENV, None)
        os.environ.pop(native.STRICT_ENV, None)
        for name in ('bfbc2', 'bfbc2-editor', 'mapexport'):
            status, path, _ = native.binary_status(name)
            if status != 'ok':
                print(f'skipped the {name} binary check: no copy here was built by build_native.py ({status})')
                continue
            check(status == 'ok' and native.binary(name) == path,
                  f'binary({name}): the built one is this checkout\'s ({native.binary_status(name)[2][:80]})')
            real = native.checkout_fingerprint
            native.reset()
            native.checkout_fingerprint = lambda rust=native.RUST, artefact='module': 'f' * 64
            try:
                status, _, why = native.binary_status(name)
                check(status == 'stale' and native.binary(name) is None and 'other Rust sources' in why,
                      f'binary({name}): stale when the checkout\'s sources differ ({status})')
            finally:
                native.checkout_fingerprint = real
                native.reset()
    finally:
        for k, old in zip((native.CLI_PYTHON_ENV, native.STRICT_ENV), saved, strict=True):
            if old is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = old
        native.reset()


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
            fp = native.checkout_fingerprint(artefact='bfbc2') or ''      # round QW1: bfbc2.exe's own fingerprint
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
        native._binaries['bfbc2'] = ('missing', None, 'bin\\bfbc2.exe is not built or downloaded')  # as if it were missing
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
        # round RT3: bfbc2.exe's list of retired Python command lines (pyproc.rs RETIRED, with the round its
        # retirement error names) is native's (leveldiff never had a Python command line: not in pyproc's)
        import re
        with open(os.path.join(SUITE, 'rust', 'bc2-cli', 'src', 'pyproc.rs'), encoding='utf-8') as f:
            src = f.read()
        block = src[src.index('pub const RETIRED'):]
        block = block[:block.index('];')]
        rust = dict(re.findall(r'\("(\w+)",\s*"([\w-]+)"\)', block))
        want = {t: r for t, r in native.RETIRED_ROUND.items() if t != 'leveldiff'}
        check(rust == want and set(native.RETIRED_ROUND) == native.RETIRED_CLI,
              f'retired: bfbc2.exe\'s list (pyproc.rs) and native.RETIRED_ROUND agree, tool and round ({rust})')
    finally:
        if saved_env is None:
            os.environ.pop(native.CLI_PYTHON_ENV, None)
        else:
            os.environ[native.CLI_PYTHON_ENV] = saved_env
        native.reset()


def main() -> int:
    fingerprints()
    retired_cli()
    stale_cli()
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
        check(r.get('navkit build', '').startswith(('FileNotFoundError', 'OSError')),
              f'matching module: navwalk.build gets past the check to its input ({r.get("navkit build", "")[:80]})')
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
