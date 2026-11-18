r"""The logs' caps and day rollover and the project caches (bfbc2core.caches, errors.DayLog; round E3q), on temp
folders only (quick tier): LOCALAPPDATA and OPFOR_ROOT point at a temp folder for every step and child process, so
the real logs and Project OPFOR BFBC2's work\ folder are never touched.

- day_log: a day's file, then its next part once it reaches the per-file cap;
- errors.setup's log: a session going past the per-file cap goes on in .1.log, .2.log ... with every line kept
  in order, the error report naming the current part; a second session the same day appends to the last part;
- prune_logs: each tool's 20 newest days; the oldest files beyond the per-tool cap, then beyond the overall cap;
  never the current session's files, nor (for the caps) a file written in the last LOG_RECENT seconds, nor one held
  open (Windows), nor anything not named like a log; the caps from the environment;
- project caches: work\navstudio\cache, work\harness\cache (its navdata\ below), work\navwater\cache found from any
  path in them; their folders under the project folder as the tools find it (OPFOR_ROOT); pruned to their caps
  (least recently used first, only .npz / .navdata.pkl); navdata.load prunes the project cache it wrote to and
  marks hits used, a folder outside the project is never pruned;
- the command: --list shows the project caches and the logs with their caps, --prune logs, --prune / --clear of a
  project cache, --clear without names leaves project caches and logs alone.

  python core\tests\test_logs_caches.py
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
CORE = os.path.dirname(HERE)
SUITE = os.path.dirname(CORE)
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import caches  # noqa: E402

DAY = 86400.0
KB = 1024
ENV_NAMES = ([n for s in caches.CACHES.values() for n in (s.env, s.cap_env)] +
             [s.cap_env for s in caches.PROJECT_CACHES.values()] +
             [caches.LOGS_ENV, caches.LOG_TOOL_ENV, caches.LOG_FILE_ENV, 'OPFOR_ROOT', 'LOCALAPPDATA'])


def write(path, n, age=0.0):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(b'x' * n)
    t = time.time() - age
    os.utime(path, (t, t))
    return path


def names(folder):
    out: set[str] = set()
    for dp, _, fs in os.walk(folder):
        out.update(os.path.relpath(os.path.join(dp, f), folder).replace('\\', '/') for f in fs)
    return out


# a tool's session: errors.setup, then numbered lines; prints the files its log went to and the error report's path
SESSION = r'''
import os, sys
sys.path.insert(0, sys.argv[1])
from bfbc2core import errors
log = errors.setup(sys.argv[2], gui=False)
for i in range(int(sys.argv[3])):
    log.info('line %05d %s', i, 'y' * 80)
h = [h for h in log.handlers if isinstance(h, errors.DayLog)][0]
print('|'.join(h.files))
print(errors._state['path'])
'''


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    tmp = tempfile.mkdtemp(prefix='logs_caches_test_')
    saved = {k: os.environ.get(k) for k in ENV_NAMES}
    for k in ENV_NAMES:
        os.environ.pop(k, None)
    os.environ['LOCALAPPDATA'] = tmp
    opfor = os.path.join(tmp, 'opfor')
    os.makedirs(os.path.join(opfor, 'game'))
    os.environ['OPFOR_ROOT'] = opfor
    logs = os.path.join(tmp, 'bfbc2-tool-suite', 'logs')

    def child_env(**extra):
        e = {k: v for k, v in os.environ.items() if k not in ENV_NAMES}
        e.update(LOCALAPPDATA=tmp, OPFOR_ROOT=opfor)
        e.update(extra)
        return e

    try:
        # ---- caps from the environment
        check(caches.log_caps() == (200 << 20, 100 << 20, 20 << 20), 'log caps: the defaults (200 / 100 / 20 MiB)')
        os.environ[caches.LOGS_ENV], os.environ[caches.LOG_TOOL_ENV] = '0.5', 'lots'
        check(caches.log_caps() == (1 << 19, 100 << 20, 20 << 20), 'log caps: MiB with fractions; a word is the default')
        del os.environ[caches.LOGS_ENV], os.environ[caches.LOG_TOOL_ENV]

        # ---- day_log: the day's last part while under the cap, else the next
        d = os.path.join(tmp, 'daylog')
        os.makedirs(d)
        p0 = caches.day_log(d, 'nav', '2026-11-17', limit=100)
        check(os.path.basename(p0) == 'nav-2026-11-17.log', 'day_log: the day\'s file first')
        write(p0, 150)
        p1 = caches.day_log(d, 'nav', '2026-11-17', limit=100)
        write(p1, 50)
        write(os.path.join(d, 'nav-x-2026-11-17.5.log'), 500)               # another tool's (nav-x): not counted
        check(os.path.basename(p1) == 'nav-2026-11-17.1.log' and caches.day_log(d, 'nav', '2026-11-17', limit=100) == p1,
              'day_log: a full file goes on in .1.log, used while under the cap; another tool\'s parts not counted')
        write(p1, 100)
        check(os.path.basename(caches.day_log(d, 'nav', '2026-11-17', limit=100)) == 'nav-2026-11-17.2.log',
              'day_log: then .2.log')

        # ---- errors.setup's log rolls over within the day, every line kept
        file_cap = 4000                                                     # bytes: about 40 lines per part
        env = child_env(**{caches.LOG_FILE_ENV: str(file_cap / (1 << 20))})
        out = subprocess.run([sys.executable, '-c', SESSION, CORE, 'roll-tool', '200'], capture_output=True, text=True,
                             env=env, timeout=120)
        lines = out.stdout.strip().splitlines()
        files = lines[0].split('|') if lines else []
        text = ''
        for f in files:
            with open(f, encoding='utf-8') as fh:
                text += fh.read()
        got = [ln.split('line ')[1][:5] for ln in text.splitlines() if ' line ' in ln]
        check(out.returncode == 0 and len(files) >= 4 and got == [f'{i:05d}' for i in range(200)],
              'a session past the per-file cap goes on in .1.log, .2.log ...: every line kept, in order',
              f'{out.returncode} {len(files)} {len(got)} {out.stderr[-400:]}')
        check(all(os.path.getsize(f) < file_cap + 400 for f in files) and len(lines) > 1 and lines[1] == files[-1],
              'each part stops at the cap (one line over at most); the error report names the current part',
              str([os.path.getsize(f) for f in files]))
        nxt = os.path.abspath(caches.day_log(logs, 'roll-tool', limit=file_cap))
        out2 = subprocess.run([sys.executable, '-c', SESSION, CORE, 'roll-tool', '3'], capture_output=True, text=True,
                              env=env, timeout=120)
        f2 = out2.stdout.strip().splitlines()[0].split('|') if out2.stdout.strip() else []
        check(out2.returncode == 0 and f2 == [nxt] and nxt in files + [caches.log_part(
              logs, 'roll-tool', time.strftime('%Y-%m-%d'), len(files))] and
              'line 00002' in open(nxt, encoding='utf-8').read(),
              'a second session the same day appends to the last part (the next once it is full)', f'{f2} {files[-1:]}')

        # ---- prune_logs
        d = os.path.join(tmp, 'prune')
        for i in range(25):                                                  # old-tool: 25 days, 2 parts on one
            write(os.path.join(d, f'old-tool-2026-01-{i + 1:02d}.log'), 10, age=(60 - i) * DAY)
        write(os.path.join(d, 'old-tool-2026-01-25.1.log'), 10, age=35 * DAY)
        write(os.path.join(d, 'notes.txt'), 50 * KB, age=90 * DAY)           # not logs: never counted or deleted
        write(os.path.join(d, 'tool.log'), 50 * KB, age=90 * DAY)
        write(os.path.join(d, 'sub', 'x-2026-01-01.log'), 50 * KB, age=90 * DAY)
        r = caches.prune_logs(d, caps=(10 << 20, 10 << 20, 1 << 20))
        left = names(d)
        old = sorted(n for n in left if n.startswith('old-tool'))
        check(len(old) == 21 and 'old-tool-2026-01-05.log' not in old and 'old-tool-2026-01-06.log' in old and
              'old-tool-2026-01-25.1.log' in old and r.deleted_files == 5,
              "prune_logs: each tool's 20 newest days kept (every part of a kept day)", f'{old} {r}')
        check({'notes.txt', 'tool.log', 'sub/x-2026-01-01.log', '.prune.lock'} <= left,
              'prune_logs: files not named like a log, folders below and the lock left alone', str(sorted(left)))

        d = os.path.join(tmp, 'caps')                                        # two tools, 10 KB files, one a day
        a = [write(os.path.join(d, f'a-2026-02-{k + 1:02d}.log'), 10 * KB, age=(30 - 2 * k) * DAY) for k in range(6)]
        b = [write(os.path.join(d, f'b-2026-02-{k + 1:02d}.log'), 10 * KB, age=(29 - 2 * k) * DAY) for k in range(6)]
        r = caches.prune_logs(d, caps=(10 ** 9, 35 * KB, 1 << 20))
        left = names(d)
        check(left >= {os.path.basename(p) for p in a[3:] + b[3:]} and not any(os.path.basename(p) in left
                                                                                  for p in a[:3] + b[:3]),
              'prune_logs: per tool, the oldest files beyond its cap deleted, the newest kept', str(sorted(left)))
        r = caches.prune_logs(d, caps=(25 * KB, 10 ** 9, 1 << 20))
        left = {n for n in names(d) if n.endswith('.log')}
        check(left == {'a-2026-02-06.log', 'b-2026-02-06.log'} and r.kept_bytes == 20 * KB,
              'prune_logs: then all tools together, the oldest first, down to the overall cap', f'{sorted(left)} {r}')

        d = os.path.join(tmp, 'protect')
        session = [write(os.path.join(d, 's-2026-03-01.log'), 10 * KB, age=40 * DAY),
                   write(os.path.join(d, 's-2026-03-01.1.log'), 10 * KB, age=39 * DAY)]
        running = write(os.path.join(d, 'r-2026-03-01.log'), 10 * KB, age=60)          # another tool, written a minute ago
        held = write(os.path.join(d, 'h-2026-03-01.log'), 10 * KB, age=30 * DAY)
        idle = write(os.path.join(d, 'i-2026-03-01.log'), 10 * KB, age=30 * DAY)
        with open(held, 'rb'):                                               # open in another process (Windows)
            r = caches.prune_logs(d, keep=session, caps=(0, 0, 0))
        left = names(d)
        check(all(os.path.basename(p) in left for p in session) and os.path.basename(running) in left and
              os.path.basename(idle) not in left,
              'prune_logs: the current session\'s files and a recently written file kept, even at cap 0', str(left))
        if sys.platform == 'win32':
            check(os.path.basename(held) in left and r.busy == 1, 'prune_logs: a log held open is kept (busy)', str(r))
        r = caches.clear_logs(d)
        check(names(d) == {'r-2026-03-01.log', '.prune.lock'}, 'clear_logs: every log but a running tool\'s',
              str(names(d)))

        # ---- a session that rolls over prunes the folder and keeps all of its own parts
        old_logs = [write(os.path.join(logs, f'other-2026-04-{k + 1:02d}.log'), 20 * KB, age=(20 - k) * DAY)
                    for k in range(5)]
        env = child_env(**{caches.LOG_FILE_ENV: str(4000 / (1 << 20)), caches.LOGS_ENV: str(30 * KB / (1 << 20))})
        out = subprocess.run([sys.executable, '-c', SESSION, CORE, 'big-tool', '300'], capture_output=True, text=True,
                             env=env, timeout=120)
        files = out.stdout.strip().splitlines()[0].split('|') if out.stdout.strip() else []
        others = [p for p in old_logs if os.path.exists(p)]
        text = ''.join(open(f, encoding='utf-8').read() for f in files if os.path.exists(f))
        check(out.returncode == 0 and len(files) >= 6 and all(os.path.exists(f) for f in files) and
              text.count(' line ') == 300 and others == [],
              "a rolling session prunes the logs to the overall cap: the other tools' old logs go, its own parts all "
              'stay',
              f'{out.returncode} {len(files)} {text.count(" line ")} {[os.path.basename(p) for p in others]} '
              f'{out.stderr[-300:]}')

        # ---- project caches: where they are
        proj = {n: os.path.join(opfor, *s.project) for n, s in caches.PROJECT_CACHES.items()}
        check(caches.folder('navstudio') == proj['navstudio'] and caches.folder('harness') == proj['harness'],
              'project caches: under the project folder as the tools find it (OPFOR_ROOT)')
        check(caches.folder('navwater', opfor=os.path.join(tmp, 'nowhere')) == proj['navwater'],
              'project caches: a given folder without game\\ falls back as find_opfor does')
        check(caches.project_cache_of(os.path.join(proj['harness'], 'navdata')) == ('harness', proj['harness']) and
              caches.project_cache_of(proj['navstudio']) == ('navstudio', proj['navstudio']) and
              caches.project_cache_of(os.path.join(tmp, 'elsewhere', 'cache')) is None and
              caches.project_cache_of(os.path.join(opfor, 'work', 'navstudio')) is None,
              'project_cache_of: a path in work\\<tool>\\cache (or below it) is that cache; others none')

        # ---- project caches: pruned to their caps, only their own entries
        hc = proj['harness']
        entries = [write(os.path.join(hc, 'navdata', f'{k:02d}' + ('.npz' if k % 2 else '.navdata.pkl')), 10 * KB,
                         age=(20 - k) * 3600) for k in range(10)]
        keep_files = [write(os.path.join(hc, 'bundles', 'ab12', 'x.dbx'), 50 * KB, age=30 * DAY),
                      write(os.path.join(hc, 'bundles', 'x.names.json'), 50 * KB, age=30 * DAY),
                      write(os.path.join(hc, 'navdata', '03.npz.4242.tmp'), 10 * KB)]
        os.environ['BFBC2_HARNESS_CACHE_MB'] = str(40 * KB / (1 << 20))
        caches._last.clear()
        rp = caches.prune_project_cache(os.path.join(hc, 'navdata'))
        check(rp is not None and [os.path.exists(p) for p in entries] == [False] * 6 + [True] * 4 and
              all(os.path.exists(p) for p in keep_files),
              'prune_project_cache: the least recently used .npz / .navdata.pkl beyond the cap; other files and a '
              "writer's temporary file kept", f'{rp} {[os.path.exists(p) for p in entries]}')
        check(caches.prune_project_cache(os.path.join(tmp, 'elsewhere')) is None,
              'prune_project_cache: a folder outside the project caches is never pruned')
        del os.environ['BFBC2_HARNESS_CACHE_MB']

        # ---- navdata.load prunes the project cache it wrote to and marks hits used
        try:
            suite_paths.use('navkit-tools')
            from bfbc2core import navdata
        except Exception as ex:                                              # noqa: BLE001
            print(f'skip navdata: {ex}')
        else:
            sc = proj['navstudio']
            old = [write(os.path.join(sc, f'old{k}.navdata.pkl'), 10 * KB, age=DAY) for k in range(3)]
            src = write(os.path.join(tmp, 'lvl', 'pathdata.hpd'), 10)
            real = navdata._load
            setattr(navdata, '_load', lambda path, hf, cd: {'path': path})  # noqa: B010 - no game data: a stand-in
            try:
                os.environ['BFBC2_NAVSTUDIO_CACHE_MB'] = str(5 * KB / (1 << 20))
                caches._last.clear()
                nav: object = navdata.load(src, None, sc)
                pk = [os.path.join(sc, n) for n in os.listdir(sc) if n.endswith('.navdata.pkl')]
                check(nav == {'path': src} and not any(os.path.exists(p) for p in old) and len(pk) == 1,
                      'navdata.load: the entry written, the project cache pruned to its cap (the old entries first)',
                      str(names(sc)))
                t = time.time() - DAY
                os.utime(pk[0], (t, t))
                nav = navdata.load(src, None, sc)
                check(nav == {'path': src} and time.time() - os.stat(pk[0]).st_mtime < 60,
                      'navdata.load: a hit marks its entry used')
                other = os.path.join(tmp, 'navcache')
                stale = write(os.path.join(other, 'stale.navdata.pkl'), 10 * KB, age=DAY)
                caches._last.clear()
                navdata.load(src, None, other)
                check(os.path.exists(stale), 'navdata.load: a cache folder outside the project is not pruned')
            finally:
                setattr(navdata, '_load', real)  # noqa: B010
                os.environ.pop('BFBC2_NAVSTUDIO_CACHE_MB', None)

        # ---- the command
        def run(*args, **extra):
            return subprocess.run([sys.executable, '-m', 'bfbc2core.caches', *args], cwd=CORE, capture_output=True,
                                  text=True, env=child_env(**extra), timeout=60)
        for f in os.listdir(logs):
            os.remove(os.path.join(logs, f))
        for i in range(3):
            write(os.path.join(logs, f'navkit-2026-05-{i + 1:02d}.log'), 100 * KB, age=(10 - i) * DAY)
        write(os.path.join(proj['navwater'], 'a.npz'), 3 * KB, age=DAY)
        write(os.path.join(proj['navwater'], 'b.navdata.pkl'), 3 * KB)
        out = run('--list')
        rows = {ln.split()[0]: ln for ln in out.stdout.splitlines() if ln.strip()}
        check(out.returncode == 0 and '300.0 KB' in rows.get('logs', '') and 'BFBC2_LOGS_MB' in rows.get('logs', '')
              and 'BFBC2_LOG_FILE_MB' in rows.get('logs', '') and '6.0 KB' in rows.get('navwater', '') and
              proj['navwater'] in rows.get('navwater', '') and 'BFBC2_NAVSTUDIO_CACHE_MB' in rows.get('navstudio', '')
              and 'project cache' in rows.get('harness', ''),
              '--list: the project caches (in the project folder) and the logs, with their caps', out.stdout + out.stderr)
        out = run('--list', 'navwater', '--opfor', opfor)
        check(out.returncode == 0 and out.stdout.startswith('navwater') and proj['navwater'] in out.stdout and
              caches.LOGS_ENV not in out.stdout, '--list of one project cache (--opfor)', out.stdout + out.stderr)
        out = run('--clear', 'navwater', '--opfor', os.path.join(tmp, 'nowhere'))
        check(out.returncode != 0 and 'no game' in out.stdout + out.stderr and len(names(proj['navwater'])) == 2,
              '--opfor without a game\\ folder is refused (no fallback to another project)', out.stdout + out.stderr)
        out = run('--prune', 'logs', **{caches.LOGS_ENV: str(150 * KB / (1 << 20))})
        check(out.returncode == 0 and sorted(os.listdir(logs)) == ['.prune.lock', 'navkit-2026-05-03.log'] and
              'deleted 2 file' in out.stdout, '--prune logs: the oldest logs beyond the overall cap',
              out.stdout + out.stderr + str(os.listdir(logs)))
        out = run('--prune', 'navwater', BFBC2_NAVWATER_CACHE_MB=str(4 * KB / (1 << 20)))
        check(out.returncode == 0 and names(proj['navwater']) >= {'b.navdata.pkl'} and
              'a.npz' not in names(proj['navwater']), '--prune navwater: the least recently used entry beyond its cap',
              out.stdout + out.stderr)
        out = run('--clear')
        check(out.returncode == 0 and 'b.navdata.pkl' in names(proj['navwater']) and
              os.path.exists(os.path.join(logs, 'navkit-2026-05-03.log')) and 'cleared only by name' in out.stdout,
              '--clear without names: project caches and logs left alone', out.stdout + out.stderr)
        out = run('--clear', 'navwater', '--opfor', opfor, OPFOR_ROOT='')
        check(out.returncode == 0 and 'b.navdata.pkl' not in names(proj['navwater']),
              '--clear navwater --opfor: that project cache emptied', out.stdout + out.stderr)
        out = run('--prune')
        check(out.returncode == 0 and all(n in out.stdout for n in ('hlsl', 'navstudio', 'harness', 'logs')),
              '--prune without names: every cache, project cache and the logs', out.stdout + out.stderr)
    finally:
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
