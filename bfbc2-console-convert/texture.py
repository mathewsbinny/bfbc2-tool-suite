r"""Xbox 360 -> PC conversion of textures: XenonTexture resources to ITexture. Format: FORMAT.md section 2.

The console keeps a texture's blocks in the Xenos GPU's tiled order (32 x 32-block tiles, the smallest mip levels
packed into one tile, cube faces level by level) with 16-bit words byte-swapped; the PC keeps them in rows, level
after level (face after face). The blocks themselves are not touched: the result draws exactly what the console
draws.

    import texture
    pc_bytes = texture.convert(console_bytes)
"""
import struct

HEADER = 92                    # 8 u32 (version, type, format, flags, width, height, depth, mip count) + 15 mip sizes
CUBE = 1
# format code (the same on both platforms) -> (bytes per block, block edge in texels, name)
FORMATS = {0: (8, 4, 'DXT1'), 18: (8, 4, 'DXT1 (normal map)'), 1: (16, 4, 'DXT3'), 2: (16, 4, 'DXT5'),
           19: (16, 4, 'BC5 / DXN'), 20: (16, 4, 'BC5 / DXN (20)')}
# console codes whose PC counterpart is another, uncompressed, format (not converted yet: FORMAT.md 2.4)
OTHER = {3: 'console 3 (PC 10, 8-bit)', 5: 'console 5 (PC 9, 32-bit)', 9: '9 (32-bit)', 10: '10 (8-bit)'}


class ConvertError(Exception):
    pass


def _log2c(v):
    n = 0
    while (1 << n) < v:
        n += 1
    return n


def _align(v, a):
    return (v + a - 1) // a * a


def tiled(x, y, pitch, log_bpp):
    """Xenos 2D tiling (XGAddress2DTiledOffset): block (x, y) of a surface pitch blocks wide -> block index."""
    aw = (pitch + 31) & ~31
    macro = ((x >> 5) + (y >> 5) * (aw >> 5)) << (log_bpp + 7)
    micro = ((x & 7) + ((y & 6) << 2)) << log_bpp
    off = macro + ((micro & ~15) << 1) + (micro & 15) + ((y & 8) << (3 + log_bpp)) + ((y & 1) << 4)
    return (((off & ~511) << 3) + ((off & 448) << 2) + (off & 63) + ((y & 16) << 7) +
            (((((y & 8) >> 2) + (x >> 3)) & 3) << 6)) >> log_bpp


def packed(w, h, mip, edge):
    """Block offset (x, y) of mip level mip inside the packed tail tile, or None when the level has its own
    storage. A level is packed once its shorter side is 16 texels or less."""
    lw, lh = _log2c(w), _log2c(h)
    ls = min(lw, lh)
    if ls > 4 + mip:
        return None
    base = ls - 4 if ls > 4 else 0
    pm = mip - base
    if pm < 3:
        x, y = (0, 16 >> pm) if lw > lh else (16 >> pm, 0)
    elif lw > lh:
        x, y = (1 << (lw - base)) >> (pm - 2), 0
    else:
        x, y = 0, (1 << (lh - base)) >> (pm - 2)
    return x // edge, y // edge


def layout(w, h, mips, fmt, slices=1):
    """Per slice, per level: (byte start, pitch in blocks, x, y block offset) in the console data; and its size.
    Level 0 of every slice first, then level by level (each level all slices); every storage 4 KB aligned."""
    bpb, edge, _ = FORMATS[fmt]
    blocks = lambda v: max((v + edge - 1) // edge, 1)     # noqa: E731
    bw, bh = blocks(w), blocks(h)
    base_pitch = _align(bw, 32)
    base_size = _align(base_pitch * _align(bh, 32) * bpb, 4096)
    where = [[None] * mips for _ in range(slices)]
    pos = base_size * slices
    tail = None                                           # (start, pitch, size)
    for m in range(mips):
        pk = packed(w, h, m, edge)
        if m == 0:
            if pk is not None:
                tail = (0, base_pitch, base_size)
            for s in range(slices):
                where[s][0] = (s * base_size, base_pitch) + (pk or (0, 0))
            continue
        sbw, sbh = blocks(max((1 << _log2c(w)) >> m, 1)), blocks(max((1 << _log2c(h)) >> m, 1))
        pitch = _align(sbw, 32)
        size = _align(pitch * _align(sbh, 32) * bpb, 4096)
        if pk is None:
            for s in range(slices):
                where[s][m] = (pos + s * size, pitch, 0, 0)
            pos += size * slices
        else:
            if tail is None:
                tail = (pos, pitch, size)
                pos += size * slices
            for s in range(slices):
                where[s][m] = (tail[0] + s * tail[2], tail[1]) + pk
    return where, pos


def convert(b):
    """A console XenonTexture resource -> the PC ITexture resource."""
    if len(b) < HEADER:
        raise ConvertError('shorter than a texture header')
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('>8I', b, 0)
    if fmt not in FORMATS:
        raise ConvertError(f'texture format {OTHER.get(fmt, fmt)} is not converted (FORMAT.md 2.4)')
    if typ not in (0, CUBE) or depth not in (0, 1):
        raise ConvertError(f'texture type {typ} / depth {depth} is not converted (2D and cube maps only)')
    bpb, edge, _ = FORMATS[fmt]
    log_bpp = {8: 3, 16: 4}[bpb]
    slices = 6 if typ == CUBE else 1
    where, size = layout(w, h, mips, fmt, slices)
    data = bytearray(b[HEADER:])
    if len(data) < size:
        raise ConvertError(f'{len(data)} bytes of texture data, the layout needs {size}')
    for i in range(0, len(data) - 1, 2):                  # 16-bit words are byte-swapped on the console
        data[i], data[i + 1] = data[i + 1], data[i]
    levels = []
    for s in range(slices):
        for m in range(mips):
            start, pitch, ox, oy = where[s][m]
            lbw, lbh = max((max(w >> m, 1) + edge - 1) // edge, 1), max((max(h >> m, 1) + edge - 1) // edge, 1)
            lev = bytearray(lbw * lbh * bpb)
            for y in range(lbh):
                for x in range(lbw):
                    src = start + tiled(x + ox, y + oy, pitch, log_bpp) * bpb
                    lev[(y * lbw + x) * bpb:(y * lbw + x + 1) * bpb] = data[src:src + bpb]
            levels.append(bytes(lev))
    sizes = [len(l) for l in levels[:mips]] + [0] * (15 - mips)
    return struct.pack('<8I', ver, typ, fmt, flags, w, h, depth, mips) + struct.pack('<15I', *sizes) + b''.join(levels)


# ---------------------------------------------------------------- checking (numpy)

def decode(level, w, h, fmt):
    """One level's blocks (PC order) -> (h, w, 4) int array: DXT1 / DXT3 / DXT5 / BC5."""
    import numpy as np
    bpb = FORMATS[fmt][0]
    bw, bh = max((w + 3) // 4, 1), max((h + 3) // 4, 1)
    blk = np.frombuffer(level[:bw * bh * bpb], np.uint8).reshape(-1, bpb)

    def color(c, force4):
        c0 = c[:, 0].astype(np.int32) | (c[:, 1].astype(np.int32) << 8)
        c1 = c[:, 2].astype(np.int32) | (c[:, 3].astype(np.int32) << 8)
        rgb = lambda v: np.stack([((v >> 11) & 31) * 255 // 31, ((v >> 5) & 63) * 255 // 63,   # noqa: E731
                                  (v & 31) * 255 // 31], -1)
        p0, p1 = rgb(c0), rgb(c1)
        four = (c0 > c1) | force4
        pal = np.stack([p0, p1, np.where(four[:, None], (2 * p0 + p1) // 3, (p0 + p1) // 2),
                        np.where(four[:, None], (p0 + 2 * p1) // 3, 0)], 1)
        bits = c[:, 4:8].astype(np.uint32)
        bits = bits[:, 0] | bits[:, 1] << 8 | bits[:, 2] << 16 | bits[:, 3] << 24
        idx = np.stack([(bits >> (2 * k)) & 3 for k in range(16)], 1)
        return np.take_along_axis(pal, idx[:, :, None].repeat(3, 2), 1)

    def alpha(a):
        a0, a1 = a[:, 0].astype(np.int32), a[:, 1].astype(np.int32)
        big = a0 > a1
        pal = np.zeros((len(a), 8), np.int32)
        pal[:, 0], pal[:, 1] = a0, a1
        for k in range(1, 7):                               # 8-value mode (a0 > a1): six interpolated values
            pal[:, k + 1] = ((7 - k) * a0 + k * a1) // 7
        for k in range(1, 5):                               # 6-value mode: four interpolated, then 0 and 255
            pal[:, k + 1] = np.where(big, pal[:, k + 1], ((5 - k) * a0 + k * a1) // 5)
        pal[:, 6] = np.where(big, pal[:, 6], 0)
        pal[:, 7] = np.where(big, pal[:, 7], 255)
        bits = np.zeros(len(a), np.uint64)
        for k in range(6):
            bits |= a[:, 2 + k].astype(np.uint64) << np.uint64(8 * k)
        idx = np.stack([(bits >> np.uint64(3 * k)) & np.uint64(7) for k in range(16)], 1).astype(np.int64)
        return np.take_along_axis(pal, idx, 1)

    if fmt in (0, 18):
        px = color(blk, False)
    elif fmt in (1, 2):
        px = color(blk[:, 8:], True)
    else:
        px = np.stack([alpha(blk[:, :8]), alpha(blk[:, 8:]), np.zeros((len(blk), 16), np.int64)], 2)
    img = px.reshape(bh, bw, 4, 4, 3).transpose(0, 2, 1, 3, 4).reshape(bh * 4, bw * 4, 3)
    return img[:max(h, 1), :max(w, 1)].astype(np.int32)


def levels(pc):
    """[(slice, level, w, h, bytes)] of a PC ITexture resource."""
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('<8I', pc, 0)
    sizes = struct.unpack_from('<15I', pc, 32)
    out, pos = [], HEADER
    for s in range(6 if typ == CUBE else 1):
        for m in range(mips):
            out.append((s, m, max(w >> m, 1), max(h >> m, 1), pc[pos:pos + sizes[m]]))
            pos += sizes[m]
    return out


def check(pc, reference=None):
    """(worst mip-chain error, correlation with reference's level 0 or None, blocks identical, blocks): each level
    should be about its larger neighbour scaled down; a reference (the PC's own file of the asset) should look the
    same where it is the same art."""
    import numpy as np
    fmt = struct.unpack_from('<I', pc, 8)[0]
    lv = levels(pc)
    worst = 0.0
    for (s, m, w, h, d), (s2, m2, w2, h2, d2) in zip(lv, lv[1:]):
        if s != s2 or w < 8 or h < 8:
            continue
        a, b = decode(d, w, h, fmt), decode(d2, w2, h2, fmt)
        small = a[:h // 2 * 2, :w // 2 * 2].reshape(h // 2, 2, w // 2, 2, 3).mean((1, 3))
        worst = max(worst, float(np.abs(small - b[:small.shape[0], :small.shape[1]]).mean()))
    corr = same = total = None
    if reference is not None and reference[:HEADER] == pc[:HEADER]:
        s0, m0, w, h, d = lv[0]
        r = levels(reference)[0][4]
        a, b = decode(d, w, h, fmt).ravel(), decode(r, w, h, fmt).ravel()
        corr = float(np.corrcoef(a, b)[0, 1]) if a.std() and b.std() else 1.0
        bpb = FORMATS[fmt][0]
        total = len(pc[HEADER:]) // bpb
        same = sum(pc[HEADER + i * bpb:HEADER + (i + 1) * bpb] == reference[HEADER + i * bpb:HEADER + (i + 1) * bpb]
                   for i in range(total))
    return worst, corr, same, total
