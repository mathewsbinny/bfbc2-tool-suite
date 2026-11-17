r"""Xbox 360 -> PC conversion of textures: XenonTexture resources to ITexture. Format: FORMAT.md section 2.

The console keeps a texture's blocks in the Xenos GPU's tiled order (32 x 32-block tiles, the smallest mip levels
packed into one tile, cube faces level by level) with 16-bit words byte-swapped; the PC keeps them in rows, level
after level (face after face). The blocks themselves are not touched: the result draws exactly what the console
draws.

    import texture
    pc_bytes = texture.convert(console_bytes)

convert runs in the suite's Rust core (rust/bc2-convert, through bfbc2core's bc2_native), which it needs: without
the module (or with one built from other sources) it raises bfbc2core.native.NativeMissing saying how to get it
(README.md, "The Rust core"); BFBC2_PURE_PYTHON does not change that. The Python converter that was its reference
was retired on 2026-11-17, after the port had given the same bytes on every texture of BF1943's source and the
Onslaught builds and passed the full test tier; its results are recorded in tests/texture_golden.json, which
tests/test_texture_native.py checks. The layout functions below (tiled, packed, layout) stay: they describe the
console's format, and texqa.py's independent decoder and the tests build console textures with them.
"""
import os
import struct
import sys

HEADER = 92                    # 8 u32 (version, type, format, flags, width, height, depth, mip count) + 15 mip sizes
CUBE = 1
# format code (the same on both platforms) -> (bytes per block, block edge in texels, name)
FORMATS = {0: (8, 4, 'DXT1'), 18: (8, 4, 'DXT1 (normal map)'), 1: (16, 4, 'DXT3'), 2: (16, 4, 'DXT5'),
           19: (16, 4, 'BC5 / DXN'), 20: (16, 4, 'BC5 / DXN (20)'), 3: (8, 4, 'DXT5A (console only: decoded to PC 10)'),
           13: (8, 1, 'RGBA16F (half floats; untiled as they are)'),
           5: (2, 1, 'RGB565 (console only: decoded to PC 9, B8G8R8A8)'), 10: (1, 1, 'L8 (8-bit, one channel)'),
           9: (4, 1, 'A8R8G8B8 (big-endian ARGB words: PC 9, B8G8R8A8)')}
# console codes whose PC counterpart is another format (not converted: FORMAT.md 2.4)
OTHER: dict[int, str] = {}


DECODED = {0, 18, 1, 2, 19, 20, 10}     # the formats decode() reads (DXT1, DXT3, DXT5, BC5; L8): check() compares


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
    where: list[list[tuple[int, ...] | None]] = [[None] * mips for _ in range(slices)]
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


# the console-only formats convert decodes (FORMAT.md 2): DXT5A (3) to the PC's 8-bit L8 (10), level by level, flag bit
# 1 cleared; RGB565 (5) and big-endian A8R8G8B8 (9) to B8G8R8A8 (9)
DXT5A, L8 = 3, 10
RGB565, BGRA8 = 5, 9
HALVED_ON_CONSOLE = 2       # header flag bit 1: DICE's build halves the texture for the console (FORMAT.md 2.1)


def require():
    """The Rust core, which converts (bfbc2core.native.require): NativeMissing, saying how to get it, when it is not
    built or downloaded or was built from other sources. A tool that converts calls this when it starts, so it
    stops before doing other work."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('Texture conversion (bfbc2-console-convert/texture.py)', 'texture_convert')


def convert(b):
    """A console XenonTexture resource -> the PC ITexture resource; ConvertError (with the reason) for a texture
    that is not converted."""
    try:
        return require().texture_convert(bytes(b))
    except ValueError as e:
        raise ConvertError(str(e)) from None


def convert_many(items):
    """convert of many textures on all cores, in order: each result the PC bytes or the ConvertError (returned, not
    raised)."""
    return [ConvertError(str(r)) if isinstance(r, ValueError) else r
            for r in require().texture_convert_many([bytes(b) for b in items])]


# ---------------------------------------------------------------- checking (numpy)

def decode(level, w, h, fmt):
    """One level's blocks (PC order) -> (h, w, 3) int array: DXT1 / DXT3 / DXT5 / BC5 (red, green; blue 0); and the
    8-bit format 10 (the value in all three; rows as stored: DICE pads a row to 4 bytes, the converter does not)."""
    import numpy as np
    if fmt == L8:
        w, h = max(w, 1), max(h, 1)
        pitch = max(w, len(level) // h)
        a = np.frombuffer(level[:pitch * h], np.uint8).reshape(h, pitch)[:, :w].astype(np.int32)
        return np.repeat(a[:, :, None], 3, 2)
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
    same where it is the same art. Every format decode() reads is compared (DECODED: the block formats and L8); for
    any other (RGBA16F, B8G8R8A8, ...) nothing is measured and the result is (None, None, None, None) - not
    compared, which a caller must not read as a pass."""
    import numpy as np
    fmt = struct.unpack_from('<I', pc, 8)[0]
    if fmt not in DECODED:
        return None, None, None, None
    lv = levels(pc)
    worst = 0.0
    for (s, _m, w, h, d), (s2, _m2, w2, h2, d2) in zip(lv, lv[1:]):
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
        if len(reference) >= HEADER + total * bpb:
            x = np.frombuffer(pc, np.uint8, total * bpb, HEADER).reshape(total, bpb)
            y = np.frombuffer(reference, np.uint8, total * bpb, HEADER).reshape(total, bpb)
            same = int((x == y).all(1).sum())
        else:
            same = sum(pc[HEADER + i * bpb:HEADER + (i + 1) * bpb] == reference[HEADER + i * bpb:HEADER + (i + 1) * bpb]
                       for i in range(total))
    return worst, corr, same, total
