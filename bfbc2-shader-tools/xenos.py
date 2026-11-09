"""Xbox 360 (Xenos GPU) compiled-shader decoder and disassembler.

Decodes the XDK shader container found in Frostbite Xbox 360 shader databases
(Battlefield 1943 / BFBC2 Xbox `Single.res`, via shaderdb.py) and the Xenos
microcode inside it into a Python IR, plus a text disassembly.

Credits / sources:
  * Microcode bit layouts, opcode names and operand semantics follow Xenia's
    src/xenia/gpu/ucode.h, ucode.cc, shader_translator.cc and
    shader_translator_disasm.cc (https://github.com/xenia-project/xenia,
    BSD-3-Clause, Copyright Ben Vanik and contributors). This module is a fresh
    Python implementation of the documented format, not a port of Xenia code.
  * The container layout (constant table offset, definition table with literal
    constants, shader header with interpolators / vertex elements) was checked
    against the field names of hedge-dev's XenosRecomp (MIT) shader.h and
    reverse-engineered/verified against all 1,550 Battlefield 1943 Wake shaders.
See XENOS.md for the spec as implemented and the open uncertainties.

API:
    sh = parse_shader(blob, is_pixel=None)   # None: taken from the magic
    print(disasm(sh))
"""
from __future__ import annotations

import struct
from typing import TYPE_CHECKING, Any

__all__ = ['parse_shader', 'disasm', 'XenosError', 'VECTOR_OPS', 'SCALAR_OPS',
           'CF_OPS', 'FETCH_OPS']


class XenosError(Exception):
    pass


def _bits(v: int, lo: int, n: int) -> int:
    return (v >> lo) & ((1 << n) - 1)


def _sbits(v: int, lo: int, n: int) -> int:
    x = _bits(v, lo, n)
    return x - (1 << n) if x & (1 << (n - 1)) else x


class Obj:
    """Plain attribute bag with a readable repr and dict export."""

    if TYPE_CHECKING:                       # any attribute: the decoder sets them per kind (cf, alu, fetch ...)
        def __getattr__(self, name: str) -> Any: ...
        def __setattr__(self, name: str, value: Any) -> None: ...

    def __init__(self, **kw: Any) -> None:
        self.__dict__.update(kw)

    def __repr__(self) -> str:
        return '%s(%s)' % (getattr(self, 'kind', type(self).__name__),
                           ', '.join('%s=%r' % kv for kv in self.__dict__.items()
                                     if kv[0] != 'kind'))

    def to_dict(self) -> dict[str, Any]:
        def conv(v: Any) -> Any:
            if isinstance(v, Obj):
                return v.to_dict()
            if isinstance(v, (list, tuple)):
                return [conv(x) for x in v]
            if isinstance(v, dict):
                return {k: conv(x) for k, x in v.items()}
            return v
        return {k: conv(v) for k, v in self.__dict__.items()}


# --------------------------------------------------------------------------
# Opcode tables (names as in Xenia ucode.h / ucode.cc)
# --------------------------------------------------------------------------

CF_OPS = ['nop', 'exec', 'exec_end', 'cond_exec', 'cond_exec_end',
          'cond_exec_pred', 'cond_exec_pred_end', 'loop_start', 'loop_end',
          'cond_call', 'return', 'cond_jmp', 'alloc',
          'cond_exec_pred_clean', 'cond_exec_pred_clean_end',
          'mark_vs_fetch_done']
_CF_EXEC = {1, 2, 3, 4, 5, 6, 13, 14}
_CF_END = {2, 4, 6, 14}

FETCH_OPS = {0: 'vfetch', 1: 'tfetch', 16: 'getBCF', 17: 'getCompTexLOD',
             18: 'getGradients', 19: 'getWeights', 24: 'setTexLOD',
             25: 'setGradientH', 26: 'setGradientV'}

# name, operand count (0, 1, 2 = c#.w + r#.x), single operand uses W and X
SCALAR_OPS = [
    ('adds', 1, True), ('adds_prev', 1, False), ('muls', 1, True),
    ('muls_prev', 1, False), ('muls_prev2', 1, True), ('maxs', 1, True),
    ('mins', 1, True), ('seqs', 1, False), ('sgts', 1, False),
    ('sges', 1, False), ('snes', 1, False), ('frcs', 1, False),
    ('truncs', 1, False), ('floors', 1, False), ('exp', 1, False),
    ('logc', 1, False), ('log', 1, False), ('rcpc', 1, False),
    ('rcpf', 1, False), ('rcp', 1, False), ('rsqc', 1, False),
    ('rsqf', 1, False), ('rsq', 1, False), ('maxas', 1, True),
    ('maxasf', 1, True), ('subs', 1, True), ('subs_prev', 1, False),
    ('setp_eq', 1, False), ('setp_ne', 1, False), ('setp_gt', 1, False),
    ('setp_ge', 1, False), ('setp_inv', 1, False), ('setp_pop', 1, False),
    ('setp_clr', 0, False), ('setp_rstr', 1, False), ('kills_eq', 1, False),
    ('kills_gt', 1, False), ('kills_ge', 1, False), ('kills_ne', 1, False),
    ('kills_one', 1, False), ('sqrt', 1, False), ('opcode_41', 0, False),
    ('mulsc', 2, False), ('mulsc', 2, False), ('addsc', 2, False),
    ('addsc', 2, False), ('subsc', 2, False), ('subsc', 2, False),
    ('sin', 1, False), ('cos', 1, False), ('retain_prev', 0, False),
] + [('opcode_%d' % i, 0, False) for i in range(51, 64)]

# name, per-operand used component masks (operand count = len)
VECTOR_OPS = [
    ('add', (15, 15)), ('mul', (15, 15)), ('max', (15, 15)), ('min', (15, 15)),
    ('seq', (15, 15)), ('sgt', (15, 15)), ('sge', (15, 15)), ('sne', (15, 15)),
    ('frc', (15,)), ('trunc', (15,)), ('floor', (15,)),
    ('mad', (15, 15, 15)), ('cndeq', (15, 15, 15)), ('cndge', (15, 15, 15)),
    ('cndgt', (15, 15, 15)), ('dp4', (15, 15)), ('dp3', (7, 7)),
    ('dp2add', (3, 3, 1)), ('cube', (15, 15)), ('max4', (15,)),
    ('setp_eq_push', (9, 9)), ('setp_ne_push', (9, 9)),
    ('setp_gt_push', (9, 9)), ('setp_ge_push', (9, 9)),
    ('kill_eq', (15, 15)), ('kill_gt', (15, 15)), ('kill_ge', (15, 15)),
    ('kill_ne', (15, 15)), ('dst', (6, 10)), ('maxa', (15, 15)),
    ('opcode_30', ()), ('opcode_31', ()),
]
_KNOWN_SCALAR = set(range(0, 41)) | set(range(42, 51))
_KNOWN_VECTOR = set(range(0, 30))

VERTEX_FORMATS = {6: 'FMT_8_8_8_8', 7: 'FMT_2_10_10_10', 16: 'FMT_10_11_11',
                  17: 'FMT_11_11_10', 25: 'FMT_16_16', 26: 'FMT_16_16_16_16',
                  31: 'FMT_16_16_FLOAT', 32: 'FMT_16_16_16_16_FLOAT',
                  33: 'FMT_32', 34: 'FMT_32_32', 35: 'FMT_32_32_32_32',
                  36: 'FMT_32_FLOAT', 37: 'FMT_32_32_FLOAT',
                  38: 'FMT_32_32_32_32_FLOAT', 57: 'FMT_32_32_32_FLOAT'}
TEX_FILTER = ['point', 'linear', 'basemap', 'keep']       # 3 = use fetch const
ANISO_FILTER = ['disabled', 'max1to1', 'max2to1', 'max4to1', 'max8to1',
                'max16to1', 'aniso6', 'keep']               # 7 = use fetch const
ARB_FILTER = ['2x4sym', '2x4asym', '4x2sym', '4x2asym', '4x4sym', '4x4asym',
              'arb6', 'keep']
DIMENSIONS = ['1D', '2D', '3D', 'Cube']
DECL_USAGE = ['POSITION', 'BLENDWEIGHT', 'BLENDINDICES', 'NORMAL', 'PSIZE',
              'TEXCOORD', 'TANGENT', 'BINORMAL', 'TESSFACTOR', 'POSITIONT',
              'COLOR', 'FOG', 'DEPTH', 'SAMPLE', 'usage14', 'usage15']
REGSETS = ['bool', 'int4', 'float4', 'sampler']
PARAM_CLASS = ['scalar', 'vector', 'matrix_rows', 'matrix_columns', 'object',
               'struct']
PARAM_TYPE = ['void', 'bool', 'int', 'float', 'string', 'texture',
              'texture1D', 'texture2D', 'texture3D', 'textureCube',
              'sampler', 'sampler1D', 'sampler2D', 'sampler3D', 'samplerCube',
              'pixelshader', 'vertexshader', 'pixelfragment',
              'vertexfragment', 'unsupported']
XYZW = 'xyzw'


Words = tuple[int, ...]                     # an ALU or fetch instruction's three dwords
Literals = dict[int, tuple[float, ...]]     # def c#: register -> 4 floats
IntLiterals = dict[int, tuple[int, ...]]    # defi i#: register -> 4 ints


# --------------------------------------------------------------------------
# Container
# --------------------------------------------------------------------------

class _Reader:
    def __init__(self, b: bytes) -> None:
        self.b = b

    def u32(self, o: int) -> int:
        if o < 0 or o + 4 > len(self.b):
            raise XenosError('read past end at 0x%x' % o)
        return struct.unpack_from('>I', self.b, o)[0]

    def u16(self, o: int) -> int:
        return struct.unpack_from('>H', self.b, o)[0]

    def f32(self, o: int) -> float:
        return struct.unpack_from('>f', self.b, o)[0]

    def cstr(self, o: int) -> str:
        e = self.b.index(b'\0', o)
        return self.b[o:e].decode('latin-1')


def _parse_type(r: _Reader, base: int, off: int, depth: int = 0) -> Obj:
    t = Obj(cls=PARAM_CLASS[r.u16(base + off)] if r.u16(base + off) < 6 else r.u16(base + off),
            type=PARAM_TYPE[r.u16(base + off + 2)] if r.u16(base + off + 2) < 20 else r.u16(base + off + 2),
            rows=r.u16(base + off + 4), cols=r.u16(base + off + 6),
            elements=r.u16(base + off + 8), members=[])
    nmem = r.u16(base + off + 10)
    moff = r.u32(base + off + 12)
    if nmem and depth < 8:
        for k in range(nmem):
            mo = base + moff + 8 * k
            t.members.append(Obj(name=r.cstr(base + r.u32(mo)),
                                 type=_parse_type(r, base, r.u32(mo + 4), depth + 1)))
    return t


def _parse_ctab(r: _Reader, off: int) -> Obj:
    size = r.u32(off)
    base = off + 4
    hdr = [r.u32(base + 4 * k) for k in range(7)]
    if hdr[0] != 0x1c:
        raise XenosError('CTAB header size %#x' % hdr[0])
    ct = Obj(offset=off, size=size, base=base, creator=r.cstr(base + hdr[1]),
             version=hdr[2], target=r.cstr(base + hdr[6]), flags=hdr[5],
             constants=[])
    for k in range(hdr[3]):
        o = base + hdr[4] + 20 * k
        rs = r.u16(o + 4)
        ti = r.u32(o + 12)
        dv = r.u32(o + 16)
        ct.constants.append(Obj(
            name=r.cstr(base + r.u32(o)), regset=REGSETS[rs] if rs < 4 else rs,
            reg=r.u16(o + 6), count=r.u16(o + 8), reserved=r.u16(o + 10),
            type=_parse_type(r, base, ti), default_offset=dv))
    return ct


def _parse_definitions(r: _Reader, off: int, vsize: int, is_pixel: bool) -> tuple[Obj, Literals, IntLiterals]:
    """Literal (def) constants. Returns (table obj, floats, ints, bools)."""
    h = [r.u32(off + 4 * k) for k in range(5)]
    size = h[4]
    words = [r.u32(off + 20 + 4 * k) for k in range(size // 4)]
    dt = Obj(offset=off, field0=h[0], field4=h[1], field8=h[2], fieldC=h[3],
             size=size, raw_words=words, float_defs=[], int_defs=[], bool_words=[])
    floats: Literals = {}
    ints: IntLiterals = {}
    p = 0
    while p < len(words) and words[p]:
        reg, cnt, phys = words[p] >> 16, words[p] & 0xffff, words[p + 1]
        dt.float_defs.append(Obj(reg=reg, count=cnt, physical_offset=phys))
        base_reg = reg - (256 if is_pixel else 0)
        for v in range((cnt + 3) // 4):
            o = vsize + phys + 16 * v
            floats[base_reg + v] = tuple(r.f32(o + 4 * c) for c in range(4))
        p += 2
    p += 1
    while p < len(words) and words[p]:
        reg, cnt = words[p] >> 16, words[p] & 0xffff
        vals = words[p + 1:p + 1 + cnt]
        dt.int_defs.append(Obj(reg=reg, count=cnt, values=vals))
        # XenosRecomp: i# = (reg - 8992) / 4; values are 4 x int8 (x..w from MSB)
        # UNCERTAIN: never seen in BF1943 (no int literals there).
        for k, v in enumerate(vals):
            ints[(reg - 8992) // 4 + k] = tuple(
                _sbits(v, s, 8) for s in (24, 16, 8, 0))
        p += 1 + cnt
    p += 1
    rest = words[p:]
    while rest and rest[-1] == 0:
        rest = rest[:-1]
    dt.bool_words = rest            # UNCERTAIN format; always empty in BF1943
    return dt, floats, ints


def _parse_header(r: _Reader, off: int, is_pixel: bool, vsize: int) -> Obj:
    w = lambda k: r.u32(off + 4 * k)
    h = Obj(offset=off, physical_offset=w(0), physical_size=w(1),
            program_cntl=w(2), context_misc=w(3), field10=w(4),
            interpolator_info=w(5))
    pc, cm = h.program_cntl, h.context_misc
    # SQ_PROGRAM_CNTL-like word (field names from the Xenos/R400 register).
    h.num_temp_regs = (_bits(pc, 8, 6) if is_pixel else _bits(pc, 0, 6)) + 1
    h.program_cntl_fields = Obj(
        vs_num_reg=_bits(pc, 0, 6), ps_num_reg=_bits(pc, 8, 6),
        vs_resource=_bits(pc, 16, 1), ps_resource=_bits(pc, 17, 1),
        param_gen=_bits(pc, 18, 1), gen_index_pix=_bits(pc, 19, 1),
        vs_export_count=_bits(pc, 20, 4), vs_export_mode=_bits(pc, 24, 3),
        ps_export_mode=_bits(pc, 27, 4), gen_index_vtx=_bits(pc, 31, 1))
    # SQ_CONTEXT_MISC-like word.
    h.context_misc_fields = Obj(
        inst_pred_optimize=_bits(cm, 0, 1), sc_output_screen_xy=_bits(cm, 1, 1),
        sc_sample_cntl=_bits(cm, 2, 2), param_gen_pos=_bits(cm, 8, 8))
    ii = h.interpolator_info
    n_interp = _bits(ii, 5, 5)
    h.interpolator_count = n_interp
    h.vertex_elements = []
    h.interpolators = []
    o = off + 24
    if is_pixel:
        h.interp_mask = w(6)
        h.outputs = w(7)
        o = off + 32
    else:
        h.field18 = w(6)            # always 1 in BF1943: count of the words below
        h.vertex_element_count = w(7)
        h.field20 = w(8)            # count of export_slots words
        o = off + 36
        h.field18_words = [r.u32(o + 4 * k) for k in range(h.field18)]   # always [0x290]: UNKNOWN
        o += 4 * h.field18
        for _k in range(h.vertex_element_count):
            v = r.u32(o)
            # bits 20-23: UNKNOWN (0..3 seen; not stream, format or swizzle)
            h.vertex_elements.append(Obj(
                raw=v, fetch_address=_bits(v, 0, 12), usage=DECL_USAGE[_bits(v, 12, 4)],
                usage_index=_bits(v, 16, 4), unknown_hi=v >> 20))
            o += 4
    for _k in range(n_interp):
        v = r.u32(o)
        # bits 12-15: always 0xf (probably a component mask - UNCERTAIN);
        # bits 16+: VS only, index of this interpolator's first entry in
        # export_slots (verified on all 735 VS).
        h.interpolators.append(Obj(
            raw=v, usage_index=_bits(v, 0, 4), usage=DECL_USAGE[_bits(v, 4, 4)],
            reg=_bits(v, 8, 4), mask=_bits(v, 12, 4), export_index=v >> 16))
        o += 4
    h.export_slots = []
    if not is_pixel:
        # field20 words: instruction slot (bits 0-11) of every ALU instruction
        # that exports an interpolator; bit 12 set on the last one of each
        # interpolator. Presumably what D3D patches when linking VS/PS.
        for _k in range(h.field20):
            v = r.u32(o)
            h.export_slots.append(Obj(raw=v, address=_bits(v, 0, 12), last=_bits(v, 12, 1),
                                      unknown_hi=v >> 13))
            o += 4
        for it in h.interpolators:
            sl = []
            for e in h.export_slots[it.export_index:]:
                sl.append(e.address)
                if e.last:
                    break
            it.export_addresses = sl
    h.end = o
    return h


# --------------------------------------------------------------------------
# Microcode
# --------------------------------------------------------------------------

def _decode_cf(w0: int, w1: int, index: int) -> Obj:
    """w0 = low dword, w1 = the 16 high bits (Xenia's dword_1)."""
    op = _bits(w1, 12, 4)
    cf = Obj(kind='cf', index=index, opcode=op, op=CF_OPS[op],
             address_mode=_bits(w1, 11, 1), raw=(w0, w1))
    if op in _CF_EXEC:
        cf.address = _bits(w0, 0, 12)
        cf.count = _bits(w0, 12, 3)
        cf.is_yield = _bits(w0, 15, 1)
        cf.sequence = _bits(w0, 16, 12)
        cf.vc = _bits(w0, 28, 4) | (_bits(w1, 0, 2) << 4)
        cf.is_end = op in _CF_END
        if op in (3, 4):
            cf.type = 'conditional'
            cf.bool_index = _bits(w1, 2, 8)
            cf.condition = _bits(w1, 10, 1)
            cf.is_predicate_clean = False
        elif op in (5, 6, 13, 14):
            cf.type = 'predicated'
            cf.condition = _bits(w1, 10, 1)
            cf.is_predicate_clean = bool(_bits(w1, 9, 1)) or op in (13, 14)
            # UNCERTAIN: Xenia treats 13/14 as cond_exec with a bool constant
            # and clean predicate; not present in BF1943.
            if op in (13, 14):
                cf.type = 'conditional'
                cf.bool_index = _bits(w1, 2, 8)
        else:
            cf.type = 'unconditional'
            cf.is_predicate_clean = bool(_bits(w1, 9, 1))
    elif op == 7:
        cf.address = _bits(w0, 0, 13)
        cf.is_repeat = _bits(w0, 13, 1)
        cf.loop_id = _bits(w0, 16, 5)
    elif op == 8:
        cf.address = _bits(w0, 0, 13)
        cf.loop_id = _bits(w0, 16, 5)
        cf.is_predicated_break = _bits(w0, 21, 1)
        cf.condition = _bits(w1, 10, 1)
    elif op in (9, 11):
        cf.address = _bits(w0, 0, 13)
        cf.is_unconditional = _bits(w0, 13, 1)
        cf.is_predicated = _bits(w0, 14, 1)
        cf.bool_index = _bits(w1, 2, 8)
        cf.condition = _bits(w1, 10, 1)
        cf.type = ('unconditional' if cf.is_unconditional else
                   'predicated' if cf.is_predicated else 'conditional')
        if op == 11:
            cf.direction = _bits(w1, 1, 1)
    elif op == 12:
        cf.size = _bits(w0, 0, 3)
        cf.is_unserialized = _bits(w1, 8, 1)
        cf.alloc_type = _bits(w1, 9, 2)
    return cf


def _swz_alu(sw: int, comp: int) -> int:
    return ((sw >> (2 * comp)) + comp) & 3


def _alu_operand(w: Words, i: int) -> Obj:
    """Vector operand i (1..3) per Xenia ParseAluInstructionOperand."""
    w0, w1, w2 = w
    reg = _bits(w2, 8 * (3 - i), 8)
    is_temp = _bits(w2, 32 - i, 1)
    swz = _bits(w1, 8 * (3 - i), 8)
    neg = _bits(w1, 27 - i, 1)
    o = Obj(kind='operand', negate=bool(neg), swizzle_raw=swz,
            swizzle=''.join(XYZW[_swz_alu(swz, c)] for c in range(4)))
    if is_temp:
        o.file = 'temp'
        o.index = reg & 0x3f
        o.rel = 'aL' if reg & 0x40 else None
        o.abs = bool(reg & 0x80)
    else:
        o.file = 'const'
        o.index = reg
        o.rel = _const_rel(w, i)
        o.abs = bool(_bits(w0, 7, 1))
    return o


def _const_rel(w: Words, i: int) -> str | None:
    w0, w1, w2 = w
    c0, c1 = _bits(w1, 31, 1), _bits(w1, 30, 1)
    t1, t2 = _bits(w2, 31, 1), _bits(w2, 30, 1)
    if i == 1:
        addressed = c0
    elif i == 2:
        addressed = c0 if t1 else c1
    else:
        addressed = c0 if (t1 and t2) else c1
    if not addressed:
        return None
    return 'a0' if _bits(w1, 29, 1) else 'aL'


def _export_target(idx: int, is_pixel: bool) -> tuple[str, int | None]:
    if idx == 32:
        return 'eA', None
    if 33 <= idx <= 37:
        return 'eM', idx - 33
    if is_pixel:
        if idx <= 3:
            return 'oC', idx
        if idx == 61:
            return 'oDepth', None
    else:
        if idx <= 15:
            return 'o', idx
        if idx == 62:
            return 'oPos', None
        if idx == 63:
            return 'oPts', None
    return 'export?', idx


def _decode_alu(w: Words, addr: int, is_pixel: bool) -> Obj:
    w0, w1, w2 = w
    vop = _bits(w2, 24, 5)
    sop = _bits(w0, 26, 6)
    is_export = bool(_bits(w0, 15, 1))
    vmask, smask = _bits(w0, 16, 4), _bits(w0, 20, 4)
    ins = Obj(kind='alu', address=addr, raw=w,
              predicated=bool(_bits(w1, 28, 1)),
              pred_condition=bool(_bits(w1, 27, 1)),
              is_export=is_export, abs_constants=bool(_bits(w0, 7, 1)),
              const_addr_reg_relative=bool(_bits(w1, 29, 1)),
              const_0_rel=bool(_bits(w1, 31, 1)), const_1_rel=bool(_bits(w1, 30, 1)),
              vector_opcode=vop, vector_op=VECTOR_OPS[vop][0],
              scalar_opcode=sop, scalar_op=SCALAR_OPS[sop][0],
              vector_write_mask_raw=vmask, scalar_write_mask_raw=smask)
    vdest_i, sdest_i = _bits(w0, 0, 6), _bits(w0, 8, 6)
    if is_export:
        tgt, ti = _export_target(vdest_i, is_pixel)
        c0 = (15 & ~(vmask | smask)) if _bits(w0, 14, 1) else 0
        c1 = vmask & smask
        vm = vmask & ~smask
        sm = smask & ~vmask
        vcomp = ''.join('0' if c0 >> c & 1 else '1' if c1 >> c & 1 else
                        XYZW[c] if vm >> c & 1 else '_' for c in range(4))
        scomp = ''.join(XYZW[c] if sm >> c & 1 else '_' for c in range(4))
        ins.vector_dest = Obj(file=tgt, index=ti, rel=None, raw_index=vdest_i,
                              write=vcomp, saturate=bool(_bits(w0, 24, 1)))
        ins.scalar_dest = Obj(file=tgt, index=ti, rel=None, raw_index=vdest_i,
                              write=scomp, saturate=bool(_bits(w0, 25, 1)))
    else:
        ins.vector_dest = Obj(file='temp', index=vdest_i,
                              rel='aL' if _bits(w0, 6, 1) else None,
                              write=''.join(XYZW[c] if vmask >> c & 1 else '_' for c in range(4)),
                              saturate=bool(_bits(w0, 24, 1)))
        ins.scalar_dest = Obj(file='temp', index=sdest_i,
                              rel='aL' if _bits(w0, 14, 1) else None,
                              write=''.join(XYZW[c] if smask >> c & 1 else '_' for c in range(4)),
                              saturate=bool(_bits(w0, 25, 1)))
    ins.vector_operands = [_alu_operand(w, i + 1) for i in range(len(VECTOR_OPS[vop][1]))]
    name, nops, two = SCALAR_OPS[sop]
    ins.scalar_operands = []
    if nops == 1:
        o = _alu_operand(w, 3)
        o.components = o.swizzle[3] + (o.swizzle[0] if two else '')
        ins.scalar_operands.append(o)
    elif nops == 2:
        swz = _bits(w1, 0, 8)
        neg = bool(_bits(w1, 24, 1))
        absc = bool(_bits(w0, 7, 1))
        ins.scalar_operands.append(Obj(
            kind='operand', file='const', index=_bits(w2, 0, 8), rel=_const_rel(w, 3),
            negate=neg, abs=absc, swizzle_raw=swz, components=XYZW[_swz_alu(swz, 3)]))
        treg = (sop & 1) | (_bits(w2, 29, 1) << 1) | (swz & 0x3c)
        ins.scalar_operands.append(Obj(
            kind='operand', file='temp', index=treg, rel=None,
            negate=neg, abs=absc, swizzle_raw=swz, components=XYZW[_swz_alu(swz, 0)]))
    ins.vector_nop = _vector_default_nop(ins)
    ins.scalar_nop = (sop == 50 and not ins.scalar_dest.write.strip('_')
                      and not ins.scalar_dest.saturate
                      and (is_export or (ins.scalar_dest.index == 0 and not ins.scalar_dest.rel)))
    if ins.vector_nop and is_export and ins.scalar_nop:
        ins.vector_nop = False
    return ins


def _vector_default_nop(ins: Obj) -> bool:
    if ins.vector_opcode != 2 or ins.vector_dest.write.strip('_') or ins.vector_dest.saturate:
        return False
    for o in ins.vector_operands:
        if (o.file != 'temp' or o.index or o.rel or o.negate or o.abs
                or o.swizzle != 'xyzw'):
            return False
    if not ins.is_export and (ins.vector_dest.index or ins.vector_dest.rel):
        return False
    return True


def _fetch_dst(swz: int) -> str:
    return ''.join('xyzw01?_'[_bits(swz, 3 * c, 3)] for c in range(4))


def _decode_fetch(w: Words, addr: int, prev_vfetch: Obj | None) -> Obj:
    w0, w1, w2 = w
    op = _bits(w0, 0, 5)
    ins = Obj(kind='fetch', address=addr, raw=w, opcode=op,
              op=FETCH_OPS.get(op, 'fetch_%d' % op),
              src=_bits(w0, 5, 6), src_rel='aL' if _bits(w0, 11, 1) else None,
              dst=_bits(w0, 12, 6), dst_rel='aL' if _bits(w0, 18, 1) else None,
              dst_swizzle=_fetch_dst(_bits(w1, 0, 12)),
              predicated=bool(_bits(w1, 31, 1)),
              pred_condition=bool(_bits(w2, 31, 1)))
    if op == 0:
        ins.fetch_kind = 'vertex'
        ins.must_be_one = _bits(w0, 19, 1)
        ins.const_index = _bits(w0, 20, 5)
        ins.const_index_sel = _bits(w0, 25, 2)
        ins.prefetch_count = _bits(w0, 27, 3)
        ins.src_swizzle = XYZW[_bits(w0, 30, 2)]
        ins.is_signed = bool(_bits(w1, 12, 1))
        ins.is_integer = bool(_bits(w1, 13, 1))
        ins.signed_rf_mode = _bits(w1, 14, 1)
        ins.is_index_rounded = bool(_bits(w1, 15, 1))
        ins.format = _bits(w1, 16, 6)
        ins.format_name = VERTEX_FORMATS.get(ins.format, 'FMT_%d' % ins.format)
        ins.exp_adjust = _sbits(w1, 24, 6)
        ins.is_mini = bool(_bits(w1, 30, 1))
        ins.stride = _bits(w2, 0, 8)
        ins.offset = _sbits(w2, 8, 23)
        full = prev_vfetch if (ins.is_mini and prev_vfetch is not None) else ins
        ins.fetch_const = full.const_index * 3 + full.const_index_sel
        ins.vf = 95 - ins.fetch_const          # D3D-side numbering (Xenia disasm)
        if ins.is_mini and full is not ins:
            ins.src, ins.src_rel, ins.src_swizzle = full.src, full.src_rel, full.src_swizzle
            ins.stride = full.stride
        ins.op = 'vfetch_mini' if ins.is_mini else 'vfetch_full'
    else:
        ins.fetch_kind = 'texture'
        ins.fetch_valid_only = bool(_bits(w0, 19, 1))
        ins.const_index = _bits(w0, 20, 5)
        ins.unnormalized = bool(_bits(w0, 25, 1))
        ins.src_swizzle_raw = _bits(w0, 26, 6)
        ins.mag_filter = TEX_FILTER[_bits(w1, 12, 2)]
        ins.min_filter = TEX_FILTER[_bits(w1, 14, 2)]
        ins.mip_filter = TEX_FILTER[_bits(w1, 16, 2)]
        ins.aniso_filter = ANISO_FILTER[_bits(w1, 18, 3)]
        ins.arbitrary_filter = ARB_FILTER[_bits(w1, 21, 3)]
        ins.vol_mag_filter = TEX_FILTER[_bits(w1, 24, 2)]
        ins.vol_min_filter = TEX_FILTER[_bits(w1, 26, 2)]
        ins.use_computed_lod = bool(_bits(w1, 28, 1))
        ins.use_register_lod = bool(_bits(w1, 29, 1))
        ins.unk = _bits(w1, 30, 1)
        ins.use_register_gradients = bool(_bits(w2, 0, 1))
        ins.sample_location = 'center' if _bits(w2, 1, 1) else 'centroid'
        ins.lod_bias = _sbits(w2, 2, 7) / 16.0
        ins.unused = _bits(w2, 9, 5)
        ins.dimension = DIMENSIONS[_bits(w2, 14, 2)]
        ins.offset_x = _sbits(w2, 16, 5) / 2.0
        ins.offset_y = _sbits(w2, 21, 5) / 2.0
        ins.offset_z = _sbits(w2, 26, 5) / 2.0
        ncomp = {'1D': 1, '2D': 2, '3D': 3, 'Cube': 3}[ins.dimension]
        if op == 18:
            ncomp = 2
        elif op == 24:
            ncomp = 1
        elif op in (25, 26):
            ncomp = 3
        ins.src_swizzle = ''.join(XYZW[_bits(ins.src_swizzle_raw, 2 * c, 2)] for c in range(ncomp))
        ins.has_dest = op not in (24, 25, 26)
        ins.has_const = op not in (24, 25, 26)
        ins.fetch_const = ins.const_index
        if op in (1, 16, 17, 19):
            ins.op = {1: 'tfetch', 16: 'getBCF', 17: 'getCompTexLOD', 19: 'getWeights'}[op] + ins.dimension
    return ins


def _decode_microcode(r: _Reader, code_off: int, size: int, is_pixel: bool) -> Obj:
    nw = size // 4
    words = [r.u32(code_off + 4 * k) for k in range(nw)]
    nslots = nw // 3
    # CF bound: lowest exec address (as Xenia / freedreno)
    bound = nslots
    i = 0
    while i < bound:
        d0, d1, d2 = words[3 * i:3 * i + 3]
        for cfw0, cfw1 in ((d0, d1 & 0xffff), (((d1 >> 16) | (d2 << 16)) & 0xffffffff, d2 >> 16)):
            if _bits(cfw1, 12, 4) in _CF_EXEC:
                bound = min(bound, _bits(cfw0, 0, 12))
        i += 1
    cfs = []
    for i in range(bound):
        d0, d1, d2 = words[3 * i:3 * i + 3]
        cfs.append(_decode_cf(d0, d1 & 0xffff, 2 * i))
        cfs.append(_decode_cf(((d1 >> 16) | (d2 << 16)) & 0xffffffff, d2 >> 16, 2 * i + 1))
    # trim trailing padding nops after the last end (keep anything meaningful)
    last = len(cfs)
    while last > 0 and cfs[last - 1].opcode == 0:
        last -= 1
    cf_padding = cfs[last:]
    cfs = cfs[:last]
    instrs: dict[int, Obj] = {}
    prev_vf: Obj | None = None
    for cf in cfs:
        if cf.opcode not in _CF_EXEC:
            continue
        cf.instructions = []
        seq = cf.sequence
        for a in range(cf.address, cf.address + cf.count):
            if a >= nslots:
                raise XenosError('exec at cf %d reads past code (%d >= %d)' % (cf.index, a, nslots))
            w = tuple(words[3 * a:3 * a + 3])
            is_fetch = seq & 1
            serialize = bool(seq & 2)
            seq >>= 2
            if is_fetch:
                ins = _decode_fetch(w, a, prev_vf)
                if ins.fetch_kind == 'vertex' and not ins.is_mini:
                    prev_vf = ins
            else:
                ins = _decode_alu(w, a, is_pixel)
            ins.serialize = serialize
            ins.cf_index = cf.index
            if a in instrs:
                raise XenosError('instruction slot %d executed by two CFs' % a)
            instrs[a] = ins
            cf.instructions.append(ins)
    unused = [a for a in range(bound, nslots) if a not in instrs]
    unused_nonzero = [a for a in unused if any(words[3 * a:3 * a + 3])]
    return Obj(words=words, cf_slots=bound, cf=cfs, cf_padding=len(cf_padding),
               instructions=[instrs[a] for a in sorted(instrs)],
               unused_slots=unused, unused_nonzero_slots=unused_nonzero)


# --------------------------------------------------------------------------
# Top level
# --------------------------------------------------------------------------

def parse_shader(blob: bytes | bytearray | memoryview, is_pixel: bool | None = None) -> Obj:
    blob = bytes(blob)
    r = _Reader(blob)
    magic = r.u32(0)
    if magic >> 8 != 0x102a11:
        raise XenosError('bad magic %08x' % magic)
    if is_pixel is None:
        is_pixel = (magic & 0xff) == 0
    c = [r.u32(4 * k) for k in range(9)]
    sh = Obj(kind='XenosShader', is_pixel=is_pixel, magic=magic,
             virtual_size=c[1], physical_size=c[2], fieldC=c[3],
             ctab_offset=c[4], def_offset=c[5], header_offset=c[6],
             field1C=c[7], field20=c[8])
    if c[1] + c[2] != len(blob):
        raise XenosError('virtual+physical size %d != blob %d' % (c[1] + c[2], len(blob)))
    sh.ctab = _parse_ctab(r, c[4]) if c[4] else None
    sh.literals, sh.int_literals, sh.bool_literals = {}, {}, {}
    sh.def_table = None
    if c[5]:
        sh.def_table, sh.literals, sh.int_literals = _parse_definitions(r, c[5], c[1], is_pixel)
    sh.header = _parse_header(r, c[6], is_pixel, c[1])
    h = sh.header
    # layout sanity: CTAB container, def table and header tile the virtual part
    sh.layout_gaps = []
    pos = 0x24
    for start, end in ((c[4], c[4] + 4 + sh.ctab.size if sh.ctab else c[4]),
                       (c[5], c[5] + 20 + sh.def_table.size if c[5] else c[5]),
                       (c[6], h.end)):
        if not start:
            continue
        if start != pos:
            sh.layout_gaps.append((pos, start))
        pos = end
    if pos != c[1]:
        sh.layout_gaps.append((pos, c[1]))
    if h.physical_offset + h.physical_size != c[2]:
        raise XenosError('microcode does not end the blob')
    sh.code_offset = c[1] + h.physical_offset
    sh.code_size = h.physical_size
    mc = _decode_microcode(r, sh.code_offset, sh.code_size, is_pixel)
    sh.cf = mc.cf
    sh.instructions = mc.instructions
    sh.cf_slots = mc.cf_slots
    sh.unused_slots = mc.unused_slots
    sh.unused_nonzero_slots = mc.unused_nonzero_slots
    sh.code_words = mc.words
    return sh


# --------------------------------------------------------------------------
# Disassembly
# --------------------------------------------------------------------------

def _pred(p: object, c: object) -> str:
    return (' (p0) ' if c else '(!p0) ') if p else '      '


def _reg(file: str, index: int, rel: str | None) -> str:
    if rel:
        return '%s[%d+%s]' % (file, index, rel)
    return '%s%d' % (file, index)


def _src(o: Obj, ncomp: int = 4) -> str:
    s = '-' if o.negate else ''
    f = 'r' if o.file == 'temp' else 'c'
    if o.abs:
        s += '|' + _reg(f, o.index, o.rel) + '|'
    else:
        s += _reg(f, o.index, o.rel)
    comps = getattr(o, 'components', None)
    if comps is None:
        comps = o.swizzle
        if comps == 'xyzw':
            return s
    return s + '.' + comps


def _dst(d: Obj) -> str:
    if d.file == 'temp':
        s = _reg('r', d.index, d.rel)
    elif d.index is None:
        s = d.file
    else:
        s = '%s%d' % (d.file, d.index)
    if d.write == 'xyzw':
        return s
    if not d.write.strip('_'):
        return s + '._'
    return s + '.' + d.write


def disasm_instruction(ins: Obj) -> list[str]:
    lines: list[str] = []
    if ins.kind == 'alu':
        p = _pred(ins.predicated, ins.pred_condition)
        if ins.vector_nop and ins.scalar_nop:
            return [p + 'nop']
        if not ins.vector_nop:
            s = ins.vector_op + ('_sat' if ins.vector_dest.saturate else '')
            ops = [_dst(ins.vector_dest)] + [_src(o) for o in ins.vector_operands]
            lines.append(p + s + ' ' + ', '.join(ops))
        if not ins.scalar_nop:
            s = ins.scalar_op + ('_sat' if ins.scalar_dest.saturate else '')
            ops = [_dst(ins.scalar_dest)] + [_src(o) for o in ins.scalar_operands]
            lines.append(('   + ' if not ins.vector_nop else '') + p + s + ' ' + ', '.join(ops))
        return lines
    p = _pred(ins.predicated, ins.pred_condition)
    if ins.fetch_kind == 'vertex':
        s = p + ins.op + ' ' + _dst(Obj(file='temp', index=ins.dst, rel=ins.dst_rel,
                                        write=ins.dst_swizzle))
        if not ins.is_mini:
            s += ', %s.%s, vf%d' % (_reg('r', ins.src, ins.src_rel), ins.src_swizzle, ins.vf)
            if ins.is_index_rounded:
                s += ', RoundIndex=true'
        if ins.exp_adjust:
            s += ', ExpAdjust=%d' % ins.exp_adjust
        if ins.offset:
            s += ', Offset=%d' % ins.offset
        s += ', DataFormat=' + ins.format_name
        if not ins.is_mini and ins.stride:
            s += ', Stride=%d' % ins.stride
        if ins.is_signed:
            s += ', Signed=true'
        if ins.is_integer:
            s += ', NumFormat=integer'
        if ins.prefetch_count:
            s += ', PrefetchCount=%d' % (ins.prefetch_count + 1)
        return [s]
    s = p + ins.op + ' '
    parts = []
    if ins.has_dest:
        parts.append(_dst(Obj(file='temp', index=ins.dst, rel=ins.dst_rel, write=ins.dst_swizzle)))
    parts.append('%s.%s' % (_reg('r', ins.src, ins.src_rel), ins.src_swizzle))
    if ins.has_const:
        parts.append('tf%d' % ins.const_index)
    s += ', '.join(parts)
    if not ins.fetch_valid_only:
        s += ', FetchValidOnly=false'
    if ins.unnormalized:
        s += ', UnnormalizedTextureCoords=true'
    for nm, v in (('MagFilter', ins.mag_filter), ('MinFilter', ins.min_filter),
                  ('MipFilter', ins.mip_filter), ('AnisoFilter', ins.aniso_filter),
                  ('VolMagFilter', ins.vol_mag_filter), ('VolMinFilter', ins.vol_min_filter)):
        if v != 'keep':
            s += ', %s=%s' % (nm, v)
    if not ins.use_computed_lod:
        s += ', UseComputedLOD=false'
    if ins.use_register_lod:
        s += ', UseRegisterLOD=true'
    if ins.use_register_gradients:
        s += ', UseRegisterGradients=true'
    if ins.sample_location != 'centroid':
        s += ', SampleLocation=center'
    if ins.lod_bias:
        s += ', LODBias=%g' % ins.lod_bias
    for nm, v in (('OffsetX', ins.offset_x), ('OffsetY', ins.offset_y), ('OffsetZ', ins.offset_z)):
        if v:
            s += ', %s=%g' % (nm, v)
    return [s]


def _cf_text(cf: Obj, is_pixel: bool) -> str:
    op = cf.opcode
    if op == 0:
        return '      cnop'
    if op in _CF_EXEC:
        name = ('exece' if cf.is_end else 'exec')
        if cf.type == 'conditional':
            s = '      c%s %sb%d' % (name, '' if cf.condition else '!', cf.bool_index)
        elif cf.type == 'predicated':
            s = (' (p0) ' if cf.condition else '(!p0) ') + name
        else:
            s = '      ' + name
        s += '    ADDR(%d) CNT(%d)' % (cf.address, cf.count)
        if cf.is_yield:
            s += ' Yield=true'
        if not cf.is_predicate_clean:
            s += ' PredicateClean=false'
        return s
    if op == 7:
        return '      loop i%d, L%d%s' % (cf.loop_id, cf.address, ', Repeat=true' if cf.is_repeat else '')
    if op == 8:
        p = (' (p0) ' if cf.condition else '(!p0) ') if cf.is_predicated_break else '      '
        return p + 'endloop i%d, L%d' % (cf.loop_id, cf.address)
    if op in (9, 11):
        nm = 'call' if op == 9 else 'jmp'
        if cf.type == 'unconditional':
            return '      %s L%d' % (nm, cf.address)
        if cf.type == 'predicated':
            return (' (p0) ' if cf.condition else '(!p0) ') + '%s L%d' % (nm, cf.address)
        return '      c%s %sb%d, L%d' % (nm, '' if cf.condition else '!', cf.bool_index, cf.address)
    if op == 10:
        return '      ret'
    if op == 12:
        t = cf.alloc_type
        what = {0: 'none', 1: 'position', 2: 'colors' if is_pixel else 'interpolators',
                3: 'export = %d' % cf.size}[t]
        return '      alloc ' + what + (' (size %d)' % cf.size if t != 3 else '')
    if op == 15:
        return '      mark_vs_fetch_done'
    return '      cf_%d' % op


def disasm(sh: Obj, show_raw: bool = False, show_header: bool = True) -> str:
    out: list[str] = []
    kind = 'ps' if sh.is_pixel else 'vs'
    if show_header:
        h = sh.header
        out.append('// %s (%s), creator %s' % (kind, sh.ctab.target if sh.ctab else '?',
                                               sh.ctab.creator if sh.ctab else '?'))
        out.append('// code at 0x%x, %d bytes (%d slots, %d CF slots); temps declared %d'
                   % (sh.code_offset, sh.code_size, sh.code_size // 12, sh.cf_slots, h.num_temp_regs))
        if sh.ctab:
            out.append('// constant table:')
            for c in sh.ctab.constants:
                pre = {'float4': 'c', 'int4': 'i', 'bool': 'b', 'sampler': 's'}.get(c.regset, '?')
                out.append('//   %-44s %s%d%s  %s %s %dx%d%s' % (
                    c.name, pre, c.reg, ('..%s%d' % (pre, c.reg + c.count - 1)) if c.count > 1 else '',
                    c.type.cls, c.type.type, c.type.rows, c.type.cols,
                    (' [%d]' % c.type.elements) if c.type.elements > 1 else ''))
        for reg in sorted(sh.literals):
            out.append('    def c%d, %s' % (reg, ', '.join('%g' % v for v in sh.literals[reg])))
        for reg in sorted(sh.int_literals):
            out.append('    defi i%d, %s' % (reg, ', '.join('%d' % v for v in sh.int_literals[reg])))
        if sh.is_pixel:
            outs = [n for b, n in ((1, 'oC0'), (2, 'oC1'), (4, 'oC2'), (8, 'oC3'), (16, 'oDepth'))
                    if h.outputs & b]
            out.append('// outputs: %s' % ' '.join(outs))
            if h.context_misc_fields.sc_output_screen_xy:
                out.append('// vPos/vFace param-gen -> r%d' % h.context_misc_fields.param_gen_pos)
            for it in h.interpolators:
                out.append('    dcl_%s%d r%d' % (it.usage.lower(), it.usage_index, it.reg))
        else:
            for ve in h.vertex_elements:
                out.append('    dcl_%s%d  (vfetch at slot %d)' % (ve.usage.lower(), ve.usage_index, ve.fetch_address))
            for it in h.interpolators:
                out.append('    dcl_%s%d o%d  (exported at slots %s)' % (
                    it.usage.lower(), it.usage_index, it.reg, ','.join(map(str, it.export_addresses))))
    labels: set[int] = set()
    for cf in sh.cf:
        if cf.opcode in (7, 8, 9, 11):
            labels.add(cf.address)
    for cf in sh.cf:
        if cf.index in labels:
            out.append('                label L%d' % cf.index)
        out.append('/* %4d.%d */ %s' % (cf.index // 2, cf.index % 2, _cf_text(cf, sh.is_pixel)))
        for ins in getattr(cf, 'instructions', []):
            for k, line in enumerate(disasm_instruction(ins)):
                prefix = '/* %4d   */ ' % ins.address if k == 0 else '             '
                if k == 0 and ins.serialize:
                    line += '    // serialize'
                if k == 0 and show_raw:
                    line = line.ljust(60) + ' // %08x %08x %08x' % ins.raw
                out.append(prefix + '   ' + line)
    return '\n'.join(out) + '\n'
