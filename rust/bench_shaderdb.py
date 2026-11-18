r"""Round E4b timings: the shader database logic, as the suite and Project OPFOR use it, on the game copy's databases
(read only; written files go to a temp folder). Each workload runs in a process of its own, twice: as before round
E4b (--before: the module's ShaderDbHandle and block readers hidden, so shaderdb.py runs as it did - the Rust reader
building every list, the queries in Python) and as now; the digest of what each produced must be the same.

  python rust\bench_shaderdb.py [workload ...]       templates, templates_api, merge, textures, load, clone (default all)
  python rust\bench_shaderdb.py --run <workload> [--before]     one run (what the above starts)

Workloads (OPFOR's code copied as it is, OPFOR not imported):
  templates      wake_install.index_templates: every stock level's Dx9 database, wake_shaders.signatures per entry
  templates_api  the same with shaderdb.entry_signatures / entry_layouts (what OPFOR could call instead)
  merge          wake_shaders.merge_stock for Guadalcanal's needs list: base mp_005, every stock level's coverage,
                 the merge, Dx9 / 10 / 11 written (temp folder)
  textures       fix_layouts' pattern: entry_textures(db, {name}) for every 25th entry of six levels, and whole
  load           shaderdb.load_levels of every level, Dx9 / 10 / 11, each database's coverage of a needs list
  clone          Merger(base) + clone_entry of every 40th entry of three donors + finish + to_bytes
"""
import hashlib
import os
import subprocess
import sys
import tempfile
import time
from typing import Any

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WW2 = ('wake_island_s', 'guadal_canal', 'iwo_jima_s', 'coral_sea')
WORKLOADS = ('templates', 'templates_api', 'merge', 'textures', 'load', 'clone')


def setup(before: bool) -> Any:
    sys.path.insert(0, SUITE)
    import suite_paths
    suite_paths.use('shader-tools', 'core')
    import shaderdb as S
    if before and S._native is not None:
        class Before:                                   # the module as round E4b found it
            def __init__(self, m: Any) -> None:
                self._m = m

            def __getattr__(self, name: str) -> Any:
                if name == 'ShaderDbHandle' or name.startswith(('shaderdb_block', 'shaderdb_texture')):
                    raise AttributeError(name)
                return getattr(self._m, name)
        S._native = Before(S._native)
    return S


def stock(S: Any) -> list[str]:
    return sorted(lv for lv in S.level_bundles(S.find_opfor(None)) if lv not in WW2 and not lv.startswith(('ons_', 'opfor_')))


def signatures(S: Any, db: Any, names: Any = None) -> Any:          # wake_shaders.signatures, as it is
    progs, out, cache = db.entry_programs(), {}, {}
    for name, g in db.entries:
        if names is not None and name not in names:
            continue
        consts, samplers = set(), set()
        for i in progs.get(g, []):
            for k in db.program_refs(i)[2:]:
                if k not in cache:
                    cache[k] = ({c[0] for c in S.block_constants(db.params[k], db.e)},
                                {t[0] for t in S.block_textures(db.params[k], db.e)})
                consts |= cache[k][0]
                samplers |= cache[k][1]
        out[name] = (tuple(sorted(consts)), len(samplers))
    return out


def needs(S: Any) -> list[str]:
    p = os.path.join(S.find_opfor(None), 'work', 'ww2', 'guadal_canal', 'levels', 'guadal_canal', 'shaders',
                     'needs_bfbc2.txt')
    return S.read_need(p)


def run(work: str, before: bool) -> tuple[float, str]:
    S = setup(before)
    h = hashlib.sha1()
    t = time.perf_counter()
    if work in ('templates', 'templates_api'):
        out: dict = {}
        for lv, got in S.load_levels(stock(S), [9]):
            db = got[9]
            if isinstance(db, S.ShaderDbError):
                continue
            lay = {r[:16]: r[16:] for r in db.layouts}
            if work == 'templates':             # wake_install.index_templates, as it is
                progs = db.entry_programs()
                for n, s in signatures(S, db).items():
                    g = dict(db.entries)[n]
                    recs = frozenset(lay[db.keys[i][32:48]] for i in progs.get(g, []) if db.keys[i][32:48] in lay)
                    out.setdefault(s, []).append((lv, n, recs))
            else:
                lays = S.entry_layouts(db)
                for n, s in S.entry_signatures(db).items():
                    out.setdefault(s, []).append((lv, n, frozenset(lay[x] for x in lays[n] if x in lay)))
        for s in sorted(out, key=repr):
            h.update(repr((s, sorted((lv, n, sorted(r)) for lv, n, r in out[s]))).encode())
    elif work == 'merge':
        wanted = needs(S)
        with tempfile.TemporaryDirectory(prefix='bench_e4b_') as tmp:
            bases = {dx: db for dx, db in S.load_level('mp_005').items()}
            have = {dx: {n for n, c in S.coverage(b, wanted).items() if c} for dx, b in bases.items()}
            chosen: dict = {dx: [] for dx in bases}
            for lv, got in S.load_levels([x for x in stock(S) if x != 'mp_005'], list(bases), chunk=4):
                for dx, db in got.items():
                    if isinstance(db, S.ShaderDbError):
                        continue
                    new = {n for n, c in S.coverage(db, wanted).items() if c} - have[dx]
                    if new:
                        chosen[dx].append((db, lv))
                        have[dx] |= new
            for dx, base in bases.items():
                m = S.merge(base, chosen[dx], wanted)
                data = base.to_bytes()
                missing = [n for n, c in S.coverage(base, wanted).items() if not c]
                with open(os.path.join(tmp, f'Dx{dx}_Single.res'), 'wb') as f:
                    f.write(data)
                h.update(data)
                h.update(repr((m.report, m.added, missing)).encode())
    elif work == 'textures':
        for lv in stock(S)[:6]:
            db = S.load(f'level:{lv}', 9)[0]
            for n, _ in db.entries[::25]:
                h.update(repr(S.entry_textures(db, {n}).get(n, [])).encode())
            h.update(repr(S.entry_textures(db)).encode())
    elif work == 'load':
        wanted = needs(S)
        for lv, got in S.load_levels(sorted(S.level_bundles(S.find_opfor(None))), (9, 10, 11)):
            for dx, db in got.items():
                if isinstance(db, S.ShaderDb):
                    h.update(repr((lv, dx, len(db.entries), sorted(S.coverage(db, wanted).items()))).encode())
    elif work == 'clone':
        base = S.load('level:mp_005', 9)[0]
        for lv in [x for x in stock(S) if x != 'mp_005'][:3]:
            d = S.load(f'level:{lv}', 9)[0]
            m = S.Merger(base, [])
            for nm, _ in d.entries[::40]:
                try:
                    m.clone_entry(d, nm, nm + '#e4b', {}, lv)
                except S.ShaderDbError as e:
                    h.update(str(e).encode())
            m.finish()
        h.update(base.to_bytes())
    return time.perf_counter() - t, h.hexdigest()


def main(argv: list[str]) -> int:
    if '--run' in argv:
        work = argv[argv.index('--run') + 1]
        secs, digest = run(work, '--before' in argv)
        print(f'{secs:.2f} {peak_mb():.0f} {digest}')
        return 0
    works = [a for a in argv[1:] if a in WORKLOADS] or list(WORKLOADS)
    env = dict(os.environ, BFBC2_FBRB_CACHE=os.environ.get('BFBC2_FBRB_CACHE', '0'))
    bad = 0
    print(f'{"workload":14} {"before":>8} {"E4b":>8} {"peak MB before / E4b":>22}  digest')
    for w in works:
        got = []
        for flag in (['--before'], []):
            r = subprocess.run([sys.executable, __file__, '--run', w, *flag], capture_output=True, text=True, env=env)
            if r.returncode:
                print(r.stderr[-2000:])
                return 1
            took, peak, digest = r.stdout.split()[-3:]
            got.append((float(took), digest, peak))
        same = got[0][1] == got[1][1]
        bad += not same
        print(f'{w:14} {got[0][0]:7.2f}s {got[1][0]:7.2f}s {got[0][2]:>11} / {got[1][2]:<8}  '
              f'{"same" if same else "DIFFERENT"} {got[1][1][:12]}', flush=True)
    return 1 if bad else 0


def peak_mb() -> float:
    """The process's peak commit (Windows), in MB; 0 elsewhere."""
    if sys.platform != 'win32':
        return 0.0
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + [
            (n, ctypes.c_size_t) for n in ('PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage',
                                           'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
                                           'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage')]
    c = Counters()
    c.cb = ctypes.sizeof(c)
    k32 = ctypes.WinDLL('kernel32')
    k32.GetCurrentProcess.restype = wintypes.HANDLE
    k32.K32GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    k32.K32GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb)
    return float(c.PeakPagefileUsage) / 1e6


if __name__ == '__main__':
    sys.exit(main(sys.argv))
