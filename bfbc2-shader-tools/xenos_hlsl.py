"""Xenos (Xbox 360 GPU) shader microcode -> HLSL for BFBC2's Dx10 / Dx11 renderers (shader model 4 / 5).

Front end: xenos.py (decoder). Instruction semantics follow Xenia's ucode.h documentation (BSD-3); cube-map fetches
keep the direction the cube instruction was given, as XenosRecomp does (MIT).

translate(sh, iface) -> HLSL source with entry point 'main'. iface (Interface) tells the translator what the PC side
provides: the vertex inputs (VS), the interpolators shared by the VS and PS, material constant values, and per-sampler
fix-ups. Engine constants keep their names (pcbuild binds them by name).
"""
from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from xenos import Obj


FLT_MAX = '3.402823466e+38'

# engine constants: natural size in float4 registers on PC (Dx10/11 cbuffer); anything else is a material constant
ENGINE_SIZE = {
    'viewMatrix': 4, 'viewProjMatrix': 4, 'worldViewProjMatrix': 4, 'worldMatrix': 4, 'viewportZMinMax': 1,
    'cameraPos': 1, 'time': 1, 'invScreenSize': 1, 'pointLightPos0': 1, 'pointLightColor0': 1, 'pointLightRadius0': 1,
    'outdoorLightDir': 1, 'outdoorLightHemisphereDir': 1, 'outdoorLightKeyColor': 1,
    'outdoorLightKeySpecularColorAndHemisphereVisibility': 1, 'outdoorLightTopColor': 1, 'outdoorLightBottomColor': 1,
    'outdoorLightShadowTransform0': 4, 'outdoorLightShadowTransform1': 4, 'outdoorLightShadowTransform2': 4,
    'outdoorLightShadowmapSizeAndInvSize': 1, 'transparentStartAndEndAndClamp': 1,
    'boneMatrices': 180, 'boneVectors': 180,
}
SHADOW = 'outdoorLightShadowmapTexture'
PROBE_X0, PROBE_Y0, PROBE_W, PROBE_H = 60, 100, 26, 20     # the bit-bar block (screen pixels)
# instanced draws (BFBC2 StreamInstancingFloat): these engine matrices are rebuilt from the instance's rows
INSTANCED = {'worldMatrix': 'instW', 'worldViewProjMatrix': 'instWVP'}
INSTANCE_SEMANTICS = (13, 14, 15)
INSTANCE_PRELUDE = '''    float4 instW[4], instWVP[4];
    [unroll] for (int k = 0; k < 3; k++)
        instW[k] = vin.inst0 * worldMatrix[k].x + vin.inst1 * worldMatrix[k].y + vin.inst2 * worldMatrix[k].z
                 + float4(0.0, 0.0, 0.0, worldMatrix[k].w);
    instW[3] = float4(0.0, 0.0, 0.0, 1.0);
    [unroll] for (int j = 0; j < 4; j++)
        instWVP[j] = viewProjMatrix[j].x * instW[0] + viewProjMatrix[j].y * instW[1] + viewProjMatrix[j].z * instW[2]
                   + float4(0.0, 0.0, 0.0, viewProjMatrix[j].w);'''


class TranslateError(Exception):
    pass


class Interface:
    def __init__(self, interpolators: Sequence[int], vs_inputs: dict[int, str] | None = None,
                 material: dict[str, Any] | None = None, sampler_fix: dict[int, str] | None = None,
                 colour_scale: float = 1.0, instanced: bool = False, bones_as_world: bool = False,
                 material_regs: dict[int, Sequence[float]] | None = None) -> None:
        self.bones_as_world = bones_as_world        # BFBC2 composite (kind 9): parts drawn with the world transform
        self.trace_nan = False                      # debug: output the first instruction whose result is NaN / inf
        self.probe: list[str] | None = None         # debug: [constant expressions] drawn as bit bars (PROBE_*)
        self.instanced = instanced                  # VS: the world transform comes per instance (BFBC2 stream 1)
        self.interpolators = interpolators          # [usage index] the VS writes (TEXCOORDn), shared VS -> PS
        self.vs_inputs = vs_inputs or {}            # VS: usage index -> HLSL type of the PC element ('float4')
        self.material = material or {}              # name -> (x, y, z, w)
        # register -> (x, y, z, w): a parameter block's constants by the register they set. A block names each value
        # (sunDir, TrackOffset, fadeFactor ...), the compiled shader declares one array, 'constants', over those
        # registers: every block constant of BF1943's Wake database lies inside the vertex or pixel shader's
        # 'constants' array and none matches a constant table name, so only the register finds the value
        self.material_regs = material_regs or {}
        self.sampler_fix = sampler_fix or {}        # sampler register -> swizzle applied to fetched values ('xxxw'), or
        #                                             a wrapping expression with {} ('xsrgb({})': an sRGB view's value re-encoded)
        self.colour_scale = colour_scale


def _f(v: float) -> str:
    if v != v:
        return '0.0'
    if v in (float('inf'), float('-inf')):
        return ('' if v > 0 else '-') + FLT_MAX
    r = repr(float(v))
    return r if ('e' in r or '.' in r) else r + '.0'


class _Emitter:
    samplers: dict[int, tuple[str, str, str]]   # sampler register -> (texture name, HLSL type, constant name)
    vfetch_element: dict[int, int]              # vfetch slot -> TEXCOORD usage index
    block_pred: bool | None
    pred_written: bool
    cube_dir_set: bool
    need_world: bool

    def __init__(self, sh: Obj, iface: Interface) -> None:
        self.sh, self.iface = sh, iface
        self.ps = sh.is_pixel
        self.lines: list[str] = []
        self.depth = 1
        self.cube_dir = None
        self.cmap: dict[int, str] = {}  # const register -> expression of a float4
        self.globals_: list[str] = []   # cbuffer declarations
        self.statics: list[str] = []
        self.textures: list[str] = []
        self.used_regs: set[int] = set()

    def w(self, s: str) -> None:
        self.lines.append('    ' * self.depth + s)

    # ---------------------------------------------------------------- declarations
    def constants(self) -> None:
        ct = self.sh.ctab.constants if self.sh.ctab else []
        declared: set[str] = set()
        for c in ct:
            if c.regset != 'float4':
                continue
            if c.name in ENGINE_SIZE:
                n = max(ENGINE_SIZE[c.name], c.count)
                inst = self.iface.instanced and c.name in INSTANCED
                if inst:
                    for need in ('worldMatrix', 'viewProjMatrix'):
                        if need not in declared:
                            self.globals_.append(f'float4 {need}[4];')
                            declared.add(need)
                elif c.name not in declared:
                    self.globals_.append(f'float4 {c.name}[{n}];')
                    declared.add(c.name)
                for k in range(c.count):
                    self.cmap[c.reg + k] = f'{INSTANCED[c.name]}[{k}]' if inst else f'{c.name}[{k}]'
            else:
                vals = self.iface.material.get(c.name)
                if vals is None and c.name == 'colorScale':
                    vals = [(self.iface.colour_scale,) * 4]
                regs = self.iface.material_regs
                if vals is None and c.name == 'constants' and any(c.reg + k in regs for k in range(c.count)):
                    # the block's values by register inside the array; registers no block value sets stay 0 (the
                    # engine's own: a tree's wind and fade)
                    vals = [tuple(regs.get(c.reg + k, (0.0, 0.0, 0.0, 0.0))) for k in range(c.count)]
                    unset = [k for k in range(c.count) if c.reg + k not in regs]
                    if unset:
                        self.statics.append(f'// material constant {c.name}: no value given for {unset}, 0')
                if vals is None:
                    vals = [(0.0, 0.0, 0.0, 0.0)] * c.count
                    self.statics.append(f'// material constant {c.name}: no value given, 0')
                if isinstance(vals[0], (int, float)):
                    vals = [tuple(vals)]
                for k in range(c.count):
                    v = vals[k] if k < len(vals) else (0.0,) * 4
                    nm = f'm_{c.name}_{k}'
                    self.statics.append(f'static const float4 {nm} = float4({", ".join(_f(x) for x in v)});')
                    self.cmap[c.reg + k] = nm
        for reg, v in sorted(self.sh.literals.items()):
            nm = f'lit{reg}'
            self.statics.append(f'static const float4 {nm} = float4({", ".join(_f(x) for x in v)});')
            self.cmap[reg] = nm
        self.samplers = {}
        for c in ct:
            if c.regset != 'sampler':
                continue
            kind = c.type.type
            name = c.name
            tex = 'texture_' + (name[len('sampler_'):] if name.startswith('sampler_') else name)
            typ = {'samplerCube': 'TextureCube', 'sampler3D': 'Texture3D', 'sampler1D': 'Texture1D'}.get(kind, 'Texture2D')
            self.textures.append(f'{typ} {tex} : register(t{c.reg});')
            self.samplers[c.reg] = (tex, typ, name)

    # ---------------------------------------------------------------- operands
    def const(self, idx: int, rel: str | None) -> str:
        if rel:
            if rel != 'a0':
                raise TranslateError(f'loop-relative constant addressing (c[{idx}+{rel}])')
            for c in (self.sh.ctab.constants if self.sh.ctab else []):
                if c.regset == 'float4' and c.reg <= idx < c.reg + c.count and c.name in ENGINE_SIZE:
                    if self.iface.bones_as_world and c.name in ('boneMatrices', 'boneVectors') and idx - c.reg < 3:
                        # BF1943 composite meshes place each part with its own 4x3 matrix; BFBC2's draw their parts
                        # with the object's world transform (boneMatrices only flags a part's visibility there)
                        self.need_world = True
                        return f'{"instW" if self.iface.instanced else "worldMatrix"}[{idx - c.reg}]'
                    return f'{c.name}[a0 + {idx - c.reg}]'
            raise TranslateError(f'relative constant addressing c[{idx}+a0] outside an engine array')
        return self.cmap.get(idx, 'float4(0.0, 0.0, 0.0, 0.0)')

    def reg(self, i: int) -> str:
        self.used_regs.add(i)
        return f'r{i}'

    def operand(self, o: Obj, swz: str | None = None) -> str:
        if o.file == 'temp':
            if o.rel:
                raise TranslateError('relative temp addressing is not supported')
            base = self.reg(o.index)
        else:
            base = self.const(o.index, o.rel)
        s = swz if swz is not None else o.swizzle
        e = f'{base}.{s}'
        if o.abs:
            e = f'abs({e})'
        if o.negate:
            e = f'(-{e})'
        return e

    # ---------------------------------------------------------------- ALU
    def vector(self, ins: Obj) -> str:
        op = ins.vector_op
        a = [self.operand(o) for o in ins.vector_operands]
        if op == 'add':
            return f'({a[0]} + {a[1]})'
        if op == 'mul':
            return f'xmul({a[0]}, {a[1]})'
        if op == 'max':
            return f'max({a[0]}, {a[1]})'
        if op == 'min':
            return f'min({a[0]}, {a[1]})'
        if op in ('seq', 'sgt', 'sge', 'sne'):
            cmp = {'seq': '==', 'sgt': '>', 'sge': '>=', 'sne': '!='}[op]
            return f'float4(({a[0]}) {cmp} ({a[1]}))'
        if op == 'frc':
            return f'frac({a[0]})'
        if op == 'trunc':
            return f'trunc({a[0]})'
        if op == 'floor':
            return f'floor({a[0]})'
        if op == 'mad':
            return f'(xmul({a[0]}, {a[1]}) + {a[2]})'
        if op in ('cndeq', 'cndge', 'cndgt'):
            cmp = {'cndeq': '==', 'cndge': '>=', 'cndgt': '>'}[op]
            return f'xsel(({a[0]}) {cmp} 0.0, {a[1]}, {a[2]})'
        if op == 'dp4':
            return f'xdot4({a[0]}, {a[1]}).xxxx'
        if op == 'dp3':
            return f'xdot4(float4(({a[0]}).xyz, 0.0), float4(({a[1]}).xyz, 0.0)).xxxx'
        if op == 'dp2add':
            return f'(xdot4(float4(({a[0]}).xy, 0.0, 0.0), float4(({a[1]}).xy, 0.0, 0.0)) + ({a[2]}).x).xxxx'
        if op == 'cube':
            # src0 = d.zzxy, src1 = d.yxzz: the direction is (src0.z, src0.w, src0.x)
            self.w(f'cubeDir = float3(({a[0]}).z, ({a[0]}).w, ({a[0]}).x);')
            return 'xcube(cubeDir)'
        if op == 'max4':
            return f'xmax4({a[0]})'
        if op.startswith('setp_') and op.endswith('_push'):
            cmp = {'eq': '==', 'ne': '!=', 'gt': '>', 'ge': '>='}[op[5:7]]
            self.w(f'p0_next = (({a[0]}).w == 0.0 && ({a[1]}).w {cmp} 0.0);')
            self.pred_written = True
            return f'((({a[0]}).x == 0.0 && ({a[1]}).x {cmp} 0.0) ? 0.0 : ({a[0]}).x + 1.0).xxxx'
        if op.startswith('kill_'):
            cmp = {'eq': '==', 'gt': '>', 'ge': '>=', 'ne': '!='}[op[5:]]
            self.w(f'if (any(({a[0]}) {cmp} ({a[1]}))) discard;')
            return f'float4(any(({a[0]}) {cmp} ({a[1]})).xxxx)'
        if op == 'dst':
            return f'float4(1.0, ({a[0]}).y * ({a[1]}).y, ({a[0]}).z, ({a[1]}).w)'
        if op == 'maxa':
            self.w(f'a0 = (int)clamp(floor(({a[0]}).w + 0.5), -256.0, 255.0);')
            return f'max({a[0]}, {a[1]})'
        raise TranslateError(f'vector op {op}')

    def scalar(self, ins: Obj) -> str:
        op = ins.scalar_op
        ops = ins.scalar_operands
        if op == 'retain_prev':
            return 'ps'
        if not ops:
            if op == 'setp_clr':
                self.w('p0_next = false;')
                self.pred_written = True
                return FLT_MAX
            raise TranslateError(f'scalar op {op}')
        b: str | None                         # the second operand, none for a single-component one
        if len(ops) == 2:                     # mulsc / addsc / subsc: c[].a (op) r[].b
            a = self.operand(ops[0], ops[0].components)
            b = self.operand(ops[1], ops[1].components)
        else:
            o = ops[0]
            a = self.operand(o, o.components[0])
            b = self.operand(o, o.components[1]) if len(o.components) > 1 else None
        if op in ('adds', 'addsc'):
            return f'({a} + {b})'
        if op == 'adds_prev':
            return f'({a} + ps)'
        if op in ('muls', 'mulsc'):
            return f'xmul1({a}, {b})'
        if op == 'muls_prev':
            return f'xmul1({a}, ps)'
        if op == 'muls_prev2':
            return f'((ps == -{FLT_MAX} || !isfinite(ps) || !isfinite({b}) || {b} <= 0.0) ? -{FLT_MAX} : xmul1({a}, ps))'
        if op == 'maxs':
            return f'max({a}, {b})'
        if op == 'mins':
            return f'min({a}, {b})'
        if op in ('seqs', 'sgts', 'sges', 'snes'):
            cmp = {'seqs': '==', 'sgts': '>', 'sges': '>=', 'snes': '!='}[op]
            return f'(({a}) {cmp} 0.0 ? 1.0 : 0.0)'
        if op == 'frcs':
            return f'frac({a})'
        if op == 'truncs':
            return f'trunc({a})'
        if op == 'floors':
            return f'floor({a})'
        if op == 'exp':
            return f'exp2({a})'
        if op in ('log', 'logc'):
            return f'xlog({a})' if op == 'logc' else f'(({a}) == 1.0 ? 0.0 : log2({a}))'
        if op in ('rcp', 'rcpc', 'rcpf'):
            return f'xrcp{op[3:]}({a})'
        if op in ('rsq', 'rsqc', 'rsqf'):
            return f'xrsq{op[3:]}({a})'
        if op in ('maxas', 'maxasf'):
            rnd = ' + 0.5' if op == 'maxas' else ''
            self.w(f'a0 = (int)clamp(floor({a}{rnd}), -256.0, 255.0);')
            return f'max({a}, {b})'
        if op in ('subs', 'subsc'):
            return f'({a} - {b})'
        if op == 'subs_prev':
            return f'({a} - ps)'
        if op in ('setp_eq', 'setp_ne', 'setp_gt', 'setp_ge'):
            cmp = {'eq': '==', 'ne': '!=', 'gt': '>', 'ge': '>='}[op[5:]]
            self.w(f'p0_next = ({a}) {cmp} 0.0;')
            self.pred_written = True
            return f'(({a}) {cmp} 0.0 ? 0.0 : 1.0)'
        if op == 'setp_inv':
            self.w(f'p0_next = ({a}) == 1.0;')
            self.pred_written = True
            return f'(({a}) == 1.0 ? 0.0 : (({a}) == 0.0 ? 1.0 : ({a})))'
        if op == 'setp_pop':
            self.w(f'p0_next = ({a}) - 1.0 <= 0.0;')
            self.pred_written = True
            return f'(({a}) - 1.0 <= 0.0 ? 0.0 : ({a}) - 1.0)'
        if op == 'setp_rstr':
            self.w(f'p0_next = ({a}) == 0.0;')
            self.pred_written = True
            return f'(({a}) == 0.0 ? 0.0 : ({a}))'
        if op.startswith('kills_'):
            cmp = {'eq': '==', 'gt': '>', 'ge': '>=', 'ne': '!=', 'one': '=='}[op[6:]]
            rhs = '1.0' if op == 'kills_one' else '0.0'
            self.w(f'if (({a}) {cmp} {rhs}) discard;')
            return f'(({a}) {cmp} {rhs} ? 1.0 : 0.0)'
        if op == 'sqrt':
            return f'sqrt({a})'
        if op == 'sin':
            return f'sin({a})'
        if op == 'cos':
            return f'cos({a})'
        raise TranslateError(f'scalar op {op}')

    def write(self, dest: Obj, value: str, comps_from: object, is_vector: bool) -> None:
        """dest.write: per component 'x'..'w' (take that component of value), '0', '1', '_'."""
        if dest.file == 'temp':
            if dest.rel:
                raise TranslateError('relative destination')
            target = self.reg(dest.index)
        else:
            target = self.out_name(dest)
        for c, ch in zip('xyzw', dest.write):
            if ch == '_':
                continue
            if ch in '01':
                self.w(f'{target}.{c} = {ch}.0;')
                continue
            src = f'{value}.{c}' if is_vector else value
            if dest.saturate:
                src = f'saturate({src})'
            self.w(f'{target}.{c} = {src};')

    def out_name(self, dest: Obj) -> str:
        if dest.file == 'oPos':
            return 'o.pos'
        if dest.file == 'o':
            return f'o.t{dest.index}'
        if dest.file == 'oC':
            return f'oC{dest.index}'
        raise TranslateError(f'export to {dest.file}')

    def alu(self, ins: Obj) -> None:
        self.pred_written = False
        k = ins.address
        vec = None if ins.vector_nop else self.vector(ins)
        sca = None if ins.scalar_nop else self.scalar(ins)
        if vec is not None:
            self.w(f'float4 v{k} = {vec};')
            if self.iface.trace_nan:
                self.w(f'if (firstNan < 0.0 && any(isnan(v{k}) || isinf(v{k}))) firstNan = {k}.0;')
        if sca is not None:
            self.w(f'float s{k} = {sca};')
            if self.iface.trace_nan:
                self.w(f'if (firstNan < 0.0 && (isnan(s{k}) || isinf(s{k}))) firstNan = {k}.5;')
        if vec is not None:
            self.write(ins.vector_dest, f'v{k}', None, True)
        if sca is not None:
            self.write(ins.scalar_dest, f's{k}', None, False)
            self.w(f'ps = s{k};')
        if self.pred_written:
            self.w('p0 = p0_next;')

    # ---------------------------------------------------------------- fetch
    def fetch(self, ins: Obj) -> None:
        if ins.fetch_kind == 'vertex':
            el = self.vfetch_element.get(ins.address)
            if el is None:
                raise TranslateError(f'vfetch at {ins.address} has no vertex element')
            val = f'float4(vin.t{el})'
            self.fetch_write(ins, val)
            return
        op = ins.op
        if op == 'setTexLOD':
            self.w(f'texLod = {self.reg(ins.src)}.{ins.src_swizzle[0]};')
            return
        if not ins.has_dest:
            return
        reg = ins.const_index
        if reg not in self.samplers:
            raise TranslateError(f'fetch from tf{reg}: no sampler s{reg} in the constant table')
        tex, typ, name = self.samplers[reg]
        src = f'{self.reg(ins.src)}.{ins.src_swizzle}'
        off = (ins.offset_x, ins.offset_y)
        if op.startswith('getWeights'):
            self.w(f'float4 f{ins.address} = float4(frac({src} * tdim({tex}) - 0.5 + float2({_f(off[0])}, {_f(off[1])})), 0.0, 0.0);')
            self.fetch_write(ins, f'f{ins.address}')
            return
        if not op.startswith('tfetch'):
            raise TranslateError(f'fetch op {op}')
        if typ == 'TextureCube':
            coord = 'cubeDir' if self.cube_dir_set else src
            e = f'{tex}.Sample(sampler1, {coord})'
        elif name == SHADOW or (ins.mag_filter == 'point' and ins.min_filter == 'point'):
            # point taps: exact texel loads (the shadow map is a depth texture: raw depth)
            e = f'{tex}.Load(int3(floor({src} * tdim({tex}) + float2({_f(off[0])}, {_f(off[1])})), 0))'
        else:
            coord = src
            if off != (0.0, 0.0):
                coord = f'({src} + float2({_f(off[0])}, {_f(off[1])}) / tdim({tex}))'
            if ins.use_register_lod:
                e = f'{tex}.SampleLevel(sampler1, {coord}, texLod)'
            elif ins.lod_bias:
                e = f'{tex}.SampleBias(sampler1, {coord}, {_f(ins.lod_bias)})'
            else:
                e = f'{tex}.Sample(sampler1, {coord})'
        fix = self.iface.sampler_fix.get(reg)
        if fix and '{}' in fix:                    # a wrapping expression, e.g. 'xsrgb({})'
            e = fix.format(e)
        elif fix:
            e = f'({e}).{fix}'
        self.w(f'float4 f{ins.address} = {e};')
        self.fetch_write(ins, f'f{ins.address}')

    def fetch_write(self, ins: Obj, val: str) -> None:
        target = self.reg(ins.dst)
        for c, ch in zip('xyzw', ins.dst_swizzle):
            if ch in '_?':
                continue
            if ch in '01':
                self.w(f'{target}.{c} = {ch}.0;')
            else:
                self.w(f'{target}.{c} = {val}.{ch};')

    # ---------------------------------------------------------------- control flow
    def instr(self, ins: Obj) -> None:
        pred = ins.predicated and not self.block_pred_covers(ins)
        if pred:
            self.w(f'if (p0 == {"true" if ins.pred_condition else "false"}) {{')
            self.depth += 1
        if ins.kind == 'alu':
            self.alu(ins)
        else:
            self.fetch(ins)
        if pred:
            self.depth -= 1
            self.w('}')

    def block_pred_covers(self, ins: Obj) -> bool:
        return self.block_pred is not None and self.block_pred == ins.pred_condition

    def cfs(self, lst: list[Obj], start: int, end: int) -> None:
        i = start
        while i < end:
            cf = lst[i]
            if cf.op in ('exec', 'exec_end', 'cond_exec_pred', 'cond_exec_pred_end'):
                if cf.op.startswith('cond_exec_pred'):
                    self.w(f'if (p0 == {"true" if cf.condition else "false"}) {{')
                    self.depth += 1
                    self.block_pred = bool(cf.condition)
                else:
                    self.block_pred = None
                for ins in cf.instructions:
                    self.instr(ins)
                if cf.op.startswith('cond_exec_pred'):
                    self.depth -= 1
                    self.w('}')
                self.block_pred = None
                if cf.is_end and i != len(lst) - 1 and any(c.op != 'alloc' for c in lst[i + 1:]):
                    raise TranslateError('exec_end before the last CF')
                i += 1
            elif cf.op == 'alloc' or cf.op == 'nop':
                i += 1
            elif cf.op == 'cond_jmp':
                if cf.type == 'unconditional':
                    cond = 'false'
                elif cf.type == 'predicated':
                    cond = f'p0 != {"true" if cf.condition else "false"}'
                else:
                    raise TranslateError('bool-constant jump')
                t = next((j for j, c in enumerate(lst) if c.index == cf.address), None)
                if t is None or t <= i or t > end:
                    raise TranslateError(f'jump to cf {cf.address} is not a nested forward jump')
                self.w(f'if ({cond}) {{')
                self.depth += 1
                self.cfs(lst, i + 1, t)
                self.depth -= 1
                self.w('}')
                i = t
            else:
                raise TranslateError(f'control flow {cf.op} is not supported')

    # ---------------------------------------------------------------- whole shader
    def run(self) -> str:
        sh = self.sh
        self.block_pred = None
        self.cube_dir_set = any(i.kind == 'alu' and i.vector_op == 'cube' and not i.vector_nop
                                for i in sh.instructions)
        self.constants()
        h = sh.header
        self.vfetch_element = {}
        if not self.ps:
            for el in h.vertex_elements:
                if el.usage != 'TEXCOORD':
                    raise TranslateError(f'vertex input usage {el.usage}')
                self.vfetch_element[el.fetch_address] = el.usage_index
        body_start = len(self.lines)
        self.cfs(sh.cf, 0, len(sh.cf))
        body = self.lines[body_start:]
        del self.lines[body_start:]
        return self.assemble(body)

    def assemble(self, body: list[str]) -> str:
        sh, iface = self.sh, self.iface
        out = ['// translated from Xbox 360 (Xenos) microcode', '#define xsel(c, a, b) ((c) ? (a) : (b))']
        out += HELPERS
        if getattr(self, 'need_world', False) or (not self.ps and iface.instanced):
            for need in ('worldMatrix', 'viewProjMatrix'):
                if not any(g.startswith(f'float4 {need}[') for g in self.globals_):
                    self.globals_.append(f'float4 {need}[4];')
        out += self.globals_
        out += self.statics + self.textures
        out.append('SamplerState sampler1 : register(s1);')
        inter = iface.interpolators
        out.append('struct V2P {')
        out.append('    float4 pos : SV_Position;')
        out += [f'    float4 t{k} : TEXCOORD{k};' for k in inter]
        out.append('};')
        n = sh.header.num_temp_regs
        regs = sorted(self.used_regs | set(range(n)))
        decl = ['    float4 ' + ', '.join(f'r{i} = 0.0' for i in regs) + ';',
                '    float ps = 0.0; bool p0 = false; bool p0_next = false; int a0 = 0; float texLod = 0.0;',
                '    float firstNan = -1.0;',
                '    float3 cubeDir = float3(0.0, 0.0, 1.0);']
        if self.ps:
            outs = [k for k in range(4) if sh.header.outputs >> k & 1]
            out.append('struct PSOut {' + ''.join(f' float4 c{k} : SV_Target{k};' for k in outs) + ' };')
            out.append('PSOut main(V2P i, bool frontFace : SV_IsFrontFace) {')
            out += decl
            out += [f'    float4 oC{k} = 0.0;' for k in outs]
            for it in sh.header.interpolators:
                if it.usage_index not in inter:
                    raise TranslateError(f'PS reads TEXCOORD{it.usage_index}, the VS does not write it')
                out.append(f'    r{it.reg} = i.t{it.usage_index};')
            if sh.header.context_misc_fields.sc_output_screen_xy:
                # PsParamGen (Xenia's dxbc_shader_translator): XY the pixel's integer position, X's sign bit set on
                # back faces; ZW point-sprite coordinates (0: not a point)
                n = sh.header.context_misc_fields.param_gen_pos
                out.append(f'    r{n} = float4(floor(i.pos.xy), 0.0, 0.0);')
                out.append(f'    if (!frontFace) r{n}.x = asfloat(asuint(r{n}.x) | 0x80000000u);')
            out += body
            out.append('    PSOut po;')
            if iface.trace_nan:
                out.append('    oC0 = float4(firstNan, 0.0, 0.0, 1.0);   // debug: first instruction giving NaN / inf')
            if iface.probe:
                # debug: the engine's constants as bit bars (white = 1) - read back from a screenshot, exact whatever
                # the tone mapping does to black and very bright white
                out.append(f'    float probeVals[{len(iface.probe)}] = {{ {", ".join(iface.probe)} }};')
                out.append(f'    int2 pc = int2((i.pos.xy - float2({PROBE_X0}, {PROBE_Y0})) / float2({PROBE_W}, {PROBE_H}));')
                out.append(f'    if (pc.x >= 0 && pc.x < 32 && pc.y >= 0 && pc.y < {len(iface.probe)})')
                out.append('        oC0 = ((asuint(probeVals[pc.y]) >> (31 - pc.x)) & 1) ? float4(2.0, 0.0, 2.0, 1.0) '
                           ': float4(0.0, 2.0, 0.0, 1.0);')
            out += [f'    po.c{k} = oC{k};' for k in outs]
            out.append('    return po;')
        else:
            out.append('struct VIn {')
            for k, typ in sorted(iface.vs_inputs.items()):
                out.append(f'    {typ} t{k} : TEXCOORD{k};')
            if iface.instanced:
                out += [f'    float4 inst{j} : TEXCOORD{s};' for j, s in enumerate(INSTANCE_SEMANTICS)]
            out.append('};')
            out.append('V2P main(VIn vin) {')
            out += decl
            if iface.instanced:
                out.append(INSTANCE_PRELUDE)
            out.append('    V2P o = (V2P)0;')
            for it in sh.header.interpolators:
                if it.usage_index not in inter:
                    raise TranslateError(f'VS writes TEXCOORD{it.usage_index}, not in the interface')
            out += body
            out.append('    return o;')
        out.append('}')
        return '\n'.join(out) + '\n'


HELPERS = r'''
float4 xcube(float3 d) {
    float3 a = abs(d);
    if (a.z >= a.x && a.z >= a.y) return float4(-d.y, d.z < 0.0 ? -d.x : d.x, 2.0 * d.z, d.z < 0.0 ? 5.0 : 4.0);
    if (a.y >= a.x) return float4(d.y < 0.0 ? -d.z : d.z, d.x, 2.0 * d.y, d.y < 0.0 ? 3.0 : 2.0);
    return float4(-d.y, d.x < 0.0 ? d.z : -d.z, 2.0 * d.x, d.x < 0.0 ? 1.0 : 0.0);
}
// Xenos multiplies as Direct3D 9 does: 0 * anything = 0 (also 0 * inf, 0 * NaN), unlike IEEE (Xenia's
// "legacy multiply"); shaders rely on it when they normalise zero vectors
float4 xmul(float4 a, float4 b) { return (a == 0.0 || b == 0.0) ? 0.0 : a * b; }
float xmul1(float a, float b) { return (a == 0.0 || b == 0.0) ? 0.0 : a * b; }
float xdot4(float4 a, float4 b) { float4 m = xmul(a, b); return m.x + m.y + m.z + m.w; }
float xmax4(float4 v) { return max(max(v.x, v.y), max(v.z, v.w)); }
float xlog(float a) { float t = a == 1.0 ? 0.0 : log2(a); return isinf(t) && t < 0.0 ? -3.402823466e+38 : t; }
float xrcp(float a) { return a == 1.0 ? 1.0 : 1.0 / a; }
float xrcpc(float a) { float t = xrcp(a); return isinf(t) ? (t < 0.0 ? -3.402823466e+38 : 3.402823466e+38) : t; }
float xrcpf(float a) { float t = xrcp(a); return isinf(t) ? (t < 0.0 ? -0.0 : 0.0) : t; }
float xrsq(float a) { return a == 1.0 ? 1.0 : rsqrt(a); }
float xrsqc(float a) { float t = xrsq(a); return isinf(t) ? (t < 0.0 ? -3.402823466e+38 : 3.402823466e+38) : t; }
float xrsqf(float a) { float t = xrsq(a); return isinf(t) ? (t < 0.0 ? -0.0 : 0.0) : t; }
float2 tdim(Texture2D t) { uint w, h; t.GetDimensions(w, h); return float2(w, h); }
float4 xsrgb(float4 c) { float3 l = saturate(c.rgb); return float4(l <= 0.0031308 ? l * 12.92 : 1.055 * pow(l, 1.0 / 2.4) - 0.055, c.a); }
'''.strip('\n').split('\n')


def translate(sh: Obj, iface: Interface) -> str:
    return _Emitter(sh, iface).run()
