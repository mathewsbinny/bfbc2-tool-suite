"""The shader translator's differential test: every BF1943 pixel shader run twice on the same inputs - its original Xenos
microcode in xenos_exec.py (the reference) and xenos_hlsl's translation compiled for Direct3D 11 and drawn by
shaderview.exe (WARP) - and the outputs compared pixel by pixel.

  python xdiff.py [--db wake] [--ps 317 ...] [--n 1024] [--seed 1] [-o out_dir] [--limit K]

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
import json
import os
import re
import struct
import sys
import tempfile
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


def case(sh: Any, rng: np.random.Generator, n: int) -> dict[str, Any]:
    """Random inputs for one shader: engine values, material values, interpolants, textures."""
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
    inputs = {it.reg: rng.uniform(-1, 1, (n, 4)).astype(np.float32) for it in sh.header.interpolators}
    textures = {}
    for c in ct:
        if c.regset == 'sampler':
            cube = c.type.type == 'samplerCube'
            textures[c.reg] = XE.Texture(ramp_texture(rng, 6 if cube else 1), cube,
                                         'zero' if (SHADOW_ZERO and c.name == X.SHADOW) else 'clamp', True)
    return {'engine': engine, 'material': material, 'consts': consts, 'inputs': inputs, 'textures': textures}


def translated(sh: Any, cs: dict[str, Any]) -> str:
    inter = sorted(it.usage_index for it in sh.header.interpolators)
    return X.translate(sh, X.Interface(inter, material=cs['material']))


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


def run_hlsl(sh: Any, cs: dict[str, Any], n: int, work: str, hlsl: str | None = None) -> tuple[np.ndarray, str]:
    """oC0 of the translated shader (or of the given HLSL) for every pixel ((n, 4); SENTINEL where discarded)."""
    inter = sorted(it.usage_index for it in sh.header.interpolators)
    hlsl = hlsl if hlsl is not None else translated(sh, cs)
    ps = P.compile_hlsl(hlsl, 'main', 'ps_5_0')
    h = (n + W - 1) // W
    vin = ' '.join(f'float4 i{k} : TEXCOORD{k + 1};' for k in range(len(inter)))
    vout = ' '.join(f'float4 t{u} : TEXCOORD{u};' for u in inter)
    copy = ' '.join(f'o.t{u} = i.i{k};' for k, u in enumerate(inter))
    vs = P.compile_hlsl(VS_SRC % {'vin': vin, 'vout': vout, 'copy': copy, 'w': W, 'h': h}, 'main', 'vs_5_0')
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
    files = {}

    def put(name: str, data: bytes) -> str:
        p = os.path.join(work, name)
        with open(p, 'wb') as f:
            f.write(data)
        files[name] = p
        return p
    job = [f'frame {put("out.raw", b"")} {W} {h} {SENTINEL} {SENTINEL} {SENTINEL} {SENTINEL}',
           f'vs {put("vs.dxbc", vs)}', f'ps {put("ps.dxbc", ps)}']
    job += [f'el {k} 2 0 {16 * k} 0' for k in range(1 + len(inter))]
    job.append(f'vb {put("vb.bin", verts.tobytes())} 0 {stride}')
    ib = np.arange(3 * n, dtype=np.uint32)
    job.append(f'ib {put("ib.bin", ib.tobytes())} 0 32 {3 * n} 0')
    job.append(f'cb ps {put("cb.bin", RQ.cbuffer(ps, cs["engine"], []))}')
    for reg, t in cs['textures'].items():
        tx = t.texels
        hdr = struct.pack('<III', tx.shape[2], tx.shape[1], tx.shape[0])
        job.append(f'tex {reg} {"cube" if t.cube else "2d"} {put(f"t{reg}.bin", hdr + tx.tobytes())}')
    job.append(f'samp 1 {sampler_desc()}')
    job += ['draw', 'end']
    jp = put('job.txt', '\n'.join(job).encode())
    RQ.run_job(jp, warp=True)
    img = np.fromfile(files['out.raw'], np.float32).reshape(-1, 4)
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
    close = np.where(finite, np.abs(a - b) <= ATOL + RTOL * np.abs(a), np.isnan(a) == np.isnan(b))
    okb = close.all(1)
    ok[np.where(both)[0]] = okb
    bad = np.where(~ok)[0]
    res = {'status': 'exact' if not len(bad) else 'differs', 'pixels': int(len(got)),
           'bad': int(len(bad)), 'killed_ref': int((~alive).sum()), 'kill_mismatch': int(kill_mismatch.sum())}
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


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--db', default='wake')
    ap.add_argument('--ps', type=int, nargs='*')
    ap.add_argument('--n', type=int, default=1024)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--limit', type=int)
    ap.add_argument('--bisect', action='store_true', help='find the first differing instruction (one --ps)')
    ap.add_argument('-o', default=os.path.join(tempfile.gettempdir(), 'xdiff'))
    a = ap.parse_args(argv)
    import bf1943import
    src = bf1943import.Source(bf1943import.default_source())
    w = S.ShaderDb.parse(src.data(DBS[a.db]), where=a.db)
    todo = a.ps if a.ps else list(range(len(w.ps)))
    if a.limit:
        todo = todo[:a.limit]
    os.makedirs(a.o, exist_ok=True)
    if a.bisect:
        p = todo[0]
        sh = xenos.parse_shader(w.ps[p].code, True)
        cs = case(sh, np.random.default_rng(a.seed * 100003 + p), a.n)
        work = os.path.join(a.o, f'ps{p}')
        os.makedirs(work, exist_ok=True)
        pos = np.stack([np.arange(a.n) % W, np.arange(a.n) // W], 1).astype(np.float32)
        print(bisect(sh, cs, a.n, work, pos))
        return 0
    results = {}
    counts: dict[str, int] = {}
    for p in todo:
        rng = np.random.default_rng(a.seed * 100003 + p)
        work = os.path.join(a.o, f'ps{p}')
        os.makedirs(work, exist_ok=True)
        try:
            sh = xenos.parse_shader(w.ps[p].code, True)
            cs = case(sh, rng, a.n)
            ref = XE.run(sh, cs['inputs'], cs['consts'], cs['textures'], a.n,
                         pos=np.stack([np.arange(a.n) % W, np.arange(a.n) // W], 1).astype(np.float32),
                         front=np.ones(a.n, bool))
        except XE.ExecError as e:
            res = {'status': 'interpreter', 'why': str(e)}
        except Exception as e:                                  # noqa: BLE001 - reported per shader
            res = {'status': 'error', 'why': f'{type(e).__name__}: {e}'[:300]}
        else:
            try:
                got, hlsl = run_hlsl(sh, cs, a.n, work)
                with open(os.path.join(work, 'ps.hlsl'), 'w') as f:
                    f.write(hlsl)
                with open(os.path.join(work, 'ps.xenos.txt'), 'w') as f:
                    f.write(xenos.disasm(sh))
                res = compare(ref, got)
            except (X.TranslateError, P.CompileError) as e:
                res = {'status': 'translate', 'why': str(e)[:300]}
            except Exception as e:                              # noqa: BLE001
                res = {'status': 'error', 'why': f'{type(e).__name__}: {e}'[:300]}
        results[p] = res
        counts[res['status']] = counts.get(res['status'], 0) + 1
        line = f'ps{p}: {res["status"]}'
        if res['status'] == 'differs':
            line += f' {res["bad"]}/{res["pixels"]} px (kill mismatch {res["kill_mismatch"]}), max err {res.get("max_err")}'
        elif 'why' in res:
            line += f' - {res["why"][:160]}'
        print(line, flush=True)
    with open(os.path.join(a.o, 'results.json'), 'w') as f:
        json.dump(results, f, indent=1)
    total = len(todo)
    exact = counts.get('exact', 0)
    print(f'\n{exact} of {total} pixel shaders exact ({100 * exact / max(total, 1):.1f} %): ' +
          ', '.join(f'{k} {v}' for k, v in sorted(counts.items())))
    return 0


if __name__ == '__main__':
    sys.exit(main())
