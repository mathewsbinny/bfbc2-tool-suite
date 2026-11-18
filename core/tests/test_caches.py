r"""bfbc2core.caches (round E3p): the size caps of the suite's on-disk caches, their least-recently-used pruning and
the command, on temp folders only (quick tier; the real caches under %LOCALAPPDATA% are never touched: every run
here points LOCALAPPDATA and the caches' variables at a temp folder).

- pruning order: the least recently used (the later of access and modification time) deleted first, down to the cap;
  a kept name never; only the cache's own entries (by suffix), never other files, dot files or what a link points to;
- caps: the variables' numbers (MiB, GiB for fbrb, fractions), anything else the default; a cache turned off;
- writers and readers in other processes: a writer's temporary file is never deleted while it writes, and its
  rename lands whole; an entry a reader holds open is kept (Windows) and read whole; an entry changed after the
  scan is kept; stale temporary files and old markers go;
- two processes pruning at once delete only the excess (the folder's lock);
- maybe_prune: at most once per interval, across processes (the stamp);
- the owners: pcbuild's disk cache and culltree's row cache mark hits used and prune after writes;
- the command: --list, --prune, --clear of one cache and of all, an unknown name.

  python core\tests\test_caches.py
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
ENV_NAMES = [n for s in caches.CACHES.values() for n in (s.env, s.cap_env)]


def write(path, n, age=0.0, data=None):
    """A file of n bytes whose access and modification times are age seconds ago."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data if data is not None else b'x' * n)
    t = time.time() - age
    os.utime(path, (t, t))
    return path


def names(folder):
    out: set[str] = set()
    for dp, _, fs in os.walk(folder):
        out.update(os.path.relpath(os.path.join(dp, f), folder).replace('\\', '/') for f in fs)
    return out


def env_for(tmp, **extra):
    e = {k: v for k, v in os.environ.items() if k not in ENV_NAMES}
    e['LOCALAPPDATA'] = tmp
    os.makedirs(os.path.join(tmp, 'opfor', 'game'), exist_ok=True)     # the project caches: here, never the real ones
    e['OPFOR_ROOT'] = os.path.join(tmp, 'opfor')
    e.update(extra)
    return e


# the second pruner process: waits for the go file, then prunes
PRUNER = r'''
import os, sys, time
sys.path.insert(0, sys.argv[1])
from bfbc2core import caches
folder, go = sys.argv[2], sys.argv[3]
while not os.path.exists(go):
    time.sleep(0.005)
r = caches.prune(folder, int(sys.argv[4]), ('.bin',))
print(r.deleted_files)
'''

# a writer as the suite's writers write: a temporary file beside the entry, written slowly, then renamed over it
WRITER = r'''
import os, sys, time
path, ready = sys.argv[1], sys.argv[2]
tmp = f'{path}.tmp-{os.getpid()}'
with open(tmp, 'wb') as f:
    f.write(b'a' * 1000)
    f.flush()
    open(ready, 'wb').close()
    for _ in range(10):
        time.sleep(0.05)
        f.write(b'b' * 1000)
os.replace(tmp, path)
'''


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    tmp = tempfile.mkdtemp(prefix='caches_test_')
    saved = {k: os.environ.get(k) for k in ENV_NAMES + ['LOCALAPPDATA']}
    for k in ENV_NAMES:
        os.environ.pop(k, None)
    os.environ['LOCALAPPDATA'] = tmp
    try:
        # ---- pruning order and the cap
        d = os.path.join(tmp, 'order')
        for i in range(10):                                  # e0 the oldest ... e9 the newest, 100 bytes each
            write(os.path.join(d, f'{i % 3:02d}', f'e{i}.bin'), 100, age=(10 - i) * 100)
        a = write(os.path.join(d, 'e_read.bin'), 100, age=5000)     # old, but read just now (access time)
        t = time.time()
        os.utime(a, (t, t - 5000))
        write(os.path.join(d, 'other.dat'), 5000, age=9000)          # not an entry: never counted or deleted
        write(os.path.join(d, '.prune.lock'), 0, age=9000)
        r = caches.prune(d, 500, ('.bin',), keep=[os.path.join(d, '00', 'e0.bin')])
        left = names(d)
        want = {'00/e0.bin', '00/e9.bin', '01/e7.bin', '02/e8.bin', 'e_read.bin'}
        check({n for n in left if n.endswith('.bin')} == want,
              'prune: the least recently used deleted first, down to the cap; a kept name and a recent access stay',
              str(sorted(left)))
        check('other.dat' in left and '.prune.lock' in left and '.pruned' in left,
              "prune: other files and the lock left alone; the stamp written", str(sorted(left)))
        check(r.deleted_files == 6 and r.deleted_bytes == 600 and r.kept_bytes == 500 and r.kept_files == 5,
              'prune: what it reports', str(r))
        r = caches.prune(d, 10 ** 9, ('.bin',))
        check(r.deleted_files == 0 and len([n for n in names(d) if n.endswith('.bin')]) == 5,
              'prune: nothing deleted under the cap')
        check(caches.prune(os.path.join(tmp, 'missing'), 0, ('.bin',)).kept_files == 0 and
              not os.path.exists(os.path.join(tmp, 'missing')), 'prune: a missing folder is empty, not made')

        # ---- links are not followed (Windows: a junction; elsewhere a symlink)
        outside = os.path.join(tmp, 'outside')
        write(os.path.join(outside, 'precious.bin'), 100, age=DAY)
        d = os.path.join(tmp, 'linked')
        os.makedirs(d)
        link = os.path.join(d, 'link')
        try:
            if sys.platform == 'win32':
                subprocess.run(['cmd', '/c', 'mklink', '/J', link, outside], check=True, capture_output=True)
            else:
                os.symlink(outside, link)
            caches.clear(d, ('.bin',))
            caches.prune(d, 0, ('.bin',))
            check(os.path.isfile(os.path.join(outside, 'precious.bin')), 'clear / prune: a link is not followed')
        finally:
            if os.path.lexists(link):
                os.rmdir(link) if sys.platform == 'win32' else os.remove(link)

        # ---- temporary files, markers, entries changed or open
        d = os.path.join(tmp, 'temps')
        fresh = [write(os.path.join(d, n), 50) for n in ('a.bin.tmp-123-456', 'b.pkl.tmp-77', 'c.9.1.tmp', 'd.77.tmp')]
        stale = write(os.path.join(d, 'e.bin.tmp-1'), 50, age=2 * caches.STALE_TMP)
        old_marker = write(os.path.join(d, 'f.seen'), 0, age=40 * DAY)
        new_marker = write(os.path.join(d, 'g.seen'), 0, age=DAY)
        r = caches.prune(d, 0, ('.bin',), markers=('.seen',))
        check(all(os.path.exists(p) for p in fresh) and not os.path.exists(stale),
              "prune: a writer's temporary file is never deleted; a dead writer's (older than STALE_TMP) is")
        check(not os.path.exists(old_marker) and os.path.exists(new_marker) and r.stale == 2,
              'prune: markers older than their age deleted, newer kept', str(r))
        e = write(os.path.join(d, 'h.bin'), 10, age=DAY)
        rec = caches._scan(d, ('.bin',)).entries[0]
        os.utime(e)                                          # touched (used) after the scan
        check(caches._delete_if_unchanged(rec) is False and os.path.exists(e),
              'prune: an entry touched after the scan is kept')
        with open(e, 'ab') as f:
            f.write(b'more')                                 # rewritten after the scan
        rec2 = caches._scan(d, ('.bin',)).entries[0]
        write(e, 3, age=DAY)
        check(caches._delete_if_unchanged(rec2) is False and os.path.exists(e),
              'prune: an entry rewritten after the scan is kept')
        payload = os.urandom(5000)
        held = write(os.path.join(d, 'i.bin'), 0, age=DAY, data=payload)
        with open(held, 'rb') as fr:                         # a Python reader in the middle of the entry
            first = fr.read(100)
            r = caches.prune(d, 0, ('.bin',))
            rest = fr.read()
        check(first + rest == payload, 'prune: a reader holding an entry reads it whole')
        if sys.platform == 'win32':
            check(os.path.exists(held) and r.busy >= 1, 'prune: an entry open without delete sharing is kept (busy)',
                  str(r))

        # ---- a writer in another process while the folder is pruned and cleared
        d = os.path.join(tmp, 'writer')
        os.makedirs(d)
        target, ready = os.path.join(d, 'w.bin'), os.path.join(tmp, 'writer_ready')
        write(target, 10, age=DAY)                           # the old entry it replaces
        p = subprocess.Popen([sys.executable, '-c', WRITER, target, ready])
        while not os.path.exists(ready) and p.poll() is None:
            time.sleep(0.01)
        caches.prune(d, 0, ('.bin',))
        caches.clear(d, ('.bin',))
        tmps = [n for n in os.listdir(d) if caches.is_temp(n)]
        p.wait(30)
        with open(target, 'rb') as fr:
            got = fr.read()
        check(len(tmps) == 1 and p.returncode == 0 and got == b'a' * 1000 + b'b' * 10000,
              "prune / clear beside a writer: its temporary file left, its rename lands whole",
              f'{tmps} {p.returncode} {len(got)}')

        # ---- two processes pruning at once delete only the excess
        d = os.path.join(tmp, 'two')
        for i in range(40):
            write(os.path.join(d, f'{i:02d}.bin'), 100, age=(40 - i) * 10)
        go = os.path.join(tmp, 'two_go')
        ps = [subprocess.Popen([sys.executable, '-c', PRUNER, CORE, d, go, '1500'], stdout=subprocess.PIPE, text=True)
              for _ in range(2)]
        time.sleep(0.3)
        open(go, 'wb').close()
        outs = [int(q.communicate(timeout=60)[0].strip() or -1) for q in ps]
        left = sorted(n for n in os.listdir(d) if n.endswith('.bin'))
        check(left == [f'{i:02d}.bin' for i in range(25, 40)] and sorted(outs) == [0, 25],
              'two processes pruning at once: the 15 newest left, 25 deleted once', f'{outs} {left}')

        # ---- caps and folders from the environment
        check(caches.cap('hlsl') == 1024 << 20 and caches.cap('fbrb') == 16 << 30 and
              caches.cap('culltree') == 2048 << 20, 'cap: the defaults')
        os.environ['BFBC2_HLSL_CACHE_MB'] = '1.5'
        os.environ['BFBC2_FBRB_CACHE_GB'] = '2'
        os.environ['BFBC2_COLLISION_CACHE_MB'] = 'lots'
        os.environ['BFBC2_EFFECTPORT_CACHE_MB'] = '-3'
        check(caches.cap('hlsl') == 3 << 19 and caches.cap('fbrb') == 2 << 30 and
              caches.cap('collision') == 256 << 20 and caches.cap('effectport') == 256 << 20,
              'cap: MiB (fractions), GiB for fbrb; a word or a negative number is the default')
        for k in ('BFBC2_HLSL_CACHE_MB', 'BFBC2_FBRB_CACHE_GB', 'BFBC2_COLLISION_CACHE_MB', 'BFBC2_EFFECTPORT_CACHE_MB'):
            del os.environ[k]
        check(caches.folder('hlsl') == os.path.join(tmp, 'bfbc2-tool-suite', 'cache', 'hlsl'),
              'folder: under LOCALAPPDATA by default')
        os.environ['BFBC2_CULLTREE_CACHE'] = '0'
        os.environ['BFBC2_EFFECTPORT_CACHE'] = os.path.join(tmp, 'elsewhere')
        check(caches.folder('culltree') is None and caches.folder('effectport') == os.path.join(tmp, 'elsewhere'),
              "folder: '0' turns a cache off, a path moves it")
        del os.environ['BFBC2_CULLTREE_CACHE'], os.environ['BFBC2_EFFECTPORT_CACHE']

        # ---- maybe_prune: once per interval, across processes
        d = os.path.join(tmp, 'maybe')
        for i in range(5):
            write(os.path.join(d, f'{i}.pkl'), 100, age=(5 - i) * 10)
        os.environ['BFBC2_CULLTREE_CACHE_MB'] = str(250 / (1 << 20))
        r1 = caches.maybe_prune('culltree', d)
        write(os.path.join(d, 'late.pkl'), 100)
        r2 = caches.maybe_prune('culltree', d)
        caches._last.clear()                                 # as another process: only the stamp says it ran
        r3 = caches.maybe_prune('culltree', d)
        r4 = caches.maybe_prune('culltree', d, every=0)
        check(r1 is not None and r1.deleted_files == 3 and r2 is None and r3 is None and
              r4 is not None and r4.deleted_files == 1 and sorted(os.listdir(d)) ==
              ['.prune.lock', '.pruned', '4.pkl', 'late.pkl'],
              'maybe_prune: prunes, then not again within the interval in this process or another',
              f'{r1} {r2} {r3} {r4} {sorted(os.listdir(d))}')
        check(caches.maybe_prune('culltree', os.path.join(tmp, 'nowhere')) is None and
              caches.maybe_prune('nosuchcache', d, every=0) is None, 'maybe_prune: a missing folder, never raises')
        del os.environ['BFBC2_CULLTREE_CACHE_MB']

        # ---- touch
        e = write(os.path.join(tmp, 'touch', 'x.bin'), 10, age=DAY)
        caches.touch(e)
        m1 = os.stat(e).st_mtime
        caches.touch(e)
        check(time.time() - m1 < 60 and os.stat(e).st_mtime == m1, 'touch: an old entry marked used; a fresh one not '
              'written again')
        caches.touch(os.path.join(tmp, 'touch', 'gone.bin'))

        # ---- the owners: culltree's row cache, pcbuild's disk cache
        suite_paths.use('culltree')
        import culltree
        d = os.path.join(tmp, 'culltree_rows')
        rc = culltree._RowCache(d)
        src = write(os.path.join(tmp, 'lvl', 'a.dbx'), 10)
        old = [write(os.path.join(d, f'old{i}.pkl'), 4000, age=DAY) for i in range(3)]
        os.environ['BFBC2_CULLTREE_CACHE_MB'] = str(1000 / (1 << 20))
        caches._last.clear()
        entry = {'code': culltree._code_id(), 'level': (1, 2), 'path': os.path.normcase(os.path.abspath(src))}
        rc.put(src, entry)
        f = rc._file(src)
        check(f is not None and os.path.isfile(f) and not any(os.path.exists(p) for p in old),
              'culltree: a row file written, the folder pruned to its cap (the old files first)', str(names(d)))
        t = time.time() - DAY
        os.utime(f, (t, t))
        got = rc.get(src, (1, 2))
        check(got is not None and time.time() - os.stat(f).st_mtime < 60, 'culltree: a hit marks its file used')
        del os.environ['BFBC2_CULLTREE_CACHE_MB']

        suite_paths.use('shader-tools')
        try:
            import pcbuild
            pcbuild.compiler_id()
        except Exception as ex:                              # noqa: BLE001 - no d3dcompiler_47 here
            print(f'skip pcbuild: {ex}')
        else:
            hroot = os.path.join(tmp, 'hlsl')
            pcbuild.use_cache(hroot)
            try:
                folder = pcbuild.cache_dir()
                old = [write(os.path.join(folder, 'aa', f'{i:064x}.bin'), 3000, age=DAY) for i in range(3)]
                write(os.path.join(hroot, 'oldcompiler', 'bb', 'x.bin'), 3000, age=2 * DAY)
                os.environ['BFBC2_HLSL_CACHE_MB'] = str(1000 / (1 << 20))
                caches._last.clear()
                key = b'\x11' * 32
                pcbuild._disk_put(key, b'compiled')
                p = pcbuild._disk_path(folder, key)
                left = names(hroot)
                check(os.path.isfile(p) and not any(os.path.exists(q) for q in old) and
                      'oldcompiler/bb/x.bin' not in left and '.pruned' in left,
                      "pcbuild: an entry written, every compiler's folder pruned to the cap", str(sorted(left)))
                t = time.time() - DAY
                os.utime(p, (t, t))
                check(pcbuild._disk_get(key) == b'compiled' and time.time() - os.stat(p).st_mtime < 60,
                      'pcbuild: a hit marks its file used')
            finally:
                pcbuild.use_cache(None)
                os.environ.pop('BFBC2_HLSL_CACHE_MB', None)

        # ---- the command
        cli = os.path.join(tmp, 'cli')
        base = os.path.join(cli, 'bfbc2-tool-suite', 'cache')
        write(os.path.join(base, 'hlsl', 'c0ffee', 'ab', 'x.bin'), 1000, age=DAY)
        write(os.path.join(base, 'hlsl', 'c0ffee', 'ab', 'y.bin'), 1000)
        write(os.path.join(base, 'fbrb', 'p.payload'), 3000)
        write(os.path.join(base, 'fbrb', 'p.seen'), 0)
        write(os.path.join(base, 'culltree', 'r.pkl'), 2000)
        write(os.path.join(base, 'culltree', 'notes.txt'), 10)
        write(os.path.join(cli, 'bfbc2-tool-suite', 'logs', 'navkit-2026-11-17.log'), 700)

        def run(*args, **env):
            return subprocess.run([sys.executable, '-m', 'bfbc2core.caches', *args], cwd=CORE, capture_output=True,
                                  text=True, env=env_for(cli, **env), timeout=60)
        out = run('--list')
        lines = {ln.split()[0]: ln for ln in out.stdout.splitlines() if ln.strip()}
        check(out.returncode == 0 and '2.0 KB' in lines.get('hlsl', '') and ' 2 file' in lines.get('hlsl', '') and
              '2.9 KB' in lines.get('fbrb', '') and '0 B' in lines.get('effectport', '') and
              'BFBC2_FBRB_CACHE_GB' in lines.get('fbrb', '') and '700 B' in lines.get('logs', ''),
              '--list: each cache with its size, files, cap and folder; the logs', out.stdout + out.stderr)
        out = run(BFBC2_EFFECTPORT_CACHE='0')
        check(out.returncode == 0 and 'off (BFBC2_EFFECTPORT_CACHE=0)' in out.stdout,
              'no option lists; a cache turned off says so', out.stdout + out.stderr)
        out = subprocess.run([sys.executable, os.path.join(CORE, 'bfbc2core', 'caches.py'), '--list', 'fbrb'],
                             capture_output=True, text=True, env=env_for(cli), timeout=60)
        check(out.returncode == 0 and out.stdout.startswith('fbrb') and 'hlsl' not in out.stdout,
              'run as a file; --list of one cache', out.stdout + out.stderr)
        out = run('--prune', 'hlsl', BFBC2_HLSL_CACHE_MB=str(1500 / (1 << 20)))
        check(out.returncode == 0 and names(os.path.join(base, 'hlsl')) >= {'c0ffee/ab/y.bin'} and
              'c0ffee/ab/x.bin' not in names(os.path.join(base, 'hlsl')) and 'deleted 1 file' in out.stdout,
              '--prune hlsl: the older entry deleted to the cap', out.stdout + out.stderr)
        out = run('--clear', 'culltree')
        check(out.returncode == 0 and names(os.path.join(base, 'culltree')) == {'notes.txt', '.prune.lock'} and
              os.path.isfile(os.path.join(base, 'fbrb', 'p.payload')),
              '--clear culltree: only that cache, only its entries', out.stdout + str(names(os.path.join(base, 'culltree'))))
        out = run('--clear')
        left = names(base)
        check(out.returncode == 0 and not any(n.endswith(('.bin', '.payload', '.pkl', '.seen')) for n in left) and
              os.path.isfile(os.path.join(cli, 'bfbc2-tool-suite', 'logs', 'navkit-2026-11-17.log')),
              '--clear: every cache emptied, the logs kept', out.stdout + str(sorted(left)))
        out = run('--clear', 'nope')
        check(out.returncode != 0 and 'unknown cache nope' in (out.stdout + out.stderr), '--clear of an unknown name '
              'is refused', out.stdout + out.stderr)
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
