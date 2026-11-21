"""L1: read a stock level's partition straight from the game copy's bundles (read only, nothing unpacked).

The level partition is levels/<lv>.dbx inside the level's level-00.fbrb; a Package copy wins over Dist (as in
the game and as bfbc2core.level does). Parsed with the suite's dbx reader (bfbc2core.dbx, Rust core).
"""
import os
import sys

SUITE = os.environ.get('BFBC2_SUITE', r'C:\Programming\Projects\bfbc2-tool-suite')
OPFOR = os.environ.get('OPFOR_ROOT', r'C:\Programming\Projects\project-opfor-bfbc2')
sys.path.insert(0, os.path.join(SUITE, 'core'))

from bfbc2core import dbx, fbrb  # noqa: E402

GAME = os.path.join(OPFOR, 'game')


def bundles(lv: str) -> list[str]:
    """level-00.fbrb candidates for a level: Package first, then Dist."""
    out = []
    for top in (os.path.join(GAME, 'Package', 'levels'), os.path.join(GAME, 'Dist', 'win32', 'levels')):
        if not os.path.isdir(top):
            continue
        for d in os.listdir(top):
            if d.lower() == lv.lower():
                p = os.path.join(top, d, 'level-00.fbrb')
                if os.path.isfile(p):
                    out.append(p)
    return out


def partition(lv: str) -> tuple[str, str, dbx.DbxFile]:
    """(bundle path, entry name, parsed partition) of a level."""
    for b in bundles(lv):
        names = fbrb.names(b)
        want = None
        for n in names:
            base = n.replace('\\', '/')
            if base.startswith('levels/') and base.count('/') == 1 and base.endswith('.dbx') \
                    and os.path.splitext(base.split('/')[-1])[0] == lv.lower():
                want = names[n].name
        if want:
            data = fbrb.read_uncached(b, [want])[want]
            return b, want, dbx.parse(data)
    raise FileNotFoundError(f'{lv}: no level partition in {bundles(lv)}')


def entries(lv: str) -> dict[str, list[str]]:
    return {b: [e.name for e in fbrb.entries(b) if not e.deleted] for b in bundles(lv)}


if __name__ == '__main__':
    from collections import Counter
    lv = sys.argv[1]
    b, name, f = partition(lv)
    print(b, name, len(f.instances))
    c = Counter((e.attr('type') or '').split('.')[-1] for e in f.instances)
    for t, n in sorted(c.items()):
        print(f'{n:5d} {t}')
