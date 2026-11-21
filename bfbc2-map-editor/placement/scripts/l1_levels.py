"""L1: the stock multiplayer levels to measure, with era and mode.

The mode of each level comes from the game copy's maps.ini (OPFOR's catalogue: one section per map, a key per mode,
the value the level path); the Onslaught levels (ons_*) and the Air Superiority level are added by name. The era is
from the name: bc1_* BFBC1, nam_* Vietnam, OPFOR's WW2 ports (BF1943 data) WW2, else BFBC2. Levels that are in the
game copy but not in maps.ini are listed by `extra()` so coverage can name them.
"""
import configparser
import os

from l1_read import GAME

WW2 = {'wake_island_s', 'guadal_canal', 'iwo_jima_s', 'coral_sea'}
MODES = {'conquest': 'CQ', 'rush': 'Rush', 'squad rush': 'SR', 'squad deathmatch': 'SDM', 'onslaught': 'ONS',
         'air superiority': 'AS'}


def era(lv: str) -> str:
    lv = lv.lower()
    if lv.startswith('bc1_'):
        return 'BFBC1'
    if lv.startswith('nam_'):
        return 'Vietnam'
    if lv in WW2:
        return 'WW2'
    return 'BFBC2'


def levels() -> list[tuple[str, str, str, str]]:
    """[(level, era, mode, map section)] from maps.ini, in its order; Onslaught by name."""
    cp = configparser.ConfigParser(strict=False, interpolation=None)
    cp.read(os.path.join(GAME, 'maps.ini'), encoding='utf-8')
    out = []
    seen = set()
    for sec in cp.sections():
        for k, v in cp.items(sec):
            m = MODES.get(k.lower())
            if not m:
                continue
            lv = v.split('/')[-1].lower()
            if lv in seen:
                continue
            seen.add(lv)
            out.append((lv, era(lv), m, sec))
    # Onslaught: the console ports OPFOR installed (stock Xbox data), whether maps.ini lists them or not
    for top in (os.path.join(GAME, 'Package', 'levels'), os.path.join(GAME, 'Dist', 'win32', 'levels')):
        for d in sorted(os.listdir(top)):
            if d.lower().startswith('ons_') and d.lower() not in seen \
                    and os.path.isfile(os.path.join(top, d, 'level-00.fbrb')):
                seen.add(d.lower())
                out.append((d.lower(), 'BFBC2', 'ONS', d.upper()[4:]))
    return out


def extra() -> list[str]:
    """Level folders with a level bundle that levels() does not list (common bundles and campaign left out)."""
    have = {x[0] for x in levels()}
    out = set()
    for top in (os.path.join(GAME, 'Package', 'levels'), os.path.join(GAME, 'Dist', 'win32', 'levels')):
        for d in os.listdir(top):
            dl = d.lower()
            if dl in have or dl.endswith('common') or dl.startswith('sp_'):
                continue
            if os.path.isfile(os.path.join(top, d, 'level-00.fbrb')):
                out.add(dl)
    return sorted(out)


if __name__ == '__main__':
    for x in levels():
        print(*x)
    print('extra', extra())
