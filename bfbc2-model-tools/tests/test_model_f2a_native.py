r"""Round F2a: the model tools in the Rust core against their Python reference - texwrite.py (texture: the levels and
the DXT1 / DXT1 with 1-bit alpha / DXT5 blocks, dxt1, dxt5, alpha1, alpha5, l8_down; the principal axes stay
numpy.linalg.eigh's) and skinmesh.py's SkinnedLod (the parts cut out, decode, the winding, replace / add_to /
add_part's encoding, to_bytes, bounds).

Compared both ways (the module's use_native on and off, or the *_reference functions): the bytes, the arrays (dtype,
shape, every value), the parts' state, or the same error.

  python test_model_f2a_native.py          quick: pictures built here (smooth, noise, flat, two- and three-colour,
                                           grey, every kind, with and without mips, 1 x 1 to 256 x 256, odd sizes for
                                           dxt1 / dxt5), the decoders on their output and damaged levels; skinned LODs
                                           built here in DICE's layouts, edited with lathes; the M1 Garand's and the
                                           Type 5's LODs when the game copy is there
  python test_model_f2a_native.py --game   + every DXT1 / DXT5 texture of the game copy decoded and encoded again in
                                           its kind (timed), every skinned LOD of the game copy read, decoded, written
                                           and edited

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import os
import random
import struct
import sys
import time
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('model-tools', 'console-convert', 'core', 'mesh-viewer')
import mesh  # noqa: E402
import skinmesh  # noqa: E402
import texture  # noqa: E402
import texwrite  # noqa: E402
from bfbc2core import native, paths  # noqa: E402
from f0_parity import Checks, outcome, stdout_utf8  # noqa: E402
from f2a_parity import Tally, bundle_entries, damaged, watch, watched  # noqa: E402
from shapes import Geo, lathe  # noqa: E402

N = native.require('texwrite.py / skinmesh.py (round F2a)', 'TexWriteJob')


def arrays(v: Any) -> Any:
    if isinstance(v, np.ndarray):
        return ('ndarray', v.dtype.str, v.shape, v.tobytes())
    if isinstance(v, np.generic):
        return ('scalar', v.dtype.str, v.tobytes())
    if isinstance(v, dict):
        return {k: arrays(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return type(v)(arrays(x) for x in v)
    return v


def both_ways(t: Tally, label: str, module: Any, fn: Any, *args: Any) -> Any:
    """fn(*args) with the module's Rust core on and off: the same result or the same error."""
    def run(on: bool) -> tuple[Any, ...]:
        module.use_native(on)
        try:
            return outcome(lambda: arrays(fn(*args)))
        finally:
            module.use_native(True)
    a, native = watched(lambda: run(True))
    r = run(False)
    t.add(a == r, f'{label}: {str(a)[:100]} / {str(r)[:100]}', r[0] == 'error', native)
    return a


# ------------------------------------------------------------------------------------------------ texwrite

def picture(rng: np.random.Generator, h: int, w: int, ch: int, style: str) -> np.ndarray:
    if style == 'noise':
        return rng.integers(0, 256, (h, w, ch)).astype(np.uint8)
    if style == 'flat':
        return np.broadcast_to(rng.integers(0, 256, ch), (h, w, ch)).astype(np.uint8).copy()
    if style == 'grey':
        g = rng.integers(0, 256, (h, w, 1))
        return np.repeat(g, ch, 2).astype(np.uint8)
    if style in ('two', 'three'):
        cols = rng.integers(0, 256, (2 if style == 'two' else 3, ch))
        if style == 'three':
            cols[:, :3] = [[255, 0, 0], [0, 255, 0], [0, 0, 255]][:len(cols)]
        return cols[rng.integers(0, len(cols), (h, w))].astype(np.uint8)
    yy, xx = np.mgrid[0:h, 0:w]
    base = [(xx * 255 // max(w - 1, 1)), (yy * 255 // max(h - 1, 1)), ((xx + yy) * 127 // max(w + h - 2, 1)), (xx * yy) % 256]
    return np.dstack(base[:ch]).astype(np.uint8)


def texwrite_quick(c: Checks, rng: np.random.Generator) -> None:
    t, td = Tally(), Tally()
    sizes = [(1, 1), (2, 1), (1, 4), (4, 4), (8, 2), (16, 16), (32, 64), (128, 128), (256, 128)]
    for i in range(240):
        h, w = sizes[i % len(sizes)]
        style = ['noise', 'flat', 'grey', 'two', 'three', 'smooth'][i % 6]
        for kind, ch in (('color', 3), ('normal', 3), ('dxt5', 4), ('color', 4)):
            img = picture(rng, h, w, ch, style)
            if kind == 'color' and ch == 4:
                img[..., 3] = np.where(rng.random((h, w)) < 0.3, 0, 255) if i % 3 else img[..., 3]
            for mips in (True, False):
                res = both_ways(t, f'{kind} {style} {h}x{w} {i}', texwrite, texwrite.texture, img, kind, mips)
            if res[0] == 'ok':
                pc = texwrite.texture(img, kind)
                lv0 = pc[texwrite.HEADER:texwrite.HEADER + struct.unpack_from('<I', pc, 32)[0]]
                five = kind == 'dxt5'
                fn = texwrite.alpha5 if five else texwrite.alpha1
                both_ways(t, f'alpha {kind} {i}', texwrite, fn, lv0, w, h)
                both_ways(td, f'alpha damaged {i}', texwrite, fn, damaged(lv0, random.Random(i)), w, h)
        for ch in (3, 4, 5):
            hh, ww = int(rng.integers(1, 23)), int(rng.integers(1, 23))
            img = picture(rng, hh, ww, ch, style)
            both_ways(t, f'dxt1 {hh}x{ww}x{ch} {i}', texwrite, texwrite.dxt1, img)
            if ch == 4:
                both_ways(t, f'dxt5 {hh}x{ww} {i}', texwrite, texwrite.dxt5, img)
        g = rng.integers(0, 256, (int(rng.integers(1, 9)) * 2, int(rng.integers(1, 9)) * 2)).astype(np.uint8)
        both_ways(t, f'l8_down {i}', texwrite, texwrite.l8_down, g)
        both_ways(td, f'l8_down odd {i}', texwrite, texwrite.l8_down, g[:-1])
    for bad in (np.zeros((48, 64, 3), np.uint8), np.zeros((8, 8, 3), np.float32), np.zeros((8, 8), np.uint8)):
        both_ways(td, f'refused {bad.shape} {bad.dtype}', texwrite, texwrite.texture, bad)
    c.check(not t.ndiff, f'texwrite on built pictures both ways: {t.text()}', '; '.join(t.differ))
    c.check(not td.ndiff, f'texwrite on damaged levels and refused pictures: {td.text()}', '; '.join(td.differ))


def texwrite_game(c: Checks, opfor: str) -> None:
    t = Tally()
    tn = tr = 0.0
    n = 0
    for nm, _, res in bundle_entries(os.path.join(opfor, 'game', '**', '*.fbrb'), {'ITexture'}):
        if len(res) < texwrite.HEADER:
            continue
        typ, fmt, _flags, w, h = struct.unpack_from('<5I', res, 4)
        if typ != 0 or fmt not in (0, 18, 2) or not w or not h or w & (w - 1) or h & (h - 1):
            continue
        size0 = struct.unpack_from('<I', res, 32)[0]
        try:
            px = texture.decode(res[texwrite.HEADER:texwrite.HEADER + size0], w, h, fmt)[:, :, :3].astype(np.uint8)
            if fmt == 2:
                px = np.dstack([px, texwrite.alpha5(res[texwrite.HEADER:texwrite.HEADER + size0], w, h)]).astype(np.uint8)
        except Exception:                                        # noqa: BLE001
            continue
        kind = {0: 'color', 18: 'normal', 2: 'dxt5'}[fmt]
        n += 1
        times = []
        for on in (True, False):
            texwrite.use_native(on)
            t0 = time.perf_counter()
            got = outcome(texwrite.texture, px, kind)
            times.append((time.perf_counter() - t0, got))
        texwrite.use_native(True)
        tn += times[0][0]
        tr += times[1][0]
        t.add(times[0][1] == times[1][1], nm, times[1][1][0] == 'error')
    c.check(not t.ndiff, f'{n:,} game textures decoded and encoded again both ways: {t.text()}', '; '.join(t.differ))
    print(f'timing (texwrite.texture of the game textures): Python {tr:.2f} s, Rust {tn:.2f} s')


# ------------------------------------------------------------------------------------------------ skinmesh

def layout(els: list[tuple[int, int, int, int]], stride: int) -> bytes:
    rec = bytearray(0x49)
    for i, e in enumerate(els):
        rec[4 * i:4 * i + 4] = bytes(e)
    rec[64], rec[69] = len(els), stride
    return bytes(rec)


SKIN = layout([(3, 1, 0, 0), (0x0C, 3, 12, 0), (0x0C, 2, 16, 0), (8, 4, 20, 0), (8, 5, 28, 0), (8, 6, 36, 0),
               (6, 0, 44, 0)], 48)
SKIN3P = layout([(8, 1, 0, 0), (5, 7, 8, 0), (0x0C, 3, 10, 0), (0x0C, 2, 14, 0), (8, 4, 18, 0), (6, 0, 26, 0)], 30)
DEPTH = layout([(3, 1, 0, 0), (0x0C, 3, 12, 0), (0x0C, 2, 16, 0)], 20)


def skinned(rng: random.Random) -> tuple[bytes, dict[str, bytes]]:
    """A PC MeshData of a few parts (a depth-only part drawing the first part's vertices now and then)."""
    lay = rng.choice([SKIN, SKIN3P])
    parts: list[dict[str, Any]] = []
    idx: list[int] = []
    lays: dict[str, bytes] = {}
    verts = b''
    for k in range(rng.randrange(1, 4)):
        nv, tri = rng.randrange(3, 12), rng.randrange(1, 6)
        first = len(idx)
        idx += [rng.randrange(nv) for _ in range(3 * tri)]
        stride = lay[69]
        rows = bytearray()
        for _ in range(nv):
            r = bytearray(rng.randbytes(stride))
            if lay is SKIN:
                r[0:12] = struct.pack('<3f', *(rng.uniform(-1, 1) for _ in range(3)))
            else:
                r[0:8] = np.array([rng.uniform(-1, 1) for _ in range(3)] + [1.0], '<f2').tobytes()
            for at in ([20, 28, 36] if lay is SKIN else [18]):
                r[at:at + 8] = np.array([rng.uniform(-1, 1) for _ in range(3)] + [1.0], '<f2').tobytes()
            r[(16 if lay is SKIN else 14):][:4] = bytes([rng.randrange(2)] * 4)
            rows += r
        parts.append({'name': b'part%d' % k, 'triangles': tri, 'vertices': nv, 'first_index': first,
                      'vertex_offset': len(verts), 'stride': stride, 'k': 3, 'fa': 1, 'fb': 2, 'bones': [rng.randrange(40), 5]})
        lays[f'part{k}'] = lay
        verts += bytes(rows)
    if rng.random() < 0.5:
        p = dict(parts[0], name=b'zonly', stride=lay[69])
        parts.append(p)
        lays['zonly'] = lay
    m = {'h0': 7, 'h1': b'\1\0', 'parts': parts, 'lists': [[0], [], [], []]}
    return mesh.write_reference(m, verts, struct.pack('<%dH' % len(idx), *idx)), lays


def lod_state(s: Any) -> Any:
    return arrays({'m': s.m, 'winding': s.winding, 'parts': [(p.info, p.layout, p.rows, p.idx, p.changed) for p in s.parts]})


def geometry(rng: random.Random, bones: list[int]) -> Geo:
    g = lathe([(0.0, 0.1), (0.2, 0.15), (0.4, 0.05)], (rng.uniform(-1, 1), 0.0), (0, 0, 1, 1), segments=rng.choice([6, 8]))
    g.bone = np.array([rng.choice(bones) for _ in range(len(g.pos))], int)
    return g


def lod_both(t: Tally, label: str, b: bytes, lays: dict[str, bytes], rng: random.Random) -> None:
    """A SkinnedLod read, decoded, edited and written both ways."""
    def run(on: bool, seed: int) -> Any:
        skinmesh.use_native(on)
        r = random.Random(seed)
        try:
            out: list[Any] = []
            s = skinmesh.SkinnedLod(b, lays, label)
            out.append(lod_state(s))
            for p in s.parts:
                out.append([arrays(s.decode(p, u)) for u in range(8)])
            out.append(outcome(lambda: arrays(s.bounds())))
            out.append(s.to_bytes())
            names = [p.name for p in s.parts]
            bones = [int(x) for x in s.parts[0].info['bones']] + [r.randrange(60)]
            out.append(outcome(lambda: lod_state(s.add_to(names[-1], geometry(r, bones)) and s)))
            out.append(outcome(lambda: lod_state(s.replace(names[0], geometry(r, bones)) and s)))
            out.append(outcome(lambda: lod_state(s.add_part('new', s.parts[0].layout, geometry(r, bones), names[0], 3, 0)
                                                 and s)))
            out.append(outcome(s.to_bytes))
            out.append(outcome(lambda: arrays(s.bounds())))
            return ('ok', out)
        except Exception as e:                                   # noqa: BLE001
            return ('error', type(e).__name__, str(e))
        finally:
            skinmesh.use_native(True)
    seed = rng.randrange(1 << 30)
    a, native = watched(lambda: run(True, seed))
    r = run(False, seed)
    t.add(a == r, f'{label}: {str(a)[:100]} / {str(r)[:100]}', r[0] == 'error', native)


def skin_quick(c: Checks, rng: random.Random) -> None:
    t, td = Tally(), Tally()
    for i in range(300):
        b, lays = skinned(rng)
        lod_both(t, f'built LOD {i}', b, lays, rng)
        lod_both(td, f'damaged LOD {i}', damaged(b, rng), lays, rng)
    c.check(not t.ndiff, f'built skinned LODs read, decoded, edited and written both ways: {t.text()}', '; '.join(t.differ))
    c.check(not td.ndiff, f'damaged skinned LODs: {td.text()}', '; '.join(td.differ))


def skin_game(c: Checks, opfor: str, only: tuple[str, ...] | None) -> None:
    import meshview
    import shaderdb
    try:
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                       # noqa: BLE001
        print(f'skip the game copy ({e})')
        return
    t = Tally()
    rng = random.Random(0x5C1)
    n = 0
    tn = tr = 0.0
    sets = [nm for nm, _, _ in bundle_entries(os.path.join(opfor, 'game', '**', '*.fbrb'), {'SkinnedMeshSet'})] \
        if only is None else list(only)
    for name in sorted(set(sets)):
        stem = name[:-4] if name.endswith('.res') else name
        if only is not None:
            stem = stem.lower()
        for k in range(8):
            b = ix.read(f'{stem}_lod{k}_data.res')
            if b is None:
                break
            try:
                m = mesh.parse_reference(b, False)
            except Exception:                                    # noqa: BLE001
                continue
            lays = {p['name'].decode('latin-1'): ix.parts.get(f'{stem}_lod{k}/{p["name"].decode("latin-1")}/standard'.lower())
                    for p in m['parts']}
            if any(v is None for v in lays.values()):
                continue
            n += 1
            for on in (True, False):
                skinmesh.use_native(on)
                t0 = time.perf_counter()
                try:
                    s = skinmesh.SkinnedLod(b, lays)
                    s.to_bytes()
                    s.bounds()
                except Exception:                                # noqa: BLE001
                    pass
                if on:
                    tn += time.perf_counter() - t0
                else:
                    tr += time.perf_counter() - t0
            skinmesh.use_native(True)
            lod_both(t, f'{stem} LOD {k}', b, lays, rng)
    c.check(n > 0 and not t.ndiff, f'{n:,} skinned LODs of the game copy read, decoded, edited and written both ways: '
            f'{t.text()}', '; '.join(t.differ))
    print(f'timing (SkinnedLod read, to_bytes, bounds): Python {tr:.2f} s, Rust {tn:.2f} s')


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    watch(texwrite, 'dxt1_reference', 'dxt5_reference', 'alpha1_reference', 'alpha5_reference', 'l8_down_reference',
          'levels')
    watch(skinmesh.SkinnedLod, '_winding_reference', 'decode_reference', 'to_bytes_reference', 'bounds_reference')
    c.check(texwrite.use_native(True) and skinmesh.use_native(True), 'the Rust core runs texwrite.py and skinmesh.py')
    texwrite_quick(c, np.random.default_rng(0xF2A))
    skin_quick(c, random.Random(0xF2A))
    opfor = paths.sibling(paths.OPFOR_DIR, suite=SUITE)
    if os.path.isdir(os.path.join(opfor, 'game')):
        skin_game(c, opfor, None if game else ('Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh',
                                                'Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh3p',
                                                'Objects/Weapons/Handheld/Beach/Jap_Type5/Jap_Type5_Mesh'))
        if game:
            texwrite_game(c, opfor)
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
