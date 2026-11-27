r"""Project OPFOR's WW2 pictures in the Rust core (round OR4, "OPFOR in Rust" stage 4: bc2_convert::pictures and
bc2_convert::meshview::page through bc2_native), on built inputs:

- Pillow's resize (Lanczos, box) of 300 seeded pictures of 1, 3 and 4 channels, against Pillow itself, pixel for pixel
  (RGBA through Pillow's premultiplied conversion);
- the loadout pictures' renderer on built parts: a bar framed to fill the picture, its colour bled past its edge,
  the 2 x 2 filter down, a part with normals against one without, the same pixels on every run; bad triangles refused;
- the loading and map pictures' checks on pictures encoded by texwrite (DICE's header and size; a wrong header, a
  far picture refused) and the map picture's shape;
- the weapon icons' silhouette of a built DXT5 picture and fit (against a Pillow fit), the minimap paint;
- the preview page: the models' JSON, the escaped title, the textures as PNG files of the same pixels.

Project OPFOR's test_ww2_pictures_parity checks the real pictures against the retired Python's answers.

  python test_pictures_native.py
"""
from __future__ import annotations

import base64
import io
import json
import os
import re
import struct
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('model-tools', 'core')
import texwrite  # noqa: E402
from bfbc2core import native  # noqa: E402

N = native.require('the WW2 pictures test', 'pictures_resize')
FAILS: list[str] = []


def check(ok: bool, what: str) -> None:
    print(f'{"ok  " if ok else "FAIL"} {what}')
    if not ok:
        FAILS.append(what)


def resize_vs_pillow() -> None:
    rnd = np.random.default_rng(1943)
    bad = 0
    for t in range(300):
        ch = (1, 3, 4)[t % 3]
        f = ('lanczos', 'box')[(t // 3) % 2]
        h, w = (int(v) for v in rnd.integers(1, 50, 2))
        hh, ww = (int(v) for v in rnd.integers(1, 70, 2))
        a = rnd.integers(0, 256, (h, w) if ch == 1 else (h, w, ch), dtype=np.uint8)
        if ch == 4 and t % 4 == 0:
            a[..., 3] = rnd.choice([0, 128, 255], (h, w))
        ref = np.asarray(Image.fromarray(a).resize((ww, hh), Image.Resampling.LANCZOS if f == 'lanczos'
                                                   else Image.Resampling.BOX))
        got = np.frombuffer(N.pictures_resize(a.tobytes(), w, h, ch, ww, hh, f), np.uint8).reshape(ref.shape)
        bad += not np.array_equal(ref, got)
    check(bad == 0, f'resize: 300 seeded pictures as Pillow resizes them ({bad} differ)')


def part(pos, tris, normals=None, uvs=None, image=None):
    img = None
    if image is not None:
        im = np.ascontiguousarray(image[..., :3], np.float64)
        img = (im.shape[0], im.shape[1], 3, im.tobytes())
    return (np.ascontiguousarray(pos, np.float64).tobytes(),
            None if normals is None else np.ascontiguousarray(normals, np.float64).tobytes(),
            None if uvs is None else np.ascontiguousarray(uvs, np.float64).tobytes(), img,
            np.ascontiguousarray(tris, np.int64).tobytes())


def renderer() -> None:
    bar = np.array([[0, 0, 0], [0, 1, 0], [0, 1, 4], [0, 0, 4]], np.float64)
    tris = np.array([[0, 1, 2], [0, 2, 3]])
    tex = np.full((4, 4, 3), 200, np.uint8)
    p = part(bar, tris, uvs=np.array([[0, 0], [0, 1], [1, 1], [1, 0]], np.float64), image=tex)
    a = np.frombuffer(N.pictures_weapon_framed([p], 128, 32), np.uint8).reshape(32, 128, 4)
    b = np.frombuffer(N.pictures_weapon_framed([p], 128, 32), np.uint8).reshape(32, 128, 4)
    check(np.array_equal(a, b), 'render: the same pixels on every run')
    check(a[16, 64, 3] == 255 and a[0, 0, 3] == 0 and a[16, 1, 3] == 0,
          'render: the bar opaque in the middle, the margins clear')
    # the colour bled into the clear texels next to the bar (DXT blocks mix them)
    left = int(np.nonzero(a[16, :, 3])[0].min())
    check(a[16, left - 1, 3] == 0 and a[16, left - 1, :3].any(), 'render: the colour bled past the edge')
    d = np.frombuffer(N.pictures_down(a.tobytes(), 128, 32, 64, 16), np.uint8).reshape(16, 64, 4)
    al = (a[..., 3].astype(float) / 255).reshape(16, 2, 64, 2).mean((1, 3))
    check(np.array_equal(d[..., 3], (al * 255).astype(np.uint8)), 'down: the alpha the 2 x 2 mean')
    lit = np.frombuffer(N.pictures_weapon_framed([part(bar, tris, normals=np.tile([-1.0, 0, 0], (4, 1)))], 128, 32),
                        np.uint8).reshape(32, 128, 4)
    check(lit[16, 64, :3].tolist() != a[16, 64, :3].tolist(), 'render: untextured parts drawn grey and lit')
    try:
        N.pictures_weapon_framed([part(bar, np.array([[0, 1, 9]]))], 64, 16)
        check(False, 'render: a triangle past the vertices refused')
    except IndexError:
        check(True, 'render: a triangle past the vertices refused')


def loading() -> None:
    rgb = (np.arange(720)[:, None, None] // 4 + np.arange(1280)[None, :, None] // 8).astype(np.uint8).repeat(3, 2)
    res = texwrite.resource(0, 1, 1280, 720, [texwrite.dxt1(rgb)])
    wrapped, psnr = N.pictures_loading_check(res, rgb.tobytes())
    check(len(wrapped) == 460956 and psnr > 30, f'loading check: DICE\'s size, {psnr:.1f} dB')
    bad = bytearray(res)
    bad[8:12] = struct.pack('<I', 2)
    try:
        N.pictures_loading_check(bytes(bad), rgb.tobytes())
        check(False, 'loading check: another format refused')
    except SystemExit as e:
        check(str(e).startswith('loading picture header (3, 0, 2, 1, 1280, 720, 1)'), 'loading check: another format refused')
    rgba = np.frombuffer(N.pictures_level_picture(rgb.tobytes()), np.uint8).reshape(256, 512, 4)
    check(rgba[:196, :295, 3].min() == 255 and rgba[196:, :, 3].max() == 0 and (rgba[:, 295:, :3] == 255).all(),
          'map picture: opaque in the top-left 295 x 196, white at alpha 0 elsewhere')
    lres = texwrite.resource(2, 1, 512, 256, [texwrite.dxt5(rgba)])
    wrapped, psnr = N.pictures_level_check(lres, rgba.tobytes())
    check(len(wrapped) == 131228, f'map check: DICE\'s size, {psnr:.1f} dB')


def icons() -> None:
    img = np.zeros((256, 1024, 4), np.uint8)
    img[100:140, 100:900, :3] = 120
    img[100:140, 100:900, 3] = 255
    img[110:120, 900:1000, :] = 255
    res = texwrite.texture(img, 'dxt5', mips=False)
    h, w, px = N.pictures_silhouette(res, 104, None)
    sil = np.frombuffer(px, np.uint8).reshape(h, w, 4)
    check(w <= 104 and h <= 32 and sil[h // 2, w // 2, 3] == 255 and sil[0, :, 3].max() <= 230,
          f'silhouette: {w} x {h}, solid inside its outline')
    for bw, bh in ((40, 20), (120, 40)):
        got = np.frombuffer(N.pictures_fit(sil.tobytes(), w, h, bw, bh), np.uint8).reshape(bh, bw, 4)
        ph, pw = h, w
        pic = sil
        if pw > bw or ph > bh:
            s = min(bw / pw, bh / ph)
            pw, ph = max(int(pw * s), 1), max(int(ph * s), 1)
            pic = np.asarray(Image.fromarray(sil, 'RGBA').resize((pw, ph), Image.Resampling.LANCZOS))
        want = np.zeros((bh, bw, 4), np.uint8)
        y, x = (bh - ph) // 2, (bw - pw) // 2
        want[y:y + ph, x:x + pw] = pic
        check(np.array_equal(got, want), f'fit: into {bw} x {bh} as Pillow scales it, centred')
    box = np.zeros((4, 4, 4), np.uint8)
    box[..., 3] = 255
    box[:2, :, :3] = (10, 20, 30)
    box[2:, :, :3] = (200, 150, 100)
    box[3, 3] = (230, 230, 230, 255)
    fill, outline = N.pictures_box_colours(box.tobytes(), 4, 4)
    check((list(fill), list(outline)) == ([200, 150, 100], [10, 20, 30]), 'box colours: the darkest and the commonest')
    plane = np.zeros((4, 4, 4), np.uint8)
    plane[1:3, 1:3] = (255, 255, 255, 255)
    plane[0, 0] = (0, 0, 0, 255)
    out = np.frombuffer(N.pictures_paint(box.tobytes(), plane.tobytes(), 4, 4), np.uint8).reshape(4, 4, 4)
    check(out[1, 1].tolist() == [200, 150, 100, 255] and out[0, 0].tolist() == [10, 20, 30, 255] and
          out[3, 3].tolist() == [10, 20, 30, 0], 'paint: white to the fill, dark to the outline, clear to the outline')


def page() -> None:
    img = np.arange(4 * 3 * 3, dtype=np.uint8).reshape(4, 3, 3)
    pos = np.zeros((3, 3), '<f4').tobytes()
    parts = [('main', 1, 'objects/x/textures/x_c', pos, np.zeros((3, 2), '<f4').tobytes(),
              np.array([0, 1, 2], '<u4').tobytes(), (3, 4, 3, img.tobytes())),
             ('bare', 1, None, pos, None, np.array([0, 1, 2], '<u4').tobytes(), None)]
    html = N.meshview_page([('objects/x/x_mesh', [0.0, 1.5, -2.0], parts)], 'A <test> & "it"')
    check('<title>A &lt;test&gt; &amp; &quot;it&quot;</title>' in html, 'page: the title escaped')
    m = re.search(r'const MODELS = (\[.*?\]);\n', html, re.S)
    models = json.loads(m.group(1)) if m else []
    ok = bool(models) and models[0]['name'] == 'x_mesh' and models[0]['offset'] == [0.0, 1.5, -2.0] and \
        [p['tris'] for p in models[0]['parts']] == [1, 1] and models[0]['parts'][1]['tex'] is None
    check(ok, 'page: the models as JSON (the last name part, the offset, each part)')
    png = Image.open(io.BytesIO(base64.b64decode(models[0]['parts'][0]['tex']))) if ok else None
    check(png is not None and np.array_equal(np.asarray(png), img), 'page: the texture a PNG file of its pixels')


def main() -> int:
    resize_vs_pillow()
    renderer()
    loading()
    icons()
    page()
    print(f'{len(FAILS)} failed' if FAILS else 'all ok')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
