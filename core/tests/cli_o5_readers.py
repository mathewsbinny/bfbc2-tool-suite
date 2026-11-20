r"""Round O5's readers group: the command lines test_cli_o5.py replays for the console readers apt.py, stfs.py and
xiso.py (bfbc2-console-readers).

cases(): the quick tier's, on inputs built here (XDVDFS discs, STFS packages of both kinds, APT movies in both byte
orders and as a menu .res; deterministic bytes, no game data). opfor_calls(opfor): the --game tier's: the user's own
Xbox 360 disc image and Update packages (read in place, as console_onslaught.py reads them through stfs.Package),
console and PC menu movies of OPFOR's work\ (copied), every output into the case folder.
"""
from __future__ import annotations

import os
import random
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import cli_o2_menu as menu  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

SECTOR = 2048
MAGIC = b'MICROSOFT*XBOX*MEDIA'
XBOX = r'C:\Games\Battlefield Bad Company 2 Limited Edition (XBOX 360)'


def noise(n: int, seed: int) -> bytes:
    return random.Random(seed).randbytes(n)


# ------------------------------------------------------------------------------------------- XDVDFS

def dir_table(entries: list[tuple[str, int, int, int]]) -> bytes:
    """A directory table: entries (name, sector, size, attr) as a binary tree, root node first (test_console_readers'
    builder)."""
    entries = sorted(entries, key=lambda e: e[0].lower())
    order: list[Any] = []
    links: dict[int, tuple[int | None, int | None]] = {}

    def place(lo: int, hi: int) -> int | None:
        if lo >= hi:
            return None
        mid = (lo + hi) // 2
        k = len(order)
        order.append(entries[mid])
        left = place(lo, mid)
        right = place(mid + 1, hi)
        links[k] = (left, right)
        return k

    place(0, len(entries))
    offsets, pos = [], 0
    for name, *_ in order:
        offsets.append(pos)
        pos += (14 + len(name) + 3) & ~3
    out = bytearray()
    for k, (name, sec, size, attr) in enumerate(order):
        left, right = links[k]
        e = struct.pack('<HHIIBB', offsets[left] // 4 if left is not None else 0,
                        offsets[right] // 4 if right is not None else 0,
                        sec, size, attr, len(name)) + name.encode('latin-1')
        out += e + b'\xff' * (((len(e) + 3) & ~3) - len(e))
    return bytes(out)


def xiso_image(base: int = 0, names: tuple[str, str] = ('default.xex', 'readme.txt'), cut: int = 0) -> bytes:
    """Root: default.xex, Levels/MP_005/level-00.fbrb, Levels/<names[1]>, Empty/ (size 0); cut: bytes taken off
    the end (a file runs past the image)."""
    data = {names[0]: noise(3000, 1), 'Levels/MP_005/level-00.fbrb': noise(5000, 2), f'Levels/{names[1]}': b'hello'}
    sectors = {names[0]: 40, 'Levels/MP_005/level-00.fbrb': 42, f'Levels/{names[1]}': 46}
    mp = dir_table([('level-00.fbrb', 42, 5000, 0x80)])
    levels = dir_table([('MP_005', 35, len(mp), 0x10), (names[1], 46, 5, 0x80)])
    root = dir_table([(names[0], 40, 3000, 0x80), ('Levels', 34, len(levels), 0x10), ('Empty', 0, 0, 0x10)])
    out = bytearray(base + 48 * SECTOR)

    def at(sector: int, blob: bytes) -> None:
        out[base + sector * SECTOR:base + sector * SECTOR + len(blob)] = blob
    at(32, MAGIC + struct.pack('<II', 33, len(root)))
    at(33, root)
    at(34, levels)
    at(35, mp)
    for name, s in sectors.items():
        at(s, data[name])
    if cut:
        return bytes(out[:base + 42 * SECTOR + 1000])
    return bytes(out)


# ------------------------------------------------------------------------------------------- STFS

def stfs_package(read_only: bool, live: str | None = None, names: tuple[str, ...] = ('a.bin', 'b.bin', 'top.txt'),
                 title: str = 'Test Package') -> bytes:
    """test_console_readers.build_stfs's package (deterministic bytes): data block 0 the file table; 1-2
    Content/<a> (consecutive); 5 -> 3 -> 4 Content/<b> (fragmented); 6 <top>."""
    a, b, top = names
    files = {a: noise(6000, 3), b: noise(9000, 4), top: b'top level'}
    h = bytearray(0x1000)
    h[0:4] = b'LIVE'
    header_size = 0x971A
    struct.pack_into('>I', h, 0x340, header_size)
    h[0x37B] = 1 if read_only else 2 if live == 'separation' else 0
    if live == 'level1':
        struct.pack_into('>I', h, 0x395, 0x100)
    struct.pack_into('<H', h, 0x37C, 1)
    name = title.encode('utf-16-be')
    h[0x411:0x411 + len(name)] = name
    base = (header_size + 0xFFF) & ~0xFFF
    tables = 1 if read_only else 2

    def entry(nm: str, flags: int, start: int, count: int, parent: int, size: int) -> bytes:
        e = bytearray(0x40)
        raw = nm.encode('latin-1')
        e[:len(raw)] = raw
        e[0x28] = len(raw) | flags
        e[0x29:0x2C] = count.to_bytes(3, 'little')
        e[0x2C:0x2F] = count.to_bytes(3, 'little')
        e[0x2F:0x32] = start.to_bytes(3, 'little')
        struct.pack_into('>H', e, 0x32, parent)
        struct.pack_into('>I', e, 0x34, size)
        return bytes(e)

    table = (entry('Content', 0x80, 0, 0, 0xFFFF, 0) + entry(a, 0x40, 1, 2, 0, 6000) +
             entry(b, 0x00, 5, 3, 0, 9000) + entry(top, 0x40, 6, 1, 0xFFFF, 9))
    blocks = {0: table, 1: files[a][:0x1000], 2: files[a][0x1000:], 5: files[b][:0x1000],
              3: files[b][0x1000:0x2000], 4: files[b][0x2000:], 6: files[top]}
    hashes = bytearray(0x1000)
    for blk, nxt in ((0, 0xFFFFFF), (5, 3), (3, 4), (4, 0xFFFFFF)):
        hashes[blk * 0x18 + 0x15:blk * 0x18 + 0x18] = nxt.to_bytes(3, 'big')
    stale = bytearray(hashes)
    for blk, nxt in ((5, 4), (4, 3), (3, 0xFFFFFF)):
        stale[blk * 0x18 + 0x15:blk * 0x18 + 0x18] = nxt.to_bytes(3, 'big')
    size = base + (tables + 7) * 0x1000
    if live == 'level1':
        size = max(size, base + 0xAD * 0x1000)
    out = bytearray(size)
    out[:0x1000] = h
    for k in range(tables):
        blob = stale if live and k == 0 else hashes
        out[base + k * 0x1000:base + (k + 1) * 0x1000] = blob
    if live == 'level1':
        l1 = bytearray(0x1000)
        l1[0x14] = 0x40
        out[base + 0xAC * 0x1000:base + 0xAD * 0x1000] = l1
    for blk, data in blocks.items():
        at = base + (tables + blk) * 0x1000
        out[at:at + 0x1000] = data.ljust(0x1000, b'\0')
    return bytes(out)


# ------------------------------------------------------------------------------------------- APT

def apt_movie(big: bool, label: bytes = b'intro', hello: bytes = b'hello') -> tuple[bytes, bytes]:
    """test_console_readers.build_apt's movie: the movie (an export "spr" = 1), a sprite with an action and a label,
    an image, a shape; constants "hello" and register 3. label / hello: their texts."""
    e = '>' if big else '<'
    a = bytearray(b'Apt Data:74\x1a\0\0\0\0\0')

    def put(data: bytes) -> int:
        while len(a) % 4:
            a.append(0)
        at = len(a)
        a.extend(data)
        return at
    code = put(bytes([0xB7]) + struct.pack(e + 'i', 0x01020304) + bytes([0xB4]) + struct.pack(e + 'f', 0.1)
               + bytes([0x07, 0x00]))
    lab = put(label + b'\0')
    export_name = put(b'spr\0')
    action = put(struct.pack(e + 'II', 1, code))
    flabel = put(struct.pack(e + 'IIIi', 2, lab, 0, 0))
    items = put(struct.pack(e + 'II', action, flabel))
    sframes = put(struct.pack(e + 'II', 2, items))
    sprite = put(struct.pack(e + 'IIIII', 5, 0x09876543, 1, sframes, 0))
    image = put(struct.pack(e + 'III', 7, 0x09876543, 2))
    shape = put(struct.pack(e + 'II4fI', 1, 0x09876543, 0, -0.5, 10, 1e7, 3))
    font = put(struct.pack(e + 'II', 3, 0x09876543))
    mframes = put(struct.pack(e + 'II', 0, 0))
    exports = put(struct.pack(e + 'II', export_name, 1))
    chars = put(b'\0' * 28)
    movie = put(struct.pack(e + 'IIIIIIIiiIIIII', 9, 0x09876543, 1, mframes, 0, 7, chars, 1280, 720, 33, 0, 0, 1,
                            exports))
    # characters: the movie, the sprite, the image, the shape, a font, an empty slot, one whose signature is wrong
    struct.pack_into(e + '7I', a, chars, movie, sprite, image, shape, font, 0, code)
    c = bytearray(b'Apt constant file\x1a\0\0')
    at = 64
    c += struct.pack(e + 'III', movie, 3, 32) + struct.pack(e + 'IIIIII', 1, at, 4, 3, 9, 7)
    c += b'\0' * (at - len(c)) + hello + b'\0'
    return bytes(a), bytes(c)


def bigf(members: dict[str, bytes]) -> bytes:
    return menu.res(members)


# ------------------------------------------------------------------------------------------- the cases

def stfs_cases() -> list[tuple[str, dict[str, Any], list[str], dict[str, str]]]:
    ro, two = stfs_package(True), stfs_package(False)
    sep, l1 = stfs_package(False, 'separation'), stfs_package(False, 'level1')
    latin = stfs_package(True, names=('caf\xe9.bin', 'b\x81.bin', 'top.txt'), title='Pak\xe9 \u0416')
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('no arguments', {}, [], {}),
        ('info without a package', {}, ['info'], {}),
        ('an unknown command', {'p.pkg': ro}, ['bogus', '{d}\\p.pkg'], {}),
        ('extract without its folder', {'p.pkg': ro}, ['extract', '{d}\\p.pkg'], {}),
        ('a missing package', {}, ['info', '{d}\\none.pkg'], {}),
        ('a folder as the package', {'dir': None}, ['list', '{d}\\dir'], {}),
        ('a header cut short', {'p.pkg': ro[:0x300]}, ['info', '{d}\\p.pkg'], {}),
        ('a file table past the end', {'p.pkg': ro[:0xA000]}, ['list', '{d}\\p.pkg'], {}),
    ]
    for label, pkg in (('read-only', ro), ('two tables', two), ('separation bit', sep), ('level-1 table', l1)):
        rows += [
            (f'info, {label}', {'p.pkg': pkg}, ['info', '{d}\\p.pkg'], {}),
            (f'list, {label}', {'p.pkg': pkg}, ['list', '{d}\\p.pkg'], {}),
            (f'list a filter, {label}', {'p.pkg': pkg}, ['list', '{d}\\p.pkg', 'CONTENT/B'], {}),
            (f'extract, {label}', {'p.pkg': pkg}, ['extract', '{d}\\p.pkg', '{d}\\out'], {}),
            (f'extract a filter, {label}', {'p.pkg': pkg}, ['extract', '{d}\\p.pkg', '{d}\\out', 'b.BIN'], {}),
        ]
    rows += [
        ('info, names outside ASCII', {'p.pkg': latin}, ['info', '{d}\\p.pkg'], {}),
        ('list, a name outside cp1252', {'p.pkg': latin}, ['list', '{d}\\p.pkg'], {}),
        ('list, a name in cp1252 only', {'p.pkg': latin}, ['list', '{d}\\p.pkg', 'caf'], {}),
        ('extract, names outside ASCII', {'p.pkg': latin}, ['extract', '{d}\\p.pkg', '{d}\\out', 'CAF\xc9'], {}),
        ('a filter that matches nothing', {'p.pkg': ro}, ['list', '{d}\\p.pkg', 'zzz'], {}),
    ]
    return rows


def xiso_cases() -> list[tuple[str, dict[str, Any], list[str], dict[str, str]]]:
    plain, xgd3 = xiso_image(), xiso_image(0x2080000)
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('no arguments', {}, [], {}),
        ('a command alone', {}, ['list'], {}),
        ('a missing image', {}, ['list', '{d}\\none.iso'], {}),
        ('no volume', {'d.iso': b'\0' * (40 * SECTOR)}, ['list', '{d}\\d.iso'], {}),
        ('the root cut short', {'d.iso': plain[:32 * SECTOR + 24]}, ['list', '{d}\\d.iso'], {}),
        ('an unknown command walks and prints nothing', {'d.iso': plain}, ['bogus', '{d}\\d.iso'], {}),
        ('list', {'d.iso': plain}, ['list', '{d}\\d.iso'], {}),
        ('list a filter', {'d.iso': plain}, ['list', '{d}\\d.iso', 'LEVELS/'], {}),
        ('list, the XGD3 partition', {'d.iso': xgd3}, ['list', '{d}\\d.iso'], {}),
        ('extract', {'d.iso': plain}, ['extract', '{d}\\d.iso', '{d}\\out'], {}),
        ('extract a filter', {'d.iso': plain}, ['extract', '{d}\\d.iso', '{d}\\out', 'Readme'], {}),
        ('extract without its folder', {'d.iso': plain}, ['extract', '{d}\\d.iso'], {}),
        ('extract, the image ends inside a file', {'d.iso': xiso_image(cut=1)}, ['extract', '{d}\\d.iso', '{d}\\o'],
         {}),
        ('list, a name outside cp1252', {'d.iso': xiso_image(names=('default.xex', 'r\x81.txt'))},
         ['list', '{d}\\d.iso'], {}),
        ('extract, a name in cp1252 only', {'d.iso': xiso_image(names=('d\xe9f.xex', 'readme.txt'))},
         ['extract', '{d}\\d.iso', '{d}\\out', 'D\xc9F'], {}),
        ('a folder as the image', {'dir': None}, ['list', '{d}\\dir'], {}),
    ]
    return rows


def apt_cases() -> list[tuple[str, dict[str, Any], list[str], dict[str, str]]]:
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('no arguments', {}, [], {}),
        ('an unknown command', {}, ['play', 'x.apt'], {}),
        ('a missing movie', {}, ['info', '{d}\\none.apt'], {}),
        ('a missing .const', {'m.apt': apt_movie(False)[0]}, ['info', '{d}\\m.apt'], {}),
        ('not a BIGF archive', {'m.res': b'RIFF' + b'\0' * 40}, ['info', '{d}\\m.res'], {}),
        ('a .const that is not one', {'m.apt': apt_movie(False)[0], 'm.const': b'nothing here at all, really....'},
         ['info', '{d}\\m.apt'], {}),
        ('a .const cut short', {'m.apt': apt_movie(False)[0], 'm.const': b'Apt constant file\x1a\0'},
         ['info', '{d}\\m.apt'], {}),
        ('find without a text', {}, ['find', 'x.apt'], {}),
    ]
    for big in (False, True):
        a, c = apt_movie(big)
        label = 'Xbox 360' if big else 'PC'
        files = {'m.apt': a, 'm.const': c}
        res = {'menu.res': bigf({'Menu.apt': a, 'Menu.const': c, 'Menu.geo': b'geo'})}
        rows += [
            (f'info, {label}', files, ['info', '{d}\\m.apt'], {}),
            (f'info of a .res, {label}', res, ['info', '{d}\\menu.res'], {}),
            (f'chars, {label}', files, ['chars', '{d}\\m.apt'], {}),
            (f'chars of a type, {label}', files, ['chars', '{d}\\m.apt', 'SHAPE'], {}),
            (f'chars of an unknown type, {label}', files, ['chars', '{d}\\m.apt', 'type13'], {}),
            (f'dump, {label}', files, ['dump', '{d}\\m.apt'], {}),
            (f'dump into a file, {label}', files, ['dump', '{d}\\m.apt', '{d}\\d.txt'], {}),
            (f'dump of a .res, {label}', res, ['dump', '{d}\\menu.res'], {}),
            (f'find, {label}', files, ['find', '{d}\\m.apt', 'HEL'], {}),
            (f'find a string, {label}', files, ['find', '{d}\\m.apt', 'Intro'], {}),
            (f'find everything, {label}', files, ['find', '{d}\\m.apt', ''], {}),
            (f'find nothing, {label}', files, ['find', '{d}\\m.apt', 'zzz'], {}),
        ]
    a, c = apt_movie(False, label=b'\x81\xe9label', hello=b'h\xe9\x81llo')
    files = {'m.apt': a, 'm.const': c}
    rows += [
        ('dump, texts outside cp1252 (replaced)', files, ['dump', '{d}\\m.apt'], {}),
        ('dump into a file, texts outside cp1252', files, ['dump', '{d}\\m.apt', '{d}\\d.txt'], {}),
        ('find, a constant outside cp1252', files, ['find', '{d}\\m.apt', 'LLO'], {}),
        ('find, a string outside cp1252', files, ['find', '{d}\\m.apt', 'label'], {}),
    ]
    return rows


def cases() -> list[Case]:
    out: list[Case] = []
    for tool, rows in (('stfs', stfs_cases()), ('xiso', xiso_cases()), ('apt', apt_cases())):
        out += [(f'{tool}: {n}', tool, f, a, e, False) for n, f, a, e in rows]
    return menu.with_variants(out, ['stfs: list, a name outside cp1252', 'stfs: list, read-only',
                                    'xiso: list', 'xiso: list, a name outside cp1252', 'apt: dump, PC',
                                    'apt: dump, texts outside cp1252 (replaced)', 'apt: find, a constant outside cp1252'])


# ------------------------------------------------------------------------------------------- the game

def opfor_calls(opfor: str) -> list[Call]:
    """The user's own Xbox 360 files (read in place; console_onslaught.py opens Update #3 through stfs.Package) and
    OPFOR's menu movies (copied), outputs into the case folder."""
    out: list[Call] = []
    folder = os.path.join(XBOX, 'Battlefield Bad Company 2 (3 DLCs Pack prototype)', '454108A8', '00000002')
    if os.path.isdir(folder):
        for f in sorted(os.listdir(folder)):
            p = os.path.join(folder, f)
            out.append((f'stfs info {f}', 'stfs', ['info', p], {}, {}, True))
            out.append((f'stfs list {f}', 'stfs', ['list', p], {}, {}, True))
            out.append((f'stfs list {f} pathdata', 'stfs', ['list', p, 'PATHDATA'], {}, {}, True))
            out.append((f'stfs extract {f} ons_mp_008 infantry', 'stfs',
                        ['extract', p, '{d}\\x', 'ons_mp_008/infantry'], {}, {}, True))
            out.append((f'stfs extract {f} mainmenu-00', 'stfs', ['extract', p, '{d}\\x', 'menu/main/mainmenu-00'],
                        {}, {}, True))
    iso = os.path.join(XBOX, 'Battlefield Bad Company 2 Limited Edition (XBOX 360).iso')
    if os.path.isfile(iso):
        out.append(('xiso list the disc', 'xiso', ['list', iso], {}, {}, True))
        out.append(('xiso list the disc, levels/mp_005', 'xiso', ['list', iso, 'levels/mp_005'], {}, {}, True))
        out.append(('xiso extract default.xex', 'xiso', ['extract', iso, '{d}\\x', 'default.xex'], {}, {}, True))
        out.append(('xiso extract mp_005 ingame', 'xiso', ['extract', iso, '{d}\\x', 'mp_005/ingame'], {}, {}, True))
    movies = [os.path.join(opfor, 'work', *rel.split('/')) for rel in menu.OPFOR_MOVIES]
    for p in [p for p in movies if os.path.isfile(p)][:8]:
        rel = os.path.relpath(p, opfor)
        for args in (['info'], ['chars'], ['chars', 'sprite'], ['dump'], ['dump', '{d}\\d.txt'], ['find', 'onslaught'],
                     ['find', 'Rank']):
            argv = [args[0], '{d}\\m.res', *args[1:]]
            out.append((f'apt {" ".join(a for a in args if "{d}" not in a)} {rel}', 'apt', argv, {'m.res': p}, {},
                        True))
    return out
