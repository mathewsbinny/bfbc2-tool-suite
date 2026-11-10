r"""texqa.py - offline QA of converted textures: every PC texture a build wrote, decoded with all its levels to RGBA and
compared with its Xbox 360 source (Battlefield 1943's, or a console BFBC2 folder) and, where BFBC2 ships a PC copy of
the same name, with DICE's copy.

    python texqa.py <folder or .res> ... [-o <out folder>] [--bf1943 <dist\xenon> | --no-bf1943] [--console <folder>]
                    [--opfor <folder> | --no-dice] [--cache <folder>] [--worst N] [--skip <dir name>]...
                    [--baseline <json>] [--write-baseline <json>] [--threshold <dB>]

Inputs: folders are walked for PC ITexture resources - loose .res files recognised by their header (92 bytes, version
3, level sizes that add up to the file), and in unpacked bundles (a folder with FbrbTool's fbrb.manifest) the entries
typed ITexture, on-demand wrapped ones ('RES\x02') unwrapped. A texture's name is its path under the bundle folder (or
under the folder given); the source and DICE's copy are found by that name, and by the name with leading folders
dropped (work\ww2\bf1943\Objects\... is objects/...). <name>.res.orig beside a texture (OPFOR's grime.py keeps the
tool's own output there before weathering it) is compared as well: an intentional repaint is reported as such.

Per texture (row in texqa.json; the text report lists the flagged ones, worst first):
  levels            every level of ours and of the source decoded to RGBA (DXT1 with its 1-bit alpha, DXT3, DXT5,
                    BC5 / DXN as red and green, 8-bit L8, RGB565, BGRA8, RGBA16F); per level PSNR (dB, the channels the
                    formats hold), an SSIM on luminance (8 x 8 windows), the largest error, the per-channel means
  checks            dimensions and aspect; level count and sizes (each level's bytes against its format); each level
                    against its box-filtered parent (against the source's own chain where there is one); channel order
                    (every permutation of the colour channels, BC5's two swapped) and orientation (flipped, mirrored,
                    rotated, transposed) against the source and against DICE's copy - a better match than as-is is a
                    swizzle or a flip; alpha present in the source and lost, or an alpha format with no alpha used;
                    the sRGB bit (flags bit 0) against the source, DICE's copy and the role; the format against DICE's
                    formats for the role (ROLE_FORMATS); normal maps: where z is (blue, DXT1 18) or that red / green
                    are a unit-disc x / y for BC5 (z = sqrt(1 - x^2 - y^2), positive, as BFBC2's shaders rebuild it)
  degenerate        all black, all white, all transparent, one constant colour, NaN / infinity (RGBA16F), blocks of
                    zero bytes the source does not have
  DICE's copy       identical; the known differences reported as such (FORMAT.md 2.1: DICE's PC copy twice the
                    console's size with flags bit 1; flags bit 1 on a same-size re-export); format; the picture at the
                    smaller size (PSNR) - under 20 dB with a low correlation is DICE's own repaint / other art
Each flag has a severity (error, warn, info) and a cause: converter (ours differs from the source where it should not),
source (the source has it too), dice (a known DICE difference), intentional (grime.py's .orig: the repaint is ours on
purpose), or unknown.

Outputs (-o, default .\texqa_out): texqa.json, texqa.txt, sheets\worst_NN.png - contact sheets of the worst N (source
| ours | DICE's | difference heat map | ours' alpha). --cache keeps the rows by the sha1 of ours and the source's and
DICE's identities (bundle, size, date, entry): unchanged textures are not decoded again.
--write-baseline stores {texture: PSNR to source / DICE, flags}; --baseline compares with one: a new error flag (a
degenerate texture, a flip, a swizzle ...) or a PSNR drop over --threshold (default 1 dB) is a regression, exit code 1.
"""
import argparse
import hashlib
import json
import os
import re
import struct
import sys
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass, field
from typing import Any

import numpy as np

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'model-tools', 'core')
import texture  # noqa: E402
from bfbc2core import errors, fbrb  # noqa: E402

VERSION = 1                 # of the rows: a cache written by another version is not used
with open(__file__, 'rb') as _fh:   # ... nor one written by other code (the checks changed: every row again)
    CODE = hashlib.sha1(_fh.read().replace(b'\r\n', b'\n')).hexdigest()[:12]
HEADER = texture.HEADER
CUBE = texture.CUBE
SRGB, HALVED = 1, texture.HALVED_ON_CONSOLE
WRAPPED = b'RES\x02'        # the on-demand bundles' form (texwrite.wrapped): 64 bytes of head
# bytes per 4 x 4 block (block formats) or per texel
BLOCK = {0: 8, 18: 8, 1: 16, 2: 16, 19: 16, 20: 16, 3: 8}
TEXEL = {10: 1, 5: 2, 9: 4, 13: 8}
# the channels a format holds (A of DXT1: only when a block uses its 1-bit alpha)
CHANNELS = {0: 'rgb', 18: 'rgb', 1: 'rgba', 2: 'rgba', 19: 'rg', 20: 'rg', 3: 'r', 10: 'r', 5: 'rgb', 9: 'rgba',
            13: 'rgba'}
NAMES = {0: 'DXT1', 18: 'DXT1 normal', 1: 'DXT3', 2: 'DXT5', 19: 'BC5', 20: 'BC5 (20)', 3: 'DXT5A', 10: 'L8',
         5: 'RGB565', 9: 'BGRA8', 13: 'RGBA16F'}
# DICE's PC formats by role (format: share), measured 2026-11-09 on 1,135 stock textures (async, the *_common levels,
# mp_005, mp_001, mp_008, nam_mp_005cq) with role() below; and the share of each role's with the sRGB bit
ROLE_FORMATS = {'normal': {18: 0.88, 19: 0.10, 20: 0.01}, 'color': {0: 0.64, 2: 0.30, 1: 0.03, 10: 0.01, 9: 0.01},
                'specular': {0: 0.87, 2: 0.05, 9: 0.04, 10: 0.03}, 'mask': {0: 0.80, 10: 0.06, 2: 0.05, 18: 0.05,
                                                                            19: 0.03},
                'ui': {2: 0.93, 0: 0.06, 9: 0.01}}
ROLE_SRGB = {'normal': 0.0, 'color': 0.97, 'specular': 0.88, 'ui': 1.0}
ROLE_TOKENS = [('normal', {'n', 'nm', 'nrm', 'normal', 'normals', 'norm', 'dxn', 'bump', 'normalmap'}),
               ('specular', {'s', 'spec', 'specular', 'sm', 'sp', 'gloss'}),
               ('mask', {'m', 'a', 'ao', 'mask', 'alpha', 'occ', 'oc', 'op', 'opacity', 'sss'}),
               ('color', {'d', 'cm', 'c', 'col', 'color', 'colour', 'diffuse', 'df', 'dif', 'alb', 'albedo', 'rgb'})]
# textures a build composes from several sources (emitteratlas.py's merged particle atlases): the source of the same
# name is one of their inputs, not their source - no source compared, no normal-map statistics (their empty BC5
# blocks are zero, as DICE's are); emitteratlas.prove checks them image by image
COMPOSITE = re.compile(r'(^|/)levels/[^/]+/emittertexture(alpha|normals)\.res$')
PSNR_SAME = 35.0            # dB: the same picture (portparity's bar)
PSNR_OTHER = 20.0           # dB: other art
BETTER = 6.0                # dB: a permutation / flip this much better than as-is is a swizzle / flip
CHAIN_TOL = (3.0, 0.25)     # a level's error against its box-filtered parent: the source's + 3, or + 25 % (FORMAT.md 2.3)
CHAIN_ABS = 24.0            # ... without a source: a mean error this large (of 255) is not a mip of the level above
CHAIN_CORR = 0.75           # ... and only with a correlation under this (re-encoded noisy pictures: 0.87 and over)
ERROR, WARN, INFO = 'error', 'warn', 'info'


class TexQaError(Exception):
    what_to_do = ('Give folders or .res files of PC textures (a build output, an unpacked bundle); texqa.py --help '
                  'shows the options.')


# ------------------------------------------------------------------------------------------------ decoding

@dataclass
class Tex:
    """A texture decoded: header values and every level as (h, w, 4) uint8 RGBA; floats for RGBA16F."""
    fmt: int
    flags: int
    typ: int
    w: int
    h: int
    mips: int
    levels: list[tuple[int, int, np.ndarray]]               # (slice, level, rgba)
    floats: list[np.ndarray] = field(default_factory=list)  # RGBA16F's levels as float32
    size_errors: list[str] = field(default_factory=list)
    zero_blocks: float = 0.0                                # share of level 0's blocks that are all zero bytes

    def level(self, m: int, s: int = 0) -> np.ndarray | None:
        for sl, lv, img in self.levels:
            if sl == s and lv == m:
                return img
        return None

    @property
    def channels(self) -> str:
        c = CHANNELS.get(self.fmt, 'rgba')
        if self.fmt in (0, 18) and self.levels and int(self.levels[0][2][..., 3].min()) < 255:
            c += 'a'
        return c


def _color(blk: np.ndarray, force4: bool) -> tuple[np.ndarray, np.ndarray]:
    """(n, 8) DXT colour halves -> (n, 16, 3) int RGB and (n, 16) alpha (0 where three-colour mode's index 3)."""
    c0 = blk[:, 0].astype(np.int32) | (blk[:, 1].astype(np.int32) << 8)
    c1 = blk[:, 2].astype(np.int32) | (blk[:, 3].astype(np.int32) << 8)

    def rgb(v: np.ndarray) -> np.ndarray:
        return np.stack([((v >> 11) & 31) * 255 // 31, ((v >> 5) & 63) * 255 // 63, (v & 31) * 255 // 31], -1)
    p0, p1 = rgb(c0), rgb(c1)
    four = (c0 > c1) | force4
    pal = np.stack([p0, p1, np.where(four[:, None], (2 * p0 + p1) // 3, (p0 + p1) // 2),
                    np.where(four[:, None], (p0 + 2 * p1) // 3, 0)], 1)
    b4 = blk[:, 4:8].astype(np.uint32)
    bits = b4[:, 0] | b4[:, 1] << 8 | b4[:, 2] << 16 | b4[:, 3] << 24
    idx = np.stack([(bits >> (2 * k)) & 3 for k in range(16)], 1).astype(np.int64)
    out = np.take_along_axis(pal, idx[:, :, None].repeat(3, 2), 1)
    alpha = np.where((idx == 3) & ~four[:, None], 0, 255)
    return out, alpha


def _alpha(a: np.ndarray) -> np.ndarray:
    """(n, 8) DXT5 alpha halves (also BC5's channels, DXT5A) -> (n, 16) values."""
    a0, a1 = a[:, 0].astype(np.int32), a[:, 1].astype(np.int32)
    big = a0 > a1
    pal = np.zeros((len(a), 8), np.int32)
    pal[:, 0], pal[:, 1] = a0, a1
    for k in range(1, 7):
        pal[:, k + 1] = ((7 - k) * a0 + k * a1) // 7
    for k in range(1, 5):
        pal[:, k + 1] = np.where(big, pal[:, k + 1], ((5 - k) * a0 + k * a1) // 5)
    pal[:, 6] = np.where(big, pal[:, 6], 0)
    pal[:, 7] = np.where(big, pal[:, 7], 255)
    bits = np.zeros(len(a), np.uint64)
    for k in range(6):
        bits |= a[:, 2 + k].astype(np.uint64) << np.uint64(8 * k)
    idx = np.stack([(bits >> np.uint64(3 * k)) & np.uint64(7) for k in range(16)], 1).astype(np.int64)
    return np.take_along_axis(pal, idx, 1)


def _blocks_to_image(px: np.ndarray, bw: int, bh: int, w: int, h: int) -> np.ndarray:
    """(n, 16, c) per-block texels (row-major blocks) -> (h, w, c)."""
    c = px.shape[2]
    return px.reshape(bh, bw, 4, 4, c).transpose(0, 2, 1, 3, 4).reshape(bh * 4, bw * 4, c)[:h, :w]


def decode_level(data: bytes, w: int, h: int, fmt: int, pitch: int | None = None) -> tuple[np.ndarray, np.ndarray | None]:
    """One level (PC order: blocks or texels in rows) -> ((h, w, 4) uint8 RGBA, float32 RGBA for RGBA16F else None).
    pitch: bytes per row of a texel format (the PC pads L8 rows to 4 bytes; the default reads it from the size)."""
    w, h = max(w, 1), max(h, 1)
    buf = np.frombuffer(data, np.uint8)
    out = np.zeros((h, w, 4), np.uint8)
    out[..., 3] = 255
    if fmt in BLOCK:
        bpb = BLOCK[fmt]
        bw, bh = max((w + 3) // 4, 1), max((h + 3) // 4, 1)
        need = bw * bh * bpb
        if len(buf) < need:
            raise TexQaError(f'a {w} x {h} level of format {fmt} needs {need} bytes, has {len(buf)}')
        blk = buf[:need].reshape(-1, bpb)
        if fmt in (0, 18):
            rgb, a = _color(blk, False)
            px = np.concatenate([rgb, (a if fmt == 0 else np.full_like(a, 255))[:, :, None]], 2)
        elif fmt == 1:
            nib = blk[:, :8].astype(np.int32)
            a = np.stack([(nib[:, k // 2] >> (4 * (k % 2))) & 15 for k in range(16)], 1) * 17
            rgb, _ = _color(blk[:, 8:], True)
            px = np.concatenate([rgb, a[:, :, None]], 2)
        elif fmt == 2:
            rgb, _ = _color(blk[:, 8:], True)
            px = np.concatenate([rgb, _alpha(blk[:, :8])[:, :, None]], 2)
        elif fmt in (19, 20):
            r, g = _alpha(blk[:, :8]), _alpha(blk[:, 8:])
            px = np.stack([r, g, np.zeros_like(r), np.full_like(r, 255)], 2)
        else:                                                         # 3: DXT5A, one channel
            v = _alpha(blk)
            px = np.stack([v, v, v, np.full_like(v, 255)], 2)
        out[:] = _blocks_to_image(px, bw, bh, w, h).astype(np.uint8)
        return out, None
    bpt = TEXEL.get(fmt)
    if bpt is None:
        raise TexQaError(f'texture format {fmt} is not decoded')
    if pitch is None:
        pitch = max(w * bpt, len(buf) // h)
    if len(buf) < pitch * (h - 1) + w * bpt:
        raise TexQaError(f'a {w} x {h} level of format {fmt} needs {w * h * bpt} bytes, has {len(buf)}')
    rows = np.lib.stride_tricks.as_strided(buf, (h, w * bpt), (pitch, 1)) if pitch != w * bpt else \
        buf[:w * h * bpt].reshape(h, w * bpt)
    t = np.ascontiguousarray(rows).reshape(h, w, bpt)
    if fmt == 10:
        out[..., 0] = out[..., 1] = out[..., 2] = t[..., 0]
    elif fmt == 9:
        out[..., 0], out[..., 1], out[..., 2], out[..., 3] = t[..., 2], t[..., 1], t[..., 0], t[..., 3]
    elif fmt == 5:
        v = t[..., 0].astype(np.int32) | (t[..., 1].astype(np.int32) << 8)
        r5, g6, b5 = (v >> 11) & 31, (v >> 5) & 63, v & 31
        out[..., 0], out[..., 1], out[..., 2] = (r5 << 3) | (r5 >> 2), (g6 << 2) | (g6 >> 4), (b5 << 3) | (b5 >> 2)
    else:                                                             # 13: four half floats
        f = t.view('<f2').reshape(h, w, 4).astype(np.float32)
        out[:] = np.clip(np.nan_to_num(f, nan=0.0, posinf=1.0, neginf=0.0) * 255 + 0.5, 0, 255).astype(np.uint8)
        return out, f
    return out, None


def level_size(fmt: int, w: int, h: int) -> int:
    """The bytes of a w x h level as DICE stores it (L8 rows padded to 4 bytes)."""
    if fmt in BLOCK:
        return max((w + 3) // 4, 1) * max((h + 3) // 4, 1) * BLOCK[fmt]
    bpt = TEXEL[fmt]
    return ((w * bpt + 3) // 4 * 4 if fmt == 10 else w * bpt) * h


def unwrap(res: bytes) -> bytes:
    """A resource in the on-demand form ('RES\\x02', type name, two sizes) -> the payload; anything else as it is."""
    if res[:4] == WRAPPED and len(res) >= 64:
        n = struct.unpack_from('<I', res, 56)[0]
        return res[64:64 + n]
    return res


def is_texture(res: bytes) -> bool:
    """A PC ITexture by its header: version 3, a known type, level sizes adding up to the file (faces x levels)."""
    if len(res) < HEADER:
        return False
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('<8I', res, 0)
    if ver != 3 or typ not in (0, 1) or not 0 < mips <= 15 or not (0 < w <= 16384 and 0 < h <= 16384):
        return False
    sizes = struct.unpack_from('<15I', res, 32)
    return HEADER + (6 if typ == CUBE else 1) * sum(sizes[:mips]) == len(res)


def read_pc(res: bytes) -> Tex:
    """A PC ITexture resource decoded, every face and level."""
    res = unwrap(res)
    if len(res) < HEADER:
        raise TexQaError('shorter than a texture header')
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('<8I', res, 0)
    sizes = struct.unpack_from('<15I', res, 32)
    t = Tex(fmt, flags, typ, w, h, mips, [])
    pos = HEADER
    for s in range(6 if typ == CUBE else 1):
        for m in range(mips):
            lw, lh = max(w >> m, 1), max(h >> m, 1)
            data = res[pos:pos + sizes[m]]
            pos += sizes[m]
            if s == 0 and fmt in BLOCK | TEXEL and sizes[m] != level_size(fmt, lw, lh) and \
                    not (fmt == 10 and sizes[m] == lw * lh):
                t.size_errors.append(f'level {m} ({lw} x {lh}): {sizes[m]} bytes, format {fmt} needs '
                                     f'{level_size(fmt, lw, lh)}')
            img, f = decode_level(data, lw, lh, fmt)
            t.levels.append((s, m, img))
            if f is not None:
                t.floats.append(f)
            if s == 0 and m == 0 and fmt in BLOCK:
                b = np.frombuffer(data, np.uint8).reshape(-1, BLOCK[fmt])
                t.zero_blocks = float((~b.any(1)).mean()) if len(b) else 0.0
    return t


def read_xbox(res: bytes) -> Tex:
    """An Xbox 360 XenonTexture decoded, every face and level: the Xenos tiling undone with texture.layout and
    texture.tiled (on arrays: every block of a level at once), 16-bit words swapped back (not for 8-bit texels)."""
    if len(res) < HEADER:
        raise TexQaError('shorter than a texture header')
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('>8I', res, 0)
    if fmt not in texture.FORMATS:
        raise TexQaError(f'console texture format {fmt} is not decoded')
    if typ not in (0, CUBE):
        raise TexQaError(f'console texture type {typ} is not decoded')
    bpb, edge, _ = texture.FORMATS[fmt]
    log_bpp = {1: 0, 2: 1, 8: 3, 16: 4}[bpb]
    slices = 6 if typ == CUBE else 1
    where, size = texture.layout(w, h, mips, fmt, slices)
    data = np.frombuffer(res, np.uint8, offset=HEADER)
    if len(data) < size:
        raise TexQaError(f'{len(data)} bytes of console texture data, the layout needs {size}')
    if bpb > 1:
        data = data[:len(data) // 2 * 2].reshape(-1, 2)[:, ::-1].reshape(-1)
    t = Tex(fmt, flags, typ, w, h, mips, [])
    for s in range(slices):
        for m in range(mips):
            start, pitch, ox, oy = where[s][m]
            lw, lh = max(w >> m, 1), max(h >> m, 1)
            lbw, lbh = max((lw + edge - 1) // edge, 1), max((lh + edge - 1) // edge, 1)
            ys, xs = np.mgrid[0:lbh, 0:lbw]
            idx = texture.tiled(xs.ravel().astype(np.int64) + ox, ys.ravel().astype(np.int64) + oy, pitch, log_bpp)
            off = start + idx * bpb
            lev = data[off[:, None] + np.arange(bpb)[None, :]].tobytes()
            img, f = decode_level(lev, lw, lh, fmt, pitch=lbw * bpb if edge == 1 else None)
            t.levels.append((s, m, img))
            if s == 0 and m == 0 and fmt in BLOCK:
                b = np.frombuffer(lev, np.uint8).reshape(-1, bpb)
                t.zero_blocks = float((~b.any(1)).mean()) if len(b) else 0.0
            if f is not None:
                t.floats.append(f)
    return t


# ------------------------------------------------------------------------------------------------ measures

def _f(a: np.ndarray, ch: str) -> np.ndarray:
    idx = ['rgba'.index(c) for c in ch]
    return a[..., idx].astype(np.float64)


def psnr(a: np.ndarray, b: np.ndarray) -> float:
    mse = float(((a - b) ** 2).mean()) if a.size else 0.0
    return float('inf') if mse == 0 else round(10 * np.log10(255.0 ** 2 / mse), 2)


def luma(a: np.ndarray) -> np.ndarray:
    if a.shape[-1] >= 3:
        return a[..., 0] * 0.299 + a[..., 1] * 0.587 + a[..., 2] * 0.114
    return a.mean(-1)


def ssim(a: np.ndarray, b: np.ndarray) -> float | None:
    """Mean SSIM of luminance over 8 x 8 windows (None under 8 texels a side)."""
    x, y = luma(a), luma(b)
    h, w = x.shape[0] // 8 * 8, x.shape[1] // 8 * 8
    if not h or not w:
        return None
    x = x[:h, :w].reshape(h // 8, 8, w // 8, 8)
    y = y[:h, :w].reshape(h // 8, 8, w // 8, 8)
    mx, my = x.mean((1, 3)), y.mean((1, 3))
    vx, vy = x.var((1, 3)), y.var((1, 3))
    cov = ((x - mx[:, None, :, None]) * (y - my[:, None, :, None])).mean((1, 3))
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    s = ((2 * mx * my + c1) * (2 * cov + c2)) / ((mx ** 2 + my ** 2 + c1) * (vx + vy + c2))
    return round(float(s.mean()), 4)


def box(a: np.ndarray) -> np.ndarray:
    """The next level by the 2 x 2 box filter (a side of 1 stays)."""
    a = a.astype(np.float64)
    if a.shape[0] > 1:
        a = (a[0::2] + a[1::2]) / 2
    if a.shape[1] > 1:
        a = (a[:, 0::2] + a[:, 1::2]) / 2
    return a


def to_size(a: np.ndarray, h: int, w: int) -> np.ndarray | None:
    """a box-filtered down to h x w (powers of two apart, both sides alike), else None."""
    a = a.astype(np.float64)
    while a.shape[0] > h and a.shape[1] > w:
        a = box(a)
    return a if a.shape[:2] == (h, w) else None


def common(a: np.ndarray, b: np.ndarray) -> tuple[np.ndarray, np.ndarray] | None:
    """The two pictures at the smaller one's size (the larger box-filtered), or None (another aspect)."""
    if a.shape[:2] == b.shape[:2]:
        return a.astype(np.float64), b.astype(np.float64)
    if a.shape[0] >= b.shape[0] and a.shape[1] >= b.shape[1]:
        x = to_size(a, *b.shape[:2])
        return (x, b.astype(np.float64)) if x is not None else None
    if b.shape[0] >= a.shape[0] and b.shape[1] >= a.shape[1]:
        y = to_size(b, *a.shape[:2])
        return (a.astype(np.float64), y) if y is not None else None
    return None


def correlation(a: np.ndarray, b: np.ndarray) -> float:
    x, y = a.ravel(), b.ravel()
    if not x.std() or not y.std():
        return 1.0 if np.array_equal(x, y) else 0.0
    return round(float(np.corrcoef(x, y)[0, 1]), 4)


def chain_errors(t: Tex) -> list[list[float] | None]:
    """Per level m >= 1 of face 0: [mean error (of 255, the format's channels), correlation] against level m - 1
    box-filtered; None for levels under 4 texels a side. A re-encoded noisy picture has a larger error at a high
    correlation (each level's block compression); a level that is not the mip of the one above correlates low."""
    ch = t.channels
    out: list[list[float] | None] = []
    for m in range(1, t.mips):
        a, b = t.level(m - 1), t.level(m)
        if a is None or b is None or min(b.shape[:2]) < 4:
            out.append(None)
            continue
        p = to_size(_f(a, ch), *b.shape[:2])
        q = _f(b, ch)
        out.append(None if p is None else [round(float(np.abs(p - q).mean()), 2), correlation(p, q)])
    return out


def role(name: str) -> str:
    """The texture's role from its name: normal, specular, mask, color, ui (menu / award pictures) or other - the last
    of its name's words that names one (k98_CM: color; nm_m16a2: normal)."""
    low = name.lower().replace('\\', '/')
    if low.startswith(('awards/', 'menu/', 'ui/')) or '/ui/' in low:
        return 'ui'
    stem = low.rsplit('/', 1)[-1]
    stem = stem[:-4] if stem.endswith('.res') else stem
    for tok in reversed([x for x in re.split(r'[_\-. ]+', stem) if x]):
        bare = re.sub(r'\d+$', '', tok)
        for r, words in ROLE_TOKENS:
            if tok in words or bare in words:
                return r
    return 'other'


ORIENT: dict[str, Callable[[np.ndarray], np.ndarray]] = {
    'flipped vertically': lambda a: a[::-1], 'mirrored': lambda a: a[:, ::-1], 'rotated 180': lambda a: a[::-1, ::-1],
    'transposed': lambda a: a.transpose(1, 0, 2), 'rotated 90': lambda a: np.rot90(a, 1), 'rotated 270': lambda a: np.rot90(a, 3)}


def _small(a: np.ndarray, side: int = 128) -> np.ndarray:
    a = a.astype(np.float64)
    while min(a.shape[:2]) > side:
        a = box(a)
    return a


def best_match(ours: np.ndarray, ref: np.ndarray, ch: str) -> dict[str, Any]:
    """Ours against ref at the smaller size (both reduced to at most 128 a side): PSNR as they are, and the best
    channel permutation / orientation when one beats it by BETTER dB."""
    c = common(ours, ref)
    if c is None:
        return {'psnr': None}
    a, b = (_small(x) for x in c)
    if a.shape != b.shape:
        return {'psnr': None}
    cols = ['rgba'.index(x) for x in ch if x != 'a']
    base = psnr(a[..., cols], b[..., cols])
    out: dict[str, Any] = {'psnr': base}
    if base == float('inf') or base >= PSNR_SAME:
        return out
    import itertools
    best: tuple[float, str | None] = (base, None)
    if sum(float(a[..., i].std()) > 2 for i in cols) >= 2:
        for perm in itertools.permutations(cols):
            if list(perm) == cols:
                continue
            p = psnr(a[..., list(perm)], b[..., cols])
            if p > best[0]:
                best = (p, 'channels ' + ''.join('rgba'[i] for i in perm) + ' (as ' + ''.join('rgba'[i] for i in cols) + ')')
    for nm, fn in ORIENT.items():
        x = fn(a)
        if x.shape != b.shape:
            continue
        p = psnr(x[..., cols], b[..., cols])
        if p > best[0]:
            best = (p, nm)
    if best[1] and best[0] - base >= BETTER and (best[0] >= 25 or best[0] - base >= 2 * BETTER):
        out['better'] = {'how': best[1], 'psnr': best[0]}
    return out


# ------------------------------------------------------------------------------------------------ one texture

def _flag(row: dict[str, Any], code: str, severity: str, cause: str, detail: str) -> None:
    row['flags'].append({'code': code, 'severity': severity, 'cause': cause, 'detail': detail})


def degenerate(t: Tex) -> list[tuple[str, str]]:
    """[(code, detail)] of level 0 (face 0): all black / white / transparent, constant, NaN."""
    out = []
    a = t.level(0)
    if a is None:
        return [('degenerate: no levels', 'the texture has no level')]
    ch = t.channels
    col = [c for c in ch if c != 'a']
    v = _f(a, ''.join(col)) if col else None
    if 'a' in ch and int(a[..., 3].max()) == 0:
        out.append(('degenerate: all transparent', 'alpha 0 everywhere'))
    if v is not None and t.fmt not in (19, 20):
        if v.max() <= 4:
            out.append(('degenerate: all black', f'largest value {v.max():.0f}'))
        elif v.min() >= 251:
            out.append(('degenerate: all white', f'smallest value {v.min():.0f}'))
    if a.shape[0] * a.shape[1] >= 64 and all(float(a[..., 'rgba'.index(c)].std()) < 0.5 for c in ch):
        out.append(('degenerate: constant', 'one value: ' + str([int(a[0, 0, 'rgba'.index(c)]) for c in ch])))
    if t.floats:
        bad = int(sum((~np.isfinite(f)).sum() for f in t.floats))
        if bad:
            out.append(('degenerate: NaN / infinity', f'{bad} half floats not finite'))
    return out


def normal_check(t: Tex) -> dict[str, Any]:
    """Level 0 read as a tangent-space normal map: DXT1 (18 and 0) x y z in red green blue (z the largest, positive);
    BC5 x y in red green, z = sqrt(1 - x^2 - y^2) - the share outside the unit disc says it is not."""
    a = t.level(0)
    if a is None:
        return {}
    v = a[..., :3].astype(np.float64) / 127.5 - 1
    if t.fmt in (19, 20):
        r2 = v[..., 0] ** 2 + v[..., 1] ** 2
        return {'mean_x': round(float(v[..., 0].mean()), 3), 'mean_y': round(float(v[..., 1].mean()), 3),
                'outside_disc': round(float((r2 > 1.05).mean()), 4), 'mean_z': round(float(np.sqrt(np.clip(1 - r2, 0, 1)).mean()), 3)}
    n = np.linalg.norm(v, axis=-1)
    means = v.reshape(-1, 3).mean(0)
    return {'mean_x': round(float(means[0]), 3), 'mean_y': round(float(means[1]), 3), 'mean_z': round(float(means[2]), 3),
            'z_channel': 'rgb'[int(np.argmax(means))], 'unit_share': round(float(((n > 0.8) & (n < 1.2)).mean()), 4)}


def stats(a: np.ndarray, ch: str) -> dict[str, list[float]]:
    v = _f(a, ch)
    return {'mean': [round(float(x), 2) for x in v.reshape(-1, len(ch)).mean(0)],
            'std': [round(float(x), 2) for x in v.reshape(-1, len(ch)).std(0)]}


def compare_levels(ours: Tex, ref: Tex, ch: str) -> list[dict[str, Any]]:
    """Per level of ours (face 0, and every face of a cube map's level 0): the reference's level of the same size (or the
    nearest larger, box-filtered): PSNR, SSIM, largest error, per-channel mean difference."""
    out = []
    for s, m, a in ours.levels:
        if s and m:
            continue
        b = None
        for s2, _m2, x in ref.levels:
            if s2 == s and x.shape[0] >= a.shape[0] and x.shape[1] >= a.shape[1]:
                b = x                                     # the last (smallest) one still as large
        if b is None:
            continue
        c = common(_f(a, ch), _f(b, ch))
        if c is None:
            continue
        x, y = c
        d = np.abs(x - y)
        out.append({'face': s, 'level': m, 'size': [a.shape[1], a.shape[0]], 'psnr': psnr(x, y),
                    'ssim': ssim(x, y) if len(ch) >= 1 else None, 'max_error': round(float(d.max()), 1),
                    'mean_diff': [round(float(q), 2) for q in (x - y).reshape(-1, len(ch)).mean(0)]})
    return out


def analyse(name: str, ours_res: bytes, source_res: bytes | None = None, dice_res: bytes | None = None,
            orig_res: bytes | None = None, source_kind: str = 'xbox') -> dict[str, Any]:
    """The QA row of one texture (module docstring). source_kind: 'xbox' (an XenonTexture) or 'pc'."""
    row: dict[str, Any] = {'name': name, 'role': role(name), 'flags': []}
    ours = read_pc(ours_res)
    row['ours'] = {'format': ours.fmt, 'flags': ours.flags, 'type': ours.typ, 'size': [ours.w, ours.h],
                   'mips': ours.mips, 'channels': ours.channels}
    a0 = ours.level(0)
    assert a0 is not None
    row['ours']['stats'] = stats(a0, ours.channels)
    for e in ours.size_errors:
        _flag(row, 'level size', ERROR, 'converter', e)
    full = max(ours.w, ours.h).bit_length()
    row['ours']['full_chain'] = full
    src: Tex | None = None
    if source_res is not None:
        try:
            src = read_xbox(source_res) if source_kind == 'xbox' else read_pc(source_res)
        except (TexQaError, ValueError, KeyError) as e:
            row['source_error'] = str(e)
    orig = read_pc(orig_res) if orig_res is not None else None
    # degenerate
    src_deg = {c for c, _ in degenerate(src)} if src is not None else set()
    for code, detail in degenerate(ours):
        _flag(row, code, ERROR, 'source' if code in src_deg else 'converter' if src is not None else 'unknown', detail)
    row['ours']['zero_blocks'] = round(ours.zero_blocks, 4)
    if src is not None and ours.zero_blocks > 0.02 + 2 * src.zero_blocks:
        _flag(row, 'zero blocks', WARN, 'converter', f'{ours.zero_blocks:.1%} of level 0 blocks are all zero bytes '
              f'(the source {src.zero_blocks:.1%})')
    # mip chain
    ce = chain_errors(ours)
    row['chain'] = ce
    if ours.mips < full and ours.mips > 1 and (src is None or ours.mips < src.mips):
        _flag(row, 'mips: short chain', WARN, 'converter' if src is not None else 'unknown',
              f'{ours.mips} levels of {full}' + (f' (the source has {src.mips})' if src is not None else ''))
    if src is not None:
        sce = chain_errors(src)
        row['source_chain'] = sce
        for m, (x, y) in enumerate(zip(ce, sce), 1):
            if x is None or y is None:
                continue
            if x[0] > max(y[0] + CHAIN_TOL[0], y[0] * (1 + CHAIN_TOL[1])):
                bad = x[1] < min(CHAIN_CORR, y[1] - 0.1)
                _flag(row, 'mips: level not its parent' if bad else 'mips: noisier chain', ERROR if bad else INFO,
                      'converter', f'level {m}: {x[0]} of 255 from level {m - 1} box-filtered, correlation {x[1]} '
                      f'(the source\'s own {y[0]}, {y[1]})')
                break
    else:
        for m, x in enumerate(ce, 1):
            if x is not None and x[0] > CHAIN_ABS and x[1] < CHAIN_CORR:
                _flag(row, 'mips: level not its parent', WARN, 'unknown',
                      f'level {m}: {x[0]} of 255 from level {m - 1} box-filtered, correlation {x[1]}')
                break
    # role: format, sRGB, normal maps
    r = row['role']

    def cause(same: bool) -> str:
        """source when the source has the same property, converter when it differs, unknown without one."""
        return 'unknown' if src is None else 'source' if same else 'converter'
    same_pic = src is not None and (src.fmt, src.w, src.h) == (ours.fmt, ours.w, ours.h)
    if (ours.fmt in (18, 19, 20) or r == 'normal') and not COMPOSITE.search(name.lower()):
        nc = normal_check(ours)
        row['normal'] = nc
        if ours.fmt in (19, 20):
            if nc.get('outside_disc', 0) > 0.05:
                _flag(row, 'normal: x / y outside the unit disc', WARN, cause(same_pic),
                      f'{nc["outside_disc"]:.1%} of texels - z = sqrt(1 - x^2 - y^2) clamps there')
            if abs(nc.get('mean_x', 0)) > 0.3 or abs(nc.get('mean_y', 0)) > 0.3:
                _flag(row, 'normal: biased x / y', WARN, cause(same_pic), f'mean x {nc["mean_x"]}, y {nc["mean_y"]}')
        elif ours.fmt in (18, 0) and r == 'normal':
            if nc.get('z_channel') != 'b' or nc.get('mean_z', 1) < 0.3:
                _flag(row, 'normal: z not in blue', ERROR if ours.fmt == 18 else WARN, cause(same_pic),
                      f'mean x {nc["mean_x"]}, y {nc["mean_y"]}, z {nc["mean_z"]} (largest: {nc["z_channel"]})')
        if ours.flags & SRGB:
            _flag(row, 'srgb: normal map with the sRGB bit', ERROR, cause(src is not None and bool(src.flags & SRGB)),
                  f'flags {ours.flags} (DICE\'s normal maps: never)')
    if r in ROLE_FORMATS and ours.fmt not in ROLE_FORMATS[r]:
        _flag(row, 'format: not DICE\'s for the role', WARN if r in ('normal', 'color') else INFO,
              cause(src is not None and src.fmt == ours.fmt),
              f'{NAMES.get(ours.fmt, ours.fmt)} for a {r} texture; DICE\'s: ' +
              ', '.join(f'{NAMES.get(k, k)} {v:.0%}' for k, v in ROLE_FORMATS[r].items()))
    if ours.fmt == 18 and r not in ('normal', 'other', 'mask'):
        _flag(row, 'format: normal-map DXT1 for a non-normal', WARN, cause(src is not None and src.fmt == 18),
              f'format 18 on a {r} texture')
    if r in ('color', 'ui') and not ours.flags & SRGB and ours.fmt not in (10, 13):
        _flag(row, 'srgb: colour map without the sRGB bit', INFO, cause(src is not None and not src.flags & SRGB),
              f'flags {ours.flags} (DICE\'s {r} textures: {ROLE_SRGB[r]:.0%} with it)')
    if ours.fmt in (2, 1) and 'a' in ours.channels and int(a0[..., 3].min()) == 255:
        _flag(row, 'alpha: format with alpha, none used', INFO, 'unknown', 'alpha 255 everywhere')
    # the source
    if src is not None:
        _vs_source(row, ours, src, orig)
    # DICE's copy
    if dice_res is not None:
        try:
            _vs_dice(row, ours_res, ours, read_pc(dice_res), unwrap(dice_res))
        except (TexQaError, ValueError) as e:
            row['dice_error'] = str(e)
    if any(f['cause'] == 'intentional' for f in row['flags']):
        for f in row['flags']:                    # the converter's output (.orig) is the source: the repaint did it
            if f['cause'] == 'converter':
                f['cause'] = 'repaint'
    if (row.get('dice') or {}).get('identical'):
        for f in row['flags']:                    # DICE's own PC copy, byte for byte (a stock file a build took):
            f['cause'], f['severity'] = 'dice', INFO   # what differs from the console source is DICE's doing
            f['code'] = 'stock copy: ' + f['code']
    row['score'] = score(row)
    return row


def _alpha_used(t: Tex) -> bool:
    a = t.level(0)
    return a is not None and 'a' in t.channels and int(a[..., 3].min()) < 250


def _vs_source(row: dict[str, Any], ours: Tex, src: Tex, orig: Tex | None) -> None:
    row['source'] = {'format': src.fmt, 'flags': src.flags, 'size': [src.w, src.h], 'mips': src.mips,
                     'channels': src.channels}
    s0, a0 = src.level(0), ours.level(0)
    assert s0 is not None and a0 is not None
    ch = ''.join(c for c in ours.channels if c in src.channels) or 'r'
    row['source']['stats'] = stats(s0, ch)
    if (ours.w * src.h) != (ours.h * src.w):
        _flag(row, 'aspect', ERROR, 'converter', f'{ours.w} x {ours.h}, the source {src.w} x {src.h}')
    elif (ours.w, ours.h) != (src.w, src.h):
        _flag(row, 'size', INFO, 'converter', f'{ours.w} x {ours.h}, the source {src.w} x {src.h}')
    if ours.typ != src.typ:
        _flag(row, 'type', ERROR, 'converter', f'type {ours.typ}, the source {src.typ}')
    if ours.fmt != src.fmt and (src.fmt, ours.fmt) not in ((3, 10), (5, 9)):
        _flag(row, 'format', INFO, 'converter', f'{NAMES.get(ours.fmt, ours.fmt)}, the source {NAMES.get(src.fmt, src.fmt)}')
    if (ours.flags ^ src.flags) & SRGB:
        _flag(row, 'srgb: differs from the source', WARN, 'converter', f'flags {ours.flags}, the source {src.flags}')
    if (ours.flags ^ src.flags) & ~(SRGB | HALVED):
        _flag(row, 'flags', INFO, 'converter', f'flags {ours.flags}, the source {src.flags}')
    if ours.mips != src.mips and (ours.w, ours.h) == (src.w, src.h):
        _flag(row, 'mips: count', WARN, 'converter', f'{ours.mips} levels, the source {src.mips}')
    if _alpha_used(src) and 'a' not in ours.channels:
        _flag(row, 'alpha: lost', ERROR, 'converter', f'the source\'s alpha is used ({NAMES.get(src.fmt)}), ours '
              f'{NAMES.get(ours.fmt)} has none')
    lv = compare_levels(ours, src, ch)
    row['levels'] = lv
    p0 = lv[0]['psnr'] if lv else None
    row['psnr_source'] = p0
    row['ssim_source'] = lv[0]['ssim'] if lv else None
    row['identical_source'] = bool(lv) and all(x['psnr'] == float('inf') for x in lv)
    bm = best_match(a0, s0, ch)
    if 'better' in bm:
        how = bm['better']['how']
        _flag(row, 'swizzle' if how.startswith('channels') else 'orientation', ERROR, 'converter',
              f'{how} matches the source at {bm["better"]["psnr"]} dB, as it is {bm["psnr"]} dB')
    if p0 is not None and p0 < PSNR_SAME and 'better' not in bm:
        if orig is not None:
            o = compare_levels(orig, src, ch)
            po = o[0]['psnr'] if o else None
            row['psnr_orig_source'] = po
            if po is not None and po >= PSNR_SAME:
                _flag(row, 'repaint (grime.py)', INFO, 'intentional',
                      f'{p0} dB from the source; the tool\'s own output (.orig) {po} dB')
                return
        worst = min((x['psnr'] for x in lv), default=p0)
        _flag(row, 'differs from the source', ERROR if p0 < PSNR_OTHER else WARN, 'converter',
              f'level 0 {p0} dB (SSIM {row["ssim_source"]}), worst level {worst} dB')
    elif lv:
        bad = [x for x in lv if x['psnr'] < PSNR_SAME and min(x['size']) >= 4]
        if bad:
            _flag(row, 'differs from the source (a level)', WARN, 'converter',
                  f'level {bad[0]["level"]} ({bad[0]["size"][0]} x {bad[0]["size"][1]}): {bad[0]["psnr"]} dB')


def _vs_dice(row: dict[str, Any], ours_res: bytes, ours: Tex, dice: Tex, dice_raw: bytes) -> None:
    row['dice'] = {'format': dice.fmt, 'flags': dice.flags, 'size': [dice.w, dice.h], 'mips': dice.mips}
    if unwrap(ours_res) == dice_raw:
        row['dice']['identical'] = True
        row['psnr_dice'] = float('inf')
        return
    known = []
    if (dice.w, dice.h) == (ours.w * 2, ours.h * 2):
        known.append('twice ours' + (' with flags bit 1 (DICE halves it for the console)' if dice.flags & HALVED
                                     else ' (no flags bit 1)'))
    elif (dice.w, dice.h) != (ours.w, ours.h):
        known.append(f'{dice.w} x {dice.h}')
    elif dice.flags & HALVED and not ours.flags & HALVED:
        known.append('flags bit 1 at the same size (DICE re-exported it)')
    if (dice.flags ^ ours.flags) & SRGB:
        known.append(f'sRGB bit: DICE {dice.flags & SRGB}, ours {ours.flags & SRGB}')
    if dice.fmt != ours.fmt:
        known.append(f'format: DICE {NAMES.get(dice.fmt, dice.fmt)}')
    d0, a0 = dice.level(0), ours.level(0)
    assert d0 is not None and a0 is not None
    ch = ''.join(c for c in ours.channels if c in CHANNELS.get(dice.fmt, 'rgba') or (c == 'a' and 'a' in dice.channels)) or 'r'
    c = common(_f(a0, ch), _f(d0, ch))
    if c is None:
        known.append('another aspect')
        row['dice']['notes'] = known
        _flag(row, 'dice: another aspect', INFO, 'dice', '; '.join(known))
        return
    p = psnr(*c)
    cor = correlation(luma(c[0]) if len(ch) >= 3 else c[0], luma(c[1]) if len(ch) >= 3 else c[1])
    row['psnr_dice'], row['dice']['correlation'] = p, cor
    bm = best_match(a0, d0, ch)
    if 'better' in bm:
        how = bm['better']['how']
        _flag(row, 'swizzle vs DICE' if how.startswith('channels') else 'orientation vs DICE', ERROR, 'converter',
              f'{how} matches DICE\'s copy at {bm["better"]["psnr"]} dB, as it is {bm["psnr"]} dB')
    if p < PSNR_OTHER and cor < 0.5 and 'better' not in bm:
        known.append(f'other art ({p} dB, correlation {cor})')
        _flag(row, 'dice: repaint / other art', INFO, 'dice', '; '.join(known))
    elif known:
        _flag(row, 'dice: known difference', INFO, 'dice', '; '.join(known) + f'; picture {p} dB')
    row['dice']['notes'] = known
    if (dice.flags ^ ours.flags) & SRGB and dice.fmt == ours.fmt:
        _flag(row, 'srgb: differs from DICE', WARN, 'converter' if row.get('source') else 'unknown',
              f'flags {ours.flags}, DICE\'s {dice.flags}')


def score(row: dict[str, Any]) -> float:
    """Worst first: errors, then warnings, then the lowest PSNR to the source (or DICE)."""
    sev = {ERROR: 100.0, WARN: 10.0, INFO: 0.0}
    s = sum(sev[f['severity']] for f in row['flags'] if f['cause'] not in ('intentional', 'dice'))
    p = row.get('psnr_source')
    if p is not None and p != float('inf'):
        s += max(0.0, 40 - p)
    return round(s, 2)


# ------------------------------------------------------------------------------------------------ finding inputs

def _sha1(b: bytes) -> str:
    return hashlib.sha1(b).hexdigest()


@dataclass
class Item:
    name: str            # path under its bundle folder / the folder given, forward slashes
    path: str            # the file
    root: str            # the folder given (a label in the reports)
    orig: str | None = None


def find(roots: Iterable[str], skip: Iterable[str] = ()) -> list[Item]:
    """Every PC texture under the roots (module docstring: inputs)."""
    skip_l = {s.lower() for s in skip}
    out = []
    for root in roots:
        root = os.path.abspath(root)
        if os.path.isfile(root):
            out.append(Item(os.path.basename(root), root, os.path.dirname(root)))
            continue
        if not os.path.isdir(root):
            raise TexQaError(f'{root}: no such file or folder')
        manifests: dict[str, dict[str, str]] = {}
        for dp, dn, fn in os.walk(root):
            dn[:] = sorted(d for d in dn if d.lower() not in skip_l)
            if 'fbrb.manifest' in fn:
                types = {}
                for line in open(os.path.join(dp, 'fbrb.manifest'), encoding='utf-8', errors='replace'):
                    if line.startswith('#') or '|' not in line:
                        continue
                    cols = line.split('|')
                    types[cols[0].replace('\\', '/').lower()] = cols[1]
                manifests[dp] = types
            base = next((m for m in manifests if dp == m or dp.startswith(m + os.sep)), None)
            for f in sorted(fn):
                if not f.lower().endswith('.res'):
                    continue
                p = os.path.join(dp, f)
                rel = os.path.relpath(p, base or root).replace(os.sep, '/')
                if base is not None:
                    if manifests[base].get(rel.lower()) != 'ITexture':
                        continue
                else:
                    with open(p, 'rb') as fh:
                        head = fh.read(64 + HEADER)
                    size = os.path.getsize(p)
                    if head[:4] == WRAPPED:
                        head, size = head[64:], size - 64
                    if not _sized_texture(head, size):
                        continue
                orig = p + '.orig'
                out.append(Item(rel, p, root, orig if os.path.isfile(orig) else None))
    return out


def _sized_texture(head: bytes, size: int) -> bool:
    if len(head) < HEADER:
        return False
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('<8I', head, 0)
    if ver != 3 or typ not in (0, 1) or not 0 < mips <= 15 or not (0 < w <= 16384 and 0 < h <= 16384):
        return False
    sizes = struct.unpack_from('<15I', head, 32)
    return HEADER + (6 if typ == CUBE else 1) * sum(sizes[:mips]) == size


def candidates(name: str) -> list[str]:
    """The lower names to look a texture up by: its name, then with leading folders dropped one at a time."""
    parts = name.lower().replace('\\', '/').split('/')
    return ['/'.join(parts[i:]) for i in range(len(parts))]


class LooseSource:
    """A folder of unpacked console resources (FbrbTool's output of console bundles), by lower relative name."""

    def __init__(self, folder: str):
        self.folder = folder
        self.entries: dict[str, str] = {}
        for dp, _, fn in os.walk(folder):
            for f in fn:
                if f.lower().endswith('.res'):
                    p = os.path.join(dp, f)
                    self.entries[os.path.relpath(p, folder).replace(os.sep, '/').lower()] = p

    def ident(self, key: str) -> str:
        p = self.entries[key]
        return f'{p}|{os.path.getsize(p)}|{int(os.path.getmtime(p))}'

    def data(self, key: str) -> bytes:
        return open(self.entries[key], 'rb').read()


class Bf1943:
    """Battlefield 1943's Xbox bundles (bf1943import.Source), opened only when a texture is not in the cache; the
    names come from a cached list keyed by the bundles' sizes and dates."""

    def __init__(self, folder: str, cache: str | None):
        self.folder = folder
        self._src: Any = None
        sig = []
        for dp, _, fn in os.walk(folder):
            for f in sorted(fn):
                if f.endswith('.fbrb'):
                    p = os.path.join(dp, f)
                    sig.append([os.path.relpath(p, folder), os.path.getsize(p), int(os.path.getmtime(p))])
        self.sig = _sha1(json.dumps(sorted(sig)).encode())[:16]
        path = os.path.join(cache, f'bf1943_names_{self.sig}.json') if cache else None
        if path and os.path.isfile(path):
            self.entries = set(json.load(open(path)))
        else:
            self.entries = {k for k, (_, e) in self.src.entries.items() if e['type'] == 'XenonTexture'}
            if path:
                _write_json(path, sorted(self.entries))

    @property
    def src(self) -> Any:
        if self._src is None:
            suite_paths.use('bf1943-import')
            import bf1943import
            self._src = bf1943import.Source(self.folder)
        return self._src

    def ident(self, key: str) -> str:
        return f'bf1943:{self.sig}:{key}'

    def data(self, key: str) -> bytes:
        return self.src.data(key)


class Dice:
    """DICE's PC textures by lower name from the game copy's stock bundles (meshview's bundle list: Package before
    Dist, OPFOR's own levels left out); the index cached in the cache folder (never in OPFOR's work folder)."""

    def __init__(self, opfor: str, cache: str | None):
        suite_paths.use('mesh-viewer')
        import meshview
        bundles = [b for _, b in meshview._bundles(opfor)]
        sig = [(b, os.path.getsize(b), int(os.path.getmtime(b))) for b in bundles]
        self.sig = _sha1(json.dumps(sig).encode())[:16]
        path = os.path.join(cache, f'dice_textures_{self.sig}.json') if cache else None
        self.stamp = {b: f'{s}|{t}' for b, s, t in sig}
        if path and os.path.isfile(path):
            self.where = {k: tuple(v) for k, v in json.load(open(path)).items()}
        else:
            self.where = {}
            for b in bundles:
                try:
                    ents = fbrb.entries(b)
                except Exception:                                   # noqa: BLE001 - an unreadable bundle is skipped
                    continue
                for e in ents:
                    if not e.deleted and e.type == 'ITexture':
                        self.where.setdefault(e.name.lower(), (b, e.name))
            if path:
                _write_json(path, self.where)
        self.entries = set(self.where)

    def ident(self, key: str) -> str:
        b, n = self.where[key]
        return f'{b}|{self.stamp.get(b, "")}|{n}'

    def data(self, key: str) -> bytes:
        b, n = self.where[key]
        return fbrb.read(b, [n])[n]

    def read_many(self, keys: Iterable[str]) -> dict[str, bytes]:
        by: dict[str, list[str]] = {}
        for k in keys:
            b, n = self.where[k]
            by.setdefault(b, []).append(n)
        out = {}
        for b, names in by.items():
            for n, d in fbrb.read(b, names).items():
                out[n.lower()] = d
        return out


def _write_json(path: str, data: Any) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + '.tmp'
    with open(tmp, 'w', encoding='utf-8') as f:
        json.dump(data, f, indent=0)
    os.replace(tmp, path)


def lookup(name: str, entries: set[str] | dict[str, Any]) -> str | None:
    for c in candidates(name):
        if c in entries:
            return c
    return None


# ------------------------------------------------------------------------------------------------ the run

def run(roots: list[str], *, sources: Sequence[Any] = (), dice: Dice | None = None, cache: str | None = None,
        skip: Iterable[str] = (), log: Callable[[str], None] | None = None) -> list[dict[str, Any]]:
    """The rows of every texture under roots (cached by content and source identities)."""
    if log is None:
        def log(msg: str) -> None:
            print(msg, flush=True)
    items = find(roots, skip)
    cache_path = os.path.join(cache, f'rows_v{VERSION}.json') if cache else None
    old: dict[str, Any] = {}
    if cache_path and os.path.isfile(cache_path):
        try:
            old = json.load(open(cache_path))
        except ValueError:
            old = {}
    new: dict[str, Any] = {}
    rows = []
    todo_dice: dict[str, str] = {}
    plan = []
    shas = {it.path: _sha1(open(it.path, 'rb').read()) for it in items}
    origs = {shas[it.path]: it.orig for it in items if it.orig}
    for it in items:
        data = open(it.path, 'rb').read()
        osha = shas[it.path]
        if it.orig is None and osha in origs:     # a copy of a repainted texture (an install staging's): its .orig
            it.orig = origs[osha]
        src_key, src_obj = None, None
        for s in sources if not COMPOSITE.search(it.name.lower()) else ():
            k = lookup(it.name, s.entries)
            if k is not None:
                src_key, src_obj = k, s
                break
        dkey = lookup(it.name, dice.entries) if dice is not None else None
        orig_sha = _sha1(open(it.orig, 'rb').read()) if it.orig else None
        ident = [VERSION, CODE, osha, src_obj.ident(src_key) if src_obj is not None and src_key else None,
                 dice.ident(dkey) if dice is not None and dkey else None, orig_sha, it.name.lower()]
        key = _sha1(json.dumps(ident).encode())
        plan.append((it, data, key, src_key, src_obj, dkey))
        if key not in old and dkey:
            todo_dice[dkey] = dkey
    dice_bytes = dice.read_many(todo_dice) if dice is not None and todo_dice else {}
    done = 0
    for it, data, key, src_key, src_obj, dkey in plan:
        if key in old:
            row = old[key]
        else:
            try:
                row = analyse(it.name, data, src_obj.data(src_key) if src_obj is not None and src_key else None,
                              dice_bytes.get(dkey) if dkey else None,
                              open(it.orig, 'rb').read() if it.orig else None,
                              'xbox' if not isinstance(src_obj, PcSource) else 'pc')
            except (TexQaError, ValueError, KeyError, struct.error) as e:
                row = {'name': it.name, 'flags': [{'code': 'unreadable', 'severity': ERROR, 'cause': 'unknown',
                                                    'detail': f'{type(e).__name__}: {e}'}], 'score': 1000.0}
            row['source_name'] = src_key
            row['source_from'] = None if src_obj is None else type(src_obj).__name__
            row['dice_name'] = dkey
            done += 1
            if done % 50 == 0:
                log(f'  {done} textures analysed ...')
        new[key] = row
        r = dict(row)
        r['path'], r['root'], r['orig'] = it.path, it.root, it.orig
        rows.append(r)
    if cache_path:
        _write_json(cache_path, new)
    log(f'{len(rows)} textures: {done} analysed, {len(rows) - done} unchanged (cache)')
    return rows


class PcSource(LooseSource):
    """A folder of PC textures as the reference (no Xbox source: e.g. a previous build)."""


# ------------------------------------------------------------------------------------------------ reports

def key_of(row: dict[str, Any]) -> str:
    """The baseline key: the folder given's last name and the file's path under it (stable across runs and machines;
    two unpacked bundles under one folder may hold the same name with other contents - the install's level and
    terrain stages)."""
    root = row.get('root', '').rstrip('\\/')
    rel = os.path.relpath(row['path'], root).replace(os.sep, '/') if row.get('path') and root else row['name']
    return f'{os.path.basename(root)}/{rel}'.lower()


def baseline_of(rows: list[dict[str, Any]]) -> dict[str, Any]:
    return {'version': VERSION, 'textures': {key_of(r): {'psnr_source': r.get('psnr_source'),
                                                         'psnr_dice': r.get('psnr_dice'),
                                                         'flags': sorted({f['code'] for f in r['flags']
                                                                          if f['severity'] == ERROR})}
                                             for r in rows}}


def regressions(rows: list[dict[str, Any]], base: dict[str, Any], threshold: float = 1.0) -> list[str]:
    """A new error flag (on a texture known before, or on a new one) or a PSNR (to the source or DICE) more than
    threshold dB under the baseline's."""
    old = base.get('textures', {})
    out = []
    for r in rows:
        k = key_of(r)
        b = old.get(k)
        errs = sorted({f['code'] for f in r['flags'] if f['severity'] == ERROR})
        if b is None:
            if errs:
                out.append(f'{k}: new texture with {", ".join(errs)}')
            continue
        added = [e for e in errs if e not in b['flags']]
        if added:
            out.append(f'{k}: new {", ".join(added)}')
        for f in ('psnr_source', 'psnr_dice'):
            was, now = b.get(f), r.get(f)
            if was is None or now is None:
                continue
            was = 99.0 if was in (float('inf'), 'inf') else float(was)
            now = 99.0 if now in (float('inf'), 'inf') else float(now)
            if now < was - threshold:
                out.append(f'{k}: {f} {was} -> {now} dB')
    return out


def summary(rows: list[dict[str, Any]]) -> dict[str, Any]:
    import collections
    by_code: collections.Counter[str] = collections.Counter()
    by_cause: collections.Counter[str] = collections.Counter()
    for r in rows:
        for f in r['flags']:
            by_code[f'{f["severity"]}: {f["code"]}'] += 1
            by_cause[f['cause']] += 1
    return {'textures': len(rows), 'with_source': sum(1 for r in rows if r.get('source')),
            'identical_to_source': sum(1 for r in rows if r.get('identical_source')),
            'with_dice_copy': sum(1 for r in rows if r.get('dice')),
            'identical_to_dice': sum(1 for r in rows if (r.get('dice') or {}).get('identical')),
            'flagged': sum(1 for r in rows if any(f['severity'] != INFO for f in r['flags'])),
            'flags': dict(sorted(by_code.items())), 'causes': dict(by_cause)}


def text_report(rows: list[dict[str, Any]], regress: list[str] | None = None) -> str:
    s = summary(rows)
    out = [f'{s["textures"]} textures; {s["with_source"]} with a source ({s["identical_to_source"]} decoding identically '
           f'at every level); {s["with_dice_copy"]} with DICE\'s PC copy ({s["identical_to_dice"]} identical); '
           f'{s["flagged"]} flagged (error / warn)', '', 'flags:']
    out += [f'  {n:5d}  {k}' for k, n in s['flags'].items()]
    if regress is not None:
        out += ['', f'regressions against the baseline: {len(regress)}'] + [f'  {x}' for x in regress]
    out += ['', 'flagged textures, worst first (severity, cause, check: detail):']
    for r in sorted(rows, key=lambda r: -r.get('score', 0)):
        fl = [f for f in r['flags'] if f['severity'] != INFO or f['cause'] in ('dice', 'intentional')]
        if not any(f['severity'] != INFO for f in fl):
            continue
        o = r.get('ours') or {}
        out.append(f'{r["name"]}  [{key_of(r)}]  {NAMES.get(o.get("format", -1), o.get("format"))} '
                   f'{o.get("size")} mips {o.get("mips")}  PSNR source {r.get("psnr_source")} DICE {r.get("psnr_dice")}')
        for f in fl:
            out.append(f'    {f["severity"]:5s} {f["cause"]:11s} {f["code"]}: {f["detail"]}')
    return '\n'.join(out) + '\n'


def _tile(a: np.ndarray | None, side: int) -> np.ndarray:
    t = np.full((side, side, 3), 40, np.uint8)
    if a is None:
        return t
    a = a.astype(np.float64)
    while max(a.shape[:2]) > side:
        a = box(a)
    sc = max(1, side // max(a.shape[:2]))
    a = np.repeat(np.repeat(a, sc, 0), sc, 1)[:side, :side]
    t[:a.shape[0], :a.shape[1]] = np.clip(a[..., :3], 0, 255).astype(np.uint8)
    return t


def heat(a: np.ndarray, b: np.ndarray | None, ch: str) -> np.ndarray | None:
    """|a - b| on the channels held (b at a's size), x 4, as a black-red-yellow-white ramp."""
    if b is None:
        return None
    c = common(_f(a, ch), _f(b, ch))
    if c is None:
        return None
    d = np.clip(np.abs(c[0] - c[1]).max(-1) * 4, 0, 765)
    return np.stack([np.clip(d, 0, 255), np.clip(d - 255, 0, 255), np.clip(d - 510, 0, 255)], -1)


def sheets(rows: list[dict[str, Any]], out: str, sources: Sequence[Any], dice: Dice | None, n: int = 24,
           per: int = 8, side: int = 160) -> list[str]:
    """Contact sheets of the n worst flagged rows: source | ours | DICE's | difference (ours - the source, else ours -
    DICE's) | ours' alpha; per rows a page. [paths]."""
    from PIL import Image, ImageDraw
    worst = [r for r in sorted(rows, key=lambda r: -r.get('score', 0))
             if any(f['severity'] != INFO and f['cause'] not in ('intentional', 'dice') for f in r['flags'])][:n]
    os.makedirs(out, exist_ok=True)
    paths = []
    label_h, pad = 34, 4
    for page in range(0, len(worst), per):
        chunk = worst[page:page + per]
        img = Image.new('RGB', (pad + 5 * (side + pad), pad + len(chunk) * (side + label_h + pad)), (20, 20, 20))
        dr = ImageDraw.Draw(img)
        for i, r in enumerate(chunk):
            y = pad + i * (side + label_h + pad)
            try:
                ours = read_pc(open(r['path'], 'rb').read())
                src = None
                sn = r.get('source_name')
                for s in sources:
                    if sn and type(s).__name__ == r.get('source_from') and sn in s.entries:
                        src = read_xbox(s.data(sn)) if not isinstance(s, PcSource) else read_pc(s.data(sn))
                        break
                dc = read_pc(dice.data(r['dice_name'])) if dice is not None and r.get('dice_name') else None
            except Exception as e:                                  # noqa: BLE001 - a texture it cannot draw is named
                dr.text((pad, y), f'{r["name"]}: {e}', fill=(255, 120, 120))
                continue
            a0 = ours.level(0)
            assert a0 is not None
            ch = ''.join(c for c in ours.channels if c != 'a') or 'r'
            s0 = src.level(0) if src is not None else None
            d0 = dc.level(0) if dc is not None else None
            hm = heat(a0, s0, ch) if s0 is not None else heat(a0, d0, ch)
            al = np.repeat(a0[..., 3:4], 3, 2) if 'a' in ours.channels else None
            for j, t in enumerate((s0, a0, d0, hm, al)):
                img.paste(Image.fromarray(_tile(t, side)), (pad + j * (side + pad), y + label_h))
            codes = '; '.join(f['code'] for f in r['flags'] if f['severity'] != INFO)
            dr.text((pad, y + 2), f'{r["name"]}  ({key_of(r).split("/")[0]})', fill=(230, 230, 230))
            dr.text((pad, y + 16), f'{codes}  | PSNR src {r.get("psnr_source")} DICE {r.get("psnr_dice")}'[:150],
                    fill=(255, 190, 90))
        p = os.path.join(out, f'worst_{page // per + 1:02d}.png')
        img.save(p)
        paths.append(p)
    if paths:
        legend = 'columns: source (Xbox) | ours | DICE\'s PC copy | difference x4 (to the source, else DICE) | ours\' alpha'
        with open(os.path.join(out, 'README.txt'), 'w') as f:
            f.write(legend + '\n')
    return paths


def _num(x: Any) -> Any:
    """JSON without Infinity (strict readers): inf as the string 'inf'."""
    if isinstance(x, float) and x == float('inf'):
        return 'inf'
    if isinstance(x, dict):
        return {k: _num(v) for k, v in x.items()}
    if isinstance(x, list):
        return [_num(v) for v in x]
    return x


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or '').split('\n')[0],
                                 formatter_class=argparse.RawDescriptionHelpFormatter, epilog=__doc__)
    ap.add_argument('inputs', nargs='+')
    ap.add_argument('-o', '--out', default='texqa_out')
    ap.add_argument('--bf1943', help="BF1943's extracted dist\\xenon (default: bf1943import's)")
    ap.add_argument('--no-bf1943', action='store_true')
    ap.add_argument('--console', action='append', default=[], help='a folder of unpacked console BFBC2 bundles')
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 (its game copy: DICE\'s PC copies)')
    ap.add_argument('--no-dice', action='store_true')
    ap.add_argument('--cache')
    ap.add_argument('--worst', type=int, default=24)
    ap.add_argument('--skip', action='append', default=[])
    ap.add_argument('--baseline')
    ap.add_argument('--write-baseline')
    ap.add_argument('--threshold', type=float, default=1.0)
    a = ap.parse_args(argv)
    for p in a.inputs:
        if not os.path.exists(p):
            raise TexQaError(f'{p}: no such file or folder')
    sources: list[Any] = [LooseSource(c) for c in a.console]
    if not a.no_bf1943:
        suite_paths.use('bf1943-import')
        import bf1943import
        folder = a.bf1943 or bf1943import.default_source()
        if os.path.isdir(folder):
            sources.append(Bf1943(folder, a.cache))
        else:
            print(f'no BF1943 source at {folder}: textures are compared with DICE\'s copies only')
    dice = None
    if not a.no_dice:
        suite_paths.use('shader-tools')
        import shaderdb
        dice = Dice(shaderdb.find_opfor(a.opfor), a.cache)
    rows = run(a.inputs, sources=sources, dice=dice, cache=a.cache, skip=a.skip)
    reg = None
    if a.baseline:
        reg = regressions(rows, json.load(open(a.baseline)), a.threshold)
    os.makedirs(a.out, exist_ok=True)
    _write_json(os.path.join(a.out, 'texqa.json'), _num({'summary': summary(rows), 'regressions': reg, 'rows': rows}))
    with open(os.path.join(a.out, 'texqa.txt'), 'w', encoding='utf-8') as f:
        f.write(text_report(rows, reg))
    sh = sheets(rows, os.path.join(a.out, 'sheets'), sources, dice, a.worst)
    if a.write_baseline:
        _write_json(a.write_baseline, _num(baseline_of(rows)))
    s = summary(rows)
    print(f'{s["textures"]} textures, {s["flagged"]} flagged -> {os.path.join(a.out, "texqa.txt")}; '
          f'{len(sh)} contact sheet(s)')
    if reg:
        print(f'{len(reg)} regression(s) against {a.baseline}:')
        for x in reg[:30]:
            print('  ' + x)
        return 1
    return 0


if __name__ == '__main__':
    errors.setup('texqa', gui=False)
    with errors.guard():
        sys.exit(main())
