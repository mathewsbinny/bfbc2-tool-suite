"""PC textures (ITexture .res) from pictures: DXT1 colour maps (format 0, flags 1), DXT1 normal maps (format 18,
flags 0) and DXT5 pictures with alpha (format 2, flags 1: DICE's award images), the layouts of DICE's own
textures. The header and level layout: the console converter's FORMAT.md section 2 (92 bytes: version 3, type 0,
format, flags, width, height, depth 0, mip count, 15 level sizes; the PC's levels in rows, level after level, down
to 1 x 1, a level never under one block).

    import texwrite
    open('x_D.res', 'wb').write(texwrite.texture(rgb))              # rgb: (h, w, 3) uint8, sides powers of two
    open('x_N.res', 'wb').write(texwrite.texture(rgb, 'normal'))    # tangent-space normals, z in blue
    open('x.res', 'wb').write(texwrite.texture(rgba, 'dxt5', mips=False))   # one level, as the award images
    texwrite.wrapped(res)          # the on-demand bundles' form (async\\ondemand_awards-00.fbrb): 'RES\\x02' + the
                                   # type name zero-padded to 56 bytes, the payload size twice (u32), the payload

The encoder: per 4 x 4 block, the colours' principal axis (symmetric eigensolve) gives the two end colours (the
extremes of the block projected on it, rounded to 5:6:5), four-colour mode (colour 0 > colour 1), each texel
the nearest of the four - but a DXT1 block with a transparent texel (a colour picture given with 1-bit alpha) in
three-colour mode (colour 0 <= colour 1: the two ends, their midpoint, index 3 transparent); DXT5's alpha half
chooses the lower-error eight-value or six-value palette (the latter reserves exact 0 and 255).
The principal axis is solved from the symmetric covariance matrix, preserving opposing channel variations. Mip levels: 2 x 2 box filter; normal maps renormalised after each step.
"""
import struct

import numpy as np

COLOR, NORMAL, DXT5 = 0, 18, 2
HEADER = 92


class TexWriteError(Exception):
    what_to_do = 'Give a picture whose sides are powers of two.'


def _565(c):
    """(n, 3) float 0-255 -> (n,) uint16 5:6:5, and the colours it stands for."""
    r = np.clip(np.round(c[:, 0] * 31 / 255), 0, 31).astype(np.int32)
    g = np.clip(np.round(c[:, 1] * 63 / 255), 0, 63).astype(np.int32)
    b = np.clip(np.round(c[:, 2] * 31 / 255), 0, 31).astype(np.int32)
    v = (r << 11) | (g << 5) | b
    back = np.stack([r * 255 // 31, g * 255 // 63, b * 255 // 31], 1).astype(float)
    return v, back


def _blocks(img, channels):
    h, w = img.shape[:2]
    ph, pw = max(4, -(-h // 4) * 4), max(4, -(-w // 4) * 4)
    img = np.pad(img.astype(float), ((0, ph - h), (0, pw - w), (0, 0)), mode='edge')
    bh, bw = ph // 4, pw // 4
    return img.reshape(bh, 4, bw, 4, channels).transpose(0, 2, 1, 3, 4).reshape(-1, 16, channels)


def dxt1(rgb):
    """(h, w, 3) uint8 -> DXT1 blocks (rows of blocks), sides multiples of 4 or under 4 (padded by edge copies).
    (h, w, 4): the fourth channel is 1-bit alpha (under 128: transparent) - a block with a transparent texel is written
    in three-colour mode (colour 0 <= colour 1, index 3 transparent), the others as without alpha."""
    img = np.asarray(rgb)
    if img.ndim == 3 and img.shape[2] == 4:
        blk = _blocks(img, 4)
        return _color_blocks(blk[:, :, :3], blk[:, :, 3] < 128).tobytes()
    return _color_blocks(_blocks(img[..., :3], 3)).tobytes()


def alpha1(level, w, h):
    """The 1-bit alpha of a DXT1 level (PC block order) -> (h, w) int array, 0 (transparent: a three-colour block's
    index 3) or 255 - what the console converter's decode, colour only, leaves out."""
    bw, bh = max((w + 3) // 4, 1), max((h + 3) // 4, 1)
    blk = np.frombuffer(level[:bw * bh * 8], np.uint8).reshape(-1, 8)
    c0 = blk[:, 0].astype(np.int64) | (blk[:, 1].astype(np.int64) << 8)
    c1 = blk[:, 2].astype(np.int64) | (blk[:, 3].astype(np.int64) << 8)
    bits = np.ascontiguousarray(blk[:, 4:8]).view('<u4')[:, 0].astype(np.uint64)
    idx = np.stack([(bits >> np.uint64(2 * k)) & np.uint64(3) for k in range(16)], 1).astype(np.int64)
    a = np.where((c0 <= c1)[:, None] & (idx == 3), 0, 255)
    return a.reshape(bh, bw, 4, 4).transpose(0, 2, 1, 3).reshape(bh * 4, bw * 4)[:h, :w]


def _alpha_blocks(a):
    """(n, 16) alpha 0-255 -> (n, 8) DXT5 alpha blocks: the lower-error six- or eight-value palette."""
    hi = np.round(a.max(1)).astype(np.int64)
    lo = np.round(a.min(1)).astype(np.int64)
    pal = np.stack([hi, lo] + [((7 - k) * hi + k * lo) // 7 for k in range(1, 7)], 1)           # (n, 8)
    idx = np.abs(a[:, :, None] - pal[:, None, :]).argmin(-1).astype(np.uint64)
    idx[hi == lo] = 0
    # Six-value mode reserves exact 0/255; fit its endpoints to the interior samples.
    # Keep eight-value mode unless the alternative reduces the decoded squared error.
    interior = (a > 0) & (a < 255)
    a0 = np.where(interior, a, 255).min(1).astype(np.int64)
    a1 = np.where(interior, a, 0).max(1).astype(np.int64)
    a0 = np.where(interior.any(1), a0, 0)
    a1 = np.where(interior.any(1), a1, 255)
    pal6 = np.stack([a0, a1] + [((5 - k) * a0 + k * a1) // 5 for k in range(1, 5)] +
                    [np.zeros_like(a0), np.full_like(a0, 255)], 1)
    idx6 = np.abs(a[:, :, None] - pal6[:, None, :]).argmin(-1).astype(np.uint64)
    err8 = ((a - np.take_along_axis(pal, idx.astype(np.int64), 1)) ** 2).sum(1)
    err6 = ((a - np.take_along_axis(pal6, idx6.astype(np.int64), 1)) ** 2).sum(1)
    six = err6 < err8
    hi, lo = np.where(six, a0, hi), np.where(six, a1, lo)
    idx = np.where(six[:, None], idx6, idx)
    bits = (idx << (3 * np.arange(16, dtype=np.uint64))).sum(1).astype(np.uint64)
    out = np.zeros((len(a), 8), np.uint8)
    out[:, 0], out[:, 1] = hi, lo
    for k in range(6):
        out[:, 2 + k] = ((bits >> np.uint64(8 * k)) & np.uint64(255)).astype(np.uint8)
    return out


def dxt5(rgba):
    """(h, w, 4) uint8 -> DXT5 blocks: each the alpha half then the colour half."""
    blk = _blocks(np.asarray(rgba), 4)
    return np.hstack([_alpha_blocks(blk[:, :, 3]), _color_blocks(blk[:, :, :3])]).tobytes()


def alpha5(level, w, h):
    """The alpha of a DXT5 level (PC block order) -> (h, w) int array (the console converter's decode gives the
    colour)."""
    bw, bh = max((w + 3) // 4, 1), max((h + 3) // 4, 1)
    blk = np.frombuffer(level[:bw * bh * 16], np.uint8).reshape(-1, 16)[:, :8].astype(np.int64)
    a0, a1 = blk[:, 0], blk[:, 1]
    big = a0 > a1
    pal = np.zeros((len(blk), 8), np.int64)
    pal[:, 0], pal[:, 1] = a0, a1
    for k in range(1, 7):
        pal[:, k + 1] = np.where(big, ((7 - k) * a0 + k * a1) // 7, 0)
    for k in range(1, 5):
        pal[:, k + 1] = np.where(big, pal[:, k + 1], ((5 - k) * a0 + k * a1) // 5)
    pal[:, 6] = np.where(big, pal[:, 6], 0)
    pal[:, 7] = np.where(big, pal[:, 7], 255)
    bits = np.zeros(len(blk), np.uint64)
    for k in range(6):
        bits |= blk[:, 2 + k].astype(np.uint64) << np.uint64(8 * k)
    idx = np.stack([(bits >> np.uint64(3 * k)) & np.uint64(7) for k in range(16)], 1).astype(np.int64)
    a = np.take_along_axis(pal, idx, 1).reshape(bh, bw, 4, 4).transpose(0, 2, 1, 3).reshape(bh * 4, bw * 4)
    return a[:h, :w]


def _color_blocks(blk, transparent=None):
    """(n, 16, 3) float -> (n, 8) uint8 DXT1 colour blocks. transparent: (n, 16) bool, the texels written as
    transparent (the blocks holding one in three-colour mode)."""
    if transparent is not None and transparent.any():
        clear = transparent.any(1)
        opaque = ~transparent
        cnt = opaque.sum(1, keepdims=True)
        fill = (blk * opaque[..., None]).sum(1) / np.maximum(cnt, 1)                # the opaque texels' mean
        blk = np.where(transparent[..., None], fill[:, None, :], blk)               # no pull on the end colours
        out = _color_blocks(blk)
        sub = blk[clear]
        mean = sub.mean(1, keepdims=True)
        d = sub - mean
        cov = np.einsum('nki,nkj->nij', d, d)
        axis = np.linalg.eigh(cov)[1][:, :, -1]
        t = np.einsum('nki,ni->nk', d, axis)
        c0, p0 = _565(np.clip(mean[:, 0] + axis * t.min(1, keepdims=True), 0, 255))
        c1, p1 = _565(np.clip(mean[:, 0] + axis * t.max(1, keepdims=True), 0, 255))
        swap = c0 > c1                                                             # three-colour: colour 0 <= 1
        c0, c1 = np.where(swap, c1, c0), np.where(swap, c0, c1)
        p0, p1 = np.where(swap[:, None], p1, p0), np.where(swap[:, None], p0, p1)
        pal = np.stack([p0, p1, (p0 + p1) / 2], 1)                                 # (n, 3, 3); index 3 transparent
        idx = ((sub[:, :, None, :] - pal[:, None, :, :]) ** 2).sum(-1).argmin(-1).astype(np.uint32)
        idx[transparent[clear]] = 3
        bits = (idx << (2 * np.arange(16, dtype=np.uint32))).sum(1).astype(np.uint32)
        o = np.zeros((len(sub), 8), np.uint8)
        o[:, 0:2] = c0.astype('<u2').view(np.uint8).reshape(-1, 2)
        o[:, 2:4] = c1.astype('<u2').view(np.uint8).reshape(-1, 2)
        o[:, 4:8] = bits.astype('<u4').view(np.uint8).reshape(-1, 4)
        out[clear] = o
        return out
    mean = blk.mean(1, keepdims=True)
    d = blk - mean
    cov = np.einsum('nki,nkj->nij', d, d)
    axis = np.linalg.eigh(cov)[1][:, :, -1]
    t = np.einsum('nki,ni->nk', d, axis)
    hi = mean[:, 0] + axis * t.max(1, keepdims=True)
    lo = mean[:, 0] + axis * t.min(1, keepdims=True)
    c0, p0 = _565(np.clip(hi, 0, 255))
    c1, p1 = _565(np.clip(lo, 0, 255))
    swap = c0 < c1
    c0, c1 = np.where(swap, c1, c0), np.where(swap, c0, c1)
    p0, p1 = np.where(swap[:, None], p1, p0), np.where(swap[:, None], p0, p1)
    pal = np.stack([p0, p1, (2 * p0 + p1) / 3, (p0 + 2 * p1) / 3], 1)              # (n, 4, 3)
    err = ((blk[:, :, None, :] - pal[:, None, :, :]) ** 2).sum(-1)                 # (n, 16, 4)
    idx = err.argmin(-1).astype(np.uint32)
    idx[c0 == c1] = 0                                                              # one colour: three-colour mode
    bits = (idx << (2 * np.arange(16, dtype=np.uint32))).sum(1).astype(np.uint32)
    out = np.zeros((len(blk), 8), np.uint8)
    out[:, 0:2] = c0.astype('<u2').view(np.uint8).reshape(-1, 2)
    out[:, 2:4] = c1.astype('<u2').view(np.uint8).reshape(-1, 2)
    out[:, 4:8] = bits.astype('<u4').view(np.uint8).reshape(-1, 4)
    return out


def _down(img, normal):
    h, w = img.shape[:2]
    a = img.astype(float)
    if h > 1:
        a = (a[0::2] + a[1::2]) / 2
    if w > 1:
        a = (a[:, 0::2] + a[:, 1::2]) / 2
    if normal:
        n = a / 127.5 - 1
        n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-6)
        a = (n + 1) * 127.5
    return np.clip(np.round(a), 0, 255).astype(np.uint8)


def levels(img, normal=False, channels=3):
    """The picture and its mip levels down to 1 x 1."""
    out = [np.asarray(img)[..., :channels].astype(np.uint8)]
    while max(out[-1].shape[:2]) > 1:
        out.append(_down(out[-1], normal))
    return out


def texture(img, kind='color', mips=True):
    """A PC ITexture resource. kind: 'color' (DXT1, format 0), 'normal' (DXT1, format 18) or 'dxt5' (RGBA, format
    2: colour with alpha); mips: every level down to 1 x 1, else level 0 only (DICE's award images have one).
    'color' from an (h, w, 4) picture keeps DXT1's 1-bit alpha (fourth channel under 128: transparent; each mip level's
    alpha the 2 x 2 mean, cut at 128 again)."""
    img = np.asarray(img)
    h, w = img.shape[:2]
    if h & (h - 1) or w & (w - 1) or not h or not w:
        raise TexWriteError(f'{w} x {h}: sides must be powers of two')
    normal = kind == 'normal'
    if kind == 'dxt5' and (img.ndim != 3 or img.shape[2] != 4):
        raise TexWriteError('dxt5 takes an (h, w, 4) picture')
    punch = kind == 'color' and img.ndim == 3 and img.shape[2] == 4    # DXT1 with 1-bit alpha (dxt1)
    lv = levels(img, normal, 4 if kind == 'dxt5' or punch else 3)
    if not mips:
        lv = lv[:1]
    if len(lv) > 15:
        raise TexWriteError(f'{w} x {h}: more than 15 levels')
    data = [dxt5(x) if kind == 'dxt5' else dxt1(x) for x in lv]
    sizes = [len(d) for d in data] + [0] * (15 - len(data))
    fmt = {'color': COLOR, 'normal': NORMAL, 'dxt5': DXT5}[kind]
    head = struct.pack('<8I', 3, 0, fmt, 0 if normal else 1, w, h, 0, len(data))
    return head + struct.pack('<15I', *sizes) + b''.join(data)


L8, BC5 = 10, 20


def resource(fmt, flags, w, h, data):
    """A PC ITexture resource from its levels as they are (bytes each, largest first): the 92-byte header (version 3,
    type 0, depth 0) and the levels after it - for textures composed level by level (emitteratlas.py's atlases)."""
    if len(data) > 15:
        raise TexWriteError(f'{w} x {h}: more than 15 levels')
    sizes = [len(d) for d in data] + [0] * (15 - len(data))
    return struct.pack('<8I', 3, 0, fmt, flags, w, h, 0, len(data)) + struct.pack('<15I', *sizes) + b''.join(data)


def l8_down(level):
    """The next level of an 8-bit texture (format 10) as DICE builds its emitter atlases: each texel the rounded mean
    of its 2 x 2 block, (a + b + c + d + 2) // 4 (every level of mp_005's and bc1_harvest_day_cq's atlases is exactly
    this of the one above)."""
    a = np.asarray(level).astype(np.int32)
    return ((a[0::2, 0::2] + a[1::2, 0::2] + a[0::2, 1::2] + a[1::2, 1::2] + 2) // 4).astype(np.uint8)


def l8_bytes(level):
    """An 8-bit level's bytes as DICE stores them: rows padded to 4 bytes (mp_005's 2 x 2 level is 8 bytes)."""
    a = np.asarray(level, np.uint8)
    h, w = a.shape
    if w >= 4:
        return a.tobytes()
    out = np.zeros((h, 4), np.uint8)
    out[:, :w] = a
    return out.tobytes()


def atlas_levels(shorter):
    """DICE's level count for an emitter atlas whose shorter side is `shorter` texels: down to a shorter side of 2
    (2048 x 2048: 11 levels, 2048 x 1024: 10, the 1024 x 512 normals: 9)."""
    n = 0
    while (1 << n) < shorter:
        n += 1
    return n


def wrapped(res, kind='ITexture'):
    """A resource in the on-demand bundles' form (every entry of async\\ondemand_awards-00.fbrb has it): 'RES\\x02',
    the type name zero-padded to 56 bytes in all, the payload's size twice (u32), the payload."""
    head = b'RES\x02' + kind.encode('ascii')
    return head + b'\0' * (56 - len(head)) + struct.pack('<2I', len(res), len(res)) + res
