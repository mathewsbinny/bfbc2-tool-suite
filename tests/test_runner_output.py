r"""The test runner survives a failing test whose output the console cannot print (quick tier).

On 2026-11-16 two runs of run_tests.py crashed while printing a failure: maturin's snake emoji, in the output of
the failing 'Rust core build' step, has no cp1252 character, so print() raised UnicodeEncodeError and the failure's
own output was lost. Here a stand-in test writes non-cp1252 text as UTF-8 bytes (as maturin does) and fails, and
run_tests.main runs it with its own output encoded as cp1252: the runner must report the failure (exit code 1),
print the text as escapes, and not crash.

  python tests\test_runner_output.py
"""
import os
import subprocess
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
TEXT = 'built \U0001f40d ✓ 日'      # a snake, a check mark, a CJK character: none of them in cp1252

# the failing stand-in test: writes UTF-8 bytes itself, whatever the console's code page, then fails
WRITER = f'import sys; sys.stdout.buffer.write({(TEXT + chr(10)).encode("utf-8")!r}); sys.exit(3)'

# runs run_tests.main with only the stand-in test
CHILD = f'''
import sys
sys.path.insert(0, {SUITE!r})
import run_tests
run_tests.tests = lambda: [('stand-in failing test', 'quick', [sys.executable, '-c', {WRITER!r}])]
sys.exit(run_tests.main(['run_tests.py', 'quick']))
'''


def main(argv: list[str]) -> int:
    env = dict(os.environ, PYTHONIOENCODING='cp1252', PYTHONUTF8='0')
    r = subprocess.run([sys.executable, '-c', CHILD], capture_output=True, env=env, cwd=SUITE)
    out = r.stdout.decode('cp1252', errors='replace')
    err = r.stderr.decode('cp1252', errors='replace')
    checks = [
        (r.returncode == 1, f'the runner reports the failure with exit code 1 (got {r.returncode})'),
        ('UnicodeEncodeError' not in out + err, 'no UnicodeEncodeError while printing the failure'),
        ('--- stand-in failing test (last 25 lines) ---' in out, "the failure's output block is printed"),
        ('\\U0001f40d' in out and '\\u2713' in out, 'characters the console lacks are printed as escapes'),
        ('0 of 1 passed' in out, 'the summary line is printed'),
    ]
    failed = 0
    for ok, what in checks:
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}')
    if failed:
        print(out[-2000:], err[-2000:])
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
