r"""Round OR14's parity check of texwrite's principal axes: bc2_convert::eigh3 (LAPACK dsyevd through OpenBLAS's
kernels, written out in Rust) against numpy.linalg.eigh, bit for bit, and the encoded blocks with either axes.

    python rust\parity_texwrite_axes.py [--random N] [--pictures N] [--seed S] [--dir DIR ...] [--time]

- pictures: N seeded random pictures of the shapes texwrite gets (noise, flat, gradients, two colours, small
  variations, 1-bit and 8-bit alpha with transparent texels, odd sizes; DXT1 colour / normal maps with every mip
  level, DXT1 with 1-bit alpha, DXT5), plus every PNG / JPG / BMP under each --dir (pictures from disk, read with
  Pillow when it is installed): each block's axis (TexWriteJob.axes against numpy.linalg.eigh of
  TexWriteJob.covariances) and each level's bytes (TexWriteJob.encode against TexWriteJob.finish with NumPy's axes);
- random: N random covariances of texwrite's kind (sums of products of 16 centred texels: integers 0-255, filled-in
  means of transparent texels, small spreads, two colours, lines) through texwrite_principal_axes;
- degenerate: zero, rank 1 and 2, repeated eigenvalues, diagonal, off-diagonal only, negative, scaled by 2^-1070
  to 2^1000 (dsyevd's and dsteqr's scalings), subnormal and huge entries.

Prints one line per group and exits 1 on any difference (the axes compared as bit patterns, NaN included). The
machine NumPy runs on decides its kernels (OpenBLAS picks them per processor): the recorded match is for NumPy 2.5.4
with scipy-openblas 0.3.34 on a SkylakeX-class processor (the WW2 oracle's machine); elsewhere a NumPy difference
is NumPy's, not texwrite's. --time also prints the axes' speed (Rust against NumPy). Needs the built module.
"""
from __future__ import annotations

import argparse
import glob
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), 'bfbc2-model-tools'))
import texwrite  # noqa: E402


def native():
    m = texwrite._native()
    if m is None or not hasattr(m, 'texwrite_principal_axes'):
        sys.exit('the built module with texwrite_principal_axes is needed: python rust\\build_native.py')
    return m


def np_axes(cov: np.ndarray) -> np.ndarray:
    return np.ascontiguousarray(np.linalg.eigh(cov.reshape(-1, 3, 3))[1][:, :, -1])


def rust_axes(nat, cov: np.ndarray) -> np.ndarray:
    got = nat.texwrite_principal_axes(np.ascontiguousarray(cov, np.float64).tobytes())
    return np.frombuffer(got, np.float64).reshape(-1, 3)


def differ(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    """Rows whose bit patterns differ."""
    return (a.view(np.uint64) != b.view(np.uint64)).any(1)


# ------------------------------------------------------------------------------------------------- pictures

def random_picture(rng: np.random.Generator) -> tuple[np.ndarray, str]:
    sides = [1, 2, 4, 8, 16, 32, 64, 128]
    h, w = int(rng.choice(sides)), int(rng.choice(sides))
    kind = int(rng.integers(0, 7))
    yy, xx = np.mgrid[0:h, 0:w]
    if kind == 0:                                   # noise
        rgb = rng.integers(0, 256, (h, w, 3))
    elif kind == 1:                                 # flat
        rgb = np.broadcast_to(rng.integers(0, 256, 3), (h, w, 3))
    elif kind == 2:                                 # gradients
        a, b = rng.integers(0, 256, 3), rng.integers(0, 256, 3)
        t = ((xx * rng.random() + yy * rng.random()) / max(h + w, 1))[..., None]
        rgb = np.round(a * (1 - t) + b * t)
    elif kind == 3:                                 # two colours
        c = rng.integers(0, 256, (2, 3))
        rgb = c[rng.integers(0, 2, (h, w))]
    elif kind == 4:                                 # small variations
        rgb = rng.integers(0, 256, 3) + rng.integers(-3, 4, (h, w, 3))
    elif kind == 5:                                 # stripes of a few colours
        c = rng.integers(0, 256, (4, 3))
        rgb = c[(xx // max(1, int(rng.integers(1, 5)))) % 4]
    else:                                           # smooth noise
        rgb = np.round(128 + 60 * np.sin(xx[..., None] * rng.random(3) + yy[..., None] * rng.random(3)))
    rgb = np.clip(rgb, 0, 255).astype(np.uint8)
    use = int(rng.integers(0, 4))
    if use == 0:
        return rgb, 'color'
    if use == 1:
        return rgb, 'normal'
    alpha = rng.integers(0, 256, (h, w))
    pick = rng.random((h, w))
    alpha = np.where(pick < 0.3, 0, np.where(pick < 0.6, 255, alpha)).astype(np.uint8)
    return np.dstack([rgb, alpha]), ('color' if use == 2 else 'dxt5')


def disk_pictures(dirs: list[str], limit_px: int = 1 << 22):
    try:
        from PIL import Image
    except ImportError:
        return
    for d in dirs:
        for pat in ('*.png', '*.jpg', '*.jpeg', '*.bmp'):
            for p in sorted(glob.glob(os.path.join(d, '**', pat), recursive=True)):
                try:
                    im = Image.open(p)
                    im.load()
                except Exception:
                    continue
                if im.width * im.height > limit_px:
                    continue
                a = np.asarray(im.convert('RGBA' if 'A' in im.mode else 'RGB'))
                yield a, p


def check_picture(nat, img: np.ndarray, kind: str, tally: dict) -> None:
    """One picture as texture() encodes it (every level): the axes and the bytes."""
    h, w = img.shape[:2]
    channels = 4 if (kind == 'dxt5' or img.shape[2] == 4) else 3
    normal = kind == 'normal'
    if img.shape[2] == 4 and kind == 'normal':
        img = img[..., :3]
        channels = 3
    lv = nat.texwrite_levels(np.ascontiguousarray(img).tobytes(), h, w, img.shape[2], normal, channels)
    if lv is None:                                  # sides that do not halve: the first level only
        lv = [(h, w, np.ascontiguousarray(img[..., :channels]).tobytes())]
    job = nat.TexWriteJob.prepare([(lh, lw, channels, px) for lh, lw, px in lv], kind == 'dxt5')
    cov = np.frombuffer(job.covariances(), np.float64).reshape(-1, 9)
    want = np_axes(cov)
    got = np.frombuffer(job.axes(), np.float64).reshape(-1, 3)
    bad = differ(got, want)
    tally['blocks'] += len(cov)
    tally['axes differ'] += int(bad.sum())
    a = job.encode()
    b = job.finish(want.tobytes())
    tally['levels'] += len(a)
    for x, y in zip(a, b):
        n = 16 if kind == 'dxt5' else 8
        xa = np.frombuffer(x, np.uint8).reshape(-1, n)
        ya = np.frombuffer(y, np.uint8).reshape(-1, n)
        tally['encoded blocks differ'] += int((xa != ya).any(1).sum())


# ------------------------------------------------------------------------------------------------- covariances

def random_blocks(rng: np.random.Generator, n: int) -> np.ndarray:
    """(n, 16, 3) float blocks of texwrite's kinds."""
    kind = rng.integers(0, 6, n)
    out = rng.integers(0, 256, (n, 16, 3)).astype(float)                      # 0: noise
    base = rng.integers(0, 256, (n, 1, 3))
    small = np.clip(base + rng.integers(-4, 5, (n, 16, 3)), 0, 255)           # 1: small spreads
    out[kind == 1] = small[kind == 1]
    two = rng.integers(0, 256, (n, 2, 3))
    pick = rng.integers(0, 2, (n, 16))
    twoc = np.take_along_axis(two, pick[..., None].repeat(3, 2), 1)             # 2: two colours
    out[kind == 2] = twoc[kind == 2]
    t = rng.random((n, 16, 1))                                                  # 3: points on a line
    line = np.round(two[:, :1] * (1 - t) + two[:, 1:] * t)
    out[kind == 3] = line[kind == 3]
    one = rng.integers(0, 2, (n, 16, 3)) * rng.integers(0, 256, (n, 1, 3))      # 4: channels on / off
    out[kind == 4] = one[kind == 4]
    # 5: transparent texels filled with the opaque texels' mean (texwrite's three-colour blocks)
    clear = rng.random((n, 16)) < rng.random((n, 1))
    opaque = ~clear
    cnt = opaque.sum(1, keepdims=True)
    fill = (out * opaque[..., None]).sum(1) / np.maximum(cnt, 1)
    filled = np.where(clear[..., None], fill[:, None, :], out)
    out[kind == 5] = filled[kind == 5]
    return out


def covariances_of(blocks: np.ndarray) -> np.ndarray:
    d = blocks - blocks.mean(1, keepdims=True)
    return np.einsum('nki,nkj->nij', d, d).reshape(-1, 9)


def degenerate(rng: np.random.Generator) -> np.ndarray:
    m = []
    m.append(np.zeros((3, 3)))
    for d in ([1, 5, 2], [5, 1, 2], [2, 2, 2], [3, 3, 1], [1, 3, 3], [0, 0, 7], [7, 0, 0], [0, 7, 0], [1, 1, 0],
              [-1, -2, -3], [256, 256, 256], [1e-3, 1, 1e3]):
        m.append(np.diag(np.array(d, float)))
    for _ in range(400):
        u = rng.integers(-255, 256, 3).astype(float)
        m.append(np.outer(u, u) * rng.integers(1, 17) / 16)                    # rank 1
        v = rng.integers(-255, 256, 3).astype(float)
        m.append(np.outer(u, u) + np.outer(v, v))                              # rank 2
        q, _ = np.linalg.qr(rng.standard_normal((3, 3)))
        lam = rng.choice([0.0, 1.0, 2.0, 16.0], 3)
        m.append((q * lam) @ q.T)                                              # repeated eigenvalues
        lam = np.array([1.0, 1.0, float(rng.integers(2, 100))])
        a = (q * lam) @ q.T
        m.append((a + a.T) / 2)
        o = np.zeros((3, 3))
        i, j = rng.choice(3, 2, replace=False)
        o[i, j] = o[j, i] = float(rng.integers(-500, 500))
        m.append(o)                                                            # off-diagonal only
        s = rng.integers(-255, 256, (3, 3)).astype(float)
        m.append(-(s @ s.T))                                                   # negative definite
        c = rng.integers(0, 2 ** 20, (3, 3)).astype(float) / 256
        m.append(np.tril(c) + np.tril(c, -1).T)
    out = np.array(m).reshape(-1, 9)
    scaled = []
    for e in (-1070, -1060, -1040, -1000, -970, -969, -600, -500, -485, -400, -300, -200, -100, -60, 60, 100,
              200, 300, 400, 485, 486, 500, 600, 900, 960, 1000):
        scaled.append(out[: 200] * 2.0 ** e)
    tiny = np.array([[5e-324, 0, 0, 0, 5e-324, 0, 0, 0, 1e-320], [1e-310, 1e-310, 0, 1e-310, 2e-310, 3e-310, 0,
                    3e-310, 1e-311], [1e308, 1e307, 1e306, 1e307, 1e308, 1e305, 1e306, 1e305, 1e308],
                     [1e-300, 1e300, 0, 1e300, 1e-300, 0, 0, 0, 1]])
    return np.vstack([out] + scaled + [tiny])


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--random', type=int, default=1_000_000, help='random covariances (default 1,000,000)')
    ap.add_argument('--pictures', type=int, default=300, help='random pictures (default 300)')
    ap.add_argument('--seed', type=int, default=20261127)
    ap.add_argument('--dir', action='append', default=[], help='folders of PNG / JPG / BMP pictures to add')
    ap.add_argument('--time', action='store_true', help="time the axes, Rust's and NumPy's")
    args = ap.parse_args(argv[1:])
    nat = native()
    rng = np.random.default_rng(args.seed)
    failed = False

    tally = dict.fromkeys(('pictures', 'levels', 'blocks', 'axes differ', 'encoded blocks differ'), 0)
    for _ in range(args.pictures):
        img, kind = random_picture(rng)
        check_picture(nat, img, kind, tally)
        tally['pictures'] += 1
    print('random pictures:', ', '.join(f'{k} {v:,}' for k, v in tally.items()))
    failed |= bool(tally['axes differ'] or tally['encoded blocks differ'])
    if args.dir:
        tally = dict.fromkeys(tally, 0)
        for img, _path in disk_pictures(args.dir):
            for kind in (['color', 'dxt5'] if img.shape[2] == 4 else ['color', 'normal']):
                check_picture(nat, img, kind, tally)
            tally['pictures'] += 1
        print('pictures from disk:', ', '.join(f'{k} {v:,}' for k, v in tally.items()))
        failed |= bool(tally['axes differ'] or tally['encoded blocks differ'])

    done = bad = 0
    chunk = 250_000
    while done < args.random:
        n = min(chunk, args.random - done)
        cov = covariances_of(random_blocks(rng, n))
        bad += int(differ(rust_axes(nat, cov), np_axes(cov)).sum())
        done += n
    print(f'random covariances: {done:,}, axes differ {bad:,}')
    failed |= bool(bad)

    cov = degenerate(rng)
    with np.errstate(all='ignore'):
        diff = differ(rust_axes(nat, cov), np_axes(cov))
    print(f'degenerate covariances: {len(cov):,}, axes differ {int(diff.sum()):,}')
    for row in cov[diff][:5]:
        print('  differs:', row.tolist())
    failed |= bool(diff.any())

    if args.time:
        cov = covariances_of(random_blocks(rng, 1_000_000))
        t0 = time.perf_counter()
        rust_axes(nat, cov)
        t1 = time.perf_counter()
        np_axes(cov)
        t2 = time.perf_counter()
        print(f'1,000,000 axes: Rust {t1 - t0:.3f} s (all cores), NumPy {t2 - t1:.3f} s')
    print('FAILED' if failed else 'identical')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
