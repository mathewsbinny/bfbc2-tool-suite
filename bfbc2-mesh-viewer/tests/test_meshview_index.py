r"""meshview.py's game-copy index without the game (quick tier): the bundles it indexes (stock levels and async
bundles, Package before Dist; Onslaught, OPFOR's and installed levels left out), the cache reused while every
bundle is unchanged, rebuilt when a bundle changes, and a corrupt or foreign cache file rebuilt rather than a crash.
Bundles here are stand-in files: unreadable bundles are skipped, so the index is empty but its cache is real.

  python test_meshview_index.py
"""
import contextlib
import io
import os
import pickle
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import meshview  # noqa: E402


def touch(path: str, data: bytes = b'not a bundle') -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data)


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
        cache = os.path.join(root, 'work', 'tools_out', 'meshview_index.pkl')
        rebuilt: bool | str                         # or the error a cache read raised
        ix, rebuilt = build()
        check(rebuilt and os.path.isfile(cache) and ix.where == {} and ix.parts == {} and ix.entries == {},
              'first run: the index is built (unreadable bundles skipped) and cached')
        ix, rebuilt = build()
        check(not rebuilt, 'second run: the cache is used')
        touch(os.path.join(g, 'Package', 'async', 'weapons.fbrb'), b'a changed bundle, of another size')
        ix, rebuilt = build()
        check(rebuilt, 'a changed bundle: the index is rebuilt')
        with open(cache, 'wb') as f:
            pickle.dump(['a foreign', 'pickle'], f)
        try:
            ix, rebuilt = build()
        except Exception as e:                          # noqa: BLE001 - the old cache reader raised TypeError
            rebuilt = f'{type(e).__name__}: {e}'
        check(rebuilt is True, 'a foreign cache file: rebuilt (it was a TypeError)', str(rebuilt))
        with open(cache, 'wb') as f:
            f.write(b'\x80\x04corrupt')
        try:
            ix, rebuilt = build()
        except Exception as e:                          # noqa: BLE001
            rebuilt = f'{type(e).__name__}: {e}'
        check(rebuilt is True, 'a corrupt cache file: rebuilt', str(rebuilt))
    finally:
        shutil.rmtree(root, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
