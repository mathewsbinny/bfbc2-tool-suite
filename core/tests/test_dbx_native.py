r"""The Rust DBX reader (rust/bc2-formats, through the bc2_native module) against the Python reader it was ported
from: the same XML text, the same element trees, the same files refused.

  python core\tests\test_dbx_native.py            quick: built files, edge cases, fuzzed bytes, the fallback;
                                                   200 corpus files when the game copy is there
  python core\tests\test_dbx_native.py --game     + every .dbx of the corpus (Project OPFOR BFBC2's work\levels,
                                                   work\navkit\common - the 31,404 files DbxEdit's fingerprints
                                                   cover - and work\ww2), on all cores

The corpus run also checks the XML against DbxEdit's fingerprints (bfbc2-dbx-tools\tests\fingerprints_795745.txt)
for every file they cover. Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import hashlib
import os
import random
import struct
import subprocess
import sys
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
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
FINGERPRINTS = os.path.join(SUITE, 'bfbc2-dbx-tools', 'tests', 'fingerprints_795745.txt')


# ------------------------------------------------------------------------------------------------ one file, both readers

def _outcome(fn: Any, data: bytes) -> tuple[str, Any]:
    try:
        return 'ok', fn(data)
    except dbx.DeletedEntry:
        return 'deleted', None
    except ValueError:
        return 'error', None


def _py_parse(data: bytes) -> dbx.DbxFile:
    return dbx._py_parse(data)


def _rs_parse(data: bytes) -> dbx.DbxFile:
    return NATIVE.dbx_parse(dbx._check(data), dbx.Element, dbx.DbxFile)


def compare(data: bytes) -> tuple[str | None, str | None]:
    """(None or the kind of difference, SHA-1 of the XML when both readers wrote it)."""
    (pk, px), (rk, rx) = _outcome(dbx._py_to_xml, data), _outcome(NATIVE.dbx_to_xml, data)
    if pk != rk:
        return f'xml: python {pk}, rust {rk}', None
    if pk == 'ok' and px != rx:
        return 'xml text differs', None
    (pk2, pt), (rk2, rt) = _outcome(_py_parse, data), _outcome(_rs_parse, data)
    if pk2 != rk2:
        return f'parse: python {pk2}, rust {rk2}', None
    if pk2 == 'ok' and pt != rt:
        return 'parse tree differs', None
    return None, hashlib.sha1(px.encode('utf-8')).hexdigest() if pk == 'ok' else None


def _chunk(files: list[str]) -> list[tuple[str, str, str | None, str | None, int]]:
    out: list[tuple[str, str, str | None, str | None, int]] = []
    for f in files:
        try:
            data = open(f, 'rb').read()
        except OSError as e:
            out.append((f, '', f'unreadable: {e}', None, 0))
            continue
        diff, xml_sha = compare(data)
        out.append((f, hashlib.sha1(data).hexdigest(), diff, xml_sha, len(data)))
    return out


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


# ------------------------------------------------------------------------------------------------ corpus

def corpus_files(root: str) -> list[str]:
    out = []
    for top in (os.path.join(root, 'work', 'levels'), os.path.join(root, 'work', 'navkit', 'common'), os.path.join(root, 'work', 'ww2')):
        for d, _, names in os.walk(top):
            out += [os.path.join(d, n) for n in names if n.lower().endswith('.dbx')]
    return sorted(out)


def run_corpus(files: list[str], check: Any, label: str) -> None:
    golden = {}
    for line in open(FINGERPRINTS):
        p = line.split()
        if len(p) >= 3 and p[0] == 'xml':
            golden[p[1]] = p[2]
    t = time.time()
    workers = max(1, (os.cpu_count() or 2) - 1)
    chunks = [files[i:i + 200] for i in range(0, len(files), 200)]
    seen: set[str] = set()
    diffs: Counter[str] = Counter()
    examples: dict[str, str] = {}
    fingerprinted = fingerprint_wrong = total_bytes = 0
    with ProcessPoolExecutor(workers) as pool:
        for rows in pool.map(_chunk, chunks):
            for f, h, diff, xml_sha, size in rows:
                if h in seen:
                    continue
                seen.add(h)
                total_bytes += size
                if diff:
                    diffs[diff] += 1
                    examples.setdefault(diff, f)
                if h in golden and xml_sha is not None:
                    fingerprinted += 1
                    fingerprint_wrong += xml_sha != golden[h]
    for k, n in diffs.items():
        print(f'       {n} x {k}, e.g. {examples[k]}')
    check(not diffs, f'{label}: {len(seen)} distinct files ({len(files)} found, {total_bytes / 1e6:.0f} MB): the same XML, '
          f'trees and refusals in both readers  ({time.time() - t:.0f} s, {workers} processes)')
    if label == 'corpus':
        check(fingerprinted >= 31_000 and not fingerprint_wrong,
              f'corpus: Rust XML identical to DbxEdit\'s on {fingerprinted} fingerprinted files', f'{fingerprint_wrong} differ')


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py); the suite runs on the Python reader')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    built = built_files()
    bad = [name for name, data in built if compare(data)[0]]
    check(not bad, f'{len(built)} built and hand-made files: the same XML, trees and refusals', ', '.join(bad))
    part = built[0][1]
    xml = NATIVE.dbx_to_xml(part)
    check('<field name="Transform">1/0.1/-2.5/1E-05/' in xml and '<field name="Empty" />' in xml
          and 'Levels/MP_005 &amp; &lt;&quot;quotes&quot;&gt;' in xml and 'NaN/Infinity/-Infinity/0/0.125' in xml,
          'the XML shows singles, doubles, escapes and empty text as DbxEdit does')

    rng = random.Random(1943)
    damaged = [m for _, d in built if d for m in mutations(d, rng, 400)]
    bad_m = [i for i, m in enumerate(damaged) if compare(m)[0]]
    kinds = Counter(_outcome(NATIVE.dbx_to_xml, m)[0] for m in damaged)
    check(not bad_m, f'{len(damaged)} damaged files (cut short, bytes changed): both readers agree, no crash '
          f'({kinds["ok"]} read, {kinds["error"]} refused)', f'first: {bad_m[:5]}')
    noise = [bytes(rng.randrange(256) for _ in range(rng.randrange(80))) for _ in range(500)]
    noise = [dbx.MAGIC + struct.pack('>4I', 0, 0, 0, rng.randrange(4)) + n for n in noise]
    bad_n = [i for i, m in enumerate(noise) if compare(m)[0]]
    check(not bad_n, f'{len(noise)} random bodies behind a header: both readers agree', f'first: {bad_n[:5]}')

    # errors as the Python reader raises them
    errs = []
    for fn in (dbx.parse, dbx.to_xml):
        for m in damaged[:300]:
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

    # the dispatcher, the batch functions and the fallback
    check(dbx.use_native(True) == 'rust' and dbx.parse(part) == dbx._py_parse(part)
          and dbx.parse(bytearray(part)) == dbx._py_parse(part) and dbx.to_xml(memoryview(part)) == dbx._py_to_xml(part),
          'dbx.parse / to_xml run in Rust when it is built (bytes, bytearray, memoryview)')
    items = [d for _, d in built]
    many_rs = dbx.parse_many(items)
    xml_rs = dbx.to_xml_many(items)
    check(dbx.use_native(False) == 'python', 'use_native(False): the Python reader')
    many_py = dbx.parse_many(items)
    xml_py = dbx.to_xml_many(items)
    same = all((isinstance(a, ValueError) and isinstance(b, ValueError) and type(a) is type(b)) or a == b
               for a, b in zip([*many_rs, *xml_rs], [*many_py, *xml_py]))
    check(same and len(many_rs) == len(items) and isinstance(many_rs[0], dbx.DbxFile)
          and isinstance(many_rs[items.index(b'')], dbx.DeletedEntry),
          'parse_many / to_xml_many: in order, a refused file\'s error in its place, the same in both readers')
    dbx.use_native(True)
    env = dict(os.environ, **{dbx.PURE_PYTHON_ENV: '1'})
    code = 'from bfbc2core import dbx; print(dbx.backend())'
    forced = subprocess.run([sys.executable, '-c', code], env=env, capture_output=True, text=True, cwd=os.path.join(SUITE, 'core'))
    plain = subprocess.run([sys.executable, '-c', code], capture_output=True, text=True,
                           env={k: v for k, v in os.environ.items() if k != dbx.PURE_PYTHON_ENV}, cwd=os.path.join(SUITE, 'core'))
    check(forced.stdout.strip() == 'python' and plain.stdout.strip() == 'rust',
          f'{dbx.PURE_PYTHON_ENV}=1 forces the Python reader; without it the Rust core runs',
          f'{forced.stdout.strip()} / {plain.stdout.strip()} {forced.stderr[-200:]}')
    check(isinstance(NATIVE.threads(), int) and NATIVE.threads() >= 1 and isinstance(NATIVE.VERSION, str),
          f'bc2_native {NATIVE.VERSION}: {NATIVE.threads()} worker threads')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the corpus runs (no Project OPFOR BFBC2 game copy)')
    else:
        files = corpus_files(root)
        if '--game' in argv:
            run_corpus(files, check, 'corpus')
        else:
            sample = random.Random(5).sample(files, min(200, len(files)))
            run_corpus(sample, check, 'corpus sample')

    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
