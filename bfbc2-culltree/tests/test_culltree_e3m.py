r"""Round E3m's culltree changes against their reference (rust/BENCHMARKS.md "E3m"): ref_rows (reference levels' rows
kept on disk) against ref_pairs, and shape_for's memo, both ways through the command line:

- quick: built trees and levels (test_culltree.py's builders: a target and three reference levels, one without a
  tree, one with a damaged tree), add --auto --misplaced and verify: the same files and report as
  BFBC2_CULLTREE_REFERENCE=1, with no cache, a cold cache and a warm one; a reference level changed after the
  cache was made is read again;
- --game: console_cull's add --auto --misplaced 2 on Onslaught MP_005 against every work\levels folder and verify on
  it (the game copy and work folders only read; outputs and the cache in a temporary folder), reference / cold /
  warm, timed.

  python test_culltree_e3m.py [--game]
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SUITE = os.path.dirname(ROOT)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('culltree', 'core')
CULL_SOURCE = ('Levels/ONS_MP_005/staticculltree_exclusive.dbx', 'Levels/ONS_MP_005.dbx')
failed = 0


def check(ok: object, what: str, detail: str = '') -> None:
    global failed
    failed += not ok
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


def run(cmd: str, tree: str, level: str, refs: list[str], out: str, env: dict[str, str]) -> tuple[int, str, list]:
    """One culltree command; (exit code, report with the output folder masked, the written files' bytes)."""
    os.makedirs(out, exist_ok=True)
    from bfbc2core import native  # round RT3: culltree.py retired, bfbc2.exe's command line
    args = [*native.cli_command('culltree'), cmd, tree, level, *refs]
    if cmd == 'add':
        args += ['--auto', '--misplaced', '2.0', '--out-tree', os.path.join(out, 'tree.dbx'),
                 '--out-level', os.path.join(out, 'level.dbx')]
    r = subprocess.run(args, capture_output=True, text=True, errors='replace', env={**os.environ, **env})
    files = [open(os.path.join(out, f), 'rb').read() if os.path.isfile(os.path.join(out, f)) else None
             for f in ('tree.dbx', 'level.dbx')]
    return r.returncode, (r.stdout + r.stderr).replace(out, '<out>'), files


def three_ways(name: str, cmd: str, tree: str, level: str, refs: list[str], tmp: str) -> None:
    cache = os.path.join(tmp, f'cache_{cmd}')
    ways = [('reference', {'BFBC2_CULLTREE_REFERENCE': '1', 'BFBC2_CULLTREE_CACHE': '0'}),
            ('no cache', {'BFBC2_CULLTREE_REFERENCE': '', 'BFBC2_CULLTREE_CACHE': '0'}),
            ('cold', {'BFBC2_CULLTREE_REFERENCE': '', 'BFBC2_CULLTREE_CACHE': cache}),
            ('warm', {'BFBC2_CULLTREE_REFERENCE': '', 'BFBC2_CULLTREE_CACHE': cache})]
    got, times = [], []
    for way, env in ways:
        t = time.perf_counter()
        got.append(run(cmd, tree, level, refs, os.path.join(tmp, f'{cmd}_{way.replace(" ", "_")}'), env))
        times.append(time.perf_counter() - t)
    lines = got[0][1].strip().splitlines()
    alike = all(g == got[0] for g in got[1:])
    check(alike and got[0][1], f'{name}: {cmd} - the same exit code ({got[0][0]}), report ({len(lines)} lines) and '
          f'files with ref_rows and the memo, no cache / cold / warm, as the reference; '
          + ', '.join(f'{w} {t:.1f} s' for (w, _), t in zip(ways, times)),
          next((f'{w}: {g[1][-600:]}' for (w, _), g in zip(ways, got) if g != got[0]), ''))


# ------------------------------------------------------------------------------------------------ quick

def build_levels(top: str) -> tuple[str, str, str, Any]:
    """test_culltree's target (huts and rocks slotted, a hut and a lamp at slot 0, a hut far from its shape) and a
    reference folder: lamps and huts with their tree, a level whose tree is missing, one whose tree is damaged.
    Returns (tree, level, reference folder, a function that moves the reference lamps)."""
    sys.path.insert(0, HERE)
    import test_culltree as tc

    def write(p: str, data: bytes) -> None:
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, 'wb') as f:
            f.write(data)
    objs = [('00000000-0000-0000-0000-000000000001', 'objects/test/hut/hut', tc.rot_y(0, (10, 5, 20)), 1),
            ('00000000-0000-0000-0000-000000000002', 'objects/test/hut/hut', tc.rot_y(90, (-40, 6, 60)), 1),
            ('00000000-0000-0000-0000-000000000003', 'objects/test/rock/rock', tc.rot_y(30, (5, 1, -70)), 1),
            ('00000000-0000-0000-0000-000000000004', 'objects/test/hut/hut', tc.rot_y(180, (80, 4, -10)), 1),
            ('00000000-0000-0000-0000-000000000005', 'objects/test/rock/rock', tc.rot_y(10, (-60, 2, -30)), 1),
            ('00000000-0000-0000-0000-000000000008', 'objects/test/hut/hut', tc.rot_y(45, (-80, 3, 10)), 1),
            ('00000000-0000-0000-0000-000000000006', 'objects/test/hut/hut', tc.rot_y(237, (30, 7, 90)), 0),
            ('00000000-0000-0000-0000-000000000007', 'objects/test/lamp/lamp', tc.rot_y(45, (35, 7, 95)), 0)]
    nodes, placed = tc.make_tree(objs, 100)
    # huts 1 and 8 swap slots: each one's leaves lie at the other's place (misplaced finds them)
    slot = {p[0]: p[3] for p in placed}
    swap = {'00000000-0000-0000-0000-000000000001': slot['00000000-0000-0000-0000-000000000008'],
            '00000000-0000-0000-0000-000000000008': slot['00000000-0000-0000-0000-000000000001']}
    placed = [(g, bp, m, swap.get(g, en)) for g, bp, m, en in placed]
    tree = os.path.join(top, 'target', 'levels', 'test', 'staticculltree_exclusive.dbx')
    level = os.path.join(top, 'target', 'levels', 'test.dbx')
    write(tree, tc.tree_file(nodes, 100))
    write(level, tc.level_file(placed))
    ref = os.path.join(top, 'ref')

    def refs(shift: float) -> None:
        for name, bp, n in (('lamps', 'objects/test/lamp/lamp', 4), ('huts', 'objects/test/hut/hut', 3)):
            robjs = [(f'1000000{len(name)}-0000-0000-0000-00000000000{i}', bp,
                      tc.rot_y(20 * i, (i * 7 + shift, 3, -i * 5)), 1) for i in range(1, n + 1)]
            rnodes, rplaced = tc.make_tree(robjs, 40)
            tref = f'levels/{name}/staticculltree_exclusive/11111111-0000-0000-0000-000000000001'
            write(os.path.join(ref, 'levels', name, 'staticculltree_exclusive.dbx'), tc.tree_file(rnodes, 40))
            write(os.path.join(ref, 'levels', f'{name}.dbx'), tc.level_file(rplaced, tree_ref=tref))
    refs(0.0)
    lamp = [('20000000-0000-0000-0000-000000000001', 'objects/test/lamp/lamp', tc.rot_y(0, (1, 2, 3)), 1)]
    rnodes, rplaced = tc.make_tree(lamp, 7)
    write(os.path.join(ref, 'levels', 'notree.dbx'),
          tc.level_file(rplaced, tree_ref='levels/notree/staticculltree_exclusive/11111111-0000-0000-0000-000000000001'))
    bad = list(rnodes)
    bad[0] = (bad[0][0] + 1,) + bad[0][1:]
    write(os.path.join(ref, 'levels', 'damaged', 'staticculltree_exclusive.dbx'), tc.tree_file(bad, 7))
    write(os.path.join(ref, 'levels', 'damaged.dbx'),
          tc.level_file(rplaced, tree_ref='levels/damaged/staticculltree_exclusive/11111111-0000-0000-0000-000000000001'))
    write(os.path.join(ref, 'levels', 'other.dbx'), b'{binary}' + bytes(40))      # not a level partition
    return tree, level, ref, lambda: refs(0.75)


def quick(tmp: str) -> None:
    tree, level, ref_dir, changed = build_levels(os.path.join(tmp, 'built'))
    for cmd in ('add', 'verify'):
        three_ways('built levels', cmd, tree, level, ['--ref-dir', ref_dir], tmp)
    # a reference changed after the cache was made: read again, the same as the reference
    changed()
    three_ways('built levels, a reference level changed', 'add', tree, level, ['--ref-dir', ref_dir],
               os.path.join(tmp, 'changed'))


# ------------------------------------------------------------------------------------------------ game

def game(tmp: str) -> None:
    from bfbc2core import fbrb, paths
    root = paths.find_opfor(None, start=SUITE)
    levels = os.path.join(root, 'work', 'levels')
    orig = os.path.join(root, 'game', 'Package', 'levels', 'ons_mp_005', 'level-00.fbrb.opfor-cull-orig')
    if not (os.path.isfile(orig) and os.path.isdir(levels)):
        print('skipped: Onslaught MP_005\'s bundle before the cull stage or work\\levels not found')
        return
    data = fbrb.read(orig, list(CULL_SOURCE))
    files = []
    for name in CULL_SOURCE:
        p = os.path.join(tmp, 'in', *name.split('/'))
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, 'wb') as fh:
            fh.write(data[name])
        files.append(p)
    refs: list[str] = []
    for d in sorted(os.listdir(levels)):
        if os.path.isdir(os.path.join(levels, d)):
            refs += ['--ref-dir', os.path.join(levels, d)]
    for cmd in ('add', 'verify'):
        three_ways('Onslaught MP_005 against every work\\levels folder', cmd, files[0], files[1], refs, tmp)


def main(argv: list[str]) -> int:
    tmp = tempfile.mkdtemp(prefix='culltree_e3m_')
    try:
        quick(tmp)
        if '--game' in argv:
            game(tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
