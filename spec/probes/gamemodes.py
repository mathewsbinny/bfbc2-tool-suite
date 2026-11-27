"""Game modes and the game mod (era) as DICE's data and the executable hold them (read-only; spec/engine/gamemodes.md,
round GM0).

    python spec/probes/gamemodes.py [--opfor <folder>]

The data side reads DICE's bundles in Project OPFOR BFBC2's game copy (files dated before 2013): the two startup
bundles (`Dist/win32/async/startup-00.fbrb` and the Vietnam patch's `Package/async/startup-00.fbrb`, or the copy
OPFOR's installers kept of it, `startup-00.fbrb.opfor-orig`, when the live one is OPFOR's), the main-menu and level
bundles. From them: every `GameModeData` (its identifiers), the level descriptions counted by mode, content set and
licence, `Menu/shared_config`'s per-era fields, the matchmaking data's mode names, and where the mode partitions live
(and the deletion markers the mode levels carry for their map's other modes).

The executable side reads `game/BFBC2Game.exe` (PC build 795745 only; data read and decoded with capstone, no code
run): every instruction whose displacement is the UI manager's game mod (+0xC8C8C) or requested game mod (+0xC8C90),
how each uses it, and the instructions that push (or load into esi / edi) the mode-id, matchmaking-id, game-mod tag and
persistence-template strings.

Prints identifiers and counts only (mode ids, field names, addresses; no text, no bytes). `checks(game)` recomputes
the stable answers the rules cite for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import struct
import sys
import time
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
if str(SUITE) not in sys.path:
    sys.path.insert(0, str(SUITE))
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx, fbrb, paths  # noqa: E402

BUILD_STAMP = 0x4E55445D                     # BFBC2Game.exe build 795745
GAME_MOD, REQUESTED = 0xC8C8C, 0xC8C90       # UiManager (0x19A4CA8) fields, ERA-MOD-1
FIELDS = ('ModeID', 'MatchmakingQueryID', 'Description', 'HudRootName', 'PrimaryLogicLayerName',
          'SecondaryLogicLayerName', 'CrosshairsName', 'InGameMenuName', 'LoaderName', 'AddToIngameBundleRefs',
          'AddToLoaderBundleRefs')
# strings whose loads the rules count: (group, text); the probe finds each string's address in the exe itself
LITERALS = (('mode', 'conquest'), ('mode', 'goldrush'), ('mode', 'squadrush'), ('mode', 'tdm'), ('mode', 'singleplayer'),
            ('query', 'CONQUEST'), ('query', 'RUSH'), ('query', 'SQDM'), ('query', 'SQRUSH'),
            ('tag', 'BC2'), ('tag', 'VIETNAM'), ('tag', 'gameMod'), ('tag', 'nam'),
            ('persistence', 'Persistence/MP_Template'), ('persistence', 'Persistence/SP_Template'))
# strings a third era or a further mode would need and DICE never put in the exe
ABSENT = ('PACIFIC', 'BFBC2143', 'BFBC2Beach', 'BC1', 'BFBC1', 'onslaught', 'ONSLAUGHT', 'Persistence/Ons_Template',
          'airsuperiority')


def stock(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def startup_bundles(game: Path) -> list[Path]:
    """DICE's two startup bundles, Dist first (the Package copy overrides it entry by entry)."""
    out = []
    dist = game / 'Dist' / 'win32' / 'async' / 'startup-00.fbrb'
    if dist.is_file() and stock(dist):
        out.append(dist)
    pkg = game / 'Package' / 'async' / 'startup-00.fbrb'
    for p in (pkg, pkg.with_name(pkg.name + '.opfor-orig')):
        if p.is_file() and stock(p):
            out.append(p)
            break
    return out


def text(e: dbx.Element | None) -> str:
    if e is None:
        return ''
    ref = e.attr('ref')
    if ref is not None:
        return ref.rsplit('/', 1)[0].lower() if ref != 'null' else ''
    return e.text or ''


def items(e: dbx.Element | None) -> int:
    return len(e.children or []) if e is not None else 0


def startup_side(game: Path, got: dict[str, Any]) -> None:
    parts: dict[str, bytes] = {}
    shared: dict[str, dict[str, int]] = {}
    bundles = startup_bundles(game)
    got['startup_bundles'] = len(bundles)
    for i, p in enumerate(bundles):
        names = [e.name for e in fbrb.entries(p) if not e.deleted and e.name.lower().endswith('.dbx')]
        data = fbrb.read(p, names)
        for n, d in data.items():
            parts[n.lower()] = d
        sc = data.get('Menu/shared_config.dbx')
        if sc is not None:
            inst = next(x for x in dbx.parse(sc).instances if (x.attr('type') or '').endswith('UISharedConfigAsset'))
            shared['Dist' if i == 0 else 'Package'] = {k: items(inst.child(k)) for k in (
                'UnlocksData', 'WeaponData', 'UnlocksDataVietnam', 'WeaponDataVietnam')}
    got['shared_config'] = shared
    modes: dict[str, dict[str, str]] = {}
    descs: collections.Counter[tuple[str, str, str]] = collections.Counter()
    mm_maps: collections.Counter[tuple[str, str]] = collections.Counter()
    playlists: list[str] = []
    filters: dict[str, list[str]] = {}
    for n, d in sorted(parts.items()):
        f = dbx.parse(d)
        for x in f.instances:
            t = (x.attr('type') or '').split('.')[-1]
            if t == 'GameModeData':
                modes[n.removeprefix('gamemodes/').removesuffix('.dbx')] = {k: text(x.child(k)) for k in FIELDS}
            elif t == 'LevelDescriptionAsset':
                de = x.child('Description')
                gm = text(de.child('GameMode')) if de is not None else ''
                descs[(gm.removeprefix('gamemodes/'), text(de.child('MatchmakingModID') if de else None),
                       text(de.child('Licenses') if de else None))] += 1
            elif t == 'MatchMakingMap':
                mm_maps[(text(x.child('GameMode')), text(x.child('Licenses')))] += 1
            elif t == 'MatchMakingPlaylist':
                playlists.append(text(x.child('Name')))
            elif t == 'MatchMakingFilter':
                pv = x.child('PossibleValues')
                filters[text(x.child('Rule'))] = [c.text or '' for c in (pv.children or [])] if pv is not None else []
    got['modes'] = modes
    got['mode_ids'] = sorted({m['ModeID'] for m in modes.values()})
    got['query_ids'] = sorted({m['MatchmakingQueryID'] for m in modes.values()})
    got['descriptions'] = sum(descs.values())
    got['descriptions_by'] = {'/'.join(k): v for k, v in sorted(descs.items())}
    got['content_sets'] = sorted({k[1] for k in descs})
    got['licences'] = sorted({k[2] for k in descs})
    got['mm_maps'] = sum(mm_maps.values())
    got['mm_map_modes'] = sorted({k[0] for k in mm_maps})
    got['playlists'] = sorted(collections.Counter(playlists).items())
    got['gamemode_filter'] = filters.get('gamemode', [])


def placement_side(game: Path, got: dict[str, Any]) -> None:
    """Which bundles hold a live GameModes partition, and the deletion markers the level bundles carry."""
    holders: collections.Counter[str] = collections.Counter()
    deleted: collections.Counter[str] = collections.Counter()
    menus = [game.parent / 'original' / 'Package' / 'menu' / 'main' / 'mainmenu-00.fbrb']
    for p in sorted(game.glob('**/*.fbrb')) + [m for m in menus if m.is_file()]:
        if not stock(p):
            continue
        rel = p.relative_to(game.parent).as_posix().lower()
        place = ('startup' if '/async/' in rel else 'common level' if '_common/' in rel else
                 'main menu' if '/mainmenu-' in rel else 'eula' if '/eula_' in rel else 'level' if '/levels/' in rel
                 else 'other')
        for e in fbrb.entries(p):
            if e.name.lower().startswith('gamemodes/'):
                if e.deleted:
                    deleted[e.name.lower().removeprefix('gamemodes/').removesuffix('.dbx')] += 1
                else:
                    holders[place] += 1
    got['mode_partitions_by_place'] = dict(sorted(holders.items()))
    got['deletion_markers'] = sum(deleted.values())
    got['deletion_markers_by_mode'] = dict(sorted(deleted.items()))


class Exe:
    def __init__(self, path: Path) -> None:
        b = self.b = path.read_bytes()
        e = struct.unpack_from('<I', b, 0x3C)[0]
        self.stamp = struct.unpack_from('<I', b, e + 8)[0]
        nsec = struct.unpack_from('<H', b, e + 6)[0]
        opt = e + 24
        self.base = struct.unpack_from('<I', b, opt + 28)[0]
        sh = opt + struct.unpack_from('<H', b, e + 20)[0]
        self.secs = []
        for i in range(nsec):
            s = sh + 40 * i
            vs, va, rs, ra = struct.unpack_from('<IIII', b, s + 8)
            fl = struct.unpack_from('<I', b, s + 36)[0]
            self.secs.append((self.base + va, min(vs, rs), ra, bool(fl & 0x20000000)))

    def va(self, off: int) -> int | None:
        for sva, size, ra, _code in self.secs:
            if ra <= off < ra + size:
                return sva + off - ra
        return None

    def code(self) -> list[tuple[int, int]]:
        return [(ra, ra + size) for _sva, size, ra, code in self.secs if code]

    def string(self, s: str) -> int | None:
        """The address of the NUL-terminated string s as a whole (after a NUL, else the first at a 4-byte aligned
        address: the compiler aligns its literals, and "squadrush" follows a double constant), or None."""
        needle, i, aligned = s.encode() + b'\0', 0, None
        while True:
            i = self.b.find(needle, i + 1)
            if i < 0:
                return aligned
            va = self.va(i)
            if va is None:
                continue
            if self.b[i - 1] == 0:
                return va
            if aligned is None and va % 4 == 0:
                aligned = va


def exe_side(game: Path, got: dict[str, Any]) -> None:
    exe = game / 'BFBC2Game.exe'
    if not exe.is_file():
        return
    x = Exe(exe)
    got['build'] = x.stamp
    if x.stamp != BUILD_STAMP:
        return
    import capstone  # noqa: PLC0415 - only the exe side needs it
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)

    def decode(off: int) -> tuple[str, str, int] | None:
        for _a, size, m, o in md.disasm_lite(x.b[off:off + 16], x.va(off) or 0, 1):
            return m, o, size
        return None

    def sites(disp: int) -> dict[int, str]:
        """Every instruction whose memory displacement is disp: {address: how it uses the field}."""
        out: dict[int, str] = {}
        needle, tag = struct.pack('<I', disp), f'{disp:#x}]'
        for lo, hi in x.code():
            i = x.b.find(needle, lo, hi)
            while i >= 0:
                for back in (2, 3, 1):
                    d = decode(i - back)
                    if d and tag in d[1] and d[2] >= back + 4:
                        m, o, _ = d
                        op = ('store' if m == 'mov' and o.startswith('dword ptr [') else
                              'cmp 1' if m == 'cmp' and o.endswith(', 1') else
                              'cmp reg' if m == 'cmp' else 'load')
                        out[x.va(i - back) or 0] = op
                        break
                i = x.b.find(needle, i + 1, hi)
        return out

    mod, req = sites(GAME_MOD), sites(REQUESTED)
    got['game_mod_sites'] = len(mod)
    got['game_mod_uses'] = dict(sorted(collections.Counter(mod.values()).items()))
    got['game_mod_addresses'] = sorted(mod)
    got['requested_sites'] = len(req)
    got['requested_uses'] = dict(sorted(collections.Counter(req.values()).items()))
    lits: dict[str, int] = {}
    for group, s in LITERALS:
        va = x.string(s)
        n = 0
        if va is not None:
            imm = struct.pack('<I', va)
            for lo, hi in x.code():
                for op in (b'\x68', b'\xb8', b'\xbe', b'\xbf'):   # push imm32, mov eax / esi / edi, imm32
                    i = x.b.find(op + imm, lo, hi)
                    while i >= 0:
                        d = decode(i)
                        if d and d[2] == 5 and d[0] in ('push', 'mov'):
                            n += 1
                        i = x.b.find(op + imm, i + 1, hi)
        lits[f'{group}:{s}'] = n
    got['literal_loads'] = lits
    got['absent_strings'] = sorted(s for s in ABSENT if x.string(s) is None)


def analyse(game: Path) -> dict[str, Any]:
    got: dict[str, Any] = {}
    startup_side(game, got)
    placement_side(game, got)
    exe_side(game, got)
    return got


# The 2026-11-27 run (game copy of Project OPFOR BFBC2, PC build 795745); spec/engine/gamemodes.md cites these.
EXPECTED: dict[str, Any] = {
    'startup_bundles': 2,
    'mode_ids': ['conquest', 'goldrush', 'singleplayer', 'squadrush', 'tdm'],
    'query_ids': ['', 'CONQUEST', 'RUSH', 'SQDM', 'SQRUSH'],
    'descriptions': 74, 'content_sets': ['BC2', 'VIETNAM'], 'licences': ['', 'ciab', 'nam', 'sqr'],
    'shared_config': {'Dist': {'UnlocksData': 2, 'WeaponData': 134, 'UnlocksDataVietnam': 0, 'WeaponDataVietnam': 0},
                      'Package': {'UnlocksData': 2, 'WeaponData': 131, 'UnlocksDataVietnam': 2,
                                  'WeaponDataVietnam': 26}},
    'mm_maps': 65, 'mm_map_modes': ['CONQUEST', 'ONSLAUGHT', 'RUSH', 'SQDM', 'SQRUSH'],
    'playlists': [('CONQUEST', 2), ('ONSLAUGHT', 1), ('RUSH', 2), ('SQDM', 2), ('SQRUSH', 2)],
    'gamemode_filter': ['RUSH', 'CONQUEST', 'SQDM', 'SQRUSH', 'ONSLAUGHT'],
    'mode_partitions_by_place': {'common level': 9, 'eula': 80, 'level': 15, 'main menu': 10, 'startup': 6},
    'deletion_markers': 168,
    'game_mod_sites': 33, 'game_mod_uses': {'cmp 1': 20, 'cmp reg': 1, 'load': 7, 'store': 5},
    'requested_sites': 8, 'requested_uses': {'cmp reg': 1, 'load': 1, 'store': 6},
    'literal_loads': {'mode:conquest': 9, 'mode:goldrush': 2, 'mode:squadrush': 3, 'mode:tdm': 7,
                      'mode:singleplayer': 5, 'query:CONQUEST': 3, 'query:RUSH': 3, 'query:SQDM': 3,
                      'query:SQRUSH': 3, 'tag:BC2': 8, 'tag:VIETNAM': 6, 'tag:gameMod': 5, 'tag:nam': 1,
                      'persistence:Persistence/MP_Template': 9, 'persistence:Persistence/SP_Template': 8},
    'absent_strings': sorted(ABSENT),
}

# The rules each recorded answer backs.
RULES: dict[str, str] = {
    'startup_bundles': 'GMD-DATA-1', 'mode_ids': 'GMD-DATA-1', 'query_ids': 'GMD-DATA-1',
    'descriptions': 'GMD-DATA-3', 'content_sets': 'GMD-DATA-3, ERA-MOD-2', 'licences': 'GMD-DATA-3, ERA-LIC-1',
    'shared_config': 'ERA-RES-2', 'mm_maps': 'GMD-MM-1', 'mm_map_modes': 'GMD-MM-1', 'playlists': 'GMD-MM-1',
    'gamemode_filter': 'GMD-MM-1', 'mode_partitions_by_place': 'GMD-DATA-2', 'deletion_markers': 'GMD-DATA-2',
    'game_mod_sites': 'ERA-MOD-1', 'game_mod_uses': 'ERA-MOD-1, ERA-MOD-3', 'requested_sites': 'ERA-MOD-1',
    'requested_uses': 'ERA-MOD-1', 'literal_loads': 'GMD-SW-1, GMD-SW-3, GMD-PERS-1, ERA-MOD-3',
    'absent_strings': 'ERA-MOD-3, GMD-PERS-1',
}


def checks(game: Path) -> list[tuple[bool, str]]:
    """(ok, description) per stable answer the rules cite; each description names its rules."""
    got = analyse(game)
    return [(got.get(k) == want, f'{RULES.get(k, "gamemodes")}: {k} = {want!r}'
             + ('' if got.get(k) == want else f' (got {got.get(k)!r})')) for k, want in EXPECTED.items()]


def report(got: dict[str, Any]) -> None:
    print(f'startup bundles: {got.get("startup_bundles")}')
    print(f'GameModeData partitions: {len(got.get("modes", {}))}')
    for name, m in sorted(got.get('modes', {}).items()):
        print(f'  {name}: ' + ', '.join(f'{k}={v}' for k, v in m.items() if k != 'Description'))
    print(f'level descriptions: {got.get("descriptions")} (mode/content set/licence: count)')
    for k, v in got.get('descriptions_by', {}).items():
        print(f'  {k}: {v}')
    print(f'shared_config per-era fields (items): {got.get("shared_config")}')
    print(f'matchmaking maps {got.get("mm_maps")}, their modes {got.get("mm_map_modes")}; playlists '
          f'{got.get("playlists")}; gamemode filter {got.get("gamemode_filter")}')
    print(f'live GameModes partitions by bundle place: {got.get("mode_partitions_by_place")}')
    print(f'deletion markers: {got.get("deletion_markers")} {got.get("deletion_markers_by_mode")}')
    if 'game_mod_sites' in got:
        print(f'exe: +0xC8C8C {got["game_mod_sites"]} instructions {got["game_mod_uses"]}; '
              f'+0xC8C90 {got["requested_sites"]} {got["requested_uses"]}')
        print('  ' + ' '.join(f'{a:#x}' for a in got['game_mod_addresses']))
        print(f'exe literal loads: {got["literal_loads"]}')
        print(f'exe strings absent: {got["absent_strings"]}')
    elif 'build' in got:
        print(f'exe: build stamp {got["build"]:#x} is not 795745\'s; exe side skipped')
    print('checks:')
    for k, want in EXPECTED.items():
        print(f'  {"ok  " if got.get(k) == want else "DIFF"} {k}')


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    report(analyse(game))


if __name__ == '__main__':
    main()
