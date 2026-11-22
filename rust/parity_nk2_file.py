r"""Round NK2's parity on real levels: a NavKit build's last stage, "graphs to file", the old Python (a suite checkout
from before NK2, 08b1382 or earlier: its navbuild.py with navislands.prune / prune_order_nodes / finalize_graph /
write_file / sector_links and both resolvers) against the new Rust (this checkout: bc2_native.navkit_write_graphs and
NavLevel.sector_links), on every NavKit regression level (navregress.build_level: Project OPFOR BFBC2's in-game
samples, read only).

Each level is built once, by this checkout, with the build clock fixed (test_navbuild_native.FIXED_CLOCK). Inside the
build every sector_links call runs the old Python on the same stage too (the stage's resolver, or merge_detailed's
same-grid / cross-grid pair) and the links are compared; at write_file the old Python writes the same graphs to a
second file (navislands.prune, then the old write_file) and the two files, and their log lines, are compared byte for
byte. The new file's SHA-1 is compared with tests/navbuild_golden.json where the level is recorded (with its inputs).
Then each stage's inputs (the graphs, anchors, order and terrain ids) are run again alone in a fresh process, old and
new, for the time and the peak working set; and the whole level is built once more by each checkout alone (its
sector_links calls timed, the files' SHA-1 compared: old build, new build and the build with the comparisons agree).

  python rust\parity_nk2_file.py <old suite> <new suite> <opfor> <out.json> [level ...]

Since round NK5-rest (navbuild.py retired) this script runs against pinned checkouts only: both suites must be
checkouts that still have bfbc2-auto-navkit\tools\navbuild.py - the old one as above, the new one from that round's
own commit up to bda5b8d (the last with navbuild.py: `git worktree add <folder> bda5b8d`). Its hooks reach into
navbuild.py's Python stage, which the build no longer has; the current build's bytes are held to the same results by
tests\navbuild_golden.json and NavKit's regression (rust/BENCHMARKS.md "NK5-rest").

Levels: the 15 regression builds of work\navkit\regress (navregress's levels) when none are named.
"""
import ctypes
import ctypes.wintypes
import gc
import hashlib
import importlib.util
import json
import os
import pickle
import subprocess
import sys
import tempfile
import time
from typing import Any

LEVELS = ['ons_mp_002', 'ons_mp_004', 'sp_003_b', 'sp_004_b', 'sp_005', 'sp_005_b', 'sp_006', 'sp_007', 'sp_008',
          'sp_008_b', 'sp_009', 'sp_01', 'sp_02', 'sp_03', 'sp_04']


LAST_WITH_NAVBUILD = 'bda5b8d'      # the suite's last commit with navbuild.py (round NK5-rest retired it)


def need_navbuild(suite: str) -> None:
    """Refuses a suite checkout without navbuild.py: this script's hooks need its Python stage (a pinned checkout)."""
    if not os.path.isfile(os.path.join(suite, 'bfbc2-auto-navkit', 'tools', 'navbuild.py')):
        raise SystemExit(f'{suite} has no bfbc2-auto-navkit\\tools\\navbuild.py (retired in round NK5-rest): run '
                         f'this parity against a checkout at {LAST_WITH_NAVBUILD} or earlier')


class _Counters(ctypes.Structure):
    _fields_ = [('cb', ctypes.wintypes.DWORD), ('PageFaultCount', ctypes.wintypes.DWORD),
                ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t)]


def _dumps(obj):
    """A result line as JSON with its keys in a fixed order (the suite's determinism rule)."""
    return json.dumps(obj, sort_keys=True)


def memory():
    """(working set, peak working set) of this process, MB."""
    c = _Counters()
    c.cb = ctypes.sizeof(c)
    k32 = ctypes.WinDLL('kernel32')
    k32.GetCurrentProcess.restype = ctypes.wintypes.HANDLE
    psapi = ctypes.WinDLL('psapi')
    psapi.GetProcessMemoryInfo.argtypes = [ctypes.wintypes.HANDLE, ctypes.POINTER(_Counters), ctypes.wintypes.DWORD]
    psapi.GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb)
    return c.WorkingSetSize / 2 ** 20, c.PeakWorkingSetSize / 2 ** 20


def use_suite(suite):
    sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('core', 'navkit-tools')


def load_old(old):
    """The old navislands / navbuild modules from the old checkout, as navislands_old / navbuild_old (they share the
    unchanged kymesh / hpdtool / navsample / navwalk already imported)."""
    tools = os.path.join(old, 'bfbc2-auto-navkit', 'tools')
    mods: dict[str, Any] = {}
    for name in ('navislands', 'navbuild'):
        spec = importlib.util.spec_from_file_location(f'{name}_old', os.path.join(tools, f'{name}.py'))
        assert spec is not None and spec.loader is not None
        m = importlib.util.module_from_spec(spec)
        sys.modules[f'{name}_old'] = m
        if name == 'navbuild':
            m.__dict__['navislands'] = mods['navislands']
        spec.loader.exec_module(m)
        mods[name] = m
    mods['navbuild'].navislands = mods['navislands']
    return mods['navislands'], mods['navbuild']


def norm_log(lines, *paths):
    out = []
    for ln in lines:
        for p in paths:
            ln = ln.replace(p, '<out>')
        out.append(ln.rsplit(' (', 1)[0] if ln.startswith('wrote ') else ln)
    return out


def level_child(old, new, opfor, lv, scratch):
    """One level's build with the comparisons; prints a JSON line."""
    use_suite(new)
    import navbuild
    import navislands
    import navregress
    sys.path.insert(0, os.path.join(new, 'bfbc2-auto-navkit', 'tests'))
    import test_navbuild_native as golden_test
    old_isl, old_nb = load_old(old)
    res = {'level': lv, 'links': 0, 'links_differ': 0, 'link_s_old': 0.0, 'link_s_new': 0.0}
    scopes: dict = {}                    # id(NavLinkScope) -> the info it was made from
    real_scope = navbuild.link_scope

    def link_scope(info):
        t = time.perf_counter()
        got = real_scope(info)
        res['link_s_new'] += time.perf_counter() - t
        scopes[id(got)] = (got, info)
        return got
    navbuild.link_scope = link_scope
    acrosses: dict = {}                  # id(link_across's tuple) -> (the other stage, this pitch)
    real_across = navbuild.link_across

    def link_across(there, here_pitch):
        t = time.perf_counter()
        got = real_across(there, here_pitch)
        res['link_s_new'] += time.perf_counter() - t
        if got is not None:
            acrosses[id(got)] = (got, there, here_pitch)
        return got
    navbuild.link_across = link_across
    real_links = navbuild.sector_links

    def links(X, Y, inf, mesh_areas, level, same, across=None):
        t = time.perf_counter()
        got = real_links(X, Y, inf, mesh_areas, level, same, across)
        res['link_s_new'] += time.perf_counter() - t
        same_info = scopes[id(same)][1]
        info = dict(same_info)
        info[(X, Y)] = inf
        t = time.perf_counter()
        s = old_nb.same_grid_resolver(level, info, X, Y)
        if across is None:
            resolve = s
        else:
            _a, there, here_pitch = acrosses[id(across)]
            c = old_nb.cross_grid_resolver({'level': level, 'info': {(X, Y): inf}, 'pitch': here_pitch}, there, X, Y)

            def resolve(other, ar, bd, s=s, c=c):
                return s(other, ar, bd) if other in same_info else c(other, ar, bd)
        want = old_nb.sector_links(X, Y, inf['areas'], mesh_areas, resolve)
        res['link_s_old'] += time.perf_counter() - t
        res['links'] += 1
        if got != want:
            res['links_differ'] += 1
            res.setdefault('link_examples', []).append(f'{X},{Y}')
        return got
    navbuild.sector_links = links
    real_write = navbuild.write_file
    stage_in = os.path.join(scratch, f'{lv}.stage.pickle')

    def write(graphs, out_path, log, t0, cfg=navbuild.DEFAULTS, anchors=None):
        with open(stage_in, 'wb') as f:
            pickle.dump({'graphs': list(graphs), 'anchors': anchors, 'order_ids': set(navbuild.ORDER_IDS),
                         'terrain_ids': set(navislands.TERRAIN_IDS), 'dist_edge_max': cfg.dist_edge_max}, f,
                        protocol=pickle.HIGHEST_PROTOCOL)
        old_nb.ORDER_IDS.clear()
        old_nb.ORDER_IDS.update(navbuild.ORDER_IDS)
        old_isl.TERRAIN_IDS.clear()
        old_isl.TERRAIN_IDS.update(navislands.TERRAIN_IDS)
        old_out = out_path + '.old'
        old_log: list[str] = []
        new_log: list[str] = []
        t = time.perf_counter()
        g2 = old_isl.prune(graphs, anchors, old_log.append)
        old_nb.write_file(g2, old_out, old_log.append, t0, cfg)
        res['file_s_old'] = time.perf_counter() - t
        t = time.perf_counter()
        real_write(graphs, out_path, new_log.append, t0, cfg, anchors)
        res['file_s_new'] = time.perf_counter() - t
        a, b = open(old_out, 'rb').read(), open(out_path, 'rb').read()
        res['file_same'] = a == b
        res['log_same'] = norm_log(old_log, old_out) == norm_log(new_log, out_path)
        res['logs'] = norm_log(new_log, out_path)
        res['order_removed_same'] = old_nb.ORDER_STATS.get('removed', []) == [
            (i, tuple(p), n) for i, p, n in navbuild.ORDER_STATS.get('removed', [])]
        res['island_stats_same'] = old_isl.STATS == navislands.STATS
        for ln in new_log:
            log(ln)
    navbuild.write_file = write
    clock = time.localtime
    time.localtime = lambda *a: golden_test.FIXED_CLOCK
    out = os.path.join(scratch, f'{lv}.hpd')
    t = time.perf_counter()
    try:
        navregress.build_level(opfor, lv, out)
    finally:
        time.localtime = clock
    res['build_s'] = time.perf_counter() - t
    data = open(out, 'rb').read()
    res['sha1'] = hashlib.sha1(data).hexdigest()
    res['bytes'] = len(data)
    g = golden_test.load().get('levels', {}).get(lv)
    if g:
        inputs = {os.path.relpath(q, opfor).replace(os.sep, '/'): golden_test.file_sha1(q)
                  for q in golden_test.input_files(opfor, lv, navregress)}
        res['golden'] = ('inputs changed' if inputs != g['inputs'] else
                         'as recorded' if g['pathdata'] == res['sha1'] else 'DIFFERENT')
    reg_file = os.path.join(opfor, 'work', 'navkit', 'regress', f'{lv}.hpd')
    if os.path.isfile(reg_file):
        res['regress_file_same_size'] = os.path.getsize(reg_file) == len(data)
    for p in (out, out + '.old'):
        if os.path.isfile(p):
            os.remove(p)
    print(_dumps(res), flush=True)


def run_child(kind, suite, stage_in):
    """One stage alone: load the inputs, run the old or the new tail; prints a JSON line (seconds, MB)."""
    use_suite(suite)
    import navbuild
    import navislands
    with open(stage_in, 'rb') as f:
        d = pickle.load(f)
    navbuild.ORDER_IDS.clear()
    navbuild.ORDER_IDS.update(d['order_ids'])
    navislands.TERRAIN_IDS.clear()
    navislands.TERRAIN_IDS.update(d['terrain_ids'])
    cfg = navbuild.DEFAULTS
    sys.path.insert(0, os.path.join(suite, 'bfbc2-auto-navkit', 'tests'))
    import test_navbuild_native as golden_test
    time.localtime = lambda *a: golden_test.FIXED_CLOCK
    gc.collect()
    ws0, peak0 = memory()
    out = stage_in + f'.{kind}.hpd'
    t = time.perf_counter()
    if kind == 'old':
        g = navislands.prune(d['graphs'], d['anchors'], None)
        navbuild.write_file(g, out, lambda *a: None, time.time(), cfg)
    else:
        navbuild.write_file(d['graphs'], out, lambda *a: None, time.time(), cfg, d['anchors'])
    took = time.perf_counter() - t
    _ws1, peak1 = memory()
    data = open(out, 'rb').read()
    os.remove(out)
    print(_dumps({'s': took, 'loaded_mb': ws0, 'peak_mb': peak1, 'peak_before_mb': peak0,
                      'sha1': hashlib.sha1(data).hexdigest()}), flush=True)


def build_child(suite, opfor, lv, scratch):
    """One level's whole build by `suite` alone (the build clock fixed), its sector_links calls timed (the stage's
    resolvers in the old Python; NavLevel.sector_links and its scopes in the new); prints a JSON line."""
    use_suite(suite)
    import navbuild
    import navregress
    sys.path.insert(0, os.path.join(suite, 'bfbc2-auto-navkit', 'tests'))
    import test_navbuild_native as golden_test
    spent = [0.0]
    for name in ('sector_links', 'link_scope', 'link_across'):
        real = getattr(navbuild, name, None)
        if real is None:
            continue

        def timed(*a, real=real, **k):
            t = time.perf_counter()
            try:
                return real(*a, **k)
            finally:
                spent[0] += time.perf_counter() - t
        setattr(navbuild, name, timed)
    time.localtime = lambda *a: golden_test.FIXED_CLOCK
    out = os.path.join(scratch, f'{lv}.{os.path.basename(suite)}.hpd')
    t = time.perf_counter()
    navregress.build_level(opfor, lv, out)
    took = time.perf_counter() - t
    _ws, peak = memory()
    data = open(out, 'rb').read()
    os.remove(out)
    print(_dumps({'build_s': took, 'links_s': spent[0], 'peak_mb': peak, 'sha1': hashlib.sha1(data).hexdigest()}),
          flush=True)


def child_json(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    lines = [ln for ln in p.stdout.splitlines() if ln.startswith('{')]
    if p.returncode or not lines:
        return {'error': (p.stderr or p.stdout)[-2000:]}
    return json.loads(lines[-1])


def main():
    if sys.argv[1] == '--level':
        level_child(*sys.argv[2:7])
        return 0
    if sys.argv[1] == '--run':
        run_child(*sys.argv[2:5])
        return 0
    if sys.argv[1] == '--build':
        build_child(*sys.argv[2:6])
        return 0
    old, new, opfor, out_json = sys.argv[1:5]
    need_navbuild(old)
    need_navbuild(new)
    levels = sys.argv[5:] or LEVELS
    scratch = tempfile.mkdtemp(prefix='nk2_parity_')
    me = os.path.abspath(__file__)
    results = []
    for lv in levels:
        r = child_json([sys.executable, me, '--level', old, new, opfor, lv, scratch])
        r.setdefault('level', lv)
        stage_in = os.path.join(scratch, f'{lv}.stage.pickle')
        if os.path.isfile(stage_in):
            for kind, suite in (('old', old), ('new', new)):
                r[f'alone_{kind}'] = child_json([sys.executable, me, '--run', kind, suite, stage_in])
            r['stage_input_mb'] = os.path.getsize(stage_in) / 2 ** 20
            os.remove(stage_in)
        for kind, suite in (('old', old), ('new', new)):
            r[f'build_{kind}'] = child_json([sys.executable, me, '--build', suite, opfor, lv, scratch])
        r['builds_same'] = r['build_old'].get('sha1') is not None and r['build_old'].get('sha1') == r['build_new'].get('sha1') == r.get('sha1')
        results.append(r)
        print(_dumps({k: v for k, v in r.items() if k != 'logs'}), flush=True)
        with open(out_json, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(results, f, indent=1, sort_keys=True)
    os.rmdir(scratch)
    bad = [r['level'] for r in results if not (r.get('file_same') and r.get('log_same') and not r.get('links_differ')
                                               and r.get('order_removed_same') and r.get('island_stats_same')
                                               and r.get('builds_same'))]
    print(f'{len(results) - len(bad)} of {len(results)} levels identical' + (f'; differ: {bad}' if bad else ''))
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
