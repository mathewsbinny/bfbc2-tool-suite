r"""culltree.py's Rust core (rust/bc2-formats culltree, through the bc2_native module) against its Python code: Doc's
element reader (the same elements, trailer and bytes written; a file the Rust reader refuses is read, and refused,
by Python), and ref_pairs' level search (level_partitions: the folder listing glob gives, the files that hold
ExclusiveStaticCullTree).

  python test_culltree_native.py          quick: built trees and levels, damaged and random streams, a folder with
                                          hidden files, links and odd names, the dispatcher; 2 trees of OPFOR's work
                                          folders when they are there
  python test_culltree_native.py --game   + every cull tree and level partition of the game copy's level bundles (read
                                          only) and of OPFOR's work\levels / work\ww2 / work\console_cull: both
                                          readers, the same CullTree / Level, written back as they were; the listing of
                                          every work\levels folder against glob; and `add --auto --misplaced 2 --ref-dir
                                          <every work\levels folder>` on Onslaught's MP_005 (console_cull's run), both
                                          backends into a temp folder: the same tree, level and report

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import glob
import os
import random
import struct
import subprocess
import sys
import tempfile
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('culltree', 'core')
sys.path.insert(0, HERE)
import culltree as ct  # noqa: E402
import test_culltree as tc  # noqa: E402
from bfbc2core import fbrb, paths  # noqa: E402

NATIVE: Any = ct._import_native()


# ------------------------------------------------------------------------------------------------ both backends

def flat(doc: ct.Doc) -> list[tuple[Any, ...]]:
    """Every element as (depth, tag, head, attrs, extra, value, container) in file order."""
    out = []
    st = [(e, 0) for e in reversed(doc.roots)]
    while st:
        e, d = st.pop()
        assert e.doc is doc
        out.append((d, e.tag, e.head, e.attrs, e.extra, e.value, e.children is not None))
        st += [(c, d + 1) for c in reversed(e.children or [])]
    return out


def outcome(data: bytes, native: bool) -> tuple[str, Any]:
    ct.use_native(native)
    try:
        d = ct.Doc(data, 'f')
    except Exception as e:  # noqa: BLE001 - every exception is compared
        return type(e).__name__, str(e)
    return 'ok', (d.head12, d.string_area, d.strings, flat(d), d.trailer, d.tobytes())


def compare(data: bytes, roundtrip: bool = True) -> tuple[str | None, str]:
    py, rs = outcome(data, False), outcome(data, True)
    ct.use_native(True)
    if py != rs:
        return (f'python {py[0]}, rust {rs[0]}' if py[0] != rs[0] else
                'messages differ' if py[0] != 'ok' else 'elements differ'), rs[0]
    if roundtrip and py[0] == 'ok' and py[1][5] != data:
        return 'written bytes are not the file\'s', 'ok'
    return None, rs[0]


def mutations(data: bytes, rng: random.Random, n: int) -> list[bytes]:
    out = [data[:i] for i in sorted({len(data) * k // 50 for k in range(50)} | set(range(20, min(len(data), 90))))]
    for _ in range(n):
        d = bytearray(data)
        for _ in range(rng.randint(1, 3)):
            i = rng.randrange(24, len(d)) if rng.random() < 0.9 else rng.randrange(len(d))
            op = rng.randrange(4)
            if op == 0:
                d[i] = rng.randrange(256)
            elif op == 1:
                d[i:i + 3] = (b'\xff' if rng.random() < 0.5 else b'\0') * 3
            elif op == 2:
                del d[i:i + rng.randint(1, 6)]
            else:
                d[i:i] = bytes([0x80 | rng.randrange(128)] * rng.randint(8, 12)) + b'\x01'   # a long varint
        out.append(bytes(d))
    return out


def tree_state(path: str) -> tuple[Any, ...]:
    t = ct.CullTree(path)
    return (t.enum_offset, t.nodes, t.used, t.spare, sorted(t.leaf_node), t.depth_and_stack())


def level_state(path: str) -> tuple[Any, ...]:
    lv = ct.Level(path)
    return (lv.tree_refs, [(o.guid, o.blueprint, o.m, o.enum) for o in lv.objects])


def _compare_file(item: tuple[str, str, str]) -> tuple[str, int, str | None, str]:
    """(label, bytes, difference, kind) for a tree or level partition: a file, or a bundle entry (copied to a temp
    file for CullTree / Level, which take paths)."""
    kind, path, entry = item
    label = path + (f':{entry}' if entry else '')
    data = fbrb.read(path, [entry])[entry] if entry else open(path, 'rb').read()
    if not data.startswith(ct.MAGIC):
        return label, len(data), None, 'text'
    diff, k = compare(data)
    if diff or k != 'ok':
        return label, len(data), diff, k
    with tempfile.TemporaryDirectory(prefix='ct_native_') as tmp:
        p = os.path.join(tmp, 'f.dbx')
        with open(p, 'wb') as fh:
            fh.write(data)
        got: list[tuple[str, Any]] = []
        for on in (False, True):
            ct.use_native(on)
            try:
                if kind == 'tree':
                    st = tree_state(p)
                    t = ct.CullTree(p)
                    t.write(os.path.join(tmp, 'w.dbx'))
                    st += (open(os.path.join(tmp, 'w.dbx'), 'rb').read() == data,)
                else:
                    st = level_state(p)
                got.append(('ok', st))
            except Exception as e:  # noqa: BLE001
                got.append((type(e).__name__, str(e)))
        ct.use_native(True)
    if got[0] != got[1]:
        return label, len(data), f'{kind}s differ ({got[0][0]} / {got[1][0]})', 'ok'
    if got[0][0] == 'ok' and kind == 'tree' and not got[0][1][-1]:
        return label, len(data), 'tree written back with other bytes', 'ok'
    return label, len(data), None, got[0][0] if got[0][0] != 'ok' else 'ok'


# ------------------------------------------------------------------------------------------------ the corpus

def corpus(root: str) -> list[tuple[str, str, str]]:
    items = []
    levels = os.path.join(root, 'game', 'Package', 'levels'), os.path.join(root, 'game', 'Dist', 'win32', 'levels')
    for top in levels:
        if not os.path.isdir(top):
            continue
        for lv in sorted(os.listdir(top)):
            b = os.path.join(top, lv, 'level-00.fbrb')
            if not os.path.isfile(b):
                continue
            for e in fbrb.entries(b):
                low = e.name.lower()
                if e.deleted or not low.endswith('.dbx'):
                    continue
                if 'staticculltree' in low:
                    items.append(('tree', b, e.name))
                elif low.count('/') == 1 and low.startswith('levels/'):
                    items.append(('level', b, e.name))
    for top in ('levels', 'ww2', 'console_cull'):
        for d, _, names in os.walk(os.path.join(root, 'work', top)):
            for n in names:
                low = n.lower()
                if low.endswith('.dbx') and 'staticculltree' in low:
                    items.append(('tree', os.path.join(d, n), ''))
                elif low.endswith('.dbx') and os.path.basename(d).lower() == 'levels':
                    items.append(('level', os.path.join(d, n), ''))
    return items


def run_corpus(root: str, check: Any) -> None:
    t = time.time()
    items = corpus(root)
    diffs: Counter[str] = Counter()
    examples: dict[str, str] = {}
    kinds: Counter[tuple[str, str]] = Counter()
    nbytes = 0
    workers = max(1, min(6, (os.cpu_count() or 2) - 1))
    with ProcessPoolExecutor(workers) as pool:
        for item, (label, size, diff, kind) in zip(items, pool.map(_compare_file, items, chunksize=4)):
            kinds[(item[0], kind)] += 1
            nbytes += size
            if diff:
                diffs[diff] += 1
                examples.setdefault(diff, label)
    for k, n in diffs.items():
        print(f'       {n} x {k}, e.g. {examples[k]}')
    trees = sum(n for (k, _), n in kinds.items() if k == 'tree')
    check(not diffs and kinds[('tree', 'ok')] > 0,
          f'{trees} cull trees and {len(items) - trees} level partitions (the game copy\'s level bundles, work\\levels, '
          f'work\\ww2, work\\console_cull; {nbytes / 1e6:.0f} MB): the same elements, trailers and bytes; the same '
          f'CullTree (nodes, leaves, spare, depth) and Level (objects, tree references); trees written back as they '
          f'were ({", ".join(f"{a} {b}: {n}" for (a, b), n in sorted(kinds.items()))})  ({time.time() - t:.0f} s)')


def run_listing(root: str, check: Any) -> None:
    levels = os.path.join(root, 'work', 'levels')
    dirs = sorted(os.path.join(levels, d) for d in os.listdir(levels) if os.path.isdir(os.path.join(levels, d)))
    t = time.time()
    bad, n, found = [], 0, 0
    for d in dirs:
        g = sorted(glob.glob(os.path.join(d, '**', '*.dbx'), recursive=True))
        r = sorted(os.path.join(d, x) for x in NATIVE.culltree_dbx_files(d))
        n += len(g)
        if g != r:
            bad.append(f'{d}: glob {len(g)}, rust {len(r)}')
    t1 = time.time()
    sample = dirs[:3]
    for d in sample:
        ct.use_native(False)
        a = ct.level_partitions(d)
        ct.use_native(True)
        b = ct.level_partitions(d)
        found += len(b)
        if a != b:
            bad.append(f'{d}: level partitions differ ({len(a)} / {len(b)})')
    check(not bad, f'{len(dirs)} folders of work\\levels ({n:,} .dbx): the Rust listing is glob\'s ({t1 - t:.0f} s); '
          f'level_partitions in both backends on {len(sample)} of them: the same {found} partitions', '; '.join(bad[:3]))


CULL_SOURCE = ('Levels/ONS_MP_005/staticculltree_exclusive.dbx', 'Levels/ONS_MP_005.dbx')   # console_cull's


def run_add(root: str, check: Any) -> None:
    """console_cull's add on Onslaught MP_005, both backends. The input: the level bundle as it was before
    console_cull's stage (the game copy's level-00.fbrb.opfor-cull-orig, read only; its tree and level partition
    written to the temp folder), else console_cull's own unpack in work (already patched: one object left to add)."""
    levels = os.path.join(root, 'work', 'levels')
    orig = os.path.join(root, 'game', 'Package', 'levels', 'ons_mp_005', 'level-00.fbrb.opfor-cull-orig')
    pkg = os.path.join(root, 'work', 'console_cull', 'ons_mp_005', 'pkg')
    with tempfile.TemporaryDirectory(prefix='culltree_add_') as tmp:
        if os.path.isfile(orig):
            source = 'the bundle before the cull stage (fresh)'
            data = fbrb.read(orig, list(CULL_SOURCE))
            for name in CULL_SOURCE:
                p = os.path.join(tmp, 'in', *name.split('/'))
                os.makedirs(os.path.dirname(p), exist_ok=True)
                with open(p, 'wb') as fh:
                    fh.write(data[name])
            tree, level = (os.path.join(tmp, 'in', *n.split('/')) for n in CULL_SOURCE)
        else:
            source = 'work\\console_cull\'s unpack (already patched)'
            tree, level = (os.path.join(pkg, *n.split('/')) for n in CULL_SOURCE)
        if not (os.path.isfile(tree) and os.path.isfile(level) and os.path.isdir(levels)):
            print('skipped: the add run (no Onslaught MP_005 source or work\\levels)')
            return
        run_add_on(tree, level, levels, source, tmp, check)


def run_add_on(tree: str, level: str, levels: str, source: str, tmp: str, check: Any) -> None:
    refs = []
    for d in sorted(os.listdir(levels)):
        if os.path.isdir(os.path.join(levels, d)):
            refs += ['--ref-dir', os.path.join(levels, d)]
    out, times = [], []
    # both sides read every reference level (BFBC2_CULLTREE_CACHE=0: no rows from round E3m's cache)
    for side, env in (('python', {'BFBC2_PURE_PYTHON': '1', 'BFBC2_CULLTREE_CACHE': '0'}),
                      ('rust', {'BFBC2_PURE_PYTHON': '', 'BFBC2_CULLTREE_CACHE': '0'})):
        o = os.path.join(tmp, side)
        os.makedirs(o)
        t = time.time()
        r = subprocess.run([sys.executable, os.path.join(ROOT, 'culltree.py'), 'add', tree, level, '--auto',
                            '--misplaced', '2.0', *refs, '--out-tree', os.path.join(o, 'tree.dbx'),
                            '--out-level', os.path.join(o, 'level.dbx')],
                           capture_output=True, text=True, errors='replace', env={**os.environ, **env})
        times.append(time.time() - t)
        files = [open(os.path.join(o, f), 'rb').read() if os.path.isfile(os.path.join(o, f)) else None
                 for f in ('tree.dbx', 'level.dbx')]
        out.append((r.returncode, r.stdout.replace(o, '<out>'), files))     # the report names its output files
    lines = out[0][1].strip().splitlines()
    added = sum(1 for x in lines if ': slot ' in x and ' leaves (shape from ' in x)
    check(out[0] == out[1] and out[0][0] == 0 and out[0][2][0] is not None,
          f'add --auto --misplaced 2 on Onslaught MP_005 ({source}) against every work\\levels folder: the same tree, '
          f'level and report both ways ({added} objects added, {len(lines)} report lines, "{lines[-1] if lines else ""}"; '
          f'Python {times[0]:.0f} s, '
          f'Rust core {times[1]:.0f} s)', f'exit {out[0][0]} / {out[1][0]}')


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py); culltree runs on Python')
        return 0
    failed = 0

    def check(ok: object, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    objs = [('00000000-0000-0000-0000-00000000000%d' % i, bp, tc.rot_y(i * 40, (i * 10, 2, -i * 7)), 1)
            for i, bp in enumerate(['objects/test/hut/hut', 'objects/test/rock/rock', 'objects/test/lamp/lamp'] * 3)]
    nodes, placed = tc.make_tree(objs, 100)
    made = [('tree', tc.tree_file(nodes, 100)), ('level', tc.level_file(placed))]
    made.append(('a trailer after the stream', made[0][1] + b'\0\x07trailing'))
    made.append(('strings that are not UTF-8', made[1][1].replace(b'objects/test/hut', b'objects/t\xe9\xffx/hut')))
    head = made[1][1][:24 + struct.unpack('>I', made[1][1][16:20])[0]]
    made.append(('a tag beyond 64 bits (Python reads it as a big integer)', head + b'\x80' * 10 + b'\x02\x50\x01\x02'))
    made.append(('a two-byte value cut by the end', head + b'\x01\x50\x07'))
    made.append(('a top-level end mark, then bytes', head + b'\x01\x50\x07\x08\x00\x01\x50'))
    bad = [f'{label}: {d}' for label, data in made if (d := compare(data)[0])]
    check(not bad, f'{len(made)} built files (a tree, a level, a trailer, non-UTF-8 strings, a tag past 64 bits, a cut '
          'value, an early end): the same elements and bytes in both readers', '; '.join(bad))

    rng = random.Random(2049)
    kinds: Counter[str] = Counter()
    bad_m, n = [], 0
    for _, data in made[:2]:
        for m in mutations(data, rng, 700):
            n += 1
            d, k = compare(m, False)
            kinds[k] += 1
            if d:
                bad_m.append(f'#{n}: {d}')
    noise = [ct.MAGIC + bytes(rng.randrange(256) for _ in range(16 + rng.randrange(80))) for _ in range(500)]
    for m in noise:
        n += 1
        d, k = compare(m)
        kinds[k] += 1
        if d:
            bad_m.append(f'noise #{n}: {d}')
    check(not bad_m, f'{n} damaged and random files (cut short, bytes changed, runs written, bytes cut out, long '
          f'varints): the same elements, or the same exception and message, in both ({dict(kinds)})',
          '; '.join(bad_m[:3]))

    # the tree and level objects on the built files, and add on them, both ways
    with tempfile.TemporaryDirectory(prefix='culltree_native_') as tmp:
        tp, lp = os.path.join(tmp, 'Levels', 'test', 'staticculltree_exclusive.dbx'), os.path.join(tmp, 'Levels', 'test.dbx')
        os.makedirs(os.path.dirname(tp))
        objs0 = [o[:3] + (0,) if i in (2, 5) else o for i, o in enumerate(objs)]
        nodes0, placed0 = tc.make_tree(objs0, 100)
        with open(tp, 'wb') as fh:
            fh.write(tc.tree_file(nodes0, 100))
        with open(lp, 'wb') as fh:
            fh.write(tc.level_file(placed0))
        res = []
        for on in (False, True):
            ct.use_native(on)
            o = os.path.join(tmp, f'out{on}')
            log: list[str] = []
            t, lv = ct.CullTree(tp), ct.Level(lp)
            found = ct.collect([(t, lv, 'level')])
            done = ct.add(t, lv, ct.auto_objects(lv, found), found, log.append)
            t.write(os.path.join(o, 'tree.dbx'))
            lv.doc.write(os.path.join(o, 'level.dbx'))
            res.append((tree_state(tp), level_state(lp), len(done), log,
                        open(os.path.join(o, 'tree.dbx'), 'rb').read(), open(os.path.join(o, 'level.dbx'), 'rb').read()))
        ct.use_native(True)
        check(res[0] == res[1] and res[0][2] == 2, 'a built tree and level: the same CullTree, Level, objects added, '
              'log and files written in both backends', f'{res[0][2]} / {res[1][2]} added')

        # the folder listing and the level search
        lv_dir = os.path.join(tmp, 'refs')
        for rel, data in (('a/Levels/one.dbx', b'..ExclusiveStaticCullTree..'), ('a/Levels/one/staticculltree_x.dbx', b'ExclusiveStaticCullTree'),
                          ('a/Levels/one_description.dbx', b'ExclusiveStaticCullTree'), ('a/b/two.DBX', b'none'),
                          ('a/.hidden/three.dbx', b'ExclusiveStaticCullTree'), ('a/.four.dbx', b'ExclusiveStaticCullTree'),
                          ('a/five.dbx.txt', b'ExclusiveStaticCullTree'), ('top.dbx', b'xxExclusiveStaticCullTree'),
                          ('a/été/six.dbx', b'ExclusiveStaticCullTree'), ('a/empty.dbx', b'')):
            p = os.path.join(lv_dir, *rel.split('/'))
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, 'wb') as fh:
                fh.write(data)
        link = os.path.join(lv_dir, 'a', 'link')
        linked = subprocess.run(['cmd', '/c', 'mklink', '/J', link, os.path.join(lv_dir, 'a', 'b')],
                                capture_output=True).returncode == 0 if os.name == 'nt' else False
        g = sorted(glob.glob(os.path.join(lv_dir, '**', '*.dbx'), recursive=True))
        r = sorted(os.path.join(lv_dir, x) for x in NATIVE.culltree_dbx_files(lv_dir))
        ct.use_native(False)
        a = ct.level_partitions(lv_dir)
        ct.use_native(True)
        b = ct.level_partitions(lv_dir)
        check(g == r and a == b and len(a) == 3,
              f'a folder with hidden files and folders, other cases, a non-ASCII folder, an empty file'
              f'{", a junction" if linked else ""}: the Rust listing is glob\'s ({len(g)} files), the same 3 level '
              'partitions in both backends', f'{g} / {r}; {a} / {b}')
        if linked:
            os.rmdir(link)                    # the junction itself, never what it points at
        os.makedirs(os.path.join(lv_dir, 'dir.dbx'))
        errs = []
        for on in (False, True):
            ct.use_native(on)
            try:
                ct.level_partitions(lv_dir)
                errs.append('none')
            except OSError as e:
                errs.append(type(e).__name__)
        ct.use_native(True)
        check(errs[0] == errs[1] != 'none', f'a folder named *.dbx: both backends raise as Python reads it ({errs[0]})')

    check(ct.backend() == 'rust' and ct.use_native(False) == 'python' and ct.use_native(True) == 'rust',
          'use_native switches the backend')
    code = 'import culltree; print(culltree.backend())'
    forced = subprocess.run([sys.executable, '-c', code], env={**os.environ, 'BFBC2_PURE_PYTHON': '1'},
                            capture_output=True, text=True, cwd=ROOT)
    plain = subprocess.run([sys.executable, '-c', code], capture_output=True, text=True, cwd=ROOT,
                           env={k: v for k, v in os.environ.items() if k != 'BFBC2_PURE_PYTHON'})
    check(forced.stdout.strip() == 'python' and plain.stdout.strip() == 'rust',
          'BFBC2_PURE_PYTHON=1 forces Python; without it the Rust core runs',
          f'{forced.stdout.strip()} / {plain.stdout.strip()} {forced.stderr[-300:]}')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the game runs (no Project OPFOR BFBC2 game copy)')
    elif '--game' in argv:
        run_corpus(root, check)
        run_listing(root, check)
        run_add(root, check)
    else:
        sample = [i for i in corpus_work(root) if i[0] == 'tree'][:2]
        bad_g = [f'{i[1]}: {one[2]}' for i in sample if (one := _compare_file(i))[2]]
        check(sample and not bad_g, f'{len(sample)} cull trees of OPFOR\'s work folders: the same elements, CullTree '
              'and bytes in both backends', '; '.join(bad_g))
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


def corpus_work(root: str) -> list[tuple[str, str, str]]:
    """The trees and levels of work\\ww2 only (quick)."""
    out = []
    for d, _, names in os.walk(os.path.join(root, 'work', 'ww2', 'coral_sea')):
        out += [('tree', os.path.join(d, n), '') for n in sorted(names) if 'staticculltree' in n.lower()]
    return out


if __name__ == '__main__':
    sys.exit(main(sys.argv))
