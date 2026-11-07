# Xenos shader container + microcode, as implemented in `xenos.py`

Sources: Xenia `src/xenia/gpu/ucode.h`, `ucode.cc`, `shader_translator.cc`, `shader_translator_disasm.cc`
(BSD-3, reference copies in `xenia_ref/`), hedge-dev XenosRecomp `shader.h` (MIT, field names for the container,
copy in `xenosrecomp_ref/`), and the 735 VS + 815 PS of BF1943 Wake `Single.res` (every claim below marked
"verified" holds for all of them; `run_all.py` re-checks it, results in `summary.txt`).
**UNCERTAIN** marks anything not proven.

All words are big-endian u32. "Slot" = one 3-dword (12-byte) microcode unit.

## 1. Container

```
0x00 magic        0x102A1100 pixel, 0x102A1101 vertex
0x04 virtual_size   header part (everything before the microcode section)
0x08 physical_size  microcode section; virtual_size + physical_size == blob size (verified)
0x0C 0
0x10 ctab_offset    always 0x24
0x14 def_offset     definition (literal constant) table, 0 if none (43 VS, 24 PS have none)
0x18 header_offset  shader header
0x1C 0, 0x20 0
```
CTAB container, def table and header tile `[0x24, virtual_size)` exactly, no gaps (verified).

### 1.1 Constant table (`ctab_offset`)
`u32 size`, then a standard D3DXSHADER_CONSTANTTABLE (big-endian) at `ctab_offset+4`; every offset inside is
relative to that base: `{Size=0x1c, Creator, Version (fffe0300 vs_3_0 / ffff0300 ps_3_0), Constants, ConstantInfo,
Flags, Target}`. ConstantInfo = 20 bytes `{Name, u16 RegisterSet (0 bool,1 int4,2 float4,3 sampler), u16 RegisterIndex,
u16 RegisterCount, u16 Reserved, TypeInfo, DefaultValue}`; TypeInfo `{u16 Class, u16 Type, u16 Rows, u16 Columns,
u16 Elements, u16 StructMembers, u32 StructMemberInfo}`. Creator "2.0.7978.3". Registers are D3D numbers:
float4 c#, sampler s#.

### 1.2 Definition table (`def_offset`) = `def` literals
```
+0x00 0
+0x04 block mask   bit k set = 4-register block c[252-4k .. 255-4k] holds literals (inferred: VS 1/3, PS 2/6/14/30/62/126;
                   PS never uses c252-255). Consistent on all, meaning inferred.
+0x08 0, +0x0C 0
+0x10 size         bytes of the list below
+0x14 float defs:  { u16 reg, u16 count (floats), u32 physical_offset } ... , u32 0
      int defs:    { u16 reg, u16 count, count x u32 } ... , u32 0      (format from XenosRecomp; i# = (reg-8992)/4,
                                                                        4 x int8. UNCERTAIN, none in BF1943)
      bool defs:   ... u32 0                                             (UNCERTAIN, none in BF1943)
```
Float values live at `virtual_size + physical_offset` (= start of the physical section, before the microcode).
`reg` is the GPU constant index: VS c0..255 = reg, **PS reg - 256** (PS file is c256..511 on the GPU; the
microcode itself uses 0..255). BF1943 always has exactly one float def, count a multiple of 4 (4..24 vec4),
ending at c255 (VS) or c251 (PS). Many of the literal vec4s are all-zero padding never referenced by code
(but some all-zero ones are referenced, so treat all as real `def`s).
Cross-check: every non-relative ALU constant operand is inside a CTAB float4 range or a literal register (verified).

### 1.3 Shader header (`header_offset`)
```
+0x00 physical_offset   microcode start inside the physical section (= 16 x literal vec4 count); code ends the blob
+0x04 physical_size     microcode bytes
+0x08 program_cntl      SQ_PROGRAM_CNTL-like: VS bits 0-5 = temps-1, PS bits 8-13 = temps-1 (verified >= max temp used;
                        equal for all VS and 739 PS); bit16 VS_RESOURCE (VS, 725/735), bit18 PARAM_GEN (PS: set exactly
                        when SC_OUTPUT_SCREEN_XY is, verified), bits 20-23 VS_EXPORT_COUNT = interpolators-1 (0 if none),
                        bits 27-30 PS_EXPORT_MODE = 2 x colour outputs (2/4/6 seen). Names from the R400/Xenos register;
                        semantics beyond the counts UNCERTAIN.
+0x0C context_misc      SQ_CONTEXT_MISC-like: PS bit1 SC_OUTPUT_SCREEN_XY, bits 2-3 SC_SAMPLE_CNTL, bits 8-15
                        PARAM_GEN_POS = temp register receiving vPos (always == interpolator count, i.e. right after the
                        interpolators). 0 for VS.
+0x10 0
+0x14 interpolator_info bits 0-4, 5-9 and 12-15 all = interpolator count (n = bits 5-9, as XenosRecomp)
PS: +0x18 interp_mask   ((1<<n)-1) * 0x10001 (verified)
    +0x1C outputs       bit0..3 = oC0..oC3, bit4 = oDepth (verified == colour exports in the code)
    +0x20 n x interpolator word
VS: +0x18 field18       always 1 = number of following words; +0x1C vertex element count m; +0x20 e = export-slot count
    +0x24 field18 words (always 0x290, UNKNOWN), then m x vertex element word, n x interpolator word,
    e x export-slot word
```
- Vertex element word: bits 0-11 = slot of the `vfetch` that loads it, 12-15 = D3DDECLUSAGE, 16-19 = usage index,
  20-23 = UNKNOWN (0..3; not stream, format, dst swizzle or src register).
- Interpolator word: bits 0-3 usage index, 4-7 usage (always TEXCOORD in BF1943), 8-11 register (o# for VS,
  r# for PS; always == position in the list), 12-15 always 0xF (probably a component mask, UNCERTAIN),
  VS bits 16-23 = index into the export-slot list of this interpolator's first export instruction.
- Export-slot word (VS): bits 0-11 = slot of an ALU instruction exporting to o#, bit 12 = last export instruction
  of its interpolator. The list holds exactly the o# export instructions (verified). Probably what D3D uses to
  patch out interpolators the bound PS does not read (UNCERTAIN).

## 2. Microcode (`code = blob[virtual_size + physical_offset :][:physical_size]`)

Control-flow (CF) instructions come first, packed two per slot (48 bits each):
`cf0 = (d0, d1 & 0xFFFF)`, `cf1 = ((d1 >> 16) | (d2 << 16), d2 >> 16)`. The CF area ends at the lowest exec
address (Xenia/freedreno rule); trailing `nop` CFs are dropped. Exec addresses are slot indices from the code start.
Every BF1943 shader ends with exactly one all-zero padding slot not reached by any exec (verified).

### 2.1 CF instruction (w0 = low 32 bits, w1 = high 16 bits; w1 bits 12-15 opcode, bit 11 address mode)
| opcode | name | fields |
|---|---|---|
| 0 | nop | |
| 1/2 | exec / exec_end | w0: 0-11 address, 12-14 count, 15 yield, 16-27 sequence (2 bits/instr: bit0 1 = fetch, 0 = ALU; bit1 serialize), 28-31 vc_hi; w1: 0-1 vc_lo, 9 predicate_clean |
| 3/4 | cond_exec(_end) | as exec; w1: 2-9 bool constant, 10 condition |
| 5/6 | cond_exec_pred(_end) | as exec; w1: 9 predicate_clean, 10 condition (exec only if p0 == condition) |
| 7 | loop_start | w0: 0-12 skip address, 13 repeat, 16-20 loop (int) constant |
| 8 | loop_end | w0: 0-12 body address, 16-20 loop constant, 21 predicated break; w1: 10 condition |
| 9 | cond_call | w0: 0-12 target, 13 unconditional, 14 predicated; w1: 2-9 bool constant, 10 condition |
| 10 | return | |
| 11 | cond_jmp | as cond_call, plus w1 bit 1 direction |
| 12 | alloc | w0: 0-2 size; w1: 8 unserialized, 9-10 type (1 position, 2 interpolators/colors, 3 memexport) |
| 13/14 | cond_exec_pred_clean(_end) | as cond_exec (bool constant) |
| 15 | mark_vs_fetch_done | |

CF targets (jmp/call/loop) are CF indices (`2*slot + half`). **BF1943 uses only**: exec, exec_end,
cond_exec_pred (predicated execs), cond_jmp (always predicated: `(!p0) jmp L`), alloc. **No loops, calls, returns,
bool-constant conditionals or memexport.** Predication is two-level: a predicated exec skips its whole block
unless p0 == condition, and each fetch/ALU instruction has its own predicate bits (17,287 of the 17,302
instructions inside predicated execs repeat the exec's predicate, 15 do not; 3,286 predicated instructions sit in
plain execs). A translator must honour both. p0 is set by setp_* scalar / setp_*_push vector ops.

### 2.2 ALU instruction (3 dwords; vector op + scalar op co-issued)
```
d0: 0-5 vector_dest, 6 vector_dest_rel(aL), 7 abs_constants, 8-13 scalar_dest, 14 scalar_dest_rel,
    15 export, 16-19 vector_write_mask, 20-23 scalar_write_mask, 24 vector_saturate, 25 scalar_saturate,
    26-31 scalar_opcode
d1: 0-7 src3_swizzle, 8-15 src2_swizzle, 16-23 src1_swizzle, 24 src3_neg, 25 src2_neg, 26 src1_neg,
    27 pred_condition, 28 predicated, 29 const relative to a0 (else aL), 30 const_1_rel, 31 const_0_rel
d2: 0-7 src3_reg, 8-15 src2_reg, 16-23 src1_reg, 24-28 vector_opcode, 29 src3_is_temp, 30 src2_is_temp,
    31 src1_is_temp
```
- Source register byte: temp: bits 0-5 index, 6 aL-relative, 7 absolute value; const: 8-bit index (0..255,
  PS too), absolute value from `abs_constants`. Constant relative addressing: operand 1 uses const_0_rel, operand 2
  uses const_0_rel if operand 1 is a temp else const_1_rel, operand 3 const_0_rel if 1 and 2 are temps else
  const_1_rel; bit 29 picks a0 vs aL. BF1943 VS use `c[n+a0]` (skinning / instancing arrays, 4,758 operands).
- Swizzles are component-relative: component i reads `((swz >> 2i) + i) & 3`, so 0 = .xyzw.
- Vector ops (5 bits): add mul max min seq sgt sge sne frc trunc floor mad cndeq cndge cndgt dp4 dp3 dp2add cube
  max4 setp_{eq,ne,gt,ge}_push kill_{eq,gt,ge,ne} dst maxa (operands 1..3 = src1..src3). `max rX, a, a` is `mov`.
- Scalar ops (6 bits) read src3: one-operand ops use component W of the swizzled src3 (`a`); two-component ops
  (adds muls muls_prev2 maxs mins maxas maxasf subs) use W then X (`a`, `b`). `*_prev` ops use the previous scalar
  result `ps`. mulsc/addsc/subsc (42-47) = `c[src3_reg].a op r[t].b` with
  t = (opcode & 1) | (src3_is_temp << 1) | (src3_swizzle & 0x3C), const component from swizzle W, temp from X.
  `retain_prev` (50) with empty mask = no scalar op. `maxs r0._, rX.xx` = load `ps` only.
- Non-export: vector result -> r[vector_dest].mask, scalar result (replicated) -> r[scalar_dest].mask.
- Export (bit 15): both results go to export register `vector_dest`; per component: vector mask only -> vector
  result, scalar mask only -> scalar result, both -> constant 1, neither -> unchanged, or constant 0 if
  scalar_dest_rel (bit 14) is set. Disassembly shows constants as `0`/`1` in the write mask (`oC0.xyz1`,
  `o0.x000`).

Export registers (`vector_dest` when exporting):

| index | VS | PS |
|---|---|---|
| 0-15 | o0..o15 interpolators (BF1943: up to o10) | 0-3 oC0..oC3 (BF1943: oC0..oC2) |
| 32 / 33-37 | eA / eM0-4 memexport (unused) | same |
| 61 | - | oDepth (unused in BF1943) |
| 62 | oPos | - |
| 63 | oPts (x point size, y edge flag, z kill vertex; unused) | - |

### 2.3 Fetch instructions (3 dwords; opcode d0 bits 0-4)
Vertex fetch (opcode 0):
```
d0: 5-10 src, 11 src_rel, 12-17 dst, 18 dst_rel, 19 must_be_one, 20-24 const_index, 25-26 const_index_sel,
    27-29 prefetch_count, 30-31 src_swizzle (component of src)
d1: 0-11 dst_swizzle (3 bits/comp: 0-3 xyzw, 4 = 0, 5 = 1, 7 = keep), 12 signed, 13 integer (not normalized),
    14 signed_rf_mode, 15 round_index, 16-21 format, 24-29 exp_adjust (signed), 30 mini, 31 predicated
d2: 0-7 stride (dwords), 8-30 offset (dwords, signed), 31 pred_condition
```
fetch constant = const_index*3 + sel; Xenia's disassembly prints `vf(95 - that)`.
Texture fetch (opcode 1 tfetch, 16 getBCF, 17 getCompTexLOD, 18 getGradients, 19 getWeights, 24 setTexLOD,
25/26 setGradientH/V):
```
d0: 5-10 src, 11 src_rel, 12-17 dst, 18 dst_rel, 19 fetch_valid_only, 20-24 const_index (tf#),
    25 unnormalized coords, 26-31 src_swizzle (2 bits/comp, absolute, 1-3 comps by dimension)
d1: 0-11 dst_swizzle, 12-13 mag, 14-15 min, 16-17 mip filter (0 point 1 linear 2 basemap 3 from fetch const),
    18-20 aniso (7 = from fetch const), 21-23 arbitrary filter, 24-25 vol_mag, 26-27 vol_min, 28 use_computed_lod,
    29 use_register_lod, 30 unknown, 31 predicated
d2: 0 use_register_gradients, 1 sample_location (0 centroid, 1 center), 2-8 lod_bias (signed /16), 14-15 dimension
    (1D 2D 3D/stacked Cube), 16-20/21-25/26-30 offset x/y/z (signed /2), 31 pred_condition
```
BF1943 PS use tfetch2D, tfetchCube, getWeights2D (bilinear PCF weights on the shadow map), setTexLOD
(+ UseRegisterLOD fetches). **PS `tfN` == CTAB sampler `sN`** and the dimension matches the sampler type (verified
for all 8,953 PS fetches that take a fetch constant). VS texture fetch: 5 VS use `tf22` while their only sampler is CTAB `s0`
(`sampler_heightmap`) - the VS sampler -> fetch constant mapping is **UNCERTAIN** (D3D-side offset, maybe 22+s).

## 3. Shader interface

### VS inputs (vertex fetch)
- On entry r0.x holds the vertex index (all vfetches use src r0, normally `.x`; 41 use `.y`/`.w` - UNCERTAIN why, maybe also template garbage).
- **The vfetch instructions are unpatched templates**: every one has fetch constant 95 (`vf0`), format 0, stride 0,
  offset 0, no sign/integer flags (verified, 3,602 fetches); only dst register/swizzle are real. D3D patches them
  at bind time from the vertex declaration. So: header vertex element (slot, usage, index) -> find the same
  (usage, index) in `db.vs[i].tail` (Xenon D3DVERTEXELEMENT9 list: stream, offset, type) for the stream, byte
  offset and data type, and the stream stride from the tail's per-stream stride bytes. Every header element is
  found in the tail decl and every vfetch is listed by exactly one header element (verified). The vfetch dst
  swizzle (e.g. `r0.xyz1`, `r4.__xy`) says which components land where and what is defaulted to 0/1.
- BF1943 declares all VS inputs as TEXCOORDn (usage 5), including positions.

### VS outputs
- `alloc position` then exports to oPos (always, 735/735), then `alloc interpolators` and exports to o0..o(n-1);
  the header interpolator list gives each o#'s semantic (TEXCOORDn, n = position in BF1943) and the exporting
  instructions. All declared o# are written (verified). Exports may be split over several instructions
  (`o0.x000` then `o0._y__` ...): the first write with bit 14 zero-fills the rest.

### PS inputs/outputs
- Interpolator k (header list, TEXCOORDn) arrives pre-loaded in **r[k]** (reg field == k, verified); the VS o# with
  the same usage/index feeds it. If SC_OUTPUT_SCREEN_XY is set, r[n] (n = interpolator count, = PARAM_GEN_POS)
  receives the param-gen vPos/vFace value (XenosRecomp: xy = screen position - 0.5, sign of x = face; UNCERTAIN in
  detail). Other temps start undefined (treat as 0).
- `alloc colors`, then exports to oC0..oC3 per the outputs mask; `oC0.xyz1`-style constant components are common.

## 4. What is not yet resolved
- Vertex element bits 20-23; VS field18 word 0x290; interpolator bits 12-15 (always 0xF).
- int/bool literal (`defi`/`defb`) encoding (absent from BF1943); loop/call semantics are decoded per Xenia but
  never exercised by this data.
- VS texture sampler -> tf mapping (tf22 for s0).
- program_cntl / context_misc bits other than the ones verified above are named after the GPU registers by analogy.
- The `vc` bits of exec instructions and `serialize` sequence bits are decoded but carry no semantics for HLSL.
