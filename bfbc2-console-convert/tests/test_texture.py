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


def main():
    if not glob.glob(os.path.join(WORK, 'onslaught', 'build_*', 'con_*')):
        print('skipped: needs Project OPFOR BFBC2 with the console Onslaught builds')
        return 0
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
        if struct.unpack_from('>I', c, 8)[0] not in texture.FORMATS:
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
    print(f'console: {dict(done)}; shared: {dict(checked)}')
    if checked['same picture'] + checked['byte-identical'] < 180:
        fails.append(f'only {checked["same picture"] + checked["byte-identical"]} comparable pairs (expected 180+)')
    if not checked['same picture (cube)']:
        fails.append('no cube map compared')
    for f in fails[:20]:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
