r"""Peak memory of the Rust core's batch readers, as Python calls them (round PR1b-1): each measure runs in a child
process whose committed memory (PrivateUsage) this process samples every 2 ms while the call runs; printed is the
peak above the memory held before the call (the inputs), and the call's time.

  python rust\bench_memory.py [--suite <checkout>] [--game <game copy>] [--only dbx,xml,fbrb,xenos,granny]

--suite picks the checkout whose core\bfbc2core (and its bc2_native module) the children import, so the same script
measures before (another checkout, e.g. main) and after. The inputs are the game copy's: every .dbx of mp_common's
and sp_009's level bundles (dbx: dbx_parse_many, xml: dbx_to_xml_many), every Dist level bundle's .dbx entries
(fbrb: fbrb_read_many, the bundle cache off), every shader of mp_common's Dx9 shader database (xenos:
xenos_parse_many), every .grannymodel / .grannyanimation resource of mp_common (granny: granny_roots_many).
"""
import argparse
import ctypes
import json
import os
import subprocess
import sys
import threading
import time
from ctypes import wintypes

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)


class _Counters(ctypes.Structure):
    _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD),
                ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t),
                ('PrivateUsage', ctypes.c_size_t)]


def _private_mb(pid: int) -> float:
    """The process's committed private memory in MB (0 when it is gone)."""
    k32 = ctypes.windll.kernel32
    k32.OpenProcess.restype = wintypes.HANDLE
    h = k32.OpenProcess(0x0400 | 0x0010, False, pid)        # PROCESS_QUERY_INFORMATION | PROCESS_VM_READ
    if not h:
        return 0.0
    try:
        c = _Counters()
        c.cb = ctypes.sizeof(c)
        info = ctypes.windll.psapi.GetProcessMemoryInfo
        info.argtypes = [wintypes.HANDLE, ctypes.POINTER(_Counters), wintypes.DWORD]
        return c.PrivateUsage / 2**20 if info(h, ctypes.byref(c), c.cb) else 0.0
    finally:
        k32.CloseHandle(h)


def _bundles(game: str, *rel: str) -> list[str]:
    return [os.path.join(game, 'Dist', 'win32', *r.split('/')) for r in rel]


def child(suite: str, game: str, what: str) -> None:
    """Loads the inputs, prints READY, waits for a line, runs the call, prints DONE <seconds> <digest>."""
    import hashlib
    sys.path.insert(0, os.path.join(suite, 'core'))
    from bfbc2core import native
    n = native.require('bench_memory', 'dbx_parse_many')
    n.fbrb_cache_configure(memory=0, no_disk=True)
    from bfbc2core import dbx, fbrb

    def dbx_of(path: str) -> list[bytes]:
        names = [e.name for e in fbrb.entries(path) if e.name.lower().endswith('.dbx') and not e.deleted]
        got = fbrb.read_uncached(path, names)
        return [got[k] for k in names]

    if what in ('dbx', 'xml'):
        items = [d for b in _bundles(game, 'levels/mp_common/level-00.fbrb', 'levels/sp_009/level-00.fbrb')
                 for d in dbx_of(b)]

        def call() -> object:
            if what == 'dbx':
                out = dbx.parse_many(items)
                return [len(f.roots) if not isinstance(f, Exception) else -1 for f in out]
            return [hashlib.sha1(x.encode()).hexdigest() if isinstance(x, str) else '-' for x in dbx.to_xml_many(items)]
    elif what == 'fbrb':
        root = os.path.join(game, 'Dist', 'win32', 'levels')
        paths = sorted(os.path.join(root, d, 'level-00.fbrb') for d in os.listdir(root)
                       if os.path.isfile(os.path.join(root, d, 'level-00.fbrb')))
        jobs = [(p, [e.name for e in fbrb.entries(p) if e.name.lower().endswith('.dbx') and not e.deleted])
                for p in paths]

        def call() -> object:
            return [sum(len(v) for v in r.values()) if isinstance(r, dict) else -1 for r in fbrb.read_many(jobs)]
    elif what == 'xenos':
        sys.path.insert(0, os.path.join(suite, 'bfbc2-shader-tools'))
        import shaderdb
        import xenos
        levels = os.path.join(os.path.dirname(game), 'work', 'xbox', 'dist', 'xenon', 'levels')
        items = []
        for lv in sorted(os.listdir(levels)):
            path = os.path.join(levels, lv, 'level-00.fbrb')
            if os.path.isfile(path) and shaderdb.bundle_databases(path):
                db, _ = shaderdb.load(path)
                items += [(bytes(s.code), False) for s in db.vs] + [(bytes(s.code), True) for s in db.ps]

        def call() -> object:
            return [0 if r is None else 1 for r in xenos.parse_many(items)]
    elif what == 'granny':
        b = _bundles(game, 'levels/mp_common/level-00.fbrb')[0]
        names = [e.name for e in fbrb.entries(b) if e.type in ('GrannyModel', 'GrannyAnimation') and not e.deleted]
        got = fbrb.read_uncached(b, names)
        blobs = [got[k] for k in names]

        def call() -> object:
            return [0 if isinstance(r, Exception) else 1 for r in n.granny_roots_many(blobs, 64)]
    else:
        raise SystemExit(f'unknown measure {what}')
    import gc
    gc.collect()
    print(f'READY {len(items) if what not in ("fbrb", "granny") else len(jobs) if what == "fbrb" else len(blobs)}',
          flush=True)
    sys.stdin.readline()
    t = time.perf_counter()
    result = call()
    s = time.perf_counter() - t
    print(f'DONE {s:.3f} {hashlib.sha1(json.dumps(result, sort_keys=True).encode()).hexdigest()[:12]}', flush=True)


def measure(suite: str, game: str, what: str) -> dict[str, object]:
    p = subprocess.Popen([sys.executable, os.path.abspath(__file__), '--child', what, '--suite', suite, '--game', game],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    assert p.stdin and p.stdout
    ready = p.stdout.readline().split()
    if not ready or ready[0] != 'READY':
        p.wait()
        raise SystemExit(f'{what}: the child failed ({ready})')
    time.sleep(0.3)
    base = _private_mb(p.pid)
    peak = [base]
    stop = threading.Event()

    def sample() -> None:
        while not stop.is_set():
            peak[0] = max(peak[0], _private_mb(p.pid))
            time.sleep(0.002)
    th = threading.Thread(target=sample, daemon=True)
    th.start()
    p.stdin.write('\n')
    p.stdin.flush()
    done = p.stdout.readline().split()
    stop.set()
    th.join()
    p.wait()
    return {'measure': what, 'items': int(ready[1]), 'inputs_mb': round(base), 'peak_above_mb': round(peak[0] - base),
            'seconds': float(done[1]), 'digest': done[2]}


class _Entry32(ctypes.Structure):
    _fields_ = [('dwSize', wintypes.DWORD), ('cntUsage', wintypes.DWORD), ('th32ProcessID', wintypes.DWORD),
                ('th32DefaultHeapID', ctypes.c_size_t), ('th32ModuleID', wintypes.DWORD),
                ('cntThreads', wintypes.DWORD), ('th32ParentProcessID', wintypes.DWORD),
                ('pcPriClassBase', ctypes.c_long), ('dwFlags', wintypes.DWORD), ('szExeFile', ctypes.c_char * 260)]


def _tree(pid: int) -> list[int]:
    """pid and every process below it (a Toolhelp snapshot)."""
    k32 = ctypes.windll.kernel32
    k32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    snap = k32.CreateToolhelp32Snapshot(0x2, 0)
    parents: dict[int, list[int]] = {}
    e = _Entry32()
    e.dwSize = ctypes.sizeof(e)
    ok = k32.Process32First(snap, ctypes.byref(e))
    while ok:
        parents.setdefault(e.th32ParentProcessID, []).append(e.th32ProcessID)
        ok = k32.Process32Next(snap, ctypes.byref(e))
    k32.CloseHandle(snap)
    out, todo = [], [pid]
    while todo:
        p = todo.pop()
        out.append(p)
        todo += [c for c in parents.get(p, []) if c not in out]
    return out


def command(argv: list[str]) -> None:
    """Runs argv, echoing its output with the seconds since the start and the process tree's committed memory then;
    at the end, the tree's peak and the line printed last before it."""
    t0 = time.time()
    p = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors='replace')
    assert p.stdout
    state = {'peak': 0.0, 'at': 0.0, 'line': '', 'peak_line': ''}
    stop = threading.Event()

    def sample() -> None:
        while not stop.is_set():
            each = sorted((round(_private_mb(x)) for x in _tree(p.pid)), reverse=True)
            mb = sum(each)
            if mb > state['peak']:
                state.update(peak=mb, at=time.time() - t0, peak_line=state['line'], each=each)
            time.sleep(0.05)
    th = threading.Thread(target=sample, daemon=True)
    th.start()
    for line in p.stdout:
        state['line'] = line.rstrip()
        print(f'{time.time() - t0:8.1f} s {sum(_private_mb(x) for x in _tree(p.pid)):7.0f} MB | {line.rstrip()}',
              flush=True)
    p.wait()
    stop.set()
    th.join()
    print(f'peak {state["peak"]:.0f} MB at {state["at"]:.1f} s, after: {state["peak_line"]}; exit {p.returncode}')
    print(f'the processes at the peak, MB: {state.get("each")}')


def main() -> None:
    if len(sys.argv) > 2 and sys.argv[1] == '--command':
        command(sys.argv[2:])
        return
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--suite', default=SUITE)
    ap.add_argument('--game', default=os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2', 'game'))
    ap.add_argument('--only', default='dbx,xml,fbrb,xenos,granny')
    ap.add_argument('--child')
    a = ap.parse_args()
    if a.child:
        child(a.suite, a.game, a.child)
        return
    for what in a.only.split(','):
        print(json.dumps(measure(a.suite, a.game, what), sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
