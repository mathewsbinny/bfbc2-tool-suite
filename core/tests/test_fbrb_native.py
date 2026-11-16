r"""The Rust bundle reader (rust/bc2-formats fbrb, through the bc2_native module) against the Python reader it was
ported from (core/bfbc2core/fbrb.py), and against FbrbTool: the same entries, the same bytes, the same bundles refused.

  python core\tests\test_fbrb_native.py            quick: built and hand-made bundles, damaged and random ones, the
                                                    dispatcher and fallback, FbrbTool's unpack of the built bundles;
                                                    8 small game bundles when the game copy is there
  python core\tests\test_fbrb_native.py --game     + every bundle of the game copy (Project OPFOR BFBC2's game\, read
                                                    only) and of work\ww2: every entry of each read by both readers

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import gzip
import hashlib
import os
import random
import struct
import subprocess
import sys
import tempfile
import time
import zlib
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
from bfbc2core import dbx, fbrb, paths  # noqa: E402

NATIVE: Any = fbrb._with_fbrb(dbx._import_native())
TOOL = os.path.join(SUITE, 'bfbc2-fbrb-tool', 'FbrbTool.exe')
BIG = 150_000_000          # bundles larger than this are compared one at a time (each reader holds every entry)


# ------------------------------------------------------------------------------------------------ one bundle, both readers

def _outcome_py(path: str) -> tuple[str, Any]:
    try:
        with open(path, 'rb') as fh:
            ents, zipped, start = fbrb._header(fh)
        keys = sorted({e.name.lower() for e in ents if not e.deleted})
        data = fbrb._py_read(path, keys)
        return 'ok', ([tuple(vars(e).values()) for e in ents], zipped, start, [hashlib.sha1(data[k]).hexdigest() for k in keys])
    except KeyError:
        return 'KeyError', None
    except ValueError:
        return 'ValueError', None


def _outcome_rs(path: str) -> tuple[str, Any]:
    try:
        ents, zipped, start = NATIVE.fbrb_header(path, fbrb.Entry)
        keys = sorted({e.name.lower() for e in ents if not e.deleted})
        data = NATIVE.fbrb_read(path, keys, keys)
        return 'ok', ([tuple(vars(e).values()) for e in ents], zipped, start, [hashlib.sha1(d).hexdigest() for d in data])
    except KeyError:
        return 'KeyError', None
    except ValueError:
        return 'ValueError', None


def compare(path: str) -> tuple[str | None, int, int]:
    """(None or the kind of difference, entries, payload bytes read) for the bundle at path."""
    (pk, pv), (rk, rv) = _outcome_py(path), _outcome_rs(path)
    if pk != rk:
        return f'python {pk}, rust {rk}', 0, 0
    if pk == 'ok':
        if pv[0] != rv[0]:
            return 'entries differ', 0, 0
        if pv[1:3] != rv[1:3]:
            return 'zipped / payload start differ', 0, 0
        if pv[3] != rv[3]:
            return 'entry data differs', 0, 0
        return None, len(pv[0]), sum(e[4] for e in pv[0] if not (e[1] == fbrb.DELETED and e[4] == 0))
    return None, 0, 0


def compare_bytes(data: bytes, folder: str, n: int) -> str | None:
    p = os.path.join(folder, f'b{n}.fbrb')
    with open(p, 'wb') as fh:
        fh.write(data)
    return compare(p)[0]


def _chunk(files: list[str]) -> list[tuple[str, str | None, int, int]]:
    return [(f, *compare(f)) for f in files]


# ------------------------------------------------------------------------------------------------ bundles made here

def gz(data: bytes, flags: int = 0, extra: bytes = b'', name: bytes = b'', comment: bytes = b'', hcrc: bool = False) -> bytes:
    """A gzip member with the optional header fields asked for."""
    body = zlib.compressobj(6, zlib.DEFLATED, -15)
    raw = body.compress(data) + body.flush()
    f = flags | (4 if extra else 0) | (8 if name else 0) | (16 if comment else 0) | (2 if hcrc else 0)
    h = bytes([0x1F, 0x8B, 8, f, 0, 0, 0, 0, 0, 255])
    if extra:
        h += struct.pack('<H', len(extra)) + extra
    h += (name + b'\0' if name else b'') + (comment + b'\0' if comment else b'')
    if hcrc:
        h += struct.pack('<H', zlib.crc32(h) & 0xFFFF)
    return h + raw + struct.pack('<II', zlib.crc32(data), len(data) & 0xFFFFFFFF)


def header(rows: list[tuple[bytes, bytes, int, int, int, int]], zipped: bool, strings: bytes | None = None,
           plen: int = 0) -> bytes:
    """A decompressed header from (name, type, flags, offset, size, size2) rows."""
    table, offs, out = bytearray(), {}, bytearray()
    for name, typ, *_ in rows:
        for s in (name, typ):
            if s not in offs:
                offs[s] = len(table)
                table += s + b'\0'
    if strings is not None:
        table = bytearray(strings)
    for name, typ, flags, off, size, size2 in rows:
        out += struct.pack('>6I', offs.get(name, 0), flags, off, size, size2, offs.get(typ, 0))
    return struct.pack('>II', 2, len(table)) + bytes(table) + struct.pack('>I', len(rows)) + bytes(out) + bytes([zipped]) + struct.pack('>I', plen)


def bundle(head: bytes, payload: bytes, packed_head: bytes | None = None) -> bytes:
    hz = gzip.compress(head) if packed_head is None else packed_head
    return b'FbRB' + struct.pack('>I', len(hz)) + hz + payload


def built_bundles() -> list[tuple[str, bytes]]:
    rng = random.Random(1055)
    big = bytes(rng.randrange(256) for _ in range(300_000)) * 9        # 2.7 MB: crosses the 1 MiB read chunks
    items = [('Levels/Test/Test.dbx', '<non-resource>', b'{binary}' + bytes(range(200))),
             ('Shared/Gone.dbx', fbrb.DELETED, b''),
             ('Terrains/Test/Test.Heightfield-000.res', 'TerrainHeightfield', big),
             ('Levels/Test/Empty.txt', 'Text', b''),
             ('levels/test/test.DBX', 'Dup', b'the later entry of a name listed twice')]
    out = [(f'built zipped={z}', fbrb.build(items, z)) for z in (True, False)]
    rows = [(b'A\xe9\xff.dbx', b'T\x80', 0x10000, 0, 3, 3), (b'B', fbrb.DELETED.encode(), 0, 99, 0, 0),
            (b'C', b'T', 0, 1, 2, 2), (b'D', b'T', 0, 1_000_000, 0, 0)]
    out.append(('non-ASCII names, overlapping entries, a zero-size entry past the payload', bundle(header(rows, True), gz(b'xyz'))))
    out.append(('stored payload shorter than an entry', bundle(header([(b'A', b'T', 0, 0, 10, 10)], False), b'12345')))
    out.append(('zipped payload shorter than an entry', bundle(header([(b'A', b'T', 0, 0, 10, 10)], True), gz(b'12345'))))
    out.append(('header in two members with zero bytes between and after them',
                bundle(b'', b'', gz(header(rows, True)[:20]) + b'\0\0' + gz(header(rows, True)[20:]) + b'\0')))
    out.append(('header with name, extra, comment and checksum fields',
                bundle(b'', gz(b'xyz', name=b'p', extra=b'xx', comment=b'c', hcrc=True),
                       gz(header(rows, True), name=b'h.bin', extra=b'e', comment=b'hello', hcrc=True))))
    out.append(('payload header checksum wrong (zlib refuses, gzip.decompress would not)',
                bundle(header(rows, True), gz(b'xyz', hcrc=True)[:12] + b'\0\0' + gz(b'xyz', hcrc=True)[14:])))
    out.append(('payload reserved flags (zlib refuses)', bundle(header(rows, True), gz(b'xyz', flags=0x40))))
    out.append(('header reserved flags (gzip.decompress reads it)', bundle(b'', gz(b'xyz'), gz(header(rows, True), flags=0x40))))
    out.append(('header followed by junk', bundle(b'', gz(b'xyz'), gz(header(rows, True)) + b'junk')))
    out.append(('payload followed by junk', bundle(header(rows, True), gz(b'xyz') + b'junk' * 10)))
    out.append(('no entries', bundle(header([], True), b'')))
    out.append(('a name without its zero byte', bundle(header([(b'A', b'T', 0, 0, 0, 0)], False, strings=b'AT'), b'')))
    out.append(('not a bundle', b'not a bundle at all'))
    out.append(('magic only', b'FbRB'))
    out.append(('empty file', b''))
    out.append(('header length past the end', b'FbRB\xff\xff\xff\xff' + gzip.compress(b'abc')))
    out.append(('empty header', bundle(b'', b'', b'')))
    return out


def mutations(data: bytes, rng: random.Random, n: int) -> list[bytes]:
    """Damaged copies: cut short at many lengths, bytes changed, runs of 0xFF / 0x00 written, bytes cut out."""
    out = [data[:i] for i in sorted(set(range(0, min(len(data), 200))) | {len(data) * k // 40 for k in range(40)})]
    for _ in range(n):
        d = bytearray(data)
        for _ in range(rng.randint(1, 3)):
            i = rng.randrange(len(d)) if rng.random() < 0.5 else rng.randrange(min(len(d), 120))
            op = rng.randrange(3)
            if op == 0:
                d[i] = rng.randrange(256)
            elif op == 1:
                d[i:i + 4] = (b'\xff' if rng.random() < 0.5 else b'\0') * 4
            else:
                del d[i:i + rng.randint(1, 8)]
        out.append(bytes(d))
    return out


# ------------------------------------------------------------------------------------------------ game bundles

def game_bundles(root: str) -> list[str]:
    out = []
    for top in (os.path.join(root, 'game'), os.path.join(root, 'work', 'ww2')):
        for d, _, names in os.walk(top):
            out += [os.path.join(d, n) for n in names if n.lower().endswith('.fbrb')]
    return sorted(out)


def run_game(files: list[str], check: Any, label: str) -> None:
    t = time.time()
    small = [f for f in files if os.path.getsize(f) <= BIG]
    big = [f for f in files if os.path.getsize(f) > BIG]
    workers = max(1, min(6, (os.cpu_count() or 2) - 1))
    diffs: Counter[str] = Counter()
    examples: dict[str, str] = {}
    n_entries = n_bytes = 0
    for group, procs, size in ((small, workers, 4), (big, 1, 1)):
        chunks = [group[i:i + size] for i in range(0, len(group), size)]
        with ProcessPoolExecutor(procs) as pool:
            for rows in pool.map(_chunk, chunks):
                for f, diff, ne, nb in rows:
                    n_entries += ne
                    n_bytes += nb
                    if diff:
                        diffs[diff] += 1
                        examples.setdefault(diff, f)
    for k, n in diffs.items():
        print(f'       {n} x {k}, e.g. {examples[k]}')
    check(not diffs, f'{label}: {len(files)} bundles, {n_entries} entries, {n_bytes / 1e9:.2f} GB of entry data: the '
          f'same entries and bytes in both readers  ({time.time() - t:.0f} s)')


def fbrbtool_agrees(path: str, work: str) -> str | None:
    """FbrbTool's unpack of path against the Rust reader: the manifest's entries and every file's bytes."""
    out = os.path.join(work, 'u')
    r = subprocess.run([TOOL, 'unpack', path, out], capture_output=True, text=True)
    if r.returncode != 0:
        return f'FbrbTool unpack failed: {(r.stdout + r.stderr).strip()[-200:]}'
    lines = [l.rstrip('\r\n') for l in open(os.path.join(out, 'fbrb.manifest'), encoding='ascii', errors='replace')
             if l.strip() and l[0] != '#']
    ents, _, _ = NATIVE.fbrb_header(path, fbrb.Entry)
    if [l.split('|') for l in lines] != [[e.name, e.type, f'{e.flags:08X}', str(e.size2), str(e.size)] for e in ents]:
        return 'manifest differs from the entries'
    live = [e for e in ents if not e.deleted]
    data = NATIVE.fbrb_read(path, [e.name.lower() for e in live], [e.name.lower() for e in live])
    for e, d in zip(live, data):
        p = os.path.join(out, e.name.replace('/', os.sep))
        if open(p, 'rb').read() != d:
            return f'{e.name}: bytes differ from FbrbTool\'s'
    import shutil
    shutil.rmtree(out)
    return None


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

    work = tempfile.mkdtemp(prefix='fbrb_native_')
    built = built_bundles()
    bad = [name for i, (name, data) in enumerate(built) if compare_bytes(data, work, i)]
    kinds = Counter(_outcome_rs(os.path.join(work, f'b{i}.fbrb'))[0] for i in range(len(built)))
    check(not bad, f'{len(built)} built and hand-made bundles: the same entries, bytes and refusals '
          f'({kinds["ok"]} read, {kinds["ValueError"]} refused)', ', '.join(bad))

    rng = random.Random(1943)
    sources = [d for name, d in built if name.startswith(('built zipped=True', 'non-ASCII', 'header with'))]
    sources.append(fbrb.build([('A.dbx', 'T', bytes(range(256)) * 50), ('B.dbx', 'T', b'b' * 3000)], True))
    sources.append(fbrb.build([('A.dbx', 'T', bytes(range(256)) * 50), ('B.dbx', 'T', b'b' * 3000)], False))
    damaged = [m for d in sources for m in mutations(d, rng, 150)]
    bad_m, kinds = [], Counter()
    for i, m in enumerate(damaged):
        if compare_bytes(m, work, 1000):
            bad_m.append(i)
        kinds[_outcome_rs(os.path.join(work, 'b1000.fbrb'))[0]] += 1
    check(not bad_m, f'{len(damaged)} damaged bundles (cut short, bytes changed, runs written, bytes cut out): both '
          f'readers agree, no crash ({kinds["ok"]} read, {kinds["ValueError"]} refused)', f'first: {bad_m[:5]}')
    noise = [b'FbRB' + struct.pack('>I', rng.randrange(60)) + bytes(rng.randrange(256) for _ in range(rng.randrange(80)))
             for _ in range(300)]
    noise += [b'FbRB' + struct.pack('>I', 20 + rng.randrange(40)) + gzip.compress(bytes(rng.randrange(256) for _ in range(rng.randrange(60))))
              for _ in range(300)]
    bad_n = [i for i, m in enumerate(noise) if compare_bytes(m, work, 2000)]
    check(not bad_n, f'{len(noise)} random headers behind the magic: both readers agree', f'first: {bad_n[:5]}')

    # the errors as the Python reader raises them, for both backends
    errs = []
    for on in (True, False):
        fbrb.use_native(on)
        for i, m in enumerate(damaged[:300]):
            p = os.path.join(work, 'e.fbrb')
            with open(p, 'wb') as fh:
                fh.write(m)
            for fn in (lambda p=p: fbrb.entries(p), lambda p=p: fbrb.read(p, ['a.dbx'])):
                try:
                    fn()
                except (ValueError, KeyError):
                    pass
                except Exception as e:
                    errs.append(f'{fbrb.backend()} {i}: {type(e).__name__}: {e}')
    fbrb.use_native(True)
    check(not errs, 'entries / read raise ValueError or KeyError (never another exception) for a damaged bundle, '
          'in both readers', '; '.join(errs[:3]))
    junk = os.path.join(work, 'junk.fbrb')
    with open(junk, 'wb') as fh:
        fh.write(b'not a bundle at all')
    msgs = []
    for on in (True, False):
        fbrb.use_native(on)
        try:
            fbrb.entries(junk)
        except ValueError as e:
            msgs.append(str(e))
        try:
            fbrb.read(os.path.join(work, 'b0.fbrb'), ['Levels/Test/Test.dbx', 'Nope.dbx'])
        except KeyError as e:
            msgs.append(str(e))
        try:
            fbrb.entries(os.path.join(work, 'missing.fbrb'))
        except FileNotFoundError:
            msgs.append('FileNotFoundError')
    fbrb.use_native(True)
    check(len(msgs) == 6 and msgs[:3] == msgs[3:] and msgs[0] == f'{junk} is not an FbRB bundle',
          'the same messages and exception types in both readers (not a bundle, a missing entry, a missing file)', repr(msgs))

    # the dispatcher, read_many and the fallback
    b0 = os.path.join(work, 'b0.fbrb')
    want = ['Levels/Test/Test.dbx', 'levels/test/test.dbx', 'Terrains/Test/Test.Heightfield-000.res', 'levels/test/empty.txt']
    rs = fbrb.read(b0, want)
    items: list[tuple[str, list[str]]] = [(b0, want), (os.path.join(work, 'b1.fbrb'), want[2:]), (junk, []),
                                          (b0, ['nope']), (os.path.join(work, 'missing.fbrb'), [])]
    many_rs = fbrb.read_many(items)
    check(fbrb.backend() == 'rust' and fbrb.use_native(False) == 'python', 'use_native(False): the Python reader')
    py = fbrb.read(b0, want)
    many_py = fbrb.read_many(items)
    fbrb.use_native(True)
    same = all(type(a) is type(b) and (isinstance(a, Exception) or a == b) for a, b in zip(many_rs, many_py))
    check(rs == py and list(rs) == want and rs[want[0]] == rs[want[1]] == b'the later entry of a name listed twice'
          and same and len(many_rs) == len(items) and isinstance(many_rs[2], ValueError) and isinstance(many_rs[3], KeyError)
          and isinstance(many_rs[4], FileNotFoundError),
          'read / read_many: keys as given, a name listed twice reads its later entry, errors in place, the same in both')
    env = dict(os.environ, **{dbx.PURE_PYTHON_ENV: '1'})
    code = 'from bfbc2core import fbrb; print(fbrb.backend())'
    forced = subprocess.run([sys.executable, '-c', code], env=env, capture_output=True, text=True, cwd=os.path.join(SUITE, 'core'))
    plain = subprocess.run([sys.executable, '-c', code], capture_output=True, text=True,
                           env={k: v for k, v in os.environ.items() if k != dbx.PURE_PYTHON_ENV}, cwd=os.path.join(SUITE, 'core'))
    check(forced.stdout.strip() == 'python' and plain.stdout.strip() == 'rust',
          f'{dbx.PURE_PYTHON_ENV}=1 forces the Python reader; without it the Rust core runs',
          f'{forced.stdout.strip()} / {plain.stdout.strip()} {forced.stderr[-200:]}')
    check(fbrb._with_fbrb(None) is None and fbrb._with_fbrb(sys) is None,
          'a module without the bundle reader (built before it) is not used')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if os.path.isfile(TOOL):
        probs = [f'{name}: {why}' for i, (name, _) in enumerate(built[:2])
                 if (why := fbrbtool_agrees(os.path.join(work, f'b{i}.fbrb'), work))]
        sample = sorted(game_bundles(root), key=os.path.getsize)[100:108] if root else []
        probs += [f'{os.path.basename(s)}: {why}' for s in sample if (why := fbrbtool_agrees(s, work))]
        check(not probs, f'FbrbTool unpack agrees with the Rust reader: the built bundles and {len(sample)} game bundles '
              '(entries, types, flags, sizes; every file\'s bytes)', '; '.join(probs[:3]))
    else:
        print(f'skipped: FbrbTool unpack comparison ({TOOL} not built)')
    if root is None:
        print('skipped: the game runs (no Project OPFOR BFBC2 game copy)')
    else:
        files = game_bundles(root)
        if '--game' in argv:
            run_game(files, check, 'every bundle')
        else:
            run_game(sorted(files, key=os.path.getsize)[200:208], check, 'game sample')
    import shutil
    shutil.rmtree(work, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
