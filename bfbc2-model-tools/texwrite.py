"""PC textures (ITexture .res) from pictures: DXT1 colour maps (format 0, flags 1) and DXT1 normal maps (format 18,
flags 0), the layouts of DICE's own weapon textures. The header and level layout: the console converter's FORMAT.md
section 2 (92 bytes: version 3, type 0, format, flags, width, height, depth 0, mip count, 15 level sizes; the PC's
levels in rows, level after level, down to 1 x 1, a level never under one 8-byte block).

    import texwrite
    open('x_D.res', 'wb').write(texwrite.texture(rgb))              # rgb: (h, w, 3) uint8, sides powers of two
    open('x_N.res', 'wb').write(texwrite.texture(rgb, 'normal'))    # tangent-space normals, z in blue

The encoder: per 4 x 4 block, the colours' principal axis (power iteration) gives the two end colours (the
extremes of the block projected on it, rounded to 5:6:5), four-colour mode always (colour 0 > colour 1), each texel
the nearest of the four. Mip levels: 2 x 2 box filter; normal maps renormalised after each step.
"""
import struct

import numpy as np

COLOR, NORMAL = 0, 18
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


def dxt1(rgb):
    """(h, w, 3) uint8 -> DXT1 blocks (rows of blocks), sides multiples of 4 or under 4 (padded by edge copies)."""
    h, w = rgb.shape[:2]
    ph, pw = max(4, -(-h // 4) * 4), max(4, -(-w // 4) * 4)
    img = np.pad(rgb.astype(float), ((0, ph - h), (0, pw - w), (0, 0)), mode='edge')
    bh, bw = ph // 4, pw // 4
    blk = img.reshape(bh, 4, bw, 4, 3).transpose(0, 2, 1, 3, 4).reshape(-1, 16, 3)
    mean = blk.mean(1, keepdims=True)
    d = blk - mean
    cov = np.einsum('nki,nkj->nij', d, d)
    axis = np.ones((len(blk), 3)) / np.sqrt(3)
    for _ in range(8):
        axis = np.einsum('nij,nj->ni', cov, axis)
        axis /= np.maximum(np.linalg.norm(axis, axis=1, keepdims=True), 1e-9)
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
    return out.tobytes()


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


def levels(img, normal=False):
    """The picture and its mip levels down to 1 x 1."""
    out = [np.asarray(img)[..., :3].astype(np.uint8)]
    while max(out[-1].shape[:2]) > 1:
        out.append(_down(out[-1], normal))
    return out


def texture(img, kind='color'):
    """A PC ITexture resource: DXT1 with every mip level. kind: 'color' (format 0) or 'normal' (format 18)."""
    img = np.asarray(img)
    h, w = img.shape[:2]
    if h & (h - 1) or w & (w - 1) or not h or not w:
        raise TexWriteError(f'{w} x {h}: sides must be powers of two')
    normal = kind == 'normal'
    lv = levels(img, normal)
    if len(lv) > 15:
        raise TexWriteError(f'{w} x {h}: more than 15 levels')
    data = [dxt1(x) for x in lv]
    sizes = [len(d) for d in data] + [0] * (15 - len(data))
    head = struct.pack('<8I', 3, 0, NORMAL if normal else COLOR, 0 if normal else 1, w, h, 0, len(data))
    return head + struct.pack('<15I', *sizes) + b''.join(data)
