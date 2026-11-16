r"""The Rust core's own checks (rust\): formatting, clippy with warnings as errors, and every crate's tests.

  python tests\test_rust.py            quick: cargo fmt --check, cargo clippy --all-targets -D warnings, cargo test
  python tests\test_rust.py --corpus   game: the corpus test (every unpacked .dbx of the game copy against
                                       DbxEdit's fingerprints, in Rust, release build)

Skipped (exit 0) when Rust is not installed: the suite then runs on the Python readers.
"""
import os
import shutil
import subprocess
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('rust')
import build_native  # noqa: E402

RUST = os.path.join(SUITE, 'rust')


def main(argv: list[str]) -> int:
    env = build_native.cargo_env()
    if env is None:
        print('skipped: Rust (cargo) is not installed - the suite runs on the Python readers')
        return 0
    if '--corpus' in argv:
        steps = [('corpus test (release)', ['cargo', 'test', '--release', '-p', 'bc2-formats', '--test', 'corpus', '--',
                                            '--ignored', '--nocapture'])]
    else:
        steps = [('cargo fmt --check', ['cargo', 'fmt', '--all', '--check']),
                 ('cargo clippy --all-targets -D warnings', ['cargo', 'clippy', '--all-targets', '--', '-D', 'warnings']),
                 ('cargo test (unit, integration, doc)', ['cargo', 'test', '--workspace'])]
    # the full path: Windows looks a program up on this process's PATH, not the child's
    cargo = shutil.which('cargo', path=env.get('PATH')) or 'cargo'
    failed = 0
    for name, args in steps:
        r = subprocess.run([cargo, *args[1:]], cwd=RUST, env=env, capture_output=True, text=True, encoding='utf-8', errors='replace')
        out = (r.stdout + r.stderr).strip()
        ok = r.returncode == 0
        failed += not ok
        summary = [l for l in out.splitlines() if l.startswith('test result:') or (l[:1].isdigit() and 'files (' in l)]
        passed = sum(int(l.split(' passed')[0].rsplit(' ', 1)[-1]) for l in summary if ' passed' in l)
        detail = f'{passed} tests passed' if summary and ok else ''
        print(f'{"ok  " if ok else "FAIL"} {name}' + (f': {detail}' if detail else ''))
        for l in summary:
            if 'files (' in l:
                print('       ' + l)
        if not ok:
            tail = '\n'.join(out.splitlines()[-40:])
            print(tail.encode('ascii', errors='backslashreplace').decode('ascii'))     # prints on any console
    print('all passed' if not failed else f'{failed} step(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
