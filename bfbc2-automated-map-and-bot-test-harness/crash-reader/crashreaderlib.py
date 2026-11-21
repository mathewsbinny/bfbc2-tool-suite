r"""Battlefield: Bad Company 2 crash-report reader's library: the report, the symbols, the exe's code (Image), the
log lines before a crash. `bfbc2 crashreader` is the command line (round RT3 retired crashreader.py: bfbc2.exe
answers it alone, bfbc2core.native.RETIRED_CLI; its usage text is rust\bc2-cli\src\crash\crashreader_usage.txt).
exeref.py (a live command line) and the tests import this module: find_reports, find_exe, read_report, read_symbols
(its warning's line is emulated by bfbc2.exe exeref: rust\bc2-cli\src\crash\common.rs includes this file), Insn,
Image, null_operands, name_of, log_before_crash and its references.

The game's own crash reporter writes the report; this module only reads it and the exe. Nothing is sent anywhere.
"""
import glob
import os
import re
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
REGISTERS = ['Eax', 'Ebx', 'Ecx', 'Edx', 'Esi', 'Edi', 'Ebp', 'Esp', 'Eip']


# --------------------------------------------------------------------------------------------------- inputs

def find_reports():
    home = os.path.expanduser('~')
    docs = [os.path.join(home, 'Documents'), os.path.join(home, 'OneDrive', 'Documents')]
    docs += glob.glob(os.path.join(home, 'OneDrive*', 'Documents'))
    found = []
    for d in docs:
        for sub in ('Project OPFOR\\BFBC2', 'BFBC2'):
            p = os.path.join(d, sub, 'crashreport.xml')
            if os.path.isfile(p):
                found.append(p)
    return sorted(set(found), key=os.path.getmtime, reverse=True)


def find_exe():
    """OPFOR_ROOT's game\\BFBC2Game.exe, else that of a "project-opfor-bfbc2" folder next to one of the folders this
    tool sits in (bfbc2core.paths.find_game_exe, walking up from the harness folder), or None."""
    suite = os.path.dirname(os.path.dirname(HERE))       # core only when needed: importing this module adds nothing
    if suite not in sys.path:
        sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import paths
    return paths.find_game_exe(start=os.path.dirname(HERE))


def read_report(path):
    root = ET.parse(path).getroot()
    text = lambda tag: (root.findtext(tag) or '').strip()
    stack = [int(v, 16) for v in text('stack').split() if re.fullmatch(r'0x[0-9A-Fa-f]+', v)]
    regs = {}
    for name, value in re.findall(r'(\w+)\s+([0-9A-Fa-f]{8})', text('contextdata')):
        if name.capitalize() in REGISTERS:
            regs[name.capitalize()] = int(value, 16)
    category = text('categoryid')
    return {
        'type': text('type'), 'sku': text('sku'), 'build': text('buildsignature'), 'time': text('createtime'),
        'category': int(category, 16) if re.fullmatch(r'0x[0-9A-Fa-f]+', category) else None,
        'stack': stack, 'registers': regs,
    }


def read_symbols(paths):
    syms = {}
    for p in paths:
        if not os.path.isfile(p):
            continue
        for line in open(p, encoding='utf-8'):
            line = line.split('#', 1)[0].rstrip()
            m = re.match(r'\s*(?:0x)?([0-9A-Fa-f]{6,8})\s+([^|]+?)\s*(?:\|\s*(.*))?$', line)
            if m:
                syms[int(m.group(1), 16)] = (m.group(2).strip(), (m.group(3) or '').strip())
    return syms


# --------------------------------------------------------------------------------------------------- the exe

class Insn:
    """An instruction from the Rust core (capstone's text and operands, rust\\bc2-harness's x86): address, size,
    mnemonic, op_str, and operands as ('imm', value, '', '', 0) / ('mem', 0, base, index, disp) - what the tools read
    of a capstone instruction."""
    __slots__ = ('address', 'size', 'mnemonic', 'op_str', 'ops')

    def __init__(self, t):
        self.address, self.size, self.mnemonic, self.op_str, self.ops = t

    def __repr__(self):
        return f'<Insn 0x{self.address:x}: {self.mnemonic} {self.op_str}>'


def _native():
    import harnesscore
    return harnesscore.native()


def _capstone_installed():
    import importlib.util
    return importlib.util.find_spec('capstone') is not None and importlib.util.find_spec('pefile') is not None


class Image:
    """The exe's code, for disassembly: the Rust core's exe handle (round E4f, rust\\bc2-harness: pefile's mapping,
    capstone's text, proved on every byte offset of BFBC2Game.exe's code) when it is built and maps the file, else
    pefile + capstone (the reference, also where the core refuses: an instruction outside its proof)."""

    def __init__(self, path):
        self.path = path
        self._nat = None
        nat = _native()
        if nat is not None:
            try:
                # strict where capstone can take what the core refuses; without capstone the core answers alone
                self._nat = nat.ExeHandle.open(path, _capstone_installed())
            except NotImplementedError:
                self._nat = None
        if self._nat is None:
            self._load_reference()
        else:
            self.base, self.end = self._nat.base, self._nat.end
            self.code = [tuple(r) for r in self._nat.code_ranges()]

    def _load_reference(self):
        """pefile + capstone (ImportError without them)."""
        if getattr(self, 'md', None) is not None:
            return
        import capstone
        import pefile
        pe = pefile.PE(self.path, fast_load=True)
        self._pe = pe
        self.base = pe.OPTIONAL_HEADER.ImageBase
        self.data = pe.get_memory_mapped_image()
        self.end = self.base + len(self.data)
        self.code = [(self.base + s.VirtualAddress, self.base + s.VirtualAddress + s.Misc_VirtualSize)
                     for s in pe.sections if s.Characteristics & 0x20000000]
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
        self.md.detail = True

    def _native_call(self, name, *args):
        """(True, the Rust core's answer) or (False, None) where it refuses (or is not in use): the reference then."""
        nat = getattr(self, '_nat', None)
        if nat is None:
            return False, None
        try:
            return True, getattr(nat, name)(*args)
        except NotImplementedError:
            self._load_reference()
            return False, None

    def is_code(self, a):
        return any(lo <= a < hi for lo, hi in self.code)

    def bytes_at(self, a, n):
        nat = getattr(self, '_nat', None)
        if nat is not None and getattr(self, 'md', None) is None:
            self.data = nat.data()
        o = a - self.base
        return self.data[o:o + n]

    def function_start(self, a, limit=0x4000):
        """Heuristic: MSVC pads functions with int3 (0xCC); the function starts after the last pad before a."""
        ok, got = self._native_call('function_start', a, limit)
        return got if ok else self.function_start_reference(a, limit)

    def function_start_reference(self, a, limit=0x4000):
        self._load_reference()
        for p in range(a, max(a - limit, self.base), -1):
            o = p - self.base
            if self.data[o - 1] == 0xCC and self.data[o] != 0xCC and (p % 16 == 0 or self.data[o - 2] == 0xCC):
                return p
        return None

    def disasm(self, start, stop):
        ok, got = self._native_call('disasm', start, stop)
        return [Insn(t) for t in got] if ok else self.disasm_reference(start, stop)

    def disasm_reference(self, start, stop):
        self._load_reference()
        return list(self.md.disasm(self.bytes_at(start, stop - start + 16), start))

    def around(self, eip, after=0x10):
        """(function start, instructions up to eip + after, index of the one at eip or None): decoded from the
        function start when it is near (instructions on real boundaries, as exeref.window does), else from the
        nearest point up to 0x60 bytes before eip whose decode lands on eip."""
        ok, got = self._native_call('around', eip, after)
        if ok:
            start, ins, at = got
            return start, [Insn(t) for t in ins], at
        return self.around_reference(eip, after)

    def around_reference(self, eip, after=0x10):
        self._load_reference()
        start = self.function_start_reference(eip)
        tries = ([start] if start is not None and eip - start <= 0x2000 else []) + list(range(max(eip - 0x60, self.base), eip + 1))
        ins: list = []
        for s in tries:
            ins = [x for x in self.disasm_reference(s, eip + after) if x.address <= eip + after]
            if any(x.address == eip for x in ins):
                break
        at = next((k for k, x in enumerate(ins) if x.address == eip), None)
        return (start if start is not None else eip - 0x40), ins, at

    def call_before(self, ret):
        """The call instruction that ends exactly at a return address, if any."""
        ok, got = self._native_call('call_before', ret)
        if ok:
            return Insn(got) if got is not None else None
        return self.call_before_reference(ret)

    def call_before_reference(self, ret):
        self._load_reference()
        for length in (5, 6, 2, 3, 7, 4):
            for ins in self.md.disasm(self.bytes_at(ret - length, length), ret - length):
                if ins.address + ins.size == ret and ins.mnemonic == 'call':
                    return ins
                break
        return None


def null_operands(ins, regs):
    """Registers a memory operand of the instruction reads through that hold a null (or tiny) pointer."""
    if isinstance(ins, Insn):
        bad = []
        for kind, _imm, base, index, disp in ins.ops:
            if kind != 'mem':
                continue
            for r in (base, index):
                if not r:
                    continue
                name = r.capitalize()
                v = regs.get(name)
                if v is not None and v < 0x10000:
                    bad.append((name, v, disp))
        return bad
    import capstone.x86 as x86
    bad = []
    for op in ins.operands:
        if op.type != x86.X86_OP_MEM:
            continue
        for r in (op.mem.base, op.mem.index):
            if r == 0:
                continue
            name = ins.reg_name(r).capitalize()
            v = regs.get(name)
            if v is not None and v < 0x10000:
                bad.append((name, v, op.mem.disp))
    return bad


# --------------------------------------------------------------------------------------------------- report

def name_of(a, syms, image):
    if a in syms:
        return syms[a][0], syms[a][1]
    start = image.function_start(a) if image and image.is_code(a) else None
    if start is not None and start in syms:
        return f'{syms[start][0]} +0x{a - start:X}', syms[start][1]
    near = max((s for s in syms if s <= a and a - s < 0x800), default=None)
    if near is not None and start is not None and near >= start:
        return f'{syms[near][0]} +0x{a - near:X}', syms[near][1]
    if start is not None:
        return f'FUN_{start:08x} +0x{a - start:X}', ''
    return '', ''


MARK = 'crashreport.xml'
BEFORE = 400                                # log_before_crash looks at the lines this far before the crash line


def _near(when):
    """The test of a crash line against `when` (None: every line passes)."""
    if when is None:
        return lambda line: True
    import time
    t = time.localtime(when)
    want = t.tm_hour * 3600 + t.tm_min * 60 + t.tm_sec

    def near(line):
        m = re.match(r'\[(\d\d):(\d\d):(\d\d)', line)
        if not m:
            return True                 # no time on the line (an older log): nothing to tell crashes apart by
        d = abs(int(m.group(1)) * 3600 + int(m.group(2)) * 60 + int(m.group(3)) - want)
        return min(d, 86400 - d) <= 120
    return near


def log_before_crash(path, lines=18, when=None):
    """The log lines that led up to the crash that wrote the report: the crashreport.xml line whose time of day is
    within 2 minutes of `when` (the report's modification time, seconds since the epoch), the latest such; without
    `when`, the last one. [] when no crash in the log matches.

    Only the bytes around the crash lines are decoded (round E3m: the game's opfor.log grows to hundreds of MB; reading
    all of it as text took 7 s): from the latest occurrence of the mark back, a window that starts just after a
    newline at least BEFORE + 1 newlines earlier and ends at the newline after the mark. A newline byte is always a
    line end and never inside a UTF-8 sequence, and the mark is ASCII, so the window's lines are the whole text's
    (log_before_crash_reference reads the whole text; tests/test_crashreader.py compares them)."""
    if REFERENCE:
        return log_before_crash_reference(path, lines, when)
    nat = _native()
    if nat is not None:
        want = None
        if when is not None:
            import time
            t = time.localtime(when)
            want = t.tm_hour * 3600 + t.tm_min * 60 + t.tm_sec
        got = nat.crash_log_before(path, lines, want)       # the Rust core (round E4f); None: it refuses
        if got is not None:
            return got
    return log_before_crash_python(path, lines, when)


def log_before_crash_python(path, lines=18, when=None):
    """log_before_crash in Python (round E3m's windows)."""
    try:
        with open(path, 'rb') as f:
            data = f.read()
    except OSError:
        return []
    near = _near(when)
    mark = MARK.encode()
    q = data.rfind(mark)
    while q >= 0:
        s = q
        for _ in range(BEFORE + 2):             # back past BEFORE + 1 whole lines (and the mark line's own start)
            s = data.rfind(b'\n', 0, s)
            if s < 0:
                break
        s = 0 if s < 0 else s + 1
        e = data.find(b'\n', q)
        e = len(data) if e < 0 else e + 1
        window = data[s:e].decode('utf-8', errors='replace')
        at = len(data[s:q].decode('utf-8', errors='replace'))      # the mark's first character in the window
        pos = end = 0
        for line in window.splitlines(keepends=True):     # the line that holds it
            if pos + len(line) > at:
                break
            pos += len(line)
            end += 1
        text = window.splitlines()
        if near(text[end]):
            picked = [l for l in text[max(0, end - BEFORE):end] if 'Kynapse error' not in l and 'per frame' not in l]
            return picked[-lines:] + [text[end]]
        q = data.rfind(mark, 0, q)
    return []


# log_before_crash's reference (BFBC2_CRASHREADER_REFERENCE=1 or REFERENCE = True): the whole log read as text
REFERENCE = os.environ.get('BFBC2_CRASHREADER_REFERENCE', '') not in ('', '0')


def log_before_crash_reference(path, lines=18, when=None):
    """log_before_crash as it was before round E3m: the whole log decoded and split into lines."""
    try:
        text = open(path, encoding='utf-8', errors='replace').read().splitlines()
    except OSError:
        return []
    marks = [i for i, l in enumerate(text) if 'crashreport.xml' in l]
    if when is not None:
        near = _near(when)
        marks = [i for i in marks if near(text[i])]
    if not marks:
        return []
    end = marks[-1]
    # skip the per-frame spam probes write, keep what led up to the crash
    picked = [l for l in text[max(0, end - 400):end] if 'Kynapse error' not in l and 'per frame' not in l]
    return picked[-lines:] + [text[end]]
