r"""Round F2b (rust/BENCHMARKS.md, "F2b"): menucommon.PEImage's header, strings, find_cstring and find_dword in the
Rust core (bc2_harness::pe::raw) against the Python reference - the same sections, the same strings in the same
insertion order with the same addresses, the same places, the same errors.

  python test_pe_native.py          quick: 400 seeded built executables (sections of code and data, printable runs
                                    of every length around the minimum, NULs at section ends, sections past the
                                    file, cut and damaged headers)
  python test_pe_native.py --game   + every .exe and .dll of the game copy (BFBC2Game.exe and the others): the
                                    header, strings(1 - 4), 200 C strings and 200 dwords each

The module is required (the test fails without it, saying how to build it).
"""
import os
import random
import struct
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('menu-tools', 'shader-tools', 'core')
import menucommon as mc  # noqa: E402

FAILS: list[str] = []
COUNT = {'same': 0, 'differ': 0}


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def outcome(f, *a):
    try:
        r = f(*a)
        return ('ok', list(r.items()) if isinstance(r, dict) else r)
    except Exception as e:                                       # noqa: BLE001 - compared: both must agree
        return ('err', type(e).__name__, str(e))


def reference(path):
    img = mc.PEImage.__new__(mc.PEImage)
    img.path = path
    img.b = open(path, 'rb').read()
    img._read_reference(path, img.b)
    return img


def compare(path, rnd, n_queries, label, bad):
    got = outcome(mc.PEImage, path)
    want = outcome(reference, path)
    if got[0] != want[0] or (got[0] == 'err' and got != want):
        bad.append((label, 'open', got, want))
        return
    if got[0] == 'err':
        COUNT['same'] += 1
        return
    a, b = got[1], want[1]
    pairs = [((a.timestamp, a.base, a.sections), (b.timestamp, b.base, b.sections))]
    for m in (1, 2, 3, 4):
        pairs.append((outcome(a.strings, m), outcome(b.strings_reference, m)))
    strs = list(b.strings_reference(3))
    for t in rnd.sample(strs, min(n_queries, len(strs))) + ['', 'nothereé', 'x']:
        pairs.append((outcome(a.find_cstring, t), outcome(b.find_cstring_reference, t)))
    data = a.b
    for _ in range(n_queries):
        o = rnd.randrange(max(len(data) - 4, 1))
        v = int.from_bytes(data[o:o + 4], 'little') if rnd.random() < 0.8 else rnd.choice([0, 1 << 32, -1, 7])
        secs = None if rnd.random() < 0.8 else a.sections
        pairs.append((outcome(a.find_dword, v, secs), outcome(b.find_dword_reference, v, secs)))
    for x, y in pairs:
        if x == y:
            COUNT['same'] += 1
        else:
            COUNT['differ'] += 1
            bad.append((label, repr(x)[:200], repr(y)[:200]))


def built(rnd):
    """A PE32 file of random sections, or a damaged one."""
    nsec = rnd.randint(1, 5)
    pe = 0x80
    opt = 0xE0
    head = bytearray(0x400)
    head[:2] = b'MZ'
    struct.pack_into('<I', head, 0x3C, pe)
    head[pe:pe + 4] = b'PE\0\0'
    struct.pack_into('<HH', head, pe + 4, 0x14C, nsec)
    struct.pack_into('<I', head, pe + 8, rnd.getrandbits(32))
    struct.pack_into('<H', head, pe + 20, opt)
    struct.pack_into('<H', head, pe + 24, 0x10B)
    struct.pack_into('<I', head, pe + 52, rnd.choice([0x400000, 0x10000000, 0xFFFF0000]))
    body = bytearray()
    at = len(head)
    for i in range(nsec):
        size = rnd.choice([0x200, 0x400, 0x1000])
        data = bytearray()
        while len(data) < size:
            r = rnd.random()
            if r < 0.4:
                data += bytes(rnd.choice([0x20, 0x41, 0x7E, 0x61]) + rnd.randrange(20) for _ in range(rnd.randint(0, 9))) \
                    + b'\0'
            elif r < 0.7:
                data += rnd.randbytes(rnd.randint(1, 8))
            else:
                data += struct.pack('<I', rnd.choice([0x401000, 0x401004, 7, 0]))
        data = data[:size]
        if rnd.random() < 0.3:
            data[-1] = 0x41
        name = rnd.choice([b'.text', b'.rdata', b'.data', b'.rsrc', b'.tls', b'abc\0d', b'12345678'])
        o = pe + 24 + opt + 40 * i
        head[o:o + 8] = name.ljust(8, b'\0')[:8]
        rs = size if rnd.random() < 0.9 else size + 0x10000
        struct.pack_into('<4I', head, o + 8, size, 0x1000 * (i + 1), rs, at)
        struct.pack_into('<I', head, o + 36, rnd.choice([0x60000020, 0x40000040, 0xC0000040]))
        body += data
        at += size
    b = bytes(head + body)
    r = rnd.random()
    if r < 0.05:
        b = b[:rnd.randrange(len(b))]
    elif r < 0.1:
        b = b[:2] + b'\0' * 2 + b[4:] if rnd.random() < 0.5 else b.replace(b'PE\0\0', b'PX\0\0', 1)
    elif r < 0.13:
        b = bytearray(b)
        struct.pack_into('<H', b, pe + 24, 0x20B)
        b = bytes(b)
    return b


def quick():
    rnd = random.Random(29)
    bad: list = []
    with tempfile.TemporaryDirectory() as d:
        for i in range(400):
            p = os.path.join(d, f'f{i}.exe')
            with open(p, 'wb') as fh:
                fh.write(built(rnd))
            compare(p, rnd, 20, f'built {i}', bad)
    check(not bad, f'400 built executables: {COUNT["same"]} answers the same', repr(bad[:2])[:400])


def game():
    import shaderdb
    try:
        root = shaderdb.find_opfor(None)
    except shaderdb.ShaderDbError as e:
        print(f'SKIP game: {e}')
        return
    game_dir = os.path.join(root, 'game')
    files = sorted(os.path.join(dp, f) for dp, _, fs in os.walk(game_dir) for f in fs
                   if f.lower().endswith(('.exe', '.dll')))
    rnd = random.Random(3)
    bad: list = []
    before = COUNT['same']
    for p in files:
        compare(p, rnd, 200, os.path.relpath(p, game_dir), bad)
    check(files and not bad, f'{len(files)} executables of the game copy: {COUNT["same"] - before} answers the same',
          repr(bad[:2])[:400])
    exe = os.path.join(game_dir, 'BFBC2Game.exe')
    if os.path.isfile(exe):
        img, ref = mc.PEImage(exe), reference(exe)
        t = time.perf_counter()
        a = img.strings(2)
        t1 = time.perf_counter()
        b = ref.strings_reference(2)
        t2 = time.perf_counter()
        print(f'     BFBC2Game.exe strings(2): {len(a)} strings, {t1 - t:.3f} s (reference {t2 - t1:.3f} s)')


def main(argv):
    from bfbc2core import native
    try:
        native.require('the menu tools\' executable reader (round F2b)', 'pe_raw_strings')
    except native.NativeMissing as e:
        check(False, 'the Rust core is built from these sources', str(e))
        return 1
    if mc._pe_native() is None:
        check(False, 'menucommon finds the module', 'BFBC2_PURE_PYTHON set?')
        return 1
    quick()
    if '--game' in argv:
        game()
    print(f'{COUNT["same"]} the same, {COUNT["differ"]} differ')
    if FAILS:
        print(f'{len(FAILS)} FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
