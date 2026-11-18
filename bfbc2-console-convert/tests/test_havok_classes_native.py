r"""Round F0: havok.classes (the Havok class layouts of BFBC2Game.exe's hkClass initializers) in the Rust core
(bc2_native.havok_classes) against its Python, the reference that stays until the port passes a full test tier
(havok.classes_reference).

Compared: the dict of classes in order, each class's name, parent, size and members (name, class, type, subtype, C
array size, flags, offset); for damaged executables the same classes or the same error.

  python test_havok_classes_native.py          quick: small PE images built here (initializers with imm8 / imm32
                                               pushes, stubs after int3 / ret, a decoy constructor, repeated
                                               objects and names, members naming classes and none), 400 damaged
                                               copies; the game copy's BFBC2Game.exe when found
  python test_havok_classes_native.py --game   + 6 damaged copies of BFBC2Game.exe (stubs, member tables, section
                                               headers changed); timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import os
import random
import shutil
import struct
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core')
import havok  # noqa: E402
from bfbc2core import native, paths  # noqa: E402
from f0_parity import Checks, outcome, stdout_utf8  # noqa: E402

N = native.require('havok.classes', 'havok_classes')
BASE, SEC_VA, SEC_RAW = 0x400000, 0x1000, 0x400


def build_pe(rng: random.Random) -> bytes:
    """A 32-bit PE image with one section: strings, member tables and initializer stubs (push x 12; mov ecx, obj;
    call ctor), most calling one constructor, a few a decoy."""
    sec = bytearray(0x3000)
    strings: dict[str, int] = {}
    spos = [0x10]                                               # the strings' own cursor (0x10 - 0x3FF)

    def string(s: str) -> int:
        if s not in strings:
            at = spos[0]
            sec[at:at + len(s) + 1] = s.encode() + b'\0'
            strings[s] = BASE + SEC_VA + at
            spos[0] += len(s) + 1
        return strings[s]

    names = [f'hk{c}' for c in rng.sample(['Shape', 'Body', 'Mopp', 'Array', 'Base', 'Entity', 'Phantom'], 5)]
    if rng.random() < 0.2:
        names.append(names[0])                                  # a repeated class name
    objs = [BASE + SEC_VA + 0x2000 + 0x40 * k for k in range(len(names))]
    if rng.random() < 0.2:
        objs[-1] = objs[0]                                      # a repeated hkClass object
    pos = 0x400
    tables = []
    for _k in names:
        count = rng.randrange(4)
        tab = pos
        for m in range(count):
            mname = string(f'm_{m}{rng.choice("abc")}') if rng.random() < 0.9 else 0
            cls = rng.choice(objs + [0, 0x1234])
            sec[pos:pos + 24] = struct.pack('<3IBBhHHI', mname, cls, 0, rng.randrange(34), rng.randrange(34),
                                            rng.randrange(-2, 5), rng.randrange(1 << 16), rng.randrange(1 << 12), 0)
            pos += 24
        tables.append((BASE + SEC_VA + tab, count))
    ctor, decoy = BASE + SEC_VA + 0x2F00, BASE + SEC_VA + 0x2F80
    pos = 0x1000
    for k, nm in enumerate(names):
        sec[pos] = rng.choice([0xCC, 0xC3])
        pos += 1
        name_va = string(nm)
        parent = rng.choice(objs + [0])
        args = [name_va, parent, rng.randrange(1, 300), 0, 0, 0, 0, tables[k][0], tables[k][1], 0, 0, rng.randrange(3)]
        for a in reversed(args):
            if a < 0x80 and rng.random() < 0.7:
                sec[pos:pos + 2] = bytes([0x6A, a])
                pos += 2
            else:
                sec[pos:pos + 5] = b'\x68' + struct.pack('<I', a)
                pos += 5
        target = decoy if (k == 0 and rng.random() < 0.3) else ctor
        here = BASE + SEC_VA + pos
        sec[pos:pos + 10] = b'\xB9' + struct.pack('<I', objs[k]) + b'\xE8' + struct.pack('<i', target - (here + 10))
        pos += 10 + rng.randrange(4)
    pe = 0x80
    hdr = bytearray(SEC_RAW)
    hdr[0:2] = b'MZ'
    struct.pack_into('<I', hdr, 0x3C, pe)
    hdr[pe:pe + 4] = b'PE\0\0'
    struct.pack_into('<H', hdr, pe + 6, 1)
    struct.pack_into('<H', hdr, pe + 20, 0xE0)
    struct.pack_into('<I', hdr, pe + 52, BASE)
    o = pe + 24 + 0xE0
    hdr[o:o + 8] = b'.text\0\0\0'
    struct.pack_into('<IIII', hdr, o + 8, len(sec), SEC_VA, len(sec), SEC_RAW)
    return bytes(hdr + sec)


def damaged(b: bytes, rng: random.Random, regions: list[tuple[int, int]]) -> bytes:
    x = bytearray(b)
    for _ in range(rng.randint(1, 6)):
        lo, hi = rng.choice(regions)
        x[rng.randrange(lo, min(hi, len(x)))] = rng.choice([0xCC, 0xC3, 0x68, 0x6A, 0xB9, 0xE8, 0, 0xFF, rng.randrange(256)])
    if rng.random() < 0.1:
        del x[rng.randrange(len(x)):]
    return bytes(x)


def compare(path: str) -> tuple[bool, bool]:
    """(the same classes or error both ways, Rust answered)."""
    a, r = outcome(havok.classes, path), outcome(havok.classes_reference, path)
    return a == r, N.havok_classes(path) is not None


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    rng = random.Random(0xF06)
    work = tempfile.mkdtemp(prefix='f0_havok_')
    try:
        same = answered = 0
        built = []
        for i in range(100):
            b = build_pe(rng)
            p = os.path.join(work, f'b{i}.exe')
            open(p, 'wb').write(b)
            built.append(p)
            ok, nat = compare(p)
            same += ok
            answered += nat
        c.check(same == 100 and answered == 100, f'built executables: {same} of 100 the same classes ({answered} in Rust)')
        same = answered = 0
        bad: list[str] = []
        regions = [(0x3C, 0x40), (0x80, 0x180), (SEC_RAW + 0x400, SEC_RAW + 0x600), (SEC_RAW + 0x1000, SEC_RAW + 0x1300)]
        for i in range(400):
            p = os.path.join(work, 'd.exe')
            open(p, 'wb').write(damaged(open(rng.choice(built), 'rb').read(), rng, regions))
            ok, nat = compare(p)
            same += ok
            answered += nat
            if not ok and len(bad) < 5:
                kept = os.path.join(work, f'bad{i}.exe')
                shutil.copyfile(p, kept)
                bad.append(f'{i}: {str(outcome(havok.classes, p))[:100]} / {str(outcome(havok.classes_reference, p))[:100]}')
        c.check(same == 400, f'damaged executables: {same} of 400 the same classes or the same error ({answered} in Rust)',
                '; '.join(bad))
        c.check(outcome(havok.classes, os.path.join(work, 'none.exe')) == outcome(havok.classes_reference, os.path.join(work, 'none.exe')),
                'a missing file: the reference\'s error')

        exe = paths.find_game_exe(walk=False, suite=SUITE)
        if exe:
            t0 = time.perf_counter()
            k = havok.classes(exe)
            t1 = time.perf_counter()
            r = havok.classes_reference(exe)
            t2 = time.perf_counter()
            nm = sum(len(x.members) for x in r.values())
            c.check(list(k.items()) == list(r.items()), f'BFBC2Game.exe: {len(r)} classes, {nm} members, the same '
                    f'both ways (Python {t2 - t1:.2f} s, Rust {t1 - t0:.2f} s)')
            if game:
                data = open(exe, 'rb').read()
                frng = random.Random(0xF07)
                # the initializer stubs sit near the strings' users: change bytes anywhere in the image
                regions = [(0, 0x400), (0x400, len(data))]
                same = 0
                for i in range(6):
                    p = os.path.join(work, f'game{i}.exe')
                    open(p, 'wb').write(damaged(data, frng, regions))
                    ok, _ = compare(p)
                    same += ok
                c.check(same == 6, f'damaged copies of BFBC2Game.exe: {same} of 6 the same classes or the same error')
        else:
            print('skip BFBC2Game.exe (no game copy)')
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
