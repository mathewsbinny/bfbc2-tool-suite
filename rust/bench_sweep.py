r"""The E3m sweep (rust/BENCHMARKS.md, "E3m: the sweep"): the real workloads of the tools no earlier round profiled,
each timed and profiled on the user's game copy and Project OPFOR's work folders (read only; outputs go to a temporary
folder).

  python rust\bench_sweep.py <workload>... [--profile <folder>] [--top N]
  python rust\bench_sweep.py --list

  --profile  each workload under cProfile: <folder>\<workload>.prof and the N (default 14) functions with the most own
             time and cumulative time printed, with their call counts
  (no workload: --list)

Every workload prints its wall time; nothing is written into the game copy or the work folder.
"""
import cProfile
import os
import pstats
import shutil
import sys
import tempfile
import time
from collections.abc import Callable

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit', 'navkit-tools')
WORK: dict[str, Callable[[str, str], str]] = {}


def workload(fn: Callable[[str, str], str]) -> Callable[[str, str], str]:
    WORK[fn.__name__] = fn
    return fn


def opfor() -> str:
    from bfbc2core import paths
    return paths.find_opfor(None, start=SUITE)


# ------------------------------------------------------------------------------------------------ culltree

CULL_SOURCE = ('Levels/ONS_MP_005/staticculltree_exclusive.dbx', 'Levels/ONS_MP_005.dbx')


def _cull_inputs(root: str, tmp: str) -> tuple[str, str]:
    from bfbc2core import fbrb
    orig = os.path.join(root, 'game', 'Package', 'levels', 'ons_mp_005', 'level-00.fbrb.opfor-cull-orig')
    if os.path.isfile(orig):
        data = fbrb.read(orig, list(CULL_SOURCE))
        out = []
        for name in CULL_SOURCE:
            p = os.path.join(tmp, 'in', *name.split('/'))
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, 'wb') as fh:
                fh.write(data[name])
            out.append(p)
        return out[0], out[1]
    pkg = os.path.join(root, 'work', 'console_cull', 'ons_mp_005', 'pkg')
    tree, level = (os.path.join(pkg, *n.split('/')) for n in CULL_SOURCE)
    return tree, level


@workload
def culltree_add(root: str, tmp: str) -> str:
    """culltree add --auto --misplaced 2 on Onslaught MP_005 against every work\\levels folder (console_cull's run)."""
    suite_paths.use('culltree')
    import culltree
    tree, level = _cull_inputs(root, tmp)
    refs: list[str] = []
    levels = os.path.join(root, 'work', 'levels')
    for d in sorted(os.listdir(levels)):
        if os.path.isdir(os.path.join(levels, d)):
            refs += ['--ref-dir', os.path.join(levels, d)]
    out = culltree.Out()
    code = culltree.cmd_add([tree, level, '--auto', '--misplaced', '2.0', *refs, '--out-tree',
                             os.path.join(tmp, 'tree.dbx'), '--out-level', os.path.join(tmp, 'level.dbx')], out)
    return f'exit {code}, {len(out.lines)} report lines'


@workload
def culltree_verify(root: str, tmp: str) -> str:
    """culltree verify on Onslaught MP_005 against every work\\levels folder."""
    suite_paths.use('culltree')
    import culltree
    tree, level = _cull_inputs(root, tmp)
    refs: list[str] = []
    levels = os.path.join(root, 'work', 'levels')
    for d in sorted(os.listdir(levels)):
        if os.path.isdir(os.path.join(levels, d)):
            refs += ['--ref-dir', os.path.join(levels, d)]
    out = culltree.Out()
    code = culltree.cmd_verify([tree, level, *refs], out)
    return f'exit {code}, {len(out.lines)} report lines'


# ------------------------------------------------------------------------------------------------ navigation data

@workload
def navdata_load(root: str, tmp: str) -> str:
    """navdata.load (no cache) of DICE's path data of every prepared MP / SP level with one."""
    suite_paths.use('navigation-studio')
    import navparity
    from bfbc2core import navdata
    n = cells = 0
    for lv in sorted(os.listdir(os.path.join(root, 'work', 'levels'))):
        ref = navparity.reference_of(root, lv)
        if ref and os.path.isfile(ref):
            nav = navdata.load(ref)
            n += 1
            cells += len(nav.cell_area)
    return f'{n} files, {cells:,} cells'


@workload
def navpath_routes(root: str, tmp: str) -> str:
    """navpath.route: 400 seeded routes between DICE's ground nodes on sp_009 and sp_004_b."""
    import random

    import navparity
    from bfbc2core import navdata, navpath
    found = 0
    for lv in ('sp_009', 'sp_004_b'):
        nav = navdata.load(navparity.reference_of(root, lv))
        g = navpath.ground_nodes(nav)
        rng = random.Random(1)
        for _ in range(200):
            a, b = nav.node_pos[rng.randrange(len(g))], nav.node_pos[rng.randrange(len(g))]
            found += navpath.route(nav, a, b).found
    return f'{found} of 400 found'


# ------------------------------------------------------------------------------------------------ menu tools

@workload
def aptcheck_all(root: str, tmp: str) -> str:
    """aptcheck's rules on every movie of the game copy's movie sets (the menu tools' game test)."""
    suite_paths.use('menu-tools')
    import aptcheck
    import menucommon as mc
    from bfbc2core import fbrb
    found = flagged = 0
    for _key, rel in mc.MOVIE_SETS:
        layers = mc.bundle_layers(root, rel)
        if not layers:
            continue
        by_bundle: dict[str, list[str]] = {}
        for _name, (bundle, entry) in mc.bundle_movies(layers).items():
            by_bundle.setdefault(bundle, []).append(entry)
        for bundle, entries in by_bundle.items():
            for entry, data in fbrb.read(bundle, entries).items():
                try:
                    m = mc.movie_from_members(mc.parse_bigf(data, entry), entry)
                except mc.MenuToolError:
                    continue
                found += 1
                flagged += bool(aptcheck.Code(m).findings())
    return f'{found} movies, {flagged} flagged'


@workload
def aptrefs_xref(root: str, tmp: str) -> str:
    """aptrefs xref: every name across every movie set and the exe's table (written to the temp folder)."""
    suite_paths.use('menu-tools')
    import aptrefs
    code = aptrefs.main(['aptrefs.py', 'xref', '--out', os.path.join(tmp, 'x.txt'), '--json', os.path.join(tmp, 'x.json')])
    return f'exit {code}'


@workload
def aptgraft_roundtrip(root: str, tmp: str) -> str:
    """aptgraft roundtrip --pack of every movie of the main menu and MP HUD sets (read from the bundles)."""
    suite_paths.use('menu-tools')
    import aptgraft
    import menucommon as mc
    from bfbc2core import fbrb
    n = 0
    for key, rel in mc.MOVIE_SETS:
        if key not in ('menu', 'mp_hud', 'sp_hud'):
            continue
        layers = mc.bundle_layers(root, rel)
        for _name, (bundle, entry) in mc.bundle_movies(layers).items():
            data = fbrb.read(bundle, [entry])[entry]
            p = os.path.join(tmp, 'rt', os.path.basename(entry) + '.res')
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, 'wb') as fh:
                fh.write(data)
            n += 1
    code = aptgraft.main(['aptgraft.py', 'roundtrip', os.path.join(tmp, 'rt'), '--pack', '--quiet'])
    return f'{n} movies, exit {code}'


@workload
def bc2loc_dump(root: str, tmp: str) -> str:
    """bc2loc dump of the English and Russian texts with key names (written to the temp folder)."""
    suite_paths.use('menu-tools')
    import bc2loc
    for lang in ('english', 'russian'):
        bc2loc.main(['bc2loc.py', 'dump', '--lang', lang, '--out', os.path.join(tmp, f'loc_{lang}.txt')])
    return 'ok'


# ------------------------------------------------------------------------------------------------ logs

@workload
def harness_import_log(root: str, tmp: str) -> str:
    """The test harness's opfor.log reader (ingame.sessions) on the game copy's log."""
    suite_paths.use('harness')
    from testharness import ingame
    path = os.path.join(root, 'game', 'logs', 'opfor.log')
    with open(path, encoding='utf-8', errors='replace') as f:
        ss = ingame.sessions(f.read())
    return f'{os.path.getsize(path) / 1e6:.0f} MB, {len(ss)} sessions'


@workload
def crashreader_log(root: str, tmp: str) -> str:
    """The crash reader's log_before_crash on the game copy's log (the last crash, and one at a given time)."""
    suite_paths.use('crash-reader')
    import crashreader
    path = os.path.join(root, 'game', 'logs', 'opfor.log')
    a = crashreader.log_before_crash(path)
    b = crashreader.log_before_crash(path, when=os.path.getmtime(path))
    return f'{len(a)} + {len(b)} lines'


@workload
def harness_run(root: str, tmp: str) -> str:
    """harness run on sp_009 and sp_004_b, every file, every test but parity (the runs and the cache written to the temp folder)."""
    suite_paths.use('harness')
    import harness
    cache = os.path.join(tmp, 'cache')

    def cache_dir(_root: str) -> str:            # the harness keeps its cache in work\harness: here the temp folder
        os.makedirs(cache, exist_ok=True)
        return cache
    harness.cache_dir = cache_dir
    code = harness.main(['harness.py', 'run', 'sweep', '--levels', 'sp_009,sp_004_b',
                         '--tests', 'structure,reachability,battery,orders,offsets,level', '--runs', os.path.join(tmp, 'runs')])
    return f'exit {code}'


# ------------------------------------------------------------------------------------------------ console converter

@workload
def havok_verify(root: str, tmp: str) -> str:
    """convert.py verify-havok on the user's Onslaught console folders (the converter's physics test)."""
    suite_paths.use('console-convert')
    sys.argv = ['test_havok.py']
    import runpy
    try:
        runpy.run_path(os.path.join(SUITE, 'bfbc2-console-convert', 'tests', 'test_havok.py'), run_name='__main__')
    except SystemExit as e:
        return f'exit {e.code}'
    return 'ok'


@workload
def texqa_ww2(root: str, tmp: str) -> str:
    """texqa over the WW2 vehicles' textures (work\\ww2\\bf1943_vehicles), cold (an empty cache folder)."""
    suite_paths.use('console-convert', 'bf1943-import')
    import texqa
    folder = os.path.join(root, 'work', 'ww2', 'bf1943_vehicles')
    code = texqa.main([folder, '-o', os.path.join(tmp, 'out'), '--cache', os.path.join(tmp, 'cache'), '--worst', '4'])
    return f'exit {code}'


# ------------------------------------------------------------------------------------------------ main

def summary(prof: str, top: int) -> str:
    st = pstats.Stats(prof)
    stats = st.stats  # type: ignore[attr-defined]
    rows = [(tt, ct, nc, f'{os.path.basename(f)}:{line}:{fn}') for (f, line, fn), (_, nc, tt, ct, _) in stats.items()]
    out = ['  own time  cumul.      calls  function']
    out += [f'  {tt:8.2f} {ct:8.2f} {nc:>10} {w}' for tt, ct, nc, w in sorted(rows, reverse=True)[:top]]
    return '\n'.join(out)


def main(argv: list[str]) -> int:
    args = argv[1:]
    prof_dir, top = None, 14
    if '--profile' in args:
        i = args.index('--profile')
        prof_dir = args[i + 1]
        del args[i:i + 2]
    if '--top' in args:
        i = args.index('--top')
        top = int(args[i + 1])
        del args[i:i + 2]
    if not args or '--list' in args:
        for name, fn in WORK.items():
            print(f'{name:22} {(fn.__doc__ or "").splitlines()[0]}')
        return 0
    root = opfor()
    for name in args:
        tmp = tempfile.mkdtemp(prefix=f'sweep_{name}_')
        try:
            pr = cProfile.Profile() if prof_dir else None
            t = time.perf_counter()
            if pr:
                pr.enable()
            what = WORK[name](root, tmp)
            if pr:
                pr.disable()
            wall = time.perf_counter() - t
            print(f'{name}: {wall:.1f} s ({what})', flush=True)
            if pr and prof_dir:
                os.makedirs(prof_dir, exist_ok=True)
                p = os.path.join(prof_dir, name + '.prof')
                pr.dump_stats(p)
                print(summary(p, top), flush=True)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
