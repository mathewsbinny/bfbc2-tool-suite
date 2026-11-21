r"""The test runner's side-by-side mode (run_tests.py -j N) keeps every test's meaning (quick tier, 2026-11-17).

Stand-in tests that log when they start and end are run through run_tests.main, -j 4 and -j 1:
  - tests that write a shared thing never overlap a test that reads or writes it, and keep their listed order
  - a test missing from NEEDS runs alone; a test declaring every CPU thread runs beside nothing
  - a failing test gives exit code 1, its own output in the failures block (in list order), the right total
  - one result line per test, each with that test's own summary line; -j 1 prints them in the listed order
And without running anything: the scheduler holds room for a heavy test held back (shorter tests fill in only while
they end in time), every NEEDS entry names a test in tests(), the log reader takes a run's durations, and
atomicio.locked keeps two processes out of the same block. Round QW1: a test whose summary line says "skipped" counts
as skipped (not passed) and parts a passing test skipped are counted; the kept durations lose names no tier has once
they are old; a test's last recorded peak raises its declared memory; matplotlib's folder is the suite's. Round TS1
(audit AU4 DF4): a test past its time limit is ended with its processes and fails; the runner killed mid-test leaves
no test process running (the job objects are KILL_ON_JOB_CLOSE), nor does a passing test's leftover child; --only
matching nothing is exit code 2; build_native.install waits out a moment's lock, moves a loaded file aside, and puts
the live file back when the new one cannot go in.

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


# a stand-in test: logs its start and end (wall clock) to its own log file (<log>.<name>: appends from several
# processes to one file are not atomic on Windows - two stand-ins ending together could lose a line under load, a flaky
# 'every stand-in test ran once'), sleeps, prints its summary, exits with code
STAND_IN = ('import sys, time; log, name, secs, code = sys.argv[1], sys.argv[2], float(sys.argv[3]), int(sys.argv[4]); '
            'mine = f"{log}.{name}"; '
            'open(mine, "a").write(f"{name} start {time.time()}\\n"); time.sleep(secs); '
            'print(f"detail of {name}"); print(f"summary of {name}"); '
            'open(mine, "a").write(f"{name} end {time.time()}\\n"); sys.exit(code)')

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
    for stand, *_ in STANDS:
        mine = f'{log}.{stand}'
        if os.path.isfile(mine):
            for line in open(mine, encoding='utf-8'):
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


def qw1(tmp):
    """Round QW1: skipped tests, pruned durations, peaks in the schedule, matplotlib's folder."""
    check(run_tests.skipped(True, 'skipped: no game copy') and run_tests.skipped(True, 'Skipped: Rust is not installed')
          and not run_tests.skipped(True, 'all passed') and not run_tests.skipped(False, 'skipped: then failed'),
          'a passing test whose summary line says skipped is skipped; a failure is a failure')
    check(run_tests.result_line(True, 'x', 1.0, 'skipped: no data').startswith('skip '), 'its result line says skip')
    code =('import sys; n = sys.argv[1]; print("skipped: part one has no data") if n == "partly" else None; '
            'print("skipped: no game copy" if n == "nothing" else "all passed")')
    spec = {'times': os.path.join(tmp, 'times_skip.json')}
    child = ('import sys\nsys.path.insert(0, %r)\nimport run_tests\n'
             'run_tests.TIMES_FILE = %r\nrun_tests.SEED_FILE = %r\n'
             'run_tests.tests = lambda: [(n, "quick", [sys.executable, "-c", %r, n]) for n in ("plain", "partly", "nothing")]\n'
             'run_tests.NEEDS = {n: run_tests.need() for n in ("plain", "partly", "nothing")}\n'
             'sys.exit(run_tests.main(["run_tests.py", "quick", "-j", "1"]))\n'
             % (SUITE, spec['times'], spec['times'] + '.none', code))
    r = subprocess.run([sys.executable, '-c', child], capture_output=True, text=True, timeout=120)
    out = r.stdout
    check(r.returncode == 0 and '2 of 3 passed' in out and '1 skipped (ran nothing' in out
          and '1 of them with parts skipped' in out and 'skip nothing' in out.replace('  ', ' '),
          'the total: 2 of 3 passed, 1 skipped, 1 with parts skipped; the skipped test\'s line says skip', out[-700:])
    kept = json.load(open(spec['times'], encoding='utf-8'))
    check(set(kept) == {'plain', 'partly'}, 'a skipped test\'s duration is not kept', str(kept))
    names = {'a', 'b'}
    got = run_tests.prune_times({'a': {'when': '2026-01-01'}, 'gone old': {'when': '2026-01-01'},
                                 'gone new': {'when': '2026-01-20'}, 'gone undated': {}}, names, today='2026-01-25')
    check(set(got) == {'a', 'gone new'}, 'durations of names no tier has go once older than STALE_DAYS', str(got))
    saved = run_tests.NEEDS
    try:
        run_tests.NEEDS = {'t': run_tests.need(mem=1), 'u': run_tests.need(mem=2)}
        jobs = run_tests.make_jobs([('t', 'quick', []), ('u', 'quick', [])], {'t': (10.0, 3000), 'u': (5.0, 100)})
        check([j.mem for j in jobs] == [3.0, 2], 'a recorded peak above the declaration raises it, never lowers it',
              str([j.mem for j in jobs]))
    finally:
        run_tests.NEEDS = saved
    env = dict(os.environ)
    env.pop('MPLCONFIGDIR', None)
    code = ('import os, sys; sys.path.insert(0, sys.argv[1]); import run_tests; '
            'run_tests.subprocess.run = lambda *a, **k: None; run_tests.warm_matplotlib(); print(os.environ["MPLCONFIGDIR"])')
    r = subprocess.run([sys.executable, '-c', code, SUITE], capture_output=True, text=True, env=env, timeout=60)
    check(r.stdout.strip() == run_tests.MPL_DIR, 'MPLCONFIGDIR is the suite\'s own folder for the tests', r.stdout + r.stderr)


def alive(pid):
    """The process pid is still running (Windows: its exit code is STILL_ACTIVE)."""
    import ctypes
    k32 = ctypes.WinDLL('kernel32', use_last_error=True)
    k32.OpenProcess.restype = ctypes.c_void_p
    k32.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    k32.CloseHandle.argtypes = [ctypes.c_void_p]
    h = k32.OpenProcess(0x1000, False, pid)         # PROCESS_QUERY_LIMITED_INFORMATION
    if not h:
        return False
    code = ctypes.c_ulong()
    ok = k32.GetExitCodeProcess(h, ctypes.byref(code))
    k32.CloseHandle(h)
    return bool(ok) and code.value == 259


def wait_for(path, secs=30.0):
    import time
    end = time.time() + secs
    while time.time() < end:
        if os.path.isfile(path) and open(path).read().strip():
            return open(path).read().strip()
        time.sleep(0.1)
    return ''


def gone(pid, secs=10.0):
    import time
    end = time.time() + secs
    while pid and alive(pid) and time.time() < end:
        time.sleep(0.2)
    return bool(pid) and not alive(pid)


# a stand-in test that starts a grandchild (sleeping two minutes), writes its pid, then sleeps `secs`
SPAWNER = ('import subprocess, sys, time; pidfile, secs = sys.argv[1], float(sys.argv[2]); '
           'p = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(120)"], '
           'stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL); '
           'open(pidfile, "w").write(str(p.pid)); time.sleep(secs); print("all passed")')

DF4_CHILD = ('import sys\nsys.path.insert(0, %r)\nimport run_tests\n'
             'run_tests.TIMES_FILE = %r\nrun_tests.SEED_FILE = %r\n'
             'run_tests.tests = lambda: [("spawner", "quick", [sys.executable, "-c", %r, %r, %r]), '
             '("plain", "quick", [sys.executable, "-c", "print(1)"])]\n'
             'run_tests.NEEDS = {n: run_tests.need() for n in ("spawner", "plain")}\n'
             'sys.exit(run_tests.main(["run_tests.py", "quick", *%r]))\n')


def df4(tmp):
    """Round TS1 (audit AU4 DF4): a test's time limit, --only matching nothing, child processes ended with the
    runner however it ends (KILL_ON_JOB_CLOSE), build_native's install retried and never leaving the live file aside."""
    import time
    if sys.platform != 'win32':
        print('skipped: the job object checks are for Windows')
        return
    group = run_tests._ProcessGroup()
    check(group.kills_on_close(), "a test's job object ends its processes when its handle closes (KILL_ON_JOB_CLOSE)")
    group.close()
    times = os.path.join(tmp, 'times_df4.json')

    def runner(pidfile, secs, args):
        code = DF4_CHILD % (SUITE, times, times + '.none', SPAWNER, pidfile, str(secs), args)
        return subprocess.Popen([sys.executable, '-c', code], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, encoding='utf-8', errors='replace')

    # a hung test: ended at its limit with its child, failed and said so; the rest of the tier runs
    pidfile = os.path.join(tmp, 'pid_timeout.txt')
    t0 = time.time()
    p = runner(pidfile, 120, ['-j', '2', '--timeout', '3'])
    out, err = p.communicate(timeout=100)
    pid = int(wait_for(pidfile, 1) or 0)
    check(p.returncode == 1 and 'timed out after 3 s' in out and '1 of 2 passed' in out and time.time() - t0 < 60,
          'a test past --timeout is ended and fails ("timed out after 3 s"), the others run', out[-600:] + err[-300:])
    check(gone(pid), "the timed-out test's own child process is ended with it", str(pid))
    # the runner killed outright (no Ctrl+C handler runs): the test's processes end with it
    pidfile = os.path.join(tmp, 'pid_killed.txt')
    p = runner(pidfile, 120, ['-j', '1'])
    pid = int(wait_for(pidfile) or 0)
    p.kill()
    p.communicate(timeout=60)
    check(gone(pid), "the runner killed mid-test: the test's processes end with it (none left running)", str(pid))
    # a passing test whose child outlives it does not leave it running
    pidfile = os.path.join(tmp, 'pid_left.txt')
    p = runner(pidfile, 0, ['-j', '1', '--only', 'spawner'])
    out, _ = p.communicate(timeout=100)
    pid = int(wait_for(pidfile, 1) or 0)
    check(p.returncode == 0 and gone(pid), "a passing test's leftover child is ended with its job", out[-300:])
    r = subprocess.run([sys.executable, os.path.join(SUITE, 'run_tests.py'), 'quick', '--only', 'no such test x'],
                       capture_output=True, text=True, timeout=120)
    check(r.returncode == 2 and 'matches no test' in r.stderr, '--only matching nothing: exit code 2, said',
          f'{r.returncode} {r.stdout[-200:]} {r.stderr[-300:]}')
    check(run_tests.time_limit('quick', 10.0) == 1800.0 and run_tests.time_limit('game', 4000.0) == 16000.0
          and run_tests.time_limit('quick', 5.0, 0) is None and run_tests.time_limit('full', None, 7.0) == 7.0,
          'the time limit: 4 x the kept duration, at least the tier floor; --timeout overrides, 0 is none')
    install_checks(tmp)


def install_checks(tmp):
    """build_native.install: a moment's lock is waited out; a loaded file is moved aside and replaced; when the new
    file cannot go in, the live file is put back, never left moved aside."""
    import threading
    sys.path.insert(0, os.path.join(SUITE, 'rust'))
    import build_native
    from bfbc2core import atomicio
    folder = os.path.join(tmp, 'install')
    os.makedirs(folder)
    target = os.path.join(folder, 'held.dll')
    dll = os.path.join(os.path.dirname(sys.executable), 'python3.dll')
    shutil.copyfile(dll, target)

    def new(data):
        path = target + '.new'
        with open(path, 'wb') as f:
            f.write(data)
        return path

    # a moment's lock (a scanner reading the file): open without delete sharing for 0.3 s, then released
    held = open(target, 'rb')
    threading.Timer(0.3, held.close).start()
    build_native.install(new(b'one'), target)
    check(open(target, 'rb').read() == b'one' and not [f for f in os.listdir(folder) if '.old-' in f],
          "install: a moment's lock is waited out (nothing moved aside)", str(os.listdir(folder)))
    # loaded by a running process (a module or an exe): renamed aside, the new file in its place
    shutil.copyfile(dll, target)
    loader = subprocess.Popen([sys.executable, '-c', 'import ctypes, sys, time; ctypes.WinDLL(sys.argv[1]); '
                               'print("loaded", flush=True); time.sleep(60)', target], stdout=subprocess.PIPE,
                              text=True)
    try:
        if loader.stdout is not None:
            loader.stdout.readline()                # the library is loaded
        build_native.install(new(b'two'), target)
        check(open(target, 'rb').read() == b'two' and len([f for f in os.listdir(folder) if '.old-' in f]) == 1,
              'install: a loaded file is moved aside and the new one put in its place', str(os.listdir(folder)))
        real = atomicio.replace
        calls = []

        def failing(a, b):
            calls.append((a, b))
            if len(calls) == 1:
                raise PermissionError(5, 'held')        # the target is held
            if len(calls) == 3:
                raise OSError(28, 'no space')           # the new file cannot go into place
            return real(a, b)

        atomicio.replace = failing
        try:
            build_native.install(new(b'three'), target)
            put_back = False
        except OSError:
            put_back = True
        finally:
            atomicio.replace = real
        check(put_back and open(target, 'rb').read() == b'two',
              'install: when the new file cannot go in, the live file is put back (never left moved aside)',
              str(os.listdir(folder)))
    finally:
        loader.kill()
        loader.wait()


def main():
    tmp = tempfile.mkdtemp(prefix='runner_parallel_')
    try:
        side_by_side(tmp)
        in_turn(tmp)
        scheduler()
        log_reader(tmp)
        lock(tmp)
        qw1(tmp)
        df4(tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
