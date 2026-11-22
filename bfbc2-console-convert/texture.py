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

The checking helpers levels, decode (round F0) and check (round F2a) run in the Rust core; their Python
(levels_reference, decode_reference, check_reference) was retired in round RT2 against its recorded answers
(bfbc2-console-convert\\tests\\f0_texture_replay*.json, f2a_console_replay*.json): a texture or level the core
does not read (cut short or damaged, a format decode does not know) raises a ConvertError that is also a
bfbc2core.native.Refused.
"""
import os
import sys
from typing import Any

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


_NATIVE: list[Any] = []


def _native() -> Any:
    """The Rust core with texture_decode (the layout helpers and the PC reader), or None: not built, or
    BFBC2_PURE_PYTHON set (convert always runs in the Rust core)."""
    if not _NATIVE:
        m = None
        try:
            root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
            if root not in sys.path:
                sys.path.insert(0, root)
            import suite_paths
            suite_paths.use('core')
            from bfbc2core import dbx as _dbx
            if os.environ.get(_dbx.PURE_PYTHON_ENV, '') in ('', '0'):
                m = _dbx._import_native()
        except ImportError:
            m = None
        _NATIVE.append(m if m is not None and hasattr(m, 'texture_decode') else None)
    return _NATIVE[0]


def use_native(on: bool) -> bool:
    """The Rust layout helpers and PC reader on (when built, whatever BFBC2_PURE_PYTHON says) or off for this process;
    True when they run."""
    _NATIVE.clear()
    if not on:
        _NATIVE.append(None)
        return False
    saved = os.environ.pop('BFBC2_PURE_PYTHON', None)
    try:
        return _native() is not None
    finally:
        if saved is not None:
            os.environ['BFBC2_PURE_PYTHON'] = saved


def _ints(*vs):
    return all(type(v) is int for v in vs)


def _log2c(v):
    n = 0
    while (1 << n) < v:
        n += 1
    return n


def _align(v, a):
    return (v + a - 1) // a * a


def tiled(x, y, pitch, log_bpp):
    """Xenos 2D tiling (XGAddress2DTiledOffset): block (x, y) of a surface pitch blocks wide -> block index. In the
    Rust core for ints (round F0), else tiled_reference (NumPy arrays too)."""
    nat = _native() if _ints(x, y, pitch, log_bpp) else None
    got = nat.texture_tiled(x, y, pitch, log_bpp) if nat is not None else None
    return got if got is not None else tiled_reference(x, y, pitch, log_bpp)


def tiled_reference(x, y, pitch, log_bpp):
    """The Python tiling (the reference)."""
    aw = (pitch + 31) & ~31
    macro = ((x >> 5) + (y >> 5) * (aw >> 5)) << (log_bpp + 7)
    micro = ((x & 7) + ((y & 6) << 2)) << log_bpp
    off = macro + ((micro & ~15) << 1) + (micro & 15) + ((y & 8) << (3 + log_bpp)) + ((y & 1) << 4)
    return (((off & ~511) << 3) + ((off & 448) << 2) + (off & 63) + ((y & 16) << 7) +
            (((((y & 8) >> 2) + (x >> 3)) & 3) << 6)) >> log_bpp


def packed(w, h, mip, edge):
    """Block offset (x, y) of mip level mip inside the packed tail tile, or None when the level has its own
    storage. A level is packed once its shorter side is 16 texels or less. In the Rust core for ints (round F0), else
    packed_reference."""
    nat = _native() if _ints(w, h, mip, edge) else None
    if nat is not None:
        answered, got = nat.texture_packed(w, h, mip, edge)
        if answered:
            return got
    return packed_reference(w, h, mip, edge)


def packed_reference(w, h, mip, edge):
    """The Python rule (the reference)."""
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
    Level 0 of every slice first, then level by level (each level all slices); every storage 4 KB aligned. In the Rust
    core for ints (round F0), else layout_reference."""
    nat = _native() if _ints(w, h, mips, fmt, slices) else None
    got = nat.texture_layout(w, h, mips, fmt, slices) if nat is not None else None
    if got is not None:
        return got[0], got[1]
    return layout_reference(w, h, mips, fmt, slices)


def layout_reference(w, h, mips, fmt, slices=1):
    """The Python layout (the reference)."""
    bpb, edge, _ = FORMATS[fmt]
    blocks = lambda v: max((v + edge - 1) // edge, 1)     # noqa: E731
    bw, bh = blocks(w), blocks(h)
    base_pitch = _align(bw, 32)
    base_size = _align(base_pitch * _align(bh, 32) * bpb, 4096)
    where: list[list[tuple[int, ...] | None]] = [[None] * mips for _ in range(slices)]
    pos = base_size * slices
    tail = None                                           # (start, pitch, size)
    for m in range(mips):
        pk = packed_reference(w, h, m, edge)
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
    8-bit format 10 (the value in all three; rows as stored: DICE pads a row to 4 bytes, the converter does not). In
    the Rust core (round F0)."""
    import numpy as np
    if isinstance(level, bytearray | memoryview):
        level = bytes(level)
    got = require().texture_decode(level, w, h, fmt) if type(level) is bytes else None
    if got is None:
        raise _refused(f'decode: a {w} x {h} level of format {fmt!r} the decoder does not read (cut short, or a '
                       'format it does not know)')
    rows, cols, px = got
    return np.frombuffer(px, np.int32).reshape(rows, cols, 3)


def _refused(message):
    from bfbc2core import native
    return native.refused(message, ConvertError)


def levels(pc):
    """[(slice, level, w, h, bytes)] of a PC ITexture resource: in the Rust core (round F0)."""
    if isinstance(pc, bytearray | memoryview):
        pc = bytes(pc)
    got = require().texture_levels(pc) if type(pc) is bytes else None
    if got is None:
        raise _refused('levels: a PC texture the reader does not read (cut short or damaged)')
    return got


def check(pc, reference=None):
    """(worst mip-chain error, correlation with reference's level 0 or None, blocks identical, blocks): each level
    should be about its larger neighbour scaled down; a reference (the PC's own file of the asset) should look the
    same where it is the same art. Every format decode() reads is compared (DECODED: the block formats and L8); for
    any other (RGBA16F, B8G8R8A8, ...) nothing is measured and the result is (None, None, None, None) - not
    compared, which a caller must not read as a pass. All in the Rust core (round F2a: bc2_convert::texcheck::check;
    the mip-chain error NumPy's value to the bit). The correlation is np.corrcoef's, computed exactly (round CI1,
    spec rule XCONS-PAR-2: integer sums, one rounding each) - NumPy's own last bits follow the CPU's BLAS kernel."""
    if isinstance(pc, bytearray | memoryview):
        pc = bytes(pc)
    if isinstance(reference, bytearray | memoryview):
        reference = bytes(reference)
    got = None
    if type(pc) is bytes and (reference is None or type(reference) is bytes):
        got = require().texture_check(pc, reference)
    if got is None:
        raise _refused('check: a PC texture (or reference) the checker does not read (cut short or damaged)')
    if got[1] is None:
        return None, None, None, None
    worst, corr, same, total = got[1]
    return worst, corr, same, total


