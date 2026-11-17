r"""The test runner's side-by-side mode (run_tests.py -j N) keeps every test's meaning (quick tier, 2026-11-17).

Stand-in tests that log when they start and end are run through run_tests.main, -j 4 and -j 1:
  - tests that write a shared thing never overlap a test that reads or writes it, and keep their listed order
  - a test missing from NEEDS runs alone; a test declaring every CPU thread runs beside nothing
  - a failing test gives exit code 1, its own output in the failures block (in list order), the right total
  - one result line per test, each with that test's own summary line; -j 1 prints them in the listed order
And without running anything: the scheduler holds room for a heavy test held back (shorter tests fill in only while
they end in time), every NEEDS entry names a test in tests(), the log reader takes a run's durations, and
atomicio.locked keeps two processes out of the same block.

  python tests\test_runner_parallel.py
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
sys.path.insert(0, SUITE)
import run_tests  # noqa: E402

failed = 0


def check(ok, what, detail=''):
    global failed
    failed += not ok
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'\n     {detail}' if not ok and detail else ''))


# a stand-in test: logs its start and end (wall clock) to the log file, sleeps, prints its summary, exits with code
STAND_IN = ('import sys, time; log, name, secs, code = sys.argv[1], sys.argv[2], float(sys.argv[3]), int(sys.argv[4]); '
            'open(log, "a").write(f"{name} start {time.time()}\\n"); time.sleep(secs); '
            'print(f"detail of {name}"); print(f"summary of {name}"); '
            'open(log, "a").write(f"{name} end {time.time()}\\n"); sys.exit(code)')

# (name, seconds, exit code, needs or None)
STANDS = [
    ('reader one', 0.8, 0, {'res': {'thing': 'r'}}),
    ('writer', 0.8, 0, {'res': {'thing': 'w'}}),
    ('reader two', 0.8, 0, {'res': {'thing': 'r'}}),
    ('plain one', 0.8, 0, {}),
    ('failing', 0.4, 3, {}),
    ('undeclared', 0.6, 0, None),
    ('all threads', 0.6, 0, {'cpu': 'all'}),
    ('plain two', 0.8, 0, {}),
    ('plain three', 0.3, 0, {}),
]

CHILD = '''
import json, sys
sys.path.insert(0, {suite!r})
import run_tests
spec = json.loads({spec!r})
run_tests.TIMES_FILE = spec['times']
run_tests.SEED_FILE = spec['times'] + '.none'
run_tests.tests = lambda: [(n, 'quick', [sys.executable, '-c', spec['code'], spec['log'], n, str(s), str(c)])
                           for n, s, c, _ in spec['stands']]
run_tests.NEEDS = {{n: run_tests.need(**{{k: v for k, v in d.items() if k != 'res'}}, **d.get('res', {{}}))
                   for n, _s, _c, d in spec['stands'] if d is not None}}
sys.exit(run_tests.main(['run_tests.py', 'quick', *spec['args']]))
'''


def run(tmp, args):
    log = os.path.join(tmp, f'log{len(os.listdir(tmp))}.txt')
    spec = json.dumps({'stands': STANDS, 'code': STAND_IN, 'log': log, 'times': os.path.join(tmp, 'times.json'),
                       'args': args})
    r = subprocess.run([sys.executable, '-c', CHILD.format(suite=SUITE, spec=spec)], capture_output=True, text=True,
                       encoding='utf-8', errors='replace', cwd=SUITE)
    spans: dict = {}
    if os.path.isfile(log):
        for line in open(log, encoding='utf-8'):
            name, what, t = line.rstrip('\n').rsplit(' ', 2)
            spans.setdefault(name, {})[what] = float(t)
    return r, {n: (v['start'], v['end']) for n, v in spans.items() if 'start' in v and 'end' in v}


def overlap(a, b):
    return a[0] < b[1] and b[0] < a[1]


def side_by_side(tmp):
    r, spans = run(tmp, ['-j', '4', '--cpus', '4'])
    out = r.stdout
    names = [s[0] for s in STANDS]
    check(set(spans) == set(names), 'every stand-in test ran once', f'{sorted(spans)}\n{out}\n{r.stderr}')
    if set(spans) != set(names):
        return
    check(not overlap(spans['writer'], spans['reader one']) and not overlap(spans['writer'], spans['reader two']),
          'a writer of a shared thing never overlaps its readers', str(spans))
    check(spans['reader one'][1] <= spans['writer'][0] <= spans['writer'][1] <= spans['reader two'][0],
          'tests sharing a thing keep their listed order (reader, writer, reader)', str(spans))
    for alone in ('undeclared', 'all threads'):
        others = [n for n in names if n != alone and overlap(spans[n], spans[alone])]
        check(not others, f'"{alone}" runs beside nothing', f'overlaps {others}')
    check(any(overlap(spans[a], spans[b]) for a in names for b in names if a < b),
          'tests do run side by side (some overlap)', str(spans))
    check(r.returncode == 1, f'a failing test: exit code 1 (got {r.returncode})')
    check(f'{len(names) - 1} of {len(names)} passed' in out, 'the total counts the failure', out[-300:])
    check('4 at a time' in out, 'the header says how many at a time')
    check('1 test(s) not in NEEDS run alone: undeclared' in out, 'a test missing from NEEDS is named', out[:400])
    lines = [ln for ln in out.splitlines() if ln.startswith(('ok   ', 'FAIL '))]
    per_test = {n: [ln for ln in lines if ln[5:].startswith(n + ' ')] for n in names}
    check(all(len(v) == 1 for v in per_test.values()) and len(lines) == len(names), 'one result line per test',
          '\n'.join(lines))
    check(all(f'summary of {n}' in per_test[n][0] for n in names if per_test[n]),
          'each result line carries its own test\'s summary', '\n'.join(lines))
    check(per_test['failing'] and per_test['failing'][0].startswith('FAIL ')
          and all(per_test[n][0].startswith('ok   ') for n in names if n != 'failing' and per_test[n]),
          'only the failing test is FAIL')
    block = out.split('--- failing (last 25 lines) ---')
    check(len(block) == 2 and 'detail of failing' in block[1]
          and not any(f'detail of {n}' in block[1] for n in names if n != 'failing'),
          "the failures block holds the failing test's own output only", out[-600:])
    kept = json.load(open(os.path.join(tmp, 'times.json'), encoding='utf-8'))
    check(set(kept) == set(names) - {'failing'} and all(v['s'] > 0 for v in kept.values()),
          'passed tests\' durations are kept for the next run (not the failed one)', str(kept))


def in_turn(tmp):
    r, spans = run(tmp, ['-j', '1'])
    names = [s[0] for s in STANDS]
    lines = [ln for ln in r.stdout.splitlines() if ln.startswith(('ok   ', 'FAIL '))]
    check([ln[5:].split('  ')[0].strip() for ln in lines] == names, '-j 1: one line per test in the listed order',
          '\n'.join(lines))
    order = sorted(spans, key=lambda n: spans[n][0])
    check(order == names and all(spans[a][1] <= spans[b][0] for a, b in zip(names, names[1:])),
          '-j 1: one after another, in the listed order', str(spans))
    check(r.returncode == 1 and f'{len(names) - 1} of {len(names)} passed' in r.stdout and 'at a time' not in r.stdout,
          '-j 1: the same exit code and total, and the header as before', r.stdout[-300:])


def scheduler():
    J = run_tests.Job
    need = run_tests.need
    medium = J(0, 'medium', 'game', [], need(cpu=2), 10.0)
    heavy = J(1, 'heavy', 'game', [], need(cpu='all'), 8.0)
    smalls = [J(2 + i, f'small{i}', 'game', [], need(), 3.0) for i in range(12)]
    plan = run_tests.simulate(run_tests.Scheduler([medium, heavy, *smalls], 8, 4, 100))
    when = {j.name: (s, e) for s, e, j in plan}
    check(when['heavy'][0] == 10.0, 'a heavy test held back starts as soon as the room it needs is free',
          str(when['heavy']))
    check(not [n for n in when if n != 'heavy' and overlap(when[n], when['heavy'])],
          'shorter tests fill in only while they end before it', str(when))
    check(any(when[s.name][0] < 10 for s in smalls), 'shorter tests do fill the wait', str(when))
    big = J(0, 'big', 'game', [], need(mem=40), 5.0)
    small = J(1, 'small', 'game', [], need(mem=1), 1.0)
    plan = run_tests.simulate(run_tests.Scheduler([big, small], 4, 8, 10))
    when = {j.name: (s, e) for s, e, j in plan}
    check(not overlap(when['big'], when['small']) and set(when) == {'big', 'small'},
          'a test needing more memory than the budget runs, alone', str(when))
    names = {n for n, _t, _a in run_tests.tests()}
    stale = sorted(set(run_tests.NEEDS) - names)
    check(not stale, 'every NEEDS entry names a test in tests() (a renamed test would otherwise run alone)', str(stale))
    undeclared = sorted(names - set(run_tests.NEEDS))
    if undeclared:
        print(f'note: {len(undeclared)} test(s) not in NEEDS (they run alone): ' + '; '.join(undeclared))


def log_reader(tmp):
    path = os.path.join(tmp, 'run.log')
    with open(path, 'w', encoding='utf-8') as f:
        f.write('BFBC2 Tool Suite tests: game, 3 test(s)\n'
                f'ok   {"water":42s}    5.3 s   37 levels checked; all passed\n'
                f'ok   {"water, every level":42s}   12.5 s   all passed\n'
                f'FAIL {"a name longer than the forty-two character column":42s} 661.7 s   failed\n'
                '\n2 of 3 passed in 680 s\n')
    got = run_tests.durations_from_log(path, ['water', 'water, every level',
                                              'a name longer than the forty-two character column'])
    check(got == {'water': 5.3, 'water, every level': 12.5,
                  'a name longer than the forty-two character column': 661.7},
          'durations read from a run log (names that start others, names past the column)', str(got))


def lock(tmp):
    counter = os.path.join(tmp, 'counter.txt')
    open(counter, 'w').write('0')
    code = ('import sys, time; sys.path.insert(0, sys.argv[1]); import suite_paths; suite_paths.use("core"); '
            'from bfbc2core import atomicio\n'
            'for _ in range(15):\n'
            '    with atomicio.locked(sys.argv[2] + ".lock", poll=0.01):\n'
            '        n = int(open(sys.argv[2]).read()); time.sleep(0.01); open(sys.argv[2], "w").write(str(n + 1))\n')
    procs = [subprocess.Popen([sys.executable, '-c', code, SUITE, counter]) for _ in range(3)]
    codes = [p.wait(120) for p in procs]
    n = int(open(counter).read())
    check(codes == [0, 0, 0] and n == 45, 'atomicio.locked: three processes, no update lost (45 of 45)',
          f'{codes} {n}')


def main():
    tmp = tempfile.mkdtemp(prefix='runner_parallel_')
    try:
        side_by_side(tmp)
        in_turn(tmp)
        scheduler()
        log_reader(tmp)
        lock(tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
