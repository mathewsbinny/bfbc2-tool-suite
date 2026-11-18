r"""The Rust DBX writer (bc2_native.dbx_write, rust/bc2-formats dbx::Writer) against the Python writer it replaces
(bfbc2core.dbx._py_write): the same bytes for every tree, the same exception where the Python writer raises one.

  python core\tests\test_dbx_write_native.py            quick: built trees and every edge of the Python writer's
                                                         input (shapes, kinds, strings, nesting) through dbx.write,
                                                         2,000 random trees, 200 of work\ww2's files written back as they
                                                         are and edited, DbxEdit's roundtrip of the Rust-written
                                                         files, the fallback
  python core\tests\test_dbx_write_native.py --game     + every .dbx of the corpus (Project OPFOR BFBC2's work\levels,
                                                         work\navkit\common and work\ww2): each parsed, written by
                                                         both writers as it is and after random edits, compared

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import copy
import os
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from types import SimpleNamespace
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx, paths  # noqa: E402

NATIVE: Any = dbx._import_native()
NATIVE = NATIVE if NATIVE is not None and hasattr(NATIVE, 'dbx_write') else None
E, C, T, N = dbx.Element, dbx.CONTAINER, dbx.TEXT, dbx.NUMBERS


def outcome(fn: Any, f: Any) -> tuple[str, bytes | None]:
    """('ok', bytes) or (exception type name, None)."""
    try:
        return 'ok', fn(f)
    except RecursionError:
        return 'RecursionError', None
    except Exception as e:
        return type(e).__name__, None


def native_takes(f: Any) -> bool:
    """Whether the Rust writer writes f itself (rather than leaving it to the Python writer)."""
    try:
        NATIVE.dbx_write(f)
        return True
    except Exception:
        return False


# ------------------------------------------------------------------------------------------------ trees

def built_tree() -> dbx.DbxFile:
    singles = b''.join(struct.pack('>f', v) for v in (1.0, 0.1, -2.5, 3.4e38))
    inst = E('instance', C, [('guid', 'AB-CD'), ('type', 'Test.Thing')], children=[
        E('field', T, [('name', 'Name')], text='Levels/MP_005 & <"quotes">'),
        E('field', T, [('name', 'Empty')]),
        E('field', T, [('ref', 'a/b'), ('name', 'Ref')]),
        E('field', N, [('name', 'Transform')], raw=bytes([4, 4]) + singles),
        E('field', N, [('name', 'Nothing')], raw=bytes([0, 4])),
        E('field', 6, [('name', 'On')], raw=b'\x00\x01'),
        E('field', 3, [('name', 'Byte')], raw=b'\x00\x07'),
        E('array', C, [('name', 'Kids')], children=[E('item', T, [('ref', f'r{i}')]) for i in range(300)]),
        E('complex', C, [('name', 'Deep')], children=[E('complex', C, [], children=[E('field', T, [], text='é ü 日本 \U0001F600')])]),
        E('array', C, [('name', 'None')], children=None),
    ])
    return dbx.DbxFile([], [E('partition', C, [('guid', 'P'), ('primaryInstance', 'AB-CD')], children=[inst])])


def nested(depth: int) -> dbx.DbxFile:
    e = E('field', T, [], text='leaf')
    for _ in range(depth):
        e = E('complex', C, [], children=[e])
    return dbx.DbxFile([], [e])


class Str(str):
    pass


def edge_trees() -> list[tuple[str, Any]]:
    """(label, tree) for every edge of the Python writer's input: made again on each call (a generator in a tree
    is used up by a write)."""
    one = lambda e: dbx.DbxFile([], [e])  # noqa: E731
    gen_children = E('array', C, [], children=None)
    gen_children.children = (x for x in [E('item', T, [], text='g')])  # type: ignore[assignment]
    return [
        ('an empty tag (the Python writer writes string 0)', one(E('', T, [], text='x'))),
        ('16 attributes', one(E('f', T, [(f'k{i}', 'v') for i in range(16)]))),
        ('15 attributes', one(E('f', 15, [(f'k{i}', f'v{i}') for i in range(15)], raw=b'\x00\x01'))),
        ('kind 16', one(E('f', 16, [], raw=b'\x00\x01'))),
        ('kind -1', one(E('f', -1, [], raw=b'\x00\x01'))),
        ('kind True', one(E('f', True, [], raw=b'\x00\x01'))),
        ('kind 10.0', one(E('f', 10.0, [], children=[]))),  # type: ignore[arg-type]
        ('kind 0', one(E('f', 0, [], raw=b'\x01\x02'))),
        ('a container kind with raw bytes and text', one(E('c', C, [], text='t', raw=b'xyz', children=[]))),
        ('a text kind with raw bytes', one(E('t', T, [], text='t', raw=b'xyz'))),
        ('a numbers kind with text', one(E('n', N, [], text='t', raw=bytes([1, 2, 0, 9])))),
        ('an int attribute value', one(E('f', T, [('name', 5)], text='x'))),  # type: ignore[list-item]
        ('attributes as a tuple of tuples', one(E('f', T, (('a', 'b'), ('c', 'd')), text='x'))),  # type: ignore[arg-type]
        ('attributes as lists', one(E('f', T, [['a', 'b']], text='x'))),  # type: ignore[list-item]
        ('a 3-item attribute', one(E('f', T, [('a', 'b', 'c')], text='x'))),  # type: ignore[list-item]
        ('a 2-character string attribute', one(E('f', T, ['ab'], text='x'))),  # type: ignore[list-item]
        ('attributes as a generator', one(E('f', T, (x for x in [('a', 'b')]), text='x'))),  # type: ignore[arg-type]
        ('attributes None', one(E('f', T, None, text='x'))),  # type: ignore[arg-type]
        ('children None', one(E('c', C, [], children=None))),
        ('children a tuple', one(E('c', C, [], children=(E('i', T, [], text='a'),)))),  # type: ignore[arg-type]
        ('children a generator', one(gen_children)),
        ('children a string', one(E('c', C, [], children='ab'))),  # type: ignore[arg-type]
        ('raw a bytearray', one(E('f', 6, [], raw=bytearray(b'\x00\x01')))),  # type: ignore[arg-type]
        ('raw a memoryview', one(E('f', 6, [], raw=memoryview(b'\x00\x01')))),  # type: ignore[arg-type]
        ('raw a list of ints', one(E('f', 6, [], raw=[0, 1]))),  # type: ignore[arg-type]
        ('raw a str', one(E('f', 6, [], raw='ab'))),  # type: ignore[arg-type]
        ('raw of any length', one(E('f', 6, [], raw=b'\x00\x01\x02\x03\x04'))),
        ('text None', one(E('f', T, [], text=None))),  # type: ignore[arg-type]
        ('a lone surrogate in a text', one(E('f', T, [], text='a\ud800b'))),
        ('a str subclass', one(E(Str('f'), T, [(Str('k'), 'v')], text=Str('v')))),
        ('equal strings as different objects', one(E('f', T, [('k', ''.join(['v', 'w']))], text='vw'))),
        ('the empty string everywhere', one(E('f', T, [('', '')], text=''))),
        ('nested 400 deep', nested(400)),
        ('nested 700 deep (past the Rust walk, within Python\'s recursion limit)', nested(700)),
        ('nested 3,000 deep (Python\'s recursion limit)', nested(3000)),
        ('duck-typed elements', dbx.DbxFile([], [SimpleNamespace(tag='c', kind=C, attributes=[('a', 'b')], children=[
            SimpleNamespace(tag='t', kind=T, attributes=[], text='x')])])),  # type: ignore[list-item]
        ('an element without raw', dbx.DbxFile([], [SimpleNamespace(tag='f', kind=6, attributes=[])])),  # type: ignore[list-item]
        ('roots a tuple', SimpleNamespace(roots=(E('f', T, [], text='x'),))),
        ('roots None', SimpleNamespace(roots=None)),
        ('no roots', dbx.DbxFile([], [])),
        ('many roots', dbx.DbxFile([], [E('f', T, [], text=str(i)) for i in range(50)])),
    ]


POOL = ['', 'a', 'name', 'guid', 'ref', 'Levels/MP_005', 'é', '日本', 'x' * 200, '&<>"', 'a/b/c', 'Name', 'zzz']


def random_tree(rng: random.Random, depth: int = 0) -> dbx.Element:
    s = lambda: rng.choice(POOL) if rng.random() < 0.8 else ''.join(rng.choice('abcxyz/_') for _ in range(rng.randrange(1, 9)))  # noqa: E731
    tag = rng.choice(['partition', 'instance', 'field', 'complex', 'array', 'item', s() or 't'])
    attrs = [(s(), s()) for _ in range(rng.choice([0, 0, 1, 1, 2, 3, 15]))]
    r = rng.random()
    if depth < 6 and r < 0.3:
        return E(tag, C, attrs, children=[random_tree(rng, depth + 1) for _ in range(rng.randrange(0, 6))])
    if r < 0.6:
        return E(tag, T, attrs, text=s())
    if r < 0.85:
        size = rng.choice([1, 2, 4, 8])
        count = rng.randrange(0, 6)
        return E(tag, N, attrs, raw=bytes([count, size]) + bytes(rng.randrange(256) for _ in range(count * size)))
    return E(tag, rng.choice([0, 1, 3, 6, 9, 15]), attrs, raw=bytes(rng.randrange(256) for _ in range(2)))


def walk(f: dbx.DbxFile) -> list[dbx.Element]:
    out, stack = [], list(f.roots)
    while stack:
        e = stack.pop()
        out.append(e)
        stack.extend(e.children or ())
    return out


def edit(f: dbx.DbxFile, rng: random.Random) -> dbx.DbxFile:
    """A copy of f with random edits of the kinds the suite's converters make: texts and attribute values changed,
    attributes added, children removed, copied, moved or added, numbers changed."""
    g = copy.deepcopy(f)
    els = walk(g)
    for _ in range(rng.randrange(1, 12)):
        e = rng.choice(els)
        op = rng.randrange(6)
        if op == 0 and e.kind == T:
            e.text = rng.choice([e.text + '_edited', '', 'Levels/WW2/new', 'é' + e.text])
        elif op == 1 and e.attributes:
            i = rng.randrange(len(e.attributes))
            e.attributes[i] = (e.attributes[i][0], e.attributes[i][1] + '2')
        elif op == 2 and len(e.attributes) < 15:
            e.attributes.append(('added', str(rng.randrange(1000))))
        elif op == 3 and e.children:
            e.children.pop(rng.randrange(len(e.children)))
        elif op == 4 and e.children:
            c = rng.choice(e.children)
            e.children.insert(rng.randrange(len(e.children) + 1), copy.deepcopy(c))
        elif op == 5 and e.kind == N and len(e.raw) > 2:
            raw = bytearray(e.raw)
            raw[-1] ^= 0x5A
            e.raw = bytes(raw)
        elif e.children is not None:
            e.children.append(E('field', T, [('name', 'Added')], text='new'))
    return g


# ------------------------------------------------------------------------------------------------ corpus

def corpus_files(root: str, quick: bool = False) -> list[str]:
    """The corpus's .dbx files (quick: work/ww2's only; listing all 140,000 takes most of a minute)."""
    out = []
    tops = ['ww2'] if quick else ['levels', os.path.join('navkit', 'common'), 'ww2']
    for top in (os.path.join(root, 'work', t) for t in tops):
        for d, _, names in os.walk(top):
            out += [os.path.join(d, n) for n in names if n.lower().endswith('.dbx')]
    return sorted(out)


def _chunk(job: tuple[list[str], int]) -> list[tuple[str, str, str | None, int, int]]:
    """(file, sha1, difference or None, elements, bytes) per binary file: written back by both writers as it is
    and after random edits."""
    import hashlib
    files, seed = job
    rng = random.Random(seed)
    out: list[tuple[str, str, str | None, int, int]] = []
    for path in files:
        try:
            data = open(path, 'rb').read()
        except OSError as e:
            out.append((path, '', f'unreadable: {e}', 0, 0))
            continue
        if not dbx.is_binary(data):
            continue
        h = hashlib.sha1(data).hexdigest()
        try:
            f = dbx.parse(data)
        except ValueError:
            out.append((path, h, None, 0, 0))     # a damaged file: the readers' tests cover it
            continue
        diff: str | None = None
        py, rs = dbx._py_write(f), NATIVE.dbx_write(f)
        if py != rs:
            diff = 'written back: the writers differ'
        elif dbx.parse(rs).roots != f.roots:
            diff = 'written back: does not parse to the same tree'
        else:
            g = edit(f, rng)
            if dbx._py_write(g) != NATIVE.dbx_write(g):
                diff = 'edited: the writers differ'
        out.append((path, h, diff, len(walk(f)), len(data)))
    return out


def run_corpus(files: list[str], check: Any, label: str, keep: str | None = None) -> list[str]:
    """Compares both writers on files (distinct by content); returns the binary files written (for DbxEdit)."""
    t = time.time()
    workers = max(1, (os.cpu_count() or 2) - 1)
    jobs = [(files[i:i + 200], i) for i in range(0, len(files), 200)]
    seen: set[str] = set()
    diffs: Counter[str] = Counter()
    examples: dict[str, str] = {}
    elements = size = 0
    written: list[str] = []
    with ProcessPoolExecutor(workers) as pool:
        for rows in pool.map(_chunk, jobs):
            for path, h, diff, n, b in rows:
                if h in seen:
                    continue
                seen.add(h)
                elements += n
                size += b
                if n:
                    written.append(path)
                if diff:
                    diffs[diff] += 1
                    examples.setdefault(diff, path)
    for k, n in diffs.items():
        print(f'       {n} x {k}, e.g. {examples[k]}')
    check(not diffs, f'{label}: {len(seen)} distinct binary files ({len(files)} found, {size / 1e6:.0f} MB, '
          f'{elements:,} elements) written back and edited: the Rust writer\'s bytes are the Python writer\'s  '
          f'({time.time() - t:.0f} s, {workers} processes)')
    return written


def check_dbxedit(files: list[str], check: Any) -> None:
    """DbxEdit's roundtrip (read, write with the file's own table; in process, bfbc2core.dbxedit) of Rust-written
    files: unchanged."""
    from bfbc2core import dbxedit
    tmp = tempfile.mkdtemp(prefix='dw')
    try:
        for i, p in enumerate(files):
            with open(os.path.join(tmp, f'{i}.dbx'), 'wb') as fh:
                fh.write(NATIVE.dbx_write(dbx.read(p)))
        r = dbxedit.run(['roundtrip', tmp])
        last = (r.stdout.strip().splitlines() or [''])[-1]
        check(r.returncode == 0 and last.startswith(f'roundtrip: {len(files)} identical, 0 different'),
              f'DbxEdit reads {len(files)} Rust-written files and writes each back unchanged', last or r.stderr[-200:])
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py); the suite writes with Python')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    dbx.use_native(True)
    b = built_tree()
    check(native_takes(b) and NATIVE.dbx_write(b) == dbx._py_write(b) and dbx.write(b) == dbx._py_write(b)
          and dbx.parse(dbx.write(b)).instances[0].child('Empty') == E('field', T, [('name', 'Empty')]),
          'a built partition: the Rust writer\'s bytes are the Python writer\'s')

    edges = edge_trees()
    bad, taken = [], 0
    for i, (label, _) in enumerate(edges):
        got, want = outcome(dbx.write, edge_trees()[i][1]), outcome(dbx._py_write, edge_trees()[i][1])
        taken += native_takes(edge_trees()[i][1])
        if got != want:
            bad.append(f'{label}: {got[0]} / {want[0]}')
    check(not bad, f'{len(edges)} edge trees through dbx.write: the Python writer\'s bytes or exception for each '
          f'({taken} written by Rust, the rest left to Python)', '; '.join(bad[:4]))
    surely = ['15 attributes', 'kind True', 'attributes as a tuple of tuples', 'attributes as lists', 'children None',
              'children a tuple', 'raw a bytearray', 'a str subclass', 'nested 400 deep', 'duck-typed elements']
    missed = [lb for lb, f in edges if lb in surely and not native_takes(f)]
    check(not missed, 'the shapes the suite builds are written by Rust (tuples, lists, None children, bytearray, '
          'str subclasses, duck-typed elements, 400 deep)', ', '.join(missed))

    rng = random.Random(1943)
    trees = [dbx.DbxFile([], [random_tree(rng) for _ in range(rng.randrange(1, 4))]) for _ in range(2000)]
    bad_r = [i for i, f in enumerate(trees) if NATIVE.dbx_write(f) != dbx._py_write(f)]
    back = [i for i, f in enumerate(trees[:300])
            if [x.kind for x in walk(dbx.parse(NATIVE.dbx_write(f)))] != [x.kind for x in walk(f)]]
    check(not bad_r and not back, f'{len(trees)} random trees (all kinds, 0-15 attributes, shared and new strings, '
          f'nesting): the same bytes from both writers, read back as the same tree', f'first: {bad_r[:3]} {back[:3]}')

    # the dispatcher and the fallback
    check(dbx.use_native(False) == 'python' and dbx.write(b) == dbx._py_write(b) and dbx.use_native(True) == 'rust',
          'use_native(False): the Python writer')
    env = dict(os.environ, **{dbx.PURE_PYTHON_ENV: '1'})
    code = 'from bfbc2core import dbx; print(dbx.backend())'
    forced = subprocess.run([sys.executable, '-c', code], env=env, capture_output=True, text=True, cwd=os.path.join(SUITE, 'core'))
    check(forced.stdout.strip() == 'python', f'{dbx.PURE_PYTHON_ENV}=1: dbx.write runs the Python writer')

    t0 = time.perf_counter()
    for _ in range(20):
        dbx._py_write(b)
    t1 = time.perf_counter()
    for _ in range(20):
        NATIVE.dbx_write(b)
    t2 = time.perf_counter()
    print(f'       a built partition, 20 writes: Python {1000 * (t1 - t0):.1f} ms, Rust {1000 * (t2 - t1):.1f} ms')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the corpus runs (no Project OPFOR BFBC2 work folder)')
    else:
        if '--game' in argv:
            files = corpus_files(root)
            written = run_corpus(files, check, 'corpus')
            check_dbxedit(random.Random(7).sample(written, min(3000, len(written))), check)
        else:
            files = corpus_files(root, quick=True)
            sample = random.Random(5).sample(files, min(200, len(files)))
            written = run_corpus(sample, check, 'corpus sample (work/ww2)')
            check_dbxedit(written, check)

    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
