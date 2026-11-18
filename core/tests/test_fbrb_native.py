r"""The bundle reader (rust/bc2-formats fbrb, through the bc2_native module) against what the retired Python reader
(core/bfbc2core/fbrb.py's _header / _py_read, round F1a 2026-11-18) answered, recorded before it was deleted
(fbrb_golden.json), and against FbrbTool's unpack (in process; its files the ones FbrbTool.exe wrote, recorded before
it was retired: csharp_golden.py): the same entries, the same bytes, the same bundles refused.

  python core\tests\test_fbrb_native.py            quick: built and hand-made bundles, damaged and random ones, the
                                                    messages, read_many, FbrbTool's unpack of the built bundles;
                                                    8 small game bundles when the game copy is there
  python core\tests\test_fbrb_native.py --game     + every bundle of the game copy (Project OPFOR BFBC2's game\, read
                                                    only) and of work\ww2: every entry of each

A game bundle whose content the recording does not have (changed since: OPFOR's installs patch the game copy) is
counted, not compared. Needs the module (python rust\build_native.py).
"""
import gzip
import hashlib
import os
import random
import struct
import sys
import tempfile
import time
import zlib
from collections import Counter
from collections.abc import Callable
from concurrent.futures import ProcessPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, HERE)
import csharp_golden as cg  # noqa: E402
import goldens  # noqa: E402
from bfbc2core import fbrb, fbrbtool, native, paths  # noqa: E402

GOLDEN = os.path.join(HERE, 'fbrb_golden.json')
GROUP = 256_000_000        # a large bundle's entries are read in groups of about this many bytes (memory)

Header = tuple[list[fbrb.Entry], bool, int]
Reader = tuple[Callable[[str], Header], Callable[[str, list[str]], dict[str, bytes]]]


def _rust_header(path: str) -> Header:
    ents, zipped, start = native.require('the bundle reader').fbrb_header(path, fbrb.Entry)
    return list(ents), bool(zipped), int(start)


def _rust_read(path: str, keys: list[str]) -> dict[str, bytes]:
    return fbrb.read_uncached(path, keys)


RUST: Reader = (_rust_header, _rust_read)


# ------------------------------------------------------------------------------------------------ one bundle

def digest(path: str, reader: Reader = RUST) -> tuple[str, int, int]:
    """(the digest of what the reader answers for the bundle - its entries, zipped flag, payload start and every
    live entry's data, or the exception's type -, entries, payload bytes)."""
    header, read = reader
    try:
        ents, zipped, start = header(path)
        live = {e.name.lower(): e for e in ents if not e.deleted}
        keys = sorted(live)
        shas: dict[str, str] = {}
        group: list[str] = []
        size = 0
        for k in sorted(keys, key=lambda k: live[k].offset + live[k].size):    # the payload's order, in groups
            group.append(k)
            size += live[k].size
            if size >= GROUP:
                shas.update((n, hashlib.sha1(d).hexdigest()) for n, d in read(path, group).items())
                group, size = [], 0
        if group or not keys:
            shas.update((n, hashlib.sha1(d).hexdigest()) for n, d in read(path, group).items())
        value = ([tuple(vars(e).values()) for e in ents], zipped, start, [shas[k] for k in keys])
        n_bytes = sum(e.size for e in ents if not e.deleted)
        return goldens.rh(('ok', value), 16), len(ents), n_bytes
    except (KeyError, ValueError) as e:
        return type(e).__name__, 0, 0


def digest_bytes(data: bytes, folder: str, n: int, reader: Reader = RUST) -> str:
    p = os.path.join(folder, f'b{n}.fbrb')
    with open(p, 'wb') as fh:
        fh.write(data)
    return digest(p, reader)[0][:10]


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
    hz = gzip.compress(head, mtime=0) if packed_head is None else packed_head
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
    out.append(('header length past the end', b'FbRB\xff\xff\xff\xff' + gzip.compress(b'abc', mtime=0)))
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


def quick_cases() -> dict[str, list[bytes]]:
    """The quick tier's bundles, in the recording's order: built, damaged (seeded), random headers (seeded)."""
    built = built_bundles()
    rng = random.Random(1943)
    sources = [d for name, d in built if name.startswith(('built zipped=True', 'non-ASCII', 'header with'))]
    sources.append(fbrb.build([('A.dbx', 'T', bytes(range(256)) * 50), ('B.dbx', 'T', b'b' * 3000)], True))
    sources.append(fbrb.build([('A.dbx', 'T', bytes(range(256)) * 50), ('B.dbx', 'T', b'b' * 3000)], False))
    damaged = [m for d in sources for m in mutations(d, rng, 150)]
    noise = [b'FbRB' + struct.pack('>I', rng.randrange(60)) + bytes(rng.randrange(256) for _ in range(rng.randrange(80)))
             for _ in range(300)]
    noise += [b'FbRB' + struct.pack('>I', 20 + rng.randrange(40)) + gzip.compress(bytes(rng.randrange(256) for _ in range(rng.randrange(60))), mtime=0)
              for _ in range(300)]
    return {'built': [d for _, d in built], 'damaged': damaged, 'noise': noise}


def api_outcomes(work: str, read: Callable[[str, list[str]], dict[str, bytes]],
                 read_many: Callable[[list[tuple[str, list[str]]]], list[Any]],
                 entries: Callable[[str], list[fbrb.Entry]]) -> dict[str, Any]:
    """The messages and read / read_many's answers on the built bundles (written to work as b0, b1 ...)."""
    junk = os.path.join(work, 'junk.fbrb')
    with open(junk, 'wb') as fh:
        fh.write(b'not a bundle at all')
    b0 = os.path.join(work, 'b0.fbrb')
    msgs = []
    for fn in (lambda: entries(junk), lambda: read(b0, ['Levels/Test/Test.dbx', 'Nope.dbx']),
               lambda: entries(os.path.join(work, 'missing.fbrb'))):
        try:
            fn()
            msgs.append('none')
        except KeyError as e:
            msgs.append(f'KeyError: {e.args[0]}'.replace(work, '<work>'))
        except OSError as e:                          # the operating system's words: the type only
            msgs.append(type(e).__name__)
        except ValueError as e:
            msgs.append(f'ValueError: {e}'.replace(work, '<work>'))
    want = ['Levels/Test/Test.dbx', 'levels/test/test.dbx', 'Terrains/Test/Test.Heightfield-000.res', 'levels/test/empty.txt']
    rs = read(b0, want)
    items: list[tuple[str, list[str]]] = [(b0, want), (os.path.join(work, 'b1.fbrb'), want[2:]), (junk, []),
                                          (b0, ['nope']), (os.path.join(work, 'missing.fbrb'), [])]
    many = [type(r).__name__ if isinstance(r, Exception) else goldens.rh(sorted(r.items())) for r in read_many(items)]
    return {'messages': msgs, 'read': [list(rs), goldens.rh(sorted(rs.items()))], 'read_many': many}


# ------------------------------------------------------------------------------------------------ game bundles

def game_bundles(root: str) -> list[str]:
    out = []
    for top in (os.path.join(root, 'game'), os.path.join(root, 'work', 'ww2')):
        for d, _, names in os.walk(top):
            out += [os.path.join(d, n) for n in names if n.lower().endswith('.fbrb')]
    return sorted(out)


def game_chunk(job: tuple[list[str], str]) -> list[tuple[str, str, str, int, int]]:
    """(path relative to root, file SHA-1, digest, entries, bytes) per bundle."""
    files, root = job
    out = []
    for f in files:
        d, n, b = digest(f)
        out.append((os.path.relpath(f, root), goldens.file_sha(f)[:16], d, n, b))
    return out


def run_game(files: list[str], root: str, check: Any, label: str, golden: dict[str, list[Any]]) -> None:
    t = time.time()
    workers = max(1, min(3, (os.cpu_count() or 2) - 1))
    differ: list[str] = []
    changed = n_entries = n_bytes = 0
    jobs = [(files[i:i + 4], root) for i in range(0, len(files), 4)]
    with ProcessPoolExecutor(workers) as pool:
        for rows in pool.map(game_chunk, jobs):
            for rel, sha, d, ne, nb in rows:
                n_entries += ne
                n_bytes += nb
                want = golden.get(rel.replace(os.sep, '/'))
                if want is None or want[0] != sha:
                    changed += 1
                elif want[1] != d:
                    differ.append(rel)
    check(not differ, f'{label}: {len(files)} bundles, {n_entries} entries, {n_bytes / 1e9:.2f} GB of entry data: the '
          f'entries and bytes the Python reader gave on {len(files) - changed - len(differ)} ({changed} changed since '
          f'the recording or not in it)  ({time.time() - t:.0f} s)', f'{len(differ)} differ: {differ[:3]}')


def fbrbtool_agrees(path: str, work: str, want: str | None) -> str | None:
    """FbrbTool's unpack of path (in process; its files the ones FbrbTool.exe wrote: want, the folder's recorded
    digest) against the reader: the manifest's entries and every file's bytes."""
    out = os.path.join(work, 'u')
    r = fbrbtool.run(['unpack', path, out])
    if r.returncode != 0:
        return f'FbrbTool unpack failed: {(r.stdout + r.stderr).strip()[-200:]}'
    if want is None:
        return 'FbrbTool.exe\'s unpack of it was not recorded'
    if cg.tree_digest(out) != want:
        return 'the unpacked files differ from FbrbTool.exe\'s'
    lines = [l.rstrip('\r\n') for l in open(os.path.join(out, 'fbrb.manifest'), encoding='ascii', errors='replace')
             if l.strip() and l[0] != '#']
    ents = fbrb.entries(path)
    if [l.split('|') for l in lines] != [[e.name, e.type, f'{e.flags:08X}', str(e.size2), str(e.size)] for e in ents]:
        return 'manifest differs from the entries'
    live = [e for e in ents if not e.deleted]
    data = fbrb.read(path, [e.name.lower() for e in live])
    for e in live:
        p = os.path.join(out, e.name.replace('/', os.sep))
        if open(p, 'rb').read() != data[e.name.lower()]:
            return f'{e.name}: bytes differ from FbrbTool\'s'
    import shutil
    shutil.rmtree(out)
    return None


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    native.require('the bundle reader', 'fbrb_read_many')
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    rec = goldens.load(GOLDEN)
    work = tempfile.mkdtemp(prefix='fbrb_native_')
    for name, items in quick_cases().items():
        got = [digest_bytes(d, work, i if name == 'built' else 5000) for i, d in enumerate(items)]
        want = rec['cases'][name]
        bad = [i for i, (a, b) in enumerate(zip(got, want)) if a != b]
        kinds = Counter('refused' if g in ('ValueError', 'KeyError') else 'read' for g in got)
        check(len(got) == len(want) and not bad, f'{len(items)} {name} bundles: the entries, bytes and refusals the '
              f'Python reader gave ({kinds["read"]} read, {kinds["refused"]} refused)', f'first: {bad[:5]}')
    for i, d in enumerate(quick_cases()['built']):          # b0, b1 ... again (the damaged cases wrote b5000)
        digest_bytes(d, work, i)

    # errors: ValueError or KeyError, never another exception
    errs = []
    for i, m in enumerate(quick_cases()['damaged'][:300]):
        p = os.path.join(work, 'e.fbrb')
        with open(p, 'wb') as fh:
            fh.write(m)
        for fn in (lambda p=p: fbrb.entries(p), lambda p=p: fbrb.read(p, ['a.dbx'])):
            try:
                fn()
            except (ValueError, KeyError):
                pass
            except Exception as e:
                errs.append(f'{i}: {type(e).__name__}: {e}')
    check(not errs, 'entries / read raise ValueError or KeyError (never another exception) for a damaged bundle',
          '; '.join(errs[:3]))
    got_api = api_outcomes(work, fbrb.read, fbrb.read_many, fbrb.entries)
    check(got_api == rec['api'], 'the messages (not a bundle, a missing entry, a missing file), read\'s keys as given '
          '(a name listed twice reads its later entry) and read_many\'s results in place: the Python reader\'s',
          f'{got_api} / {rec["api"]}')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    golden = cg.load('fbrbtool_golden.json').get('reader_built', {})
    recorded = cg.load('fbrbtool_golden_game.json').get('bundles', {})
    labels = [label for label, _ in built_bundles()[:2]]
    probs = [f'b{i}: {why}' for i in range(2)
             if (why := fbrbtool_agrees(os.path.join(work, f'b{i}.fbrb'), work, golden.get(labels[i])))]
    sample = sorted(game_bundles(root), key=os.path.getsize)[100:108] if root else []
    probs += [f'{os.path.basename(s)}: {why}' for s in sample
              if (why := fbrbtool_agrees(s, work, recorded.get(cg.sha1_file(s), {}).get('unpack', [None] * 4)[3]))]
    check(not probs, f'FbrbTool\'s unpack (the files FbrbTool.exe wrote: fbrbtool_golden*.json) agrees with the '
          f'reader: the built bundles and {len(sample)} game bundles (entries, types, flags, sizes; every file\'s '
          'bytes)', '; '.join(probs[:3]))
    if root is None:
        print('skipped: the game runs (no Project OPFOR BFBC2 game copy)')
    else:
        files = game_bundles(root)
        if '--game' in argv:
            run_game(files, root, check, 'every bundle', rec['game'])
        else:
            run_game(sorted(files, key=os.path.getsize)[200:208], root, check, 'game sample', rec['game'])
    import shutil
    shutil.rmtree(work, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
