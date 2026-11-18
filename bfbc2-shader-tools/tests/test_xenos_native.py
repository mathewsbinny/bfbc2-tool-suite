r"""The Xbox 360 shader translator in the Rust core (rust/bc2-shader through the bc2_native module: round E3f).

The decoder: xenos.py's parse_shader in Rust against parse_shader_python, the format reference that stays: the same
trees (every attribute, its value and type, the attribute order, the CFs' instruction lists holding the same objects
as instructions) and the same exceptions.

The translation: xenos_hlsl.translate's Python (translate_python) was the reference until 2026-11-18, when its Rust
port had passed the full test tier (173/173 on 2026-11-17) and it was retired (rust/BENCHMARKS.md, "Retired
Python"). Its results were recorded first, from the Python code itself (each translation's text SHA-1, each
TranslateError's message): xenos_golden.json (the built programs) and xenos_golden_game.json (the game's programs)
beside this file. They are the oracle now. Every tree is translated in Rust: a fresh one decoded there, one whose
microcode was read (materialized) and the Python decoder's own (both handed over attribute by attribute), each
against the recording.

  python test_xenos_native.py          quick, no game data: programs built here (pixel and vertex shaders, constant
                                       tables, literals, every ALU and fetch opcode, the control flow the translator
                                       takes and refuses) translated under seeded random interfaces; seeded damaged
                                       blobs (decoded alike or refused with the same exception); Python's float repr;
                                       the dispatch (lazy microcode, copies, pickles, use_native, parse_many,
                                       translate_many, trees built by hand, declines)
  python test_xenos_native.py --game   + every program of Battlefield 1943's Xbox shader databases (the levels', the
                                       UI's, the post-process containers') and of BFBC2's Xbox level databases (OPFOR's
                                       work\xbox), each decoded both ways and translated under three interfaces
                                       (the plain one and two seeded random ones); counts and timings printed
  python test_xenos_native.py --record [--game]
                                       write the recordings from the current translator: only after a deliberate
                                       change to the translation, proven on the game (test_xdiff.py --game-all) -
                                       the recordings are the only oracle

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import copy
import hashlib
import json
import math
import os
import pickle
import random
import struct
import sys
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core', 'console-convert', 'bf1943-import')
import xenos  # noqa: E402
import xenos_hlsl as X  # noqa: E402

xenos.use_native(True)          # the decoders are compared, whatever BFBC2_PURE_PYTHON says (pool workers too)

GOLDEN = os.path.join(HERE, 'xenos_golden.json')
GOLDEN_GAME = os.path.join(HERE, 'xenos_golden_game.json')
RECORD = '--record' in sys.argv
fails: list[str] = []


def check(cond: bool, what: str) -> None:
    print(('ok   ' if cond else 'FAIL ') + what, flush=True)
    if not cond:
        fails.append(what)


# ------------------------------------------------------------------------------------------------ comparing trees

def diff(a: Any, b: Any, path: str = 'sh') -> str | None:
    """Where two trees differ (types exactly, attribute and key order, floats by repr), else None."""
    for x in (a, b):
        if type(x) is xenos._NativeShader:
            x._materialize()
    if isinstance(a, xenos.Obj) and isinstance(b, xenos.Obj):
        da, db = a.__dict__, b.__dict__
        if list(da) != list(db):
            return f'{path}: attributes {list(da)} / {list(db)}'
        for k in da:
            d = diff(da[k], db[k], f'{path}.{k}')
            if d:
                return d
        return None
    if type(a) is not type(b):
        return f'{path}: {type(a).__name__} {a!r:.60} / {type(b).__name__} {b!r:.60}'
    if isinstance(a, dict):
        if list(a) != list(b):
            return f'{path}: keys {list(a)} / {list(b)}'
        for k in a:
            d = diff(a[k], b[k], f'{path}[{k!r}]')
            if d:
                return d
        return None
    if isinstance(a, (list, tuple)):
        if len(a) != len(b):
            return f'{path}: {len(a)} / {len(b)} items'
        for j, (x, y) in enumerate(zip(a, b)):
            d = diff(x, y, f'{path}[{j}]')
            if d:
                return d
        return None
    if isinstance(a, float):
        return None if repr(a) == repr(b) else f'{path}: {a!r} / {b!r}'
    return None if a == b else f'{path}: {a!r:.80} / {b!r:.80}'


def shares(sh: Any) -> bool:
    """The CFs' instruction lists hold the very objects of sh.instructions (as parse_shader_python's)."""
    ids = {id(i) for i in sh.instructions}
    return all(id(i) in ids for cf in sh.cf for i in getattr(cf, 'instructions', []))


def outcome(fn: Any, *args: Any) -> tuple[str, Any]:
    try:
        return 'ok', fn(*args)
    except Exception as e:  # noqa: BLE001 - every exception is compared
        return type(e).__name__, str(e)


def compare_parse(blob: bytes, is_pixel: bool | None) -> tuple[str | None, str, Any, Any]:
    """(None or how the decoders differ, the outcome's kind, the Python tree, the Rust tree)."""
    py = outcome(xenos.parse_shader_python, blob, is_pixel)
    rs = outcome(xenos.parse_shader, blob, is_pixel)
    if py[0] != rs[0]:
        return f'python {py[0]} {py[1]!s:.80}, rust {rs[0]} {rs[1]!s:.80}', rs[0], None, None
    if py[0] != 'ok':
        return (None if py[1] == rs[1] else f'messages differ: {py[1]!r} / {rs[1]!r}'), py[0], None, None
    if type(rs[1]) is not xenos._NativeShader:
        return 'not decoded in Rust', 'ok', None, None
    rs[1]._materialize()
    d = diff(py[1], rs[1])
    if not d and not shares(rs[1]):
        d = 'CF instruction lists do not share the instruction objects'
    return d, 'ok', py[1], rs[1]


# ------------------------------------------------------------------------------------------------ translations

def digest(text: str) -> str:
    """The first 16 hex digits of the text's SHA-1 (64 bits: plenty to tell a changed translation, a small file)."""
    return hashlib.sha1(text.encode()).hexdigest()[:16]


def translation(sh: Any, iface: Any) -> str:
    """'H:' and the HLSL text's digest, 'E:' and a TranslateError's message, or 'X:' and another exception's type and
    message (the recordings' form)."""
    try:
        return 'H:' + digest(X.translate(sh, iface))
    except X.TranslateError as e:
        return 'E:' + str(e)
    except Exception as e:  # noqa: BLE001 - recorded
        return f'X:{type(e).__name__}: {e}'


def fresh_trees(blob: bytes, is_pixel: bool | None, sh_py: Any) -> list[tuple[str, Any]]:
    """The trees a caller can hand the translator, each made anew: the Rust decoder's untouched (its program kept in
    Rust), the same with its microcode read (handed over attribute by attribute) and the Python decoder's."""
    mat = xenos.parse_shader(blob, is_pixel)
    mat._materialize()
    return [('fresh', xenos.parse_shader(blob, is_pixel)), ('materialized', mat), ('python tree', sh_py)]


def against(label: str, want: str | None, got: list[tuple[str, str]], bad: Counter[str],
            example: dict[str, str]) -> None:
    """Each tree's translation against the recording (want None: not recorded)."""
    for how, g in got:
        if want is None:
            key = 'not recorded'
        elif g != want:
            key = f'{how}: {"text" if g[:2] == want[:2] == "H:" else g[:2] + " / recorded " + want[:2]}'
        else:
            continue
        bad[key] += 1
        example.setdefault(key, f'{label} ({how}): {g[:120]!r} / recorded {want and want[:120]!r}')


# ------------------------------------------------------------------------------------------------ recordings

def load(path: str) -> dict[str, Any]:
    try:
        with open(path, encoding='utf-8') as f:
            return dict(json.load(f))
    except FileNotFoundError:
        return {}


def write(path: str, golden: dict[str, Any]) -> None:
    """A recording, one case per line (a change shows as the lines of the cases it changed)."""
    lines = ['{']
    keys = sorted(golden)
    for i, k in enumerate(keys):
        v, end = golden[k], (',' if i < len(keys) - 1 else '')
        if isinstance(v, list):
            rows = [json.dumps(r) for r in v]
            lines += [f'{json.dumps(k)}: [', *[r + ',' for r in rows[:-1]], *rows[-1:], ']' + end]
        elif isinstance(v, dict):
            rows = [f'{json.dumps(n)}: {json.dumps(v[n])}' for n in sorted(v)]
            lines += [f'{json.dumps(k)}: {{', *[r + ',' for r in rows[:-1]], *rows[-1:], '}' + end]
        else:
            lines.append(f'{json.dumps(k)}: {json.dumps(v)}{end}')
    lines.append('}')
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')


# ------------------------------------------------------------------------------------------------ interfaces

ENGINE_NAMED = ['heightmapScaleAndOffsetXZ', 'dynamicMaskAtlasParams', 'terrainScale']
SPECIAL = [0.0, -0.0, 1.0, -1.0, 0.5, 1e16, 1e15, 1.5e-5, 1e-4, 123456789.125, float('nan'), float('inf'),
           -float('inf'), 5e-324, 3.4028234663852886e+38, 1e-7, 2, 0, -3, True]
FIXES = ['xxxw', 'xsrgb({})', '{} * (1.0 - saturate(wheelDecalMask))', 'xsrgb(({{}}).xxxw)', 'xxxw', '', None]


def f32_bits(rng: random.Random) -> int:
    """A literal's bits: an ordinary float, a special one or any 32 bits."""
    r = rng.random()
    if r < 0.6:
        return struct.unpack('>I', struct.pack('>f', rng.choice([0.0, -0.0, 1.0, 0.5, 2.0, -1.0, 1e-5, 1e16, 3.0,
                                                                  rng.uniform(-50, 50)])))[0]
    if r < 0.7:
        return rng.choice([0x7f800000, 0xff800000, 0x7fc00000, 1, 0x80000001, 0x7f7fffff])
    return rng.getrandbits(32)


def number(rng: random.Random) -> Any:
    r = rng.random()
    if r < 0.3:
        return rng.choice(SPECIAL)
    if r < 0.6:
        return struct.unpack('<f', struct.pack('<f', rng.uniform(-100, 100)))[0]
    if r < 0.8:
        return rng.uniform(-1e6, 1e6) * 10 ** rng.randint(-12, 12)
    return struct.unpack('<d', struct.pack('<Q', rng.getrandbits(64)))[0]


def interface(sh: Any, rng: random.Random | None) -> X.Interface:
    """The plain interface (rng None: the shader's own interpolators and inputs) or a seeded random one."""
    inter = sorted(it.usage_index for it in sh.header.interpolators)
    vs_in = {el.usage_index: 'float4' for el in getattr(sh.header, 'vertex_elements', [])}
    if rng is None:
        return X.Interface(inter, vs_inputs=vs_in)
    if rng.random() < 0.1 and inter:
        inter.pop(rng.randrange(len(inter)))
    if rng.random() < 0.5:
        inter = sorted(set(inter) | {rng.randrange(16)})
    vs_in = {k: rng.choice(['float4', 'int4', 'uint4', 'float2']) for k in vs_in}
    consts = sh.ctab.constants if sh.ctab else []
    floats = [c for c in consts if c.regset == 'float4']
    samplers = [c.reg for c in consts if c.regset == 'sampler']
    material: dict[str, Any] = {}
    for c in floats:
        if rng.random() < 0.6:
            k = rng.randrange(4)
            if k == 0:
                material[c.name] = tuple(number(rng) for _ in range(4))
            elif k == 1:
                material[c.name] = [tuple(number(rng) for _ in range(rng.choice([4, 4, 3]))) for _ in range(rng.randint(1, c.count + 1))]
            elif k == 2:
                material[c.name] = [[number(rng) for _ in range(4)]]
            else:
                material[c.name] = None
    regs: dict[int, Any] = {}
    for c in floats:
        for j in range(c.count):
            if rng.random() < 0.3:
                regs[c.reg + j] = tuple(number(rng) for _ in range(4))
    named: dict[int, str] = {}
    for c in floats:
        if rng.random() < 0.1:
            named[c.reg + rng.randrange(max(c.count, 1))] = rng.choice(ENGINE_NAMED * 6 + ['viewMatrix', '1bad'])
    fix: dict[int, Any] = {s: rng.choice(FIXES) for s in samplers if rng.random() < 0.5}
    sregs = {s: rng.choice([0, 1, 2, 3, 7, 15] + [16] * (rng.random() < 0.05)) for s in samplers if rng.random() < 0.5}
    fregs = {rng.randrange(32): rng.choice(samplers or [0]) for _ in range(rng.randint(0, 2))}
    iface = X.Interface(inter, vs_inputs=vs_in, material=material, sampler_fix=fix,
                        colour_scale=rng.choice([1.0, 2.0, 0.5, 4]), instanced=rng.random() < 0.3,
                        bones_as_world=rng.random() < 0.3, material_regs=regs, pc_shadow_atlas=rng.random() < 0.5,
                        sampler_registers=sregs, named_vs=named if not sh.is_pixel else {},
                        named_ps=named if sh.is_pixel else {}, fetch_registers=fregs)
    iface.trace_nan = rng.random() < 0.2
    if rng.random() < 0.2:
        iface.probe = ['constants[0].x', 'constants[1].y']
    return iface


# ------------------------------------------------------------------------------------------------ built programs

def build(rng: random.Random, pixel: bool) -> bytes:
    """A Xenos shader blob: container, constant table, literal definitions, header and seeded microcode."""
    be = lambda *v: struct.pack(f'>{len(v)}I', *v)                                  # noqa: E731
    # the constant table: engine and material float4s, a 'constants' array, samplers (2D, cube, the shadow map)
    consts = [('worldViewProjMatrix', 2, 0, 4, 2), ('cameraPos', 2, 4, 1, 1), ('colorScale', 2, 5, 1, 1),
              ('constants', 2, 6, 4, 1), ('tint', 2, 10, 2, 1), ('boneMatrices', 2, 21, 180, rng.choice([2, 3, 1]))]
    smp = [('sampler_Diffuse', 3, 0, 1, 12), ('sampler_Env', 3, 1, 1, 14),
           ('outdoorLightShadowmapTexture', 3, 2, 1, 12), ('sampler_Vol', 3, 3, 1, rng.choice([13, 11, 12, 12, 12]))]
    consts += smp
    if rng.random() < 0.2:
        consts.append(('weird', 7, 30, 1, 1))
    strings = b''
    str_off: dict[str, int] = {}

    def s(t: str) -> int:
        nonlocal strings
        if t not in str_off:
            str_off[t] = 0x1c + 20 * len(consts) + 16 * len(consts) + len(strings)
            strings += t.encode('latin-1') + b'\0'
        return str_off[t]
    for c in consts:
        s(c[0])
    s('ps_3_0' if pixel else 'vs_3_0')
    s('test creator')
    ctab = be(0x1c, s('test creator'), 0xfffe0300, len(consts), 0x1c, 0, s('ps_3_0' if pixel else 'vs_3_0'))
    types = b''
    for k, (name, rs, reg, cnt, tkind) in enumerate(consts):
        toff = 0x1c + 20 * len(consts) + 16 * k
        cls = 2 if tkind == 2 else (3 if name == 'boneMatrices' and rng.random() < 0.5 else 1)
        ctab += struct.pack('>IHHHHII', s(name), rs, reg, cnt, 0, toff, 0)
        types += struct.pack('>HHHHHHI', cls if rs == 2 else 4, 3 if rs == 2 else tkind, 1, 4, cnt, 0, 0)
    ctab += types + strings
    ctab += b'\0' * (-len(ctab) % 4)
    # literals: float defs into the physical part, maybe int defs
    nlit = rng.randint(0, 3)
    lit_regs = rng.sample(range(32, 64), nlit)
    lits = b''.join(struct.pack('>4I', *(f32_bits(rng) for _ in range(4))) for _ in range(nlit))
    words = []
    for k, r in enumerate(lit_regs):
        words += [((r + (256 if pixel else 0)) << 16) | 4, 16 * k]
    words.append(0)
    if rng.random() < 0.3:
        words += [((8992 + 4 * rng.randrange(4)) << 16) | 1, rng.getrandbits(32), 0]
    else:
        words.append(0)
    if rng.random() < 0.2:
        words += [rng.getrandbits(32), 0]
    deftab = be(0, 0, 0, 0, 4 * len(words)) + be(*words)
    # microcode: CFs then instruction slots
    nins = rng.randint(1, 14)
    fetch_slots = []
    cfs: list[tuple[Any, ...]] = []
    pos = 0
    while pos < nins:
        n = min(rng.randint(1, 6), nins - pos)
        seq = 0
        for j in range(n):
            f = rng.random() < 0.3
            seq |= (f | (rng.random() < 0.2) << 1) << (2 * j)
            if f:
                fetch_slots.append(j + pos)
        op = rng.choice([1, 1, 1, 1, 5] + [2, 6] * (rng.random() < 0.03))
        cfs.append(('exec', op, pos, n, seq))
        pos += n
        if rng.random() < 0.15:
            cfs.append(('jmp',))
        if rng.random() < 0.1:
            cfs.append(('alloc',))
        if rng.random() < 0.03:
            cfs.append(('other', rng.choice([0, 3, 7, 8, 9, 10, 13, 15])))
    if rng.random() < 0.5:
        cfs.append(('exec', rng.choice([2, 2, 6]), 0, 0, 0))
    ncf_slots = (len(cfs) + 1) // 2
    enc = []
    for k, c in enumerate(cfs):
        if c[0] == 'exec':
            _, op, addr, n, seq = c
            w0 = ((addr + ncf_slots) & 0xfff) | (n << 12) | (seq << 16)
            w1 = (op << 12) | (rng.getrandbits(1) << 10) | (rng.getrandbits(1) << 9) | rng.getrandbits(2)
        elif c[0] == 'jmp':
            target = rng.randint(k + 1, len(cfs) + 1)
            w0 = target | (rng.getrandbits(1) << 13) | (rng.getrandbits(1) << 14)
            w1 = (11 << 12) | (rng.getrandbits(1) << 10)
        elif c[0] == 'alloc':
            w0, w1 = rng.getrandbits(3), (12 << 12) | (rng.getrandbits(3) << 8)
        else:
            w0, w1 = rng.getrandbits(32), (c[1] << 12) | rng.getrandbits(12)
            if c[1] in (3, 13):
                w0 = (w0 & ~0xfff) | ncf_slots
        enc.append((w0 & 0xffffffff, w1 & 0xffff))
    if len(enc) % 2:
        enc.append((0, 0))
    code = []
    for a, b in zip(enc[0::2], enc[1::2]):
        code += [a[0], (a[1] & 0xffff) | ((b[0] & 0xffff) << 16), (b[0] >> 16) | (b[1] << 16)]
    for j in range(nins):
        if j in fetch_slots:
            if not pixel and rng.random() < 0.6:
                w0 = (rng.randrange(32) << 5) | (rng.randrange(32) << 12) | (1 << 19) | (rng.randrange(32) << 20) | \
                    (rng.randrange(4) << 25) | (rng.getrandbits(5) << 27)
                w1 = rng.getrandbits(12) | (rng.getrandbits(4) << 12) | (rng.choice([6, 7, 25, 26, 32, 35, 38, 57, 3]) << 16) | \
                    (rng.getrandbits(6) << 24) | ((rng.random() < 0.3) << 30) | ((rng.random() < 0.2) << 31)
                w2 = rng.getrandbits(32)
            else:
                op = rng.choice([1] * 12 + [19, 19, 24, 25, 16, 18])
                w0 = op | (rng.randrange(8) << 5) | (rng.randrange(8) << 12) | (rng.getrandbits(1) << 19) | \
                    (rng.choice([0, 0, 0, 1, 1, 1, 2, 2, 3] + [5] * (rng.random() < 0.2)) << 20) | ((rng.random() < 0.1) << 25) | (rng.getrandbits(6) << 26)
                w1 = rng.getrandbits(12) | (rng.getrandbits(18) << 12)
                if rng.random() < 0.8:
                    w1 &= ~(1 << 29)
                w2 = rng.getrandbits(32) & ~(1 if rng.random() < 0.9 else 0) & ~((1 << 31) if rng.random() < 0.5 else 0)
                if rng.random() < 0.5:
                    w2 &= ~(0x7fff << 16)          # no offsets
                if rng.random() < 0.3:
                    w1 = (w1 & ~(0xf << 12)) | (0 << 12)          # point filters
                if (w0 >> 11) & 1 and rng.random() < 0.8:
                    w0 &= ~(1 << 11)
                if (w0 >> 18) & 1 and rng.random() < 0.8:
                    w0 &= ~(1 << 18)
        else:
            w0 = rng.getrandbits(32)
            w1 = rng.getrandbits(32)
            w2 = rng.getrandbits(32)
            if rng.random() < 0.97:     # translatable: known ops, no relative addressing
                w2 = (w2 & ~(0x1f << 24)) | (rng.randrange(30) << 24)
                w0 = (w0 & ~(0x3f << 26)) | (rng.choice(list(range(41)) + list(range(42, 51))) << 26)
                w1 &= ~(7 << 29)
                w0 &= ~((1 << 6) | (1 << 14))
                for i in (1, 2, 3):
                    if not (w2 >> (32 - i)) & 1:
                        pass
                    elif rng.random() < 0.99:
                        w2 &= ~(0x40 << (8 * (3 - i)))
                if (w0 >> 15) & 1:
                    good = [0, 1, 2, 3] if pixel else [0, 1, 2, 3, 15, 62, 62]
                    w0 = (w0 & ~0x3f) | rng.choice(good * 8 + [63, 32, 33, 61])
            if rng.random() < 0.1:
                w0 = (w0 & ~(0x3f << 26)) | (50 << 26)
                w0 &= ~(0xff << 16)
            if rng.random() < 0.05:
                w0, w1, w2 = 0, 0, 0
        code += [w0 & 0xffffffff, w1 & 0xffffffff, w2 & 0xffffffff]
    code_b = be(*code)
    # header
    ninterp = rng.randint(0, 4)
    interps = [rng.randrange(8) | (5 << 4) | (rng.randrange(16) << 8) | (0xf << 12) for _ in range(ninterp)]
    cm = (rng.random() < 0.2) << 1 | rng.randrange(8) << 8
    pc = rng.randrange(8) | rng.randrange(8) << 8 | rng.getrandbits(1) << 18
    if pixel:
        hdr = [len(lits), len(code_b), pc, cm, 0, ninterp << 5, 0, rng.randrange(1, 32)] + interps
    else:
        ves = [(a + ncf_slots) | (rng.choice([5] * 30 + [3]) << 12) | (rng.randrange(8) << 16) for a in fetch_slots
               if rng.random() < 0.95]
        slots = [rng.randrange(nins) + ncf_slots | (rng.getrandbits(1) << 12) for _ in range(rng.randint(0, 3))]
        interps = [v | (rng.randrange(4) << 16) for v in interps]          # each one's first export slot
        hdr = [len(lits), len(code_b), pc, cm, 0, ninterp << 5, 1, len(ves), len(slots), 0x290] + ves + interps + slots
    head = be(*hdr)
    ctab_off = 0x24
    def_off = ctab_off + 4 + len(ctab)
    hdr_off = def_off + len(deftab)
    vsize = hdr_off + len(head)
    phys = lits + code_b
    container = be(0x102a1100 | (0 if pixel else 1), vsize, len(phys), 0, ctab_off, def_off, hdr_off, 0, 0)
    return container + be(len(ctab)) + ctab + deftab + head + phys


def mutate(blob: bytes, rng: random.Random) -> bytes:
    b = bytearray(blob)
    for _ in range(rng.randint(1, 4)):
        k = rng.random()
        if k < 0.5 and b:
            i = rng.randrange(len(b))
            b[i] ^= 1 << rng.randrange(8)
        elif k < 0.7 and len(b) > 4:
            i = rng.randrange(0, min(len(b), 0x24 + 64) - 3)
            b[i:i + 4] = struct.pack('>I', rng.choice([0, 1, 0xffffffff, rng.getrandbits(32), len(b), 0x24]))
        elif k < 0.85:
            del b[rng.randrange(len(b) + 1):]
        else:
            b += bytes(rng.getrandbits(8) for _ in range(rng.randint(1, 16)))
    return bytes(b)


def quick(golden: dict[str, Any]) -> None:
    """The built programs (seed 1943: the generator of the Python-vs-Rust test before 2026-11-18, its random draws in
    the same order): decoded both ways, translated against the recording."""
    t = time.time()
    rng = random.Random(1943)
    kinds: Counter[str] = Counter()
    tkinds: Counter[str] = Counter()
    bad: Counter[str] = Counter()
    example: dict[str, str] = {}
    rec: dict[str, str] = {}
    want = golden.get('built', {})
    n = 1500
    for k in range(n):
        pixel = k % 2 == 0
        blob = build(rng, pixel)
        given: Any = rng.choice([None, pixel, pixel, int(pixel)])        # 1 / 0: kept as given
        if RECORD:
            kind, sh_py = outcome(xenos.parse_shader_python, blob, given)
            d = None
        else:
            d, kind, sh_py, _ = compare_parse(blob, given)
        kinds[kind] += 1
        if d:
            bad['decode: ' + d[:60]] += 1
            example.setdefault('decode: ' + d[:60], d)
            continue
        if kind != 'ok':
            continue
        for j in range(3):
            iface = interface(sh_py, None if j == 0 else random.Random(k * 7 + j))
            key = f'{k}.{j}'
            if RECORD:
                rec[key] = translation(sh_py, iface)
                tkinds[rec[key][:2]] += 1
                continue
            got = [(how, translation(tree, iface)) for how, tree in fresh_trees(blob, None, sh_py)]
            tkinds[got[0][1][:2]] += 1
            against(f'program {k} interface {j}', want.get(key), got, bad, example)
    if RECORD:
        golden['built'] = rec
        print(f'{len(rec)} translations of {n} built programs recorded ({dict(tkinds)})')
    else:
        for key, c in bad.items():
            print(f'       {c} x {example[key]}')
        check(not bad and kinds['ok'] > n // 2 and tkinds['H:'] > n // 2 and len(want) == sum(tkinds.values()),
              f'{n} built programs: the same trees ({kinds["ok"]} decoded, others refused alike: '
              f'{dict((k, v) for k, v in kinds.items() if k != "ok")}); under three interfaces each, every tree '
              f'(fresh, materialized, the Python decoder\'s) translated as recorded ({tkinds["H:"]} texts, '
              f'{tkinds["E:"]} TranslateErrors, others {tkinds["X:"]}; {len(want)} recorded)  ({time.time() - t:.0f} s)')
    # damaged blobs (the decoders only)
    t = time.time()
    kinds.clear()
    bad.clear()
    n = 3000
    for k in range(n):
        blob = mutate(build(rng, k % 2 == 0), rng)
        if RECORD:
            continue
        d, kind, _, _ = compare_parse(blob, None)
        kinds[kind] += 1
        if d:
            bad[d[:60]] += 1
            example.setdefault(d[:60], d)
    if not RECORD:
        for key, c in bad.items():
            print(f'       {c} x {example[key]}')
        check(not bad, f'{n} damaged blobs: decoded alike or refused with the same exception and message '
              f'({dict(kinds)})  ({time.time() - t:.0f} s)')
    # random 32-bit words as whole microcode (the decoder's every field)
    t = time.time()
    kinds.clear()
    bad.clear()
    tkinds.clear()
    rec = {}
    want = golden.get('random microcode', {})
    for k in range(1000):
        pixel = k % 2 == 0
        raw = bytearray(build(rng, pixel))
        code_off = struct.unpack_from('>I', raw, 4)[0]
        hdr = struct.unpack_from('>I', raw, 0x18)[0]
        start = code_off + struct.unpack_from('>I', raw, hdr)[0]
        for i in range(start + 12 * (rng.random() < 0.5), len(raw) - 3, 4):
            raw[i:i + 4] = struct.pack('>I', rng.getrandbits(32))
        blob = bytes(raw)
        if RECORD:
            kind, sh_py = outcome(xenos.parse_shader_python, blob, None)
            d = None
        else:
            d, kind, sh_py, _ = compare_parse(blob, None)
        kinds[kind] += 1
        if d:
            bad[d[:60]] += 1
            example.setdefault(d[:60], d)
        elif kind == 'ok':
            iface = interface(sh_py, random.Random(k))
            if RECORD:
                rec[str(k)] = translation(sh_py, iface)
                tkinds[rec[str(k)][:2]] += 1
            else:
                got = [(how, translation(tree, iface)) for how, tree in fresh_trees(blob, None, sh_py)]
                tkinds[got[0][1][:2]] += 1
                against(f'random program {k}', want.get(str(k)), got, bad, example)
    if RECORD:
        golden['random microcode'] = rec
        print(f'{len(rec)} translations of random microcode recorded ({dict(tkinds)})')
        return
    for key, c in bad.items():
        print(f'       {c} x {example[key]}')
    check(not bad and len(want) == sum(tkinds.values()),
          f'1000 programs of random microcode words: the same trees ({dict(kinds)}), every tree translated as '
          f'recorded ({dict(tkinds)}; {len(want)} recorded)  ({time.time() - t:.0f} s)')


def float_reprs() -> None:
    rng = random.Random(5)
    vals = [float(v) for v in SPECIAL] + [struct.unpack('<d', struct.pack('<Q', rng.getrandbits(64)))[0] for _ in range(20000)]
    vals += [struct.unpack('<f', struct.pack('<I', rng.getrandbits(32)))[0] for _ in range(20000)]
    vals += [rng.uniform(-1, 1) * 10 ** rng.randint(-20, 20) for _ in range(20000)]
    vals += [float(10 ** k) for k in range(-6, 20)] + [1.5 * 10 ** k for k in range(-6, 20)]
    # powers of two and their neighbours (the narrower interval below them), f32 values widened (exact ties)
    for k in range(-1074, 1024):
        p = math.ldexp(1.0, k)
        vals += [p, math.nextafter(p, 0.0), math.nextafter(p, math.inf)]
    vals += [struct.unpack('<f', struct.pack('<f', rng.uniform(-1e4, 1e4)))[0] for _ in range(20000)]
    bad = [v for v in vals if not math.isnan(v) and xenos._native.xenos_float_repr(v) != repr(v)]
    check(not bad, f'{len(vals)} doubles: the Rust repr is Python\'s repr(float) {bad[:3]}')


def dispatch(golden: dict[str, Any]) -> None:
    rng = random.Random(2)
    blob = build(rng, True)
    while True:
        try:
            sh0 = xenos.parse_shader_python(blob, True)
            X.translate(sh0, interface(sh0, None))
            break
        except (xenos.XenosError, X.TranslateError):
            blob = build(rng, True)
    jobs = []
    for k in range(60):
        b = build(rng, k % 2 == 0)
        try:
            s = xenos.parse_shader(b)
        except xenos.XenosError:
            continue
        jobs.append((s, b, interface(s, random.Random(k))))
    if RECORD:
        golden['dispatch'] = {'program': translation(sh0, interface(sh0, None)),
                              'translate_many': [translation(xenos.parse_shader_python(b), i) for _s, b, i in jobs]}
        print(f'dispatch: {len(jobs)} translations recorded')
        return
    want = golden.get('dispatch', {})
    sh = xenos.parse_shader(blob, True)
    check(type(sh) is xenos._NativeShader and xenos.native_program(sh) is not None and 'cf' not in sh.__dict__,
          'parse_shader: decoded in Rust, the microcode not built yet')
    check(translation(sh, interface(sh, None)) == want.get('program'), 'translate: an untouched tree as recorded')
    check(sh.instructions is sh.__dict__['instructions'] and xenos.native_program(sh) is None
          and translation(sh, interface(sh, None)) == want.get('program'),
          'reading the microcode builds it; the tree is then handed over attribute by attribute, as recorded')
    check(not diff(sh0, sh) and repr(sh0) == repr(sh) and sh0.to_dict() == sh.to_dict(), 'repr and to_dict: the same')
    sh2 = xenos.parse_shader(blob, True)
    for name, c in (('copy', copy.copy(sh2)), ('deepcopy', copy.deepcopy(sh2)), ('pickle', pickle.loads(pickle.dumps(sh2)))):
        check(type(c) is xenos.Obj and not diff(sh0, c) and translation(c, interface(c, None)) == want.get('program'),
              f'{name}: a plain Obj, the same tree, translated as recorded')
    check(xenos.disasm(xenos.parse_shader(blob, True)) == xenos.disasm(sh0), 'disasm: the same text')
    one: Any = 1
    check(xenos.parse_shader(blob, one).is_pixel == 1 and xenos.parse_shader(blob, one).is_pixel is not True,
          'is_pixel given as 1 is kept as given (as the Python keeps it)')
    many = xenos.parse_many([(blob, True), (b'junk', None), (blob, None)], errors='return')
    check(not diff(sh0, many[0]) and isinstance(many[1], xenos.XenosError) and not diff(
        xenos.parse_shader_python(blob, None), many[2]), 'parse_many: trees in order, the failure in its place')
    try:
        xenos.parse_many([(b'junk', None)])
        check(False, 'parse_many raises the first failure')
    except xenos.XenosError:
        check(True, 'parse_many raises the first failure')
    ref = want.get('translate_many', [])
    for how, trees in (('Rust trees', [s for s, _, _ in jobs]), ('Python trees', [xenos.parse_shader_python(b) for _, b, _ in jobs])):
        got = X.translate_many([(s, i) for s, (_, _, i) in zip(trees, jobs)], errors='return')
        got = ['E:' + str(g) if isinstance(g, X.TranslateError) else 'H:' + digest(g) for g in got]
        check(got == ref and len(ref) == len(jobs),
              f'translate_many ({how}): {len(jobs)} programs, the recorded texts and errors in order')
    first = next((k for k, r in enumerate(ref) if r.startswith('E:')), None)
    try:
        X.translate_many([(s, i) for s, _, i in jobs])
        check(first is None, 'translate_many raises the first TranslateError')
    except X.TranslateError as e:
        check(first is not None and 'E:' + str(e) == ref[first], 'translate_many raises the first TranslateError')
    xenos.use_native(False)
    check(xenos.backend() == 'python' and type(xenos.parse_shader(blob)) is xenos.Obj
          and translation(xenos.parse_shader(blob, True), interface(sh0, None)) == want.get('program'),
          'use_native(False): Python trees (the decoder), still translated in Rust as recorded')
    check(xenos.use_native(True) == 'rust' and type(xenos.parse_shader(blob)) is xenos._NativeShader,
          'use_native(True): the Rust core again')
    hand_built()
    declines(blob, sh0)


class Stub:
    """A tree node built by hand (any class with attributes: the translator reads them by name)."""

    def __init__(self, **k: Any) -> None:
        self.__dict__.update(k)


def hand_built() -> None:
    """Trees callers make themselves (test_xdiff's, test_asset_accuracy's): only the attributes the translation
    reads, of plain classes; CF instruction lists holding objects of their own; an edited microcode."""
    const = Stub(regset='float4', name='constants', reg=4, count=2, type=Stub(type='float4', cls='vector'))
    smp = Stub(regset='sampler', name='sampler_Texture2', reg=2, count=1, type=Stub(type='sampler2D', cls='object'))
    head = Stub(vertex_elements=[], interpolators=[Stub(usage_index=0, reg=0)], num_temp_regs=2, outputs=1,
                context_misc_fields=Stub(sc_output_screen_xy=False))
    fetch = Stub(kind='fetch', fetch_kind='texture', op='tfetch2D', has_dest=True, const_index=2, src=0,
                 src_swizzle='xy', dst=1, dst_swizzle='xyzw', address=1, offset_x=0.0, offset_y=0.0,
                 mag_filter='linear', min_filter='linear', use_register_lod=True, use_computed_lod=True, lod_bias=0.5,
                 predicated=False)
    sh = Stub(is_pixel=True, ctab=Stub(constants=[const, smp]), literals={7: (1, 0.5, -0.0, 2)}, instructions=[],
              cf=[Stub(op='exec_end', instructions=[fetch], is_end=True)], header=head)
    text = X.translate(sh, X.Interface([0], sampler_registers={2: 3}))  # type: ignore[arg-type]
    lines = text.split('\n')
    check('    float4 f1 = texture_Texture2.SampleLevel(sampler3, r0.xy, (texLod + 0.5));' in lines
          and 'static const float4 lit7 = float4(1.0, 0.5, -0.0, 2.0);' in lines
          and '    r0 = i.t0;' in lines and '    r1.w = f1.w;' in lines,
          'a tree built by hand (plain classes, a CF instruction not in sh.instructions): translated')
    # an edited tree: the materialized microcode changed by a caller is what is translated
    rng = random.Random(4)
    while True:
        b = build(rng, True)
        try:
            e = xenos.parse_shader(b, True)
            base = X.translate(e, X.Interface(sorted(i.usage_index for i in e.header.interpolators)))
        except (xenos.XenosError, X.TranslateError):
            continue
        tex = [i for i in e.instructions if i.kind == 'fetch' and i.fetch_kind == 'texture' and i.op.startswith('tfetch')
               and i.has_dest]
        if tex and 'Sample(' in base:
            break
    tex[0].lod_bias = 1.25
    tex[0].use_computed_lod = False
    edited = X.translate(e, X.Interface(sorted(i.usage_index for i in e.header.interpolators)))
    check(edited != base and ', 1.25);' in edited, 'a materialized tree edited by a caller: the edit translated')


def declines(blob: bytes, sh0: Any) -> None:
    """What the Rust translator does not take is refused with a ValueError saying what (never another backend)."""
    sh = xenos.parse_shader(blob, True)
    odd = interface(sh, None)
    odd.material = {'tint': [object()]}
    try:
        X.translate(sh, odd)
        check(False, 'an interface value of another type: refused')
    except ValueError as e:
        check(not isinstance(e, X.TranslateError) and 'interface' in str(e),
              f'an interface value of another type: refused ({e})')
    bad = copy.deepcopy(sh0)
    bad.literals = {3: (1.0, 2.0)}
    try:
        X.translate(bad, interface(bad, None))
        check(False, 'a literal of another length: refused')
    except ValueError as e:
        check('literal' in str(e), f'a literal of another length: refused ({e})')


# ------------------------------------------------------------------------------------------------ the game corpus

def bf1943_programs(level: str | None = None) -> Any:
    """(group, label, blob, is_pixel) of every program in BF1943's extracted Xbox bundles (bf1943import's source
    folder): the shader databases' (levels/<level>/shaders/single.res and the UI's) and the post-process containers'
    (raw shader blobs). One bundle in memory at a time (bf1943import.Source holds all 550 MB of them); level: only
    that level's databases."""
    import bf1943import
    import shaderdb as S
    folder = bf1943import.default_source()
    fb = bf1943import.fbrb1943()
    for d, dirs, names in os.walk(folder):
        dirs.sort()
        if level and os.path.relpath(d, folder).replace('\\', '/') != f'levels/{level}':
            continue
        for f in sorted(names):
            if not f.endswith('.fbrb') or f.startswith('streaming_sounds'):
                continue
            try:
                bd = fb.Bundle(open(os.path.join(d, f), 'rb').read())
            except ValueError:                      # the bundles of another layout (fbrb1943's docstring)
                continue
            for e in bd.entries:
                k = e['name'].lower()
                if 'shader' not in k or not k.endswith('.res'):
                    continue
                data = bytes(bd.data(e['name']))
                if data[:3] == b'\x10\x2a\x11':
                    if not level:
                        yield 'BF1943 containers', k, data, data[3] == 0
                    continue
                try:
                    db = S.ShaderDb.parse(data, where=k)
                except S.ShaderDbError:
                    continue
                for j, s in enumerate(db.vs):
                    yield 'BF1943 databases', f'{k} vs{j}', bytes(s.code), False
                for j, s in enumerate(db.ps):
                    yield 'BF1943 databases', f'{k} ps{j}', bytes(s.code), True
            del bd


def corpus() -> tuple[list[tuple[str, int, bytes, bool]], list[str]]:
    """(label, count of copies, blob, is_pixel) of every distinct program, and what was skipped."""
    import shaderdb as S
    from bfbc2core import paths
    items: dict[bytes, list[Any]] = {}
    skipped = []
    groups: Counter[str] = Counter()

    def add(group: str, label: str, blob: bytes, pixel: bool) -> None:
        groups[group] += 1
        h = hashlib.sha1(blob + bytes([pixel])).digest()
        if h in items:
            items[h][1] += 1
        else:
            items[h] = [label, 1, blob, pixel]
    try:
        for group, label, blob, pixel in bf1943_programs():
            add(group, label, blob, pixel)
    except Exception as e:  # noqa: BLE001 - reported
        skipped.append(f'Battlefield 1943 ({type(e).__name__}: {e})')
    try:
        opfor = S.find_opfor(None)
        levels = os.path.join(opfor, 'work', 'xbox', 'dist', 'xenon', 'levels')
    except S.ShaderDbError as e:
        levels = ''
        skipped.append(f'BFBC2 Xbox ({e})')
    if levels and os.path.isdir(levels):
        for lv in sorted(os.listdir(levels)):
            path = os.path.join(levels, lv, 'level-00.fbrb')
            if not os.path.isfile(path) or not S.bundle_databases(path):
                continue
            db, _ = S.load(path)
            for j, s in enumerate(db.vs):
                add('BFBC2 Xbox levels', f'{lv} vs{j}', bytes(s.code), False)
            for j, s in enumerate(db.ps):
                add('BFBC2 Xbox levels', f'{lv} ps{j}', bytes(s.code), True)
    elif levels:
        skipped.append(f'BFBC2 Xbox (no unpacked copy at {levels})')
    del paths
    print('programs: ' + ', '.join(f'{n:,} {g}' for g, n in groups.items()) + f'; {len(items):,} distinct', flush=True)
    return [tuple(v) for v in items.values()], skipped


def program_key(blob: bytes, pixel: bool) -> str:
    return hashlib.sha1(blob + bytes([pixel])).hexdigest()[:16]


def recorded_form(outcomes: list[str]) -> Any:
    """A program's three translations as recorded: one digest of the three texts when all translated, else the
    three outcomes (the errors' messages in plain)."""
    if all(o.startswith('H:') for o in outcomes):
        return 'H:' + digest(''.join(outcomes))
    return outcomes


def _check_item(item: tuple[str, int, bytes, bool]) -> tuple[str, Any, list[str], str, list[str], float, float, float]:
    """(the program's key, its translations in the recorded form for each tree kind (fresh, Python decoder's), the
    decoders' differences, the decode kind, the translate kinds, Python decode s, Rust decode s, Rust translate s)."""
    label, _, blob, pixel = item
    key = program_key(blob, pixel)
    t0 = time.perf_counter()
    py = outcome(xenos.parse_shader_python, blob, pixel)
    t1 = time.perf_counter()
    rs = outcome(xenos.parse_shader, blob, pixel)
    t2 = time.perf_counter()
    del rs
    if RECORD:
        d, kind, sh_py = None, py[0], py[1]
    else:
        d, kind, sh_py, _ = compare_parse(blob, pixel)
    diffs = [f'{label}: decode: {d}'] if d else []
    trs = 0.0
    forms: dict[str, Any] = {}
    tk: list[str] = []
    if not d and kind == 'ok':
        h = int.from_bytes(hashlib.sha1(blob).digest()[:4], 'big')
        ifaces = [interface(sh_py, None if j == 0 else random.Random(h * 3 + j)) for j in range(3)]
        hows = [('python tree', sh_py)] if RECORD else [('fresh', None), ('python tree', sh_py)]
        for how, tree in hows:
            got = []
            for iface in ifaces:
                a = time.perf_counter()
                got.append(translation(tree if tree is not None else xenos.parse_shader(blob, pixel), iface))
                trs += time.perf_counter() - a
            forms[how] = recorded_form(got)
            if how == hows[0][0]:
                tk = [g[:2] for g in got]
    return key, forms, diffs, kind, tk, t1 - t0, t2 - t1, trs


def game(golden: dict[str, Any]) -> None:
    t = time.time()
    items, skipped = corpus()
    if skipped:
        print('skipped: ' + '; '.join(skipped))
    if not items:
        check(False, 'no game programs found')
        return
    kinds: Counter[str] = Counter()
    tkinds: Counter[str] = Counter()
    diffs: list[str] = []
    tp: list[float] = [0.0, 0.0, 0.0]
    rec: dict[str, Any] = {}
    same = 0
    bad: Counter[str] = Counter()
    workers = max(1, min(2, (os.cpu_count() or 2) - 1))       # each under ~300 MB
    with ProcessPoolExecutor(workers) as pool:
        for key, forms, d, kind, tk, *times in pool.map(_check_item, items, chunksize=64):
            diffs += d
            kinds[kind] += 1
            tkinds.update(tk)
            for k in range(3):
                tp[k] += float(times[k])
            if not forms:
                continue
            if RECORD:
                rec[key] = forms['python tree']
                continue
            want = golden.get(key)
            ok = True
            for how, f in forms.items():
                if want is None:
                    bad['not recorded'] += 1
                    ok = False
                elif f != want:
                    bad[how] += 1
                    ok = False
                    if bad[how] <= 10:
                        diffs.append(f'{key} ({how}): {f!r:.200} / recorded {want!r:.200}')
            same += ok
    if RECORD:
        golden.clear()
        golden.update(rec)
        print(f'{len(rec):,} programs recorded ({dict(tkinds)}, {dict(kinds)})  ({time.time() - t:.0f} s)')
        return
    for line in diffs[:20]:
        print('       ' + line)
    copies = sum(i[1] for i in items)
    translated = sum(kinds[k] for k in ('ok',))
    check(not diffs and kinds['ok'] == len(items) and same == translated and len(golden) == translated,
          f'{len(items):,} distinct programs ({copies:,} with copies): the same trees ({kinds["ok"]:,} decoded'
          f'{", " + str(dict(kinds)) if len(kinds) > 1 else ""}); 3 interfaces each, the fresh tree and the Python '
          f'decoder\'s translated as recorded on {same:,} of {len(golden):,} recorded programs ({tkinds["H:"]:,} texts, '
          f'{tkinds["E:"]:,} TranslateErrors, others {tkinds["X:"]:,}; differing: {dict(bad)})  ({time.time() - t:.0f} s)')
    print(f'       per program, single thread: decode Python {1e3 * tp[0] / len(items):.3f} ms, Rust (eager tree) '
          f'{1e3 * tp[1] / len(items):.3f} ms; translate Rust (fresh tree and the Python decoder\'s, each with its '
          f'decode) {1e3 * tp[2] / max(2 * sum(tkinds.values()), 1):.3f} ms')


def main() -> int:
    if not RECORD:
        from bfbc2core import native
        native.require('The Xbox shader translator\'s test', 'xenos_tree_program')
        if xenos._native is None:
            check(False, 'the decoder\'s Rust core is not loaded')
            return 1
        float_reprs()
    golden = load(GOLDEN)
    dispatch(golden)
    quick(golden)
    if RECORD:
        write(GOLDEN, golden)
    if '--game' in sys.argv:
        game_golden = load(GOLDEN_GAME)
        game(game_golden)
        if RECORD:
            write(GOLDEN_GAME, game_golden)
    print(f'\n{len(fails)} failed' if fails else '\nall passed')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
