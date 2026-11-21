r"""tests\test_rust.py's own rules (round TS1), on a stand-in workspace in a temp folder (no cargo run):

- a crate's fingerprint follows its own files, the workspace crates it depends on (its dev-dependencies too, not a
  dependency's), the workspace's manifests and the build's environment, and not a crate it does not depend on;
- green fingerprints are merged under a lock: three processes recording at once lose none;
- crates are never left out with --no-skip, BFBC2_RUST_NO_SKIP=1 or on CI;
- the skip lines counted ("skipped: ...", "<level>: skipped, ...") and not ("..., 0 skipped").

  python tests\test_rust_skip.py
"""
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import test_rust  # noqa: E402

failed = 0


def check(ok, what, detail=''):
    global failed
    failed += not ok
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'\n     {detail}' if not ok and detail else ''))


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(text)


def workspace(root):
    rust = os.path.join(root, 'rust')
    write(os.path.join(root, 'VERSION'), '0.1.0\n')
    write(os.path.join(rust, 'Cargo.toml'),
          '[workspace]\nmembers = ["a", "b", "c", "d"]\n\n[workspace.dependencies]\n'
          'a = { path = "a" }\nb = { path = "b" }\nc = { path = "c" }\nd = { path = "d" }\n')
    write(os.path.join(rust, 'Cargo.lock'), '# lock\n')
    write(os.path.join(rust, 'a', 'Cargo.toml'), '[package]\nname = "a"\n')
    write(os.path.join(rust, 'a', 'src', 'lib.rs'), '//! a\n')
    # b depends on a; its tests use c (a dev-dependency); d uses b, so it reaches a but not c
    write(os.path.join(rust, 'b', 'Cargo.toml'),
          '[package]\nname = "b"\n\n[dependencies]\na.workspace = true\n\n[dev-dependencies]\nc.workspace = true\n')
    write(os.path.join(rust, 'b', 'src', 'lib.rs'), '//! b\npub const T: &str = include_str!("../../../text.txt");\n')
    write(os.path.join(root, 'text.txt'), 'included\n')
    write(os.path.join(rust, 'c', 'Cargo.toml'), '[package]\nname = "c"\n')
    write(os.path.join(rust, 'c', 'src', 'lib.rs'), '//! c\n')
    write(os.path.join(rust, 'd', 'Cargo.toml'), '[package]\nname = "d"\n\n[dependencies]\nb = { path = "../b" }\n')
    write(os.path.join(rust, 'd', 'src', 'lib.rs'), '//! d\n')
    return rust


def fingerprints(rust, env=None):
    return {c: test_rust.crate_fingerprint(c, 'tests', rust, env or {}) for c in 'abcd'}


def main():
    tmp = tempfile.mkdtemp(prefix='rust_skip_')
    try:
        rust = workspace(tmp)
        check(test_rust.closure('b', rust) == ['a', 'b', 'c'] and test_rust.closure('d', rust) == ['a', 'b', 'd'],
              "a crate's closure: its dependencies and its own dev-dependencies, not a dependency's",
              f"{test_rust.closure('b', rust)} {test_rust.closure('d', rust)}")
        before = fingerprints(rust)
        write(os.path.join(rust, 'a', 'src', 'lib.rs'), '//! a, changed\n')
        after = fingerprints(rust)
        check([c for c in 'abcd' if before[c] != after[c]] == ['a', 'b', 'd'],
              'a change in a crate changes its fingerprint and its dependents\', no other', str(after))
        before = after
        write(os.path.join(rust, 'c', 'src', 'lib.rs'), '//! c, changed\n')
        after = fingerprints(rust)
        check([c for c in 'abcd' if before[c] != after[c]] == ['b', 'c'],
              "a dev-dependency's change reaches the crate whose tests use it, not that crate's dependents")
        before = after
        write(os.path.join(rust, 'b', 'tests', 'data', 'x.bin'), 'test data\n')
        after = fingerprints(rust)
        check([c for c in 'abcd' if before[c] != after[c]] == ['b', 'd'], "a crate's test data is in its fingerprint")
        before = after
        write(os.path.join(tmp, 'text.txt'), 'included, changed\n')
        after = fingerprints(rust)
        check(before['b'] != after['b'] and before['c'] == after['c'],
              'a file included from outside the crate (include_str!) is in its fingerprint')
        before = after
        write(os.path.join(rust, 'Cargo.lock'), '# lock, changed\n')
        after = fingerprints(rust)
        check(all(before[c] != after[c] for c in 'abcd'), "the workspace's lock is in every fingerprint")
        check(fingerprints(rust, {'RUSTFLAGS': '-Copt-level=3'}) != after
              and fingerprints(rust, {'RUSTC_WRAPPER': 'sccache'}) == after,
              "the build's flags are in the fingerprint, the compile cache is not")
        crlf = fingerprints(rust)
        with open(os.path.join(rust, 'd', 'src', 'lib.rs'), 'w', encoding='utf-8', newline='\r\n') as f:
            f.write('//! d\n')
        check(fingerprints(rust) == crlf, 'line endings do not change a fingerprint')

        green = os.path.join(tmp, 'green.json')
        code = ('import sys; sys.path.insert(0, sys.argv[1]); import test_rust\n'
                'for k in range(15): test_rust.mark_green("tests-quick", {f"c{sys.argv[2]}": f"fp{k}"}, sys.argv[3])\n')
        procs = [subprocess.Popen([sys.executable, '-c', code, HERE, str(n), green]) for n in range(3)]
        codes = [p.wait(120) for p in procs]
        got = [sum(1 for k in range(15) if test_rust.green_since('tests-quick', f'c{n}', f'fp{k}', green))
               for n in range(3)]
        check(codes == [0, 0, 0] and got == [15, 15, 15],
              'green fingerprints from three processes at once: none lost (45 of 45)', f'{codes} {got}')
        check(test_rust.green_since('clippy', 'c0', 'fp1', green) is None, 'green is per step (clippy apart)')

        saved = dict(os.environ)
        try:
            for k in ('CI', test_rust.NO_SKIP_ENV):
                os.environ.pop(k, None)
            plain = test_rust.may_skip([])
            flag = test_rust.may_skip(['--no-skip'])
            os.environ[test_rust.NO_SKIP_ENV] = '1'
            env_off = test_rust.may_skip([])
            os.environ.pop(test_rust.NO_SKIP_ENV)
            os.environ['CI'] = 'true'
            ci = test_rust.may_skip([])
        finally:
            os.environ.clear()
            os.environ.update(saved)
        check(plain and not flag and not env_off and not ci,
              'green crates left out only off CI, without --no-skip and BFBC2_RUST_NO_SKIP')

        counted = ['skipped: no game copy', 'mp_005: skipped, not in this game copy', 'parity skipped: no tool',
                   'the Python reference is skipped: no python on PATH']
        not_counted = ["leveldiff: 21 recorded cases identical to LevelDiff.exe's, 0 skipped", 'all passed']
        check(all(test_rust.SKIP.search(s) for s in counted) and not any(test_rust.SKIP.search(s) for s in not_counted),
              'the skip lines counted, and a count of none not')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
