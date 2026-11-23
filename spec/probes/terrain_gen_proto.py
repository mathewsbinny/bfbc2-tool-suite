"""Round TG0's throw-away terrain generator prototype (spec/transforms/terrain_gen.md): proves that a generated
terrain can be written as BFBC2's terrain resources and read back, before round TG1 writes the real generator in Rust.

    python spec/probes/terrain_gen_proto.py [--seed S] [--n 1024|2048] [--biome arid|jungle|snow|temperate|vietnam]
                                            [--out <folder>] [--donor <level>] [--opfor <folder>]

What it does (TGEN-GEN-*, TGEN-OUT-*):

1. a height field from a seed: spectral synthesis with the radial power spectrum's slope of the biome's DICE median
   (P(k) ~ k^-beta, TGEN-SHAPE-3), a border rise, thermal erosion to the biome's slope, a water level put where the
   biome's DICE median share of ground lies under it, and flat pads for objectives (TGEN-PLAY-3);
2. the main block written whole (TERR-HF-1 - TERR-HF-11: header, heights, a depth map of 255, the min / max tree, a
   lower envelope built under the heights), a material map painted from slope and height (TERR-MM-1 - TERR-MM-4), a
   one-body water mesh (WATER-MESH-1), and with `--donor` a one-block visual terrain whose cells all draw the donor's
   rule-only patch shader with the most layers (TGEN-VIS-1);
3. every file read back: the height field by the suite's Rust reader (bfbc2core.terrain), its tail re-encoded and its
   envelope under every sample (spec/probes/terrain.py's DICE-checked readers), the material map decoded and
   re-encoded byte for byte, the water mesh by the Rust reader, the visual terrain by the probe's reader; the same seed
   gives the same bytes, another seed others; the generated shape measured with terrain_shape.py's statistics against
   the biome's DICE range.

Nothing is installed or written into a game folder; `--out` (default: a temporary folder) receives the files.
"""
from __future__ import annotations

import argparse
import hashlib
import math
import struct
import sys
import tempfile
from pathlib import Path
from typing import Any

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / 'core'))
sys.path.insert(0, str(HERE))
import terrain as tp  # noqa: E402
import terrain_shape as ts  # noqa: E402
from bfbc2core import terrain, water  # noqa: E402

# TGEN-SHAPE-7: per biome, DICE's medians the prototype aims at (spec/probes/terrain_shape.py, 2026-11-23):
# spectrum slope beta, median slope (degrees), share of ground under water, height range (m), border minus centre (m)
TARGETS = {
    'arid': (4.02, 8.0, 0.12, 500.0, 35.0),
    'jungle': (4.07, 22.2, 0.20, 750.0, 15.0),
    'snow': (4.38, 30.5, 0.42, 1000.0, 10.0),
    'temperate': (3.71, 26.0, 0.03, 500.0, 37.0),
    'vietnam': (3.94, 14.2, 0.18, 500.0, 7.0),
}
MATERIALS = {   # TGEN-MAT-1: names of DICE's grids per biome; index 0 defaultmaterial (TERR-MM-4)
    'arid': ('defaultmaterial', 'sand', 'stone', 'seabed', 'gravel'),
    'jungle': ('defaultmaterial', 'grass', 'stone', 'seabed', 'dirt'),
    'snow': ('defaultmaterial', 'snow', 'stone', 'seabed', 'gravel'),
    'temperate': ('defaultmaterial', 'forest', 'rock', 'seabed', 'gravel'),
    'vietnam': ('defaultmaterial', 'grass', 'stone', 'water_solid', 'dirt'),
}


# ----------------------------------------------------------------------------------------------------------- generate

def spectral(n: int, beta: float, rng: np.random.Generator) -> Any:
    """Gaussian noise filtered to P(k) ~ k^-beta (the radially averaged spectrum DICE's terrains have), unit range."""
    k = np.fft.fftfreq(n)
    kr = np.hypot(k[None, :], k[:, None])
    kr[0, 0] = 1.0
    amp = kr ** (-beta / 2)
    amp[0, 0] = 0.0
    f = np.fft.fft2(rng.standard_normal((n, n))) * amp
    h = np.fft.ifft2(f).real
    return (h - h.min()) / (h.max() - h.min())


def thermal(h: Any, talus: float, rounds: int, rate: float = 0.25) -> Any:
    """Thermal erosion: where a sample stands more than `talus` metres above a 4-neighbour, a share of the excess
    moves down (edges clamped)."""
    h = h.copy()
    for _ in range(rounds):
        p = np.pad(h, 1, mode='edge')
        moved = np.zeros_like(h)
        for dz, dx in ((0, 1), (2, 1), (1, 0), (1, 2)):
            nb = p[dz:dz + h.shape[0], dx:dx + h.shape[1]]
            d = h - nb
            m = np.where(d > talus, rate * (d - talus) / 2, 0.0)
            moved -= m
            pn = np.pad(m, 1)
            moved += pn[2 - dz:2 - dz + h.shape[0], 2 - dx:2 - dx + h.shape[1]]
        h += moved
    return h


def generate(n: int, seed: int, biome: str, pads: list[tuple[int, int, int]] | None = None) -> tuple[Any, float, float]:
    """(heights in metres (n x n, rows along z), height range, water level) for a seed and a biome."""
    beta, slope50, wet, rng_m, border = TARGETS[biome]
    rng = np.random.default_rng(seed)
    h = spectral(n, beta, rng)
    z, x = np.mgrid[0:n, 0:n] / (n - 1) - 0.5
    bowl = np.clip(np.hypot(x, z) * 2, 0, 1) ** 3            # 0 in the middle, 1 at the border's middle
    relief = 0.45 * rng_m
    h = h * relief + bowl * border * 1.5
    # scale the relief until the median slope is the biome's, then erode the steepest away
    for _ in range(6):
        gz, gx = np.gradient(h)
        s50 = float(np.median(np.degrees(np.arctan(np.hypot(gx, gz)))))
        if s50 <= 0:
            break
        h = h * (math.tan(math.radians(slope50)) / math.tan(math.radians(s50)))
    h = thermal(h, math.tan(math.radians(42.0)), 20)
    for i, j, r in pads or []:                               # flat pads for objectives (TGEN-PLAY-3)
        zz, xx = np.ogrid[0:n, 0:n]
        d = np.hypot(xx - i, zz - j)
        core = d <= r
        level = float(np.median(h[core]))
        blend = np.clip((d - r) / r, 0, 1)
        h = np.where(d <= 2 * r, level + (h - level) * blend, h)
    h -= h.min()
    water_y = float(np.quantile(h, wet)) if wet > 0 else -1.0
    top = float(h.max())
    rng_y = max(rng_m, math.ceil(top / 50) * 50)
    return h.astype(np.float32), float(rng_y), water_y


# ------------------------------------------------------------------------------------------------------------ write

def envelope(v: Any, n: int) -> Any:
    """TERR-HF-11 grids, coarsest first; each vertex the lowest sample of the cells around it, so every grid's
    triangles (split as TERR-HF-3) lie at or under every sample."""
    out = []
    for side in tp.pyramid_sides(n):
        step = n // (side - 1)
        g = np.empty((side, side), np.uint16)
        for a in range(side):
            for b in range(side):
                z0, z1 = max(0, (a - 1) * step), min(n, (a + 1) * step + 1)
                x0, x1 = max(0, (b - 1) * step), min(n, (b + 1) * step + 1)
                g[a, b] = v[z0:z1, x0:x1].min()
        out.append(g.ravel())
    return np.concatenate(out).astype('<u2')


def depth_runs(depth: Any) -> tuple[bytes, bytes]:
    """TERR-HF-9: the depth map's row offsets and maximal (count, value) runs, row by row."""
    n = depth.shape[0]
    offsets, runs, at = [], bytearray(), 0
    for row in depth:
        offsets.append(at)
        cut = np.flatnonzero(np.diff(row.astype(np.int16))) + 1
        starts = np.r_[0, cut]
        counts = np.diff(np.r_[starts, n])
        for s, c in zip(starts.tolist(), counts.tolist(), strict=True):
            runs += struct.pack('<HH', c, int(row[s]))
        at += 2 * len(starts)
    return np.array(offsets, '<u4').tobytes(), bytes(runs)


def encode_block(v: Any, bounds: tuple[float, ...], depth: Any = None, words: tuple[int, int, int, int] | None = None,
                 grids: Any = None) -> bytes:
    """A main block (TERR-HF-1 - TERR-HF-11) from its u16 heights: the header (words 0x24, 0x26, 0x28, 0x2C as given,
    else DICE's usual 0, n / 256, 0, 0), the depth map (all 255 unless given), the min / max tree, and the lower
    envelope (built under the heights unless given)."""
    n = v.shape[0]
    offsets, runs = depth_runs(np.full((n, n), 255, np.uint8) if depth is None else depth)
    pad = (-(len(offsets) + len(runs))) % 16
    w24, w26, w28, w2c = words or (0, n // 256, 0, 0)
    head = struct.pack('<6f', *bounds) + struct.pack('<III', n, int(math.log2(n)) - 3, int(math.log2(n)) - 2)
    head += struct.pack('<HHIBI', w24, w26, w28, w2c, len(offsets) + len(runs) + pad)
    env = envelope(v, n) if grids is None else grids
    return head + v.astype('<u2').tobytes() + offsets + runs + bytes(pad) + tp.minmax_tree(v, n).tobytes() + env.tobytes()


def write_heightfield(h: Any, rng_y: float, n: int, cell: float = 1.0) -> bytes:
    """A new main block: n samples of `cell` metres centred on the origin as DICE's 30 (TGEN-REQ-2), min y 0, max y
    rng_y, depth 255 everywhere (TEDIT-HF-3)."""
    v = np.clip(np.round(h.astype(np.float64) / rng_y * 65535), 0, 65535).astype('<u2')
    side = n * cell
    return encode_block(v, (-side / 2, 0.0, -side / 2, side / 2, rng_y, side / 2))


def rewrite_dice(d: bytes) -> tuple[bytes, bytes]:
    """TGEN-OUT-4, the writer canary: a DICE main block written again from its own heights, depth map and header
    words, once with its own envelope and once with one built under the heights."""
    m = tp.main_tail(d)
    bounds = struct.unpack_from('<6f', d)
    words = struct.unpack_from('<HHIB', d, 0x24)
    own = np.concatenate([g.ravel() for g in m['grids']]).astype('<u2')
    return (encode_block(m['heights'], bounds, m['depth'], words, own),
            encode_block(m['heights'], bounds, m['depth'], words))


def write_material_map(cells: Any, names: tuple[str, ...]) -> bytes:
    """TERR-MM-1 - TERR-MM-3: width x width cells of 4-bit indices in blocks of 512 x 256, rows coded as DICE codes
    them."""
    w = cells.shape[0]
    per_row = w // 512
    nb = per_row * (w // 256)
    out = bytearray(struct.pack('<IH', w, nb))
    for k in range(nb):
        by, bx = divmod(k, per_row)
        rows = [tp.encode_material_row(cells[by * 256 + r, bx * 512:(bx + 1) * 512]) for r in range(256)]
        block = b''.join(rows)
        out += struct.pack('<I', len(block)) + block + struct.pack('<256H', *(len(r) for r in rows))
    blob = b''.join(nm.encode('latin-1') + b'\0' for nm in names)
    return bytes(out) + struct.pack('<I', len(blob)) + blob


def paint(h: Any, water_y: float, cell: float, width: int) -> Any:
    """TGEN-MAT-2: material index per material-map cell from height and slope: under water 3 (sea bed), slope over
    35 degrees 2 (stone / rock), 25 - 35 degrees 4 (gravel / dirt), else 1 (the biome's ground)."""
    n = h.shape[0]
    gz, gx = np.gradient(h.astype(np.float64), cell)
    s = np.degrees(np.arctan(np.hypot(gx, gz)))
    idx = np.ones((n, n), np.uint8)
    idx[s > 25] = 4
    idx[s > 35] = 2
    idx[h < water_y] = 3
    rep = width // n
    return np.repeat(np.repeat(idx, rep, axis=0), rep, axis=1) if rep > 1 else idx[::n // width, ::n // width]


def write_water(water_y: float, side: float) -> bytes:
    """WATER-MESH-1: one body, a quad over the whole (centred) terrain at the water level."""
    a = side / 2
    v = np.array([[-a, water_y, -a], [a, water_y, -a], [a, water_y, a], [-a, water_y, a]], '<f4')
    idx = np.array([0, 2, 1, 0, 3, 2], '<u4')
    return struct.pack('<II', 1, 4) + v.tobytes() + struct.pack('<I', 6) + idx.tobytes()


def shader_span(d: bytes) -> tuple[int, int]:
    """TERR-VT-1: where a visual terrain's patch shaders start and end (after the 2N road blocks)."""
    w = tp.Walk(d)
    n = w.u32()
    for _ in range(2 * n):
        for _ in range(w.u32()):
            w.string(), w.string(), w.string()
            w.o += 8 + 24 + 4
        _v, vb, i = w.u32(), w.u32(), w.u32()
        w.o += vb + 4 * i
    start = w.o
    for _ in range(w.u32()):
        w.string(), w.string()
        w.o += 8
        for _ in range(w.u32()):
            for _ in range(w.u32()):
                w.string(), w.string(), w.string()
                w.o += 48 + 2
    return start, w.o


def write_visual(donor: bytes, name: str, side: float, rng_y: float) -> tuple[bytes, int]:
    """TGEN-VIS-1: one block (TEDIT-BLK-1), empty road blocks, the donor's patch shaders as they are, one patch map of
    W = side / 16 cells all drawn by the donor's rule-only shader with the most layers (no tiles, no flags); the
    atlas named as the donor names it, its layout kept in the header (TERR-VT-5)."""
    vt = tp.visual_terrain(donor)
    rule = [(s[2], i) for i, s in enumerate(vt['shaders']) if s[3] == 0]
    pick = max(rule)[1]
    header = vt['maps'][0][0]
    atlas = vt['maps'][0][6]
    start, end = shader_span(donor)
    w = int(side // 16)
    out = bytearray(struct.pack('<I', 1))
    out += struct.pack('<4I', 0, 0, 0, 0) * 2                # full and low-detail road blocks, empty
    out += donor[start:end]
    out += struct.pack('<I', 1)
    out += struct.pack('<6f', -side / 2, 0.0, -side / 2, side / 2, rng_y, side / 2)
    out += struct.pack('<5I', 66, 1, header[2], header[3], w)
    out += bytes(8 * w * w) + bytes(4 * w * w)
    out += struct.pack('<I', w) + bytes([pick]) * (w * w)
    out += f'Terrains/{name}/{name}.Heightfield-000'.encode() + b'\0' + atlas.encode('latin-1') + b'\0'
    return bytes(out), pick


# ------------------------------------------------------------------------------------------------------------ check

def build(n: int, seed: int, biome: str) -> dict[str, Any]:
    pads = [(n // 2, n // 2, 24), (n // 3, n // 2, 24), (2 * n // 3, n // 2, 24)]
    h, rng_y, water_y = generate(n, seed, biome, pads)
    hf = write_heightfield(h, rng_y, n)
    width = 2 * n                                           # 0.5 m cells (TERR-MM-3)
    cells = paint(h, water_y, 1.0, width)
    mm = write_material_map(cells, MATERIALS[biome])
    wm = write_water(water_y, float(n))
    return {'h': h, 'range': rng_y, 'water_y': water_y, 'hf': hf, 'cells': cells, 'mm': mm, 'water': wm,
            'pads': pads}


def checks_for(n: int, seed: int, biome: str, donor: bytes | None = None) -> list[tuple[bool, str]]:
    out: list[tuple[bool, str]] = []
    b = build(n, seed, biome)
    hf = terrain.parse(b['hf'])                             # the suite's Rust reader
    q = b['range'] / 65535
    out.append((hf.size == n and float(np.abs(hf.heights - b['h']).max()) <= q,
                f'TGEN-OUT-1: the Rust reader reads the {n} block back within one step ({q:.4f} m)'))
    (kind, *_), words = terrain.tail(b['hf'])
    out.append((kind == 'sections' and words[1] == int(math.log2(n)) - 2,
                f'TGEN-OUT-1: the Rust reader takes the tail as a main block\'s ({kind}, {words})'))
    m = tp.main_tail(b['hf'])
    out.append((bool(m['re_encoded']) and bool(m['maximal']) and bool((m['depth'] == 255).all()),
                'TGEN-OUT-1: the tail re-encodes (depth runs, padding, min / max tree) as the DICE-checked reader'))
    worst = min(tp.below(g, m['heights']) for g in m['grids'])
    out.append((worst == 1.0, f'TGEN-OUT-1: every envelope grid under every sample ({worst:.4f})'))
    mm = tp.material_map(b['mm'])
    out.append((bool(mm['re_encoded']) and bool((mm['cells'] == b['cells']).all())
                and mm['names'] == list(MATERIALS[biome]),
                'TGEN-OUT-2: the material map decodes to the painted cells and re-encodes byte for byte'))
    v, t = water.parse_mesh(b['water'])
    out.append((v.shape == (4, 3) and t.shape == (2, 3), 'TGEN-OUT-3: the water mesh reads (Rust reader)'))
    again = build(n, seed, biome)
    other = build(n, seed + 1, biome)
    out.append((again['hf'] == b['hf'] and again['mm'] == b['mm'] and other['hf'] != b['hf'],
                'TGEN-GEN-1: the same seed gives the same bytes, another seed others'))
    s = ts.shape(b['h'], (-n / 2, 0.0, -n / 2, n / 2, b['range'], n / 2), n, b['water_y'])
    beta, slope50, wet, _r, _b = TARGETS[biome]
    out.append((abs(s['beta'] - beta) < 0.6 and abs(s['slope'][1] - slope50) < 4 and abs(s['wet'] - wet) < 0.05,
                f"TGEN-GEN-2: shape near the biome's DICE medians (beta {s['beta']} / {beta}, slope p50 "
                f"{s['slope'][1]} / {slope50}, wet {s['wet']} / {wet}; p99 slope {s['slope'][3]}, "
                f"relief 66 m {s['rough64']})"))
    pad_ok = all(float(np.ptp(b['h'][j - r // 2:j + r // 2, i - r // 2:i + r // 2])) < 0.5 for i, j, r in b['pads'])
    out.append((pad_ok, 'TGEN-PLAY-3: objective pads flat (under 0.5 m over their middle)'))
    if donor is not None:
        vis, pick = write_visual(donor, 'TG_PROTO', float(n), b['range'])
        vt = tp.visual_terrain(vis)
        mp = vt['maps'][0]
        out.append((vt['n'] == 1 and len(vt['shaders']) == len(tp.visual_terrain(donor)['shaders'])
                    and set(mp[4]) == {pick} and not any(mp[2]),
                    f'TGEN-VIS-1: a one-block visual terrain with the donor\'s rule-only shader {pick} reads back'))
    return out


def canary(game: Path) -> list[tuple[bool, str]]:
    """TGEN-OUT-4 on DICE's 30 main blocks (spec/probes/terrain.py's corpus, each once)."""
    from bfbc2core import fbrb
    seen: set[str] = set()
    same = upto = under = total = 0
    for p in tp.stock_bundles(game):
        for e in fbrb.entries(p):
            low = e.name.lower()
            if e.deleted or e.type != 'TerrainHeightfield' or not low.endswith('-000.res') or low in seen:
                continue
            seen.add(low)
            d = fbrb.read(p, [e.name])[e.name]
            own, built = rewrite_dice(d)
            n = struct.unpack_from('<I', d, 0x18)[0]
            env = 2 * sum(k * k for k in tp.pyramid_sides(n))
            total += 1
            same += own == d
            upto += built[:-env] == d[:-env] and len(built) == len(d)
            m = tp.main_tail(built)
            under += min(tp.below(g, m['heights']) for g in m['grids']) == 1.0
    return [(total == 30 and same == 30 and upto == 30 and under == 30,
             f"TGEN-OUT-4: DICE's main blocks written again: {same} of {total} byte for byte with their own envelope, "
             f'{upto} up to the envelope with a built one, {under} built envelopes under every sample')]


def checks(game: Path | None = None) -> list[tuple[bool, str]]:
    donor = None
    if game is not None:
        p = game / 'Dist' / 'win32' / 'levels' / 'mp_005' / 'terrain-00.fbrb'
        if p.is_file():
            from bfbc2core import fbrb
            names = {e.name.lower(): e.name for e in fbrb.entries(p)}
            n = names.get('terrains/mp_005/mp_005.visual.res')
            donor = fbrb.read(p, [n])[n] if n else None
    return checks_for(1024, 7, 'arid', donor) + (canary(game) if game is not None else [])


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=7)
    ap.add_argument('--n', type=int, default=1024, choices=(1024, 2048))
    ap.add_argument('--biome', default='arid', choices=sorted(TARGETS))
    ap.add_argument('--out', help='folder for the written files (default: a temporary folder)')
    ap.add_argument('--donor', help="a DICE level whose visual terrain lends its patch shaders (e.g. mp_005)")
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (for --donor)')
    a = ap.parse_args()
    donor = None
    if a.donor:
        from bfbc2core import fbrb, paths
        game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
        p = game / 'Dist' / 'win32' / 'levels' / a.donor / 'terrain-00.fbrb'
        names = {e.name.lower(): e.name for e in fbrb.entries(p)}
        vis = [v for k, v in names.items() if k.endswith('.visual.res')]
        donor = fbrb.read(p, vis[:1])[vis[0]]
    results = checks_for(a.n, a.seed, a.biome, donor)
    for ok, what in results:
        print(f"{'ok  ' if ok else 'FAIL'} {what}")
    b = build(a.n, a.seed, a.biome)
    out = Path(a.out) if a.out else Path(tempfile.mkdtemp(prefix='tg0_'))
    out.mkdir(parents=True, exist_ok=True)
    files = {'TG_PROTO.Heightfield-000.res': b['hf'], 'TG_PROTO.MaterialMap.res': b['mm'],
             'TG_PROTO.Water.Mesh.res': b['water']}
    if donor is not None:
        files['TG_PROTO.Visual.res'] = write_visual(donor, 'TG_PROTO', float(a.n), b['range'])[0]
    for name, data in files.items():
        (out / name).write_bytes(data)
        print(f'{name}: {len(data):,} bytes, sha1 {hashlib.sha1(data).hexdigest()[:12]}')
    print(f'written to {out} (range {b["range"]:g} m, water at {b["water_y"]:.2f} m)')
    sys.exit(0 if all(ok for ok, _ in results) else 1)


if __name__ == '__main__':
    main()
