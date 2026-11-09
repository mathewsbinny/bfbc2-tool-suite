r"""texwrite.py (unit tier; the game tier part skipped without the game):

- layout: a 512 x 512 colour texture has DICE's header and level sizes (Accesories_D's: 10 levels, 131072 ... 8, 8, 8
  bytes; 174868 bytes in all), a normal map format 18 flags 0;
- round trip: the console converter's decoder reads the blocks back - a smooth picture within 10 of 255 on average
  per channel, a block of one colour exactly (to 5:6:5), a two-colour block's texels each one of its colours;
- normal maps: every mip level's texels decode to unit-length normals within 0.15 (DXT1 rounds each to 5:6:5);
- sides that are not powers of two are refused;
- DXT5 (award images): one level, header (3, 0, 2, 1, w, h, 0, 1), colour within 10 on average, an alpha ramp within
  10 everywhere (alpha5 decodes it); the on-demand wrapper's layout;
- game: two of DICE's DXT1 weapon textures, decoded and encoded again, read back within 4 of 255 on average; DICE's
  M1 Garand award image (DXT5, wrapped) encoded again: wrapper and header byte-identical, within 4 on average.

  python test_texwrite.py
"""
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402
suite_paths.use('model-tools', 'console-convert', 'mesh-viewer')
import texture  # noqa: E402
import texwrite  # noqa: E402

DICE_512 = (131072, 32768, 8192, 2048, 512, 128, 32, 8, 8, 8)


def decode(res, level=0):
    _, _, fmt, _, w, h, _, mips = struct.unpack_from('<8I', res, 0)
    sizes = struct.unpack_from('<15I', res, 32)
    o = texwrite.HEADER + sum(sizes[:level])
    lw, lh = max(w >> level, 1), max(h >> level, 1)
    return texture.decode(res[o:o + sizes[level]], lw, lh, 0)[:lh, :lw, :3]


def main():
    fails = []
    yy, xx = np.mgrid[0:512, 0:512]
    smooth = np.stack([xx / 2, yy / 2, (xx + yy) / 4], -1).astype(np.uint8)
    res = texwrite.texture(smooth)
    head = struct.unpack_from('<8I', res, 0)
    sizes = struct.unpack_from('<15I', res, 32)
    if head != (3, 0, 0, 1, 512, 512, 0, 10) or sizes[:10] != DICE_512 or any(sizes[10:]) or len(res) != 174868:
        fails.append(f'layout: header {head}, sizes {sizes}, {len(res)} bytes')
    err = np.abs(decode(res).astype(float) - smooth).mean(axis=(0, 1))
    print(f'smooth picture: mean error per channel {np.round(err, 2)}')
    if err.max() > 10:
        fails.append(f'smooth picture: mean error {err}')
    flat = np.zeros((8, 8, 3), np.uint8)
    flat[:] = (200, 100, 40)
    _, back = texwrite._565(flat.reshape(-1, 3).astype(float))
    if not np.array_equal(decode(texwrite.texture(flat))[:, :], back.reshape(8, 8, 3).astype(int)):
        fails.append('one-colour block not exact')
    two = np.zeros((4, 4, 3), np.uint8)
    two[::2] = (248, 252, 248)
    got = decode(texwrite.texture(two)).reshape(-1, 3)
    cols = {tuple(c) for c in got}
    if len(cols) != 2:
        fails.append(f'two-colour block decodes to {cols}')
    # normal maps
    n = np.stack([np.sin(xx / 20) * 0.5, np.cos(yy / 15) * 0.5, np.ones_like(xx, float)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    nres = texwrite.texture(((n + 1) * 127.5).astype(np.uint8), 'normal')
    if struct.unpack_from('<4I', nres, 0)[2:] != (18, 0):
        fails.append('normal map: not format 18 flags 0')
    worst = 0.0
    for k in range(10):
        d = decode(nres, k).astype(float) / 127.5 - 1
        worst = max(worst, float(np.abs(np.linalg.norm(d, axis=-1) - 1).max()))
    print(f'normal map: worst length error over all levels {worst:.3f}')
    if worst > 0.15:
        fails.append(f'normal map lengths off by {worst:.3f}')
    try:
        texwrite.texture(np.zeros((48, 64, 3), np.uint8))
        fails.append('48 x 64 not refused')
    except texwrite.TexWriteError:
        pass
    # DXT5 with alpha, one level (the award images' layout), and the on-demand wrapper
    rgba = np.dstack([smooth[:128], (np.clip(xx[:128] - 128, 0, 255)).astype(np.uint8)])       # alpha: a ramp
    res5 = texwrite.texture(rgba, 'dxt5', mips=False)
    head5, sizes5 = struct.unpack_from('<8I', res5, 0), struct.unpack_from('<15I', res5, 32)
    if head5 != (3, 0, 2, 1, 512, 128, 0, 1) or sizes5[0] != 512 * 128 or any(sizes5[1:]) or len(res5) != 92 + 65536:
        fails.append(f'dxt5: header {head5}, sizes {sizes5[:2]}, {len(res5)} bytes')
    col = texture.decode(res5[texwrite.HEADER:], 512, 128, 2)[:, :, :3].astype(float)
    alp = texwrite.alpha5(res5[texwrite.HEADER:], 512, 128).astype(float)
    ec, ea = np.abs(col - smooth[:128]).mean(), np.abs(alp - rgba[..., 3]).max()
    print(f'dxt5: colour mean error {ec:.2f}, alpha worst error {ea:.0f} of 255')
    if ec > 10 or ea > 10:
        fails.append(f'dxt5: colour error {ec:.2f}, alpha error {ea}')
    wr = texwrite.wrapped(res5)
    if wr[:12] != b'RES\x02ITexture' or any(wr[12:56]) or struct.unpack_from('<2I', wr, 56) != (len(res5),) * 2 or \
            wr[64:] != res5:
        fails.append('wrapper layout')
    # game tier
    try:
        import meshview
        import shaderdb
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                   # noqa: BLE001 - no game copy: unit tier only
        print(f'SKIP game tier: {e}')
        ix = None
    if ix:
        for name in ('objects/weapons/handheld/vietnam/nam_shg_870/870mcs_d.res',
                     'objects/weapons/handheld/vietnam/common/accesories_d.res'):
            b = ix.read(name)
            if b is None:
                print(f'SKIP {name}: not in the game copy')
                continue
            w, h = struct.unpack_from('<2I', b, 16)
            src = texture.decode(b[texwrite.HEADER:], w, h, 0)[:, :, :3].astype(np.uint8)
            ours = decode(texwrite.texture(src))
            err = float(np.abs(ours.astype(float) - src).mean())
            print(f'{name}: DICE\'s picture encoded again, mean error {err:.2f} of 255')
            if err > 4:
                fails.append(f'{name}: mean error {err:.2f}')
        # DICE's award image (DXT5, one level, wrapped): decoded and encoded again - the same wrapper and header
        name = 'awards/images/weapons/size2/m1garand.res'
        b = ix.read(name)
        if b is None:
            print(f'SKIP {name}: not in the game copy')
        else:
            res = b[64:]
            w, h = struct.unpack_from('<2I', res, 16)
            src = np.dstack([texture.decode(res[texwrite.HEADER:], w, h, 2)[:, :, :3],
                             texwrite.alpha5(res[texwrite.HEADER:], w, h)]).astype(np.uint8)
            again = texwrite.wrapped(texwrite.texture(src, 'dxt5', mips=False))
            same_head = again[:64 + texwrite.HEADER] == b[:64 + texwrite.HEADER]
            ours = np.dstack([texture.decode(again[64 + texwrite.HEADER:], w, h, 2)[:, :, :3],
                              texwrite.alpha5(again[64 + texwrite.HEADER:], w, h)]).astype(float)
            err = np.abs(ours - src).mean(axis=(0, 1))
            print(f'{name}: wrapper and header {"identical" if same_head else "DIFFERENT"}; encoded again, mean '
                  f'error per channel {np.round(err, 2)}')
            if not same_head or err.max() > 4:
                fails.append(f'{name}: header same {same_head}, error {err}')
    for f in fails:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
