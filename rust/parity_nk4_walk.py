r"""Round NK4's parity on real levels: NavKit's walkability end to end (navsample.load, navwalk.coarse, the hand edits,
the water and reference-extent rules, navwalk.walkable) - the old Python (a suite checkout from before NK4, 30f19b0 or
earlier) against the Rust (a checkout with NK4: bc2_navbuild::walk through bc2_native), on every NavKit regression
level (navregress.build_level: Project OPFOR BFBC2's in-game samples, read only; the build clock fixed).

  python rust\parity_nk4_walk.py capture <suite> <opfor> <out.jsonl> [level ...]
      build each level with `suite` (each level in its own process, one at a time), recording per stage (the normal
      pass and the detailed pass) what walkability hands the rest of the build - the sample run (start, y, normal,
      flags, hit, pitch, origin, grid) and keep / gx / gz / stair with their dtypes, as SHA-1s - with the stage's
      walkability time and the process's peak private bytes during it (a sampler thread, every 10 ms), the build's log
      lines (seconds blanked), navwalk's statistics and the path data's SHA-1; one JSON line per level
  python rust\parity_nk4_walk.py compare <old.jsonl> <new.jsonl>
      the two captures value for value (the times and memory apart), and the times and peaks side by side

Levels: the 15 regression builds of work\navkit\regress (navregress's levels) when none are named.
"""
import ctypes
import ctypes.wintypes
import hashlib
import json
import os
import re
import subprocess
import sys
import threading
import time
from typing import Any

LEVELS = ['ons_mp_002', 'ons_mp_004', 'sp_003_b', 'sp_004_b', 'sp_005', 'sp_005_b', 'sp_006', 'sp_007', 'sp_008',
          'sp_008_b', 'sp_009', 'sp_01', 'sp_02', 'sp_03', 'sp_04']
TIMED = ('walk_s', 'walk_peak_mb', 'walk_start_mb', 'build_s', 'peak_mb')


class _Counters(ctypes.Structure):
    _fields_ = [('cb', ctypes.wintypes.DWORD), ('PageFaultCount', ctypes.wintypes.DWORD),
                ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t)]


def memory() -> tuple[float, float]:
    """(private bytes, peak private bytes) of this process, MB: the memory it committed (PagefileUsage), which,
    unlike the working set, does not drop when Windows trims pages under the other processes' load."""
    c = _Counters()
    c.cb = ctypes.sizeof(c)
    k32 = ctypes.WinDLL('kernel32')
    k32.GetCurrentProcess.restype = ctypes.wintypes.HANDLE
    psapi = ctypes.WinDLL('psapi')
    psapi.GetProcessMemoryInfo.argtypes = [ctypes.wintypes.HANDLE, ctypes.POINTER(_Counters), ctypes.wintypes.DWORD]
    psapi.GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb)
    return c.PagefileUsage / 2 ** 20, c.PeakPagefileUsage / 2 ** 20


class Sampler:
    """The largest private bytes since the last reset, sampled every 10 ms on a thread."""

    def __init__(self) -> None:
        self.top = 0.0
        self.lock = threading.Lock()
        t = threading.Thread(target=self._run, daemon=True)
        t.start()

    def _run(self) -> None:
        while True:
            ws, _ = memory()
            with self.lock:
                self.top = max(self.top, ws)
            time.sleep(0.01)

    def reset(self) -> float:
        ws, _ = memory()
        with self.lock:
            self.top = ws
        return ws

    def peak(self) -> float:
        ws, _ = memory()
        with self.lock:
            return max(self.top, ws)


def sha(a: Any) -> str:
    import numpy as np
    if a is None:
        return 'None'
    a = np.ascontiguousarray(a)
    return f'{a.dtype.str}{a.shape}:' + hashlib.sha1(a.tobytes()).hexdigest()


def run_child(suite: str, opfor: str, lv: str) -> dict[str, Any]:
    sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('core', 'navkit-tools')
    import navbuild
    import navregress
    import navwalk
    sys.path.insert(0, os.path.join(suite, 'bfbc2-auto-navkit', 'tests'))
    import test_navbuild_native as golden_test
    sampler = Sampler()
    stages: list[dict[str, Any]] = []
    window: dict[str, Any] = {}
    real_stage = navbuild.stage

    def stage(*a: Any, **k: Any) -> Any:
        window['start_mb'] = sampler.reset()
        window['t'] = time.perf_counter()
        return real_stage(*a, **k)
    navbuild.stage = stage
    real_init = navbuild.Level.__init__

    def init(self: Any, s: Any, keep: Any, step: Any, gx: Any = None, gz: Any = None, stair: Any = None,
             cfg: Any = navbuild.DEFAULTS) -> None:
        if 't' in window:
            took, peak = time.perf_counter() - window.pop('t'), sampler.peak()
            stages.append({'pitch': s.pitch, 'grid': [s.nx, s.nz, s.min_x, s.min_z], 'surfaces': len(s.y),
                           'start': sha(s.start), 'y': sha(s.y), 'normal': sha(s.normal), 'flags': sha(s.flags),
                           'hit': sha(s.hit), 'keep': sha(keep), 'kept': int(keep.sum()), 'gx': sha(gx), 'gz': sha(gz),
                           'stair': sha(stair), 'steep_stats': dict(navwalk.STEEP_STATS), 'walk_s': round(took, 2),
                           'walk_peak_mb': round(peak), 'walk_start_mb': round(window['start_mb'])})
        real_init(self, s, keep, step, gx, gz, stair, cfg)
    navbuild.Level.__init__ = init
    lines: list[str] = []
    out = os.path.join(os.environ.get('TEMP', '.'), f'nk4_{lv}_{os.getpid()}.hpd')

    def log(*a: Any) -> None:
        lines.append(re.sub(r'\d+(\.\d+)? s\b', 'N s', ' '.join(str(x) for x in a).replace(out, '<out>')))
    clock = time.localtime
    time.localtime = lambda *a: golden_test.FIXED_CLOCK
    t = time.perf_counter()
    try:
        navregress.build_level(opfor, lv, out, log=log)
    finally:
        time.localtime = clock
    took = time.perf_counter() - t
    _ws, peak = memory()
    data = open(out, 'rb').read()
    os.remove(out)
    res: dict[str, Any] = {'level': lv, 'sha1': hashlib.sha1(data).hexdigest(), 'bytes': len(data), 'stages': stages,
                           'log': lines, 'build_s': round(took, 1), 'peak_mb': round(peak)}
    g = golden_test.load().get('levels', {}).get(lv)
    if g:
        inputs = {os.path.relpath(q, opfor).replace(os.sep, '/'): golden_test.file_sha1(q)
                  for q in golden_test.input_files(opfor, lv, navregress)}
        res['golden'] = ('inputs changed' if inputs != g['inputs'] else
                         'as recorded' if g['pathdata'] == res['sha1'] else 'DIFFERENT')
    return res


def strip(r: dict[str, Any]) -> dict[str, Any]:
    out = {k: v for k, v in r.items() if k not in TIMED}
    out['stages'] = [{k: v for k, v in s.items() if k not in TIMED} for s in r.get('stages', [])]
    return out


def main() -> int:
    me = os.path.abspath(__file__)
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        suite, opfor, lv = sys.argv[2:5]
        print('RESULT ' + json.dumps(run_child(suite, opfor, lv), sort_keys=True), flush=True)
        return 0
    cmd = sys.argv[1] if len(sys.argv) > 1 else ''
    if cmd == 'capture':
        suite, opfor, out = sys.argv[2:5]
        for lv in sys.argv[5:] or LEVELS:
            p = subprocess.run([sys.executable, me, '--child', suite, opfor, lv], capture_output=True, text=True)
            got = [ln[7:] for ln in p.stdout.splitlines() if ln.startswith('RESULT ')]
            r = json.loads(got[-1]) if got else {'level': lv, 'error': (p.stderr or p.stdout)[-3000:]}
            with open(out, 'a', encoding='utf-8', newline='\n') as f:
                f.write(json.dumps(r, sort_keys=True) + '\n')
            print(json.dumps({k: v for k, v in r.items() if k not in ('log', 'stages')}, sort_keys=True), flush=True)
        return 0
    if cmd == 'compare':
        def read(p: str) -> dict[str, dict[str, Any]]:
            return {r['level']: r for r in map(json.loads, open(p, encoding='utf-8'))}
        old, new = read(sys.argv[2]), read(sys.argv[3])
        same = 0
        for lv in [lv for lv in LEVELS if lv in old and lv in new]:
            a, b = old[lv], new[lv]
            eq = strip(a) == strip(b)
            same += eq
            diff = [] if eq else [k for k in strip(a) if strip(a).get(k) != strip(b).get(k)]
            times = ' | '.join(f'{sa.get("pitch")}: {sa.get("walk_s")} -> {sb.get("walk_s")} s, '
                               f'{sa.get("walk_peak_mb")} -> {sb.get("walk_peak_mb")} MB'
                               for sa, sb in zip(a.get('stages', []), b.get('stages', [])))
            print(f'{lv}: {"identical" if eq else "DIFFERENT " + str(diff)}; {times}; build {a.get("build_s")} -> '
                  f'{b.get("build_s")} s, peak {a.get("peak_mb")} -> {b.get("peak_mb")} MB; golden {b.get("golden")}')
        print(f'{same} of {len([lv for lv in LEVELS if lv in old and lv in new])} levels identical')
        return 0
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main())
