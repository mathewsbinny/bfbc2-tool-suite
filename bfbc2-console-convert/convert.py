r"""Converts Xbox 360 Bad Company 2 resources to the PC's formats (the user's own console files; nothing is shipped).

  python convert.py havok <console.res> <out.res> [--exe <BFBC2Game.exe>]
  python convert.py verify-havok <pc folder> <console folder>... [--exe <BFBC2Game.exe>] [--report <file>]
  python convert.py texture <console.res> <out.res>
  python convert.py verify-texture <pc folder> <console folder>... [--report <file>]

havok         one HavokPhysicsData resource (physics). The class layouts come from the game executable (default:
              Project OPFOR BFBC2's game copy).
verify-havok  converts every HavokPhysicsData resource that an unpacked console bundle folder and an unpacked PC
              bundle folder both have, and compares: exact, differing only in values the game never reads or
              rebuilds at load (FORMAT.md 1.3, counted by reason), or wrong. Exit code 1 if any is wrong.
texture       one XenonTexture resource -> ITexture (DXT1 / DXT3 / DXT5 / BC5, 2D and cube maps).
verify-texture converts every texture both folders have (same size and format) and checks it (FORMAT.md 2.3):
              byte-identical, or the same picture re-encoded (correlation with the PC's level 0 over 0.9) with a
              mip chain no worse than the PC file's own (+3 or +25 %); different art is counted, not failed. Exit code 1
              if a conversion fails or a mip chain is broken.

Format notes: FORMAT.md.
"""
import argparse
import collections
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..', 'core'))
from bfbc2core import errors  # noqa: E402
import havok  # noqa: E402
import texture  # noqa: E402


def default_exe():
    for root in (os.environ.get('OPFOR_ROOT'), os.path.join(HERE, '..', '..', 'project-opfor-bfbc2')):
        if root and os.path.isfile(os.path.join(root, 'game', 'BFBC2Game.exe')):
            return os.path.join(root, 'game', 'BFBC2Game.exe')
    raise FileNotFoundError('no game copy (Project OPFOR BFBC2) found for BFBC2Game.exe: pass --exe')


def manifest(folder, kind):
    """{lower-case relative path: full path} of the folder's entries of a resource type (its fbrb.manifest)."""
    out = {}
    man = os.path.join(folder, 'fbrb.manifest')
    for line in open(man, encoding='utf-8', errors='replace'):
        f = line.rstrip('\n').split('|')
        if len(f) >= 2 and f[1] == kind:
            p = os.path.join(folder, *f[0].split('/'))
            if not os.path.isfile(p):                 # entry names keep the bundle's case; the disk may not
                p = _find(folder, f[0])
            if p:
                out[f[0].lower()] = p
    return out


def _find(root, rel):
    cur = root
    for part in rel.split('/'):
        if not os.path.isdir(cur):
            return None
        m = [x for x in os.listdir(cur) if x.lower() == part.lower()]
        if not m:
            return None
        cur = os.path.join(cur, m[0])
    return cur


def cmd_havok(a):
    k = havok.classes(a.exe or default_exe())
    data = open(a.input, 'rb').read()
    out = havok.convert(data, k)
    open(a.output, 'wb').write(out)
    print(f'{a.output}: {len(out):,} bytes')
    return 0


def cmd_verify_havok(a):
    k = havok.classes(a.exe or default_exe())
    pc = manifest(a.pc, 'HavokPhysicsData')
    con = {}
    for f in a.console:
        for rel, p in manifest(f, 'HavokPhysicsData').items():
            con.setdefault(rel, p)
    total = collections.Counter()
    reasons = collections.Counter()
    lines = []
    for rel in sorted(set(pc) & set(con)):
        c, p = open(con[rel], 'rb').read(), open(pc[rel], 'rb').read()
        if len(c) != len(p):
            total['different asset (sizes differ)'] += 1
            lines.append(f'different asset  {rel}')
            continue
        try:
            exact, why, bad = havok.verify(c, p, k)
        except havok.ConvertError as e:
            total['error'] += 1
            lines.append(f'ERROR  {rel}: {e}')
            continue
        if bad:
            total['wrong'] += 1
            lines.append(f'WRONG  {rel}: {len(bad)} bytes, first at {bad[0]:#x}')
        elif exact:
            total['exact'] += 1
        else:
            total['equivalent'] += 1
            reasons.update(why)
    print(f'{sum(total.values())} files: ' + ', '.join(f'{v} {k}' for k, v in total.most_common()))
    for r, n in reasons.most_common():
        print(f'  differing bytes the game does not read or rebuilds: {n:,} {r}')
    if a.report:
        open(a.report, 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    for l in lines:
        if not l.startswith('different'):
            print(l)
    return 1 if total['wrong'] or total['error'] else 0


def cmd_texture(a):
    out = texture.convert(open(a.input, 'rb').read())
    open(a.output, 'wb').write(out)
    print(f'{a.output}: {len(out):,} bytes')
    return 0


def cmd_verify_texture(a):
    pc = manifest(a.pc, 'ITexture')
    con = {}
    for f in a.console:
        for rel, p in manifest(f, 'XenonTexture').items():
            con.setdefault(rel, p)
    total, lines = collections.Counter(), []
    for rel in sorted(set(pc) & set(con)):
        c, p = open(con[rel], 'rb').read(), open(pc[rel], 'rb').read()
        if c[8:12][::-1] != p[8:12] or c[16:24] != bytes(reversed(p[16:20])) + bytes(reversed(p[20:24])):
            total['not comparable (size or format differs)'] += 1
            continue
        try:
            out = texture.convert(c)
        except texture.ConvertError as e:
            total['not converted'] += 1
            lines.append(f'not converted  {rel}: {e}')
            continue
        if out == p:
            total['byte-identical'] += 1
            continue
        ours, corr, same, n = texture.check(out, p)
        theirs = texture.check(p)[0]
        if ours > max(theirs + 3, theirs * 1.25):
            total['broken mip chain'] += 1
            lines.append(f'BROKEN  {rel}: mip chain error {ours:.1f} (the PC file {theirs:.1f})')
        elif corr is not None and corr < 0.9:
            total['different art'] += 1
        else:
            total['same picture, re-encoded'] += 1
    print(f'{sum(total.values())} textures: ' + ', '.join(f'{v} {k}' for k, v in total.most_common()))
    for l in lines:
        print(l)
    if a.report:
        open(a.report, 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    return 1 if total['broken mip chain'] else 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    sub = ap.add_subparsers(dest='cmd', required=True)
    h = sub.add_parser('havok')
    h.add_argument('input')
    h.add_argument('output')
    h.add_argument('--exe')
    v = sub.add_parser('verify-havok')
    v.add_argument('pc')
    v.add_argument('console', nargs='+')
    v.add_argument('--exe')
    v.add_argument('--report')
    t = sub.add_parser('texture')
    t.add_argument('input')
    t.add_argument('output')
    vt = sub.add_parser('verify-texture')
    vt.add_argument('pc')
    vt.add_argument('console', nargs='+')
    vt.add_argument('--report')
    a = ap.parse_args(argv[1:])
    return {'havok': cmd_havok, 'verify-havok': cmd_verify_havok, 'texture': cmd_texture,
            'verify-texture': cmd_verify_texture}[a.cmd](a)


if __name__ == '__main__':
    errors.setup('console-convert', gui=False)
    with errors.guard():
        sys.exit(main(sys.argv))
