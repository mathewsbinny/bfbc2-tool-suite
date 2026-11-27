r"""The shader translator's differential test (xdiff.py) and its reference interpreter (xenos_exec.py).

Quick (no game data):
- the interpreter's ALU semantics on made-up values: 0 * inf = 0 (mul, mad, dp), max / min picking the second
  operand on NaN, the cube op's face, axis and coordinates on each of the six faces, saturate leaving ps unsaturated;
- xenos_hlsl's xcubefetch (a fetch's (s, t, face) back to a direction) compiled and run in shaderview: the direction
  it gives lands on the face and texel the cube op's coordinates name (a cube map whose texels hold their own face and
  position, point sampled);
- probe(): the recording line goes inside the predicate block of the instruction it follows;
- the forward emitter (spec XSH-CF-1) on a hand-built if / else whose two forward jumps overlap, a pixel and a vertex
  shader: the translation (its _cf guards) drawn against the interpreter exact, both branches run, and a translation
  with a jump's target moved by one block caught (the comparison sees a wrong jump).
With --game (Battlefield 1943's Wake database from the BF1943 tool suite's source): the shaders that exposed each
translator fix run through the whole differential test and must be exact - ps317 (the Chi-Ha's track: the shadow
map's point taps at the edge), ps36 (a depth-buffer tap outside the texture), ps120 / ps202 (two cube fetches),
ps476 (a texture in fetch constant 16); and (round E3l) the run served on worker threads (shaderview -serve) gives
the same printed lines, results.json and files as one shaderview process per shader, one at a time.
Also with --game (round XD2): the 20 programs of the Xbox shader databases the forward emitter translates (all
vertex shaders, skinned: 4 of BF1943's Guadalcanal and Iwo Jima databases, 16 only in BFBC2's Xbox levels, read from
OPFOR's unpacked Xbox copy), each found by its program key, exact on every output with each jump taken by some
vertices and every CF reached.
With --game-all: the same comparison on every Wake pixel shader (815), all exact.

  python test_xdiff.py [--game] [--game-all]
"""
import os
import shutil
import struct
import sys
import tempfile

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'console-convert', 'core')
import pcbuild as P  # noqa: E402
import renderqa as RQ  # noqa: E402
import xenos_exec as XE  # noqa: E402
import xenos_hlsl as X  # noqa: E402

F = np.float32
fails: list[str] = []


def check(cond, what):
    print(('ok   ' if cond else 'FAIL ') + what)
    if not cond:
        fails.append(what)


class Stub:
    def __init__(self, **k):
        self.__dict__.update(k)


def operand(file, index, swz='xyzw', neg=False, ab=False):
    return Stub(file=file, index=index, swizzle=swz, negate=neg, abs=ab, rel=None, components=swz[3] + swz[0])


def machine(n=4):
    sh = Stub(is_pixel=True, header=Stub(program_cntl=(15 << 8)), literals={})
    return XE._Machine(sh, n, {}, {})


def alu_semantics():
    m = machine(3)
    m.r[0] = np.array([[0, 1, 2, 3], [0, 0, 0, 0], [1, 1, 1, 1]], F)
    m.r[1] = np.array([[np.inf, np.nan, 2, -1], [np.inf, np.inf, np.inf, np.inf], [np.nan, 0, 1, 2]], F)
    ins = Stub(vector_op='mul', vector_operands=[operand('temp', 0), operand('temp', 1)])
    r, _ = m.vector(ins, np.ones(3, bool))
    check(r[0, 0] == 0 and r[1].tolist() == [0, 0, 0, 0] and r[2, 1] == 0, 'mul: 0 * inf and 0 * NaN are 0')
    ins = Stub(vector_op='max', vector_operands=[operand('temp', 1), operand('temp', 0)])
    r, _ = m.vector(ins, np.ones(3, bool))
    check(r[2, 0] == 1 and r[0, 1] == 1, 'max: a NaN first operand gives the second (a >= b ? a : b)')
    ins = Stub(vector_op='dp3', vector_operands=[operand('temp', 0), operand('temp', 1)])
    r, _ = m.vector(ins, np.ones(3, bool))
    check(r[1, 0] == 0, 'dp3: zero times inf contributes 0')
    # the cube op on the six faces: src0 = d.zzxy, src1 = d.yxzz
    dirs = np.array([[2, .5, .25], [-2, .5, .25], [.5, 2, .25], [.5, -2, .25], [.5, .25, 2], [.5, .25, -2]], F)
    s0 = np.stack([dirs[:, 2], dirs[:, 2], dirs[:, 0], dirs[:, 1]], 1)
    s1 = np.stack([dirs[:, 1], dirs[:, 0], dirs[:, 2], dirs[:, 2]], 1)
    c = XE._Machine._cube(s0, s1)
    check(c[:, 3].tolist() == [0, 1, 2, 3, 4, 5], 'cube: face ids +X -X +Y -Y +Z -Z')
    check(np.allclose(np.abs(c[:, 2]), 4), 'cube: 2 x the major axis')
    # saturate on the stored value, ps keeps the raw result
    m = machine(1)
    m.r[2] = np.array([[3, 0, 0, 5]], F)
    ins = Stub(predicated=False, vector_nop=True, scalar_nop=False, scalar_op='adds',
            scalar_operands=[operand('temp', 2, 'xxxw')], scalar_dest=Stub(file='temp', index=3, write='x___',
                                                                            saturate=True, rel=None))
    m.alu(ins, np.ones(1, bool))
    check(m.r[3][0, 0] == 1 and m.ps[0] == 8, 'adds_sat: register saturated, ps unsaturated (Xenia StoreResult)')


def cubefetch_in_shaderview():
    """xcubefetch through Direct3D: each face's (s, t) must land on the texel the interpreter's cube sampling reads."""
    src = X.helpers()
    a = src.index('float3 xcubefetch(')
    b = src.index('\n}\n', a) + 3
    fn = src[a:b]
    n = 6 * 16
    rng = np.random.default_rng(3)
    face = np.repeat(np.arange(6), 16).astype(F)
    st = rng.uniform(1.03, 1.97, (n, 2)).astype(F)
    ps = ('TextureCube tc : register(t2); SamplerState s1 : register(s1);\n' + fn +
          'struct V2P { float4 pos : SV_Position; float4 t0 : TEXCOORD0; };\n'
          'float4 main(V2P i) : SV_Target0 { return tc.SampleLevel(s1, xcubefetch(i.t0.xyz), 0); }')
    vs = ('struct VIn { float4 p : TEXCOORD0; float4 c : TEXCOORD1; };\n'
          'struct V2P { float4 pos : SV_Position; float4 t0 : TEXCOORD0; };\n'
          f'V2P main(VIn i) {{ V2P o; o.pos = float4(i.p.x / {n}.0 * 2.0 - 1.0, 1.0 - i.p.y * 2.0, 0.5, 1.0); '
          'o.t0 = i.c; return o; }')
    T = np.zeros((6, 16, 16, 4), F)
    for f in range(6):
        T[f, :, :, 0] = f
        T[f, :, :, 1] = np.arange(16)[None, :]
        T[f, :, :, 2] = np.arange(16)[:, None]
        T[f, :, :, 3] = 1
    work = tempfile.mkdtemp(prefix='xdiff test ')
    verts = np.zeros((n, 3, 2, 4), F)
    for j, (dx, dy) in enumerate(((0.1, 0.1), (1.4, 0.1), (0.1, 1.4))):
        verts[:, j, 0, 0] = np.arange(n) + dx
        verts[:, j, 0, 1] = dy
        verts[:, j, 1, :2] = st
        verts[:, j, 1, 2] = face

    def put(name, data):
        p = os.path.join(work, name)
        open(p, 'wb').write(data)
        return p

    def asset(name, data):
        return RQ.job_path(put(name, data))

    job = [f'frame {asset("o.raw", b"")} {n} 1 -1 -1 -1 -1', f'vs {asset("vs", P.compile_hlsl(vs, "main", "vs_5_0"))}',
           f'ps {asset("ps", P.compile_hlsl(ps, "main", "ps_5_0"))}', 'el 0 2 0 0 0', 'el 1 2 0 16 0',
           f'vb {asset("vb", verts.tobytes())} 0 32', f'ib {asset("ib", np.arange(3 * n, dtype=np.uint32).tobytes())} 0 32 {3 * n} 0',
           f'tex 2 cube {asset("t", struct.pack("<III", 16, 16, 6) + T.tobytes())}',
           'samp 1 ' + struct.pack('<IIIIfII4fff', 0, 3, 3, 3, 0.0, 1, 1, 0, 0, 0, 0, 0.0, 0.0).hex(), 'draw', 'end']
    RQ.run_job(put('job.txt', '\n'.join(job).encode()), warp=True)
    got = np.fromfile(os.path.join(work, 'o.raw'), F).reshape(-1, 4)
    ref = XE._Machine._sample_cube(XE.Texture(T, True, 'clamp', True), np.concatenate([st, face[:, None]], 1))
    check(np.array_equal(got[:, :3], ref[:, :3]), 'xcubefetch: every (s, t, face) reads the texel the Xenos fetch reads '
          f'({int((got[:, :3] == ref[:, :3]).all(1).sum())} of {n})')


def probe_placement():
    import xdiff
    h = 'struct PSOut { float4 c0 : SV_Target0; };\nPSOut main(V2P i) {\n    if (p0) {\n        float4 v5 = r1;\n' \
        '        r2.x = v5.x;\n    }\n    PSOut po;\n    po.c0 = oC0;\n    return po;\n}'
    out = xdiff.probe(h, 5, 2)
    lines = out.split('\n')
    k = next(i for i, l in enumerate(lines) if 'probeVal = r2;' in l)
    check(lines[k - 1].strip() == 'r2.x = v5.x;' and lines[k].startswith('        '),
          'probe: recorded inside the predicate block, after the instruction\'s last statement')


# hand-built microcode (Xenos ALU / fetch words; xenos.py's decoders): p0 = r0.w > 0 (scalar setp_gt, the vector half
# a nop), r1 = r0 + r0, r1 = r0 * r0, an export of max(r, r), a vfetch of TEXCOORD0 into r0 (FMT_32_32_32_32_FLOAT)
SETP = (29 << 26, 0, (2 << 24) | (7 << 29))
ADD = (1 | (0xf << 16) | (50 << 26), 0, 7 << 29)
MUL = (1 | (0xf << 16) | (50 << 26), 0, (1 << 24) | (7 << 29))
VFETCH = (1 << 19, 0x688 | (38 << 16), 0)


def export(target: int, reg: int) -> tuple[int, int, int]:
    return ((1 << 15) | target | (0xf << 16) | (50 << 26), 0, (2 << 24) | (7 << 29) | (reg << 16) | (reg << 8))


def forward_program(pixel: bool) -> bytes:
    """if (r0.w > 0) r1 = r0 * r0; else r1 = r0 + r0 - laid out with two overlapping forward jumps around plain
    (unpredicated) execs, so the jumps alone decide which block runs:
      0 exec (p0 = r0.w > 0)   1 (p0) jmp 4   2 exec (r1 = r0 + r0)   3 jmp 5   4 exec (r1 = r0 * r0)   5 exec_end
    A pixel shader exports oC0 = r1 (r0 its TEXCOORD0); a vertex shader fetches r0 (TEXCOORD0) first and exports
    oPos = r1 and o0 = r0."""
    be = lambda *v: struct.pack(f'>{len(v)}I', *v)                                  # noqa: E731
    first = [SETP] if pixel else [VFETCH, SETP]
    last = [export(0, 1)] if pixel else [export(62, 1), export(0, 0)]
    ins = first + [ADD, MUL] + last
    n0 = len(first)
    cfs = [(0, 1, 0, n0, 0 if pixel else 1), (1, 4, 2, 1), (0, 1, n0, 1, 0), (1, 5, 1, 0), (0, 1, n0 + 1, 1, 0),
           (0, 2, n0 + 2, len(last), 0)]
    slots = (len(cfs) + 1) // 2
    enc = []
    for c in cfs:
        if c[0] == 0:                       # exec: opcode, first instruction, count, sequence (1: a fetch)
            _, op, at, cnt, seq = c
            enc.append(((slots + at) | (cnt << 12) | (seq << 16), op << 12))
        else:                               # cond_jmp: target, kind (1 unconditional, 2 predicated), condition
            _, target, kind, cond = c
            enc.append((target | ((kind == 1) << 13) | ((kind == 2) << 14), (11 << 12) | (cond << 10)))
    code: list[int] = []
    for a, b in zip(enc[0::2], enc[1::2]):
        code += [a[0], (a[1] & 0xffff) | ((b[0] & 0xffff) << 16), (b[0] >> 16) | (b[1] << 16)]
    for i in ins:
        code += i
    interp = 0xf050                         # TEXCOORD0 (usage 5) in r0 / o0, mask 0xf
    if pixel:
        hdr = [0, 4 * len(code), 0x0f0f, 0, 0, 1 << 5, 0, 1, interp]
    else:
        fetch = slots | (5 << 12)           # the vfetch's slot, TEXCOORD0
        export_slot = slots + len(ins) - 1 | (1 << 12)
        hdr = [0, 4 * len(code), 0x0f0f, 0, 0, 1 << 5, 1, 1, 1, 0x290, fetch, interp, export_slot]
    vsize = 0x24 + 4 * len(hdr)
    return be(0x102a1100 | (0 if pixel else 1), vsize, 4 * len(code), 0, 0, 0, 0x24, 0, 0) + be(*hdr) + be(*code)


def forward_jumps():
    import xdiff
    rng = np.random.default_rng(7)
    n = 256
    with tempfile.TemporaryDirectory(prefix='xdiff forward ') as work:
        for pixel in (True, False):
            what = 'pixel' if pixel else 'vertex'
            code = forward_program(pixel)
            sh = xenos_parse(code, pixel)
            r0 = rng.uniform(-1, 1, (n, 4)).astype(F)
            ref = XE.run(sh, {0: r0}, {}, {}, n)
            want = np.where(r0[:, 3:] > 0, r0 * r0, r0 + r0)
            got_ref = ref.outputs[0] if pixel else ref.position
            check(got_ref is not None and np.allclose(got_ref, want) and (pixel or np.array_equal(ref.outputs[0], r0)),
                  f'forward jumps ({what} shader): the interpreter runs the if / else ({int((r0[:, 3] > 0).sum())} '
                  f'of {n} lanes through the else block)')
            res = xdiff.run_shader(code, 0, 1, n, work, None, pixel)
            kind = 'ps' if pixel else 'vs'
            hlsl = open(os.path.join(work, f'{kind}0', f'{kind}.hlsl')).read()
            jumps = res.get('jumps', {})
            check('int _cf = 0;' in hlsl and res['status'] == 'exact' and jumps.get('1', [0, 0])[0] > 0
                  and jumps['1'][1] > 0 and jumps.get('3', [0])[0] > 0 and res.get('unreached') == [],
                  f'forward jumps ({what} shader): the forward emitter\'s translation exact against the interpreter, '
                  f'both branches run ({res["status"]}, jumps {jumps})')
            # the comparison sees a wrong jump: 1 -> 5 instead of 1 -> 4 skips the else block
            bad = hlsl.replace('_cf = 4;', '_cf = 5;')
            cs = xdiff.case(sh, np.random.default_rng(1 * 100003 + (0 if pixel else xdiff.VS_SEED)), n)
            ref = XE.run(sh, cs['inputs'], cs['consts'], cs['textures'], n,
                         pos=np.stack([np.arange(n) % xdiff.W, np.arange(n) // xdiff.W], 1).astype(F),
                         front=np.ones(n, bool))
            if pixel:
                wrong = xdiff.compare(ref, xdiff.run_hlsl(sh, cs, n, work, bad)[0])
            else:
                wrong = xdiff.compare_vs(ref, xdiff.run_hlsl_vs(sh, cs, n, work, bad), xdiff.vs_slots(sh))
            check(bad != hlsl and wrong['status'] == 'differs',
                  f'forward jumps ({what} shader): a jump to the wrong block is caught ({wrong["status"]}, '
                  f'{wrong.get("bad")} of {wrong.get("pixels")} outputs)')


def xenos_parse(code: bytes, pixel: bool):
    import xenos
    return xenos.parse_shader(code, pixel)


def hlsl_job_paths():
    import subprocess

    import xdiff
    with tempfile.TemporaryDirectory(prefix='xdiff job paths ') as work:
        sh = Stub(header=Stub(interpolators=[]))
        got, _ = xdiff.run_hlsl(sh, {'inputs': {}, 'engine': {}, 'textures': {}}, 3, work,
                               'float4 main() : SV_Target0 { return float4(0.25, 0.5, 0.75, 1); }')
        check(np.all(got == np.array([0.25, 0.5, 0.75, 1], F)),
              'run_hlsl: shader and buffer paths containing spaces render correctly')
        # Legacy jobs use unquoted path fields. A relative output avoids assumptions about TEMP's path.
        job = os.path.join(work, 'legacy job.txt')
        with open(job, 'w') as fh:
            fh.write('frame legacy.raw 2 2 0.25 0.5 0.75 1\nend\n')
        result = subprocess.run([RQ.shaderview(), job, '-warp'], cwd=work, capture_output=True)
        check(result.returncode == 0, 'shaderview: legacy unquoted path fields still accepted')
        if result.returncode == 0:
            check(np.all(np.fromfile(os.path.join(work, 'legacy.raw'), F).reshape(-1, 4) ==
                         np.array([0.25, 0.5, 0.75, 1], F)), 'shaderview: legacy job renders expected output')


def compare_nonfinite():
    import xdiff
    for a, b, exact in [(np.inf, 1, False), (1, np.inf, False), (np.inf, -np.inf, False),
                        (np.inf, np.inf, True), (-np.inf, -np.inf, True),
                        (np.nan, np.nan, True), (np.nan, 1, False), (1, np.nan, False),
                        (1, 1 + xdiff.ATOL / 2, True), (1, 2, False)]:
        ref = XE.Result({0: np.full((1, 4), a, F)}, np.ones(1, bool))
        result = xdiff.compare(ref, np.full((1, 4), b, F))
        check((result['status'] == 'exact') == exact, f'compare: {a} vs {b} exact={exact}')


def engine_named_constants():
    """A block constant the engine writes by name (the terrain strips' heightmapScaleAndOffsetXZ ...): declared in
    $Globals under its name after the engine's constants, the rest of the 'constants' array still baked in; the
    build finds its register (pcbuild.named_registers) and writes the PC block's records with the names' Adler-32
    (lower case) hashes, which every stock record has."""
    const = Stub(regset='float4', name='constants', reg=4, count=2, type=Stub(type='float4', cls='vector'))
    eng = Stub(regset='float4', name='viewProjMatrix', reg=0, count=4, type=Stub(type='float4', cls='matrix_rows'))
    head = Stub(vertex_elements=[], interpolators=[], num_temp_regs=1, outputs=1,
                context_misc_fields=Stub(sc_output_screen_xy=False))
    sh = Stub(is_pixel=True, ctab=Stub(constants=[eng, const]), literals={}, instructions=[], cf=[], header=head)
    iface = X.Interface([], material_regs={4: (9.0, 9.0, 9.0, 9.0), 5: (1.0, 2.0, 3.0, 4.0)},
                        named_ps={4: 'heightmapScaleAndOffsetXZ'})
    src = X.translate(sh, iface)  # type: ignore[arg-type]
    lines = src.split('\n')
    check('float4 heightmapScaleAndOffsetXZ;' in lines and 'm_constants_0' not in src
          and 'static const float4 m_constants_1 = float4(1.0, 2.0, 3.0, 4.0);' in lines,
          'named constant: a $Globals variable, its placeholder not baked; the unnamed register baked')
    check(lines.index('float4 viewProjMatrix[4];') < lines.index('float4 heightmapScaleAndOffsetXZ;'),
          'named constant: declared after the engine constants')
    # (the stub has no instructions: a read added, else the compiler drops $Globals altogether)
    code = P.compile_hlsl(src.replace('    return po;', '    po.c0 = heightmapScaleAndOffsetXZ; return po;'),
                          'main', 'ps_5_0')
    recs = P.named_registers(code, {'heightmapScaleAndOffsetXZ': (4, (0.0, 0.0, 0.0, 0.0))})
    check(recs == [('heightmapScaleAndOffsetXZ', 4, 4, (0.0, 0.0, 0.0, 0.0))],
          f'named constant: its register after viewProjMatrix ({recs})')
    check(P.name_hash('z') == 0x7b007b and P.name_hash('heightmap') == 0x127303b8
          and P.name_hash('heightmapScaleAndOffsetXZ') == 0x85b00a6c, 'block record hash: Adler-32 of the lower-case name')
    vs_iface = X.Interface([], named_vs={4: 'heightmapDecalOffsetY'})
    check(vs_iface.named_ps == {} and 'heightmapDecalOffsetY' not in X.translate(sh, vs_iface),  # type: ignore[arg-type]
          'named constant: per stage (a vertex shader\'s name not used by a pixel shader)')
    try:
        X.translate(sh, X.Interface([], named_ps={4: 'viewProjMatrix'}))  # type: ignore[arg-type]
        check(False, 'named constant: an engine constant\'s name refused')
    except X.TranslateError:
        check(True, 'named constant: an engine constant\'s name refused')


def game():
    import xdiff
    # a folder of its own (was %TEMP%\xdiff_test_game: two runs side by side wrote over each other's results.json,
    # and a stale one from an earlier run could be read back; 2026-11-17)
    out = tempfile.mkdtemp(prefix='xdiff_test_game_')
    try:
        _game(xdiff, out)
    finally:
        shutil.rmtree(out, ignore_errors=True)


def _game(xdiff, out):
    # 745 / 502: terrain strips (Quad_TireTracks, Wake_DirtRoad) - engine-named constants from their blocks
    ids = ('317', '36', '120', '202', '476', '745', '502')
    rc = xdiff.main(['--ps', *ids, '--n', '512', '-o', out])
    check(rc == 0, 'game differential command succeeds')
    import json
    r = json.load(open(os.path.join(out, 'results.json')))
    for p in ids:
        check(r[p]['status'] == 'exact', f'ps{p}: the translation exact against the interpreter ({r[p]["status"]})')
    hlsl = open(os.path.join(out, 'ps745', 'ps.hlsl')).read()
    check('float4 dynamicMaskAtlasParams;' in hlsl, 'ps745 (a terrain strip): its block\'s engine-named constants '
          'translated as $Globals variables')
    served_alike(xdiff, out, list(ids), '--workers', '3')
    forward_programs(xdiff)


# The 20 programs the forward emitter translates on the Xbox databases (test_xenos_native.py --game; round XD2): the
# database (xdiff --db), the vertex shader, the program key (test_xenos_native.program_key: SHA-1 of the code and the
# pixel flag, 16 hex digits). One shape, a skinned shader's if / else on a bone's row (XSH-CF-1)
FORWARD = [
    ('guadal_canal', 285, '3c509a0b5dc63fce'), ('guadal_canal', 469, '60ae25cd6a8fe9d9'),
    ('iwo_jima_s', 255, 'fafc8e54df968d09'), ('iwo_jima_s', 520, 'e85687aa2d5b4293'),
    ('bfbc2:mp_001', 44, '8ec95f1d3c613978'), ('bfbc2:mp_001', 106, '6afecf03cda33452'),
    ('bfbc2:mp_001', 213, 'e5c022424c33d1da'), ('bfbc2:mp_001', 245, 'adb7e1df15c5ccc9'),
    ('bfbc2:mp_001', 276, '30f66d70fbbe4e3c'), ('bfbc2:mp_001', 299, '808cb7917c66fe5f'),
    ('bfbc2:mp_001', 358, '736a4132be1d623b'), ('bfbc2:mp_001', 712, 'aaf25668930f8790'),
    ('bfbc2:mp_002', 8, '899af7bb6cb886a2'), ('bfbc2:mp_002', 754, '53d496baff113026'),
    ('bfbc2:mp_003', 14, 'a62d958bb6189b33'), ('bfbc2:mp_003', 153, 'f9d0df9fa78d67e5'),
    ('bfbc2:mp_004', 305, 'e1feefde2f2c2b25'), ('bfbc2:mp_004', 824, '1f8f7c9c634b6377'),
    ('bfbc2:mp_008', 344, '5b51fd573ccc9b27'), ('bfbc2:mp_008', 421, 'cf8c4fca5714f766'),
]


def forward_programs(xdiff):
    """The 20: each database's vertex shaders through xdiff (512 vertices), found by key; exact on every output, the
    if / else's first jump taken by some vertices and not by others, every CF reached."""
    import hashlib
    import json
    good = 0
    for db in dict.fromkeys(d for d, _, _ in FORWARD):
        want = [(v, k) for d, v, k in FORWARD if d == db]
        w = xdiff.load_db(db)
        keys = [hashlib.sha1(bytes(w.vs[v].code) + b'\0').hexdigest()[:16] for v, _ in want]
        check(keys == [k for _, k in want], f'{db}: vertex shaders {[v for v, _ in want]} are the recorded programs')
        out = tempfile.mkdtemp(prefix='xdiff_test_forward_')
        try:
            xdiff.main(['--db', db, '--vs', *[str(v) for v, _ in want], '--n', '512', '-o', out])
            r = json.load(open(os.path.join(out, 'results.json')))
            hlsl = {v: open(os.path.join(out, f'vs{v}', 'vs.hlsl')).read() for v, _ in want
                    if os.path.isfile(os.path.join(out, f'vs{v}', 'vs.hlsl'))}
        finally:
            shutil.rmtree(out, ignore_errors=True)
        for v, _ in want:
            res = r.get(f'vs{v}', {})
            j = res.get('jumps', {})
            # every jump taken by some vertices, the first one (the if) also not taken by some
            ok = (res.get('status') == 'exact' and 'int _cf = 0;' in hlsl.get(v, '') and res.get('unreached') == []
                  and bool(j) and all(t > 0 for t, _ in j.values()) and next(iter(j.values()))[1] > 0)
            good += ok
            if not ok:
                print(f'       {db} vs{v}: {res}')
    check(good == len(FORWARD), f'the forward emitter\'s {len(FORWARD)} programs exact against the interpreter, both '
          f'sides of their if / else run ({good} of {len(FORWARD)})')


def tree(root):
    """{relative path: SHA-256} of an xdiff output folder (the job files name their folder: left out)."""
    import hashlib
    got = {}
    for d, _, names in os.walk(root):
        for n in names:
            if n != 'job.txt':
                p = os.path.join(d, n)
                got[os.path.relpath(p, root)] = hashlib.sha256(open(p, 'rb').read()).hexdigest()
    return got


def served_alike(xdiff, out, ids, *args):
    """Round E3l: a run served (shaderview -serve, worker threads) against one shaderview process per shader, one
    shader at a time (BFBC2_SHADERVIEW_SERVE=0, --workers 1): the printed lines, results.json and every file alike."""
    import contextlib
    import io
    ref = out + '_reference'
    saved = os.environ.get('BFBC2_SHADERVIEW_SERVE')
    try:
        runs = []
        for folder, serve, extra in ((out, '1', list(args)), (ref, '0', ['--workers', '1'])):
            os.environ['BFBC2_SHADERVIEW_SERVE'] = serve
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                xdiff.main(['--ps', *ids, '--n', '512', '-o', folder] + extra)
            runs.append((buf.getvalue(), tree(folder)))
        (a, fa), (b, fb) = runs
        check(a == b and fa == fb and len(fa) == 4 * len(ids) + 1,
              f'served on worker threads against one process per shader, one at a time: printed lines alike, '
              f'{sum(fa[k] == fb.get(k) for k in fa)} of {len(fb)} files alike (results.json; per shader its '
              'packed inputs, draw, ps.hlsl, disassembly)')
    finally:
        RQ.close_servers()
        if saved is None:
            os.environ.pop('BFBC2_SHADERVIEW_SERVE', None)
        else:
            os.environ['BFBC2_SHADERVIEW_SERVE'] = saved
        shutil.rmtree(ref, ignore_errors=True)


def game_all():
    """Every Wake pixel shader (815): served on worker threads against one shaderview process per shader, one at a
    time - the verdicts, printed lines and every file alike (~4 min at below-normal priority, most of it the
    reference)."""
    import xdiff
    out = tempfile.mkdtemp(prefix='xdiff_test_all_')
    try:
        db = xdiff.S.ShaderDb.parse(xdiff.bf1943_entry(xdiff.DBS['wake']), where='wake')
        ids = [str(p) for p in range(len(db.ps))]
        served_alike(xdiff, out, ids)
        import json
        r = json.load(open(os.path.join(out, 'results.json')))
        exact = sum(v['status'] == 'exact' for v in r.values())
        check(exact == len(ids), f'every Wake pixel shader exact ({exact} of {len(ids)})')
    finally:
        shutil.rmtree(out, ignore_errors=True)


if __name__ == '__main__':
    alu_semantics()
    compare_nonfinite()
    hlsl_job_paths()
    probe_placement()
    forward_jumps()
    engine_named_constants()
    cubefetch_in_shaderview()
    if '--game' in sys.argv:
        game()
    if '--game-all' in sys.argv:
        game_all()
    print('all passed' if not fails else f'{len(fails)} failed')
    sys.exit(1 if fails else 0)
