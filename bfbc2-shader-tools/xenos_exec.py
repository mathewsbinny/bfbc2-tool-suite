"""A reference interpreter for Xenos (Xbox 360) shader microcode, as decoded by xenos.py: runs a pixel shader on N
pixels at once (numpy, float32) from given inputs, constants and textures, and returns its outputs.

It is the other half of the translator's differential test (xdiff.py): the same inputs go through BF1943's original
microcode here and through xenos_hlsl's translation compiled for Direct3D 11, and the outputs are compared. The
semantics are written from Xenia's documentation of the instruction set (src/xenia/gpu/ucode.h, reference copy in the
session's xenia_ref), not from xenos_hlsl.py, so the two implementations check each other; both read the instructions
through xenos.py, so a decoding error is common to both (the Xenia trace replay is the check for that).

  run(sh, inputs, consts, textures, n) -> Result
    sh        xenos.parse_shader(...) of a pixel shader
    inputs    {interpolator register: (n, 4) float32} - what the PS finds in r0.. at entry (dcl_texcoord); the
              screen-position register (param gen) is filled from 'pos' when given ((n, 2) pixel positions)
    consts    {c register: (4,) floats} - the engine's and the material's values; the shader's own literals (def)
              are added
    textures  {fetch constant (sampler register): Texture}
Semantics notes (ucode.h):
  - 0 * x = 0 for every x (inf, NaN) in mul, mad, dp*, muls*, mulsc - the D3D9 rule the hardware follows
  - max / min: dest = a >= b ? a : b / a < b ? a : b (a NaN picks b)
  - vector and scalar halves of an ALU slot read the registers as they were before the slot; ps is the previous
    scalar result; every scalar op but retain_prev sets ps
  - predicated instructions / cond_exec_pred blocks run where p0 == condition; cond_jmp jumps where it holds
  - kills discard the pixel (its outputs stay as they were when it was killed; Result.alive marks the survivors)
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Any

import numpy as np

F = np.float32
FLT_MAX = np.float32(3.4028234663852886e38)
XYZW = 'xyzw'
# bilinear weights rounded to this many fraction bits (Direct3D's texture units: 8; 0 = exact); the Xbox's
# own precision is unknown, the differential test matches the Direct3D side it compares with
FILTER_BITS = 8
# classification aid (xdiff): point fetches read as Direct3D's Load reads (0 outside the texture)
ZERO_OUTSIDE = False


class ExecError(Exception):
    pass


@dataclass
class Texture:
    """A texture as the texture unit returns it: float32 texels (faces, h, w, 4) after format decoding (the
    differential test uses float textures, so filtering is the only difference between the two sides).
    address: 'wrap' | 'clamp'."""
    texels: np.ndarray
    cube: bool = False
    address: str = 'wrap'
    point: bool = False                                 # the fetch constant's filter: point (the shadow map's)

    @property
    def size(self) -> tuple[int, int]:
        return self.texels.shape[2], self.texels.shape[1]


@dataclass
class Result:
    outputs: dict[int, np.ndarray]                      # colour target -> (n, 4)
    alive: np.ndarray                                   # (n,) bool: not killed
    trace: list[str] = field(default_factory=list)
    snaps: dict[int, Any] | None = None


def _mul(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    with np.errstate(all='ignore'):
        r = (a * b).astype(F)
    return np.where((a == 0) | (b == 0), F(0), r)


def _sat(v: np.ndarray) -> np.ndarray:
    with np.errstate(invalid='ignore'):
        return np.where(np.isnan(v), F(0), np.clip(v, 0, 1)).astype(F)


class _Machine:
    def __init__(self, sh: Any, n: int, consts: dict[int, Any], textures: dict[int, Texture]) -> None:
        self.sh, self.n = sh, n
        temps = (sh.header.program_cntl >> 8 & 0x3f) + 1 if sh.is_pixel else (sh.header.program_cntl & 0x3f) + 1
        self.r = np.zeros((max(temps, 64), n, 4), F)
        self.c = np.zeros((256, 4), F)
        for k, v in consts.items():
            self.c[k] = np.asarray(v, F)[:4]
        for k, v in sh.literals.items():
            self.c[k] = np.asarray(v, F)[:4]
        self.ps = np.zeros(n, F)
        self.p0 = np.zeros(n, bool)
        self.a0 = np.zeros(n, np.int64)
        self.alive = np.ones(n, bool)
        self.out: dict[int, np.ndarray] = {}
        self.tex = textures
        self.tex_lod = np.zeros(n, F)
        self.last_mask = np.zeros(n, bool)
        self.snaps: dict[int, tuple[np.ndarray, np.ndarray, np.ndarray]] | None = None     # trace: address -> (registers, alive)

    # ------------------------------------------------------------------ operands
    def read(self, o: Any, mask: np.ndarray) -> np.ndarray:
        """(n, 4) operand value, swizzled, abs / negate applied."""
        if o.file == 'temp':
            if o.rel:
                raise ExecError('relative temp addressing')
            v = self.r[o.index]
        else:
            if o.rel == 'a0':
                idx = np.clip(o.index + self.a0, 0, 255)
                v = self.c[idx]
            elif o.rel:
                raise ExecError('loop-relative constant addressing')
            else:
                v = np.broadcast_to(self.c[o.index], (self.n, 4))
        sw = [XYZW.index(ch) for ch in o.swizzle]
        v = v[:, sw]
        if o.abs:
            v = np.abs(v)
        if o.negate:
            v = -v
        return v.astype(F)

    def scalar_in(self, ins: Any, mask: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        """(a, b) of a scalar op: one operand's two components (W, X), or c#.w and r#.x."""
        ops = ins.scalar_operands
        if len(ops) == 2:
            return self._comp(ops[0], ops[0].components, mask), self._comp(ops[1], ops[1].components, mask)
        if len(ops) == 1:
            o = ops[0]
            a = self._comp(o, o.components[0], mask)
            b = self._comp(o, o.components[1], mask) if len(o.components) > 1 else a
            return a, b
        z = np.zeros(self.n, F)
        return z, z

    def _comp(self, o: Any, ch: str, mask: np.ndarray) -> np.ndarray:
        if o.file == 'temp':
            v = self.r[o.index][:, XYZW.index(ch)]
        else:
            if o.rel == 'a0':
                v = self.c[np.clip(o.index + self.a0, 0, 255)][:, XYZW.index(ch)]
            elif o.rel:
                raise ExecError('loop-relative constant addressing')
            else:
                v = np.full(self.n, self.c[o.index][XYZW.index(ch)], F)
        if o.abs:
            v = np.abs(v)
        if o.negate:
            v = -v
        return v.astype(F)

    # ------------------------------------------------------------------ ALU
    def vector(self, ins: Any, mask: np.ndarray) -> tuple[np.ndarray | None, dict[str, Any]]:
        op = ins.vector_op
        a = [self.read(o, mask) for o in ins.vector_operands]
        fx: dict[str, Any] = {}
        with np.errstate(all='ignore'):
            if op == 'add':
                r = a[0] + a[1]
            elif op == 'mul':
                r = _mul(a[0], a[1])
            elif op == 'max':
                r = np.where(a[0] >= a[1], a[0], a[1])
            elif op == 'min':
                r = np.where(a[0] < a[1], a[0], a[1])
            elif op in ('seq', 'sgt', 'sge', 'sne'):
                cmp = {'seq': np.equal, 'sgt': np.greater, 'sge': np.greater_equal, 'sne': np.not_equal}[op]
                r = cmp(a[0], a[1]).astype(F)
            elif op == 'frc':
                r = a[0] - np.floor(a[0])
            elif op == 'trunc':
                r = np.trunc(a[0])
            elif op == 'floor':
                r = np.floor(a[0])
            elif op == 'mad':
                r = _mul(a[0], a[1]) + a[2]
            elif op in ('cndeq', 'cndge', 'cndgt'):
                cmp = {'cndeq': np.equal, 'cndge': np.greater_equal, 'cndgt': np.greater}[op]
                r = np.where(cmp(a[0], 0), a[1], a[2])
            elif op == 'dp4':
                r = np.repeat(_mul(a[0], a[1]).sum(1, dtype=F)[:, None], 4, 1)
            elif op == 'dp3':
                r = np.repeat(_mul(a[0][:, :3], a[1][:, :3]).sum(1, dtype=F)[:, None], 4, 1)
            elif op == 'dp2add':
                r = np.repeat((_mul(a[0][:, :2], a[1][:, :2]).sum(1, dtype=F) + a[2][:, 0])[:, None], 4, 1)
            elif op == 'cube':
                r = self._cube(a[0], a[1])
            elif op == 'max4':
                r = np.repeat(a[0].max(1)[:, None], 4, 1)
            elif op.startswith('setp_') and op.endswith('_push'):
                cmp = {'eq': np.equal, 'ne': np.not_equal, 'gt': np.greater, 'ge': np.greater_equal}[op[5:7]]
                fx['p0'] = (a[0][:, 3] == 0) & cmp(a[1][:, 3], 0)
                r = np.where(((a[0][:, 0] == 0) & cmp(a[1][:, 0], 0))[:, None], F(0), a[0][:, 0:1] + 1)
                r = np.repeat(r.reshape(-1, 1), 4, 1)
            elif op.startswith('kill_'):
                cmp = {'eq': np.equal, 'gt': np.greater, 'ge': np.greater_equal, 'ne': np.not_equal}[op[5:]]
                k = cmp(a[0], a[1]).any(1)
                fx['kill'] = k
                r = np.repeat(k.astype(F)[:, None], 4, 1)
            elif op == 'dst':
                r = np.stack([np.ones(self.n, F), _mul(a[0][:, 1], a[1][:, 1]), a[0][:, 2], a[1][:, 3]], 1)
            elif op == 'maxa':
                fx['a0'] = np.clip(np.floor(a[0][:, 3] + 0.5), -256, 255).astype(np.int64)
                r = np.where(a[0] >= a[1], a[0], a[1])
            else:
                raise ExecError(f'vector op {op}')
        return r.astype(F), fx

    @staticmethod
    def _cube(s0: np.ndarray, s1: np.ndarray) -> np.ndarray:
        # cube r, src.zzxy, src.yxzz: the direction is (s1.y, s1.x... ) per Xenia: x = src0.z? - use the source
        # direction as the canonical (x, y, z) = (s1.y, s0.z?...) - Xenia: "Expects src0.zzxy and src1.yxzz", so
        # src0 = (z, z, x, y) and src1 = (y, x, z, z) of the direction: x = src0.z, y = src0.w, z = src0.x
        x, y, z = s0[:, 2], s0[:, 3], s0[:, 0]
        ax, ay, az = np.abs(x), np.abs(y), np.abs(z)
        zmaj = (az >= ax) & (az >= ay)
        ymaj = ~zmaj & (ay >= ax)
        tc = np.where(zmaj, -y, np.where(ymaj, np.where(y < 0, -z, z), -y))
        sc = np.where(zmaj, np.where(z < 0, -x, x), np.where(ymaj, x, np.where(x < 0, z, -z)))
        ma = np.where(zmaj, 2 * z, np.where(ymaj, 2 * y, 2 * x))
        fid = np.where(zmaj, np.where(z < 0, 5, 4), np.where(ymaj, np.where(y < 0, 3, 2), np.where(x < 0, 1, 0)))
        return np.stack([tc, sc, ma, fid.astype(F)], 1).astype(F)

    def scalar(self, ins: Any, mask: np.ndarray) -> tuple[np.ndarray | None, dict[str, Any]]:
        op = ins.scalar_op
        a, b = self.scalar_in(ins, mask)
        ps = self.ps
        fx: dict[str, Any] = {}
        one, zero = F(1), F(0)
        with np.errstate(all='ignore'):
            if op in ('adds', 'addsc'):
                r = a + b
            elif op == 'adds_prev':
                r = a + ps
            elif op in ('muls', 'mulsc'):
                r = _mul(a, b)
            elif op == 'muls_prev':
                r = _mul(a, ps)
            elif op == 'muls_prev2':
                bad = (ps == -FLT_MAX) | ~np.isfinite(ps) | ~np.isfinite(b) | (b <= 0)
                r = np.where(bad, -FLT_MAX, _mul(a, ps))
            elif op == 'maxs':
                r = np.where(a >= b, a, b)
            elif op == 'mins':
                r = np.where(a < b, a, b)
            elif op in ('seqs', 'sgts', 'sges', 'snes'):
                cmp = {'seqs': np.equal, 'sgts': np.greater, 'sges': np.greater_equal, 'snes': np.not_equal}[op]
                r = cmp(a, 0).astype(F)
            elif op == 'frcs':
                r = a - np.floor(a)
            elif op == 'truncs':
                r = np.trunc(a)
            elif op == 'floors':
                r = np.floor(a)
            elif op == 'exp':
                r = np.where(a == 0, one, np.exp2(a))
            elif op in ('log', 'logc'):
                r = np.where(a == 1, zero, np.log2(a))
                if op == 'logc':
                    r = np.where(r == -np.inf, -FLT_MAX, r)
            elif op in ('rcp', 'rcpc', 'rcpf'):
                r = np.where(a == 1, one, one / a)
                if op == 'rcpc':
                    r = np.clip(r, -FLT_MAX, FLT_MAX)
                elif op == 'rcpf':
                    r = np.where(np.isinf(r), np.copysign(zero, r), r)
            elif op in ('rsq', 'rsqc', 'rsqf'):
                r = np.where(a == 1, one, one / np.sqrt(a))
                if op == 'rsqc':
                    r = np.clip(r, -FLT_MAX, FLT_MAX)
                elif op == 'rsqf':
                    r = np.where(np.isinf(r), np.copysign(zero, r), r)
            elif op in ('maxas', 'maxasf'):
                fx['a0'] = np.clip(np.floor(a + (0.5 if op == 'maxas' else 0.0)), -256, 255).astype(np.int64)
                r = np.where(a >= b, a, b)
            elif op in ('subs', 'subsc'):
                r = a - b
            elif op == 'subs_prev':
                r = a - ps
            elif op in ('setp_eq', 'setp_ne', 'setp_gt', 'setp_ge'):
                cmp = {'setp_eq': np.equal, 'setp_ne': np.not_equal, 'setp_gt': np.greater,
                       'setp_ge': np.greater_equal}[op]
                c = cmp(a, 0)
                fx['p0'] = c
                r = np.where(c, zero, one)
            elif op == 'setp_inv':
                c = a == 1
                fx['p0'] = c
                r = np.where(c, zero, np.where(a == 0, one, a))
            elif op == 'setp_pop':
                c = a - 1 <= 0
                fx['p0'] = c
                r = np.where(c, zero, a - 1)
            elif op == 'setp_clr':
                fx['p0'] = np.zeros(self.n, bool)
                r = np.full(self.n, FLT_MAX)
            elif op == 'setp_rstr':
                c = a == 0
                fx['p0'] = c
                r = np.where(c, zero, a)
            elif op.startswith('kills_'):
                cmp = {'eq': lambda v: v == 0, 'gt': lambda v: v > 0, 'ge': lambda v: v >= 0,
                       'ne': lambda v: v != 0, 'one': lambda v: v == 1}[op[6:]]
                k = cmp(a)
                fx['kill'] = k
                r = k.astype(F)
            elif op == 'sqrt':
                r = np.sqrt(a)
            elif op == 'sin':
                r = np.sin(a)
            elif op == 'cos':
                r = np.cos(a)
            elif op == 'retain_prev':
                return ps.copy(), {'keep_ps': True}
            else:
                raise ExecError(f'scalar op {op}')
        return np.asarray(r, F), fx

    def write(self, dest: Any, val: np.ndarray, mask: np.ndarray, scalar: bool) -> None:
        """val: (n, 4) (a scalar result is replicated)."""
        if dest.saturate:
            val = _sat(val)
        comps = dest.write
        if not comps.strip('_'):
            return
        if dest.file == 'temp':
            if dest.rel:
                raise ExecError('relative destination')
            tgt = self.r[dest.index]
        elif dest.file == 'oC':
            tgt = self.out.setdefault(dest.index, np.zeros((self.n, 4), F))
        else:
            return                                      # oDepth, memory exports: not compared
        for k, ch in enumerate(comps):
            if ch == '_':
                continue
            v = F(0) if ch == '0' else F(1) if ch == '1' else None
            if v is None:
                col = val[:, k]
            else:
                col = np.full(self.n, v, F)
            tgt[mask, k] = col[mask]

    def alu(self, ins: Any, mask: np.ndarray) -> None:
        if ins.predicated:
            mask = mask & (self.p0 == ins.pred_condition)
        self.last_mask = mask
        if not mask.any():
            return
        vres, vfx = (None, {}) if ins.vector_nop else self.vector(ins, mask)
        sres, sfx = self.scalar(ins, mask) if not ins.scalar_nop else (None, {'keep_ps': True})
        if vres is not None:
            self.write(ins.vector_dest, vres, mask, False)
        if sres is not None:
            self.write(ins.scalar_dest, np.repeat(sres[:, None], 4, 1), mask, True)
            if not sfx.get('keep_ps'):
                self.ps = np.where(mask, sres, self.ps).astype(F)
        for fx in (vfx, sfx):
            if 'p0' in fx:
                self.p0 = np.where(mask, fx['p0'], self.p0)
            if 'a0' in fx:
                self.a0 = np.where(mask, fx['a0'], self.a0)
            if 'kill' in fx:
                self.alive &= ~(mask & fx['kill'])

    # ------------------------------------------------------------------ fetch
    def fetch(self, ins: Any, mask: np.ndarray) -> None:
        if ins.predicated:
            mask = mask & (self.p0 == ins.pred_condition)
        self.last_mask = mask
        if not mask.any():
            return
        op = ins.op
        if op == 'setTexLOD':
            self.tex_lod = np.where(mask, self.r[ins.src][:, XYZW.index(ins.src_swizzle[0])], self.tex_lod).astype(F)
            return
        if not getattr(ins, 'has_dest', True):
            return
        t = self.tex.get(ins.const_index)
        if t is None:
            raise ExecError(f'no texture for tf{ins.const_index}')
        src = self.r[ins.src][:, [XYZW.index(ch) for ch in ins.src_swizzle]]
        w, h = t.size
        off = np.array([ins.offset_x, ins.offset_y], F)
        if op.startswith('getWeights'):
            fr = src[:, :2] * np.array([w, h], F) - 0.5 + off
            val = np.concatenate([fr - np.floor(fr), np.zeros((self.n, 2), F)], 1)
        elif op == 'tfetchCube':
            val = self._sample_cube(t, src)
        elif op == 'tfetch2D':
            point = ins.mag_filter == 'point' and ins.min_filter == 'point'
            if point and ZERO_OUTSIDE and t.address == 'clamp':     # classify: Direct3D Load's 0 outside
                t = Texture(t.texels, t.cube, 'zero', True)
            val = self._sample2d(t, src[:, :2], off, point or t.point)
        else:
            raise ExecError(f'fetch {op}')
        reg = self.r[ins.dst]
        for k, ch in enumerate(ins.dst_swizzle):
            if ch in '_?':
                continue
            col = np.full(self.n, F(0 if ch == '0' else 1), F) if ch in '01' else val[:, XYZW.index(ch)]
            reg[mask, k] = col[mask]

    @staticmethod
    def _sample2d(t: Texture, uv: np.ndarray, off: np.ndarray, point: bool) -> np.ndarray:
        img = t.texels[0]
        h, w = img.shape[:2]
        x = uv[:, 0] * w + off[0] - (0 if point else 0.5)
        y = uv[:, 1] * h + off[1] - (0 if point else 0.5)

        def at(ix: np.ndarray, iy: np.ndarray) -> np.ndarray:
            if t.address == 'zero':                        # Direct3D Load: 0 outside the texture
                inside = (ix >= 0) & (ix < w) & (iy >= 0) & (iy < h)
                return np.where(inside[:, None], img[np.clip(iy, 0, h - 1), np.clip(ix, 0, w - 1)], 0)
            if t.address == 'clamp':
                ix, iy = np.clip(ix, 0, w - 1), np.clip(iy, 0, h - 1)
            else:
                ix, iy = np.mod(ix, w), np.mod(iy, h)
            return img[iy, ix]
        if point:
            return at(np.floor(x).astype(np.int64), np.floor(y).astype(np.int64)).astype(F)
        x0, y0 = np.floor(x), np.floor(y)
        fx, fy = (x - x0)[:, None], (y - y0)[:, None]
        if FILTER_BITS:                                 # the filter weights' fixed-point precision
            q = F(1 << FILTER_BITS)
            fx, fy = np.round(fx * q) / q, np.round(fy * q) / q
        x0, y0 = x0.astype(np.int64), y0.astype(np.int64)
        return ((at(x0, y0) * (1 - fx) + at(x0 + 1, y0) * fx) * (1 - fy)
                + (at(x0, y0 + 1) * (1 - fx) + at(x0 + 1, y0 + 1) * fx) * fy).astype(F)

    @staticmethod
    def _sample_cube(t: Texture, src: np.ndarray) -> np.ndarray:
        # tfetchCube's (s, t, face) from the cube op sequence (ST in 1..2 after the mad 1.5): face index -> texel
        s, tt, face = src[:, 0] - 1, src[:, 1] - 1, np.clip(np.round(src[:, 2]), 0, 5).astype(np.int64)
        out = np.zeros((len(s), 4), F)
        for f in range(t.texels.shape[0]):
            m = face == f
            if m.any():
                sub = Texture(t.texels[f:f + 1], False, 'clamp')
                out[m] = _Machine._sample2d(sub, np.stack([s[m], tt[m]], 1), np.zeros(2, F), t.point)
        return out

    # ------------------------------------------------------------------ control flow
    def run(self) -> None:
        cfs = self.sh.cf
        by_index = {c.index: k for k, c in enumerate(cfs)}
        pc = np.zeros(self.n, np.int64)                  # per pixel: position in cfs
        done = np.zeros(self.n, bool)
        guard = 0
        while not done.all():
            guard += 1
            if guard > 100000:
                raise ExecError('control flow does not terminate')
            k = int(pc[~done].min())
            here = ~done & (pc == k)
            if k >= len(cfs):
                done |= here
                continue
            c = cfs[k]
            op = c.op
            nxt = k + 1
            if op in ('exec', 'exec_end', 'cond_exec', 'cond_exec_end', 'cond_exec_pred', 'cond_exec_pred_end',
                      'cond_exec_pred_clean', 'cond_exec_pred_clean_end'):
                m = here.copy()
                if op.startswith('cond_exec_pred'):
                    m &= self.p0 == bool(c.condition)
                elif op.startswith('cond_exec'):
                    if not self._bool_const(c):
                        m[:] = False
                for ins in c.instructions or []:
                    if ins.kind == 'alu':
                        self.alu(ins, m)
                    else:
                        self.fetch(ins, m)
                    if self.snaps is not None:
                        self.snaps[ins.address] = (self.r.copy(), self.alive.copy(), self.last_mask.copy())
                pc[here] = nxt
                if c.is_end:
                    done |= here
            elif op == 'cond_jmp':
                if c.type == 'predicated':
                    take = here & (self.p0 == bool(c.condition))
                elif c.type == 'unconditional':
                    take = here.copy()
                else:
                    take = here & self._bool_const(c)
                tgt = by_index.get(c.address)              # a CF instruction index (two per CF slot)
                if tgt is None:
                    raise ExecError(f'jump to CF {c.address}: no such instruction')
                pc[take] = tgt
                pc[here & ~take] = nxt
            elif op in ('nop', 'alloc', 'mark_vs_fetch_done'):
                pc[here] = nxt
            else:
                raise ExecError(f'control flow {op}')

    def _bool_const(self, c: Any) -> bool:
        raise ExecError('bool-constant conditions (none in BF1943) are not supported')


def run(sh: Any, inputs: dict[int, np.ndarray], consts: dict[int, Any], textures: dict[int, Texture], n: int,
        pos: np.ndarray | None = None, front: np.ndarray | None = None, trace: bool = False) -> Result:
    """trace: Result.snaps holds the registers and the live pixels after every instruction (by address)."""
    if not sh.is_pixel:
        raise ExecError('pixel shaders only (vertex shaders: not yet)')
    m = _Machine(sh, n, consts, textures)
    if trace:
        m.snaps = {}
    for it in sh.header.interpolators:
        v = inputs.get(it.reg)
        if v is not None:
            m.r[it.reg] = np.asarray(v, F)
    cm = sh.header.context_misc_fields
    if cm.sc_output_screen_xy:
        g = np.zeros((n, 4), F)
        if pos is not None:
            g[:, :2] = np.floor(pos)
        if front is not None:
            g[:, 0] = np.where(front, g[:, 0], -np.abs(g[:, 0]) - (g[:, 0] == 0) * F(0))
            neg0 = ~front & (g[:, 0] == 0)
            g[neg0, 0] = F(-0.0)
        m.r[cm.param_gen_pos] = g
    m.run()
    outs = [k for k in range(4) if sh.header.outputs >> k & 1]
    res = Result({k: m.out.get(k, np.zeros((n, 4), F)) for k in outs}, m.alive)
    res.snaps = m.snaps
    return res
