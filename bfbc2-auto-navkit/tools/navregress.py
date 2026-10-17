r"""NavKit regression test: rebuilds levels with the current code and checks that no DICE-parity measure got
worse than a saved reference run.

  navregress.py [--levels sp_004_b,sp_009] [--reference <run>] [--save <run>] [--tolerance 0.05] [--opfor <folder>]

Per level: build with the same inputs as `navkit.py build` (in-game samples, ForbidPathfinding, dynamic-object
and stair triangles, the high-capsule samples, soldier spawns as seeds; the level's SectorOffsets), into
OPFOR's work\navkit\regress\<level>.hpd; then navparity's measures (navparity.py) against DICE's file.

Each measure is compared with the reference run (default "regression"; navparity.py's work\navkit\parity\
<run>.json). Measures where higher is better must not drop, measures where lower is better must not rise, by
more than the tolerance (points); counts are listed when they change but do not fail. Exit code 0: nothing
worse; 1: something worse (or a level failed to build); 2: no reference run.

The run is saved as "regression_last". After an intended improvement, make it the reference:
  navregress.py --save regression          (or: copy regression_last.json to regression.json)
Default levels: the 13 campaign levels (about 45 minutes); --levels for a subset.
"""
import glob
import json
import os
import re
import sys
import functools
import time

print = functools.partial(print, flush=True)   # progress shows at once when the output goes to a file

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import navbuild  # noqa: E402
import navparity  # noqa: E402

HIGHER = ['ground', 'areas_same', 'portals', 'outline', 'borders', 'costs', 'nodes_same', 'nodes_near', 'edges_same',
          'connectivity', 'ai_coverage', 'ai_pairwise']
LOWER = ['extra_ground', 'over_connection', 'sectors_missing']
COUNTS = ['sectors_candidate', 'areas_candidate', 'detailed_candidate', 'islands_candidate', 'large_islands_candidate',
          'ai_main_candidate']


def inputs(root, lv):
    """The build inputs of a level, as navkit.py build uses them; raises if something is missing."""
    L = os.path.join(root, 'work', 'levels', lv)
    samples = None
    for name in (f'{lv}_navkit', f'{lv}_full'):
        p = os.path.join(root, 'work', 'navsample', name + '.bin')
        if os.path.isfile(p) and os.path.isfile(p[:-4] + '_hi.bin'):
            samples = p
            break
    if samples is None:
        raise RuntimeError(f'{lv}: no in-game samples (work\\navsample\\{lv}_navkit.bin or _full.bin, with _hi)')
    settings = os.path.join(root, 'work', 'navkit', lv, 'navkit.json')
    if os.path.isfile(settings):
        s = json.load(open(settings))
        xoff, zoff = s['xoff'], s['zoff']
    else:
        so = glob.glob(os.path.join(L, 'dist', 'levels', lv, 'Precomputed', 'Infantry', 'SectorOffsets.xml'))
        if not so:
            raise RuntimeError(f'{lv}: no navkit.json and no SectorOffsets.xml (run navkit.py prepare {lv})')
        t = open(so[0], encoding='utf-8').read()
        xoff = int(re.search(r'name="XOffset">(-?\d+)<', t).group(1))
        zoff = int(re.search(r'name="ZOffset">(-?\d+)<', t).group(1))
    files = {k: os.path.join(L, k) for k in ('forbid_tris.npy', 'dynamic_tris.npy', 'stair_tris.npy', 'soldier_spawns.json')}
    missing = [k for k, p in files.items() if not os.path.isfile(p)]
    if missing:
        raise RuntimeError(f'{lv}: {", ".join(missing)} missing (run navkit.py prepare {lv})')
    return samples, xoff, zoff, files


def build_level(root, lv, out, log):
    samples, xoff, zoff, f = inputs(root, lv)
    navbuild.build(samples, f['forbid_tris.npy'], xoff, zoff, out, dynamic_path=f['dynamic_tris.npy'],
                   high_path=samples[:-4] + '_hi.bin', stairs_path=f['stair_tris.npy'],
                   seeds=json.load(open(f['soldier_spawns.json'])), log=lambda *a: None)


def compare(ref, now, tol):
    """(worse, better, count changes) for one level: lists of (measure, reference, now)."""
    worse, better, counts = [], [], []
    for k in HIGHER + LOWER:
        a, b = ref.get(k), now.get(k)
        if a is None or b is None:
            continue
        d = (b - a) if k in HIGHER else (a - b)
        if d < -tol:
            worse.append((k, a, b))
        elif d > tol:
            better.append((k, a, b))
    for k in COUNTS:
        if ref.get(k) != now.get(k):
            counts.append((k, ref.get(k), now.get(k)))
    return worse, better, counts


def main(argv):
    args = argv[1:]
    opt = lambda k, d=None: args[args.index(k) + 1] if k in args else d
    root = navparity.opfor_root(opt('--opfor'))
    levels = opt('--levels').split(',') if opt('--levels') else navparity.LEVELS
    reference = opt('--reference', 'regression')
    tol = float(opt('--tolerance', 0.05))
    ref_path = navparity.save_path(root, reference)
    ref_rows = {}
    if os.path.isfile(ref_path):
        ref_rows = {r['level']: r for r in json.load(open(ref_path))['levels']}
    elif '--save' not in args:
        print(f'no reference run "{reference}" ({ref_path}); make one with: navregress.py --save {reference}')
        return 2
    outdir = os.path.join(root, 'work', 'navkit', 'regress')
    os.makedirs(outdir, exist_ok=True)
    rows, failed = [], 0
    print(f'NavKit regression: {len(levels)} level(s), reference "{reference}", tolerance {tol} points')
    for lv in levels:
        t0 = time.time()
        out = os.path.join(outdir, f'{lv}.hpd')
        try:
            build_level(root, lv, out, print)
            row = navparity.run_level(root, lv, out, lambda *a: None)
        except Exception as e:
            failed += 1
            print(f'FAIL {lv}: {e}')
            continue
        rows.append(row)
        ref = ref_rows.get(lv)
        if ref is None:
            print(f'new  {lv}: not in the reference run ({time.time() - t0:.0f} s)')
            continue
        worse, better, counts = compare(ref, row, tol)
        failed += bool(worse)
        print(f'{"FAIL" if worse else "ok  "} {lv:9s} ground {row["ground"]:6.2f}  extra {row["extra_ground"]:6.2f}  '
              f'nodes {row["nodes_same"]:5.2f}  connect {row["connectivity"]:6.2f}  over {row["over_connection"]:6.2f}  '
              f'AI {row["ai_coverage"]:5.1f}  ({time.time() - t0:.0f} s)')
        for k, a, b in worse:
            print(f'       worse:  {k} {a} -> {b}')
        for k, a, b in better:
            print(f'       better: {k} {a} -> {b}')
        for k, a, b in counts:
            print(f'       count:  {k} {a} -> {b}')
    run = {'name': 'regression_last', 'date': time.strftime('%Y-%m-%d %H:%M'), 'levels': rows}
    json.dump(run, open(navparity.save_path(root, 'regression_last'), 'w'), indent=1)
    if '--save' in args:
        name = opt('--save')
        run['name'] = name
        if name == reference and ref_rows:                 # a subset run keeps the other levels' reference rows
            done = {r['level'] for r in rows}
            run['levels'] = rows + [r for lv, r in ref_rows.items() if lv not in done]
        json.dump(run, open(navparity.save_path(root, name), 'w'), indent=1)
        print(f'saved as reference run "{name}"')
    print('all passed' if not failed else f'{failed} level(s) worse or failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'core'))
    from bfbc2core import errors
    errors.setup('navregress', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
