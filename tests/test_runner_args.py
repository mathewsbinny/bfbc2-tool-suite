r"""The test runner's command line (quick tier, round RT3).

`run_tests.py --help` ran the quick tier: the runner ignored every argument it did not know. Now `-h`, `--help` and
`/?` print the runner's text and run nothing (exit code 0), and an unknown argument, a missing or wrong value, or two
tiers are a usage error (exit code 2) that runs nothing. The good command lines (--list, --plan, the tiers' options)
still work. run_tests.main runs with a stand-in test list whose only test writes a marker file, so a command line that
wrongly runs tests is seen (and costs nothing); `run_tests.py --help` also runs as the real script (with a time limit).

  python tests\test_runner_args.py
"""
import os
import subprocess
import sys
import tempfile

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))

# runs run_tests.main with one stand-in test that writes the marker file named by sys.argv[1]
CHILD = f'''
import sys
sys.path.insert(0, {SUITE!r})
import run_tests
marker = sys.argv[1]
writer = 'import sys; open(sys.argv[1], "w").close()'
run_tests.tests = lambda: [('stand-in', 'stand-in test', 'quick', [sys.executable, '-c', writer, marker])]
run_tests.warm_matplotlib = lambda: None
run_tests.keep_times = lambda results: None      # the stand-in's time is not kept with the real tests'
sys.exit(run_tests.main(['run_tests.py', *sys.argv[2:]]))
'''

CASES = [   # (arguments, exit code, the stand-in test runs, text the output must hold)
    (['--help'], 0, False, 'Runs the suite\'s tests'),
    (['-h'], 0, False, 'Runs the suite\'s tests'),
    (['/?'], 0, False, 'Runs the suite\'s tests'),
    (['quick', '--help'], 0, False, '--no-game'),
    (['--bogus'], 2, False, "unknown argument '--bogus'"),
    (['quick', '-x'], 2, False, "unknown argument '-x'"),
    (['quik'], 2, False, "unknown argument 'quik'"),
    (['quick', '-j'], 2, False, '-j needs a value'),
    (['quick', '-j', 'two'], 2, False, "-j takes a whole number, not 'two'"),
    (['quick', '--mem', 'lots'], 2, False, "--mem takes a number, not 'lots'"),
    (['quick', 'game'], 2, False, 'one tier at a time'),
    (['--list'], 0, False, 'stand-in test'),
    (['quick', '--plan', '-j', '2', '--mem', '4.5'], 0, False, 'Plan: quick'),
    (['quick', '-j', '1'], 0, True, '1 of 1 passed'),
    (['quick', '--only', 'stand-in', '--jobs', '1'], 0, True, '1 of 1 passed'),
]


def main(argv: list[str]) -> int:
    failed = 0
    with tempfile.TemporaryDirectory(prefix='runner_args_') as tmp:
        for i, (args, code, runs, text) in enumerate(CASES):
            marker = os.path.join(tmp, f'ran{i}')
            r = subprocess.run([sys.executable, '-c', CHILD, marker, *args], capture_output=True, text=True,
                               cwd=SUITE, timeout=120)
            ran = os.path.exists(marker)
            out = r.stdout + r.stderr
            ok = r.returncode == code and ran == runs and text in out
            failed += not ok
            print(f'{"ok  " if ok else "FAIL"} run_tests.py {" ".join(args)}: exit {r.returncode} (want {code}), '
                  f'the test {"ran" if ran else "did not run"} (want {"ran" if runs else "not"}), '
                  f'{"has" if text in out else "LACKS"} {text!r}')
            if not ok:
                print(out[-1500:])
        # the real script, as a user types it: the text, at once, and no test started
        try:
            r = subprocess.run([sys.executable, os.path.join(SUITE, 'run_tests.py'), '--help'], capture_output=True,
                               text=True, cwd=SUITE, timeout=60)
            ok = r.returncode == 0 and r.stdout.startswith('Runs the suite\'s tests') and 'BFBC2 Tool Suite tests:' \
                not in r.stdout
            why = f'exit {r.returncode}'
        except subprocess.TimeoutExpired:
            ok, why = False, 'still running after 60 s (it is running tests)'
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} python run_tests.py --help prints the text and runs nothing ({why})')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
