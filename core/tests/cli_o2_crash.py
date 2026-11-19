r"""Round O2's crash group: the command lines test_cli_o2.py replays for exeref.py and crashreader.py
(bfbc2-automated-map-and-bot-test-harness\crash-reader).

cases(): the quick tier's, on synthetic inputs made in the case folder (small 32-bit PE files, crash reports, symbols
and opfor.log files; no game data). Every case sets OPFOR_ROOT to a game copy made in the case folder (or passes
--exe), so neither tool finds the real one by walking up from the suite.
opfor_calls(opfor): the --game tier's: Project OPFOR's real calls (testrun.py's crashreader line on the real crash
reports, with the real exe and opfor.log read in place), crash-like reports at real addresses, and exeref lines on
the real BFBC2Game.exe.
"""
from __future__ import annotations

import calendar
import glob
import os
import random
import struct
import time
from typing import Any

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]

VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}
OPFOR = {'OPFOR_ROOT': '{d}\\opfor'}
BASE = 0x400000


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    """rows, each again with stderr on stdout's pipe, and the picked ones under Python's stdio switches."""
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


# ------------------------------------------------------------------------------------------- built exes

def pe32(sections: list[tuple[str, int, bytes, int]], base: int = BASE) -> bytes:
    """A minimal valid PE32 image: [(name, rva, data, characteristics)] (test_exeref.pe32)."""
    falign, salign, head = 0x200, 0x1000, 0x400
    raw, pos = [], head
    for name, rva, data, ch in sections:
        size = (len(data) + falign - 1) // falign * falign
        raw.append((name, rva, data, ch, pos, size))
        pos += size
    image = max(rva + ((len(d) + salign - 1) // salign) * salign for _, rva, d, _ in sections)
    out = bytearray(pos)
    out[0:2] = b'MZ'
    struct.pack_into('<I', out, 0x3C, 0x80)
    out[0x80:0x84] = b'PE\0\0'
    struct.pack_into('<HHIIIHH', out, 0x84, 0x14C, len(sections), 0x12345678, 0, 0, 0xE0, 0x0102)
    code = sum(s[5] for s in raw if s[3] & 0x20)
    struct.pack_into('<HBBIIIIIIIIIHHHHHHIIIIHHIIIIII', out, 0x98, 0x10B, 9, 0, code, 0, 0, sections[0][1],
                     sections[0][1], sections[-1][1], base, salign, falign, 5, 1, 0, 0, 5, 1, 0, image, head, 0, 3, 0,
                     0x100000, 0x1000, 0x100000, 0x1000, 0, 16)
    for k, (name, rva, data, ch, ptr, size) in enumerate(raw):
        struct.pack_into('<8sIIIIIIHHI', out, 0x98 + 0xE0 + 40 * k, name.encode(), len(data), rva, size, ptr, 0, 0, 0,
                         0, ch)
        out[ptr:ptr + len(data)] = data
    return bytes(out)


def build(extra_strings: int = 0) -> tuple[bytes, dict[str, int]]:
    """Code: 16 int3, f1 at 0x401010 (push ebp; mov ebp, esp; push "Server.TestSetting"; call f2; add esp, 4; pop
    ebp; ret), f2 at 0x401030 (mov eax, table; ret), f3 at 0x401040 (push ebp; mov ebp, esp; mov ecx, [ebp+8];
    mov eax, [ecx+0x1C]; mov edx, [eax+ecx*4-0x10]; call f1; push "Server.Other"; call [table]; pop ebp; ret),
    f4 at 0x401070 (jmp f1). Data: strings (a Latin-1 one, one with a byte cp1252 lacks), a table holding addresses,
    and extra_strings more "Server.Extra<n>" strings each pushed by a code snippet (long outputs)."""
    rdata = bytearray(b'\0' * 4)
    names = [b'Server.TestSetting', b'Server.Other', b'Caf\xe9.Setting', b'A\x81B', b'tab\there', b'Win32',
             b'x', b'not\x01printable Server']
    va: dict[str, int] = {}
    for n in names:
        va[n.decode('latin-1')] = BASE + 0x2000 + len(rdata)
        rdata += n + b'\0'
    extra = []
    for i in range(extra_strings):
        extra.append(BASE + 0x2000 + len(rdata))
        rdata += b'Server.Extra%04d.SomeLongSettingNameToFillTheLines\0' % i
    rdata += b'\0' * ((-len(rdata)) % 4)
    table_va = BASE + 0x2000 + len(rdata)
    f1, f2, f3, f4 = BASE + 0x1010, BASE + 0x1030, BASE + 0x1040, BASE + 0x1070
    rdata += struct.pack('<IIII', va['Server.TestSetting'], 7, f1, f3)
    code = bytearray(b'\xcc' * 0x10)
    code += b'\x55\x8b\xec' + b'\x68' + struct.pack('<I', va['Server.TestSetting'])
    call_at = BASE + 0x1000 + len(code)
    code += b'\xe8' + struct.pack('<i', f2 - (call_at + 5))
    code += b'\x83\xc4\x04\x5d\xc3'
    code += b'\xcc' * (0x30 - len(code))
    code += b'\xb8' + struct.pack('<I', table_va) + b'\xc3'
    code += b'\xcc' * (0x40 - len(code))
    code += b'\x55\x8b\xec\x8b\x4d\x08\x8b\x41\x1c\x8b\x54\x88\xf0'
    c2 = BASE + 0x1000 + len(code)
    code += b'\xe8' + struct.pack('<i', f1 - (c2 + 5))
    code += b'\x68' + struct.pack('<I', va['Server.Other'])
    code += b'\xff\x15' + struct.pack('<I', table_va + 8) + b'\x5d\xc3'
    code += b'\xcc' * (0x70 - len(code))
    j = BASE + 0x1000 + len(code)
    code += b'\xe9' + struct.pack('<i', f1 - (j + 5))
    code += b'\xcc' * 11
    for v in extra:                          # each: push <string>; ret, 16-byte aligned after int3
        code += b'\x68' + struct.pack('<I', v) + b'\xc3' + b'\xcc' * 10
    exe = pe32([('.text', 0x1000, bytes(code), 0x60000020), ('.rdata', 0x2000, bytes(rdata), 0x40000040)])
    return exe, dict(f1=f1, f2=f2, f3=f3, f4=f4, call=call_at, call2=c2, table=table_va, s=va['Server.TestSetting'],
                     other=va['Server.Other'], push=f1 + 3, load=f3 + 6, jmp=j)


def random_exe(seed: int) -> bytes:
    """An exe of random code (test_crash_native.random_exe): plausible instructions with random bytes among them,
    int3 padding, calls between functions, strings and a pointer table; some instructions are outside the Rust
    decoder's proof (the command line then goes to the Python)."""
    rng = random.Random(seed)
    strings = [b'Server.' + bytes(rng.choice(b'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz')
                                  for _ in range(rng.randrange(3, 20))) for _ in range(12)]
    rdata = bytearray(b'\0' * 4)
    s_va = []
    for s in strings:
        s_va.append(BASE + 0x40000 + len(rdata))
        rdata += s + b'\0'
    rdata += b'\0' * ((-len(rdata)) % 4)
    rdata += b''.join(struct.pack('<I', v) for v in s_va)
    pieces = [b'\x55\x8b\xec', b'\x8b\x45\x08', b'\x89\x4c\x24\x10', b'\x83\xc4\x04', b'\x5d\xc3', b'\x85\xc0',
              b'\x74\x05', b'\x0f\x84\x10\x00\x00\x00', b'\xd9\x44\x24\x04', b'\xde\xc1', b'\xf3\x0f\x10\x45\x08',
              b'\x66\x8b\x01', b'\xc2\x08\x00', b'\x8d\x4c\x24\x20', b'\x33\xc0', b'\xf3\xa5', b'\x0f\xb6\x41\x02',
              b'\x6a\xff']
    code = bytearray(b'\xcc' * 16)
    funcs: list[int] = []
    for _ in range(40):
        while len(code) % 16:
            code += b'\xcc'
        funcs.append(BASE + 0x1000 + len(code))
        for _ in range(rng.randrange(4, 30)):
            r = rng.random()
            if r < 0.6:
                code += rng.choice(pieces)
            elif r < 0.7:
                code += b'\x68' + struct.pack('<I', rng.choice(s_va))
            elif r < 0.8 and len(funcs) > 1:
                at = BASE + 0x1000 + len(code)
                code += b'\xe8' + struct.pack('<i', rng.choice(funcs) - (at + 5))
            else:
                code += bytes(rng.randrange(256) for _ in range(rng.randrange(1, 6)))
        code += b'\xc3' + b'\xcc' * rng.randrange(1, 8)
    return pe32([('.text', 0x1000, bytes(code), 0x60000020), ('.rdata', 0x40000, bytes(rdata), 0x40000040)])


SYMBOLS = ('# test symbols\r\n00401010  first function  | f1 hint: a test hint\r\n'
           '0x401030 second function\r\n  00401040\tthird (three)   |   \r\n00401070 jumper # comment\r\n'
           'zz not a line\r\n0040104A  crash site in f3 | the load through ecx: a null object\r\n')


def game_files(exe: bytes, log: bytes | None = None) -> dict[str, Any]:
    """The fake game copy OPFOR_ROOT names: opfor\\game\\BFBC2Game.exe (and its logs\\opfor.log)."""
    out: dict[str, Any] = {'opfor\\game\\BFBC2Game.exe': exe}
    if log is not None:
        out['opfor\\game\\logs\\opfor.log'] = log
    return out


# ------------------------------------------------------------------------------------------- exeref

def exeref_cases() -> list[Case]:
    exe, at = build()
    big, _ = build(160)
    g = game_files(exe)
    x = dict(g, **{'x.exe': exe})
    rnd = random_exe(7)
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('the usage', {}, [], {}),
        ('an unknown command', {}, ['what'], {}),
        ('str', x, ['str', 'Server', '--exe', '{d}\\x.exe'], {}),
        ('str, the exe found (OPFOR_ROOT)', g, ['str', 'TestSetting'], OPFOR),
        ('str --dis', x, ['str', 'testsetting', '--dis', '--exe', '{d}\\x.exe'], {}),
        ('str --dis 2 last', x, ['str', 'Server', '--exe', '{d}\\x.exe', '--dis', '2'], {}),
        ('str --exact, another case', x, ['str', 'server.testsetting', '--exact', '--exe', '{d}\\x.exe'], {}),
        ('str --exact', x, ['str', 'Server.TestSetting', '--exact', '--exe', '{d}\\x.exe'], {}),
        ('str of two words', x, ['str', 'not', 'printable', '--exe', '{d}\\x.exe'], {}),
        ('str with no string', x, ['str', 'NoSuchThing', '--exe', '{d}\\x.exe'], {}),
        ('str --max 1', x, ['str', 'e', '--max', '1', '--exe', '{d}\\x.exe'], {}),
        ('str --max 0', x, ['str', 'Server', '--max', '0', '--exe', '{d}\\x.exe'], {}),
        ('str --max without a value', x, ['str', 'Server', '--exe', '{d}\\x.exe', '--max'], {}),
        ('str --max abc', x, ['str', 'Server', '--max', 'abc', '--exe', '{d}\\x.exe'], {}),
        ('str --max -1', x, ['str', 'Server', '--max', '-1', '--exe', '{d}\\x.exe'], {}),
        ('str of Latin-1 text', x, ['str', 'Café', '--exe', '{d}\\x.exe'], {}),
        ('str of text outside Latin-1', x, ['str', 'CafЖЗ', '--exe', '{d}\\x.exe'], {}),
        ('str --exact of a byte cp1252 lacks', x, ['str', 'A\x81B', '--exact', '--exe', '{d}\\x.exe'], {}),
        ('str --exact with a tab', x, ['str', 'tab\there', '--exact', '--exe', '{d}\\x.exe'], {}),
        ('str with no text', x, ['str', '--exe', '{d}\\x.exe'], {}),
        ('str with a NUL-free empty text', x, ['str', '', '--exe', '{d}\\x.exe', '--max', '3'], {}),
        ('str: 160 strings, over 8,192 bytes', {'x.exe': big}, ['str', 'Extra', '--max', '200', '--exe', '{d}\\x.exe'],
         {}),
        ('refs of a string', x, ['refs', f'{at["s"]:#x}', '--exe', '{d}\\x.exe'], {}),
        ('refs of a function --dis', x, ['refs', f'{at["f1"]:#010x}', '--dis', '--exe', '{d}\\x.exe'], {}),
        ('refs of a function --dis 1, symbols', x, ['refs', f'{at["f1"]:X}', '--dis', '1', '--exe', '{d}\\x.exe',
                                                   '--symbols', '{d}\\s.txt'], {}),
        ('refs of the table (held in data)', x, ['refs', f'{at["table"]:#x}', '--exe', '{d}\\x.exe'], {}),
        ('refs of f2 (a call), --max 1', x, ['refs', f'{at["f2"]:#x}', '--max', '1', '--exe', '{d}\\x.exe'], {}),
        ('refs of nothing', x, ['refs', '0x12345678', '--exe', '{d}\\x.exe'], {}),
        ('refs in decimal', x, ['refs', str(at['f2']), '--exe', '{d}\\x.exe'], {}),
        ('refs of a bad address', x, ['refs', 'zz', '--exe', '{d}\\x.exe'], {}),
        ('refs of a negative address', x, ['refs', '-5', '--exe', '{d}\\x.exe'], {}),
        ('refs without an address', x, ['refs', '--exe', '{d}\\x.exe'], {}),
        ('dis', x, ['dis', f'{at["f1"]:#x}', '--exe', '{d}\\x.exe'], {}),
        ('dis inside an instruction', x, ['dis', f'{at["push"] + 2:#x}', '--exe', '{d}\\x.exe'], {}),
        ('dis --before / --after', x, ['dis', f'{at["load"]:#x}', '--before', '0x10', '--after', '8', '--exe',
                                       '{d}\\x.exe', '--symbols', '{d}\\s.txt'], {}),
        ('dis, six hex digits', x, ['dis', f'{at["f3"]:06X}', '--exe', '{d}\\x.exe'], {}),
        ('dis 0o and underscores', x, ['dis', '0o20010_060', '--exe', '{d}\\x.exe'], {}),
        ('dis, a big --after', x, ['dis', f'{at["f1"]:#x}', '--after', '0x1000', '--before', '0x900', '--exe',
                                   '{d}\\x.exe'], {}),
        ('dis of data', x, ['dis', f'{at["s"]:#x}', '--exe', '{d}\\x.exe'], {}),
        ('dis of a negative address', x, ['dis', '-0x10', '--exe', '{d}\\x.exe'], {}),
        ('dis --before zz', x, ['dis', f'{at["f1"]:#x}', '--before', 'zz', '--exe', '{d}\\x.exe'], {}),
        ('dis --dis abc', x, ['dis', f'{at["f1"]:#x}', '--dis', 'abc', '--exe', '{d}\\x.exe'], {}),
        ('--exe without a value', x, ['dis', '0x401010', '--exe'], {}),
        ('--exe followed by an option', x, ['dis', '0x401010', '--exe', '--max', '3'], {}),
        ('--exe missing', {}, ['dis', '0x401010', '--exe', '{d}\\none.exe'], {}),
        ('--exe missing, outside cp1252', {}, ['dis', '0x401010', '--exe', '{d}\\Ж.exe'], {}),
        ('--exe a folder', {'f': None}, ['dis', '0x401010', '--exe', '{d}\\f'], {}),
        ('--exe empty (the game copy\'s)', g, ['dis', '0x401010', '--exe', ''], OPFOR),
        ('--exe not a PE file', {'x.exe': b'MZ' + bytes(200)}, ['dis', '0x401010', '--exe', '{d}\\x.exe'], {}),
        ('symbols in bad UTF-8', dict(x, **{'b.txt': b'00401010 caf\xe9\n'}),
         ['dis', '0x401010', '--exe', '{d}\\x.exe', '--symbols', '{d}\\b.txt'], {}),
        ('symbols missing, and --symbols last', x, ['dis', '0x401040', '--exe', '{d}\\x.exe', '--symbols',
                                                    '{d}\\none.txt', '--symbols'], {}),
        ('a random exe: dis', {'x.exe': rnd}, ['dis', '0x401010', '--exe', '{d}\\x.exe'], {}),
        ('a random exe: str --dis', {'x.exe': rnd}, ['str', 'Server', '--dis', '--exe', '{d}\\x.exe'], {}),
        ('a random exe: refs', {'x.exe': rnd}, ['refs', '0x401010', '--exe', '{d}\\x.exe'], {}),
    ]
    out: list[Case] = []
    for name, files, args, env in rows:
        files = dict(files)
        if any(a == '{d}\\s.txt' for a in args):
            files['s.txt'] = SYMBOLS
        out.append((f'exeref: {name}', 'exeref', files, args, env, False))
    return with_variants(out, ['exeref: str --exact of a byte cp1252 lacks', 'exeref: --exe missing, outside cp1252',
                               'exeref: str: 160 strings, over 8,192 bytes', 'exeref: str of text outside Latin-1',
                               'exeref: str of Latin-1 text'])


# ------------------------------------------------------------------------------------------- crashreader

def report(eip: int | None, regs: dict[str, int], stack: list[int], build: str = '795745', category: str | None = None,
           sku: str = 'ea.dice.RomePC.Cl', head: str = '<?xml version="1.0" encoding="UTF-8"?>') -> str:
    reg_text = ', '.join(f'{k} {v:08x}' for k, v in regs.items())
    cat = category if category is not None else (f' 0x{eip:X}' if eip is not None else '')
    eip_text = f', Eip {eip:08x}' if eip is not None else ''
    return (f'{head}<report><version>2</version><sessionid>0123456789abcdef</sessionid>'
            f'<type>crash</type><sku>{sku}</sku><createtime>2026-10-16 12:00:00</createtime>'
            f'<buildsignature>{build}</buildsignature><systemconfig></systemconfig><categoryid>{cat}</categoryid>'
            f'<stack> {" ".join(f"0x{a:X}" for a in stack)}</stack><threads></threads><screenshot></screenshot>'
            f'<memdump></memdump><contextdata>registers: {reg_text}{eip_text}, Esp 001ae65c</contextdata></report>')


def local(y: int, mo: int, d: int, h: int, mi: int, s: int) -> float:
    """The epoch time of a local wall-clock time (this PC's time zone)."""
    return time.mktime((y, mo, d, h, mi, s, 0, 0, -1))


def log_text(when: float, lines: int = 30, extra: str = '', crash_at: float | None = None) -> bytes:
    """An opfor.log: lines with [HH:MM:SS] stamps (local), spam the reader leaves out, a crash line at crash_at."""
    t0 = time.localtime(crash_at if crash_at is not None else when)
    hms = lambda k: f'[{t0.tm_hour:02d}:{t0.tm_min:02d}:{max(0, t0.tm_sec - k):02d}]'      # noqa: E731
    out = [f'{hms(30)} game started']
    for k in range(lines):
        out.append(f'{hms(20)} line {k}: loading level stuff' + (' Kynapse error x' if k % 7 == 3 else '')
                   + (' per frame y' if k % 11 == 5 else ''))
    out.append(f'{hms(1)} {extra}' if extra else f'{hms(1)} last line ' + 'z' * 250)
    out.append(f'{hms(0)} crash handler: crashreport.xml written')
    out.append(f'{hms(0)} after the crash')
    return ('\r\n'.join(out) + '\r\n').encode('utf-8')


def crashreader_cases() -> list[Case]:
    exe, at = build()
    t = local(2026, 7, 15, 14, 30, 5)                      # summer time where the zone has it
    w = local(2026, 1, 20, 9, 5, 59)                       # winter
    good = report(at['load'], {'Eax': 0x12345678, 'Ebx': 0, 'Ecx': 0x0, 'Edx': 0x10, 'Esi': 0x1A015DA0},
                  [at['load'], at['call'] + 5, at['call2'] + 5, 0x7560A7DF, 0x41424344, at['s'], at['f1'] + 1,
                   0x401016, 0x500000, 0x1])
    g = game_files(exe)
    log = log_text(t)
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('-h', {}, ['-h'], OPFOR),
        ('--help after the report', {}, ['{d}\\r.xml', '--help'], OPFOR),
        ('a report, the exe found, its log', dict(game_files(exe, log), **{'r.xml': (good, t)}), ['{d}\\r.xml'], OPFOR),
        ('a report --exe --log (testrun.py)', dict(g, **{'r.xml': (good, t), 'x.exe': exe, 'o.log': log}),
         ['{d}\\r.xml', '--exe', '{d}\\x.exe', '--log', '{d}\\o.log'], OPFOR),
        ('symbols with hints', dict(g, **{'r.xml': (good, w), 's.txt': SYMBOLS}),
         ['{d}\\r.xml', '--symbols', '{d}\\s.txt'], OPFOR),
        ('--no-code', dict(g, **{'r.xml': (good, w), 's.txt': SYMBOLS}),
         ['--no-code', '{d}\\r.xml', '--symbols', '{d}\\s.txt'], OPFOR),
        ('a crash at a symbol (hint), null registers', dict(g, **{'r.xml': (report(
            at['f1'], {'Eax': 0, 'Ecx': 5, 'Ebp': 0}, [at['f1'], at['f3'] + 0x10, 0x20202020]), w), 's.txt': SYMBOLS}),
         ['{d}\\r.xml', '--symbols', '{d}\\s.txt'], OPFOR),
        ('Eip 0: the category', dict(g, **{'r.xml': (report(None, {'Eip': 0, 'Eax': 1}, [at['f2']],
                                                              category=f' 0x{at["f2"]:x}'), w)}), ['{d}\\r.xml'], OPFOR),
        ('no address', dict(g, **{'r.xml': (report(None, {}, [], category='none'), w)}), ['{d}\\r.xml'], OPFOR),
        ('another build', dict(g, **{'r.xml': (report(at['f2'], {}, [at['f2']], build='123'), w)}), ['{d}\\r.xml'],
         OPFOR),
        ('an address outside the code', dict(g, **{'r.xml': (report(0x77001234, {'Edi': 0xFFFF}, [0x77001234]), w)}),
         ['{d}\\r.xml'], OPFOR),
        ('a long stack', dict(g, **{'r.xml': (report(at['f1'], {}, [at['f1'] + k for k in range(60)]), w),
                                     's.txt': SYMBOLS}), ['{d}\\r.xml', '--symbols', '{d}\\s.txt'], OPFOR),
        ('long hints (over 8,192 bytes)', dict(g, **{'r.xml': (report(at['f1'], {}, [at['f1'] + 1] * 45), w),
                                                      's.txt': '00401010 f1 | ' + 'a long hint ' * 30 + '\n'}),
         ['{d}\\r.xml', '--symbols', '{d}\\s.txt'], OPFOR),
        ('no report anywhere', {'home\\Documents\\x.txt': ''}, [], dict(OPFOR, USERPROFILE='{d}\\home')),
        ('the newest report in Documents', dict(g, **{
            'home\\Documents\\Project OPFOR\\BFBC2\\crashreport.xml': (good, w),
            'home\\OneDrive - Work\\Documents\\BFBC2\\crashreport.xml': (report(at['f2'], {}, []), t)}),
         [], dict(OPFOR, USERPROFILE='{d}\\home')),
        ('the report in OneDrive\\Documents', dict(g, **{
            'home\\OneDrive\\Documents\\BFBC2\\crashreport.xml': (good, w)}), [], dict(OPFOR, USERPROFILE='{d}\\home')),
        ('a missing report', g, ['{d}\\none.xml'], OPFOR),
        ('a missing report outside cp1252', g, ['{d}\\Ж.xml'], OPFOR),
        ('a folder as the report', dict(g, f=None), ['{d}\\f'], OPFOR),
        ('a report that is not XML', dict(g, **{'r.xml': 'not xml <'}), ['{d}\\r.xml'], OPFOR),
        ('a report with entities, CDATA, comments, CRLF', dict(g, **{'r.xml': (
            '<?xml version="1.0"?>\r\n<!-- c --><report>\r\n<type>cr&amp;ash&#33;</type><sku><![CDATA[a<b]]> x</sku>'
            '<createtime>t<!-- mid -->ime</createtime><stack>0x401010 zz 0X5 0x0000000000401030\r\n0x401011</stack>'
            '<categoryid>0x401010</categoryid><contextdata>EAX 00000000 eip 00401040 ecx 1234567 '
            'Esi 12345678ff</contextdata><buildsignature> 795745 </buildsignature></report>\r\n', w)}),
         ['{d}\\r.xml'], OPFOR),
        ('a report with a DOCTYPE', dict(g, **{'r.xml': ('<!DOCTYPE report><report><type>x</type></report>', w)}),
         ['{d}\\r.xml'], OPFOR),
        ('a report in UTF-16', dict(g, **{'r.xml': (good.replace('UTF-8', 'UTF-16').encode('utf-16'), w)}),
         ['{d}\\r.xml'], OPFOR),
        ('a report outside cp1252 (stdout)', dict(g, **{'r.xml': (report(at['f1'], {}, [], sku='Ж sku'), w)}),
         ['{d}\\r.xml'], OPFOR),
        ('a report in Latin-1 text', dict(g, **{'r.xml': (report(at['f1'], {}, [], sku='café'), w)}),
         ['{d}\\r.xml'], OPFOR),
        ('--exe missing', dict(g, **{'r.xml': (good, w)}), ['{d}\\r.xml', '--exe', '{d}\\none.exe'], OPFOR),
        ('--exe a folder', dict(g, **{'r.xml': (good, w)}, f=None), ['{d}\\r.xml', '--exe', '{d}\\f'], OPFOR),
        ('--exe not a PE file', dict(g, **{'r.xml': (good, w), 'x.exe': b'MZ' + bytes(100)}),
         ['{d}\\r.xml', '--exe', '{d}\\x.exe'], OPFOR),
        ('--exe as the last argument', dict(g, **{'r.xml': (good, w)}), ['{d}\\r.xml', '--exe'], OPFOR),
        ('--log missing', dict(g, **{'r.xml': (good, t)}), ['{d}\\r.xml', '--log', '{d}\\none.log'], OPFOR),
        ('a log with no crash at the time', dict(g, **{'r.xml': (good, t), 'o.log': log_text(t, crash_at=w)}),
         ['{d}\\r.xml', '--log', '{d}\\o.log'], OPFOR),
        ('a log line outside cp1252', dict(g, **{'r.xml': (good, t), 'o.log': log_text(t, extra='Ж café')}),
         ['{d}\\r.xml', '--log', '{d}\\o.log'], OPFOR),
        ('a log line in Latin-1 text', dict(g, **{'r.xml': (good, t), 'o.log': log_text(t, extra='café ok')}),
         ['{d}\\r.xml', '--log', '{d}\\o.log'], OPFOR),
        ('the reference log reader', dict(g, **{'r.xml': (good, t), 'o.log': log}),
         ['{d}\\r.xml', '--log', '{d}\\o.log'], dict(OPFOR, BFBC2_CRASHREADER_REFERENCE='1')),
        ('a random exe', dict(g, **{'r.xml': (report(0x401020, {'Eax': 0}, [0x401010 + 7 * k for k in range(40)]), w),
                                    'x.exe': random_exe(3)}), ['{d}\\r.xml', '--exe', '{d}\\x.exe'], OPFOR),
    ]
    # local times around this PC's daylight-saving changes (UTC instants), and a fractional time that rounds up
    for label, ts in (('March change', calendar.timegm((2026, 3, 8, 6, 30, 0))),
                      ('March change + 1 h', calendar.timegm((2026, 3, 8, 7, 30, 0))),
                      ('November change', calendar.timegm((2026, 11, 1, 5, 30, 0))),
                      ('November change + 1 h', calendar.timegm((2026, 11, 1, 6, 30, 0))),
                      ('European change', calendar.timegm((2026, 10, 25, 0, 59, 59))),
                      ('a time that rounds up', calendar.timegm((2026, 5, 1, 10, 0, 0)) + 0.9999996),
                      ('a time just under a second', calendar.timegm((2026, 5, 1, 10, 0, 0)) + 0.999999)):
        rows.append((f'the report\'s time: {label}', dict(g, **{'r.xml': (good, ts), 'o.log': log_text(ts)}),
                     ['{d}\\r.xml', '--log', '{d}\\o.log', '--no-code'], OPFOR))
    out: list[Case] = [(f'crashreader: {name}', 'crashreader', files, args, env, False) for name, files, args, env in rows]
    return with_variants(out, ['crashreader: a report outside cp1252 (stdout)', 'crashreader: a log line outside cp1252',
                               'crashreader: long hints (over 8,192 bytes)', 'crashreader: a missing report outside cp1252',
                               'crashreader: a report in Latin-1 text', 'crashreader: a log line in Latin-1 text'])


# ------------------------------------------------------------------------------------------- the group

def cases() -> list[Case]:
    return exeref_cases() + crashreader_cases()


def opfor_calls(opfor: str) -> list[Call]:
    exe = os.path.join(opfor, 'game', 'BFBC2Game.exe')
    log = os.path.join(opfor, 'game', 'logs', 'opfor.log')
    if not os.path.isfile(exe):
        return []
    env = {'OPFOR_ROOT': opfor}
    out: list[Call] = []
    home = os.path.expanduser('~')
    reports = []
    for d in [os.path.join(home, 'Documents'), os.path.join(home, 'OneDrive', 'Documents')] + \
            glob.glob(os.path.join(home, 'OneDrive*', 'Documents')):
        for sub in ('Project OPFOR\\BFBC2', 'BFBC2'):
            p = os.path.join(d, sub, 'crashreport.xml')
            if os.path.isfile(p) and p not in reports:
                reports.append(p)
    for k, rep in enumerate(reports):
        src = {'crashreport.xml': rep}
        tag = f'report {k + 1} ({os.path.basename(os.path.dirname(os.path.dirname(rep)))}\\BFBC2)'
        # testrun.py: crashreader.py <report> --exe <game>\BFBC2Game.exe --log <game>\logs\opfor.log
        out.append((f'crashreader {tag} --exe --log (testrun.py)', 'crashreader',
                    ['{d}\\crashreport.xml', '--exe', exe, '--log', log], src, env, True))
        out.append((f'crashreader {tag}, the log found beside the exe', 'crashreader', ['{d}\\crashreport.xml'], src,
                    env, True))
        out.append((f'crashreader {tag} --no-code', 'crashreader', ['{d}\\crashreport.xml', '--no-code'], src, env,
                    True))
    if reports:                                          # the newest report found in Documents, read in place
        out.append(('crashreader: the newest report in Documents', 'crashreader', ['--exe', exe], {}, env, True))
    # crash-like reports at real addresses: symbols_795745.txt's crash sites and functions, the log's times
    sym_path = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
                            'bfbc2-automated-map-and-bot-test-harness', 'crash-reader', 'symbols_795745.txt')
    syms = []
    with open(sym_path, encoding='utf-8') as f:
        for line in f:
            line = line.split('#', 1)[0].strip()
            if line[:1].isalnum():
                syms.append(int(line.split()[0], 16))
    when = os.path.getmtime(reports[0]) if reports else time.time()
    rng = random.Random(5)
    for eip in ([0x006EFF8A, 0x005AC3F0 + 0x21, 0x008F4720 + 0x40, 0x00401090, 0x0061BE40 + 0x13,
                             0x00753540 + 0x30]):
        stack = [eip] + [rng.choice(syms) + rng.randrange(0, 0x80) for _ in range(20)] + [0x7560A7DF, 0x434F5250]
        rep = report(eip, {'Eax': 0, 'Ecx': 0x8, 'Edx': 0x1A015DA0, 'Esi': 0, 'Edi': 0x19FDB118, 'Ebp': 0x19FDB458},
                     stack)
        out.append((f'crashreader: a crash-like report at 0x{eip:08X}', 'crashreader',
                    ['{d}\\crashreport.xml', '--exe', exe, '--log', log], {'crashreport.xml': (rep.encode(), when)},
                    env, True))
    # exeref on the real exe: strings, the symbols' addresses, windows
    for args in (['str', 'presenceRequestOnline'], ['str', 'presenceRequestOnline', '--exact', '--dis'],
                 ['str', 'LevelData', '--max', '5'], ['str', 'crashreport'], ['refs', '0x006EFBC0'],
                 ['refs', '0x005AC3F0', '--dis', '2'], ['refs', '0x00401090', '--max', '10'], ['refs', '0x0143C898'],
                 ['dis', '0x006EFF8A'], ['dis', '0x00401090', '--before', '0x10', '--after', '0x80'],
                 ['dis', '008F4720'], ['dis', '0x0077BC10', '--after', '0x100']):
        out.append(('exeref ' + ' '.join(args), 'exeref', args + ['--exe', exe], {}, env, True))
    out.append(('exeref str ServerTerrainEntity (the game copy found)', 'exeref', ['str', 'ServerTerrainEntity'], {},
                env, True))
    return out
