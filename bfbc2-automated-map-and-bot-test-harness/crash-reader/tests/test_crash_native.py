r"""The harness's and the crash-reader tools' Rust core (round E4f, rust\bc2-harness) against the Python reference.

Quick (built data; needs the Rust core, and pefile + capstone for the exe parts):
  - x86: 20,000 seeded byte strings (prefixes, one-byte, 0F, 0F 38 / 3A, x87 opcodes) decoded by the core (strict)
    and by capstone: every answer the core gives is capstone's (size, mnemonic, op_str, immediates and memory
    operands); the others it refuses (capstone decodes them);
  - the exe test_exeref builds and seeded exes of random code (int3 padding, calls, strings, tables): the mapped image
    as pefile's, and at every code address function_start, around, call_before, on_boundary and window, and
    find_strings, cstr, refs, data_refs over a set of values, as the reference (pefile + capstone);
  - opfor.log sessions of 400 seeded logs (line breaks of every kind, bytes that are not UTF-8, digits outside ASCII)
    as sessions_reference; the reachability pair bits of seeded graphs and the cell index's columns as the Python's.
Game (--game, Project OPFOR BFBC2's BFBC2Game.exe and opfor.log, read only): the mapped image as pefile's; every
61st byte offset of the code decoded by both (--full: every offset, about 8 minutes) with no refusal; exeref's
command line and the crash reader on built reports at sampled code addresses, byte for byte as with
BFBC2_HARNESS_PYTHON=1; harness import-log's sessions of the real log as the reference's, with times.

  python test_crash_native.py [--game [--full]]
"""
import os
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
TOOL = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools', 'navkit', 'core', 'harness', 'crash-reader')
sys.path.insert(0, HERE)
import crashreaderlib as crashreader  # noqa: E402
import harnesscore  # noqa: E402

PY = sys.executable


class Checks:
    def __init__(self) -> None:
        self.failed = 0

    def check(self, ok: object, what: str, detail: str = '') -> None:
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


def norm_cs(ins):
    """A capstone instruction (or the core's Insn / tuple) as comparable data."""
    if ins is None:
        return None
    if isinstance(ins, tuple):
        ins = crashreader.Insn(ins)
    if isinstance(ins, crashreader.Insn):
        ops = [('imm', v & 0xFFFFFFFF) if k == 'imm' else ('mem', b, i, d & 0xFFFFFFFF) for k, v, b, i, d in ins.ops]
        return (ins.address, ins.size, ins.mnemonic, ins.op_str, ops)
    import capstone.x86 as x86
    ops = []
    for op in ins.operands:
        if op.type == x86.X86_OP_IMM:
            ops.append(('imm', op.imm & 0xFFFFFFFF))
        elif op.type == x86.X86_OP_MEM:
            ops.append(('mem', ins.reg_name(op.mem.base) if op.mem.base else '', ins.reg_name(op.mem.index) if op.mem.index else '',
                        op.mem.disp & 0xFFFFFFFF))
    return (ins.address, ins.size, ins.mnemonic, ins.op_str, ops)


def text_of(ins):
    """(address, size, mnemonic, op_str) of either kind of instruction."""
    return None if ins is None else (ins.address, ins.size, ins.mnemonic, ins.op_str)


# ------------------------------------------------------------------------------------------------ x86

def x86_fuzz(c: Checks, nat, n: int = 20000) -> None:
    import capstone
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    md.detail = True
    rng = random.Random(23)
    prefixes = [0x66, 0x67, 0xF0, 0xF2, 0xF3, 0x2E, 0x3E, 0x64, 0x26]
    answered = refused = same = 0
    first_bad = ''
    for _ in range(n):
        pre = bytes(rng.choice(prefixes) for _ in range(rng.choice([0, 0, 0, 1, 1, 2])))
        style = rng.random()
        if style < 0.45:
            op = bytes([rng.randrange(256)])
        elif style < 0.75:
            op = bytes([0x0F, rng.randrange(256)])
        elif style < 0.85:
            op = bytes([0x0F, rng.choice([0x38, 0x3A]), rng.randrange(256)])
        else:
            op = bytes([rng.randrange(0xD8, 0xE0)])
        code = (pre + op + bytes(rng.randrange(256) for _ in range(16)))[:16]
        addr = rng.randrange(0x401000, 0x1400000)
        ref = norm_cs(next(md.disasm(code, addr), None))
        try:
            got = nat.harness_x86_decode(code, addr, True)
        except NotImplementedError:
            refused += 1
            continue
        answered += 1
        mine = norm_cs(got)
        if mine == ref:
            same += 1
        elif not first_bad:
            first_bad = f'{code.hex(" ")}: core {mine}, capstone {ref}'
    c.check(same == answered, f'x86: {same} of {answered} answers of the core capstone\'s ({refused} of {n} seeded '
            f'byte strings refused: outside the parity proof)', first_bad)


# ------------------------------------------------------------------------------------------------ built exes

def pe32(sections, base=0x400000):
    import test_exeref
    return test_exeref.pe32(sections, base)


def random_exe(seed: int) -> tuple[bytes, list[int]]:
    """An exe of random code: functions of plausible instructions with random bytes among them, int3 padding,
    calls between them, strings and a pointer table in .rdata."""
    rng = random.Random(seed)
    base = 0x400000
    strings = [b'Server.' + bytes(rng.choice(b'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz') for _ in range(rng.randrange(3, 20)))
               for _ in range(12)] + [b'Win32', b'bad\x01string', b'x']
    rdata = bytearray(b'\0' * 4)
    s_va = []
    for s in strings:
        s_va.append(base + 0x40000 + len(rdata))
        rdata += s + b'\0'
    rdata += b'\0' * ((-len(rdata)) % 4)
    rdata += b''.join(struct.pack('<I', v) for v in s_va)
    pieces = [b'\x55\x8b\xec', b'\x8b\x45\x08', b'\x89\x4c\x24\x10', b'\x83\xc4\x04', b'\x5d\xc3', b'\x85\xc0', b'\x74\x05',
              b'\x0f\x84\x10\x00\x00\x00', b'\xd9\x44\x24\x04', b'\xde\xc1', b'\xf3\x0f\x10\x45\x08', b'\x66\x8b\x01',
              b'\xc2\x08\x00', b'\x8d\x4c\x24\x20', b'\x33\xc0', b'\xf3\xa5', b'\x0f\xb6\x41\x02', b'\x6a\xff']
    code = bytearray(b'\xcc' * 16)
    funcs = []
    for _ in range(60):
        while len(code) % 16:
            code += b'\xcc'
        funcs.append(base + 0x1000 + len(code))
        for _ in range(rng.randrange(4, 30)):
            r = rng.random()
            if r < 0.6:
                code += rng.choice(pieces)
            elif r < 0.7:
                code += b'\x68' + struct.pack('<I', rng.choice(s_va))
            elif r < 0.8 and len(funcs) > 1:
                at = base + 0x1000 + len(code)
                code += b'\xe8' + struct.pack('<i', rng.choice(funcs) - (at + 5))
            else:
                code += bytes(rng.randrange(256) for _ in range(rng.randrange(1, 6)))
        code += b'\xc3' + b'\xcc' * rng.randrange(1, 8)
    exe = pe32([('.text', 0x1000, bytes(code), 0x60000020), ('.rdata', 0x40000, bytes(rdata), 0x40000040)], base)
    values = s_va + funcs[:20] + [rng.randrange(base, base + 0x50000) for _ in range(10)]
    return exe, values


def exes_alike(c: Checks, nat, path: str, values: list[int], label: str, stride: int = 1) -> None:
    """Every method at every code address (stride), the core's Exe against the reference's."""
    import exeref
    harnesscore.use_native(True)
    a = exeref.Exe(path)
    harnesscore.use_native(False)
    b = exeref.Exe(path)
    harnesscore.use_native(True)
    h = a._nat
    if h is None:
        c.check(False, f'{label}: the core maps the exe')
        return
    import pefile
    c.check(h.data() == pefile.PE(path, fast_load=True).get_memory_mapped_image() and a.sections == b.sections,
            f'{label}: the mapped image and the sections as pefile\'s')
    syms = crashreader.read_symbols([os.path.join(TOOL, 'symbols_795745.txt')])
    lo, hi = a.code[0]
    calls = same = native = tried = 0
    bad = ''

    def same_as(what, x, y):
        nonlocal calls, same, bad
        calls += 1
        if x == y:
            same += 1
        elif not bad:
            bad = f'{what}: core {x!r}, reference {y!r}'

    def answered(fn, *args):
        nonlocal native, tried
        tried += 1
        try:
            fn(*args)
            native += 1
        except NotImplementedError:
            pass
    for va in range(lo, hi, stride):
        same_as(f'function_start {va:x}', a.function_start(va), b.function_start(va))
        sa, ia, ka = a.around(va)
        sb, ib, kb = b.around(va)
        same_as(f'around {va:x}', (sa, [text_of(x) for x in ia], ka), (sb, [text_of(x) for x in ib], kb))
        same_as(f'call_before {va:x}', text_of(a.call_before(va)), text_of(b.call_before(va)))
        same_as(f'on_boundary {va:x}', a.on_boundary(va), b.on_boundary(va))
        same_as(f'window {va:x}', exeref.window(a, va, 24, 24, syms), exeref.window(b, va, 24, 24, syms))
        same_as(f'name_of {va:x}', crashreader.name_of(va, syms, a), crashreader.name_of(va, syms, b))
        for fn, args in ((h.around, (va, 0x10)), (h.call_before, (va,)), (h.on_boundary, (va,)), (h.window, (va, 24, 24, syms))):
            answered(fn, *args)
    for text in ('server', 'Server.', 'win32', 'WIN32', 'x', 'bad', 'string', 'zz-not-there', ''):
        for exact in (False, True):
            same_as(f'find_strings {text} {exact}', a.find_strings(text, exact), b.find_strings(text, exact))
    for v in values:
        same_as(f'refs {v:x}', [(va, text_of(i), k) for va, i, k in a.refs(v)], [(va, text_of(i), k) for va, i, k in b.refs(v)])
        same_as(f'data_refs {v:x}', a.data_refs(v), b.data_refs(v))
        same_as(f'cstr {v:x}', a.cstr(v), b.cstr(v))
        answered(h.refs, v, 200)
    c.check(same == calls, f'{label}: {same} of {calls} answers as the reference\'s (the core\'s handle alone: {native} of '
            f'{tried} calls answered, the rest refused to capstone)', bad)


# ------------------------------------------------------------------------------------------------ logs, graphs

def sessions_alike(c: Checks, nat, n: int = 400) -> None:
    from testharness import ingame
    sys.path.insert(0, os.path.join(TOOL, '..', 'tests'))
    import test_harness
    rng = random.Random(5)
    lines = test_harness.LOG.splitlines()
    extra = ['plain line', 'Kynapse error: x', ' inside', 'bad \udcff', 'pid ٣٤ digits',
             '[10:00:00.0] unit counters (exit): ٣ pathfinders', '---- opfor.dll loaded into PID 9 ----',
             '[11:00:00.000] sector offsets: on - X=-99999999999999999999 Z=1', '\x85next', 'level=\'x\'']
    seps = ['\n', '\r\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\x85', ' ', ' ']
    same = native = 0
    bad = ''
    for _ in range(n):
        pool = lines + (extra if rng.random() < 0.25 else extra[:5] + extra[6:7] + extra[8:])
        text = ''.join(rng.choice(pool) + rng.choice(seps) for _ in range(rng.randrange(0, 60)))
        if rng.random() < 0.3:
            text += rng.choice(pool)
        text = text.encode('utf-8', 'surrogatepass').decode('utf-8', 'replace')
        got = ingame.sessions(text)
        ref = ingame.sessions_reference(text)
        native += nat.harness_sessions(text) is not None
        if got == ref:
            same += 1
        elif not bad:
            bad = repr(text[:200])
    c.check(same == n, f'opfor.log sessions: {same} of {n} seeded logs as sessions_reference ({native} read by the core, '
            f'the rest refused: digits outside ASCII, numbers past 64 bits)', bad)


def graphs_alike(c: Checks, nat) -> None:
    import numpy as np
    from testharness import checks, classify
    rng = np.random.default_rng(3)
    same = 0
    for _k in range(60):
        n = int(rng.integers(1, 300))
        m = int(rng.integers(0, 900))
        e = rng.integers(0, n, size=(m, 2))
        order = np.argsort(e[:, 0], kind='stable')
        start = np.searchsorted(e[order, 0], np.arange(n + 1))
        nbr = e[order, 1]
        src = [None if rng.random() < 0.1 else int(rng.integers(0, n)) for _ in range(int(rng.integers(0, 40)))]
        tgt = [None if rng.random() < 0.1 else int(rng.integers(0, n)) for _ in range(int(rng.integers(0, 40)))]
        same += checks.pair_bits(None, (start, nbr), src, tgt) == checks.pair_bits_python(None, (start, nbr), src, tgt)
    c.check(same == 60, f'reachability pair bits: {same} of 60 seeded graphs as reach_from\'s')

    class Nav:
        cell_xz: Any
        cell_y: Any
    ok = total = 0
    for _k in range(20):
        nav = Nav()
        ij = rng.integers(-400, 400, size=(int(rng.integers(1, 5000)), 2))
        nav.cell_xz = ij.astype(np.float64) * 0.8
        nav.cell_y = rng.normal(0, 30, size=len(ij))
        g = classify.Ground(nav)
        for _ in range(100):
            x, z = float(rng.choice(nav.cell_xz[:, 0]) + rng.choice([0.0, 2.0, -2.0, 0.4, 1.99])), float(rng.uniform(-330, 330))
            total += 1
            ok += np.array_equal(g.cells(x, z), g.cells_python(x, z))
    c.check(ok == total, f'ground cells: {ok} of {total} columns as the full scan\'s (in cell order)')


# ------------------------------------------------------------------------------------------------ game

def game(c: Checks, nat, full: bool, work: str) -> None:
    exe = crashreader.find_exe()
    if not exe:
        print('skip --game: BFBC2Game.exe not found')
        return
    import capstone
    import pefile
    pe = pefile.PE(exe, fast_load=True)
    h = nat.ExeHandle.open(exe, True)
    data = pe.get_memory_mapped_image()
    c.check(h.data() == data, f'BFBC2Game.exe: the mapped image as pefile\'s ({len(data):,} bytes)')
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    md.detail = True
    base = pe.OPTIONAL_HEADER.ImageBase
    stride = 1 if full else 61
    n = same = refused = 0
    bad = ''
    t = time.perf_counter()
    for lo, hi in h.code_ranges():
        for a in range(lo, hi, stride):
            o = a - base
            code = data[o:o + 16]
            ref = norm_cs(next(md.disasm(code, a), None))
            try:
                got = norm_cs(nat.harness_x86_decode(code, a, True))
            except NotImplementedError:
                refused += 1
                continue
            n += 1
            if got == ref:
                same += 1
            elif not bad:
                bad = f'{a:x}: {got} / {ref}'
    c.check(same == n and refused == 0, f'BFBC2Game.exe: {same} of {n} code offsets (every {stride}) decoded as capstone, '
            f'{refused} refused ({time.perf_counter() - t:.0f} s)', bad)
    # exeref's command line, both ways
    cmds = [['str', 'Server.UnrestrictedUnlocks', '--exact'], ['str', 'Kynapse', '--max', '5', '--dis', '3'],
            ['str', 'presenceRequestOnline', '--exact'], ['str', 'Win32', '--max', '4', '--dis'], ['str', 'crashreport'],
            ['refs', '0x8527D0'], ['refs', '0x0143375C', '--dis', '2'], ['refs', '0x6EFBC0'],
            ['dis', '0x95B7D3', '--before', '0x10', '--after', '0x10'], ['dis', '0x6EFF8A', '--before', '0x80', '--after', '0x80'],
            ['dis', '0x401000'], ['dis', '0xF85000']]
    syms = crashreader.read_symbols([os.path.join(TOOL, 'symbols_795745.txt')])
    rng = random.Random(9)
    lo, hi = h.code_ranges()[0]
    cmds += [['dis', f'{a:#x}', '--before', '0x40', '--after', '0x40'] for a in sorted(syms)[:6] + [rng.randrange(lo, hi) for _ in range(6)]]
    tn = tr = 0.0
    same = 0
    bad = ''
    for cmd in cmds:
        outs = []
        for env_py in ('', '1'):
            env = {**os.environ, 'BFBC2_HARNESS_PYTHON': env_py, 'LOCALAPPDATA': work}
            t = time.perf_counter()
            r = subprocess.run([PY, os.path.join(TOOL, 'exeref.py'), *cmd, '--exe', exe], capture_output=True, text=True,
                               encoding='utf-8', errors='replace', env=env)
            dt = time.perf_counter() - t
            if env_py:
                tr += dt
            else:
                tn += dt
            outs.append((r.returncode, r.stdout))
        if outs[0] == outs[1]:
            same += 1
        elif not bad:
            bad = f'{cmd}: {outs[0][1][-300:]!r} / {outs[1][1][-300:]!r}'
    c.check(same == len(cmds), f'exeref command line: {same} of {len(cmds)} commands byte for byte as the reference\'s '
            f'({tn:.1f} s, the reference {tr:.1f} s)', bad)
    # the crash reader on built reports at sampled code addresses
    import test_crashreader
    suite_paths.use('core')
    from bfbc2core import native as bfnative
    log = os.path.join(work, 'small.log')
    with open(log, 'w') as f:
        f.write('line\n[12:00:00.000] crash: crashreport.xml\n')
    addrs = [0x006EFF8A] + [rng.randrange(lo, hi) for _ in range(39)]
    same = 0
    bad = ''
    tn = tr = 0.0
    for k, eip in enumerate(addrs):
        regs = {'Edi': 0x1C0C28AC, 'Esi': 0, 'Ebx': rng.randrange(0, 0x20000), 'Edx': 0, 'Ecx': 0x1C0F45A4, 'Eax': 0, 'Ebp': 0x0019F000}
        stack = [eip] + [rng.randrange(lo, hi) for _ in range(12)] + [0x76FAB5AC, int.from_bytes(b'clie', 'little')]
        path = os.path.join(work, f'report{k}.xml')
        with open(path, 'w', encoding='utf-8') as f:
            f.write(test_crashreader.report(eip, regs, stack))
        # round RT3: crashreader.py's command line was retired; bfbc2.exe crashreader reads every report (the code
        # view's pieces - around, call_before, function_start - are compared with capstone per call above)
        t = time.perf_counter()
        r = subprocess.run(bfnative.cli_command('crashreader') + [path, '--exe', exe, '--log', log], capture_output=True,
                           text=True, errors='replace')
        tn += time.perf_counter() - t
        if r.returncode == 0 and f'Crash at 0x{eip:08X}' in r.stdout and 'Call stack' in r.stdout:
            same += 1
        elif not bad:
            bad = f'{eip:x}: exit {r.returncode}: {(r.stderr or r.stdout)[-200:]!r}'
    c.check(same == len(addrs), f'bfbc2 crashreader: {same} of {len(addrs)} built reports at sampled code addresses '
            f'read with their code view ({tn:.1f} s; each loads the exe)', bad)
    # import-log's sessions of the real log
    logp = os.path.join(os.path.dirname(exe), 'logs', 'opfor.log')
    if os.path.isfile(logp):
        from testharness import ingame
        t = time.perf_counter()
        got = ingame.sessions_of_file(logp)
        t1 = time.perf_counter()
        text = open(logp, encoding='utf-8', errors='replace').read()
        ref = ingame.sessions_python(text)
        t2 = time.perf_counter()
        c.check(got == ref, f'opfor.log ({os.path.getsize(logp) / 1e6:.0f} MB): {len(got)} sessions read by the core, field '
                f'for field as the Python\'s ({t1 - t:.2f} s with the read; the Python {t2 - t1:.2f} s)')


def main(argv: list[str]) -> int:
    c = Checks()
    nat = harnesscore.native()
    if nat is None:
        print('the Rust core is not built (python rust\\build_native.py) - nothing to compare')
        return 0
    work = tempfile.mkdtemp(prefix='crash_native_')
    try:
        if '--game' in argv:
            game(c, nat, '--full' in argv, work)
        else:
            sessions_alike(c, nat)
            graphs_alike(c, nat)
            try:
                import capstone  # noqa: F401
                import pefile  # noqa: F401
            except ImportError:
                print('pefile / capstone not installed - the x86 and exe parts not compared')
            else:
                x86_fuzz(c, nat)
                import test_exeref
                data, at = test_exeref.build()
                p = os.path.join(work, 'built.exe')
                open(p, 'wb').write(data)
                exes_alike(c, nat, p, [at['s_va'], at['f1'], at['f2'], at['table_va'], at['push'], 0x12345678], 'test_exeref\'s exe')
                for seed in (1, 2):
                    exe, values = random_exe(seed)
                    p = os.path.join(work, f'random{seed}.exe')
                    open(p, 'wb').write(exe)
                    exes_alike(c, nat, p, values, f'random-code exe {seed}', stride=5)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    print('all passed' if not c.failed else f'{c.failed} check(s) failed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
