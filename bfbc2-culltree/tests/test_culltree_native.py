r"""culltree.py's element reader (rust/bc2-formats culltree, through the bc2_native module) against what its retired
Python reader (Doc's element loop; round F1a, 2026-11-18) read, recorded before it was deleted (culltree_golden.json):
the same elements, trailer and bytes written, the same CullTree / Level, the same add run; and ref_pairs' level search
(level_partitions: the folder listing glob gives, the files that hold ExclusiveStaticCullTree).

  python test_culltree_native.py          quick: built trees and levels, damaged and random streams, add on a built
                                          level, a folder with hidden files, links and odd names; 2 trees of OPFOR's
                                          work folders when they are there
  python test_culltree_native.py --game   + every cull tree and level partition of the game copy's level bundles (read
                                          only) and of OPFOR's work\levels / work\ww2 / work\console_cull: the same
                                          CullTree / Level, written back as they were; the listing of every work\levels
                                          folder against glob; and `add --auto --misplaced 2 --ref-dir <every work\levels
                                          folder>` on Onslaught's MP_005 (console_cull's run) into a temp folder: the
                                          recorded Python run's tree, level and report

Where the Python reader raised IndexError (a stream cut inside an element) the reader now raises CullTreeError saying
so; where it read a varint past 64 bits as a big integer (no real file has one) it raises CullTreeError too: the
counts are printed. A file the recording does not have (changed since) is counted, not compared. Needs the module.
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
sys.path.insert(0, os.path.join(_ROOT, 'core', 'tests'))
import culltreelib as ct  # noqa: E402
import goldens  # noqa: E402
import test_culltree as tc  # noqa: E402
from bfbc2core import fbrb, native, paths  # noqa: E402

GOLDEN = os.path.join(HERE, 'culltree_golden.json')


# ------------------------------------------------------------------------------------------------ one file

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


def outcome(data: bytes) -> tuple[str, str]:
    """(the digest of what Doc reads - 'ok:' and its header, strings, elements, trailer and bytes written, or 'E:',
    the exception's type and its message's digest -, the message)."""
    try:
        d = ct.Doc(data, 'f')
    except Exception as e:  # noqa: BLE001 - every exception is recorded
        return f'E:{type(e).__name__}:{goldens.h(str(e), 8)}', str(e)
    return 'ok:' + goldens.rh((d.head12, d.string_area, d.strings, flat(d), d.trailer, d.tobytes()), 10), ''


class Tally:
    """How the reader's answers compare with the recorded ones."""

    def __init__(self) -> None:
        self.n: Counter[str] = Counter()
        self.bad: list[str] = []

    def add(self, label: str, want: str, got: tuple[str, str]) -> None:
        g, msg = got
        if g == want:
            self.n['the same'] += 1
        elif want.startswith(('E:IndexError', 'E:CullTreeError')) and g.startswith('E:CullTreeError') and 'truncated file' in msg:
            self.n['cut short: CullTreeError (Python: ' + want.split(':')[1] + ')'] += 1
        elif g.startswith('E:CullTreeError') and 'longer than 64 bits' in msg:
            python = 'read it' if want.startswith('ok:') else want.split(':')[1]
            self.n[f'a varint past 64 bits: CullTreeError (Python: {python})'] += 1
        else:
            self.bad.append(f'{label}: {g} {msg[:60]} / recorded {want}')

    def text(self) -> str:
        return ', '.join(f'{k}: {v}' for k, v in sorted(self.n.items()))


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


OBJS = [('00000000-0000-0000-0000-00000000000%d' % i, bp, tc.rot_y(i * 40, (i * 10, 2, -i * 7)), 1)
        for i, bp in enumerate(['objects/test/hut/hut', 'objects/test/rock/rock', 'objects/test/lamp/lamp'] * 3)]


def built_files() -> list[tuple[str, bytes]]:
    nodes, placed = tc.make_tree(OBJS, 100)
    made = [('tree', tc.tree_file(nodes, 100)), ('level', tc.level_file(placed))]
    made.append(('a trailer after the stream', made[0][1] + b'\0\x07trailing'))
    made.append(('strings that are not UTF-8', made[1][1].replace(b'objects/test/hut', b'objects/t\xe9\xffx/hut')))
    head = made[1][1][:24 + struct.unpack('>I', made[1][1][16:20])[0]]
    made.append(('a tag beyond 64 bits (Python read it as a big integer)', head + b'\x80' * 10 + b'\x02\x50\x01\x02'))
    made.append(('a two-byte value cut by the end', head + b'\x01\x50\x07'))
    made.append(('a top-level end mark, then bytes', head + b'\x01\x50\x07\x08\x00\x01\x50'))
    return made


def quick_cases() -> list[tuple[str, bytes]]:
    """(label, data) in the recording's order: built files, damaged copies of the tree and level, random streams."""
    made = built_files()
    out = list(made)
    rng = random.Random(2049)
    for _, data in made[:2]:
        out += [('damaged', m) for m in mutations(data, rng, 700)]
    out += [('noise', ct.MAGIC + bytes(rng.randrange(256) for _ in range(16 + rng.randrange(80)))) for _ in range(500)]
    return out


def tree_state(path: str) -> tuple[Any, ...]:
    t = ct.CullTree(path)
    return (t.enum_offset, t.nodes, t.used, t.spare, sorted(t.leaf_node), t.depth_and_stack())


def level_state(path: str) -> tuple[Any, ...]:
    lv = ct.Level(path)
    return (lv.tree_refs, [(o.guid, o.blueprint, o.m, o.enum) for o in lv.objects])


def built_add(tmp: str) -> str:
    """CullTree, Level, collect and add on a built tree and level (two objects at slot 0): the digest of the states,
    the objects added, the log and the files written."""
    tp, lp = os.path.join(tmp, 'Levels', 'test', 'staticculltree_exclusive.dbx'), os.path.join(tmp, 'Levels', 'test.dbx')
    os.makedirs(os.path.dirname(tp), exist_ok=True)
    objs0 = [o[:3] + (0,) if i in (2, 5) else o for i, o in enumerate(OBJS)]
    nodes0, placed0 = tc.make_tree(objs0, 100)
    with open(tp, 'wb') as fh:
        fh.write(tc.tree_file(nodes0, 100))
    with open(lp, 'wb') as fh:
        fh.write(tc.level_file(placed0))
    o = os.path.join(tmp, 'out')
    log: list[str] = []
    t, lv = ct.CullTree(tp), ct.Level(lp)
    found = ct.collect([(t, lv, 'level')])
    done = ct.add(t, lv, ct.auto_objects(lv, found), found, log.append)
    t.write(os.path.join(o, 'tree.dbx'))
    lv.doc.write(os.path.join(o, 'level.dbx'))
    return goldens.rh((tree_state(tp), level_state(lp), len(done), log, open(os.path.join(o, 'tree.dbx'), 'rb').read(),
                       open(os.path.join(o, 'level.dbx'), 'rb').read()), 16)


def file_outcome(item: tuple[str, str, str]) -> tuple[str, str, str, tuple[str, str], int]:
    """(label, data SHA-1, digest of the CullTree / Level and written bytes or the exception, Doc's outcome, bytes) for
    a tree or level partition: a file, or a bundle entry (copied to a temp file for CullTree / Level, which take
    paths)."""
    kind, path, entry = item
    label = path + (f':{entry}' if entry else '')
    data = fbrb.read(path, [entry])[entry] if entry else open(path, 'rb').read()
    sha = goldens.h(data, 16)
    if not data.startswith(ct.MAGIC):
        return label, sha, 'text', ('text', ''), len(data)
    doc = outcome(data)
    if not doc[0].startswith('ok:'):
        return label, sha, doc[0], doc, len(data)
    with tempfile.TemporaryDirectory(prefix='ct_native_') as tmp:
        p = os.path.join(tmp, 'f.dbx')
        with open(p, 'wb') as fh:
            fh.write(data)
        try:
            if kind == 'tree':
                st = tree_state(p)
                ct.CullTree(p).write(os.path.join(tmp, 'w.dbx'))
                st += (open(os.path.join(tmp, 'w.dbx'), 'rb').read() == data,)
            else:
                st = level_state(p)
            state = 'ok:' + goldens.rh(st, 16)
        except Exception as e:  # noqa: BLE001
            state = f'E:{type(e).__name__}:{goldens.h(str(e), 8)}'
    return label, sha, f'{doc[0]}|{state}', doc, len(data)


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


def key(label: str, root: str) -> str:
    return os.path.relpath(label, root).replace(os.sep, '/')


def run_corpus(root: str, check: Any, golden: dict[str, list[str]], items: list[tuple[str, str, str]] | None = None,
               what: str = '') -> None:
    t = time.time()
    items = corpus(root) if items is None else items
    tally = Tally()
    kinds: Counter[tuple[str, str]] = Counter()
    nbytes = changed = 0
    workers = max(1, min(3, (os.cpu_count() or 2) - 1))
    with ProcessPoolExecutor(workers) as pool:
        for item, (label, sha, d, doc, size) in zip(items, pool.map(file_outcome, items, chunksize=4)):
            kinds[(item[0], d.split(':')[0] if '|' not in d else 'ok')] += 1
            nbytes += size
            want = golden.get(key(label, root))
            if want is None or want[0] != sha:
                changed += 1
            elif want[1] == d:
                tally.n['the same'] += 1
            elif doc[0] == want[1].split('|')[0]:
                tally.bad.append(f'{label}: the same elements, another CullTree / Level or written bytes')
            else:
                tally.add(label, want[1].split('|')[0], doc)
    trees = sum(n for (k, _), n in kinds.items() if k == 'tree')
    check(not tally.bad and kinds[('tree', 'ok')] > 0,
          f'{what or ""}{trees} cull trees and {len(items) - trees} level partitions ({nbytes / 1e6:.0f} MB): the elements, '
          f'trailers and bytes, CullTree (nodes, leaves, spare, depth) and Level (objects, tree references) the Python '
          f'reader gave, trees written back as they were ({tally.text()}; {changed} changed since the recording)  '
          f'({time.time() - t:.0f} s)', '; '.join(tally.bad[:3]))


def level_partitions_glob(folder: str) -> list[str]:
    """level_partitions as glob and a plain read find them (the test's own oracle)."""
    files = sorted(glob.glob(os.path.join(folder, '**', '*.dbx'), recursive=True))
    out = []
    for p in files:
        b = os.path.basename(p).lower()
        if 'staticculltree' in b or '_description' in b:
            continue
        with open(p, 'rb') as f:
            if ct.LEVEL_MARK in f.read():
                out.append(p)
    return out


def run_listing(root: str, check: Any) -> None:
    levels = os.path.join(root, 'work', 'levels')
    dirs = sorted(os.path.join(levels, d) for d in os.listdir(levels) if os.path.isdir(os.path.join(levels, d)))
    t = time.time()
    bad, n, found = [], 0, 0
    m = native.require('culltree')
    for d in dirs:
        g = sorted(glob.glob(os.path.join(d, '**', '*.dbx'), recursive=True))
        r = sorted(os.path.join(d, x) for x in m.culltree_dbx_files(d))
        n += len(g)
        if g != r:
            bad.append(f'{d}: glob {len(g)}, rust {len(r)}')
    t1 = time.time()
    sample = dirs[:3]
    for d in sample:
        a = level_partitions_glob(d)
        b = ct.level_partitions(d)
        found += len(b)
        if a != b:
            bad.append(f'{d}: level partitions differ ({len(a)} / {len(b)})')
    check(not bad, f'{len(dirs)} folders of work\\levels ({n:,} .dbx): the Rust listing is glob\'s ({t1 - t:.0f} s); '
          f'level_partitions on {len(sample)} of them: the {found} partitions glob and a read find', '; '.join(bad[:3]))


CULL_SOURCE = ('Levels/ONS_MP_005/staticculltree_exclusive.dbx', 'Levels/ONS_MP_005.dbx')   # console_cull's


def add_run(root: str, tmp: str, env: dict[str, str]) -> tuple[str, str, Any] | None:
    """console_cull's add on Onslaught MP_005: (where the input came from, its digest, (exit code, report, the tree and
    level written)). The input: the level bundle as it was before console_cull's stage (the game copy's
    level-00.fbrb.opfor-cull-orig, read only; its tree and level partition written to tmp), else console_cull's own
    unpack in work (already patched: one object left to add)."""
    levels = os.path.join(root, 'work', 'levels')
    orig = os.path.join(root, 'game', 'Package', 'levels', 'ons_mp_005', 'level-00.fbrb.opfor-cull-orig')
    pkg = os.path.join(root, 'work', 'console_cull', 'ons_mp_005', 'pkg')
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
        return None
    refs = []
    dirs = sorted(d for d in os.listdir(levels) if os.path.isdir(os.path.join(levels, d)))
    for d in dirs:
        refs += ['--ref-dir', os.path.join(levels, d)]
    inputs = goldens.rh((goldens.file_sha(tree), goldens.file_sha(level), dirs), 16)
    o = os.path.join(tmp, 'out')
    os.makedirs(o, exist_ok=True)
    r = subprocess.run([*native.cli_command('culltree'), 'add', tree, level, '--auto',
                        '--misplaced', '2.0', *refs, '--out-tree', os.path.join(o, 'tree.dbx'),
                        '--out-level', os.path.join(o, 'level.dbx')],
                       capture_output=True, text=True, errors='replace', env={**os.environ, **env})
    files = [goldens.file_sha(os.path.join(o, f)) if os.path.isfile(os.path.join(o, f)) else None
             for f in ('tree.dbx', 'level.dbx')]
    return source, inputs, [r.returncode, r.stdout.replace(o, '<out>').replace(tmp, '<tmp>'), files]


def run_add(root: str, check: Any, golden: dict[str, Any]) -> None:
    with tempfile.TemporaryDirectory(prefix='culltree_add_') as tmp:
        t = time.time()
        got = add_run(root, tmp, {'BFBC2_CULLTREE_CACHE': '0'})    # every reference level read (no E3m rows)
    if got is None:
        print('skipped: the add run (no Onslaught MP_005 source or work\\levels)')
        return
    source, inputs, (code, report, files) = got
    if golden.get('inputs') != inputs:
        print(f'skipped: the add run ({source}: its input or the work\\levels folders changed since the recording)')
        return
    lines = report.strip().splitlines()
    added = sum(1 for x in lines if ': slot ' in x and ' leaves (shape from ' in x)
    check([code, goldens.h(report, 16), files] == golden['result'] and code == 0 and files[0] is not None,
          f'add --auto --misplaced 2 on Onslaught MP_005 ({source}) against every work\\levels folder: the recorded '
          f'Python run\'s tree, level and report ({added} objects added, {len(lines)} report lines, '
          f'"{lines[-1] if lines else ""}"; {time.time() - t:.0f} s)', f'exit {code}')


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    m = native.require('culltree', 'culltree_elements')
    failed = 0

    def check(ok: object, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    rec = goldens.load(GOLDEN)
    cases = quick_cases()
    tally = Tally()
    roundtrip = []
    for i, ((label, data), want) in enumerate(zip(cases, rec['cases'])):
        got = outcome(data)
        tally.add(f'{label} #{i}', want, got)
        if got[0].startswith('ok:') and label not in ('damaged',) and ct.Doc(data, 'f').tobytes() != data:
            roundtrip.append(label)
    check(len(cases) == len(rec['cases']) and not tally.bad and not roundtrip,
          f'{len(cases)} built, damaged and random files (cut short, bytes changed, runs written, bytes cut out, long '
          f'varints): the elements and bytes the Python reader gave, or an error ({tally.text()}); built files and '
          'random streams written back as they were', '; '.join(tally.bad[:3] + roundtrip))
    try:
        ct.Doc(bytearray(cases[0][1]), 'f')
        ok_any = True
    except Exception:  # noqa: BLE001
        ok_any = False
    check(ok_any, 'Doc reads bytearray as well as bytes')

    with tempfile.TemporaryDirectory(prefix='culltree_native_') as tmp:
        check(built_add(tmp) == rec['built_add'], 'a built tree and level: the CullTree, Level, objects added, log and '
              'files written the Python reader gave')

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
        r = sorted(os.path.join(lv_dir, x) for x in m.culltree_dbx_files(lv_dir))
        a, b = level_partitions_glob(lv_dir), ct.level_partitions(lv_dir)
        check(g == r and a == b and len(a) == 3,
              f'a folder with hidden files and folders, other cases, a non-ASCII folder, an empty file'
              f'{", a junction" if linked else ""}: the Rust listing is glob\'s ({len(g)} files), the 3 level '
              'partitions glob and a read find', f'{g} / {r}; {a} / {b}')
        if linked:
            os.rmdir(link)                    # the junction itself, never what it points at
        os.makedirs(os.path.join(lv_dir, 'dir.dbx'))
        try:
            ct.level_partitions(lv_dir)
            err = 'none'
        except OSError as e:
            err = type(e).__name__
        check(err != 'none', f'a folder named *.dbx: level_partitions raises as reading it does ({err})')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the game runs (no Project OPFOR BFBC2 game copy)')
    elif '--game' in argv:
        run_corpus(root, check, rec['game'])
        run_listing(root, check)
        run_add(root, check, rec['add'])
    else:
        run_corpus(root, check, rec['game'], [i for i in corpus_work(root) if i[0] == 'tree'][:2], 'OPFOR\'s work folders: ')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


def corpus_work(root: str) -> list[tuple[str, str, str]]:
    """The trees and levels of work\\ww2's Coral Sea only (quick)."""
    out = []
    for d, _, names in os.walk(os.path.join(root, 'work', 'ww2', 'coral_sea')):
        out += [('tree', os.path.join(d, n), '') for n in sorted(names) if 'staticculltree' in n.lower()]
    return out


if __name__ == '__main__':
    sys.exit(main(sys.argv))
