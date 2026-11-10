r"""texqa.py, the converted-texture QA (quick tier: synthetic textures, no game data).

- decoding: a DXT1 / DXT5 / BC5 texture texwrite writes decodes back to its picture; DXT1's 1-bit alpha, DXT3's
  explicit alpha, L8 with DICE's padded rows, BGRA8 and RGBA16F (NaN found);
- the Xbox side: a PC texture tiled into the console's layout (the test's own encoder, checked against
  texture.convert) decodes to exactly the PC texture's levels, a packed mip tail included;
- the checks: a texture equal to its source passes with no error; flipped, mirrored, channel-swapped, all black, a
  level that is not its parent's mip, a normal map with the sRGB bit, BC5's channels swapped against DICE's copy - each
  flagged, with its cause; DICE's copy twice the size with flags bit 1 is reported as DICE's known difference; a grime
  repaint (.orig beside it) is intentional;
- inputs: loose textures recognised by their header, unpacked bundles by their manifest (on-demand wrapped entries
  unwrapped), .orig picked up; the cache: a second run analyses nothing;
- the baseline: a new error flag and a PSNR drop are regressions, an unchanged run is not.

  python test_texqa.py
"""
import json
import os
import struct
import sys
import tempfile

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'model-tools', 'core')
import texqa  # noqa: E402
import texture  # noqa: E402
import texwrite  # noqa: E402

FAILED: list[str] = []


def check(ok: bool, what: str) -> None:
    print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)
    if not ok:
        FAILED.append(what)


def picture(w: int = 64, h: int = 64, seed: int = 1) -> np.ndarray:
    """A smooth colour picture with a feature in one corner (so flips and swaps show)."""
    y, x = np.mgrid[0:h, 0:w]
    rng = np.random.default_rng(seed)
    img = np.stack([x * 255 // max(w - 1, 1), y * 255 // max(h - 1, 1), (x + y) * 127 // max(w + h - 2, 1)], -1)
    img[: h // 4, : w // 4] = [250, 30, 30]
    img = img + rng.integers(0, 6, img.shape)
    return np.clip(img, 0, 255).astype(np.uint8)


def to_xbox(pc: bytes) -> bytes:
    """A PC ITexture (2D) tiled into the console's XenonTexture layout: texture.layout's storage, texture.tiled's
    block order, 16-bit words swapped, a big-endian header with the layout's size."""
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('<8I', pc, 0)
    bpb, edge, _ = texture.FORMATS[fmt]
    log_bpp = {1: 0, 2: 1, 8: 3, 16: 4}[bpb]
    where, size = texture.layout(w, h, mips, fmt, 1)
    data = bytearray(size)
    for s, m, lw, lh, lev in texture.levels(pc):
        start, pitch, ox, oy = where[s][m]
        lbw, lbh = max((lw + edge - 1) // edge, 1), max((lh + edge - 1) // edge, 1)
        for y in range(lbh):
            for x in range(lbw):
                dst = start + texture.tiled(x + ox, y + oy, pitch, log_bpp) * bpb
                data[dst:dst + bpb] = lev[(y * lbw + x) * bpb:(y * lbw + x + 1) * bpb]
    if bpb > 1:
        data[0::2], data[1::2] = data[1::2], data[0::2]
    return struct.pack('>8I', ver, typ, fmt, flags, w, h, depth, mips) + struct.pack('>15I', size, *[0] * 14) + bytes(data)


def bc5(img2: np.ndarray, flags: int = 0) -> bytes:
    """A BC5 texture (format 20) of a two-channel picture, every level (the alpha-half encoder for each channel)."""
    levels = []
    a = img2.astype(np.uint8)
    while True:
        h, w = a.shape[:2]
        blocks = []
        for c in range(2):
            ch = a[..., c:c + 1]
            blocks.append(texwrite._alpha_blocks(texwrite._blocks(ch, 1)[:, :, 0]))
        levels.append(np.hstack(blocks).tobytes())
        if max(h, w) == 1:
            break
        a = np.clip(np.round(texqa.box(a)), 0, 255).astype(np.uint8)
    return texwrite.resource(20, flags, img2.shape[1], img2.shape[0], levels)


def errors_of(row: dict) -> set[str]:
    return {f['code'] for f in row['flags'] if f['severity'] == texqa.ERROR}


def test_decode() -> None:
    img = picture()
    t = texqa.read_pc(texwrite.texture(img))
    a0 = t.level(0)
    check(a0 is not None and texqa.psnr(a0[..., :3].astype(float), img.astype(float)) > 30 and t.mips == 7,
          'DXT1 decodes to its picture, every level present')
    rgba = np.concatenate([img, np.tile(np.arange(64, dtype=np.uint8)[None, :, None] * 4, (64, 1, 1))], 2)
    t5 = texqa.read_pc(texwrite.texture(rgba, 'dxt5', mips=False))
    a5 = t5.level(0)
    check(a5 is not None and np.abs(a5[..., 3].astype(int) - rgba[..., 3]).max() <= 6, 'DXT5 alpha decodes')
    # DXT1 1-bit alpha: colour 0 <= colour 1, index 3 transparent
    blk = bytes([0, 0, 255, 255]) + bytes([0xFF] * 4)
    lv, _ = texqa.decode_level(blk, 4, 4, 0)
    check(int(lv[..., 3].max()) == 0, "DXT1's three-colour mode index 3 is transparent")
    # DXT3 explicit alpha
    blk3 = bytes([0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE]) + bytes([0xFF, 0xFF, 0, 0, 0, 0, 0, 0])
    lv3, _ = texqa.decode_level(blk3, 4, 4, 1)
    check(lv3[..., 3].ravel().tolist() == [k * 17 for k in range(16)], 'DXT3 explicit alpha (4 bits, x 17)')
    # L8 rows padded to 4 bytes (a 2 x 2 level is 8 bytes)
    lv8, _ = texqa.decode_level(bytes([1, 2, 0, 0, 3, 4, 0, 0]), 2, 2, 10)
    check(lv8[..., 0].tolist() == [[1, 2], [3, 4]], "L8's padded rows")
    lvb, _ = texqa.decode_level(bytes([10, 20, 30, 40]), 1, 1, 9)
    check(lvb[0, 0].tolist() == [30, 20, 10, 40], 'BGRA8 read as RGBA')
    half = np.array([[0.5, float('nan'), 1.0, 1.0]], '<f2').tobytes()
    lvh, f = texqa.decode_level(half, 1, 1, 13)
    t13 = texqa.Tex(13, 0, 0, 1, 1, 1, [(0, 0, lvh)], [f] if f is not None else [])
    check(any(c.startswith('degenerate: NaN') for c, _ in texqa.degenerate(t13)), 'RGBA16F: a NaN found')


def test_xbox() -> None:
    for w, h, kind in ((64, 64, 'color'), (256, 64, 'color'), (32, 32, 'dxt5'), (8, 8, 'color')):
        img = picture(w, h)
        pc = texwrite.texture(np.concatenate([img, np.full((h, w, 1), 200, np.uint8)], 2) if kind == 'dxt5' else img, kind)
        xb = to_xbox(pc)
        check(texture.convert(xb) == pc, f'{w} x {h} {kind}: the test encoder is texture.convert\'s inverse')
        a, b = texqa.read_xbox(xb), texqa.read_pc(pc)
        same = len(a.levels) == len(b.levels) and all(np.array_equal(x[2], y[2]) for x, y in zip(a.levels, b.levels))
        check(same, f'{w} x {h} {kind}: the Xbox copy decodes to the PC levels exactly ({len(a.levels)} levels)')


def test_checks() -> None:
    img = picture()
    pc = texwrite.texture(img)
    xb = to_xbox(pc)
    row = texqa.analyse('objects/x/x_d.res', pc, xb)
    check(not errors_of(row) and row['identical_source'] and row['psnr_source'] == float('inf'),
          f'equal to its source: no error, identical ({errors_of(row)})')
    row = texqa.analyse('objects/x/x_d.res', texwrite.texture(img[::-1].copy()), xb)
    check('orientation' in errors_of(row) and any('flipped vertically' in f['detail'] for f in row['flags']),
          'flipped vertically: an orientation error')
    row = texqa.analyse('objects/x/x_d.res', texwrite.texture(img[:, ::-1].copy()), xb)
    check('orientation' in errors_of(row) and any('mirrored' in f['detail'] for f in row['flags']), 'mirrored')
    row = texqa.analyse('objects/x/x_d.res', texwrite.texture(img[..., ::-1].copy()), xb)
    check('swizzle' in errors_of(row), 'red and blue swapped: a swizzle error')
    black = texwrite.texture(np.zeros_like(img))
    row = texqa.analyse('objects/x/x_d.res', black, xb)
    f = [x for x in row['flags'] if x['code'] == 'degenerate: all black']
    check(bool(f) and f[0]['cause'] == 'converter', 'all black (the source is not): degenerate, the converter')
    row = texqa.analyse('objects/x/x_d.res', black, to_xbox(black))
    f = [x for x in row['flags'] if x['code'] == 'degenerate: all black']
    check(bool(f) and f[0]['cause'] == 'source', 'all black like its source: the source')
    # a level that is not its parent's mip
    lv = texture.levels(pc)
    noise = texwrite.dxt1(np.random.default_rng(3).integers(0, 255, (32, 32, 3)).astype(np.uint8))
    broken = pc[:texqa.HEADER] + lv[0][4] + noise + b''.join(x[4] for x in lv[2:])
    row = texqa.analyse('objects/x/x_d.res', broken, xb)
    check('mips: level not its parent' in errors_of(row), 'level 1 replaced by noise: the mip chain error')
    # aspect
    row = texqa.analyse('objects/x/x_d.res', texwrite.texture(picture(64, 32)), xb)
    check('aspect' in errors_of(row), 'another aspect than the source')
    # normal map with the sRGB bit
    nrm = bytearray(texwrite.texture(np.full((16, 16, 3), [128, 128, 255], np.uint8), 'normal'))
    nrm[12:16] = struct.pack('<I', 1)
    row = texqa.analyse('objects/x/x_nm.res', bytes(nrm))
    check('srgb: normal map with the sRGB bit' in errors_of(row), 'a normal map with the sRGB bit')
    # a DXT1 normal map with z in red
    bad = texwrite.texture(np.full((16, 16, 3), [255, 128, 128], np.uint8), 'normal')
    row = texqa.analyse('objects/x/x_nm.res', bad)
    check('normal: z not in blue' in errors_of(row), 'a DXT1 normal map with z in red')
    # BC5: swapped channels against DICE's copy
    y, x = np.mgrid[0:64, 0:64]
    two = np.stack([128 + (x - 32) * 2, 128 + (y - 32) // 2], -1).clip(0, 255)
    ours = bc5(two)
    row = texqa.analyse('objects/x/x_nm.res', ours, dice_res=bc5(two[..., ::-1].copy()))
    check('swizzle vs DICE' in errors_of(row), "BC5 red / green swapped against DICE's copy")
    row = texqa.analyse('levels/wake/emittertexturenormals.res', ours)
    check('normal' not in row, 'a merged emitter atlas (a composite): no normal-map statistics')
    row = texqa.analyse('objects/x/x_nm.res', ours, dice_res=ours)
    check(row['dice'].get('identical') and not errors_of(row), "BC5 equal to DICE's copy")
    # DICE's copy twice the size with flags bit 1
    big = bytearray(texwrite.texture(picture(128, 128)))
    big[12:16] = struct.pack('<I', 3)
    row = texqa.analyse('objects/x/x_d.res', pc, xb, bytes(big))
    known = [x for x in row['flags'] if x['code'] == 'dice: known difference']
    check(bool(known) and 'flags bit 1' in known[0]['detail'] and known[0]['cause'] == 'dice',
          "DICE's copy twice the size with flags bit 1: DICE's known difference")
    # a grime repaint: .orig equals the source
    dirty = texwrite.texture(np.clip(img.astype(int) - np.random.default_rng(5).integers(0, 60, img.shape), 0, 255).astype(np.uint8))
    row = texqa.analyse('objects/x/x_d.res', dirty, xb, orig_res=pc)
    check(any(f['code'] == 'repaint (grime.py)' and f['cause'] == 'intentional' for f in row['flags'])
          and not errors_of(row), 'a grime repaint (.orig = the source): intentional, no error')


def test_inputs_cache_baseline() -> None:
    img = picture()
    pc = texwrite.texture(img)
    with tempfile.TemporaryDirectory() as tmp:
        build = os.path.join(tmp, 'build')
        os.makedirs(os.path.join(build, 'objects', 'x'))
        open(os.path.join(build, 'objects', 'x', 'x_d.res'), 'wb').write(pc)
        open(os.path.join(build, 'objects', 'x', 'x_d.res.orig'), 'wb').write(pc)
        open(os.path.join(build, 'objects', 'x', 'mesh.res'), 'wb').write(b'not a texture' * 20)
        stage = os.path.join(tmp, 'stage', 'awards')
        os.makedirs(os.path.join(stage, 'img'))
        open(os.path.join(stage, 'img', 'a.res'), 'wb').write(texwrite.wrapped(pc))
        open(os.path.join(stage, 'img', 'b.res'), 'wb').write(pc)
        open(os.path.join(stage, 'fbrb.manifest'), 'w').write(
            '# fbrb.manifest\nimg/a.res|ITexture|00010000|1|1\nimg/b.res|MeshData|00010000|1|1\n')
        items = texqa.find([build, os.path.join(tmp, 'stage')])
        names = sorted(i.name for i in items)
        check(names == ['img/a.res', 'objects/x/x_d.res'], f'inputs: the loose texture and the manifest\'s ITexture ({names})')
        check(any(i.orig for i in items), '.orig picked up')
        src_dir = os.path.join(tmp, 'xbox', 'objects', 'x')
        os.makedirs(src_dir)
        open(os.path.join(src_dir, 'x_d.res'), 'wb').write(to_xbox(pc))
        src = texqa.LooseSource(os.path.join(tmp, 'xbox'))
        cache = os.path.join(tmp, 'cache')
        logs: list[str] = []
        rows = texqa.run([build], sources=[src], cache=cache, log=logs.append)
        check(rows[0].get('identical_source') is True and rows[0]['source_name'] == 'objects/x/x_d.res',
              'the source found by name (the build folder dropped), identical')
        logs.clear()
        texqa.run([build], sources=[src], cache=cache, log=logs.append)
        check(any('0 analysed' in x for x in logs), f'the second run analyses nothing ({logs})')
        rows_w = texqa.run([os.path.join(tmp, 'stage')], cache=None, log=logs.append)
        check(len(rows_w) == 1 and not errors_of(rows_w[0]), 'a wrapped on-demand texture reads')
        base = json.loads(json.dumps(texqa._num(texqa.baseline_of(rows))))
        check(not texqa.regressions(rows, base), 'the same rows: no regression')
        open(os.path.join(build, 'objects', 'x', 'x_d.res'), 'wb').write(texwrite.texture(img[::-1].copy()))
        os.remove(os.path.join(build, 'objects', 'x', 'x_d.res.orig'))
        rows2 = texqa.run([build], sources=[src], cache=cache, log=logs.append)
        reg = texqa.regressions(rows2, base)
        check(any('orientation' in r for r in reg) and any('psnr_source' in r for r in reg),
              f'a flipped rebuild: new error flag and PSNR drop are regressions ({reg})')
        out = texqa.text_report(rows2, reg)
        check('orientation' in out and 'regressions against the baseline: ' in out, 'the text report lists them')
        sh = texqa.sheets(rows2, os.path.join(tmp, 'sheets'), [src], None, 4)
        check(len(sh) == 1 and os.path.getsize(sh[0]) > 1000, 'a contact sheet of the worst')


def main() -> int:
    test_decode()
    test_xbox()
    test_checks()
    test_inputs_cache_baseline()
    print('all passed' if not FAILED else f'{len(FAILED)} check(s) failed')
    return 1 if FAILED else 0


if __name__ == '__main__':
    sys.exit(main())
