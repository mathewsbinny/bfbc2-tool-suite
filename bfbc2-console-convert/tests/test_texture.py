r"""texture.py on the game's files (game tier; skipped without them):

- the layout's sizes: every console texture of OPFOR's Onslaught builds in a converted format is exactly as long as
  the layout says, and converts;
- every texture a stock PC bundle and a console bundle share (same size and format; PC: the stock bundles unpacked
  under OPFOR's work - audit, common, levels other than ons_*, console_deps\src's PC levels - never folders with
  console-derived files; console: the Onslaught builds' con_* folders) converts to a file that is byte-identical, or
  the same picture re-encoded (correlation of level 0 over 0.9) with a mip chain no worse than the PC file's own
  (+3 or +25 %); at least 180 such pairs, at least one cube map among them;
- formats the converter does not know are refused with a ConvertError.

  python test_texture.py
"""
import collections
import glob
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import texture  # noqa: E402

OPFOR = os.environ.get('OPFOR_ROOT') or os.path.abspath(os.path.join(HERE, '..', '..', '..', 'project-opfor-bfbc2'))
WORK = os.path.join(OPFOR, 'work')


def console_folder(root):
    parts = [p.lower() for p in root.split(os.sep)]
    return any(p.startswith('build_') for p in parts) and any(p.startswith('con_') for p in parts)


def pc_folder(root):
    rel = os.path.relpath(root, WORK).replace(os.sep, '/').lower()
    return bool(re.match(r'(audit|common)/[^/]+$|levels/(?!ons_)[^/]+$|console_deps/src/dist_win32_levels_(?!ons_)'
                         r'[^/]+$', rel))


def index(kind, console):
    out = {}
    for root, _, files in os.walk(WORK):
        if 'fbrb.manifest' not in files or not (console_folder(root) if console else pc_folder(root)):
            continue
        for line in open(os.path.join(root, 'fbrb.manifest'), encoding='utf-8', errors='replace'):
            f = line.split('|')
            if len(f) >= 2 and f[1] == kind:
                p = os.path.join(root, *f[0].split('/'))
                if os.path.isfile(p) and os.path.getsize(p) >= texture.HEADER:
                    out.setdefault(f[0].lower(), p)
    return out


# BFBC2 twins in console formats 5 (RGB565) and 13 (RGBA16F): (name, console, PC) - same size where noted
TWINS_5_13 = ['terrains/_textures/_general/mudmask_01_rgbm.res', 'terrains/mp_003/textures/mp03_mudmask.res',
              'terrains/mp_004/textures/mp04_mudmask.res', 'textures/sky/sky_beach_generic_rgba16f.res']


def formats_5_13(fails):
    """Console 5 -> PC 9 within the RGB565 rounding (red / blue 8, green 4) on the twins of the same size; console 13
    (RGBA16F cube) untiled: its level 0 equals the PC's level 1 (the PC copy is twice the size) on every face."""
    sys.path.insert(0, os.path.join(HERE, '..', '..', 'core'))
    from bfbc2core import fbrb
    import numpy as np
    want = set(TWINS_5_13)
    found = {'x': {}, 'p': {}}
    for tag, pat in (('x', os.path.join(WORK, 'xbox', '**', '*.fbrb')), ('p', os.path.join(OPFOR, 'game', '**', '*.fbrb'))):
        for path in glob.glob(pat, recursive=True):
            try:
                for e in fbrb.entries(path):
                    if e.name.lower() in want and not getattr(e, 'deleted', False):
                        found[tag].setdefault(e.name.lower(), (path, e.name))
            except Exception:                                    # noqa: BLE001 - not a bundle this reader takes
                continue
    pairs = [n for n in TWINS_5_13 if n in found['x'] and n in found['p']]
    if not pairs:
        print('SKIP console 5 / 13: no BFBC2 twins')
        return
    ok5 = ok13 = 0
    for n in pairs:
        x = fbrb.read(found['x'][n][0], [found['x'][n][1]])[found['x'][n][1]]
        p = fbrb.read(found['p'][n][0], [found['p'][n][1]])[found['p'][n][1]]
        out = texture.convert(x)
        fmt = struct.unpack_from('<I', out, 8)[0]
        w, h = struct.unpack_from('<2I', out, 16)
        if fmt == texture.BGRA8:
            a = np.frombuffer(out, np.uint8, w * h * 4, texture.HEADER).reshape(-1, 4).astype(int)
            b = np.frombuffer(p, np.uint8, w * h * 4, texture.HEADER).reshape(-1, 4).astype(int)
            d = np.abs(a - b).max(0)
            ok5 += bool(d[0] <= 8 and d[1] <= 4 and d[2] <= 8 and d[3] == 0)
        else:
            sizes = struct.unpack_from('<15I', p, 32)
            n0 = w * h * 8
            psum = sum(sizes[:struct.unpack_from('<I', p, 28)[0]])
            osum = sum(struct.unpack_from('<15I', out, 32)[:struct.unpack_from('<I', out, 28)[0]])
            ok13 += all(out[texture.HEADER + f * osum:texture.HEADER + f * osum + n0] ==
                        p[texture.HEADER + f * psum + sizes[0]:texture.HEADER + f * psum + sizes[0] + n0] for f in range(6))
    n5 = sum(1 for n in pairs if 'rgba16f' not in n)
    print(f'console 5 (RGB565 -> PC 9): {ok5} of {n5} twins within the rounding; console 13 (RGBA16F cube): '
          f'{ok13} of {len(pairs) - n5} equal to the PC\'s level 1 on every face')
    if ok5 != n5 or ok13 != len(pairs) - n5:
        fails.append('console 5 / 13: a twin out of bounds')


def main():
    fails5 = []
    formats_5_13(fails5)
    for f in fails5:
        print('FAIL', f)
    if not glob.glob(os.path.join(WORK, 'onslaught', 'build_*', 'con_*')):
        print('skipped: needs Project OPFOR BFBC2 with the console Onslaught builds')
        return 1 if fails5 else 0
    fails = []
    con = index('XenonTexture', True)
    pc = index('ITexture', False)
    done = collections.Counter()
    for rel, p in con.items():
        b = open(p, 'rb').read()
        fmt = struct.unpack_from('>I', b, 8)[0]
        if fmt not in texture.FORMATS:
            try:
                texture.convert(b)
                fails.append(f'{rel}: format {fmt} converted although it is not known')
            except texture.ConvertError:
                done['refused (format)'] += 1
            continue
        ver, typ, _, _, w, h, depth, mips = struct.unpack_from('>8I', b, 0)
        _, size = texture.layout(w, h, mips, fmt, 6 if typ == texture.CUBE else 1)
        if size != len(b) - texture.HEADER:
            fails.append(f'{rel}: {len(b) - texture.HEADER} bytes, the layout says {size}')
            continue
        try:
            texture.convert(b)
            done['converted'] += 1
        except texture.ConvertError as e:
            fails.append(f'{rel}: {e}')
    checked = collections.Counter()
    for rel in sorted(set(pc) & set(con)):
        c, p = open(con[rel], 'rb').read(), open(pc[rel], 'rb').read()
        cf = struct.unpack_from('>I', c, 8)[0]
        if cf not in texture.FORMATS or cf in (texture.DXT5A, texture.RGB565):   # decoded to another format: own checks
            continue
        out = texture.convert(c)
        if out[:texture.HEADER] != p[:texture.HEADER]:
            continue
        cube = struct.unpack_from('<I', out, 4)[0] == texture.CUBE
        if out == p:
            checked['byte-identical'] += 1
            continue
        ours, corr, _, _ = texture.check(out, p)
        theirs = texture.check(p)[0]
        if ours > max(theirs + 3, theirs * 1.25):
            fails.append(f'{rel}: mip chain error {ours:.1f}, the PC file {theirs:.1f}')
        elif corr is not None and corr < 0.9:
            checked['different art'] += 1
        else:
            checked['same picture' + (' (cube)' if cube else '')] += 1
    # console 3 (DXT5A) -> PC 8-bit: the console copies are half the PC's size, so the converted level 0 is held
    # against the PC's level 1
    import numpy as np
    corrs = []
    for rel in sorted(set(pc) & set(con)):
        c = open(con[rel], 'rb').read()
        if struct.unpack_from('>I', c, 8)[0] != texture.DXT5A:
            continue
        p = open(pc[rel], 'rb').read()
        out = texture.convert(c)
        w, h = struct.unpack_from('<2I', out, 16)
        pw, ph, = struct.unpack_from('<2I', p, 16)
        if struct.unpack_from('<I', p, 8)[0] != texture.L8 or (pw, ph) != (2 * w, 2 * h):
            continue
        mine = np.frombuffer(out, np.uint8, w * h, texture.HEADER).astype(float)
        theirs = np.frombuffer(p, np.uint8, w * h, texture.HEADER + struct.unpack_from('<I', p, 32)[0]).astype(float)
        if mine.std() > 0 and theirs.std() > 0:
            corrs.append(float(np.corrcoef(mine, theirs)[0, 1]))
    if corrs:
        print(f'console 3 (DXT5A): {len(corrs)} pairs, the PC level 1 matched with correlation {min(corrs):.4f} at least')
        if min(corrs) < 0.98:
            fails.append(f'console 3: a pair matched with correlation {min(corrs):.3f} only')
    else:
        print('console 3 (DXT5A): no pair to compare')
    print(f'console: {dict(done)}; shared: {dict(checked)}')
    if checked['same picture'] + checked['byte-identical'] < 180:
        fails.append(f'only {checked["same picture"] + checked["byte-identical"]} comparable pairs (expected 180+)')
    if not checked['same picture (cube)']:
        fails.append('no cube map compared')
    fails += fails5
    for f in fails[:20]:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
