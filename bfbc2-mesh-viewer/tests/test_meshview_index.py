r"""meshview.py's game-copy index without the game (quick tier): the bundles it indexes (stock levels and async
bundles, Package before Dist; Onslaught, OPFOR's and installed levels left out), the cache reused while every
bundle is unchanged, rebuilt when a bundle changes, and a corrupt or foreign cache file rebuilt rather than a crash.
Bundles here are empty bundles made by the test, so the index is empty but its cache is real; an unreadable bundle
is said and its index not cached (suite deep scan 2026-11-15, C3). The cache is meshview_index.bin (round
PR1b-2-rest: bc2_gamefs::meshindex's format, the index built in Rust; the pickle retired).

  python test_meshview_index.py
"""
import contextlib
import gzip
import io
import os
import shutil
import struct
import sys
import tempfile
from collections.abc import Callable
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('mesh-viewer')
import meshview  # noqa: E402


def bundle(n: int = 0) -> bytes:
    """A readable bundle with n one-byte text entries (FbrbTool's layout)."""
    strings, table = bytearray(), bytearray()
    for i in range(n):
        off = len(strings)
        strings += f'e{i}.txt'.encode('ascii') + b'\0'
        table += struct.pack('>6I', off, 0, i, 1, 1, len(strings))
        strings += b'Text\0'
    header = struct.pack('>2I', 2, len(strings)) + strings + struct.pack('>I', n) + table + b'\x01' + struct.pack('>I', n)
    part1 = gzip.compress(bytes(header))
    return b'FbRB' + struct.pack('>I', len(part1)) + part1 + gzip.compress(b'x' * n)


EMPTY = bundle()


def touch(path: str, data: bytes = EMPTY) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data)


def partitions(root: str, check: Callable[..., None]) -> None:
    """Index.read's partition cache (meshview.Partitions): the same bytes as reading each entry alone, one bundle
    pass for all its partitions, a rewritten bundle read again, the least recently used dropped beyond the budget,
    an unreadable bundle read the old way."""
    from bfbc2core import fbrb
    g = os.path.join(root, 'game', 'Package', 'levels')
    items = {lv: [(f'levels/{lv}/p{i}.dbx', 'dbx', os.urandom(50 + i)) for i in range(20)]
             + [(f'levels/{lv}/t.res', 'ITexture', b'texture')] for lv in ('mp_001', 'mp_002')}
    for lv, its in items.items():
        touch(os.path.join(g, lv, 'level-00.fbrb'), fbrb.build(its))
    with contextlib.redirect_stderr(io.StringIO()):
        ix = meshview.Index(root, cache=os.path.join(root, 'ix2.bin'))
    reads: list[int] = []
    real = fbrb.read

    def counting(path: Any, wanted: Any) -> dict[str, bytes]:
        keys = list(wanted)
        reads.append(len(keys))
        return real(path, keys)
    meshview.fbrb.read = counting
    meshview.PARTITIONS = meshview.Partitions()
    try:
        names = [n for its in items.values() for n, _t, _b in its]
        got = {n: ix.read(n) for n in names}
        want = {n: b for its in items.values() for n, _t, b in its}
        check(got == want, 'cached reads: the same bytes as each entry alone')
        check(sorted(reads) == [1, 1, 20, 20], 'one pass per bundle for its partitions, .res entries alone', str(reads))
        reads.clear()
        check(ix.read('levels/mp_001/p3.dbx') == want['levels/mp_001/p3.dbx'] and reads == [], 'a second read: no pass')
        new = [(n, t, b[::-1]) for n, t, b in items['mp_001']]
        p = os.path.join(g, 'mp_001', 'level-00.fbrb')
        touch(p, fbrb.build(new))
        os.utime(p, ns=(os.stat(p).st_atime_ns, os.stat(p).st_mtime_ns + 10 ** 9))
        check(ix.read('levels/mp_001/p3.dbx') == new[3][2], 'a rewritten bundle is read again')
        meshview.PARTITIONS = meshview.Partitions(budget=1)
        ix.read('levels/mp_001/p1.dbx')
        ix.read('levels/mp_002/p1.dbx')
        check(len(meshview.PARTITIONS.bundles) == 1, 'beyond the budget the least recently used bundle is dropped')
        touch(p, b'not a bundle')
        try:
            ix.read('levels/mp_001/p1.dbx')
            check(False, 'an unreadable bundle: read alone, its error raised')
        except ValueError:
            check(True, 'an unreadable bundle: read alone, its error raised')
    finally:
        meshview.fbrb.read = real
        meshview.PARTITIONS = meshview.Partitions()


def main(argv: list[str]) -> int:
    failed = 0

    def check(ok: object, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    root = tempfile.mkdtemp(prefix='meshview_index_')
    try:
        g = os.path.join(root, 'game')
        for rel in ('Package/levels/mp_005/level.fbrb', 'Package/levels/ons_mp_002/level.fbrb',
                    'Package/levels/opfor_test/level.fbrb', 'Package/levels/wake/level.fbrb',
                    'Package/async/weapons.fbrb', 'Dist/win32/levels/mp_005/level.fbrb', 'Package/levels/mp_005/notes.txt'):
            touch(os.path.join(g, *rel.split('/')))
        touch(os.path.join(g, 'Package', 'levels', 'wake', 'opfor-wake-installed.json'), b'{}')
        got = [rel + '/' + os.path.basename(b) for rel, b in meshview._bundles(root)]
        want = ['async/weapons.fbrb', 'levels/mp_005/level.fbrb', 'levels/mp_005/level.fbrb']
        check(got == want, 'bundles indexed: stock levels and async, Package before Dist; Onslaught, OPFOR\'s and installed '
              'levels left out', str(got))
        check(meshview._bundles(root)[1][1].startswith(os.path.join(g, 'Package')), 'Package before Dist')

        def build() -> tuple[meshview.Index, bool]:
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                ix = meshview.Index(root)
            return ix, 'indexing the game copy' in err.getvalue()
        cache = os.path.join(root, 'work', 'tools_out', 'meshview_index.bin')
        check(not os.path.exists(os.path.join(root, 'work', 'tools_out', 'meshview_index.pkl')), 'no pickle')
        rebuilt: bool | str                         # or the error a cache read raised
        ix, rebuilt = build()
        check(rebuilt and os.path.isfile(cache) and ix.where == {} and ix.parts == {} and ix.entries == {},
              'first run: the index is built and cached')
        ix, rebuilt = build()
        check(not rebuilt, 'second run: the cache is used')
        touch(os.path.join(g, 'Package', 'async', 'weapons.fbrb'), bundle(1))
        ix, rebuilt = build()
        check(rebuilt and 'e0.txt' in ix.where, 'a changed bundle: the index is rebuilt')
        os.remove(cache)
        touch(os.path.join(g, 'Package', 'async', 'weapons.fbrb'), b'not a bundle')
        ix, rebuilt = build()
        check(rebuilt and not os.path.isfile(cache), 'an unreadable bundle: the index is built without it, not cached')
        ix, rebuilt = build()
        check(rebuilt, 'an unreadable bundle: the next run indexes again')
        touch(os.path.join(g, 'Package', 'async', 'weapons.fbrb'), bundle(1))
        ix, rebuilt = build()
        with open(cache, 'wb') as f:
            f.write(b'\x80\x04a foreign pickle')
        try:
            ix, rebuilt = build()
        except Exception as e:                          # noqa: BLE001 - the old cache reader raised TypeError
            rebuilt = f'{type(e).__name__}: {e}'
        check(rebuilt is True, 'a foreign cache file: rebuilt (it was a TypeError)', str(rebuilt))
        good = open(cache, 'rb').read()
        with open(cache, 'wb') as f:
            f.write(good[:-21] + bytes([good[-21] ^ 1]) + good[-20:])
        try:
            ix, rebuilt = build()
        except Exception as e:                          # noqa: BLE001
            rebuilt = f'{type(e).__name__}: {e}'
        check(rebuilt is True, 'a corrupt cache file: rebuilt', str(rebuilt))
        partitions(root, check)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
