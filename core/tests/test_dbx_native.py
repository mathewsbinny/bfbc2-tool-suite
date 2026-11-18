r"""The DBX reader (rust/bc2-formats dbx, through the bc2_native module) against what the retired Python reader answered
(round F1a, 2026-11-18): the same XML text, the same element trees, the same files refused - the Python's answers
recorded as digests before it was deleted (dbx_golden.json: the built, damaged and random files below;
dbx_golden_corpus_0.json - _3.json: every distinct .dbx of the corpus, with the Python writer's bytes for each tree as read and
after seeded random edits).

  python core\tests\test_dbx_native.py            quick: built files, edge cases, fuzzed bytes, the batch functions;
                                                   200 corpus files when the game copy is there
  python core\tests\test_dbx_native.py --game     + every .dbx of the corpus (Project OPFOR BFBC2's work\levels,
                                                   work\navkit\common - the 31,404 files DbxEdit's fingerprints
                                                   cover - and work\ww2), on all cores

The corpus run also checks the XML against DbxEdit's fingerprints (bfbc2-dbx-tools\tests\fingerprints_795745.txt)
for every file they cover. A corpus file whose content the recording does not have (changed since) is counted, not
compared. Needs the module (python rust\build_native.py).
"""
import copy
import os
import random
import struct
import sys
import time
from collections import Counter
from collections.abc import Callable
from concurrent.futures import ProcessPoolExecutor
from typing import Any, NamedTuple

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, HERE)
import goldens  # noqa: E402
from bfbc2core import dbx, native, paths  # noqa: E402

FINGERPRINTS = os.path.join(SUITE, 'bfbc2-dbx-tools', 'tests', 'fingerprints_795745.txt')
GOLDEN = os.path.join(HERE, 'dbx_golden.json')
GOLDEN_CORPUS = [os.path.join(HERE, f'dbx_golden_corpus_{k}.json') for k in range(4)]   # by SHA-1 (under 1 MB each)


def corpus_golden() -> dict[str, str]:
    """The corpus recording's {file SHA-1 (16 digits): digest}, its four shards joined."""
    out: dict[str, str] = {}
    for p in GOLDEN_CORPUS:
        out.update(goldens.load(p)['files'])
    return out


class Impl(NamedTuple):
    """The reader and writer whose answers are digested (the Rust core's; the recording was made with the Python's)."""
    to_xml: Callable[[bytes], str]
    parse: Callable[[bytes], dbx.DbxFile]
    write: Callable[[dbx.DbxFile], bytes]


RUST = Impl(dbx.to_xml, dbx.parse, dbx.write)


# ------------------------------------------------------------------------------------------------ digests

def xml_outcome(data: bytes, impl: Impl = RUST) -> str:
    try:
        return 'x' + goldens.h(impl.to_xml(data))
    except dbx.DeletedEntry:
        return 'deleted'
    except ValueError:
        return 'error'


def parse_outcome(data: bytes, impl: Impl = RUST) -> tuple[str, dbx.DbxFile | None]:
    try:
        f = impl.parse(data)
    except dbx.DeletedEntry:
        return 'deleted', None
    except ValueError:
        return 'error', None
    return 't' + goldens.rh(f), f


def outcome(data: bytes, impl: Impl = RUST) -> str:
    """The XML and tree of one file (or how each was refused), as one digest."""
    return goldens.h(f'{xml_outcome(data, impl)}|{parse_outcome(data, impl)[0]}', 10)


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
    E, T, N = dbx.Element, dbx.TEXT, dbx.NUMBERS
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


def file_digest(data: bytes, sha: str, impl: Impl = RUST) -> tuple[str, str]:
    """(the corpus digest of one file, its XML digest): XML, tree, the writer's bytes for the tree as read and after
    the seeded random edits (seed: the file's SHA-1)."""
    xml = xml_outcome(data, impl)
    tree, f = parse_outcome(data, impl)
    parts = [xml, tree]
    if f is not None:
        try:
            parts.append(goldens.h(impl.write(f)))
            parts.append(goldens.h(impl.write(edit(f, random.Random(int(sha[:12], 16))))))
        except Exception as e:  # noqa: BLE001 - recorded as the exception's type
            parts.append(type(e).__name__)
    return goldens.h('|'.join(parts), 16), xml


# ------------------------------------------------------------------------------------------------ built files

def _file(strings: list[bytes], body: bytes, area: int | None = None) -> bytes:
    """A binary .dbx of raw string bytes and a body, the header as the format has it."""
    offsets, blob = [], b''
    for s in strings:
        offsets.append(len(blob))
        blob += s + b'\0'
    a = 4 * len(strings) + len(blob) if area is None else area
    return dbx.MAGIC + struct.pack('>4I', 24 + a, 0, a, len(strings)) + struct.pack(f'>{len(strings)}I', *offsets) + blob + body


def built_files() -> list[tuple[str, bytes]]:
    """Files written with dbx.write, and hand-made bytes for what write never makes."""
    E, T, N, C = dbx.Element, dbx.TEXT, dbx.NUMBERS, dbx.CONTAINER
    singles = [1.0, 0.1, -2.5, 1e-5, 3.4028234663852886e38, 1.1754943508222875e-38, 0.33333334, 16777217.0, 123456.79,
               9.999999e-5, 1e7, 0.5, 2.5, -0.0]
    bits = [0, 1, 0x00800000, 0x7F800000, 0xFF800000, 0x7FC00000, 0x80000001, 0x00FFFFFF, 0x3F800001, 0xC2C80000]
    doubles = [0.1, 1 / 3, 1e21, 1e-300, 5e-324, 2.0 ** 70, -123.456, float('nan'), float('inf'), -float('inf'), -0.0,
               0.125, 1e15 + 0.5]
    f4 = b''.join(struct.pack('>f', v) for v in singles) + b''.join(struct.pack('>I', b) for b in bits)
    f8 = b''.join(struct.pack('>d', v) for v in doubles)
    count = len(singles) + len(bits)
    inst = E('instance', C, [('guid', 'AB-CD'), ('type', 'Test.Thing')], children=[
        E('field', T, [('name', 'Name')], text='Levels/MP_005 & <"quotes">'),
        E('field', T, [('name', 'Empty')]),
        E('field', T, [('ref', 'a/b'), ('name', 'Ref')]),
        E('field', N, [('name', 'Transform')], raw=bytes([count, 4]) + f4),
        E('field', N, [('name', 'Id')], raw=bytes([count, 4]) + f4),
        E('field', N, [('name', 'D')], raw=bytes([len(doubles), 8]) + f8),
        E('field', N, [('name', 'S')], raw=bytes([3, 2]) + bytes([0, 1, 0xFF, 0xFF, 0x12, 0x34])),
        E('field', N, [('name', 'Odd')], raw=bytes([2, 3]) + bytes([1, 2, 3, 4, 5, 6])),
        E('field', N, [('name', 'Nothing')], raw=bytes([0, 4])),
        E('field', 6, [('name', 'On')], raw=b'\x00\x01'),
        E('field', 6, [('name', 'Off')], raw=b'\x00\x00'),
        E('field', 3, [('name', 'Byte')], raw=b'\x00\x07'),
        E('array', C, [('name', 'Kids')], children=[E('item', T, [('ref', f'r{i}')]) for i in range(200)]),
        E('complex', C, [('name', 'Deep')], children=[E('complex', C, [], children=[E('field', T, [], text='é ü 日本')])]),
    ])
    part = E('partition', C, [('guid', 'P'), ('primaryInstance', 'AB-CD')], children=[inst])
    out = [('built partition', dbx.write(dbx.DbxFile([], [part])))]
    # strings a writer never makes: invalid UTF-8, a last string without its zero byte, two roots, a stop tag
    bad_utf8 = [b'', b'field', b'name', b'a\xffb\xed\xa0\x80c\xf0\x80', b'\xe2\x82']
    out.append(('invalid utf-8', _file(bad_utf8, bytes([1, 0x21, 2, 3, 4]))))
    s = _file([b'', b'field', b'x'], bytes([1, 0x20, 2]))
    out.append(('last string without its end', s[:-1]))
    out.append(('two roots, then a stop tag and junk', _file([b'', b'f', b'v'], bytes([1, 0x20, 2, 1, 0x20, 2, 0, 0xFF, 0xFF]))))
    out.append(('empty', b''))
    out.append(('not binary', b'<?xml version="1.0"?>\r\n<partition />\r\n'))
    out.append(('header only', dbx.MAGIC + bytes(17)))
    out.append(('container flag at the end', _file([b'', b'c'], bytes([1, 0xA0]))))
    out.append(('body offset past the end', _file([b'', b'c'], bytes([1, 0x20, 0]), area=10_000)))
    out.append(('body offset inside the table', _file([b'', b'c', b'dd'], bytes([1, 0x20, 0]), area=4)))
    return out


def mutations(data: bytes, rng: random.Random, n: int) -> list[bytes]:
    """Damaged copies: cut short at every length (up to 300), single bytes changed, runs of 0xFF / 0x00."""
    out = [data[:i] for i in range(min(len(data), 300))]
    for _ in range(n):
        d = bytearray(data)
        for _ in range(rng.randint(1, 4)):
            i = rng.randrange(len(d))
            op = rng.randrange(3)
            if op == 0:
                d[i] = rng.randrange(256)
            elif op == 1:
                d[i:i + 6] = b'\xff' * 6
            else:
                del d[i:i + rng.randint(1, 8)]
        out.append(bytes(d))
    return out


def quick_cases() -> dict[str, list[bytes]]:
    """The quick tier's files, in the recording's order: built, damaged (seeded), random bodies (seeded)."""
    built = built_files()
    rng = random.Random(1943)
    damaged = [m for _, d in built if d for m in mutations(d, rng, 400)]
    noise = [bytes(rng.randrange(256) for _ in range(rng.randrange(80))) for _ in range(500)]
    noise = [dbx.MAGIC + struct.pack('>4I', 0, 0, 0, rng.randrange(4)) + n for n in noise]
    return {'built': [d for _, d in built], 'damaged': damaged, 'noise': noise}


# ------------------------------------------------------------------------------------------------ corpus

def corpus_files(root: str) -> list[str]:
    out = []
    for top in (os.path.join(root, 'work', 'levels'), os.path.join(root, 'work', 'navkit', 'common'), os.path.join(root, 'work', 'ww2')):
        for d, _, names in os.walk(top):
            out += [os.path.join(d, n) for n in names if n.lower().endswith('.dbx')]
    return sorted(out)


def corpus_chunk(job: tuple[list[str], bool]) -> list[tuple[str, str, str, str, int]]:
    """(file, SHA-1, corpus digest, XML digest, bytes) per file (python=True: the retired Python's, for the
    recording)."""
    import hashlib
    files, python = job
    impl = python_impl() if python else RUST
    out: list[tuple[str, str, str, str, int]] = []
    for f in files:
        try:
            data = open(f, 'rb').read()
        except OSError as e:
            out.append((f, '', f'unreadable: {e}', '', 0))
            continue
        sha = hashlib.sha1(data).hexdigest()
        digest, xml = file_digest(data, sha, impl)
        out.append((f, sha, digest, xml, len(data)))
    return out


def python_impl() -> Impl:
    """The retired Python reader and writer (present only in the recording's commit)."""
    return Impl(lambda d: dbx._py_to_xml(dbx._bytes(d)), dbx._py_parse, dbx._py_write)  # type: ignore[attr-defined]


def run_corpus(files: list[str], check: Any, label: str, golden: dict[str, str]) -> None:
    fingerprints = {}
    for line in open(FINGERPRINTS):
        p = line.split()
        if len(p) >= 3 and p[0] == 'xml':
            fingerprints[p[1]] = p[2]
    t = time.time()
    workers = max(1, min(6, (os.cpu_count() or 2) - 1))
    chunks = [(files[i:i + 200], False) for i in range(0, len(files), 200)]
    seen: set[str] = set()
    differ: list[str] = []
    unrecorded = fingerprinted = fingerprint_wrong = total_bytes = 0
    with ProcessPoolExecutor(workers) as pool:
        for rows in pool.map(corpus_chunk, chunks):
            for f, sha, digest, xml, size in rows:
                if sha in seen:
                    continue
                seen.add(sha)
                total_bytes += size
                want = golden.get(sha[:16])
                if want is None:
                    unrecorded += 1
                elif want != digest:
                    differ.append(f)
                if sha in fingerprints and xml.startswith('x'):
                    fingerprinted += 1
                    fingerprint_wrong += xml[1:] != fingerprints[sha][:len(xml) - 1]
    check(not differ, f'{label}: {len(seen)} distinct files ({len(files)} found, {total_bytes / 1e6:.0f} MB): XML, tree, '
          f'written bytes as read and edited the recorded Python\'s on {len(seen) - unrecorded - len(differ)} '
          f'({unrecorded} not in the recording: changed since)  ({time.time() - t:.0f} s, {workers} processes)',
          f'{len(differ)} differ, e.g. {differ[:3]}')
    if label == 'corpus':
        check(fingerprinted >= 31_000 and not fingerprint_wrong,
              f'corpus: XML identical to DbxEdit\'s on {fingerprinted} fingerprinted files', f'{fingerprint_wrong} differ')


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    native.require('the DBX reader')
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    rec = goldens.load(GOLDEN)['cases']
    cases = quick_cases()
    for name, files in cases.items():
        got = [outcome(d) for d in files]
        bad = [i for i, (a, b) in enumerate(zip(got, rec[name])) if a != b]
        kinds = Counter(xml_outcome(d)[0] for d in files)
        check(len(got) == len(rec[name]) and not bad,
              f'{len(files)} {name} files: the XML, trees and refusals the Python reader gave ({kinds["x"]} read, '
              f'{kinds["e"]} refused, {kinds["d"]} deleted markers)', f'{len(got)} / {len(rec[name])} cases, first: {bad[:5]}')
    part = cases['built'][0]
    xml = dbx.to_xml(part)
    check('<field name="Transform">1/0.1/-2.5/1E-05/' in xml and '<field name="Empty" />' in xml
          and 'Levels/MP_005 &amp; &lt;&quot;quotes&quot;&gt;' in xml and 'NaN/Infinity/-Infinity/0/0.125' in xml,
          'the XML shows singles, doubles, escapes and empty text as DbxEdit does')

    # errors: ValueError (DeletedEntry for an empty file), never another exception
    errs = []
    for fn in (dbx.parse, dbx.to_xml):
        for m in cases['damaged'][:300]:
            try:
                fn(m)
            except ValueError:
                pass
            except Exception as e:
                errs.append(f'{fn.__name__}: {type(e).__name__}')
    check(not errs, 'parse / to_xml raise ValueError (never another exception) for a damaged file', ', '.join(errs[:3]))
    try:
        dbx.parse(b'')
        check(False, 'an empty file raises DeletedEntry')
    except dbx.DeletedEntry:
        check(True, 'an empty file raises DeletedEntry')

    # the input types and the batch functions
    check(dbx.parse(bytearray(part)) == dbx.parse(part) and dbx.to_xml(memoryview(part)) == xml,
          'dbx.parse / to_xml take bytes, bytearray and memoryview')
    items = cases['built']
    many = dbx.parse_many(items)
    xmls = dbx.to_xml_many(items)
    one = [parse_outcome(d)[0] for d in items]
    same = all((isinstance(m, dbx.DeletedEntry) and o == 'deleted') or (isinstance(m, ValueError) and o == 'error')
               or (isinstance(m, dbx.DbxFile) and o == 't' + goldens.rh(m)) for m, o in zip(many, one))
    same_x = all(xml_outcome(d) == ('error' if isinstance(x, ValueError) else 'x' + goldens.h(x))
                 for x, d in zip(xmls, items))
    check(same and same_x and len(many) == len(items) and isinstance(many[items.index(b'')], dbx.DeletedEntry),
          'parse_many / to_xml_many: in order, a refused file\'s error in its place, as one at a time')
    core = native.require('the DBX reader')
    check(isinstance(core.threads(), int) and core.threads() >= 1 and isinstance(core.VERSION, str),
          f'bc2_native {core.VERSION}: {core.threads()} worker threads')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the corpus runs (no Project OPFOR BFBC2 game copy)')
    else:
        paths_ = corpus_files(root)
        golden = corpus_golden()
        if '--game' in argv:
            run_corpus(paths_, check, 'corpus', golden)
        else:
            sample = random.Random(5).sample(paths_, min(200, len(paths_)))
            run_corpus(sample, check, 'corpus sample', golden)

    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
