r"""The Rust bundle packer (rust/bc2-formats fbrb::pack and zlib, through the bc2_native module) against FbrbTool's
pack, which it replaces: the same bytes from the same unpacked folder.

  python core\tests\test_fbrb_pack_native.py            quick: zlib's deflate against Python's zlib (levels 4-9,
                                                         built and random data), built folders in every layout
                                                         FbrbTool knows (zipped, align4096 / gap64, hdr, plen=size2,
                                                         size2=minus64, edited entries, old manifests, markers with
                                                         and without files, CR LF / BOM manifests), the refusals,
                                                         bfbc2core.fbrb.pack / pack_many;
                                                         8 small game bundles when the game copy is there
  python core\tests\test_fbrb_pack_native.py --game     + every bundle of the game copy (Project OPFOR BFBC2's game\,
                                                         read only): unpacked by FbrbTool into a temporary folder,
                                                         packed by FbrbTool and by Rust, the two bundles compared

Each game bundle is also compared with the game's own file: identical, or (for a zipped bundle DICE packed)
identical but for the two 10-byte gzip headers (DICE's xfl 0 / os 0x0b, FbrbTool's xfl 4 / os 0): the deflate
streams are zlib level 6 in all three. Needs the module; skipped (exit 0) when FbrbTool.exe is not built.
"""
import os
import random
import shutil
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
from bfbc2core import fbrb, native, paths  # noqa: E402

NATIVE: Any = native.load()
TOOL = os.path.join(SUITE, 'bfbc2-fbrb-tool', 'FbrbTool.exe')
SMALL = 8
EMPTY = 'identical but gzip headers and the empty payload stream DICE writes (FbrbTool writes none)'


def tool(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([TOOL, *args], capture_output=True, text=True, errors='replace')


def write(path: str, data: bytes | str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data.encode('utf-8') if isinstance(data, str) else data)


def both(folder: str, scratch: str) -> tuple[bytes | str, bytes | str]:
    """(FbrbTool's bundle or its failure, Rust's bundle or its failure) of folder."""
    a, b = os.path.join(scratch, 'tool.fbrb'), os.path.join(scratch, 'rust.fbrb')
    for p in (a, b):
        if os.path.exists(p):
            os.remove(p)
    r = tool('pack', folder, a)
    ta: bytes | str = open(a, 'rb').read() if r.returncode == 0 else 'error'
    try:
        NATIVE.fbrb_pack(folder, b)
        tb: bytes | str = open(b, 'rb').read()
    except (ValueError, OSError):
        tb = 'error'
    return ta, tb


# ------------------------------------------------------------------------------------------------ zlib

def zlib_cases(rng: random.Random) -> list[bytes]:
    out = [b'', b'a', b'ab', b'abc', b'\0' * 70_000, bytes(range(256)) * 300]
    for n in (10, 1000, 40_000, 140_000, 400_000):
        out.append(bytes(rng.randrange(256) for _ in range(n)))
        out.append(bytes(rng.choice(b'abcab \n') for _ in range(n)))
        words = [bytes(rng.randrange(97, 123) for _ in range(rng.randrange(2, 12))) for _ in range(50)]
        out.append(b' '.join(rng.choice(words) for _ in range(n // 6)))
        base = bytes(rng.randrange(256) for _ in range(300))
        out.append(b''.join(base[rng.randrange(250):] if rng.random() < 0.6 else bytes([rng.randrange(256)])
                            for _ in range(n // 40 + 1)))
    # matches far back (TOO_FAR, the window slide) and long runs across block ends
    out.append(b''.join(struct.pack('>I', i * 2654435761 & 0xFFFFFFFF) for i in range(60_000)) * 2)
    return out


def check_zlib(check: Any) -> None:
    rng = random.Random(1943)
    cases = zlib_cases(rng)
    bad = []
    for level in range(4, 10):
        for i, d in enumerate(cases):
            c = zlib.compressobj(level, zlib.DEFLATED, -15, 8)
            if NATIVE.zlib_deflate(d, level) != c.compress(d) + c.flush():
                bad.append((level, i, len(d)))
    check(not bad, f'zlib deflate: {len(cases)} inputs x levels 4-9 byte-identical to Python\'s zlib '
          f'({zlib.ZLIB_RUNTIME_VERSION})', f'first: {bad[:5]}')
    try:
        NATIVE.zlib_deflate(b'x', 3)
        check(False, 'zlib deflate refuses the levels not ported (1-3)')
    except ValueError:
        check(True, 'zlib deflate refuses the levels not ported (1-3)')


# ------------------------------------------------------------------------------------------------ built folders

LINE0 = '# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped='


def built_folders(root: str, rng: random.Random) -> list[tuple[str, str]]:
    """(label, folder) of hand-made unpacked folders, in every layout and rule FbrbTool's pack knows."""
    out: list[tuple[str, str]] = []

    def folder(label: str, first: str, rows: list[tuple[str, str, str, int | None, bytes | None]],
               eol: str = '\n', bom: bool = False) -> None:
        d = os.path.join(root, f'f{len(out)}')
        lines = [first]
        for name, typ, flags, size2, data in rows:
            if data is not None:
                write(os.path.join(d, *name.replace('\\', '/').split('/')), data)
            size = '' if data is None or size2 is None else f'|{len(data)}'
            lines.append(f'{name}|{typ}|{flags}|{size2 if size2 is not None else len(data or b"")}{size}')
        write(os.path.join(d, 'fbrb.manifest'), ('﻿' if bom else '') + eol.join(lines) + eol)
        out.append((label, d))

    blob = lambda n: bytes(rng.randrange(256) for _ in range(n))  # noqa: E731
    text = lambda n: bytes(rng.choice(b'Levels/MP_005 dbx ') for _ in range(n))  # noqa: E731
    common = [('Levels/A.dbx', '<non-resource>', '00010000', None, text(5000)),
              ('Gone', '*deleted*', '00000000', 0, None),
              ('Levels/B.res', 'RigidMeshSet', '00010000', None, blob(3000)),
              ('Levels/C.res', 'RigidMeshSet', '00000000', None, blob(10)),
              ('Gone2', '*deleted*', '00000000', 0, b'')]
    folder('zipped', LINE0 + '1', common)
    folder('stored', LINE0 + '0', common)
    folder('zipped, CR LF and a byte-order mark', LINE0 + '1', common, eol='\r\n', bom=True)
    folder('empty payload, zipped', LINE0 + '1', [('Gone', '*deleted*', '00000000', 0, None)])
    folder('no entries, zipped', LINE0 + '1', [])
    folder('no entries, stored', LINE0 + '0', [])
    for lay in ('align4096', 'gap64'):
        for hdr in (4096, 4, 1):
            folder(f'{lay} hdr={hdr} plen=size2 size2=minus64', LINE0 + f'0 layout={lay} hdr={hdr} plen=size2 size2=minus64',
                   [('a.res', 'T', '00010000', 3000 - 64, blob(3000)),
                    ('gone', '*deleted*', '00000000', 0, None),
                    ('b.res', 'T', '00010000', 16, blob(5000)),
                    ('c.res', 'U', '00010000', 4096 - 64, blob(4096))])
        folder(f'{lay} without other tokens', LINE0 + f'0 layout={lay}',
               [('a.res', 'T', '00010000', None, blob(100)), ('b.res', 'T', '00010000', None, blob(7000))])
    # b.res's size column: the size it unpacked to was 80, the file now has 5000 bytes (an edited entry)
    for label, f in out:
        if 'minus64' in label:
            m = open(os.path.join(f, 'fbrb.manifest'), encoding='utf-8').read().replace('b.res|T|00010000|16|5000',
                                                                                         'b.res|T|00010000|16|80')
            write(os.path.join(f, 'fbrb.manifest'), m)
    folder('kept size2 (unchanged entry), edited entry', LINE0 + '1',
           [('a.res', 'T', '00010000', 7, blob(100)), ('b.res', 'T', '00010000', 7, blob(100))])
    m = os.path.join(out[-1][1], 'fbrb.manifest')
    write(m, open(m, encoding='utf-8').read().replace('b.res|T|00010000|7|100', 'b.res|T|00010000|7|90'))
    folder('old manifest (no size column), size2 = size', LINE0 + '1', [('a.res', 'T', '0001', None, blob(50))])
    m = os.path.join(out[-1][1], 'fbrb.manifest')
    write(m, open(m, encoding='utf-8').read().replace('a.res|T|0001|50|50', 'a.res|T|0001|50'))
    folder('a marker with an empty file (older unpacks), names with backslashes', LINE0 + '1',
           [('x\\y.dbx', 'T', 'ffffffff', None, b'z'), ('m', '*deleted*', '0', 0, b''), ('n', '*deleted*', '0', 0, b'ab')])
    big = text(300_000) + blob(200_000)
    folder('a large zipped payload (several deflate blocks, window slides)', LINE0 + '1',
           [(f'e{i}.res', f'T{i % 3}', '00010000', None, big[i * 1000:i * 1000 + 40_000]) for i in range(30)])
    return out


def refused_folders(root: str) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []

    def folder(label: str, manifest: str, files: dict[str, bytes]) -> None:
        d = os.path.join(root, f'r{len(out)}')
        for n, b in files.items():
            write(os.path.join(d, n), b)
        write(os.path.join(d, 'fbrb.manifest'), manifest)
        out.append((label, d))

    folder('a missing file', LINE0 + '1\nmissing.res|T|0|1|1\n', {})
    folder('an entry outside the folder', LINE0 + '1\n../x.res|T|0|1|1\n', {'x.res': b'x'})
    folder('a rooted entry', LINE0 + '1\n\\x.res|T|0|1|1\n', {'x.res': b'x'})
    folder('an old manifest whose size2 differs', LINE0 + '1\nx.res|T|0|5\n', {'x.res': b'x'})
    folder('flags that are not hex', LINE0 + '1\nx.res|T|zz|1|1\n', {'x.res': b'x'})
    folder('a line with too few fields', LINE0 + '1\nx.res|T|0\n', {'x.res': b'x'})
    out.append(('no manifest', os.path.join(root, 'nothing-here')))
    return out


def check_built(check: Any) -> None:
    rng = random.Random(5)
    root = tempfile.mkdtemp(prefix='fp')
    try:
        scratch = os.path.join(root, 'out')
        os.makedirs(scratch)
        folders = built_folders(root, rng)
        bad = []
        for label, d in folders:
            a, b = both(d, scratch)
            if not isinstance(a, bytes) or a != b:
                bad.append(f'{label}: {"FbrbTool failed" if not isinstance(a, bytes) else "differs"}')
        check(not bad, f'{len(folders)} built folders: Rust\'s bundle byte-identical to FbrbTool\'s (zipped, stored, '
              'padded layouts, size2 rules, markers, old manifests)', '; '.join(bad[:4]))
        refused = refused_folders(root)
        bad = []
        for label, d in refused:
            a, b = both(d, scratch)
            if a != 'error' or b != 'error':
                bad.append(f'{label}: FbrbTool {"refused" if a == "error" else "packed"}, Rust {"refused" if b == "error" else "packed"}')
        check(not bad, f'{len(refused)} folders FbrbTool refuses: Rust refuses them too (ValueError / OSError)',
              '; '.join(bad))
        kinds = []
        for _label, d in refused:
            try:
                fbrb.pack(d, os.path.join(scratch, 'x.fbrb'))
            except OSError:
                kinds.append('OSError')
            except ValueError:
                kinds.append('ValueError')
        check(kinds == ['OSError', 'ValueError', 'ValueError', 'ValueError', 'ValueError', 'ValueError', 'OSError'],
              'fbrb.pack raises OSError for a missing file, ValueError for a manifest or name it cannot use', str(kinds))

        # fbrb.pack and pack_many (the Rust core: FbrbTool.exe is no fallback since round F1a)
        d = folders[0][1]
        tool('pack', d, os.path.join(scratch, 'want.fbrb'))
        want = open(os.path.join(scratch, 'want.fbrb'), 'rb').read()
        target = os.path.join(scratch, 'over.fbrb')
        write(target, b'an older bundle')
        got = fbrb.pack(d, target)
        e = fbrb.entries(target)
        check(got == (len(e), sum(x.size for x in e)) and open(target, 'rb').read() == want
              and not [n for n in os.listdir(scratch) if '.tmp-' in n],
              'fbrb.pack runs in Rust: FbrbTool\'s bytes over an older bundle, (entries, payload bytes), no temporary '
              'file left')
        many = fbrb.pack_many([(f, os.path.join(scratch, f'm{i}.fbrb')) for i, (_, f) in enumerate(folders[:4])]
                              + [(refused[0][1], os.path.join(scratch, 'bad.fbrb'))])
        check(all(isinstance(m, tuple) for m in many[:4]) and isinstance(many[4], OSError)
              and open(os.path.join(scratch, 'm0.fbrb'), 'rb').read() == want,
              'fbrb.pack_many: in order, a folder that cannot be packed gives its exception in its place')
    finally:
        shutil.rmtree(root, ignore_errors=True)


# ------------------------------------------------------------------------------------------------ game bundles

def _gz_payload_split(data: bytes) -> tuple[int, int]:
    """(start of the header's gzip stream, start of the payload) of a bundle."""
    cut = struct.unpack('>I', data[4:8])[0]
    return 8, 8 + cut


def same_but_gzip_headers(a: bytes, b: bytes) -> bool:
    """a and b equal once each gzip stream's 10-byte header is blanked (zipped bundles: the header stream and the
    payload stream)."""
    if len(a) != len(b) or a[:8] != b[:8]:
        return False
    h, p = _gz_payload_split(a)
    zipped = a[p:p + 2] == b'\x1f\x8b' and b[p:p + 2] == b'\x1f\x8b'
    blank = [(h, h + 10)] + ([(p, p + 10)] if zipped else [])
    aa, bb = bytearray(a), bytearray(b)
    for lo, hi in blank:
        aa[lo:hi] = bb[lo:hi] = bytes(hi - lo)
    return aa == bb


def repack_one(bundle: str) -> tuple[str, str, int, float, float]:
    """(bundle, outcome, size, FbrbTool pack seconds, Rust pack seconds): outcome 'identical', 'identical but gzip
    headers' (compared with the game's file), or what differs."""
    root = tempfile.mkdtemp(prefix='fp')
    try:
        d = os.path.join(root, 'u')
        r = tool('unpack', bundle, d)
        if r.returncode != 0:
            return bundle, f'FbrbTool unpack failed: {r.stderr.strip()[-200:]}', 0, 0, 0
        a, b = os.path.join(root, 'tool.fbrb'), os.path.join(root, 'rust.fbrb')
        t = time.perf_counter()
        r = tool('pack', d, a)
        t_tool = time.perf_counter() - t
        if r.returncode != 0:
            return bundle, f'FbrbTool pack failed: {r.stderr.strip()[-200:]}', 0, 0, 0
        t = time.perf_counter()
        try:
            NATIVE.fbrb_pack(d, b)
        except (ValueError, OSError) as e:
            return bundle, f'Rust pack failed: {e}', 0, 0, 0
        t_rust = time.perf_counter() - t
        ta, tb = open(a, 'rb').read(), open(b, 'rb').read()
        if ta != tb:
            k = next((i for i in range(min(len(ta), len(tb))) if ta[i] != tb[i]), min(len(ta), len(tb)))
            return bundle, f'differs from FbrbTool (sizes {len(ta)} / {len(tb)}, first difference at byte {k})', 0, 0, 0
        orig = open(bundle, 'rb').read()
        # a zipped bundle of markers only: DICE's payload is an empty 20-byte gzip stream, FbrbTool's (.NET) none
        empty = len(orig) == len(tb) + 20 and orig[len(tb):len(tb) + 2] == b'\x1f\x8b' \
            and zlib.decompressobj(-15).decompress(orig[len(tb) + 10:]) == b''
        outcome = 'identical' if orig == tb else 'identical but gzip headers' if same_but_gzip_headers(orig, tb) \
            else EMPTY if empty and same_but_gzip_headers(orig[:len(tb)], tb) \
            else 'identical to FbrbTool; the game\'s file differs beyond the gzip headers'
        return bundle, outcome, len(tb), t_tool, t_rust
    finally:
        shutil.rmtree(root, ignore_errors=True)


def game_bundles(root: str) -> list[str]:
    out = []
    for d, _, names in os.walk(os.path.join(root, 'game')):
        out += [os.path.join(d, n) for n in names if n.lower().endswith('.fbrb')]
    return sorted(out)


def run_bundles(bundles: list[str], check: Any, label: str) -> None:
    t = time.time()
    workers = max(1, min(6, (os.cpu_count() or 2) // 2))
    outcomes: Counter[str] = Counter()
    examples: dict[str, str] = {}
    total = 0
    t_tool = t_rust = 0.0
    order = sorted(bundles, key=lambda p: -os.path.getsize(p))      # the largest first: they set the wall time
    with ProcessPoolExecutor(workers) as pool:
        for bundle, outcome, size, a, b in pool.map(repack_one, order):
            outcomes[outcome] += 1
            examples.setdefault(outcome, bundle)
            total += size
            t_tool += a
            t_rust += b
    good = ('identical', 'identical but gzip headers', EMPTY,
            "identical to FbrbTool; the game's file differs beyond the gzip headers")
    for k, n in outcomes.items():
        print(f'       {n} x {k}' + ('' if k in good[:3] else f', e.g. {examples[k]}'))
    bad = sum(n for k, n in outcomes.items() if k not in good)
    check(not bad, f'{label}: {len(bundles)} bundles ({total / 1e9:.2f} GB) unpacked by FbrbTool and packed again: '
          f'Rust\'s bundle byte-identical to FbrbTool\'s for {len(bundles) - bad}; the same as the game\'s file '
          f'{outcomes["identical"]}, but for the gzip headers {outcomes["identical but gzip headers"]}  '
          f'(pack time FbrbTool {t_tool:.0f} s, Rust {t_rust:.0f} s; {time.time() - t:.0f} s wall, {workers} processes)')


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    native.require('the bundle packer', 'fbrb_pack')
    if not os.path.isfile(TOOL):
        print('skipped: FbrbTool.exe is not built (bfbc2-fbrb-tool\\build.bat): nothing to compare the packer with')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    check_zlib(check)
    check_built(check)
    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the game bundles (no Project OPFOR BFBC2 game copy)')
    else:
        bundles = game_bundles(root)
        if '--game' in argv:
            run_bundles(bundles, check, 'game copy')
        else:
            small = sorted(bundles, key=os.path.getsize)
            pick = random.Random(3).sample(small[:len(small) // 2], min(SMALL, len(small)))
            run_bundles(pick, check, f'{len(pick)} small game bundles')
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
