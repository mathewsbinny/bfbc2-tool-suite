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
persistence-template strings. Since round GM1 (spec/engine/eras.md) also a digest of each instruction range round GM2
patches for native eras 2 and 3 and of each Vietnam ownership check no patch may touch, that each range ends on an
instruction boundary, and that no patch range overlaps a kept one. Since round GM4 (spec/engine/eras_n.md) also the
further ranges an N-era table needs (the setter's whole label branch, the offline list's tag builder, the menu
tracker's page bound, writer and four page tests, the launcher's `expansion` test) and the tracker's page count.

Prints identifiers and counts only (mode ids, field names, addresses; no text, no bytes). `checks(game)` recomputes
the stable answers the rules cite for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
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
# Round GM1 (spec/engine/eras.md): the instruction ranges round GM2 patches in memory for native eras 2 and 3
# (ERA-SITE-1), as (name, address, length); each range starts and ends on an instruction boundary.
PATCH_SITES = (
    ('parser', 0x8FF330, 0x31), ('setter.entry', 0x75FCB0, 8), ('setter.gate', 0x75FCC0, 15),
    ('setter.label_test', 0x75FDB2, 5), ('setter.label_push', 0x75FDB7, 10),
    ('menucfg.test', 0x75AEA7, 3), ('menucfg.branch', 0x75AEB0, 2), ('menucfg.name', 0x75AEB2, 5),
    ('hudcfg.test', 0x7584A8, 3), ('hudcfg.branch', 0x7584AB, 2), ('hudcfg.name', 0x7584AD, 5),
    ('tips.hud', 0x751BC2, 19), ('tips.loading', 0x7776EA, 19), ('icons', 0x775D16, 22),
    ('unlocks.test', 0x93D14F, 7), ('unlocks.branch', 0x93D198, 2), ('unlocks.slot', 0x93D19A, 10),
    ('grid.unlockGrid.test', 0x9469D5, 7), ('grid.unlockGrid.branch', 0x9469DC, 2),
    ('grid.unlockGrid.slot', 0x9469E3, 6), ('grid.kitRankData.test', 0x946A75, 7),
    ('grid.kitRankData.branch', 0x946A7C, 2), ('grid.kitRankData.slot', 0x946A83, 6),
    ('grid.unlockGridChildren.test', 0x946B35, 7), ('grid.unlockGridChildren.branch', 0x946B3C, 2),
    ('grid.unlockGridChildren.slot', 0x946B43, 6),
    ('weapons.a.test', 0x93C4E1, 7), ('weapons.a.branch', 0x93C4E8, 2), ('weapons.a.map', 0x93C4EF, 6),
    ('weapons.b.test', 0x93C561, 7), ('weapons.b.branch', 0x93C568, 2), ('weapons.b.map', 0x93C56F, 6),
    ('maps.class', 0x942247, 8), ('maps.tag3', 0x942291, 5), ('maps.tag7', 0x9422E0, 5),
    ('search.a.test', 0x77C994, 7), ('search.a.tag7', 0x77C9C0, 5), ('search.a.tag3', 0x77CA63, 5),
    ('search.b.test', 0x77D0C3, 7), ('search.b.tag7', 0x77D0EF, 5), ('search.b.tag3', 0x77D194, 5),
    ('seen.a', 0x920CF8, 7), ('seen.b', 0x920E45, 7), ('seen.c', 0x920F88, 7), ('seen.d', 0x9210D5, 7),
)
# Vietnam's ownership checks no GM2 patch may touch (ERA-LIC-2): the redeem pop-up's test, playerHasLicenseToNam's
# body, the licence-token test and its licence callers, the sign-in request.
KEEP_SITES = (
    ('redeem', 0x75FE32, 5), ('licenseToNam', 0x76FCDE, 28), ('tokenTest', 0x649ED0, 15),
    ('applyLicenses.a', 0x8528E3, 5), ('applyLicenses.b', 0x852904, 5), ('levelLicence', 0x951A83, 5),
    ('weaponLicence.a', 0x6523A7, 5), ('weaponLicence.b', 0x6523C8, 5), ('weaponLicence.c', 0x6524EC, 5),
    ('signIn', 0x920C00, 0x62),
)
# Round GM4 (spec/engine/eras_n.md): the further ranges an N-era table needs (ERA-N-3). 'setter.labels' and
# 'maps.builder' replace GM1's narrower label and tag ranges when the host takes that path (they overlap them).
N_SITES = (
    ('setter.labels', 0x75FDA4, 29), ('maps.builder', 0x942266, 186),
    ('tracker.bound', 0x9573D1, 3), ('tracker.writer', 0x957631, 6),
    ('seen.a.key', 0x920D24, 13), ('seen.b.key', 0x920E77, 13), ('seen.c.key', 0x920FB4, 13),
    ('seen.d.key', 0x921107, 13), ('launcher.expansion', 0x5B5104, 25),
)


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

    def ranges(sites: tuple[tuple[str, int, int], ...]) -> tuple[dict[str, str], list[str]]:
        """{name: 'address+length digest'} (sha256 of the range's bytes, 16 hex digits) and the names whose range
        does not end on an instruction boundary when decoded from its start."""
        digests, ragged = {}, []
        for name, va, n in sites:
            off = next(ra + va - sva for sva, size, ra, _c in x.secs if sva <= va < sva + size)
            raw = x.b[off:off + n]
            digests[name] = f'{va:#x}+{n} {hashlib.sha256(raw).hexdigest()[:16]}'
            ends = [a + size for a, size, _m, _o in md.disasm_lite(raw + b'\x90' * 16, va) if a < va + n]
            if not ends or ends[-1] != va + n:
                ragged.append(name)
        return digests, ragged
    got['patch_sites'], got['patch_ragged'] = ranges(PATCH_SITES)
    got['keep_sites'], got['keep_ragged'] = ranges(KEEP_SITES)
    got['patch_keep_overlap'] = sorted(f'{p}/{k}' for p, pa, pn in PATCH_SITES for k, ka, kn in KEEP_SITES
                                       if pa < ka + kn and ka < pa + pn)
    got['n_sites'], got['n_ragged'] = ranges(N_SITES)
    got['n_keep_overlap'] = sorted(f'{p}/{k}' for p, pa, pn in N_SITES for k, ka, kn in KEEP_SITES
                                   if pa < ka + kn and ka < pa + pn)
    # ERA-N-2: the menu tracker's loader reads pages 0 .. 3 (UIMenuTrackerPage_Count); its writer fills 0, 1, 2
    at = next(ra + 0x9573D1 - sva for sva, size, ra, _c in x.secs if sva <= 0x9573D1 < sva + size)
    bound = x.b[at:at + 3]
    got['tracker_pages'] = bound[2] if bound[:2] == b'\x83\xf8' else None


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
    # round GM1: the bytes GM2 patches (address+length, sha256 prefix) and the Vietnam checks it must not touch
    'patch_sites': {
        'parser': '0x8ff330+49 862f0dbdc29ad7bd',
        'setter.entry': '0x75fcb0+8 85c70c74d899cd59',
        'setter.gate': '0x75fcc0+15 2fed963876a84c2e',
        'setter.label_test': '0x75fdb2+5 42a118f10ece9a2e',
        'setter.label_push': '0x75fdb7+10 08ef9090c6b2a3b3',
        'menucfg.test': '0x75aea7+3 1ea67153973d48b6',
        'menucfg.branch': '0x75aeb0+2 dbdbc8ce6333f81b',
        'menucfg.name': '0x75aeb2+5 545bff5cf582ebbe',
        'hudcfg.test': '0x7584a8+3 1ea67153973d48b6',
        'hudcfg.branch': '0x7584ab+2 dbdbc8ce6333f81b',
        'hudcfg.name': '0x7584ad+5 52b44a99f34563fd',
        'tips.hud': '0x751bc2+19 9df2366494763619',
        'tips.loading': '0x7776ea+19 9df2366494763619',
        'icons': '0x775d16+22 3ede458bfed90b85',
        'unlocks.test': '0x93d14f+7 68ba7558570bc83e',
        'unlocks.branch': '0x93d198+2 1f39a9b3a90c11a3',
        'unlocks.slot': '0x93d19a+10 92cd66fba3fc9ad7',
        'grid.unlockGrid.test': '0x9469d5+7 9f851b27583a1e27',
        'grid.unlockGrid.branch': '0x9469dc+2 9a6e750100c3087d',
        'grid.unlockGrid.slot': '0x9469e3+6 3c654fcf3758fc41',
        'grid.kitRankData.test': '0x946a75+7 9f851b27583a1e27',
        'grid.kitRankData.branch': '0x946a7c+2 9a6e750100c3087d',
        'grid.kitRankData.slot': '0x946a83+6 bbfaef93d6e7d942',
        'grid.unlockGridChildren.test': '0x946b35+7 9f851b27583a1e27',
        'grid.unlockGridChildren.branch': '0x946b3c+2 9a6e750100c3087d',
        'grid.unlockGridChildren.slot': '0x946b43+6 3c654fcf3758fc41',
        'weapons.a.test': '0x93c4e1+7 27dae747ccff1284',
        'weapons.a.branch': '0x93c4e8+2 db283a2c168a0901',
        'weapons.a.map': '0x93c4ef+6 98af35710d2de0be',
        'weapons.b.test': '0x93c561+7 27dae747ccff1284',
        'weapons.b.branch': '0x93c568+2 db283a2c168a0901',
        'weapons.b.map': '0x93c56f+6 96a89393ae74ec1a',
        'maps.class': '0x942247+8 e1fa101434d9e08b',
        'maps.tag3': '0x942291+5 e95018f045d48380',
        'maps.tag7': '0x9422e0+5 e345d4abdac6ae46',
        'search.a.test': '0x77c994+7 282ba277a41a2df5',
        'search.a.tag7': '0x77c9c0+5 e345d4abdac6ae46',
        'search.a.tag3': '0x77ca63+5 e95018f045d48380',
        'search.b.test': '0x77d0c3+7 9f851b27583a1e27',
        'search.b.tag7': '0x77d0ef+5 e345d4abdac6ae46',
        'search.b.tag3': '0x77d194+5 e95018f045d48380',
        'seen.a': '0x920cf8+7 9f851b27583a1e27',
        'seen.b': '0x920e45+7 9f851b27583a1e27',
        'seen.c': '0x920f88+7 9f851b27583a1e27',
        'seen.d': '0x9210d5+7 9f851b27583a1e27',
    },
    'patch_ragged': [],
    'keep_sites': {
        'redeem': '0x75fe32+5 a959f05c75ef8460',
        'licenseToNam': '0x76fcde+28 c26e35a39ccaf857',
        'tokenTest': '0x649ed0+15 ebcebfe3b4ee6e75',
        'applyLicenses.a': '0x8528e3+5 7d0c9c7c05146971',
        'applyLicenses.b': '0x852904+5 7c69ba1d16bc54d8',
        'levelLicence': '0x951a83+5 10f130df83f8df7e',
        'weaponLicence.a': '0x6523a7+5 a12098d105a2afc1',
        'weaponLicence.b': '0x6523c8+5 174c9234a0318b00',
        'weaponLicence.c': '0x6524ec+5 03ef5653fb23104f',
        'signIn': '0x920c00+98 0531fdcab5921bce',
    },
    'keep_ragged': [],
    'patch_keep_overlap': [],
    # round GM4: the N-era table's further ranges, and the menu tracker's page count
    'n_sites': {
        'setter.labels': '0x75fda4+29 f442e82c440b3731',
        'maps.builder': '0x942266+186 247317599241bc72',
        'tracker.bound': '0x9573d1+3 ec9d1a6c95187ab3',
        'tracker.writer': '0x957631+6 6276656afb30851f',
        'seen.a.key': '0x920d24+13 9376c5308ef30473',
        'seen.b.key': '0x920e77+13 4f6a8c3f48b373d8',
        'seen.c.key': '0x920fb4+13 9376c5308ef30473',
        'seen.d.key': '0x921107+13 4f6a8c3f48b373d8',
        'launcher.expansion': '0x5b5104+25 877ee9d2ec587389',
    },
    'n_ragged': [],
    'n_keep_overlap': [],
    'tracker_pages': 4,
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
    'patch_sites': 'ERA-SITE-1', 'patch_ragged': 'ERA-SITE-1', 'keep_sites': 'ERA-LIC-2', 'keep_ragged': 'ERA-LIC-2',
    'patch_keep_overlap': 'ERA-LIC-2',
    'n_sites': 'ERA-N-3', 'n_ragged': 'ERA-N-3', 'n_keep_overlap': 'ERA-N-3, ERA-LIC-2', 'tracker_pages': 'ERA-N-2',
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
        print(f'native-era patch ranges: {len(got["patch_sites"])} (not on an instruction boundary: '
              f'{got["patch_ragged"]}); Vietnam check ranges kept: {len(got["keep_sites"])} (ragged '
              f'{got["keep_ragged"]}); overlaps {got["patch_keep_overlap"]}')
        print(f'N-era ranges: {len(got["n_sites"])} (ragged {got["n_ragged"]}; overlapping a kept range '
              f'{got["n_keep_overlap"]}); menu tracker pages loaded: {got["tracker_pages"]}')
        for name, v in got['n_sites'].items():
            print(f'  {name}: {v}')
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
