r"""Round F0: the game's localized texts (bc2loc.LocFile, khash) in the Rust core (bc2_native.loc_parse,
loc_histogram / LocTable, loc_decode, loc_write, loc_khash) against bc2loc's Python, the reference that stays until
the port passes a full test tier (LocFile._parse_reference, decode_reference, to_bytes_reference, khash_reference).

Also BIGF archives (menucommon.parse_bigf / apt.read_bigf: bc2_native.bigf_parse) against parse_bigf_reference: 200
built and 2,000 damaged archives, the menu / HUD movies of the game copy (the main menu's in quick, every set in
--game).

Compared: every public attribute of a LocFile (name, data, offsets, raw, order, strings, changed, table; their types,
order and values), every entry's decoded text, the bytes written (unchanged and after set), every hash; for damaged
inputs the same object or the same error.

  python test_bc2loc_native.py           quick: files and histograms built here (one- and two-byte characters,
                                         repeated hashes, offsets past the end, surrogates in the table), 2,000
                                         damaged copies, 3,000 keys hashed; the game copy's English file when found
  python test_bc2loc_native.py --game    + every language_*.bin of every layer of the game copy's overlay bundle and
                                         the pristine copy (texts, huge, numbers, histograms): both ways, every
                                         text decoded, written back unchanged and after set, 30 damaged copies of
                                         each; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import os
import random
import re
import struct
import sys
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core', 'menu-tools')
import bc2loc  # noqa: E402
import menucommon as mc  # noqa: E402
from bfbc2core import native  # noqa: E402
from f0_parity import Checks, canon, stdout_utf8  # noqa: E402


def state(loc: Any) -> Any:
    """A LocFile's public attributes, in order."""
    return canon({k: v for k, v in vars(loc).items() if not k.startswith('_')})


def make(data: bytes, hist: bytes | None, on: bool) -> tuple[Any, ...]:
    """('ok', LocFile) or ('error', type, message) with the core on or off."""
    bc2loc.use_native(on)
    try:
        return ('ok', bc2loc.LocFile(data, hist, 'test file'))
    except Exception as ex:          # noqa: BLE001 - the error is what is compared
        return ('error', type(ex).__name__, str(ex))
    finally:
        bc2loc.use_native(True)


class Tally:
    def __init__(self) -> None:
        self.same = self.ndiff = 0
        self.native_texts = self.ref_texts = 0
        self.differ: list[str] = []

    def add(self, ok: bool, label: str) -> None:
        if ok:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 10:
                self.differ.append(label)


def compare(data: bytes, hist: bytes | None, t: Tally, label: str, edit: bool = True) -> None:
    """One file both ways: the object or error, every text, the bytes unchanged and after set."""
    a, b = make(data, hist, True), make(data, hist, False)
    if a[0] != b[0] or (a[0] == 'error' and a != b):
        t.add(False, f'{label}: {a[:3]} / {b[:3]}')
        return
    if a[0] == 'error':
        t.add(True, label)
        return
    na, ref = a[1], b[1]
    ok = state(na) == state(ref)
    nat_table = na._native_table()[0] is not None
    texts = [(na.decode(na.raw[h]), ref.decode_reference(ref.raw[h])) for h in ref.order]
    ok = ok and all(x == y for x, y in texts)
    if nat_table:
        got = [na._native_table()[1].decode(na.raw[h]) if na.table is not None else native.require('bc2loc', 'loc_decode').loc_decode(na.raw[h])
               for h in ref.order]
        t.native_texts += sum(g is not None for g in got)
        t.ref_texts += sum(g is None for g in got)
    ok = ok and na.to_bytes() == ref.to_bytes_reference()
    if edit and ok:
        rng = random.Random(len(data))
        keys = [f'0x{rng.choice(ref.order):x}'] if ref.order and rng.random() < 0.5 else []
        keys += [f'OPFOR_F0_{rng.randrange(10 ** 6)}', 'NAME_M1']
        for k in keys:
            text = ''.join(chr(rng.randrange(1, 128)) for _ in range(rng.randrange(12)))
            ha, hb = na.set(k, text), ref.set(k, text)
            ok = ok and ha == hb
        ok = ok and na.to_bytes() == ref.to_bytes_reference() and state(na) == state(ref)
    t.add(ok, label)


# ------------------------------------------------------------------------------------------------ built inputs

def build(rng: random.Random) -> tuple[bytes, bytes | None]:
    """A random text file (sometimes with repeated hashes or offsets past the end) and histogram."""
    loc = bc2loc.LocFile(empty_file(), None)
    for _ in range(rng.randrange(40)):
        text = ''.join(chr(rng.randrange(1, 128)) for _ in range(rng.randrange(20)))
        loc.set(f'KEY_{rng.randrange(10 ** 6)}', text)
    data = bytearray(loc.to_bytes_reference())
    # high bytes in the texts (the histogram's characters)
    _, _, count, table, strings = struct.unpack_from('<5I', data, 0)
    for i in range(8 + strings, len(data)):
        if data[i] and rng.random() < 0.3:
            data[i] = rng.randrange(0x80, 0x100)
    if count and rng.random() < 0.3:                    # a repeated hash
        h, _ = struct.unpack_from('<II', data, 8 + table)
        struct.pack_into('<I', data, 8 + table + 8 * (count - 1), h)
    if count and rng.random() < 0.2:                    # an offset past the end
        struct.pack_into('<I', data, 8 + table + 4, len(data) + rng.randrange(100))
    hist = None
    if rng.random() < 0.8:
        n = rng.choice([0, 10, 256, 256 + 128 * 4])
        vals = []
        for _ in range(n):
            r = rng.random()
            vals.append(rng.randrange(0x80) if r < 0.4 else 0 if r < 0.5 else rng.choice([0xD800, 0xDFFF]) if r < 0.53
                        else rng.randrange(0x80, 0x10000))
        hist = struct.pack('<3I', 0x39001, 12 + 2 * n - 8, n) + struct.pack(f'<{n}H', *vals) + (b'\x01' if rng.random() < 0.2 else b'')
    return bytes(data), hist


def empty_file() -> bytes:
    return struct.pack('<5I', 0x39000, 28, 0, 28, 28) + b'NORMAL'.ljust(16, b'\0')


def damaged(data: bytes, rng: random.Random) -> bytes:
    b = bytearray(data)
    k = rng.randrange(3)
    if k in (0, 2) and b:
        del b[rng.randrange(len(b)):]
    if k in (1, 2) and b:
        for _ in range(rng.randint(1, 3)):
            b[rng.randrange(min(len(b), 64)) if rng.random() < 0.6 else rng.randrange(len(b))] = rng.randrange(256)
    return bytes(b)


def keys(rng: random.Random) -> list[Any]:
    out: list[Any] = ['', 'ID_M_OK', 'WIN32_PC_SERVER_BROWSER', 'café', '€ uro', '\ud800x', b'\xff\x00ab',
                      bytearray(b'abc'), [65, 66], 7]
    for _ in range(3000):
        r = rng.random()
        if r < 0.6:
            out.append(''.join(chr(rng.randrange(32, 127)) for _ in range(rng.randrange(40))))
        elif r < 0.9:
            out.append(''.join(chr(rng.randrange(1, 0x3000)) for _ in range(rng.randrange(10))))
        else:
            out.append(bytes(rng.randrange(256) for _ in range(rng.randrange(20))))
    return out


def outcome(fn: Any, *args: Any) -> tuple[Any, ...]:
    try:
        return ('ok', fn(*args))
    except Exception as ex:          # noqa: BLE001
        return ('error', type(ex).__name__, str(ex))


# ------------------------------------------------------------------------------------------------ the game's files

def game_files(all_files: bool) -> list[tuple[str, bytes, bytes | None]]:
    """(label, file, its histogram) of the overlay bundle's language files (English only unless all_files)."""
    try:
        root = mc.find_opfor()
    except (mc.MenuToolError, FileNotFoundError) as ex:
        print(f'skip the game files ({ex})')
        return []
    from bfbc2core import fbrb
    bundles = mc.bundle_layers(root, bc2loc.OVERLAY)
    pristine = os.path.join(root, 'original', 'Dist', 'win32', bc2loc.OVERLAY)
    if os.path.isfile(pristine):
        bundles.append(pristine)
    out: list[tuple[str, bytes, bytes | None]] = []
    for bundle in bundles:
        names = [e.name for e in fbrb.entries(bundle) if not e.deleted
                 and re.fullmatch(r'Localization/bin/language_\w+\.bin', e.name, re.I)]
        if not all_files:
            names = [n for n in names if 'english' in n.lower()]
        got = fbrb.read(bundle, names)
        lower = {k.lower(): v for k, v in got.items()}
        for name, data in sorted(got.items()):
            if name.lower().endswith('_histogram.bin'):
                out.append((f'{os.path.basename(bundle)}:{name}', data, None))
                continue
            hist = lower.get(re.sub(r'_(normal|huge|numbers)\.bin$', '_histogram.bin', name.lower()))
            out.append((f'{os.path.basename(bundle)}:{name}', data, hist))
        if not all_files and out:
            break
    return out


def bigf(members: list[tuple[str, bytes]], rng: random.Random) -> bytes:
    """A BIGF archive of the members (big-endian index, data after it; now and then a repeated name)."""
    index = b''.join(struct.pack('>II', 0, 0) + n.encode() + b'\0' for n, _ in members)
    pos = 16 + len(index)
    out_index, data = b'', b''
    for n, d in members:
        out_index += struct.pack('>II', pos + len(data), len(d)) + n.encode() + b'\0'
        data += d
    return b'BIGF' + struct.pack('>III', 16 + len(index) + len(data), len(members), 0) + out_index + data


def bigf_checks(c: Checks, rng: random.Random, game: bool) -> None:
    """menucommon.parse_bigf / apt.read_bigf (bc2_native.bigf_parse) against parse_bigf_reference."""
    nat = native.require('bc2loc', 'bigf_parse')
    built = []
    for _ in range(200):
        names = [rng.choice(['m.apt', 'm.const', 'x', '\u00e9t\u00e9.res', 'a' * rng.randrange(1, 30)]) for _ in range(rng.randrange(5))]
        built.append(bigf([(n, bytes(rng.randrange(256) for _ in range(rng.randrange(50)))) for n in names], rng))
    t = Tally()
    for i in range(2200):
        b = built[i] if i < 200 else damaged(rng.choice(built), rng)
        a, r = outcome(mc.parse_bigf, b), outcome(mc.parse_bigf_reference, b)
        t.add(a == r and (i >= 200 or nat.bigf_parse(b) is not None), f'bigf {i}: {str(a)[:80]} / {str(r)[:80]}')
    c.check(not t.ndiff, f'BIGF archives (menucommon.parse_bigf): 200 built and 2,000 damaged, {t.same} the same '
            'members or the same error', '; '.join(t.differ))
    try:
        root = mc.find_opfor()
    except (mc.MenuToolError, FileNotFoundError):
        return
    from bfbc2core import fbrb
    tg = Tally()
    nmov = 0
    for _key, rel in mc.MOVIE_SETS:
        layers = mc.bundle_layers(root, rel)
        by: dict[str, list[str]] = {}
        for _name, (bundle, entry) in mc.bundle_movies(layers).items():
            by.setdefault(bundle, []).append(entry)
        for bundle, entries in by.items():
            for entry, data in fbrb.read(bundle, entries).items():
                nmov += 1
                a, r = outcome(mc.parse_bigf, data), outcome(mc.parse_bigf_reference, data)
                tg.add(a == r and nat.bigf_parse(data) is not None, entry)
        if not game:
            break
    c.check(not tg.ndiff, f'{nmov} game menu / HUD movies: the same BIGF members both ways', '; '.join(tg.differ))


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    native.require('bc2loc', 'loc_parse')
    c.check(bc2loc.use_native(True), 'the Rust core runs bc2loc')
    rng = random.Random(0xF01)

    built = [build(rng) for _ in range(200)]
    t = Tally()
    for i, (data, hist) in enumerate(built):
        compare(data, hist, t, f'built {i}')
    c.check(not t.ndiff, f'built files both ways: {t.same} identical ({t.native_texts} texts decoded in Rust, '
            f'{t.ref_texts} left to the reference: a lone surrogate)', '; '.join(t.differ))

    td = Tally()
    for i in range(2000):
        data, hist = rng.choice(built)
        if hist is not None and rng.random() < 0.3:
            compare(data, damaged(hist, rng), td, f'damaged histogram {i}', edit=False)
        else:
            compare(damaged(data, rng), hist, td, f'damaged {i}', edit=False)
    c.check(not td.ndiff, f'damaged files and histograms: {td.same} the same object or the same error', '; '.join(td.differ))

    ks = keys(rng)
    same = sum(outcome(bc2loc.khash, k) == outcome(bc2loc.khash_reference, k) for k in ks)
    nat = native.require('bc2loc', 'loc_khash')
    in_rust = sum(nat.loc_khash(k) is not None for k in ks)
    c.check(same == len(ks), f'khash: {same} of {len(ks)} keys the same hash or error ({in_rust} hashed in Rust)')
    c.check(bc2loc.khash('X', 5) == bc2loc.khash_reference('X', 5), 'khash from another start: the Python')

    # a LocFile whose table is replaced decodes with the new one (the core's copy is not used)
    data, hist = next((d, h) for d, h in built if h is not None and len(h) > 600)
    loc = bc2loc.LocFile(data, hist)
    loc.table = tuple(0x41 for _ in loc.table or ())
    c.check(all(loc.decode(r) == loc.decode_reference(r) for r in loc.raw.values()), 'a replaced table: decoded with it')

    bigf_checks(c, rng, game)

    files = game_files(game)
    if files:
        tg = Tally()
        tgd = Tally()
        tn = tr = 0.0
        texts = 0
        for label, data, hist in files:
            if data[:4] == struct.pack('<I', 0x39000):              # a text file (not a histogram): timed
                t0 = time.perf_counter()
                a = bc2loc.LocFile(data, hist, label)
                [a.decode(r) for r in a.raw.values()]
                a.to_bytes()
                t1 = time.perf_counter()
                bc2loc.use_native(False)
                b = bc2loc.LocFile(data, hist, label)
                [b.decode_reference(r) for r in b.raw.values()]
                b.to_bytes_reference()
                t2 = time.perf_counter()
                bc2loc.use_native(True)
                tn += t1 - t0
                tr += t2 - t1
                texts += len(a.raw)
            compare(data, hist, tg, label)
            frng = random.Random(len(data))
            for k in range(30 if game else 5):
                compare(damaged(data, frng), hist, tgd, f'{label} damaged {k}', edit=False)
        c.check(not tg.ndiff, f'{len(files)} game localization files ({texts:,} texts) both ways: {tg.same} identical, '
                f'every text, written back and after set ({tg.native_texts:,} texts decoded in Rust, {tg.ref_texts} '
                'left to the reference)', '; '.join(tg.differ))
        c.check(not tgd.ndiff, f'damaged game files: {tgd.same} the same object or the same error', '; '.join(tgd.differ))
        print(f'timing (read, decode every text, write): Python {tr:.2f} s, Rust {tn:.2f} s')
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
