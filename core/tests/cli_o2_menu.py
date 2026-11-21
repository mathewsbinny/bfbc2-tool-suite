r"""Round O2's menu group: the command lines test_cli_o2.py replays for aptcheck, aptrefs and bc2loc.

cases(): the quick tier's, on inputs built here (bfbc2-menu-tools\tests\test_menu_tools.py's movie, exe and language
file builders, test_fbrbtool's bundles; a fake game copy under the case folder, named by OPFOR_ROOT; no game data).
opfor_calls(opfor): the --game tier's (Project OPFOR's real invocations; the game copy read in place, OPFOR's work\
inputs copied to the case folder, every output into the case folder).
"""
from __future__ import annotations

import glob
import os
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import test_fbrbtool as tfb  # noqa: E402

sys.path.insert(0, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests'))
import test_menu_tools as tmt  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}
OP = {'OPFOR_ROOT': '{d}\\op'}                      # the fake game copy


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    """rows, each again with stderr on stdout's pipe, and the picked ones (keys) under Python's stdio switches."""
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


# ------------------------------------------------------------------------------------------- inputs

A, K, MARKS = tmt.build_movie()
MEMBERS = {'t.apt': A, 't.const': K}
ENTRY = 32 + 8 * tmt.STRINGS.index('value') + 4          # the constant 'value''s text offset


def res(members: dict[str, bytes]) -> bytes:
    return bytes(tmt.bigf(members))


def patched(apt_changes: dict[int, bytes] | None = None,
            const_changes: dict[int, bytes] | None = None) -> dict[str, bytes]:
    return dict(tmt.patched(MEMBERS, apt_changes, const_changes))


SHARED = {((MARKS['push2'] + 4) & ~3) + 4: struct.pack('<I', MARKS['list1'])}


def big_movie() -> dict[str, bytes]:
    """The movie with every rule broken at once (verify and check list them all)."""
    return patched({**SHARED, MARKS['body_end'] - 2: b'\x00\x00', (MARKS['branch'] + 4) & ~3: struct.pack('<i', 1),
                    MARKS['list1']: struct.pack('<I', 999)})


STAND_IN_MARK = b'bfbc2-tool-suite test stand-in, not a program\0'


def exe_795745() -> bytes:
    """test_menu_tools' exe with build 795745's time stamp and the getters' switch tables (index bytes 0..2, jump
    tables of the default and two cases)."""
    base = 0x400000
    rdata = bytearray(b'\0' * 4)
    names = ['setInputState', 'presenceRequestOnline', 'isLive'] + [f'name{i}' for i in range(58)]
    ids = [0x0D, 0x1D, 0x87] + [0x100 + i for i in range(58)]
    ptrs = []
    for n in names:
        while len(rdata) % 4:
            rdata.append(0)
        ptrs.append(base + 0x2000 + len(rdata))
        rdata += n.encode() + b'\0'
    while len(rdata) % 4:
        rdata.append(0)
    rdata += b'\xff' * 8
    for p, i in zip(ptrs, ids):
        rdata += struct.pack('<II', p, i)
    rdata += b'\xff' * 8
    a = bytearray(0x1200)                        # 0x76F000..0x770200
    b = bytearray(0x1000)                        # 0x95D000..0x95E000

    def put(buf: bytearray, lo: int, va: int, data: bytes) -> None:
        buf[va - lo:va - lo + len(data)] = data
    for index, count, jumps, default in ((0x76FE2C, 0xE6, 0x76FD78, 0x76FD20), (0x76FF4C, 0x3B, 0x76FF14, 0x76FD20)):
        put(a, 0x76F000, index, bytes(k % 3 for k in range(count)))
        put(a, 0x76F000, jumps, struct.pack('<3I', default, 0x770000 + count, 0x770100 + count))
    put(b, 0x95D000, 0x95DB68, bytes((k * 7) % 3 for k in range(0x15F)))
    put(b, 0x95D000, 0x95D98C, struct.pack('<3I', 0x95D934, 0x95E000, 0x95E004))
    # round RT3: without this text in the unused end of .data2, Microsoft Defender's machine-learning scan took the
    # file for a trojan (Trojan:Win32/Bearfoos.A!ml, whatever its name or extension) and blocked every read of it, so
    # the cases failed with OSError 22 on any PC running Defender; no tool reads these bytes
    put(b, 0x95D000, 0x95E000 - len(STAND_IN_MARK), STAND_IN_MARK)
    return bytes(tmt.pe32([('.text', 0x1000, b'\xc3' + b'\xcc' * 15, 0x60000020),
                           ('.rdata', 0x2000, bytes(rdata), 0x40000040),
                           ('.data1', 0x36F000, bytes(a), 0xC0000040), ('.data2', 0x55D000, bytes(b), 0xC0000040)],
                          base, stamp=0x4E55445D))


def khash(key: str) -> int:
    h = 0xFFFFFFFF
    for c in key.encode('latin1', 'replace'):
        h = (h * 33 + c) & 0xFFFFFFFF
    return h


def loc_file(texts: dict[str, bytes]) -> tuple[bytes, bytes]:
    """A language file of {key: raw text} and test_menu_tools' histogram."""
    _, histogram = tmt.build_loc()
    items = sorted((khash(k), v) for k, v in texts.items())
    strings, offs = bytearray(), []
    for _h, v in items:
        offs.append(len(strings))
        strings += v + b'\0'
    table_off = 0x1C
    body = bytearray(b'NORMAL'.ljust(16, b'\0'))
    for (h, _v), o in zip(items, offs):
        body += struct.pack('<II', h, o)
    body += strings
    data = struct.pack('<5I', 0x39000, 0, len(items), table_off, table_off + 8 * len(items)) + body
    return data[:4] + struct.pack('<I', len(data) - 8) + data[8:], bytes(histogram)


LOC_TEXTS = {'MENU_PLAY_NOW': b'PLAY NOW', 'WIN32_PC_SERVER_BROWSER': b'SERVER BROWSER', 'RU': b'\x81\x82',
             'JP': b'\x80\x85\x83', 'QUOTE': b"it's \"q\"\\ \t", 'value': b'VALUE TEXT', 'isLive': b'LIVE?',
             'name7': b'Seven', 'HIGH': b'caf\xe9 \xff', 'EMPTY': b''}
BIG_TEXTS = {**LOC_TEXTS, **{f'KEY_{i:04d}': f'text number {i} of the big file'.encode() for i in range(400)}}


def fake_opfor() -> dict[str, Any]:
    """A game copy under {d}\\op: the main menu bundle (Package over Dist, a deleted movie, a .res that is no
    movie), the pristine copy, a HUD bundle, the exe and the overlay with two languages."""
    other = patched({MARKS['notnot']: b'\x17\x73'})
    pkg_menu = tfb.bundle([('Menu/t.res', 'SwfMovie', 0x10000, res(other)),
                           ('Menu/Other.res', 'SwfMovie', 0x10000, res(MEMBERS)),
                           ('Menu/notmovie.res', 'SwfMovie', 0x10000, b'not a BIGF archive'),
                           ('Menu/gone.res', tfb.DELETED, 0x10000, b''),
                           ('Menu/sub/deep.res', 'SwfMovie', 0x10000, res(MEMBERS)),
                           ('Menu/data.res', 'RawFileData', 0x10000, b'x')])
    dist_menu = tfb.bundle([('Menu/gone.res', 'SwfMovie', 0x10000, res(MEMBERS)),
                            ('Menu/old.res', 'SwfMovie', 0x10000, res(big_movie()))])
    pristine = tfb.bundle([('Menu/t.res', 'SwfMovie', 0x10000, res(MEMBERS))])
    hud = tfb.bundle([('Menu/hud.res', 'SwfMovie', 0x10000, res(MEMBERS)),
                      ('Menu/HUD2.res', 'SwfMovie', 0x10000, res(other))])
    eng, hist = loc_file(LOC_TEXTS)
    tst, _ = loc_file(BIG_TEXTS)
    overlay = tfb.bundle([('Localization/bin/language_english_normal.bin', 'RawFileData', 0x10000, eng),
                          ('Localization/bin/language_english_histogram.bin', 'RawFileData', 0x10000, hist),
                          ('Localization/bin/language_Test_normal.bin', 'RawFileData', 0x10000, tst),
                          ('Localization/bin/language_test_histogram.bin', 'RawFileData', 0x10000, hist),
                          ('Localization/bin/language_gone_normal.bin', tfb.DELETED, 0x10000, b'')])
    return {'op\\game\\Package\\menu\\main\\mainmenu-00.fbrb': pkg_menu,
            'op\\game\\Dist\\win32\\menu\\main\\mainmenu-00.fbrb': dist_menu,
            'op\\original\\Package\\menu\\main\\mainmenu-00.fbrb': pristine,
            'op\\game\\Package\\levels\\mp_common\\ingame-00.fbrb': hud,
            'op\\game\\BFBC2Game.exe': bytes(tmt.build_exe()),
            'op\\game\\Dist\\win32\\overlay-00.fbrb': overlay}


# ------------------------------------------------------------------------------------------- aptcheck

def good_patch() -> str:
    return ('# test patch\n\n'
            f't.apt {MARKS["notnot"]:X} 12 12 -> 17 73   # isLive popped, true pushed\n'
            '# the constant gets new text\n'
            'append t.const NEWTEXT "HELLO WORLD"\n'
            f't.const {ENTRY:X} {" ".join(f"{x:02X}" for x in K[ENTRY:ENTRY + 4])} -> @NEWTEXT\n')


BAD_PATCH = (f't.apt {MARKS["notnot"]:X} 12 13 -> 17 73   # wrong expected byte\n'
             f't.apt {MARKS["notnot"]:X} 12 -> 17\n'
             f't.apt {MARKS["pushbyte"]:X} B5 05 -> B5   # size change\n'
             't.apt zz 12 -> 17   # not hex\n'
             't.apt 10 12 13 -> 17 @NOPE # undefined label\n'
             'append t.const X "one"   # first\n'
             'append t.const X "twö"   # twice, not ASCII\n'
             'append t.const "no label"\n'
             'append t.const Y z w "x" # too many\n'
             'nomember.apt 0 00 -> 01   # no member\n'
             f't.apt {len(A) - 1:X} 00 00 -> 01 02   # past the end\n'
             't.apt 20 -> 01   # missing expected\n'
             't.apt 20 1 -> 01   # one digit\n'
             't.apt 20 100 -> 01 # three digits\n'
             't.apt 20 12 -> 1G # bad new\n'
             't.apt 20 12 -> -> 01 # two arrows\n'
             't.apt 0x24 41 -> 41 # changes nothing\n'
             '\tt.apt 1_0 70 -> 71\r\n'
             'T.APT 0 41 -> 42 # upper-case member\n')


def aptcheck_cases() -> list[Case]:
    ok_res, new_res, big_res = res(MEMBERS), res(patched(SHARED)), res(big_movie())
    good = res(patched({MARKS['notnot']: b'\x17\x73'}))
    files_v = {'t.res': ok_res, 'new.res': new_res}
    op = fake_opfor()
    many_ranges = res(patched({o: b'\x90' for o in range(0x10, MARKS['code'], 2)}))
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('the usage', {}, [], {}),
        ('an unknown command', {}, ['nope'], {}),
        ('check a .res', {'t.res': ok_res}, ['check', '{d}\\t.res'], {}),
        ('check a .apt with its .const', {'m.apt': A, 'm.const': K}, ['check', '{d}\\m.apt'], {}),
        ('check a .apt without its .const', {'m.apt': A}, ['check', '{d}\\m.apt'], {}),
        ('check a broken movie (every rule)', {'b.res': big_res}, ['check', '{d}\\b.res'], {}),
        ('check several, one missing midway', {'t.res': ok_res, 'b.res': big_res},
         ['check', '{d}\\t.res', '{d}\\b.res', '{d}\\missing.res', '{d}\\t.res'], {}),
        ('check no movie', {}, ['check'], {}),
        ('check a missing path with a folder', {}, ['check', 'a\\b'], {}),
        ('check a name in the game copy', op, ['check', 'Other'], OP),
        ('check a name with .res, other case', op, ['check', 'T.RES'], OP),
        ('check a name only in Dist', op, ['check', 'old'], OP),
        ('check a deleted movie', op, ['check', 'gone'], OP),
        ('check an unknown name', op, ['check', 'nothing'], OP),
        ('check a .res that is no movie', op, ['check', 'notmovie'], OP),
        ('check pristine:', op, ['check', 'pristine:t'], OP),
        ('check pristine: without the pristine bundle', {'op\\game\\x': b''}, ['check', 'pristine:t'], OP),
        ('check <bundle>:<name>', op, ['check', '{d}\\op\\game\\Package\\levels\\mp_common\\ingame-00.fbrb:hud2'], {}),
        ('check a missing bundle', {}, ['check', '{d}\\no.fbrb:t'], {}),
        ('check --opfor', op, ['check', 'Other', '--opfor', '{d}\\op'], {}),
        ('check --opfor without a value', {}, ['check', 'x', '--opfor'], {}),
        ('check a file that is no BIGF', {'x.res': b'hello'}, ['check', '{d}\\x.res'], {}),
        ('check a .res without .apt', {'x.res': res({'a.geo': b'1'})}, ['check', '{d}\\x.res'], {}),
        ('check a .res without .const', {'x.res': res({'a.apt': A})}, ['check', '{d}\\x.res'], {}),
        ('check a bad constant file', {'x.res': res({'a.apt': A, 'A.CONST': b'not a const file ' * 3})},
         ['check', '{d}\\x.res'], {}),
        ('check the same movie 120 times (stdout over 8,192 bytes)', {'t.res': ok_res},
         ['check'] + ['{d}\\t.res'] * 120, {}),
        ('check 120 then a missing one', {'t.res': ok_res}, ['check'] + ['{d}\\t.res'] * 120 + ['{d}\\no.res'], {}),
        ('verify a shared list', files_v, ['verify', '{d}\\t.res', '{d}\\new.res'], {}),
        ('verify --brief first', files_v, ['verify', '--brief', '{d}\\t.res', '{d}\\new.res'], {}),
        ('verify a good patch', {'t.res': ok_res, 'g.res': good}, ['verify', '{d}\\t.res', '{d}\\g.res'], {}),
        ('verify the same movie', {'t.res': ok_res}, ['verify', '{d}\\t.res', '{d}\\t.res'], {}),
        ('verify every rule broken', {'t.res': ok_res, 'b.res': big_res}, ['verify', '{d}\\t.res', '{d}\\b.res'], {}),
        ('verify every rule broken, reversed', {'t.res': ok_res, 'b.res': big_res},
         ['verify', '{d}\\b.res', '{d}\\t.res'], {}),
        ('verify a decode shift', {'t.res': ok_res, 's.res': res(patched({MARKS['pushbyte']: b'\xB7'}))},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify a shorter member', {'t.res': ok_res, 's.res': res({'t.apt': A[:-4], 't.const': K})},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify appended bytes', {'t.res': ok_res, 's.res': res({'t.apt': A, 't.const': K + b'NEW\0'})},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify a new member and a missing one', {'t.res': res({**MEMBERS, 'x.geo': b'1'}),
                                                   's.res': res({**MEMBERS, 'y.geo': b'2'})},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify members renamed in case', {'t.res': ok_res, 's.res': res({'T.apt': A, 't.CONST': K})},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify a constant retargeted',
         {'t.res': ok_res, 's.res': res(patched(const_changes={ENTRY: struct.pack('<I', 32 + 8 * len(tmt.STRINGS))}))},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify constant data changed', {'t.res': ok_res, 's.res': res(patched(const_changes={len(K) - 2: b'Z'}))},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify many ranges (stdout over 8,192 bytes)', {'t.res': ok_res, 's.res': many_ranges},
         ['verify', '{d}\\t.res', '{d}\\s.res'], {}),
        ('verify many ranges --brief', {'t.res': ok_res, 's.res': many_ranges},
         ['verify', '{d}\\t.res', '{d}\\s.res', '--brief'], {}),
        ('verify with --patch', {'t.res': ok_res, 'g.res': good, 'p.patch': good_patch()},
         ['verify', '{d}\\t.res', '{d}\\g.res', '--patch', '{d}\\p.patch'], {}),
        ('verify with a bad --patch', {'t.res': ok_res, 'g.res': good, 'p.patch': BAD_PATCH},
         ['verify', '{d}\\t.res', '{d}\\g.res', '--patch', '{d}\\p.patch'], {}),
        ('verify with a --patch naming less', {'t.res': ok_res, 'g.res': good,
                                               'p.patch': f't.apt {MARKS["notnot"]:X} 12 -> 17 # one\n'},
         ['verify', '{d}\\t.res', '{d}\\g.res', '--patch', '{d}\\p.patch'], {}),
        ('verify with a missing --patch', {'t.res': ok_res},
         ['verify', '{d}\\t.res', '{d}\\t.res', '--patch', '{d}\\no'], {}),
        ('verify --patch without a value', {}, ['verify', 'a', 'b', '--patch'], {}),
        ('verify one movie', {'t.res': ok_res}, ['verify', '{d}\\t.res'], {}),
        ('verify against the game copy', op, ['verify', 'pristine:t', 't'], OP),
        ('lint a good patch', {'t.res': ok_res, 't.patch': good_patch()},
         ['lint', '{d}\\t.patch', '--source', '{d}\\t.res'], {}),
        ('lint a bad patch', {'t.res': ok_res, 't.patch': BAD_PATCH},
         ['lint', '{d}\\t.patch', '--source', '{d}\\t.res'], {}),
        ('lint a bad patch --brief', {'t.res': ok_res, 't.patch': BAD_PATCH},
         ['lint', '--brief', '{d}\\t.patch', '--source', '{d}\\t.res'], {}),
        ('lint a glob', {'t.res': ok_res, 'a.patch': good_patch(), 'B.PATCH': BAD_PATCH, '.hidden.patch': 'x',
                         'c.txt': 'x'}, ['lint', '{d}\\*.patch', '--source', '{d}\\t.res'], {}),
        ('lint a glob with ?', {'t.res': ok_res, 'a1.patch': good_patch(), 'a2.patch': good_patch()},
         ['lint', '{d}\\a?.patch', '--source', '{d}\\t.res', '--brief'], {}),
        ('lint a glob matching nothing', {}, ['lint', '{d}\\*.patch'], {}),
        ('lint the pristine source --installed', {**op, 't.patch': good_patch()},
         ['lint', '{d}\\t.patch', '--installed'], OP),
        ('lint --installed, the installed movie the result',
         {**op, 'other.patch': f't.apt {MARKS["notnot"]:X} 12 12 -> 17 73   # as installed\n'},
         ['lint', '{d}\\other.patch', '--installed', '--source',
          '{d}\\op\\original\\Package\\menu\\main\\mainmenu-00.fbrb:t'], OP),
        ('lint --installed, no installed movie', {**op, 'zz.patch': good_patch()},
         ['lint', '{d}\\zz.patch', '--installed', '--source', 'pristine:t'], OP),
        ('lint a missing file', {}, ['lint', '{d}\\no.patch'], {}),
        ('lint nothing', {}, ['lint'], {}),
        ('lint a patch with no line', {'t.res': ok_res, 'e.patch': '# only a comment\n\n'},
         ['lint', '{d}\\e.patch', '--source', '{d}\\t.res'], {}),
        ('lint a patch in UTF-8 with a BOM and bad bytes',
         {'t.res': ok_res, 'u.patch': b'\xef\xbb\xbf# hi\n' + good_patch().encode() + b'# \xff\xfe bad\n'},
         ['lint', '{d}\\u.patch', '--source', '{d}\\t.res'], {}),
        # the Python's own exceptions (struct.error) and a movie the Rust decoder refuses: handed to the Python
        ('check a constant file cut short (struct.error)', {'x.res': res({'a.apt': A, 'a.const': b'Apt constant file\x1a'})},
         ['check', '{d}\\x.res'], {}),
        ('check a BIGF index cut short', {'x.res': b'BIGF' + bytes(4) + struct.pack('>I', 5) + bytes(4)},
         ['check', '{d}\\x.res'], {}),
        ('check a movie cut short', {'x.res': res({'t.apt': A[:0x70], 't.const': K})}, ['check', '{d}\\x.res'], {}),
        ('verify a movie cut short', {'t.res': ok_res, 'x.res': res({'t.apt': A[:0x70], 't.const': K})},
         ['verify', '{d}\\t.res', '{d}\\x.res'], {}),
        ('a movie path outside cp1252', {'Ж.res': ok_res}, ['check', '{d}\\Ж.res'], {}),
        ('a movie path in cp1252', {'café.res': ok_res}, ['check', '{d}\\café.res'], {}),
        ('a missing path outside cp1252', {}, ['check', '{d}\\ЖЗ.res'], {}),
    ]
    out: list[Case] = [(f'aptcheck: {n}', 'aptcheck', f, a, e, False) for n, f, a, e in rows]
    return with_variants(out, ['aptcheck: a movie path outside cp1252', 'aptcheck: a movie path in cp1252',
                               'aptcheck: a missing path outside cp1252',
                               'aptcheck: check the same movie 120 times (stdout over 8,192 bytes)',
                               'aptcheck: check 120 then a missing one',
                               'aptcheck: verify many ranges (stdout over 8,192 bytes)'])


# ------------------------------------------------------------------------------------------- aptrefs

def aptrefs_cases() -> list[Case]:
    ok_res = res(MEMBERS)
    op = fake_opfor()
    plain = bytes(tmt.pe32([('.text', 0x1000, b'\xc3', 0x60000020),
                            ('.rdata', 0x2000, b'\0presenceRequestOnline\0', 0x40000040)]))
    files = {'t.res': ok_res, 'x.exe': bytes(tmt.build_exe()), 'b.exe': exe_795745()}
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('the usage', {}, [], {}),
        ('an unknown command', {}, ['x'], {}),
        ('movie', {'t.res': ok_res}, ['movie', '{d}\\t.res'], {}),
        ('movie twice with --json', {'t.res': ok_res},
         ['movie', '{d}\\t.res', '{d}\\t.res', '--json', '{d}\\o.json'], {}),
        ('movie, --json twice (the last)', {'t.res': ok_res},
         ['movie', '--json', '{d}\\a.json', '{d}\\t.res', '--json', '{d}\\b.json'], {}),
        ('movie none', {}, ['movie'], {}),
        ('movie a missing one', {'t.res': ok_res}, ['movie', '{d}\\t.res', '{d}\\no.res'], {}),
        ('movie from the game copy', op, ['movie', 'Other', 'pristine:t', 'hud'], OP),
        ('movie --json into a missing folder', {'t.res': ok_res},
         ['movie', '{d}\\t.res', '--json', '{d}\\no\\o.json'], {}),
        ('table', files, ['table', '--exe', '{d}\\x.exe'], {}),
        ('table, build 795745', files, ['table', '--exe', '{d}\\b.exe'], {}),
        ('table, the game copy\'s exe', op, ['table'], OP),
        ('table, no exe in the game copy', {'op\\game\\x': b''}, ['table'], OP),
        ('table, a missing --exe', {}, ['table', '--exe', '{d}\\no.exe'], {}),
        ('table, not an exe', {'n.exe': b'hello'}, ['table', '--exe', '{d}\\n.exe'], {}),
        ('table, not 32-bit', {'n.exe': b'MZ' + bytes(0x3A) + struct.pack('<I', 0x40) + b'PE\0\0' + bytes(0x40)},
         ['table', '--exe', '{d}\\n.exe'], {}),
        ('table, no name table', {'p.exe': plain}, ['table', '--exe', '{d}\\p.exe'], {}),
        ('table, --exe without a value', {}, ['table', '--exe'], {}),
        ('xref --movie', files, ['xref', '--exe', '{d}\\x.exe', '--movie', '{d}\\t.res', '--out', '{d}\\x.txt',
                                 '--json', '{d}\\x.json'], {}),
        ('xref --movie, build 795745', files, ['xref', '--exe', '{d}\\b.exe', '--movie', '{d}\\t.res', '--movie',
                                               '{d}\\t.res', '--out', '{d}\\x.txt'], {}),
        ('xref the game copy\'s sets', op, ['xref', '--out', '{d}\\x.txt', '--json', '{d}\\x.json'], OP),
        ('xref the game copy\'s sets, build 795745', {**op, 'op\\game\\BFBC2Game.exe': exe_795745()},
         ['xref', '--out', '{d}\\x.txt', '--json', '{d}\\x.json'], OP),
        ('xref --sets', op, ['xref', '--sets', 'mp_hud,nothing', '--out', '{d}\\x.txt'], OP),
        ('xref, the default outputs', op, ['xref'], OP),
        ('find', files, ['find', 'live', '--exe', '{d}\\x.exe', '--movie', '{d}\\t.res'], {}),
        ('find, build 795745', files, ['find', 'e', '--exe', '{d}\\b.exe', '--movie', '{d}\\t.res'], {}),
        ('find only in the table', files, ['find', 'NAME1', '--exe', '{d}\\x.exe', '--movie', '{d}\\t.res'], {}),
        ('find nothing', files, ['find', 'zzz', '--exe', '{d}\\x.exe', '--movie', '{d}\\t.res'], {}),
        ('find without a name', files, ['find', '--exe', '{d}\\x.exe', '--movie', '{d}\\t.res'], {}),
        ('find in the game copy\'s sets', op, ['find', 'on'], OP),
        ('find a name outside cp1252', files, ['find', 'Ж', '--exe', '{d}\\x.exe', '--movie', '{d}\\t.res'], {}),
        ('a movie path outside cp1252', {'Ж.res': ok_res}, ['movie', '{d}\\Ж.res'], {}),
    ]
    out: list[Case] = [(f'aptrefs: {n}', 'aptrefs', f, a, e, False) for n, f, a, e in rows]
    return with_variants(out, ['aptrefs: a movie path outside cp1252', 'aptrefs: find a name outside cp1252',
                               'aptrefs: table, build 795745'])


# ------------------------------------------------------------------------------------------- bc2loc

def bc2loc_cases() -> list[Case]:
    normal, hist = loc_file(LOC_TEXTS)
    lf = {'language_test_normal.bin': normal, 'language_test_histogram.bin': hist, 't.res': res(MEMBERS)}
    lp = '{d}\\language_test_normal.bin'
    op = fake_opfor()
    dup = bytearray(normal)                       # the first entry's hash written over the second's
    dup[0x24 + 8:0x28 + 8] = dup[0x24:0x28]
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('the usage', {}, [], {}),
        ('an unknown command', {}, ['x'], {}),
        ('get', lf, ['get', 'MENU_PLAY_NOW', 'PC_SERVER_BROWSER', 'QUOTE', 'EMPTY', '--file', lp], {}),
        ('get non-cp1252 texts (replaced)', lf, ['get', 'RU', 'JP', 'HIGH', '--file', lp], {}),
        ('get by hash', lf, ['get', f'{khash("RU"):#x}', '0xffffffff', '--file', lp], {}),
        ('get a missing key', lf, ['get', 'NOPE', '--file', lp], {}),
        ('get a missing and a found key', lf, ['get', 'NOPE', 'RU', '0x12', '--file', lp], {}),
        ('get no key', lf, ['get', '--file', lp], {}),
        ('get without the histogram', {'language_x_normal.bin': normal},
         ['get', 'RU', 'JP', '--file', '{d}\\language_x_normal.bin'], {}),
        ('get, a file named otherwise', {'n.bin': normal, 'n_histogram.bin': hist},
         ['get', 'RU', '--file', '{d}\\n.bin'], {}),
        ('get, an empty histogram', {'language_x_normal.bin': normal, 'language_x_histogram.bin': b''},
         ['get', 'RU', '--file', '{d}\\language_x_normal.bin'], {}),
        ('get, a histogram that is not one', {'language_x_HUGE.bin': normal, 'language_x_histogram.bin': b'12345678'},
         ['get', 'RU', '--file', '{d}\\language_x_HUGE.bin'], {}),
        ('get, duplicate hashes', {'language_x_normal.bin': bytes(dup)},
         ['get', 'MENU_PLAY_NOW', 'RU', 'JP', '--file', '{d}\\language_x_normal.bin'], {}),
        ('get, a histogram cut short (struct.error)', {'language_x_normal.bin': normal, 'language_x_histogram.bin': b'\x01'},
         ['get', 'RU', '--file', '{d}\\language_x_normal.bin'], {}),
        ('get, a missing --file', {}, ['get', 'X', '--file', '{d}\\no.bin'], {}),
        ('get, not a language file', {'n.bin': b'hello' * 10}, ['get', 'X', '--file', '{d}\\n.bin'], {}),
        ('get, a header that does not match', {'n.bin': normal + b'x'}, ['get', 'X', '--file', '{d}\\n.bin'], {}),
        ('get --lang of the game copy', op, ['get', 'KEY_0001', 'RU', '--lang', 'TEST'], OP),
        ('get the default language', op, ['get', 'MENU_PLAY_NOW'], OP),
        ('get a language the game copy lacks', op, ['get', 'X', '--lang', 'klingon'], OP),
        ('search --no-keys', lf, ['search', 'Browser', '--file', lp, '--no-keys'], {}),
        ('search two words', lf, ['search', 'play', 'now', '--file', lp, '--no-keys'], {}),
        ('search, keys from the game copy', {**lf, **op}, ['search', 'e', '--file', lp], OP),
        ('search everything in the big file (stdout over 8,192 bytes)', op,
         ['search', 't', '--lang', 'test', '--no-keys'], OP),
        ('search nothing found', lf, ['search', 'zzz', '--file', lp, '--no-keys'], {}),
        ('search no text', lf, ['search', '--file', lp], {}),
        ('movie', lf, ['movie', '{d}\\t.res', '--file', lp], {}),
        ('movie of the game copy', op, ['movie', 'Other', 'hud2'], OP),
        ('movie none', lf, ['movie', '--file', lp], {}),
        ('movie a missing one', lf, ['movie', '{d}\\no.res', '--file', lp], {}),
        ('dump --no-keys --out', lf, ['dump', '--file', lp, '--no-keys', '--out', '{d}\\d.txt'], {}),
        ('dump with keys --out', {**lf, **op}, ['dump', '--file', lp, '--out', '{d}\\d.txt'], OP),
        ('dump, the default output', op, ['dump', '--lang', 'test'], OP),
        ('dump into a missing folder', lf, ['dump', '--file', lp, '--no-keys', '--out', '{d}\\no\\d.txt'], {}),
        ('langs', op, ['langs'], OP),
        ('set', lf, ['set', 'MENU_PLAY_NOW=PLAY', 'PC_SERVER_BROWSER=SB', 'NEWKEY=new = text', '0x1234=hex',
                     'NEWKEY=again', '--out', '{d}\\out', '--file', lp], {}),
        ('set every language', op, ['set', 'MENU_PLAY_NOW=GO', '--lang', 'all', '--out', '{d}\\out'], OP),
        ('set one language of the game copy', op, ['set', 'KEY_0002=x', '--lang', 'Test', '--out', '{d}\\out'], OP),
        ('set without pairs', lf, ['set', 'x', '--out', '{d}\\out', '--file', lp], {}),
        ('set without --out', lf, ['set', 'a=b', '--file', lp], {}),
        ('set --file with --lang all', lf, ['set', 'a=b', '--out', '{d}\\out', '--file', lp, '--lang', 'all'], {}),
        ('set a non-ASCII text', lf, ['set', 'a=café', '--out', '{d}\\out', '--file', lp], {}),
        ('set a text outside cp1252', lf, ['set', 'a=Ж', '--out', '{d}\\out', '--file', lp], {}),
        ('--lang without a value', lf, ['get', 'x', '--lang'], {}),
        ('a key outside cp1252', lf, ['get', 'ЖЗ', '--file', lp], {}),
        ('a file path outside cp1252', {'Ж.bin': normal}, ['get', 'RU', '--file', '{d}\\Ж.bin'], {}),
    ]
    out: list[Case] = [(f'bc2loc: {n}', 'bc2loc', f, a, e, False) for n, f, a, e in rows]
    return with_variants(out, ['bc2loc: get non-cp1252 texts (replaced)', 'bc2loc: a key outside cp1252',
                               'bc2loc: a file path outside cp1252',
                               'bc2loc: search everything in the big file (stdout over 8,192 bytes)'])


# ------------------------------------------------------------------------------------------- the group

def cases() -> list[Case]:
    return aptcheck_cases() + aptrefs_cases() + bc2loc_cases()


# round RT2: lines whose answer changed on purpose (menucommon.parse_bigf_reference retired: a BIGF index cut short
# is now refused in the archive's words instead of struct's)
_BIGF_REFUSED = ('error: Refused: <dir>\\x.res: a BIGF archive cut short or damaged (its index runs past the data)\r\n'
                 "  what to do: If it happens again, send the log file to the tool's author.\r\n  details: <log>\r\n")
_BIGF_LOG = ('[aptcheck-<date>.log]\nINFO    --- aptcheck started (<who>) ---\nERROR   aptcheck error: Refused: '
             '<dir>\\x.res: a BIGF archive cut short or damaged (its index runs past the data)\n')
NOW: dict[str, dict[str, object]] = {
    'aptcheck: check a BIGF index cut short': {'err': _BIGF_REFUSED, 'log': _BIGF_LOG},
    'aptcheck: check a BIGF index cut short (merged)': {'out': _BIGF_REFUSED, 'log': _BIGF_LOG},
}


OPFOR_MOVIES = [
    'aptgraft/bundles/Dist/win32/levels/mp_common/ingame-00/Menu/TextManagerComponent.res',
    'aptgraft/bundles/Dist/win32/levels/sp_common/loader-00/Menu/sharedGraphics.res',
    'aptgraft/bundles/Package/levels/mp_001/ingame-00/Menu/hudlayer_mp_flagcapture.res',
    'aptgraft/bundles/Package/levels/mp_002/ingame-00/Menu/hudlayer_mp_goldrush.res',
    'aptgraft/bundles/Package/levels/mp_005/ingame-00/Menu/hudlayer_mp_flagcapture.res',
    'aptgraft/bundles/Package/levels/mp_009sr/ingame-00/Menu/hudlayer_mp_goldrush.res',
    'aptgraft/bundles/Package/levels/ons_mp_004/ingame-00/Menu/eor_squad_ranking.res',
    'aptgraft/bundles/Package/levels/sp_003_b/ingame-00/Menu/hud_root.res',
    'aptgraft/bundles/Package/levels/sp_007/loader-00/Menu/weaponicon.res',
    'aptgraft/bundles/Package/levels/sp_03/ingame-00/Menu/weaponicon.res',
    'aptgraft/bundles/Dist/win32/menu/main/mainmenu-00/Menu/main_nav.res',
    'console_hud/ons_mp_002/loader-00/Menu/loadingscreen_mp.res',
    'console_hud/ons_mp_005/loadingscreen_mp.3.res',
    'console_loading/ons_mp_005/loader/Menu/eor_squad_ranking.res',
    'onslaught/build_ons_mp_002/pc/mp_002_ingame_pkg/Menu/hudlayer_mp_goldrush.res',
    'onslaught/build_ons_mp_004/pc/mp_common_ingame_dist/Menu/hud_crosshair.res',
    'onslaught/build_ons_mp_008/pc/mp_008_ingame_pkg/Menu/eor_squad_ranking.res',
    'ww2/menu_install/apply/mainmenu/Menu/sharedGraphics.res',
    'ww2/wake_ui/loader/Menu/eor_scoreboard.res',
]


def opfor_calls(opfor: str) -> list[Call]:
    """Project OPFOR's real calls of the menu tools, read-only (the game copy read in place; work\\ inputs copied)."""
    env = {'OPFOR_ROOT': opfor}
    out: list[Call] = []
    # aptcheck check of the movies OPFOR's steps edit and check (console_icons, menu_install, duplicates, type95_icon,
    # wake_ui, weapon_icons, ww2_crosshairs): the edited .res files they leave in work\
    # (a sample of OPFOR_MOVIES' kinds, picked 2026-11-19 from every BIGF .res under work\: a recursive search of work\
    # takes minutes, so the sample is listed; those still there are replayed)
    movies = [os.path.join(opfor, 'work', *rel.split('/')) for rel in OPFOR_MOVIES]
    movies = [p for p in movies if os.path.isfile(p)]
    for p in movies:
        out.append((f'aptcheck check {os.path.relpath(p, opfor)}', 'aptcheck', ['check', '{d}\\m.res'], {'m.res': p},
                    env, False))
    for p in [p for p in movies if os.path.basename(p).lower() == 'main_nav.res'][:1]:
        out.append((f'aptcheck verify pristine:main_nav {os.path.relpath(p, opfor)}', 'aptcheck',
                    ['verify', 'pristine:main_nav', '{d}\\m.res'], {'m.res': p}, env, False))
    for spec in ('main_nav', 'pristine:main_nav', 'main_root', 'pristine:main_root'):
        out.append((f'aptcheck check {spec}', 'aptcheck', ['check', spec], {}, env, True))
    patches = sorted(glob.glob(os.path.join(opfor, 'src', 'menus', '*.patch')))
    if patches:
        files = {f'p\\{os.path.basename(p)}': p for p in patches}
        out.append(('aptcheck lint src\\menus\\*.patch --brief', 'aptcheck', ['lint', '{d}\\p\\*.patch', '--brief'],
                    files, env, True))
        out.append(('aptcheck lint src\\menus\\*.patch --installed', 'aptcheck',
                    ['lint', '{d}\\p\\*.patch', '--installed'], files, env, False))
    for args in (['table'], ['movie', 'main_nav', 'main_root'], ['find', 'presence'],
                 ['find', 'isLive', '--sets', 'menu'], ['xref', '--out', '{d}\\x.txt', '--json', '{d}\\x.json']):
        out.append((f'aptrefs {" ".join(a for a in args if "{d}" not in a)}', 'aptrefs', args, {}, env, True))
    for args in (['langs'], ['get', 'MENU_PLAY_NOW', 'PC_SERVER_BROWSER', 'IDK_A'],
                 ['get', 'MENU_PLAY_NOW', '--lang', 'russian'], ['get', 'MENU_PLAY_NOW', '--lang', 'japanese'],
                 ['search', 'server browser', '--no-keys'], ['search', 'kill', '--lang', 'russian', '--no-keys'],
                 ['search', 'browser'], ['movie', 'main_nav'], ['dump', '--no-keys', '--out', '{d}\\d.txt'],
                 ['dump', '--lang', 'japanese', '--out', '{d}\\d.txt'],
                 ['set', 'MENU_PLAY_NOW=PLAY', '--lang', 'all', '--out', '{d}\\out']):
        out.append((f'bc2loc {" ".join(a for a in args if "{d}" not in a)}', 'bc2loc', args, {}, env, True))
    return out
