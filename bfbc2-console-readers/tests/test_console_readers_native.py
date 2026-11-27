r"""Round F0: the console containers in the Rust core - STFS packages (bc2_native.stfs_open / StfsReader /
stfs_refusal) and XDVDFS discs (bc2_native.xiso_find_partition / xiso_walk, and since round RT3 their bytes entry
points xiso_find_partition_bytes / xiso_walk_bytes) - against the answers of their Python twins (stfs.py's
Package._open_reference / read_reference, xisolib.py's find_partition_reference / walk_reference), recorded in
console_readers_answers.json by record_console_readers_answers.py while the twins were there (round OR14): neither
twin is imported here, so the test runs once they are deleted.

Compared: every value a package keeps (magic, name, header size, separation, file table, base, allocated, levels,
the entries with their paths; types and order) and every entry's bytes, as one digest per package; every listed file
of a disc in order with the positions find_partition and walk leave, as one digest per disc, read from the file and
from memory. Where the twin raised, the core must refuse (stfs_open / xiso_find_partition / xiso_walk answer None:
the input the twin's error was raised on). A package whose folders never reach the root made the twin loop for ever:
recorded as such (found by the recorder without running the twin's path loop, and by stfs_refusal), the core must
refuse it as "cycle".

  python test_console_readers_native.py          quick: the packages of test_console_readers (read-only and with
                                                 two hash tables, both live-copy rules) and discs at each partition
                                                 offset, 60 damaged packages and 30 damaged discs
  python test_console_readers_native.py --game [--xbox <folder>]
                                                 + 1,500 damaged packages and 300 damaged discs; the Xbox disc image
                                                 and the three Update packages: every file listed and every package
                                                 entry read, against the twins' recorded digests; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import hashlib
import os
import random
import shutil
import struct
import sys
import tempfile
import time
from collections.abc import Callable, Iterator
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests'), HERE):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core')
import test_console_readers as tcr  # noqa: E402
from f0_parity import Checks, canon, stdout_utf8  # noqa: E402

READ_LIMIT = 4096
SECTOR = tcr.SECTOR
QUICK_PACKAGES, GAME_PACKAGES = 60, 1500
QUICK_DISCS, GAME_DISCS = 30, 300
BIG_COUNT = 0xFFFFFF
PKG_BASE = (0x971A + 0xFFF) & ~0xFFF
# the header's fields, the file table and the hash tables (level 0 and 1), the data
PKG_REGIONS = [(0, 0x1000), (0x330, 0x3A0), (0x400, 0x500), (PKG_BASE, PKG_BASE + 0x3000),
               (PKG_BASE + 0xAC * 0x1000, PKG_BASE + 0xAE * 0x1000), (PKG_BASE, PKG_BASE + 0x10000)]


# ------------------------------------------------------------------------------------------------ the inputs

def built_packages(work: str) -> list[tuple[str, str]]:
    """test_console_readers' packages with bytes from fixed seeds (the recorded answers' inputs): (label, path)."""
    out = []
    for k, (read_only, live) in enumerate(((True, None), (False, None), (False, 'separation'), (False, 'level1'))):
        label = f'pkg_{int(read_only)}_{live}'
        p = os.path.join(work, label)
        tcr.build_stfs(p, read_only, live, seed=0xF020 + 2 * k)
        out.append((label, p))
    return out


def built_discs(work: str) -> list[tuple[str, str, int]]:
    """test_console_readers' disc at each partition offset, bytes from fixed seeds: (label, path, offset)."""
    out = []
    for k, b in enumerate(tcr.PARTITIONS):
        label = f'disc_{b:x}'
        p = os.path.join(work, label + '.iso')
        tcr.build_xiso(p, b, seed=0xF030 + 2 * k)
        out.append((label, p, b))
    return out


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


def damaged_packages(pkgs: list[tuple[str, str]], dst: str, n: int) -> Iterator[str]:
    """n damaged copies of the built packages, one after the other at dst (the first n of the recorded ones)."""
    rng = random.Random(0xF02)
    for _ in range(n):
        damage(rng.choice(pkgs)[1], dst, rng, PKG_REGIONS)
        yield dst


def damaged_discs(discs: list[tuple[str, str, int]], dst: str, n: int) -> Iterator[str]:
    """n damaged copies of the small built discs (offset 0, and XGD2's: large but sparse), one after the other at dst."""
    rng = random.Random(0xF03)
    for _ in range(n):
        _, p, b = discs[rng.randrange(2)]
        damage(p, dst, rng, [(b + 32 * SECTOR, b + 32 * SECTOR + 28), (b + 33 * SECTOR, b + 36 * SECTOR)])
        yield dst


# ------------------------------------------------------------------------------------------------ the answers

def package_digest(values: list[Any], read: Callable[[dict[str, Any]], bytes | None]) -> str | None:
    """A package's answer: its values (PKG_FIELDS, canon) and every entry's bytes (read(e), entries up to READ_LIMIT
    blocks: one damaged to millions of blocks is listed, not read), as one digest; None when read gives None."""
    h = hashlib.sha256(repr(canon(values)).encode('utf-8'))
    for e in values[-1]:
        if e['dir'] or e['count'] > READ_LIMIT:
            continue
        b = read(e)
        if b is None:
            return None
        h.update(hashlib.sha256(b).digest())
    return h.hexdigest()[:24]


def disc_digest(base: int, after_find: int, files: list[Any], pos: int) -> str:
    """A disc's answer: the partition, the positions find_partition and walk leave, the files in order."""
    return hashlib.sha256(repr(canon(('ok', base, after_find, files, pos))).encode('utf-8')).hexdigest()[:24]


def root_of(path: str, base: int, data: bytes | None = None) -> tuple[int, int]:
    """The root directory's sector and size from the volume descriptor (struct.error where the image ends first)."""
    at = base + 32 * SECTOR + 20
    if data is not None:
        raw = data[at:at + 8]
    else:
        with open(path, 'rb') as f:
            f.seek(at)
            raw = f.read(8)
    sec, size = struct.unpack('<II', raw)
    return sec, size


def rust_package(path: str) -> list[Any]:
    """['ok', digest], ['refused', stfs_refusal's reason] or ['entry refused', path] (an entry the core does not read)."""
    nat = tcr.core()
    got = nat.stfs_open(path)
    if got is None:
        return ['refused', nat.stfs_refusal(path)]
    values, reader = list(got[:len(tcr.PKG_FIELDS)]), got[len(tcr.PKG_FIELDS)]
    missed: list[str] = []

    def read(e: dict[str, Any]) -> bytes | None:
        b = reader.read(e['start'], e['count'], e['consecutive'], e['size'])
        if b is None:
            missed.append(e['path'])
        return None if b is None else bytes(b)
    d = package_digest(values, read)
    return ['ok', d] if d is not None else ['entry refused', missed[0]]


def judge_package(recorded: list[Any], got: list[Any]) -> bool:
    """The core's answer as the twin's: the same digest; where the twin raised, a refusal (header, table or io);
    where it looped for ever, the refusal 'cycle'."""
    if recorded[0] == 'ok':
        return got == recorded
    if recorded[0] == 'cycle':
        return got == ['refused', 'cycle']
    return got[0] == 'refused' and got[1] in ('header', 'table', 'io')


def rust_disc(path: str, memory: bool = False) -> list[Any]:
    """['ok', digest], ['refused', 'find' | 'walk'] or ['error', 'root', type, message] (the descriptor cut short),
    the image read by path or (memory) from its bytes."""
    nat = tcr.core()
    data = open(path, 'rb').read() if memory else None
    base = nat.xiso_find_partition_bytes(data) if data is not None else nat.xiso_find_partition(path)
    if base is None:
        return ['refused', 'find']
    try:
        sec, size = root_of(path, base, data)
    except struct.error as ex:
        return ['error', 'root', type(ex).__name__, str(ex)]
    got = (nat.xiso_walk_bytes(data, base, sec, size, '') if data is not None else
           nat.xiso_walk(path, base, sec, size, ''))
    if got is None:
        return ['refused', 'walk']
    files, pos = got
    return ['ok', disc_digest(base, base + 32 * SECTOR + 20, list(files), pos)]


def judge_disc(recorded: list[Any], got: list[Any]) -> bool:
    """The core's answer as the twin's: the same digest or the same error of the test's own descriptor read; where
    the twin's find_partition or walk raised, the core refuses at the same step."""
    if recorded[0] == 'ok' or recorded[1] == 'root':
        return got == recorded
    return got == ['refused', recorded[1]]


class Tally:
    def __init__(self) -> None:
        self.same = self.ndiff = 0
        self.differ: list[str] = []
        self.kinds: dict[str, int] = {}

    def add(self, ok: bool, label: str, kind: str = 'ok') -> None:
        self.kinds[kind] = self.kinds.get(kind, 0) + 1
        if ok:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 10:
                self.differ.append(label)

    def counts(self) -> str:
        return ', '.join(f'{n} {k}' for k, n in sorted(self.kinds.items()))


def game_key(path: str) -> str:
    """A user's file in the recorded answers: its name and size (a changed file is not compared)."""
    return f'{os.path.basename(path)} {os.path.getsize(path)}'


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    tcr.core()
    rec = tcr.answers()
    c.check(True, 'the Rust core has the console readers (stfs_open / StfsReader / stfs_refusal, xiso_find_partition / '
            'xiso_walk and their bytes entry points)')
    work = tempfile.mkdtemp(prefix='f0_console_')
    try:
        pkgs = built_packages(work)
        t = Tally()
        for label, p in pkgs:
            got = rust_package(p)
            t.add(judge_package(rec['packages'][label], got) and got[0] == 'ok', f'{label}: {got}')
        c.check(not t.ndiff, f'built packages: {t.same} with the values and every entry\'s bytes stfs.py gave '
                '(recorded), read in Rust', '; '.join(t.differ))
        td = Tally()
        dst = os.path.join(work, 'damaged')
        recorded = rec['damaged_packages']
        for i, p in enumerate(damaged_packages(pkgs, dst, GAME_PACKAGES if game else QUICK_PACKAGES)):
            got = rust_package(p)
            td.add(judge_package(recorded[i], got), f'damaged package {i}: {recorded[i]} / {got}', recorded[i][0])
        c.check(not td.ndiff, f'damaged packages: {td.same} with stfs.py\'s values and bytes, or refused where it raised '
                f'or never finished (recorded: {td.counts()})', '; '.join(td.differ))

        discs = built_discs(work)
        tx = Tally()
        for label, p, _ in discs:
            a, m = rust_disc(p), rust_disc(p, memory=True)
            tx.add(a == m == rec['discs'][label], f'{label}: {rec["discs"][label]} / {a} / in memory {m}')
        c.check(not tx.ndiff, f'built discs at each partition offset: {tx.same} with xisolib.py\'s files, order and '
                'positions (recorded), read from the file and from memory', '; '.join(tx.differ))
        txd = Tally()
        recorded = rec['damaged_discs']
        for i, p in enumerate(damaged_discs(discs, dst, GAME_DISCS if game else QUICK_DISCS)):
            a, m = rust_disc(p), rust_disc(p, memory=True)
            txd.add(judge_disc(recorded[i], a) and a == m, f'damaged disc {i}: {recorded[i]} / {a} / in memory {m}',
                    recorded[i][0] if recorded[i][0] == 'ok' else f'error in {recorded[i][1]}')
        c.check(not txd.ndiff, f'damaged discs: {txd.same} with xisolib.py\'s files and positions, or refused at the '
                f'step where it raised (recorded: {txd.counts()}), read from the file and from memory (round RT3)',
                '; '.join(txd.differ))

        # an entry damaged to millions of blocks: the twin built every block's bytes before it cut at the size
        # (gigabytes for a chain that loops through the package); the core stops at the size: the same bytes as the
        # twin's own loop stopped there (recorded)
        pk = tcr.Package(pkgs[1][1])
        e = dict(next(x for x in pk.entries if x['path'] == 'Content/b.bin'))
        for consecutive in (False, True):
            e['consecutive'] = consecutive
            t0 = time.perf_counter()
            got_b = pk.read_or_none(e, BIG_COUNT)
            secs = time.perf_counter() - t0
            want = rec['big_entry']['consecutive' if consecutive else 'chained']
            have = None if got_b is None else [len(got_b), hashlib.sha256(got_b).hexdigest()]
            c.check(have == want and secs < 30, f'an entry damaged to {BIG_COUNT:,} {"consecutive" if consecutive else "chained"} '
                    f'blocks: its size in bytes, as stfs.py\'s loop reads them (recorded; {secs:.2f} s)', f'{have} / {want}')

        # round RT3: an image in memory is read by the Rust core (its bytes entry point), as the twin's walk_reference
        # read it from an io.BytesIO (recorded)
        nat = tcr.core()
        data = open(discs[0][1], 'rb').read()
        c.check(nat.xiso_find_partition_bytes(data) == 0 and rust_disc(discs[0][1], memory=True) == rec['discs'][discs[0][0]],
                'an image in memory (find_partition_bytes / walk_bytes): read in the Rust core, as xisolib.py read it '
                '(recorded)')

        if game:
            game_part(c, rec.get('game', {}), argv)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return c.done()


def game_part(c: Checks, recorded: dict[str, Any], argv: list[str]) -> None:
    """The user's disc image and Update packages against the twins' recorded digests (files of another name or size:
    not compared)."""
    xbox = argv[argv.index('--xbox') + 1] if '--xbox' in argv else tcr.XBOX_DEFAULT
    iso = os.path.join(xbox, 'Battlefield Bad Company 2 Limited Edition (XBOX 360).iso')
    if os.path.isfile(iso) and game_key(iso) in recorded:
        t0 = time.perf_counter()
        a = rust_disc(iso)
        c.check(a == recorded[game_key(iso)] and a[0] == 'ok', f'the Xbox disc image: every file listed, the order and '
                f'positions xisolib.py gave (recorded; Rust {time.perf_counter() - t0:.3f} s)', f'{a}')
    else:
        print(f'skip the disc image (not found or not recorded: {iso})')
    folder = os.path.join(xbox, 'Battlefield Bad Company 2 (3 DLCs Pack prototype)', '454108A8', '00000002')
    if not os.path.isdir(folder):
        print(f'skip the Update packages (not found: {folder})')
        return
    tg = Tally()
    tn = 0.0
    nbytes = 0
    skipped = []
    for f in sorted(os.listdir(folder)):
        p = os.path.join(folder, f)
        if game_key(p) not in recorded:
            skipped.append(f)
            continue
        t0 = time.perf_counter()
        a = rust_package(p)
        tn += time.perf_counter() - t0
        nbytes += os.path.getsize(p)
        tg.add(a == recorded[game_key(p)] and a[0] == 'ok', f'{f}: {a}')
    if skipped:
        print(f'not compared (not recorded): {", ".join(skipped)}')
    c.check(not tg.ndiff and tg.same == 3, f'the Update packages: {tg.same} read, every entry byte for byte as stfs.py '
            f'read it (recorded; {nbytes / 1e6:.0f} MB; Rust {tn:.2f} s)', '; '.join(tg.differ))


if __name__ == '__main__':
    sys.exit(main(sys.argv))
