r"""Times the bundle cache (round E3e, rust\BENCHMARKS.md "E3e") on a replay of OPFOR's WW2 level build's bundle
reads: rust\bench_fbrb_replay.json, read from the game copy (read only).

  python rust\bench_fbrb_cache.py [--opfor <folder>] [--only before|cold|warm|steps] [--steps N]

  before  every read with fbrb.read_uncached (the reader before E3e: each read decompresses from the start)
  cold    every read with fbrb.read, in one process, an empty disk cache (a folder of its own, deleted after); then
          warm: the same again in a new process, the disk cache left by cold
  memory  every read with fbrb.read, the disk cache off (memory only)
  steps   the replay cut into N processes run one after another (a rebuild's steps), an empty disk cache, then
          the same N processes again (warm); steps-before: the N processes without the cache

Each run is a child process (below-normal priority on Windows) that prints its time, its cache statistics, its peak
memory and a SHA-1 of every read's bytes: every run must give the same 731 hashes.

The replay (bench_fbrb_replay.json): [[step, bundle path under game\, [entry names]], ...], made by --make from
OPFOR's captured reads (material_grid.wake_names and detail_layers.index_levels, logged read only on 2026-11-17) and
the profile's other callers rebuilt from the same bundles (seeded): see BENCHMARKS.md.
"""
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import fbrb, paths  # noqa: E402

REPLAY = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'bench_fbrb_replay.json')


def peak_mb() -> float:
    """This process's peak working set, MB (Windows; 0 elsewhere)."""
    if os.name != 'nt':
        return 0.0
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD),
                    ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                    ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                    ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                    ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t)]
    c = Counters()
    c.cb = ctypes.sizeof(c)
    current = ctypes.windll.kernel32.GetCurrentProcess
    current.restype = wintypes.HANDLE
    info = ctypes.windll.psapi.GetProcessMemoryInfo
    info.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    info(current(), ctypes.byref(c), c.cb)
    return c.PeakWorkingSetSize / 2**20


def worker(game: str, mode: str, first: int, last: int) -> None:
    """Replays reads first..last-1 and prints {'s', 'sha1', 'stats', 'peak_mb'} as JSON."""
    replay = json.load(open(REPLAY))[first:last]
    if not fbrb.cache_stats():
        raise SystemExit('the Rust core with the bundle cache is not in use (python rust\\build_native.py)')
    read = fbrb.read_uncached if mode == 'before' else fbrb.read
    sums = []
    t = time.perf_counter()
    reading = 0.0
    for _, rel, names in replay:
        r0 = time.perf_counter()
        got = read(os.path.join(game, rel), names)
        reading += time.perf_counter() - r0
        h = hashlib.sha1()
        for n in names:
            h.update(n.encode())
            h.update(got[n])
        sums.append(h.hexdigest())
    s = time.perf_counter() - t
    print(json.dumps({'s': s, 'read_s': reading, 'sums': sums, 'stats': fbrb.cache_stats(),
                      'peak_mb': round(peak_mb())}))


def child(game: str, mode: str, first: int, last: int, cache: str) -> dict:
    env = dict(os.environ, BFBC2_FBRB_CACHE=cache)
    flags = 0x4000 if os.name == 'nt' else 0                      # BELOW_NORMAL_PRIORITY_CLASS
    r = subprocess.run([sys.executable, os.path.abspath(__file__), '--worker', game, mode, str(first), str(last)],
                       capture_output=True, text=True, env=env, creationflags=flags)
    if r.returncode:
        raise SystemExit(r.stderr)
    return json.loads(r.stdout.strip().splitlines()[-1])


def show(label: str, runs: list[dict]) -> None:
    s = sum(r['s'] for r in runs)
    st: dict[str, int] = {}
    for r in runs:
        for k, v in r['stats'].items():
            st[k] = st.get(k, 0) + v
    rs = sum(r['read_s'] for r in runs)
    print(f'{label:34} {s:8.2f} s (reads {rs:7.2f} s)  peak {max(r["peak_mb"] for r in runs):5d} MB  '
          f'decompressed {st.get("decompressed_bytes", 0) / 2**30:6.2f} GiB  disk hits {st.get("disk_hits", 0):4d}  '
          f'memory hits {st.get("memory_hits", 0):4d}  written {st.get("written_bytes", 0) / 2**30:5.2f} GiB')


def make(game: str, captured: list[str]) -> None:
    """bench_fbrb_replay.json from captured logs ([{'bundle', 'names'}]: the 'grid' and 'levels' captures) and the
    profile's other callers rebuilt with a seeded choice of entries from the bundles the captures read."""
    def rel(p: str) -> str:
        return os.path.relpath(p, game)
    grid = [('material_grid', rel(e['bundle']), e['names']) for e in json.load(open(captured[0]))]
    levels = [('shaderdb', rel(e['bundle']), e['names']) for e in json.load(open(captured[1]))]
    rng = random.Random(726)
    commons = sorted({b for _, b, _ in grid})
    ents = {b: fbrb.names(os.path.join(game, b)) for b in commons}
    res = [(b, e.name) for b in commons for e in ents[b].values() if e.name.lower().endswith('.res')]
    tex = [(b, e.name) for b, n in res for e in [ents[b][n.lower()]] if e.type == 'ITexture']
    dbx = {b: [e.name for e in ents[b].values() if e.name.lower().endswith('.dbx')] for b in commons}
    out = []
    # the level build (one process): 415 shader database reads, the material grid's 59 (here 71), meshview's 104
    # single resources, effectport's 111 batches of partitions; then wake_translate's 30 texture headers
    shader = [levels[i % len(levels)] for i in range(415)]
    mesh = [('meshview', b, [n]) for b, n in rng.sample(res, 104)]
    effects = []
    for _ in range(111):
        b = rng.choice([b for b in commons if dbx[b]])
        effects.append(('effectport', b, rng.sample(dbx[b], min(len(dbx[b]), rng.randint(1, 12)))))
    build = shader + grid + mesh + effects
    rng.shuffle(build)
    out = [('level_build', b, n) for _, b, n in build]
    out += [('wake_translate', b, [n]) for b, n in rng.sample(tex, 30)]
    lines = ',\n'.join(json.dumps(r) for r in out)
    open(REPLAY, 'w', encoding='utf-8', newline='\n').write(f'[\n{lines}\n]\n')
    print(f'{len(out)} reads of {len({b for _, b, _ in out})} bundles -> {REPLAY}')


def main(argv: list[str]) -> int:
    if argv[1:2] == ['--worker']:
        worker(argv[2], argv[3], int(argv[4]), int(argv[5]))
        return 0
    opfor = argv[argv.index('--opfor') + 1] if '--opfor' in argv else None
    game = paths.game_dir(paths.find_opfor(opfor))
    if '--make' in argv:
        make(game, argv[argv.index('--make') + 1:argv.index('--make') + 3])
        return 0
    only = set(argv[argv.index('--only') + 1].split(',')) if '--only' in argv else {'before', 'cold', 'memory', 'steps'}
    steps = int(argv[argv.index('--steps') + 1]) if '--steps' in argv else 6
    replay = json.load(open(REPLAY))
    n = len(replay)
    print(f'{n} reads of {len({b for _, b, _ in replay})} bundles')
    hashes = set()

    def keep(runs: list[dict]) -> None:
        hashes.add(hashlib.sha1(''.join(x for r in runs for x in r['sums']).encode()).hexdigest())
    if 'before' in only:
        r = child(game, 'before', 0, n, '0')
        show('before (no cache)', [r])
        keep([r])
    if 'cold' in only:
        cache = tempfile.mkdtemp(prefix='fbrb_cache_bench_')
        try:
            r = child(game, 'cached', 0, n, cache)
            show('cold (one process, empty cache)', [r])
            keep([r])
            r = child(game, 'cached', 0, n, cache)
            show('warm (a new process, its cache)', [r])
            keep([r])
        finally:
            shutil.rmtree(cache, ignore_errors=True)
    if 'memory' in only:
        r = child(game, 'cached', 0, n, '0')
        show('memory only (disk cache off)', [r])
        keep([r])
    if 'steps' in only:
        cache = tempfile.mkdtemp(prefix='fbrb_cache_bench_')
        cuts = [n * i // steps for i in range(steps + 1)]
        try:
            for label in ('cold', 'warm'):
                runs = [child(game, 'cached', a, b, cache) for a, b in zip(cuts, cuts[1:])]
                show(f'{steps} processes, {label}', runs)
                keep(runs)
        finally:
            shutil.rmtree(cache, ignore_errors=True)
    if 'steps-before' in only:
        cuts = [n * i // steps for i in range(steps + 1)]
        before = [child(game, 'before', a, b, '0') for a, b in zip(cuts, cuts[1:])]
        show(f'{steps} processes, before', before)
        keep(before)
    print('the same bytes in every run' if len(hashes) == 1 else f'DIFFERENT BYTES: {hashes}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
