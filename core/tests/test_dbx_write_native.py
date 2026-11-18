r"""The DBX writer (bc2_native.dbx_write, rust/bc2-formats dbx::Writer) against what the retired Python writer
(dbx._py_write, round F1a 2026-11-18) wrote, recorded before it was deleted (dbx_golden.json; the corpus's in
dbx_golden_corpus_*.json): the same bytes for every tree, an error where the Python writer raised one.

  python core\tests\test_dbx_write_native.py            quick: built trees and every edge of the writer's input
                                                         (shapes, kinds, strings, nesting) through dbx.write, 2,000
                                                         random trees, 200 of work\ww2's files written back as they are
                                                         and edited, DbxEdit's roundtrip of the written files
  python core\tests\test_dbx_write_native.py --game     + every .dbx of the corpus (test_dbx_native.py --game's run:
                                                         each written as it is and after seeded edits) and DbxEdit's
                                                         roundtrip of 3,000 of them

The edges where the Rust writer deliberately answers otherwise than the Python one did are listed in DELIBERATE (a
tree the Python wrote that does not read back, or a clearer error). Needs the module (python rust\build_native.py).
"""
import os
import random
import shutil
import struct
import sys
import tempfile
import time
from collections.abc import Callable
from types import SimpleNamespace
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, HERE)
import goldens  # noqa: E402
import test_dbx_native as tdn  # noqa: E402
from bfbc2core import dbx, native, paths  # noqa: E402

E, C, T, N = dbx.Element, dbx.CONTAINER, dbx.TEXT, dbx.NUMBERS
# the edges where the writer answers otherwise than the retired Python writer did, and what it answers now
DELIBERATE = {
    'an empty tag (the Python writer writes string 0)': 'ValueError',   # Python wrote tag 0: it reads back as an end mark
    'a 2-character string attribute': 'TypeError',                       # Python took 'ab' as the pair ('a', 'b')
    'an int attribute value': 'TypeError',                               # Python: AttributeError (int.encode)
    'text None': 'TypeError',                                            # Python: AttributeError (None.encode)
    'nested 3,000 deep (Python\'s recursion limit)': 'ok',               # Python: RecursionError; the walk has no limit
}


def outcome(fn: Callable[[Any], bytes], f: Any) -> str:
    """'ok:' and the bytes' digest, or the exception's type name."""
    try:
        return 'ok:' + goldens.h(fn(f))
    except RecursionError:
        return 'RecursionError'
    except Exception as e:
        return type(e).__name__


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

def random_trees() -> list[dbx.DbxFile]:
    rng = random.Random(1943)
    return [dbx.DbxFile([], [random_tree(rng) for _ in range(rng.randrange(1, 4))]) for _ in range(2000)]


def quick_outcomes(write: Callable[[Any], bytes]) -> dict[str, Any]:
    """What a writer gives for the quick tier's trees: the built partition, every edge, the random trees."""
    return {'write_built': outcome(write, built_tree()),
            'write_edges': {label: outcome(write, edge_trees()[i][1]) for i, (label, _) in enumerate(edge_trees())},
            'write_random': [outcome(write, f)[3:13] for f in random_trees()]}


# ------------------------------------------------------------------------------------------------ corpus

def corpus_files(root: str) -> list[str]:
    """work/ww2's .dbx files (the quick tier's sample; listing all 140,000 takes most of a minute)."""
    out = []
    for d, _, names in os.walk(os.path.join(root, 'work', 'ww2')):
        out += [os.path.join(d, n) for n in names if n.lower().endswith('.dbx')]
    return sorted(out)


def check_dbxedit(files: list[str], check: Any) -> None:
    """DbxEdit's roundtrip (read, write with the file's own table; in process, bfbc2core.dbxedit) of files the
    writer wrote: unchanged."""
    from bfbc2core import dbxedit
    tmp = tempfile.mkdtemp(prefix='dw')
    try:
        for i, p in enumerate(files):
            with open(os.path.join(tmp, f'{i}.dbx'), 'wb') as fh:
                fh.write(dbx.write(dbx.read(p)))
        r = dbxedit.run(['roundtrip', tmp])
        last = (r.stdout.strip().splitlines() or [''])[-1]
        check(r.returncode == 0 and last.startswith(f'roundtrip: {len(files)} identical, 0 different'),
              f'DbxEdit reads {len(files)} written files and writes each back unchanged', last or r.stderr[-200:])
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def binary(files: list[str]) -> list[str]:
    """The files that parse (the ones written)."""
    out = []
    for p in files:
        try:
            dbx.read(p)
            out.append(p)
        except ValueError:
            pass
    return out


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    native.require('the DBX writer', 'dbx_write')
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    rec = goldens.load(tdn.GOLDEN)['cases']
    got = quick_outcomes(dbx.write)
    b = built_tree()
    check(got['write_built'] == rec['write_built'] and dbx.parse(dbx.write(b)).instances[0].child('Empty') == E('field', T, [('name', 'Empty')]),
          'a built partition: the bytes the Python writer wrote', f'{got["write_built"]} / {rec["write_built"]}')
    bad, kept = [], 0
    for label, want in rec['write_edges'].items():
        now = got['write_edges'].get(label)
        if label in DELIBERATE:
            if not (now or '').startswith(DELIBERATE[label]):
                bad.append(f'{label}: {now} (expected {DELIBERATE[label]})')
        elif now != want:
            bad.append(f'{label}: {now} / Python {want}')
        else:
            kept += 1
    check(not bad and len(got['write_edges']) == len(rec['write_edges']),
          f'{len(rec["write_edges"])} edge trees through dbx.write: {kept} the Python writer\'s bytes or exception, '
          f'{len(DELIBERATE)} deliberately otherwise (DELIBERATE)', '; '.join(bad[:4]))
    bad_r = [i for i, (a, w) in enumerate(zip(got['write_random'], rec['write_random'])) if a != w]
    trees = random_trees()
    back = [i for i, f in enumerate(trees[:300])
            if [x.kind for x in tdn.walk(dbx.parse(dbx.write(f)))] != [x.kind for x in tdn.walk(f)]]
    check(not bad_r and not back and len(got['write_random']) == len(rec['write_random']),
          f'{len(trees)} random trees (all kinds, 0-15 attributes, shared and new strings, nesting): the Python '
          'writer\'s bytes, read back as the same tree', f'first: {bad_r[:3]} {back[:3]}')

    t0 = time.perf_counter()
    for _ in range(20):
        dbx.write(b)
    print(f'       a built partition, 20 writes: {1000 * (time.perf_counter() - t0):.1f} ms')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the corpus runs (no Project OPFOR BFBC2 work folder)')
    else:
        golden = tdn.corpus_golden()
        if '--game' in argv:
            files = tdn.corpus_files(root)
            tdn.run_corpus(files, check, 'corpus (written as read and edited)', golden)
            check_dbxedit(binary(random.Random(7).sample(files, min(3000, len(files)))), check)
        else:
            sample = random.Random(5).sample(corpus_files(root), min(200, len(corpus_files(root))))
            tdn.run_corpus(sample, check, 'corpus sample (work/ww2; written as read and edited)', golden)
            check_dbxedit(binary(sample), check)

    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
