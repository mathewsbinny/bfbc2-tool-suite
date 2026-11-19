r"""The suite's application icon (round I1) from the author's own M67 grenade render: a square crop around the grenade
on the render's dark grey backdrop, rounded corners, every Windows size.

  python design\icons\make_icon.py design\icons\source\m67_render.png design\icons [--sheet]

Writes <folder>\app.ico (256, 128, 64, 48, 32, 24 and 16 pixels) and <folder>\app_512.png (a preview); --sheet also
writes app_sizes.png (every size on dark and light grounds, for a look; not committed). Needs numpy and Pillow.
"""
import os
import sys

import numpy as np
from PIL import Image, ImageDraw

SIZES = [256, 128, 64, 48, 32, 24, 16]


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith('--')]
    if not args:
        print(__doc__)
        return 2
    src = Image.open(args[0]).convert('RGB')
    out_dir = args[1] if len(args) > 1 else os.path.dirname(os.path.abspath(__file__))
    a = np.asarray(src).astype(int)
    bg = np.median(np.concatenate([a[:20].reshape(-1, 3), a[-20:].reshape(-1, 3)]), axis=0)
    # the grenade: pixels far enough from the backdrop colour
    mask = np.abs(a - bg).sum(-1) > 30
    ys, xs = np.nonzero(mask)
    x0, x1 = np.percentile(xs, 0.2), np.percentile(xs, 99.8)
    y0, y1 = np.percentile(ys, 0.2), np.percentile(ys, 99.8)
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    side = max(x1 - x0, y1 - y0) * 1.10                    # a little margin round the grenade
    box = (int(cx - side / 2), int(cy - side / 2), int(cx + side / 2), int(cy + side / 2))
    canvas = Image.new('RGB', (box[2] - box[0], box[3] - box[1]), tuple(int(v) for v in bg))
    canvas.paste(src, (-box[0], -box[1]))
    s = 1024
    sq = canvas.resize((s, s), Image.Resampling.LANCZOS).convert('RGBA')
    m = Image.new('L', (s, s), 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, s - 1, s - 1], radius=180, fill=255)
    icon = Image.new('RGBA', (s, s), (0, 0, 0, 0))
    icon.paste(sq, (0, 0), m)
    ImageDraw.Draw(icon).rounded_rectangle([3, 3, s - 4, s - 4], radius=180, outline=(20, 20, 20, 255), width=8)
    icon.resize((512, 512), Image.Resampling.LANCZOS).save(os.path.join(out_dir, 'app_512.png'))
    imgs = [icon.resize((n, n), Image.Resampling.LANCZOS) for n in SIZES]
    imgs[0].save(os.path.join(out_dir, 'app.ico'), sizes=[(n, n) for n in SIZES], append_images=imgs[1:])
    if '--sheet' in argv:
        sheet = Image.new('RGBA', (sum(SIZES) + 20 * (len(SIZES) + 1), 2 * 296), (32, 32, 32, 255))
        ImageDraw.Draw(sheet).rectangle([0, 296, sheet.width, 2 * 296], fill=(243, 243, 243, 255))
        for row in range(2):
            x = 20
            for n, im in zip(SIZES, imgs):
                sheet.alpha_composite(im, (x, row * 296 + 20 + 256 - n))
                x += n + 20
        sheet.save(os.path.join(out_dir, 'app_sizes.png'))
    print('crop', box, 'backdrop', bg)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
