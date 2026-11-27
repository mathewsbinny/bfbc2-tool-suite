r"""Round OR5's parity check of the menu art's picture operations: bc2_menu::pil / modeart (through the example
pil_ops) against Pillow and numpy as Project OPFOR's retired src\tools\menus\onslaught_art.py used them.

    python rust\parity_menuart.py --reference <onslaught_art.py> [--cases N] [--seed S]

--reference is the retired script (git show <OPFOR commit before OR5>:src/tools/menus/onslaught_art.py > file): its
dxt5_encode, xbox_untile_dxt, dds_decode, cutout and fit are the reference, with Pillow's resize, paste and
alpha_composite. Random pictures of random sizes (and alpha with many 0 / 255 pixels, as cut-outs have) go through
each operation; every output must be byte for byte the same. Prints one line per operation and exits 1 on a
difference. Needs numpy and Pillow; builds the example with the checkout's build folder.
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import types

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))


def load_reference(path: str) -> types.ModuleType:
    """The retired script's functions without its OPFOR imports (opforcore is stubbed)."""
    src = open(path, encoding='utf-8').read()
    for line in ('from opforcore.guid import guid as opfor_guid', 'from opforcore.paths import SUITE',
                 'from opforcore.util import dbx_run'):
        src = src.replace(line, 'pass')
    mod = types.ModuleType('onslaught_art_reference')
    mod.__file__ = path
    exec(compile(src, path, 'exec'), mod.__dict__)
    return mod


def build() -> str:
    target = subprocess.run([sys.executable, os.path.join(HERE, 'build_native.py'), '--target-dir'],
                            capture_output=True, text=True, check=True).stdout.strip()
    env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS=os.environ.get('CARGO_BUILD_JOBS', '4'))
    subprocess.run(['cargo', 'build', '--release', '-p', 'bc2-menu', '--example', 'pil_ops', '--target-dir', target],
                   cwd=HERE, check=True, env=env)
    return os.path.join(target, 'release', 'examples', 'pil_ops.exe')


def picture(rng: np.random.Generator, w: int, h: int) -> np.ndarray:
    a = rng.integers(0, 256, (h, w, 4), dtype=np.uint8)
    pick = rng.random((h, w))
    a[:, :, 3] = np.where(pick < 0.3, 0, np.where(pick < 0.6, 255, a[:, :, 3]))
    return a


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--reference', required=True)
    ap.add_argument('--cases', type=int, default=40)
    ap.add_argument('--seed', type=int, default=20261123)
    args = ap.parse_args(argv[1:])
    ref = load_reference(args.reference)
    exe = build()
    rng = np.random.default_rng(args.seed)
    tmp = tempfile.mkdtemp(prefix='or5_pil_')
    fin, fout, fbase = (os.path.join(tmp, n) for n in ('in.raw', 'out.raw', 'base.raw'))
    counts: dict[str, list[int]] = {}

    def rust(op: str, data: bytes, w: int, h: int, *more: object) -> bytes:
        open(fin, 'wb').write(data)
        subprocess.run([exe, op, fin, str(w), str(h), fout, *map(str, more)], check=True)
        return open(fout, 'rb').read()

    def check(op: str, got: bytes, want: bytes) -> None:
        c = counts.setdefault(op, [0, 0])
        c[0] += 1
        if got != want:
            c[1] += 1

    for _ in range(args.cases):
        w, h = int(rng.integers(1, 120)), int(rng.integers(1, 120))
        a = picture(rng, w, h)
        im = Image.fromarray(a, 'RGBA')
        nw, nh = int(rng.integers(1, 200)), int(rng.integers(1, 200))
        check('resize', rust('resize', a.tobytes(), w, h, nw, nh),
              np.asarray(im.resize((nw, nh), Image.Resampling.LANCZOS)).tobytes())
        check('cutout', rust('cutout', a.tobytes(), w, h), np.asarray(ref.cutout(im)).tobytes())
        check('fit', rust('fit', a.tobytes(), w, h), np.asarray(ref.fit(im)).tobytes())
        bw, bh = int(rng.integers(1, 150)), int(rng.integers(1, 150))
        base = picture(rng, bw, bh)
        x, y = int(rng.integers(-w, bw)), int(rng.integers(-h, bh))
        out = Image.fromarray(base.copy(), 'RGBA')
        layer = Image.new('RGBA', out.size, (0, 0, 0, 0))
        layer.paste(im, (x, y), im)
        out.alpha_composite(layer)
        open(fbase, 'wb').write(base.tobytes())
        check('layer', rust('layer', a.tobytes(), w, h, fbase, x, y, bw, bh), np.asarray(out).tobytes())
        check('flip', rust('flip', a.tobytes(), w, h),
              np.asarray(im.transpose(Image.Transpose.FLIP_LEFT_RIGHT)).tobytes())
        cx0, cy0 = int(rng.integers(-5, w)), int(rng.integers(-5, h))
        cx1, cy1 = cx0 + int(rng.integers(0, w + 5)), cy0 + int(rng.integers(0, h + 5))
        check('crop', rust('crop', a.tobytes(), w, h, cx0, cy0, cx1, cy1),
              np.asarray(im.crop((cx0, cy0, cx1, cy1))).tobytes())
        # DXT5: random blocks decoded; random pictures (multiples of 4) encoded; Xbox 360 untiling
        bw4, bh4 = 4 * int(rng.integers(1, 20)), 4 * int(rng.integers(1, 20))
        blocks = rng.integers(0, 256, bw4 * bh4, dtype=np.uint8).tobytes()
        check('decode', rust('decode', blocks, bw4, bh4), np.asarray(ref.dds_decode(blocks, bw4, bh4)).tobytes())
        p4 = picture(rng, bw4, bh4)
        check('encode', rust('encode', p4.tobytes(), bw4, bh4), ref.dxt5_encode(p4).tobytes())
        tw, th = 4 * 32 * int(rng.integers(1, 3)), 4 * 32 * int(rng.integers(1, 3))
        tiled = rng.integers(0, 256, tw * th, dtype=np.uint8).tobytes()
        check('untile', rust('untile', tiled, tw, th), ref.xbox_untile_dxt(tiled, tw, th))
    bad = 0
    for op, (n, d) in sorted(counts.items()):
        print(f'{op:7} {n - d} of {n} identical')
        bad += d
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
