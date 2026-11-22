r"""Round NK5's parity on real levels: NavKit's whole build (the stage, merge_detailed, the file) - the old build (a
suite checkout from before NK5, eee12ba or earlier: the stage's glue in Python) against the new one (a checkout with
NK5: bc2_navbuild::build through bc2_native, or bfbc2.exe), on every NavKit regression level (navregress.build_level:
Project OPFOR BFBC2's in-game samples, read only; the build clock fixed).

  python rust\parity_nk5_build.py capture <suite> <opfor> <out.jsonl> [level ...]
      build each level with `suite` (each level in its own process, one at a time), recording the path data's SHA-1
      and size, the build's log lines (seconds and the output path blanked), the build's time and the process's
      peak private bytes (a sampler thread, every 10 ms) with the time and peak of each stage (from the stage's log
      lines as they come); one JSON line per level
  python rust\parity_nk5_build.py compare <old.jsonl> <new.jsonl>
      the two captures value for value (the times and memory apart), and the times and peaks side by side

Levels: the 15 regression builds of work\navkit\regress (navregress's levels) when none are named.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from parity_nk4_walk import LEVELS, Sampler, memory  # noqa: E402

TIMED = ('build_s', 'peak_mb', 'phases')


def run_child(suite: str, opfor: str, lv: str) -> dict[str, Any]:
    sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('core', 'navkit-tools')
    import navregress
    sys.path.insert(0, os.path.join(suite, 'bfbc2-auto-navkit', 'tests'))
    import test_navbuild_native as golden_test
    sampler = Sampler()
    lines: list[str] = []
    phases: list[tuple[str, float, float]] = []
    out = os.path.join(os.environ.get('TEMP', '.'), f'nk5_{lv}_{os.getpid()}.hpd')
    t = time.perf_counter()
    sampler.reset()

    def log(*a: Any) -> None:
        text = ' '.join(str(x) for x in a)
        phases.append((text[:40], round(time.perf_counter() - t, 2), round(sampler.peak())))
        sampler.reset()
        lines.append(re.sub(r'\d+(\.\d+)? s\b', 'N s', text.replace(out, '<out>')))
    clock = time.localtime
    time.localtime = lambda *a: golden_test.FIXED_CLOCK
    try:
        navregress.build_level(opfor, lv, out, log=log)
    finally:
        time.localtime = clock
    took = time.perf_counter() - t
    _ws, peak = memory()
    data = open(out, 'rb').read()
    os.remove(out)
    res: dict[str, Any] = {'level': lv, 'sha1': hashlib.sha1(data).hexdigest(), 'bytes': len(data), 'log': lines,
                           'build_s': round(took, 1), 'peak_mb': round(peak), 'phases': phases}
    g = golden_test.load().get('levels', {}).get(lv)
    if g:
        inputs = {os.path.relpath(q, opfor).replace(os.sep, '/'): golden_test.file_sha1(q)
                  for q in golden_test.input_files(opfor, lv, navregress)}
        res['golden'] = ('inputs changed' if inputs != g['inputs'] else
                         'as recorded' if g['pathdata'] == res['sha1'] else 'DIFFERENT')
    return res


def strip(r: dict[str, Any]) -> dict[str, Any]:
    return {k: v for k, v in r.items() if k not in TIMED}


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
            print(json.dumps({k: v for k, v in r.items() if k not in ('log', 'phases')}, sort_keys=True), flush=True)
        return 0
    if cmd == 'compare':
        def read(p: str) -> dict[str, dict[str, Any]]:
            rows: dict[str, dict[str, Any]] = {}
            for r in map(json.loads, open(p, encoding='utf-8')):
                rows[r['level']] = r                    # the last capture of a level
            return rows
        old, new = read(sys.argv[2]), read(sys.argv[3])
        same = 0
        both = [lv for lv in LEVELS if lv in old and lv in new]
        for lv in both:
            a, b = old[lv], new[lv]
            eq = strip(a) == strip(b)
            same += eq
            diff = [] if eq else [k for k in strip(a) if strip(a).get(k) != strip(b).get(k)]
            if 'log' in diff:
                for x, y in zip(a.get('log', []), b.get('log', [])):
                    if x != y:
                        print(f'   old: {x}\n   new: {y}')
                if len(a.get('log', [])) != len(b.get('log', [])):
                    print(f'   log lines {len(a.get("log", []))} -> {len(b.get("log", []))}')
            print(f'{lv}: {"identical" if eq else "DIFFERENT " + str(diff)}; build {a.get("build_s")} -> '
                  f'{b.get("build_s")} s, peak {a.get("peak_mb")} -> {b.get("peak_mb")} MB; golden {b.get("golden")}')
        print(f'{same} of {len(both)} levels identical')
        return 0
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main())
