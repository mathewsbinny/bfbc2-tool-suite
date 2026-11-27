"""The shader translator's differential test: every BF1943 pixel shader run twice on the same inputs - its original Xenos
microcode in xenos_exec.py (the reference) and xenos_hlsl's translation compiled for Direct3D 11 and drawn by
shaderview.exe (WARP) - and the outputs compared pixel by pixel. Vertex shaders (--vs) the same way, vertex by vertex:
every output (oPos and each interpolator) of the translation drawn as a pixel of its own (vs_wrapper).

  python xdiff.py [--db wake] [--ps 317 ...] [--vs [12 ...]] [--n 1024] [--seed 1] [-o out_dir] [--limit K]

--db: a short name (DBS), a level folder (levels/<level>/shaders/single.res: 'iwo_jima_s' ...), a database's
resource name ('ui/shaders/single.res'), or bfbc2:<level> (a BFBC2 Xbox level's database from OPFOR's unpacked Xbox
copy: 'bfbc2:mp_001'). Without --ps / --vs: every pixel shader; --vs with no numbers: every
vertex shader.

Inputs per shader: random constants (the engine's by constant-table name, the material's by name or register), random
interpolants in [-1, 1] (+ pixel positions for the screen-position register), and per sampler a float texture whose
channels are linear ramps in u and v with clamped addressing: bilinear filtering of a linear ramp is exact, so the
two sides' filtering differs by rounding only. The shadow map gets the same treatment (a ramp of depths).
Result: per shader 'exact' (every pixel within tolerance, the same pixels discarded), 'differs' (with the share of
pixels, the worst error and the first differing pixel's values) or an error (the translation or the interpreter
refused it). A summary line gives the share of exact shaders: the translator's accuracy on the shader code itself
(texture formats, sampler states and the engine's inputs are other layers - see README).
"""
from __future__ import annotations

import argparse
import concurrent.futures
import contextlib
import json
import os
import re
import struct
import sys
import tempfile
from collections.abc import Iterable
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(HERE)
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'shader-tools', 'console-convert', 'bf1943-import')
import pcbuild as P  # noqa: E402
import renderqa as RQ  # noqa: E402
import shaderdb as S  # noqa: E402
import xenos  # noqa: E402
import xenos_exec as XE  # noqa: E402
import xenos_hlsl as X  # noqa: E402

DBS = {'wake': 'levels/wake_island_s/shaders/single.res'}
VS_SEED = 70001                         # a vertex shader's seed offset (seed * 100003 + VS_SEED + index)
VS_POS_SEMANTIC = 16                    # the wrapper's own input (pixel, output slot): past the Xenos usage indices 0-15
W = 64                                  # render target width (pixels per row)
SENTINEL = -12345.0                     # clear colour: a discarded pixel keeps it
TEX = 16                                # ramp texture size
RTOL, ATOL = 2e-3, 2e-3
SHADOW_ZERO = bool(os.environ.get('XDIFF_SHADOW_ZERO'))
XE.ZERO_OUTSIDE = bool(os.environ.get('XDIFF_POINT_ZERO'))   # classify: the shadow map read as Direct3D's Load does
MIN_SHARE = 0.02                        # bisect: a register differing on fewer live pixels is rounding noise


def ramp_texture(rng: np.random.Generator, faces: int = 1) -> np.ndarray:
    """(faces, TEX, TEX, 4) float32: each channel a + b*u + c*v at texel centres (u, v in texels / TEX)."""
    u = (np.arange(TEX) + 0.5) / TEX
    uu, vv = np.meshgrid(u, u)
    out = np.zeros((faces, TEX, TEX, 4), np.float32)
    for f in range(faces):
        for ch in range(4):
            a, b, c = rng.uniform(0.1, 0.5), rng.uniform(-0.4, 0.4), rng.uniform(-0.4, 0.4)
            out[f, :, :, ch] = a + b * uu + c * vv
    return out


def sampler_desc(clamp: bool = True) -> str:
    addr = 3 if clamp else 1
    d = struct.pack('<IIIIfII4fff', 0x0, addr, addr, addr, 0.0, 1, 1, 0, 0, 0, 0, 0.0, 0.0)
    return d.hex()


VS_SRC = """
struct VIn { float4 p : TEXCOORD0; %(vin)s };
struct V2P { float4 pos : SV_Position; %(vout)s };
V2P main(VIn i) {
    V2P o;
    o.pos = float4(i.p.x / %(w)d.0 * 2.0 - 1.0, 1.0 - i.p.y / %(h)d.0 * 2.0, 0.5, 1.0);
    %(copy)s
    return o;
}
"""


def db_key(name: str) -> str:
    """A shader database's resource name from --db: a short name (DBS), a resource name, or a level folder."""
    if name in DBS:
        return DBS[name]
    if '/' in name:
        return name.lower()
    return f'levels/{name.lower()}/shaders/single.res'


def load_db(name: str) -> S.ShaderDb:
    """--db's database: BF1943's (bf1943_entry, db_key) or, as bfbc2:<level>, a BFBC2 Xbox level's from OPFOR's
    unpacked Xbox copy (work/xbox/dist/xenon/levels/<level>/level-00.fbrb: the programs test_xenos_native.py --game
    reads beside BF1943's)."""
    if name.lower().startswith('bfbc2:'):
        lv = name.split(':', 1)[1]
        path = os.path.join(S.find_opfor(None), 'work', 'xbox', 'dist', 'xenon', 'levels', lv, 'level-00.fbrb')
        return S.load(path)[0]
    return S.ShaderDb.parse(bf1943_entry(db_key(name)), where=name)


def engine_named(w: S.ShaderDb, pixel: bool = True) -> dict[int, dict[int, str]]:
    """{pixel (or vertex) shader: {register: name}} of the block constants the engine writes by name
    (pcbuild.ENGINE_NAMED: the terrain strips' heightmapScaleAndOffsetXZ, dynamicMaskAtlasParams ...) in the programs'
    second blocks and the shader's own block - translated as $Globals variables (xenos_hlsl Interface.named_ps /
    named_vs), as the build does."""
    out: dict[int, dict[int, str]] = {}
    shaders = w.ps if pixel else w.vs
    for rec in w.programs:
        vs, ps, _, b2 = struct.unpack_from(w.e + '4I', rec, 0x20)
        k = ps if pixel else vs
        # (a vertex shader's: its own block only - the program's second block names the pixel shader's registers;
        # FORMAT.md section 1)
        for b in (b2, shaders[k].param) if pixel else (shaders[k].param,):
            for name, reg, _, _ in S.block_constants(w.params[b], w.e):
                if name in P.ENGINE_NAMED:
                    have = out.setdefault(k, {})
                    if have.setdefault(reg, name) != name:
                        raise ValueError(f'{"ps" if pixel else "vs"}{k}: c{reg} named {have[reg]} and {name}')
    return out


def case(sh: Any, rng: np.random.Generator, n: int, named: dict[int, str] | None = None) -> dict[str, Any]:
    """Random inputs for one shader: engine values, material values, interpolants, textures. named: the registers
    the engine writes by name (engine_named) - the same random values, given to the translation under their names."""
    ct = sh.ctab.constants if sh.ctab else []
    engine, material, consts = {}, {}, {}
    for c in ct:
        if c.regset != 'float4':
            continue
        vals = rng.uniform(-1, 1, (c.count, 4)).astype(np.float32)
        if c.name in X.ENGINE_SIZE:
            size = max(X.ENGINE_SIZE[c.name], c.count)
            full = np.zeros((size, 4), np.float32)
            full[:c.count] = vals
            engine[c.name] = full
        else:
            material[c.name] = [tuple(map(float, v)) for v in vals]
        for k in range(c.count):
            consts[c.reg + k] = vals[k]
    if sh.is_pixel:
        inputs = {it.reg: rng.uniform(-1, 1, (n, 4)).astype(np.float32) for it in sh.header.interpolators}
    else:
        # a vertex shader: its elements' values (by usage index); the second half of the vertices in [0, 20): a bone
        # index (trunc, x 3, c[k + a0]) is 0 for any value in (-1, 1), so the branches a bone's values decide (the
        # skinned shaders' if / else on a bone's row) would go one way on every vertex
        half = n // 2
        inputs = {el.usage_index: np.concatenate([rng.uniform(-1, 1, (half, 4)), rng.uniform(0, 20, (n - half, 4))])
                  .astype(np.float32) for el in sh.header.vertex_elements}
    textures = {}
    for c in ct:
        if c.regset == 'sampler':
            cube = c.type.type == 'samplerCube'
            textures[c.reg] = XE.Texture(ramp_texture(rng, 6 if cube else 1), cube,
                                         'zero' if (SHADOW_ZERO and c.name == X.SHADOW) else 'clamp', True)
    named = {r: nm for r, nm in (named or {}).items() if r in consts}
    for r, nm in named.items():
        engine[nm] = consts[r]
    return {'engine': engine, 'material': material, 'consts': consts, 'inputs': inputs, 'textures': textures,
            'named': named}


def translated(sh: Any, cs: dict[str, Any]) -> str:
    """The plain interface (the shader's own interpolators; a vertex shader's elements as float4s)."""
    inter = sorted(it.usage_index for it in sh.header.interpolators)
    if not sh.is_pixel:
        vs_in = {el.usage_index: 'float4' for el in sh.header.vertex_elements}
        return X.translate(sh, X.Interface(inter, vs_inputs=vs_in, material=cs['material'],
                                           named_vs=cs.get('named')))
    return X.translate(sh, X.Interface(inter, material=cs['material'], named_ps=cs.get('named')))


def vs_slots(sh: Any) -> list[int | None]:
    """A vertex shader's outputs as the differential test reads them: None (oPos), then each interpolator's TEXCOORD
    (V2P's fields t<k>: the export register o<k>, which the translation writes as o.t<k>)."""
    return [None, *sorted(it.usage_index for it in sh.header.interpolators)]


def vs_wrapper(hlsl: str, slots: list[int | None]) -> tuple[str, str]:
    """(vertex, pixel) shader sources reading back a translated vertex shader: its main renamed xdiff_vs, called by
    a main of the wrapper's own that also takes a TEXCOORD<VS_POS_SEMANTIC> (x, y: the pixel; z: the output slot; w:
    the frame's height) and passes that slot's output (pos or t<k>) unchanged (nointerpolation) to the pixel shader,
    which returns it."""
    head = 'V2P main(VIn vin) {'
    if hlsl.count(head) != 1:
        raise ValueError('not a translated vertex shader (no "V2P main(VIn vin)")')
    picks = ' '.join(f'if (k == {j}) v = r.{"pos" if s is None else f"t{s}"};' for j, s in enumerate(slots))
    out = 'struct XOut { float4 pos : SV_Position; nointerpolation float4 v : TEXCOORD0; };\n'
    vs = (hlsl.replace(head, 'V2P xdiff_vs(VIn vin) {') + '\n' + out +
          f'XOut main(VIn vin, float4 xp : TEXCOORD{VS_POS_SEMANTIC}) {{\n'
          '    V2P r = xdiff_vs(vin);\n'
          '    int k = (int)xp.z;\n'
          f'    float4 v = 0.0; {picks}\n'
          f'    XOut o; o.pos = float4(xp.x / {W}.0 * 2.0 - 1.0, 1.0 - xp.y / xp.w * 2.0, 0.5, 1.0); o.v = v;\n'
          '    return o;\n}\n')
    return vs, out + 'float4 main(XOut i) : SV_Target0 { return i.v; }\n'


def run_hlsl_vs(sh: Any, cs: dict[str, Any], n: int, work: str, hlsl: str) -> np.ndarray:
    """A translated vertex shader's outputs ((len(vs_slots), n, 4)): vertex j's slot s drawn as pixel s * n + j, a
    triangle of three copies of the vertex (the same inputs)."""
    slots = vs_slots(sh)
    vsrc, psrc = vs_wrapper(hlsl, slots)
    vs = P.compile_hlsl(vsrc, 'main', 'vs_5_0')
    ps = P.compile_hlsl(psrc, 'main', 'ps_5_0')
    usages = sorted(cs['inputs'])
    total = n * len(slots)
    h = (total + W - 1) // W
    verts = np.zeros((total, 3, 1 + len(usages), 4), np.float32)
    pix = np.arange(total)
    for j, (dx, dy) in enumerate(((0.1, 0.1), (1.4, 0.1), (0.1, 1.4))):
        verts[:, j, 0, 0] = pix % W + dx
        verts[:, j, 0, 1] = pix // W + dy
        verts[:, j, 0, 2] = pix // n
        verts[:, j, 0, 3] = h
        for k, u in enumerate(usages):
            verts[:, j, 1 + k] = np.tile(cs['inputs'][u], (len(slots), 1))
    pack: list[bytes] = []
    size = 0

    def asset(data: bytes) -> str:
        nonlocal size
        pack.append(data)
        size += len(data)
        return f'*{size - len(data)}:{len(data)}'

    out_raw = os.path.join(work, 'out.raw')
    job = [f'frame {RQ.job_path(out_raw)} {W} {h} {SENTINEL} {SENTINEL} {SENTINEL} {SENTINEL}',
           f'vs {asset(vs)}', f'ps {asset(ps)}', f'el {VS_POS_SEMANTIC} 2 0 0 0']
    job += [f'el {u} 2 0 {16 * (1 + k)} 0' for k, u in enumerate(usages)]
    job.append(f'vb {asset(verts.tobytes())} 0 {16 * (1 + len(usages))}')
    job.append(f'ib {asset(np.arange(3 * total, dtype=np.uint32).tobytes())} 0 32 {3 * total} 0')
    job.append(f'cb vs {asset(RQ.cbuffer(vs, cs["engine"], []))}')
    for reg, t in cs['textures'].items():
        tx = t.texels
        hdr = struct.pack('<III', tx.shape[2], tx.shape[1], tx.shape[0])
        job.append(f'vtex {reg} {"cube" if t.cube else "2d"} {asset(hdr + tx.tobytes())}')
    job.append(f'samp 1 {sampler_desc()}')
    job += ['draw', 'end']
    pack_path = os.path.join(work, 'inputs.bin')
    with open(pack_path, 'wb') as f:
        f.write(b''.join(pack))
    job.insert(0, f'pack {RQ.job_path(pack_path)}')
    jp = os.path.join(work, 'job.txt')
    with open(jp, 'wb') as f:
        f.write('\n'.join(job).encode())
    with contextlib.suppress(FileNotFoundError):
        os.remove(out_raw)
    RQ.run_job(jp, warp=True)
    img = np.fromfile(out_raw, np.float32).reshape(-1, 4)
    return img[:total].reshape(len(slots), n, 4)


def compare_vs(ref: XE.Result, got: np.ndarray, slots: list[int | None]) -> dict[str, Any]:
    """A vertex shader's outputs against the interpreter's: every output of every vertex within tolerance (NaN = NaN,
    infinities equal only to themselves)."""
    n = got.shape[1]
    zero = np.zeros((n, 4), np.float32)
    pos = ref.position if ref.position is not None else zero
    want = np.stack([pos if s is None else ref.outputs.get(s, zero) for s in slots])
    with np.errstate(invalid='ignore'):
        close = np.isclose(got, want, rtol=RTOL, atol=ATOL, equal_nan=True).all(2)
    oob = ref.out_of_range if ref.out_of_range is not None else np.zeros(n, bool)
    close |= oob[None, :]                   # (a c[k + a0] read outside its array: the register file's layout)
    bad = np.argwhere(~close)
    names = ['oPos' if s is None else f'o{s}' for s in slots]
    res: dict[str, Any] = {'status': 'exact' if not len(bad) else 'differs', 'pixels': int(close.size),
                           'bad': int(len(bad)), 'kill_mismatch': 0, 'outputs': names}
    if oob.any():
        res['out_of_range'] = int(oob.sum())
    if oob.all():
        res['status'] = 'out-of-range'
    if len(bad):
        s, i = (int(v) for v in bad[0])
        res['first'] = {'output': names[s], 'vertex': i, 'ref': want[s, i].tolist(), 'hlsl': got[s, i].tolist()}
        a, b = want[~close], got[~close]
        fin = np.isfinite(a) & np.isfinite(b)
        res['max_err'] = float(np.abs(a - b)[fin].max()) if fin.any() else None
    return res


PROBE_MISS = -23456.0                   # a probe's colour where the instruction did not run


def probe(hlsl: str, addr: int, reg: int) -> str:
    """The translation recording r<reg> right after instruction <addr> (after its last statement, inside whatever
    predicate or branch encloses it) and returning the recorded value as its colour (PROBE_MISS where the
    instruction never ran)."""
    lines = hlsl.split('\n')
    tok = re.compile(rf'\b[vsf]{addr}\b')
    last = max((k for k, l in enumerate(lines) if tok.search(l)), default=None)
    if last is None:
        raise ValueError(f'instruction {addr} not found in the translation')
    ind = lines[last][:len(lines[last]) - len(lines[last].lstrip())]
    lines.insert(last + 1, f'{ind}probeVal = r{reg};')
    start = next(k for k, l in enumerate(lines) if l.startswith('PSOut main('))
    lines.insert(start + 1, f'    float4 probeVal = float4({PROBE_MISS}, {PROBE_MISS}, {PROBE_MISS}, {PROBE_MISS});')
    end = max(k for k, l in enumerate(lines) if l.strip() == 'PSOut po;')
    lines.insert(end, '    { PSOut q; q.c0 = probeVal; return q; }')
    return '\n'.join(lines)


def compile_jobs(sh: Any, n: int, hlsl: str) -> tuple[P.CompileJob, P.CompileJob]:
    """(pixel, vertex) shader compiles of a run: the translation, and the vertex shader feeding its interpolants."""
    inter = sorted(it.usage_index for it in sh.header.interpolators)
    h = (n + W - 1) // W
    vin = ' '.join(f'float4 i{k} : TEXCOORD{k + 1};' for k in range(len(inter)))
    vout = ' '.join(f'float4 t{u} : TEXCOORD{u};' for u in inter)
    copy = ' '.join(f'o.t{u} = i.i{k};' for k, u in enumerate(inter))
    return (P.CompileJob(hlsl, 'main', 'ps_5_0'),
            P.CompileJob(VS_SRC % {'vin': vin, 'vout': vout, 'copy': copy, 'w': W, 'h': h}, 'main', 'vs_5_0'))


def run_hlsl(sh: Any, cs: dict[str, Any], n: int, work: str, hlsl: str | None = None) -> tuple[np.ndarray, str]:
    """oC0 of the translated shader (or of the given HLSL) for every pixel ((n, 4); SENTINEL where discarded)."""
    inter = sorted(it.usage_index for it in sh.header.interpolators)
    hlsl = hlsl if hlsl is not None else translated(sh, cs)
    ps_job, vs_job = compile_jobs(sh, n, hlsl)
    ps = P.compile_hlsl(*ps_job)
    h = (n + W - 1) // W
    vs = P.compile_hlsl(*vs_job)
    by_usage = {it.usage_index: it.reg for it in sh.header.interpolators}
    stride = 16 * (1 + len(inter))
    verts = np.zeros((n, 3, 1 + len(inter), 4), np.float32)
    px = np.arange(n) % W
    py = np.arange(n) // W
    for j, (dx, dy) in enumerate(((0.1, 0.1), (1.4, 0.1), (0.1, 1.4))):
        verts[:, j, 0, 0] = px + dx
        verts[:, j, 0, 1] = py + dy
        for k, u in enumerate(inter):
            verts[:, j, 1 + k] = cs['inputs'][by_usage[u]]
    # the draw's inputs in one file, inputs.bin (shaderview "pack"; round E3l): every new file is virus-scanned when
    # shaderview opens it, ~5 ms each - a dozen per shader were most of the run's time
    pack: list[bytes] = []
    size = 0

    def asset(name: str, data: bytes) -> str:
        nonlocal size
        pack.append(data)
        size += len(data)
        return f'*{size - len(data)}:{len(data)}'

    out_raw = os.path.join(work, 'out.raw')
    job = [f'frame {RQ.job_path(out_raw)} {W} {h} {SENTINEL} {SENTINEL} {SENTINEL} {SENTINEL}',
           f'vs {asset("vs.dxbc", vs)}', f'ps {asset("ps.dxbc", ps)}']
    job += [f'el {k} 2 0 {16 * k} 0' for k in range(1 + len(inter))]
    job.append(f'vb {asset("vb.bin", verts.tobytes())} 0 {stride}')
    ib = np.arange(3 * n, dtype=np.uint32)
    job.append(f'ib {asset("ib.bin", ib.tobytes())} 0 32 {3 * n} 0')
    job.append(f'cb ps {asset("cb.bin", RQ.cbuffer(ps, cs["engine"], []))}')
    for reg, t in cs['textures'].items():
        tx = t.texels
        hdr = struct.pack('<III', tx.shape[2], tx.shape[1], tx.shape[0])
        job.append(f'tex {reg} {"cube" if t.cube else "2d"} {asset(f"t{reg}.bin", hdr + tx.tobytes())}')
    job.append(f'samp 1 {sampler_desc()}')
    for slot, desc in cs.get('samplers', {}).items():
        job.append(f'samp {slot} {desc.hex()}')
    job += ['draw', 'end']
    pack_path = os.path.join(work, 'inputs.bin')
    with open(pack_path, 'wb') as f:
        f.write(b''.join(pack))
    job.insert(0, f'pack {RQ.job_path(pack_path)}')
    jp = os.path.join(work, 'job.txt')
    with open(jp, 'wb') as f:
        f.write('\n'.join(job).encode())
    with contextlib.suppress(FileNotFoundError):
        os.remove(out_raw)                                  # (an earlier run's)
    RQ.run_job(jp, warp=True)
    img = np.fromfile(out_raw, np.float32).reshape(-1, 4)
    return img[:n], hlsl


def compare(ref: XE.Result, got: np.ndarray) -> dict[str, Any]:
    r0 = ref.outputs.get(0)
    if r0 is None:
        return {'status': 'no-output'}
    killed_hlsl = np.all(got == SENTINEL, 1)
    alive = ref.alive
    kill_mismatch = alive == killed_hlsl
    both = alive & ~killed_hlsl
    ok = np.ones(len(got), bool)
    ok &= ~kill_mismatch
    a, b = r0[both], got[both]
    finite = np.isfinite(a) & np.isfinite(b)
    with np.errstate(invalid='ignore'):
        close = np.isclose(b, a, rtol=RTOL, atol=ATOL, equal_nan=True)
    okb = close.all(1)
    ok[np.where(both)[0]] = okb
    oob = ref.out_of_range
    if oob is not None and oob.any():       # (a c[k + a0] read outside its array: the register file's layout)
        ok |= oob
    bad = np.where(~ok)[0]
    res: dict[str, Any] = {'status': 'exact' if not len(bad) else 'differs', 'pixels': int(len(got)),
           'bad': int(len(bad)), 'killed_ref': int((~alive).sum()), 'kill_mismatch': int(kill_mismatch.sum())}
    if oob is not None and oob.any():
        res['out_of_range'] = int(oob.sum())
    if len(bad):
        i = int(bad[0])
        res['first'] = {'pixel': i, 'ref': r0[i].tolist(), 'hlsl': got[i].tolist(), 'ref_alive': bool(alive[i])}
        with np.errstate(invalid='ignore'):
            err = np.abs(a - b)[finite.all(1)] if finite.any() else np.zeros(1)
        res['max_err'] = float(np.nanmax(err)) if err.size else None
    return res


def bisect(sh: Any, cs: dict[str, Any], n: int, work: str, pos: np.ndarray) -> str:
    """The first instruction whose destination register differs between the two sides (live pixels only)."""
    ref = XE.run(sh, cs['inputs'], cs['consts'], cs['textures'], n, pos=pos, front=np.ones(n, bool), trace=True)
    base = translated(sh, cs)
    dis = {}
    for c in sh.cf:
        for ins in getattr(c, 'instructions', None) or []:
            dis[ins.address] = ' | '.join(xenos.disasm_instruction(ins))
    assert ref.snaps is not None
    for addr, (regs, alive, ran) in ref.snaps.items():
        ins = next(i for c in sh.cf for i in (getattr(c, 'instructions', None) or []) if i.address == addr)
        dests = []
        if ins.kind == 'fetch':
            if getattr(ins, 'has_dest', False) and ins.dst_swizzle.strip('_?'):
                dests.append(ins.dst)
        else:
            for d in (ins.vector_dest, ins.scalar_dest):
                if d.file == 'temp' and d.write.strip('_') and d.index not in dests:
                    dests.append(d.index)
        for reg in dests:
            try:
                got, _ = run_hlsl(sh, cs, n, work, probe(base, addr, reg))
            except ValueError:
                continue
            hit = ~np.all(got == PROBE_MISS, 1)
            if (hit != ran).sum() > MIN_SHARE * len(ran):
                k = np.where(hit != ran)[0][0]
                return (f'instruction {addr} runs on different pixels: xenos {ran.sum()}, hlsl {hit.sum()} '
                        f'(pixel {k}: xenos {bool(ran[k])})' + chr(10) + f'  {dis[addr]}')
            live = alive & ran & hit
            a, b = regs[reg][live], got[live]
            close = np.isclose(a, b, rtol=RTOL, atol=ATOL) | (np.isnan(a) & np.isnan(b))
            bad = np.where(~close.all(1))[0]
            if len(bad) > MIN_SHARE * live.sum():
                i = bad[0]
                return (f'first difference after {addr}: r{reg} on {len(bad)}/{live.sum()} live pixels\n  {dis[addr]}\n'
                        f'  pixel {np.where(live)[0][i]}: xenos {a[i].tolist()} hlsl {b[i].tolist()}')
    return 'no register differs (the difference is in the outputs / kills)'


def bf1943_entry(key: str) -> bytes:
    """A BF1943 resource by name from the bundles of its own folder (levels/<level>/), one bundle in memory at a time:
    bf1943import.Source holds all 550 MB of BF1943's bundles, which took this test past 1 GB."""
    import bf1943import
    root = bf1943import.default_source()
    fb = bf1943import.fbrb1943()
    folder = os.path.join(root, *key.split('/')[:2])
    for d, dirs, names in os.walk(folder if os.path.isdir(folder) else root):
        dirs.sort()
        for f in sorted(names):
            if f.endswith('.fbrb'):
                try:
                    bd = fb.Bundle(open(os.path.join(d, f), 'rb').read())
                except ValueError:
                    continue
                for e in bd.entries:
                    if e['name'].lower() == key:
                        return bytes(bd.data(e['name']))
    raise KeyError(key)


def default_workers() -> int:
    """Shaders run at a time: BFBC2_XDIFF_WORKERS, else up to 4 (each a WARP draw, itself on several cores)."""
    try:
        return max(1, int(os.environ.get('BFBC2_XDIFF_WORKERS', '')))
    except ValueError:
        return max(1, min(4, os.cpu_count() or 1))


def run_shader(code: bytes, p: int, seed: int, n: int, out: str, named: dict[int, str] | None,
               pixel: bool = True) -> dict[str, Any]:
    """One pixel (or vertex) shader through both sides: its result record (status, and the comparison or the reason).
    Writes <out>\\ps<p>\\ (vs<p>: the draw's inputs and output, ps.hlsl / vs.hlsl, ps.xenos.txt / vs.xenos.txt).
    Thread-safe."""
    kind = 'ps' if pixel else 'vs'
    rng = np.random.default_rng(seed * 100003 + (0 if pixel else VS_SEED) + p)
    work = os.path.join(out, f'{kind}{p}')
    os.makedirs(work, exist_ok=True)
    try:
        sh = xenos.parse_shader(code, pixel)
        cs = case(sh, rng, n, named)
        # translated before the interpreter reads the microcode: the program kept in Rust, as the builds translate
        # it (a tree whose microcode was read is handed to the translator attribute by attribute: the same text)
        hlsl: str | Exception
        try:
            hlsl = translated(sh, cs)
        except X.TranslateError as e:
            hlsl = e
        ref = XE.run(sh, cs['inputs'], cs['consts'], cs['textures'], n,
                     pos=np.stack([np.arange(n) % W, np.arange(n) // W], 1).astype(np.float32),
                     front=np.ones(n, bool))
    except XE.ExecError as e:
        return {'status': 'interpreter', 'why': str(e)}
    except Exception as e:                                      # noqa: BLE001 - reported per shader
        return {'status': 'error', 'why': f'{type(e).__name__}: {e}'[:300]}
    try:
        if isinstance(hlsl, Exception):
            raise hlsl
        if pixel:
            got, hlsl = run_hlsl(sh, cs, n, work, hlsl)
        else:
            vgot = run_hlsl_vs(sh, cs, n, work, hlsl)
        with open(os.path.join(work, f'{kind}.hlsl'), 'w') as f:
            f.write(hlsl)
        with open(os.path.join(work, f'{kind}.xenos.txt'), 'w') as f:
            f.write(xenos.disasm(sh))
        res = compare(ref, got) if pixel else compare_vs(ref, vgot, vs_slots(sh))
        if ref.jumps:                       # each jump's lanes that took it and that did not; the CFs never reached
            res['jumps'] = {str(k): v for k, v in sorted(ref.jumps.items())}
            res['unreached'] = ref.unreached
        return res
    except (X.TranslateError, P.CompileError) as e:
        return {'status': 'translate', 'why': str(e)[:300]}
    except Exception as e:                                      # noqa: BLE001
        return {'status': 'error', 'why': f'{type(e).__name__}: {e}'[:300]}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--db', default='wake')
    ap.add_argument('--ps', type=int, nargs='*')
    ap.add_argument('--vs', type=int, nargs='*', help='vertex shaders (no numbers: every one)')
    ap.add_argument('--n', type=int, default=1024)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--limit', type=int)
    ap.add_argument('--bisect', action='store_true', help='find the first differing instruction (one --ps)')
    ap.add_argument('-o', default=os.path.join(tempfile.gettempdir(), 'xdiff'))
    ap.add_argument('--workers', type=int, help='shaders at a time (default BFBC2_XDIFF_WORKERS, else up to 4)')
    a = ap.parse_args(argv)
    w = load_db(a.db)
    todo: list[tuple[bool, int]] = [(True, p) for p in (a.ps or ([] if a.vs is not None else range(len(w.ps))))]
    if a.vs is not None:
        todo += [(False, v) for v in (a.vs or range(len(w.vs)))]
    if a.limit:
        todo = todo[:a.limit]
    os.makedirs(a.o, exist_ok=True)
    named = engine_named(w)
    named_vs = engine_named(w, False) if a.vs is not None else {}
    if a.bisect:
        p = todo[0][1]
        sh = xenos.parse_shader(w.ps[p].code, True)
        cs = case(sh, np.random.default_rng(a.seed * 100003 + p), a.n, named.get(p))
        work = os.path.join(a.o, f'ps{p}')
        os.makedirs(work, exist_ok=True)
        pos = np.stack([np.arange(a.n) % W, np.arange(a.n) // W], 1).astype(np.float32)
        print(bisect(sh, cs, a.n, work, pos))
        return 0
    if len(todo) > 1:
        # every translation first, its compiles started on all cores (pcbuild.prefetch): the loop's compile_hlsl
        # takes them, or waits for one under way, while the interpreter runs (round E3d). The same seeds: the same
        # cases and translations as the loop's
        jobs: list[P.CompileJob] = []
        for pixel, p in todo:
            try:
                sh = xenos.parse_shader((w.ps if pixel else w.vs)[p].code, pixel)
                cs = case(sh, np.random.default_rng(a.seed * 100003 + (0 if pixel else VS_SEED) + p), a.n,
                          (named if pixel else named_vs).get(p))
                if pixel:
                    jobs += compile_jobs(sh, a.n, translated(sh, cs))
                else:
                    vsrc, psrc = vs_wrapper(translated(sh, cs), vs_slots(sh))
                    jobs += [P.CompileJob(vsrc, 'main', 'vs_5_0'), P.CompileJob(psrc, 'main', 'ps_5_0')]
            except Exception:                                   # noqa: BLE001 - the loop reports it
                pass
        P.prefetch(jobs)
    results: dict[int | str, Any] = {}
    counts: dict[str, int] = {}
    workers = max(1, min(a.workers or default_workers(), len(todo)))

    def one(item: tuple[bool, int]) -> dict[str, Any]:
        pixel, p = item
        if pixel:
            return run_shader(w.ps[p].code, p, a.seed, a.n, a.o, named.get(p))
        return run_shader(w.vs[p].code, p, a.seed, a.n, a.o, named_vs.get(p), False)
    # the shaders on worker threads (round E3l): each one's interpreter (numpy) and its draw (a shaderview -serve
    # process of its own, renderqa.run_job) overlap the others'; reported in order, every result the one alone
    if workers > 1:
        pool = concurrent.futures.ThreadPoolExecutor(workers)
        done: Iterable[dict[str, Any]] = pool.map(one, todo)
    else:
        pool, done = None, map(one, todo)
    for (pixel, p), res in zip(todo, done):
        results[p if pixel else f'vs{p}'] = res          # (a pixel shader's key its number, as before vertex shaders)
        counts[res['status']] = counts.get(res['status'], 0) + 1
        line = f'{"ps" if pixel else "vs"}{p}: {res["status"]}'
        if res['status'] == 'differs':
            line += f' {res["bad"]}/{res["pixels"]} px (kill mismatch {res["kill_mismatch"]}), max err {res.get("max_err")}'
        elif 'why' in res:
            line += f' - {res["why"][:160]}'
        print(line, flush=True)
    if pool is not None:
        pool.shutdown()
    with open(os.path.join(a.o, 'results.json'), 'w') as f:
        json.dump(results, f, indent=1)
    total = len(todo)
    exact = counts.get('exact', 0)
    what = 'pixel shaders' if a.vs is None else 'shaders'
    print(f'\n{exact} of {total} {what} exact ({100 * exact / max(total, 1):.1f} %): ' +
          ', '.join(f'{k} {v}' for k, v in sorted(counts.items())))
    return 0


if __name__ == '__main__':
    sys.exit(main())
