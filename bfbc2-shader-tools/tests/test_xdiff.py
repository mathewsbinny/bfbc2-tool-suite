r"""The shader translator's differential test (xdiff.py) and its reference interpreter (xenos_exec.py).

Quick (no game data):
- the interpreter's ALU semantics on made-up values: 0 * inf = 0 (mul, mad, dp), max / min picking the second
  operand on NaN, the cube op's face, axis and coordinates on each of the six faces, saturate leaving ps unsaturated;
- xenos_hlsl's xcubefetch (a fetch's (s, t, face) back to a direction) compiled and run in shaderview: the direction
  it gives lands on the face and texel the cube op's coordinates name (a cube map whose texels hold their own face and
  position, point sampled);
- probe(): the recording line goes inside the predicate block of the instruction it follows.
With --game (Battlefield 1943's Wake database from the BF1943 tool suite's source): the shaders that exposed each
translator fix run through the whole differential test and must be exact - ps317 (the Chi-Ha's track: the shadow
map's point taps at the edge), ps36 (a depth-buffer tap outside the texture), ps120 / ps202 (two cube fetches),
ps476 (a texture in fetch constant 16).

  python test_xdiff.py [--game]
"""
import os
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
fails = []


def check(cond, what):
    print(('ok   ' if cond else 'FAIL ') + what)
    if not cond:
        fails.append(what)


class O:
    def __init__(self, **k):
        self.__dict__.update(k)


def operand(file, index, swz='xyzw', neg=False, ab=False):
    return O(file=file, index=index, swizzle=swz, negate=neg, abs=ab, rel=None, components=swz[3] + swz[0])


def machine(n=4):
    sh = O(is_pixel=True, header=O(program_cntl=(15 << 8)), literals={})
    return XE._Machine(sh, n, {}, {})


def alu_semantics():
    m = machine(3)
    m.r[0] = np.array([[0, 1, 2, 3], [0, 0, 0, 0], [1, 1, 1, 1]], F)
    m.r[1] = np.array([[np.inf, np.nan, 2, -1], [np.inf, np.inf, np.inf, np.inf], [np.nan, 0, 1, 2]], F)
    ins = O(vector_op='mul', vector_operands=[operand('temp', 0), operand('temp', 1)])
    r, _ = m.vector(ins, np.ones(3, bool))
    check(r[0, 0] == 0 and r[1].tolist() == [0, 0, 0, 0] and r[2, 1] == 0, 'mul: 0 * inf and 0 * NaN are 0')
    ins = O(vector_op='max', vector_operands=[operand('temp', 1), operand('temp', 0)])
    r, _ = m.vector(ins, np.ones(3, bool))
    check(r[2, 0] == 1 and r[0, 1] == 1, 'max: a NaN first operand gives the second (a >= b ? a : b)')
    ins = O(vector_op='dp3', vector_operands=[operand('temp', 0), operand('temp', 1)])
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
    ins = O(predicated=False, vector_nop=True, scalar_nop=False, scalar_op='adds',
            scalar_operands=[operand('temp', 2, 'xxxw')], scalar_dest=O(file='temp', index=3, write='x___',
                                                                            saturate=True, rel=None))
    m.alu(ins, np.ones(1, bool))
    check(m.r[3][0, 0] == 1 and m.ps[0] == 8, 'adds_sat: register saturated, ps unsaturated (Xenia StoreResult)')


def cubefetch_in_shaderview():
    """xcubefetch through Direct3D: each face's (s, t) must land on the texel the interpreter's cube sampling reads."""
    src = open(X.__file__).read()
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
    work = tempfile.mkdtemp(prefix='xdiff_test_')
    verts = np.zeros((n, 3, 2, 4), F)
    for j, (dx, dy) in enumerate(((0.1, 0.1), (1.4, 0.1), (0.1, 1.4))):
        verts[:, j, 0, 0] = np.arange(n) + dx
        verts[:, j, 0, 1] = dy
        verts[:, j, 1, :2] = st
        verts[:, j, 1, 2] = face
    files = {}

    def put(name, data):
        p = os.path.join(work, name)
        open(p, 'wb').write(data)
        return p
    job = [f'frame {put("o.raw", b"")} {n} 1 -1 -1 -1 -1', f'vs {put("vs", P.compile_hlsl(vs, "main", "vs_5_0"))}',
           f'ps {put("ps", P.compile_hlsl(ps, "main", "ps_5_0"))}', 'el 0 2 0 0 0', 'el 1 2 0 16 0',
           f'vb {put("vb", verts.tobytes())} 0 32', f'ib {put("ib", np.arange(3 * n, dtype=np.uint32).tobytes())} 0 32 {3 * n} 0',
           f'tex 2 cube {put("t", struct.pack("<III", 16, 16, 6) + T.tobytes())}',
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


def game():
    import xdiff
    out = os.path.join(tempfile.gettempdir(), 'xdiff_test_game')
    rc = xdiff.main(['--ps', '317', '36', '120', '202', '476', '--n', '512', '-o', out])
    import json
    r = json.load(open(os.path.join(out, 'results.json')))
    for p in ('317', '36', '120', '202', '476'):
        check(r[p]['status'] == 'exact', f'ps{p}: the translation exact against the interpreter ({r[p]["status"]})')


if __name__ == '__main__':
    alu_semantics()
    probe_placement()
    cubefetch_in_shaderview()
    if '--game' in sys.argv:
        game()
    print('all passed' if not fails else f'{len(fails)} failed')
    sys.exit(1 if fails else 0)
