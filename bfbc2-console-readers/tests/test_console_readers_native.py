r"""Round F0: the console containers in the Rust core - STFS packages (stfs.Package: bc2_native.stfs_open /
StfsReader) and XDVDFS discs (xiso.find_partition / walk: bc2_native.xiso_find_partition / xiso_walk) - against
their Python, the reference that stays until the port passes a full test tier (Package._open_reference /
read_reference, find_partition_reference, walk_reference).

Compared: every value a Package keeps (magic, name, header size, separation, file table, base, allocated, levels,
the entries with their paths; types and order), every entry's bytes, every listed file of a disc in order with the
file position the walk leaves; for damaged inputs the same values or the same error. A package whose folders never
reach the root makes the reference loop for ever: the port refuses it (stfs_refusal says "cycle") and those are
counted, not run.

  python test_console_readers_native.py          quick: the packages of test_console_readers (read-only and with
                                                 two hash tables, both live-copy rules) and discs at each partition
                                                 offset, 60 damaged packages and 30 damaged discs
  python test_console_readers_native.py --game [--xbox <folder>]
                                                 + 1,500 damaged packages and 300 damaged discs; the Xbox disc image
                                                 and the three Update packages: every file listed and every package
                                                 entry read both ways; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import os
import random
import shutil
import struct
import sys
import tempfile
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests'), HERE):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core', 'xiso', 'stfs')
import stfs  # noqa: E402
import test_console_readers as tcr  # noqa: E402
import xiso  # noqa: E402
from bfbc2core import native  # noqa: E402
from f0_parity import Checks, canon, stdout_utf8  # noqa: E402

READ_LIMIT = 4096
PKG_FIELDS = ('magic', 'name', 'header_size', 'separation', 'ft_blocks', 'ft_start', 'base', 'allocated',
              'top_level', 'shift', 'entries')


def package(path: str, on: bool) -> tuple[Any, ...]:
    """('ok', values, every entry's bytes) or ('error', type, message), the core on or off."""
    stfs.use_native(on)
    p = None
    try:
        p = stfs.Package(path)
        values = canon([getattr(p, k) for k in PKG_FIELDS])
        # an entry damaged to millions of blocks is listed, not read (both ways would read for minutes)
        data = [p.read(e) for e in p.entries if not e['dir'] and e['count'] <= READ_LIMIT]
        native_read = p._reader is not None
        return ('ok', values, canon(data), native_read)
    except Exception as ex:          # noqa: BLE001 - the error is what is compared
        return ('error', type(ex).__name__, str(ex))
    finally:
        if p is not None:
            p.f.close()
        stfs.use_native(True)


def disc(path: str, on: bool) -> tuple[Any, ...]:
    xiso.use_native(on)
    try:
        with open(path, 'rb') as f:
            base = xiso.find_partition(f)
            after_find = f.tell()
            f.seek(base + 32 * xiso.SECTOR + 20)
            root_sec, root_size = struct.unpack('<II', f.read(8))
            files = list(xiso.walk(f, base, root_sec, root_size))
            return ('ok', base, after_find, canon(files), f.tell())
    except (Exception, SystemExit) as ex:          # noqa: BLE001 - the error is what is compared
        return ('error', type(ex).__name__, str(ex))
    finally:
        xiso.use_native(True)


class Tally:
    def __init__(self) -> None:
        self.same = self.ndiff = self.native = self.cycles = 0
        self.differ: list[str] = []

    def add(self, ok: bool, label: str) -> None:
        if ok:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 10:
                self.differ.append(label)


def compare_package(path: str, t: Tally, label: str) -> None:
    nat = native.require('stfs', 'stfs_open')
    if nat.stfs_refusal(path) == 'cycle':
        t.cycles += 1
        return
    a, b = package(path, True), package(path, False)
    if a[0] == 'ok' and a[3]:
        t.native += 1
    t.add(a[:3] == b[:3], f'{label}: {str(a[:2])[:120]} / {str(b[:2])[:120]}')


def damage(src: str, dst: str, rng: random.Random, regions: list[tuple[int, int]]) -> None:
    b = bytearray(open(src, 'rb').read())
    k = rng.randrange(3)
    if k in (1, 2):
        for _ in range(rng.randint(1, 4)):
            lo, hi = rng.choice([r for r in regions if r[0] < len(b)] or [(0, len(b))])
            at = rng.randrange(lo, min(hi, len(b)))
            b[at] = rng.randrange(256)
    if k in (0, 2):
        del b[rng.randrange(len(b)):]
    open(dst, 'wb').write(b)


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    native.require('stfs', 'stfs_open')
    c.check(stfs.use_native(True) and xiso.use_native(True), 'the Rust core runs stfs and xiso')
    rng = random.Random(0xF02)
    work = tempfile.mkdtemp(prefix='f0_console_')
    try:
        pkgs = []
        for read_only, live in ((True, None), (False, None), (False, 'separation'), (False, 'level1')):
            p = os.path.join(work, f'pkg_{int(read_only)}_{live}')
            tcr.build_stfs(p, read_only, live)
            pkgs.append(p)
        t = Tally()
        for p in pkgs:
            compare_package(p, t, os.path.basename(p))
        c.check(not t.ndiff and t.native == len(pkgs), f'built packages: {t.same} identical, every entry read '
                f'({t.native} in Rust)', '; '.join(t.differ))
        base = (0x971A + 0xFFF) & ~0xFFF
        # the header's fields, the file table and the hash tables (level 0 and 1), the data
        regions = [(0, 0x1000), (0x330, 0x3A0), (0x400, 0x500), (base, base + 0x3000), (base + 0xAC * 0x1000, base + 0xAE * 0x1000),
                   (base, base + 0x10000)]
        td = Tally()
        dst = os.path.join(work, 'damaged')
        for i in range(1500 if game else 60):
            damage(rng.choice(pkgs), dst, rng, regions)
            compare_package(dst, td, f'damaged package {i}')
        c.check(not td.ndiff, f'damaged packages: {td.same} the same values and bytes or the same error ({td.native} '
                f'answered in Rust, {td.cycles} with folders that never reach the root: not run, the reference '
                'never finishes)', '; '.join(td.differ))

        discs = []
        for b in xiso.PARTITIONS:
            p = os.path.join(work, f'disc_{b:x}.iso')
            tcr.build_xiso(p, b)
            discs.append((p, b))
        tx = Tally()
        for p, _ in discs:
            a, r = disc(p, True), disc(p, False)
            tx.add(a == r and a[0] == 'ok', os.path.basename(p))
        c.check(not tx.ndiff, f'built discs at each partition offset: {tx.same} identical (files, order, positions)',
                '; '.join(tx.differ))
        txd = Tally()
        for i in range(300 if game else 30):
            p, b = discs[rng.randrange(2)]                      # the small ones: 0 and XGD2's offset is large but sparse
            damage(p, dst, rng, [(b + 32 * 2048, b + 32 * 2048 + 28), (b + 33 * 2048, b + 36 * 2048)])
            a, r = disc(dst, True), disc(dst, False)
            txd.add(a == r, f'damaged disc {i}: {str(a)[:100]} / {str(r)[:100]}')
        c.check(not txd.ndiff, f'damaged discs: {txd.same} the same files and positions or the same error',
                '; '.join(txd.differ))

        # an entry damaged to millions of blocks: the reference builds every block's bytes before it cuts at the size
        # (gigabytes for a chain that loops through the package); the port stops at the size: the same bytes as the
        # reference's own loop stopped there
        pk = stfs.Package(pkgs[1])
        e: Any = dict(next(x for x in pk.entries if x['path'] == 'Content/b.bin'))
        for count, consecutive in ((0xFFFFFF, False), (0xFFFFFF, True)):
            e['count'], e['consecutive'] = count, consecutive
            want, b = bytearray(), e['start']
            for _ in range(count):
                if len(want) >= e['size']:
                    break
                pk.f.seek(pk._addr(b))
                want += pk.f.read(stfs.BLOCK)
                b = b + 1 if consecutive else pk._next(b)
                if consecutive and pk._addr(b) > os.path.getsize(pkgs[1]):
                    break
            t0 = time.perf_counter()
            got = pk.read(e)
            c.check(got == bytes(want[:e['size']]) and time.perf_counter() - t0 < 30,
                    f'an entry damaged to {count:,} {"consecutive" if consecutive else "chained"} blocks: its size in '
                    f'bytes, as the reference reads them ({time.perf_counter() - t0:.2f} s)')
        pk.f.close()

        # not a plain file: the Python
        import io
        data = open(discs[0][0], 'rb').read()
        bio = io.BytesIO(data)
        c.check(xiso.find_partition(bio) == 0 and xiso._path_of(bio) is None, 'a file object without a path: the Python')

        if game:
            xbox = argv[argv.index('--xbox') + 1] if '--xbox' in argv else tcr.XBOX_DEFAULT
            iso = os.path.join(xbox, 'Battlefield Bad Company 2 Limited Edition (XBOX 360).iso')
            if os.path.isfile(iso):
                disc(iso, True)                                 # the timings warm: the module and the file loaded
                disc(iso, False)
                t0 = time.perf_counter()
                a = disc(iso, True)
                t1 = time.perf_counter()
                r = disc(iso, False)
                t2 = time.perf_counter()
                n = len(a[3][1]) if a[0] == 'ok' else 0
                c.check(a == r and a[0] == 'ok', f'the Xbox disc image: {n} files listed both ways, the same order and '
                        f'positions (Python {t2 - t1:.3f} s, Rust {t1 - t0:.3f} s)')
            else:
                print(f'skip the disc image (not found: {iso})')
            folder = os.path.join(xbox, 'Battlefield Bad Company 2 (3 DLCs Pack prototype)', '454108A8', '00000002')
            if os.path.isdir(folder):
                tg = Tally()
                tn = tr = 0.0
                nbytes = 0
                for f in sorted(os.listdir(folder)):
                    p = os.path.join(folder, f)
                    t0 = time.perf_counter()
                    a = package(p, True)
                    t1 = time.perf_counter()
                    r = package(p, False)
                    t2 = time.perf_counter()
                    tn += t1 - t0
                    tr += t2 - t1
                    nbytes += os.path.getsize(p)
                    tg.add(a[:3] == r[:3] and a[0] == 'ok' and a[3], f)
                c.check(not tg.ndiff and tg.same == 3, f'the Update packages: {tg.same} read both ways, every entry '
                        f'byte for byte ({nbytes / 1e6:.0f} MB; Python {tr:.2f} s, Rust {tn:.2f} s)', '; '.join(tg.differ))
            else:
                print(f'skip the Update packages (not found: {folder})')
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
