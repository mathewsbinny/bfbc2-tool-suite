r"""Round O4's texqa group: the command lines test_cli_o4.py replays for texqa.py (bfbc2 texqa).

cases(): the quick tier's: PC and Xbox 360 textures encoded here from made-up pictures (every format texqa decodes;
loose and in unpacked-bundle folders with a manifest; wrapped, cube, flipped, swizzled, degenerate, re-encoded, short
chains, damaged ones), sources by --console, caches, baselines and regressions, argument errors. No game data.
opfor_calls(opfor): the --game tier's: texqa over the converted textures under Project OPFOR's work\ (the WW2
vehicles of rust\bench_sweep's texqa_ww2 and the Japanese bazooka beside them), read in place, the outputs and caches
in the case folder. Every run has BFBC2_TEXQA_WORKERS=2.
"""
from __future__ import annotations

import hashlib
import json
import math
import os
import random
import re
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]

ENV = {'BFBC2_TEXQA_WORKERS': '2'}
BASE = ['--no-bf1943', '--no-dice', '-o', '{d}\\out']          # the options every Rust-answered line has
BLOCK = {0: 8, 18: 8, 1: 16, 2: 16, 19: 16, 20: 16, 3: 8}
TEXEL = {10: 1, 5: 2, 9: 4, 13: 8}


# ------------------------------------------------------------------------------------------- pictures

Pic = list[list[tuple[int, int, int, int]]]


def picture(w: int, h: int, seed: int, kind: str = 'smooth') -> Pic:
    rng = random.Random(seed)
    fx, fy, ph = rng.uniform(1, 4), rng.uniform(1, 4), rng.uniform(0, 6)
    out: Pic = []
    for y in range(h):
        row = []
        for x in range(w):
            u, v = x / max(w, 1), y / max(h, 1)
            if kind == 'black':
                p = (rng.randint(0, 3), rng.randint(0, 3), rng.randint(0, 3), 255)
            elif kind == 'white':
                p = (255, 254, 253, 255)
            elif kind == 'const':
                p = (90, 120, 30, 200)
            elif kind == 'noise':
                p = (rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255))
            elif kind == 'normal':
                a = math.sin(6 * u + ph) * 0.4
                b = math.cos(5 * v) * 0.4
                c = math.sqrt(max(0.0, 1 - a * a - b * b))
                p = (int((a + 1) * 127.5), int((b + 1) * 127.5), int((c + 1) * 127.5), 255)
            else:
                r = 127 + 120 * math.sin(fx * 6.28 * u + ph)
                g = 127 + 120 * math.cos(fy * 6.28 * v)
                b = 127 + 100 * math.sin(3 * (u + v)) + rng.uniform(-8, 8)
                a = 255 if kind != 'alpha' else (255 if (x // 4 + y // 4) % 2 else 0)
                p = (int(r), int(g), int(max(0, min(255, b))), a)
            row.append(p)
        out.append(row)
    return out


def half_pic(p: Pic) -> Pic:
    h, w = len(p), len(p[0])
    nh, nw = max(h // 2, 1), max(w // 2, 1)
    out: Pic = []
    for y in range(nh):
        row: list[tuple[int, int, int, int]] = []
        for x in range(nw):
            ys = [min(2 * y, h - 1), min(2 * y + 1, h - 1)]
            xs = [min(2 * x, w - 1), min(2 * x + 1, w - 1)]
            m = [(sum(p[a][b][c] for a in ys for b in xs) + 2) // 4 for c in range(4)]
            row.append((m[0], m[1], m[2], m[3]))
        out.append(row)
    return out


def flip(p: Pic) -> Pic:
    return p[::-1]


def swap_rb(p: Pic) -> Pic:
    return [[(q[2], q[1], q[0], q[3]) for q in row] for row in p]


def _565(c: tuple[int, ...]) -> int:
    return ((c[0] >> 3) << 11) | ((c[1] >> 2) << 5) | (c[2] >> 3)


def _color_block(px: list[tuple[int, int, int, int]], alpha1: bool = False) -> bytes:
    lum = [q[0] * 3 + q[1] * 6 + q[2] for q in px]
    hi, lo = px[lum.index(max(lum))], px[lum.index(min(lum))]
    c0, c1 = _565(hi), _565(lo)
    if alpha1 and any(q[3] < 128 for q in px):
        c0, c1 = min(c0, c1), max(c0, c1)       # three-colour mode: index 3 transparent
    elif c0 < c1:
        c0, c1 = c1, c0
    def rgb(v: int) -> tuple[int, int, int]:
        return ((v >> 11 & 31) * 255 // 31, (v >> 5 & 63) * 255 // 63, (v & 31) * 255 // 31)
    p0, p1 = rgb(c0), rgb(c1)
    if c0 > c1:
        pal = [p0, p1, tuple((2 * a + b) // 3 for a, b in zip(p0, p1)), tuple((a + 2 * b) // 3 for a, b in zip(p0, p1))]
    else:
        pal = [p0, p1, tuple((a + b) // 2 for a, b in zip(p0, p1)), (0, 0, 0)]
    bits = 0
    for k, q in enumerate(px):
        if c0 <= c1 and alpha1 and q[3] < 128:
            i = 3
        else:
            cand = range(4) if c0 > c1 else range(3)
            i = min(cand, key=lambda j: sum((pal[j][c] - q[c]) ** 2 for c in range(3)))
        bits |= i << (2 * k)
    return struct.pack('<HHI', c0, c1, bits)


def _alpha_block(vals: list[int]) -> bytes:
    a0, a1 = max(vals), min(vals)
    if a0 == a1:
        a1 = max(0, a0 - 1) if a0 else 1
        a0, a1 = max(a0, a1), min(a0, a1)
    pal = [a0, a1] + [((7 - k) * a0 + k * a1) // 7 for k in range(1, 7)]
    bits = 0
    for k, v in enumerate(vals):
        i = min(range(8), key=lambda j: abs(pal[j] - v))
        bits |= i << (3 * k)
    return bytes([a0, a1]) + bits.to_bytes(6, 'little')


def blocks_of(p: Pic) -> list[list[tuple[int, int, int, int]]]:
    h, w = len(p), len(p[0])
    out = []
    for by in range(max((h + 3) // 4, 1)):
        for bx in range(max((w + 3) // 4, 1)):
            out.append([p[min(by * 4 + k // 4, h - 1)][min(bx * 4 + k % 4, w - 1)] for k in range(16)])
    return out


def encode(p: Pic, fmt: int) -> bytes:
    """One level in a format, PC order (blocks or texel rows; L8 rows padded to 4 bytes)."""
    if fmt in BLOCK:
        out = bytearray()
        for px in blocks_of(p):
            if fmt in (0, 18):
                out += _color_block(px, alpha1=fmt == 0)
            elif fmt == 1:
                nib = bytearray(8)
                for k, q in enumerate(px):
                    nib[k // 2] |= (q[3] >> 4) << (4 * (k % 2))
                out += bytes(nib) + _color_block(px)
            elif fmt == 2:
                out += _alpha_block([q[3] for q in px]) + _color_block(px)
            elif fmt in (19, 20):
                out += _alpha_block([q[0] for q in px]) + _alpha_block([q[1] for q in px])
            else:
                out += _alpha_block([q[0] for q in px])
        return bytes(out)
    out = bytearray()
    for row in p:
        line = bytearray()
        for q in row:
            if fmt == 10:
                line.append(q[0])
            elif fmt == 9:
                line += bytes([q[2], q[1], q[0], q[3]])
            elif fmt == 5:
                line += struct.pack('<H', _565(q))
            else:
                line += struct.pack('<4e', *(c / 255 for c in q))
        if fmt == 10:
            line += bytes((-len(line)) % 4)
        out += line
    return bytes(out)


def chain(p: Pic, mips: int) -> list[Pic]:
    out = [p]
    for _ in range(mips - 1):
        out.append(half_pic(out[-1]))
    return out


def pc_texture(levels: list[bytes], fmt: int, w: int, h: int, flags: int = 0, typ: int = 0,
               sizes: list[int] | None = None, faces: int = 1) -> bytes:
    sz = sizes if sizes is not None else [len(x) for x in levels]
    head = struct.pack('<8I', 3, typ, fmt, flags, w, h, 1, len(levels)) + struct.pack('<15I', *(sz + [0] * (15 - len(sz))))
    return head + b''.join(levels) * faces


def pc(p: Pic, fmt: int, mips: int = 0, flags: int = 0, **kw: Any) -> bytes:
    h, w = len(p), len(p[0])
    mips = mips or max(w, h).bit_length()
    return pc_texture([encode(x, fmt) for x in chain(p, mips)], fmt, w, h, flags, **kw)


def wrapped(res: bytes) -> bytes:
    head = bytearray(64)
    head[:4] = b'RES\x02'
    head[8:16] = b'ITexture'
    struct.pack_into('<I', head, 56, len(res))
    return bytes(head) + res


def _texture_module() -> Any:
    suite_paths.use('console-convert')
    import texture
    return texture


def xbox(p: Pic, fmt: int, mips: int = 0, flags: int = 0, typ: int = 0) -> bytes:
    """An Xbox 360 XenonTexture of the picture: big-endian header, the levels tiled (texture.layout / tiled, the
    reference), 16-bit words swapped."""
    tx = _texture_module()
    h, w = len(p), len(p[0])
    mips = mips or max(w, h).bit_length()
    bpb, edge, _ = tx.FORMATS[fmt]
    log_bpp = {1: 0, 2: 1, 8: 3, 16: 4}.get(bpb, 2)
    slices = 6 if typ == 1 else 1
    where, size = tx.layout_reference(w, h, mips, fmt, slices)
    data = bytearray(size)
    for s in range(slices):
        for m, lvp in enumerate(chain(p, mips)):
            start, pitch, ox, oy = where[s][m]
            lw, lh = max(w >> m, 1), max(h >> m, 1)
            lbw, lbh = max((lw + edge - 1) // edge, 1), max((lh + edge - 1) // edge, 1)
            enc = encode(lvp, fmt)
            if fmt == 10:
                enc = b''.join(enc[r * ((lw + 3) // 4 * 4):r * ((lw + 3) // 4 * 4) + lw] for r in range(lh))
            for by in range(lbh):
                for bx in range(lbw):
                    off = start + tx.tiled_reference(bx + ox, by + oy, pitch, log_bpp) * bpb
                    k = (by * lbw + bx) * bpb
                    data[off:off + bpb] = enc[k:k + bpb]
    if bpb > 1:
        data[0::2], data[1::2] = data[1::2], data[0::2]
    return struct.pack('>8I', 3, typ, fmt, flags, w, h, 1, mips) + bytes(15 * 4) + bytes(data)


def manifest(entries: dict[str, str]) -> str:
    return ''.join(f'{n}|{t}|0|0\n' for n, t in entries.items())


# ------------------------------------------------------------------------------------------- the cases

def _sets() -> dict[str, dict[str, Any]]:
    """Folders of textures, each {relative path: bytes}."""
    sm = picture(32, 32, 1)
    al = picture(32, 32, 2, 'alpha')
    nm = picture(32, 32, 3, 'normal')
    sets: dict[str, dict[str, Any]] = {}
    # every PC format against its Xbox source of the same picture
    fmt_set: dict[str, Any] = {}
    for fmt in (0, 18, 1, 2, 19, 20, 10, 5, 9, 13):
        pic = nm if fmt in (18, 19, 20) else al if fmt in (1, 2) else sm
        name = {18: 'f18_nm', 19: 'f19_nm', 20: 'f20_n'}.get(fmt, f'f{fmt}_d')
        fmt_set[f'ours\\objects\\{name}.res'] = pc(pic, fmt, flags=1 if fmt in (0, 1, 2) else 0)
        xfmt = {10: 3, 9: 5}.get(fmt, fmt)
        fmt_set[f'src\\objects\\{name}.res'] = xbox(pic, xfmt, flags=1 if fmt in (0, 1, 2) else 0)
    sets['formats'] = fmt_set
    # problems: flipped, swizzled, other art, degenerate, short chain, size errors, sRGB, alpha lost
    pr: dict[str, Any] = {
        'ours\\a\\flip_cm.res': pc(flip(sm), 9),
        'src\\a\\flip_cm.res': xbox(sm, 5),
        'ours\\a\\swz_d.res': pc(swap_rb(sm), 9),
        'src\\a\\swz_d.res': xbox(sm, 5),
        'ours\\a\\other_d.res': pc(picture(32, 32, 9, 'noise'), 0),
        'src\\a\\other_d.res': xbox(picture(32, 32, 10), 0),
        'ours\\a\\black_d.res': pc(picture(16, 16, 4, 'black'), 0),
        'src\\a\\black_d.res': xbox(picture(16, 16, 4, 'black'), 0),
        'ours\\a\\white_s.res': pc(picture(16, 16, 4, 'white'), 9),
        'ours\\a\\const_m.res': pc(picture(16, 16, 4, 'const'), 2),
        'ours\\a\\short_d.res': pc(sm, 0, mips=2),
        'src\\a\\short_d.res': xbox(sm, 0),
        'ours\\a\\sizes_d.res': pc_texture([encode(sm, 0), encode(half_pic(sm), 0)], 0, 32, 32,
                                           sizes=[len(encode(sm, 0)), len(encode(half_pic(sm), 0)) + 8]),
        'ours\\a\\lost_d.res': pc(al, 18),
        'src\\a\\lost_d.res': xbox(al, 2),
        'ours\\a\\nosrgb_cm.res': pc(sm, 0),
        'ours\\a\\srgbn_nm.res': pc(nm, 18, flags=1),
        'ours\\a\\bias_nm.res': pc(picture(16, 16, 5), 19),
        'ours\\a\\zn_n.res': pc(picture(16, 16, 6), 18),
        'ours\\a\\nan_d.res': pc_texture([struct.pack('<4e', float('nan'), 1.0, float('inf'), 0.5) * 64], 13, 8, 8),
        'ours\\a\\clear_d.res': pc(picture(16, 16, 7, 'alpha'), 1, flags=1),
        'ours\\a\\noisy_d.res': pc(picture(64, 64, 8, 'noise'), 9),
        'ours\\a\\unknown.res': pc_texture([bytes(32)], 7, 4, 4),
        'ours\\a\\cut_d.res': pc_texture([encode(sm, 0)[:100]], 0, 32, 32, sizes=[100]),
        'ours\\a\\cube_d.res': pc(picture(16, 16, 11), 0, mips=3, typ=1, faces=6),
        'ours\\a\\wrap_d.res': wrapped(pc(sm, 2)),
        'ours\\a\\small_d.res': pc(picture(4, 2, 12), 9),
        'ours\\a\\wide_d.res': pc(picture(64, 16, 13), 0),
        'src\\a\\wide_d.res': xbox(picture(32, 8, 13), 0),
        'ours\\a\\aspect_d.res': pc(picture(32, 16, 14), 0),
        'src\\a\\aspect_d.res': xbox(picture(32, 32, 14), 0),
        'ours\\a\\badsrc_d.res': pc(sm, 0),
        'src\\a\\badsrc_d.res': b'\x00' * 40,
        'src\\a\\f9src_d.res': b'',
        'ours\\a\\notatexture.res': b'hello',
    }
    pr['ours\\a\\f9src_d.res'] = pc(sm, 9)
    pr['src\\a\\f9src_d.res'] = xbox(sm, 9)
    pr['ours\\a\\fmt99src_d.res'] = pc(sm, 0)
    x99 = bytearray(xbox(sm, 0))
    struct.pack_into('>I', x99, 8, 99)
    pr['src\\a\\fmt99src_d.res'] = bytes(x99)
    pr['ours\\a\\repaint_d.res'] = pc(swap_rb(picture(32, 32, 15)), 9)
    pr['ours\\a\\repaint_d.res.orig'] = pc(picture(32, 32, 15), 9)
    pr['src\\a\\repaint_d.res'] = xbox(picture(32, 32, 15), 5)
    sets['problems'] = pr
    # an unpacked bundle: the manifest says which entries are textures
    bun: dict[str, Any] = {
        'bundle\\fbrb.manifest': manifest({'objects/tank_d.res': 'ITexture', 'objects/tank_mesh.res': 'MeshSet',
                                           'Objects\\Gun_CM.res': 'ITexture', 'menu/icon.res': 'ITexture'}),
        'bundle\\objects\\tank_d.res': pc(sm, 0, flags=1),
        'bundle\\objects\\tank_mesh.res': pc(sm, 0),
        'bundle\\Objects\\Gun_CM.res': wrapped(pc(al, 2, flags=1)),
        'bundle\\menu\\icon.res': pc(al, 2, flags=1),
        'bundle\\skipme\\x_d.res': pc(sm, 0),
        'src\\objects\\tank_d.res': xbox(sm, 0, flags=1),
        'src\\levels\\mp_001\\emittertexturealpha.res': xbox(sm, 0),
        'bundle\\levels\\mp_001\\emittertexturealpha.res': pc(sm, 0),
    }
    bun['bundle\\fbrb.manifest'] += 'levels/mp_001/emittertexturealpha.res|ITexture|0|0\r\n# a comment|x\nno bar\n'
    sets['bundle'] = bun
    # a clean set: nothing flagged (the report has no contact sheets: the run the Rust answers with the default --worst)
    sets['clean'] = {'ours\\clean\\wall_d.res': pc(sm, 0, flags=1), 'src\\clean\\wall_d.res': xbox(sm, 0, flags=1),
                     'ours\\clean\\wall_nm.res': pc(nm, 18), 'src\\clean\\wall_nm.res': xbox(nm, 18)}
    return sets


def _noisy(p: Pic, rng: random.Random, amount: int) -> Pic:
    return [[tuple(max(0, min(255, c + rng.randint(-amount, amount))) for c in q) for q in row]  # type: ignore[misc]
            for row in p]


def _random_set(n: int, seed: int) -> dict[str, Any]:
    """n textures of random sizes (powers of two to 256, odd multiples of 8: the box filter's odd sides), formats,
    roles and pictures; each with an Xbox source of the picture with noise added (a re-encode), some flipped."""
    rng = random.Random(seed)
    out: dict[str, Any] = {}
    roles = ['d', 'cm', 'nm', 's', 'm', 'mask', 'x']
    for i in range(n):
        w = rng.choice([8, 16, 32, 64, 128, 256, 24, 40])
        h = rng.choice([w, w, w // 2 or 8, w * 2 if w <= 64 else w])
        fmt = rng.choice([0, 0, 2, 2, 1, 18, 19, 9, 10, 5, 13])
        kind = rng.choice(['smooth', 'smooth', 'noise', 'alpha', 'normal'])
        pic = picture(w, h, seed * 1000 + i, kind)
        mips = rng.choice([0, 0, 1, 2])
        name = f'ours\\r\\t{i:02d}_{rng.choice(roles)}.res'
        ours = flip(pic) if rng.random() < 0.1 else pic
        out[name] = pc(ours, fmt, mips=mips, flags=rng.choice([0, 1]))
        xf = {10: 3, 9: 5}.get(fmt, fmt)
        out[name.replace('ours\\', 'src\\', 1)] = xbox(_noisy(pic, rng, rng.choice([0, 2, 12, 40])), xf,
                                                       flags=rng.choice([0, 1]))
    return out


def _code() -> str:
    with open(os.path.join(SUITE, 'bfbc2-console-convert', 'texqa.py'), 'rb') as fh:
        return hashlib.sha1(fh.read().replace(b'\r\n', b'\n')).hexdigest()[:12]


def rekey_cache(case: str, rel: str, data: bytes) -> bytes:
    """REKEY (round QW2): a rows cache's keys with texqa.py's source hash taken out. A key hashes [VERSION, the hash of
    texqa.py's source, the texture's sha1, its source, DICE's copy, its .orig's sha1, its name] (texqa.run; the Rust
    reads the same source through include_bytes), so ANY edit of texqa.py - round DF2's to texqa.Dice, which no case
    here runs - changed the three cached cases' file digests: the goldens failed after the merge, and builds made from
    two of DF2's commits wrote two digests, which read as run-to-run noise. Each key the case's own textures give
    under the current hash becomes the same key with '<texqa.py>' in the hash's place; a key that matches none stays
    as it is, so a wrong key still fails."""
    if not re.fullmatch(r'cache[\\/]rows_v\d+\.json', rel):
        return data
    try:
        rows = json.loads(data)
    except ValueError:
        return data
    if not isinstance(rows, dict):
        return data
    files: dict[str, str] = {}
    for d, _ds, fs in os.walk(case):
        for f in fs:
            files[os.path.relpath(os.path.join(d, f), case).replace(os.sep, '/').lower()] = os.path.join(d, f)

    def sha(p: str) -> str:
        with open(p, 'rb') as fh:
            return hashlib.sha1(fh.read()).hexdigest()
    code = _code()
    known: dict[str, str] = {}                 # a key under the current hash -> the same key under '<texqa.py>'
    for r, p in files.items():
        if r.endswith('.orig') or not r.endswith('.res'):
            continue
        parts = r.split('/')
        for name in ('/'.join(parts[i:]) for i in range(len(parts))):     # the name under any input root
            for orig in [None] + ([sha(files[r + '.orig'])] if r + '.orig' in files else []):
                ident: list[Any] = [1, code, sha(p), None, None, orig, name]
                key = hashlib.sha1(json.dumps(ident).encode()).hexdigest()
                ident[1] = '<texqa.py>'
                known[key] = hashlib.sha1(json.dumps(ident).encode()).hexdigest()
    for key in rows:
        if key in known:
            data = data.replace(key.encode(), known[key].encode())
    return data


REKEY = [rekey_cache]


def _cache_with(data: bytes, name: str, row: dict[str, Any]) -> str:
    ident = [1, _code(), hashlib.sha1(data).hexdigest(), None, None, None, name.lower()]
    key = hashlib.sha1(json.dumps(ident).encode()).hexdigest()
    return json.dumps({key: row, 'other': {'name': 'x', 'flags': []}}, indent=0)


def cases() -> list[Case]:
    sets = _sets()
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = []
    fmt, pr, bun, clean = sets['formats'], sets['problems'], sets['bundle'], sets['clean']
    rows += [
        ('formats against their Xbox sources', fmt, ['{d}\\ours', '--console', '{d}\\src', '--worst', '0'] + BASE, {}),
        ('formats alone', fmt, ['{d}\\ours', '--worst', '0'] + BASE, {}),
        ('problems against sources', pr, ['{d}\\ours', '--console', '{d}\\src', '--worst', '0'] + BASE, {}),
        ('problems alone', pr, ['{d}\\ours', '--worst', '0'] + BASE, {}),
        ('problems, the default --worst (contact sheets: the Python)', pr,
         ['{d}\\ours', '--console', '{d}\\src'] + BASE, {}),
        ('a bundle folder with its manifest', bun,
         ['{d}\\bundle', '--console', '{d}\\src', '--worst', '0', '--skip', 'SKIPME'] + BASE, {}),
        ('a bundle folder, nothing skipped', bun, ['{d}\\bundle', '--worst=0'] + BASE, {}),
        ('a clean set, the default --worst', clean, ['{d}\\ours', '--console', '{d}\\src'] + BASE, {}),
        ('a clean set, two inputs and a file', clean,
         ['{d}\\ours\\clean', '{d}\\ours\\clean\\wall_d.res', '--console', '{d}\\src', '--console', '{d}\\none'] + BASE,
         {}),
        ('one file', clean, ['{d}\\ours\\clean\\wall_d.res'] + BASE, {}),
        # a source's key names its path (the case folder's, another each run): the cache written without sources
        # here; with them in the --live comparison (the same folder both ways) and in the game tier
        ('a cache written', pr, ['{d}\\ours', '--worst', '0', '--cache', '{d}\\cache'] + BASE, {}),
        ('a write-baseline', pr, ['{d}\\ours', '--worst', '0', '--write-baseline', '{d}\\b\\base.json'] + BASE, {}),
        ('workers 1', fmt, ['{d}\\ours', '--console', '{d}\\src', '--worst', '0', '--workers', '1'] + BASE, {}),
        ('BF1943 folder missing (BF1943_XENON)', clean,
         ['{d}\\ours', '--console', '{d}\\src', '--no-dice', '-o', '{d}\\out'], {'BF1943_XENON': '{d}\\nobf'}),
        ('BF1943 folder given, missing', clean,
         ['{d}\\ours', '--bf1943', '{d}\\nobf', '--console', '{d}\\src', '--no-dice', '-o', '{d}\\out', '--opfor',
          '{d}'], {}),
        # (a BF1943 folder that is there, and DICE's copies, are handed to the Python; their answers depend on the
        # projects beside the checkout - bf1943-tool-suite, Project OPFOR - so they are not recorded here)
        ('a missing input', {}, ['{d}\\missing'] + BASE, {}),
        ('a missing input outside cp1252', {}, ['{d}\\Ж'] + BASE, {}),
        ('an empty input', {}, [''] + BASE, {}),
        ('no inputs (argparse)', {}, BASE, {}),
        ('--help (argparse)', {}, ['--help'], {}),
        ('an unknown option (argparse)', clean, ['{d}\\ours', '--bogus'] + BASE, {}),
        ('a bad --worst (argparse)', clean, ['{d}\\ours', '--worst', 'x'] + BASE, {}),
        ('inputs split by an option (argparse)', clean, ['{d}\\ours', '--no-dice', '{d}\\ours'] + BASE, {}),
        ('an empty folder', {'empty': None}, ['{d}\\empty'] + BASE, {}),
        ('names in cp1252 and outside', {'ours\\café_d.res': pc(picture(8, 8, 3), 0),
                                          'ours\\Жук_d.res': pc(picture(8, 8, 4), 0)},
         ['{d}\\ours', '--worst', '0'] + BASE, {}),
    ]
    # a cache seeded with a row for a texture (reused as it is) and a damaged cache
    data = pc(picture(16, 16, 21), 0, flags=1)
    seeded = {'ours\\x\\seed_d.res': data, 'ours\\x\\fresh_d.res': pc(picture(16, 16, 22), 0, flags=1),
              'cache\\rows_v1.json': _cache_with(data, 'x/seed_d.res', {'name': 'cached', 'flags': [], 'score': 0})}
    rows.append(('a cache with a row', seeded, ['{d}\\ours', '--worst', '0', '--cache', '{d}\\cache'] + BASE, {}))
    rows.append(('a damaged cache', dict(seeded, **{'cache\\rows_v1.json': '{not json'}),
                 ['{d}\\ours', '--worst', '0', '--cache', '{d}\\cache'] + BASE, {}))
    # baselines: regressions (a PSNR drop, a new error, a new texture) and none
    base = {'version': 1, 'textures': {
        'ours/a/flip_cm.res': {'psnr_source': 'inf', 'psnr_dice': None, 'flags': []},
        'ours/a/other_d.res': {'psnr_source': 30.5, 'psnr_dice': None, 'flags': ['differs from the source']},
        'ours/a/swz_d.res': {'psnr_source': 2, 'psnr_dice': None, 'flags': ['swizzle']}}}
    rows.append(('a baseline with regressions', dict(pr, **{'base.json': json.dumps(base)}),
                 ['{d}\\ours', '--console', '{d}\\src', '--worst', '0', '--baseline', '{d}\\base.json',
                  '--threshold', '0.5'] + BASE, {}))
    rows.append(('a baseline without regressions', dict(clean, **{'base.json': json.dumps({'textures': {}})}),
                 ['{d}\\ours', '--console', '{d}\\src', '--baseline', '{d}\\base.json'] + BASE, {}))
    rows.append(('a missing baseline', clean, ['{d}\\ours', '--baseline', '{d}\\nobase.json'] + BASE, {}))
    # many textures: the "50 textures analysed" lines, stdout past one chunk
    many = {f'ours\\m\\t{i:03d}_d.res': pc(picture(8, 8, 100 + i), 0, flags=1) for i in range(120)}
    rows.append(('120 textures', many, ['{d}\\ours', '--worst', '0'] + BASE, {}))
    rows.append(('120 textures, the default --worst (sheets: the Python)', many, ['{d}\\ours'] + BASE, {}))
    rows.append(('48 random textures against noisy sources', _random_set(48, 7),
                 ['{d}\\ours', '--console', '{d}\\src', '--worst', '0'] + BASE, {}))
    rows.append(('48 random textures alone', _random_set(48, 8), ['{d}\\ours', '--worst', '0'] + BASE, {}))
    out: list[Case] = [(f'texqa: {n}', 'texqa', f, a, dict(ENV, **e), False) for n, f, a, e in rows]
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in list(out)]
    picked = ('texqa: names in cp1252 and outside', 'texqa: a missing input outside cp1252', 'texqa: 120 textures')
    for label, env in (('PYTHONUTF8=1', {'PYTHONUTF8': '1'}), ('PYTHONUNBUFFERED=1', {'PYTHONUNBUFFERED': '1'})):
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


# ------------------------------------------------------------------------------------------- the game

def opfor_calls(opfor: str) -> list[Call]:
    """texqa over OPFOR's converted textures, read in place (outputs and caches in the case folder): the WW2
    vehicles (bench_sweep's texqa_ww2; BF1943's bundles and DICE's copies are the Python's, so with --no-bf1943
    --no-dice and the sheets off) and the bazooka; then bench_sweep's real line (BF1943 source, DICE's copies,
    sheets), which the Python answers."""
    work = os.path.join(opfor, 'work')
    calls: list[Call] = []
    ww2 = os.path.join(work, 'ww2')
    for sub in ('bf1943_vehicles', 'bf1943_jp_bazooka'):
        p = os.path.join(ww2, sub)
        if os.path.isdir(p):
            calls.append((f'texqa {sub} (no sheets, no BF1943 / DICE)', 'texqa',
                          [p, '--worst', '0', '--cache', '{d}\\cache'] + BASE, {}, dict(ENV), False))
    p = os.path.join(ww2, 'bf1943_vehicles')
    if os.path.isdir(p):
        calls.append(('texqa bf1943_vehicles, a baseline written and read', 'texqa',
                      [p, '--worst', '0', '--write-baseline', '{d}\\b\\base.json'] + BASE, {}, dict(ENV), False))
        # the folders texqa finds beside the suite, named (the replay's Python runs from a copy elsewhere)
        env = dict(ENV)
        projects = os.path.dirname(SUITE)
        xenon = os.path.join(projects, 'operation-rising-sun', 'work', 'extracted', 'dist', 'xenon')
        fbrb1943 = os.path.join(projects, 'bf1943-tool-suite', 'bf1943-fbrb-tool')
        if os.path.isdir(xenon):
            env['BF1943_XENON'] = xenon
        if os.path.isdir(fbrb1943):
            env['BF1943_SUITE_FBRB'] = fbrb1943
        calls.append(('texqa bf1943_vehicles: bench_sweep\'s line (BF1943, DICE, sheets: the Python)', 'texqa',
                      [p, '-o', '{d}\\out', '--cache', '{d}\\cache', '--worst', '4', '--opfor', opfor], {}, env, True))
    return calls
