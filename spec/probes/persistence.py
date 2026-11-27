"""The award data against the value templates the career writes (read-only; spec/engine/persistence.md PERS-AWD-6 -
PERS-AWD-8, rounds SPG2-rest5 / rest6).

    python spec/probes/persistence.py [--opfor <folder>]

Reads `Persistence/MP_Awards`, `MP_Template`, `SP_Awards` and `SP_Template` from DICE's start-up bundle of Project
OPFOR BFBC2's game copy (`Dist/win32/async/startup-00.fbrb`, dated before 2013): per award set the awards, their
`Code`s and `Repeat`s, the criteria's `CriteriaType`s, and whether `<Code>_00` and `<Code>_01` (the two stats a grant
writes: PERS-AWD-5) are among the template's `Final` value templates. From `game/BFBC2Game.exe` (PC build 795745 only,
data read, no code run) it reads the grant's two name formats, `StatEvent`'s values and the kill handler's event
numbers (PERS-AWD-6), and the criterion factory's type tests and classes (PERS-AWD-8, round SPG2-rest6).

Prints counts and identifiers only (award codes are counted, not listed). `checks(game)` recomputes the stable answers
the rules cite for core/tests/test_spec_probes.py.
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
if str(SUITE / 'core') not in sys.path:
    sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths  # noqa: E402

BUNDLE = ('Dist', 'win32', 'async', 'startup-00.fbrb')
SETS = (('mp', 'persistence/mp_awards.dbx', 'persistence/mp_template.dbx'),
        ('sp', 'persistence/sp_awards.dbx', 'persistence/sp_template.dbx'))
# PERS-AWD-5: the grant 0x852CB0 formats the award's Code with "%s_00" (0x141D918) and "%s_01" (0x14333AC).
BUILD_STAMP = 0x4E55445D
FORMATS = ((0x141D918, b'%s_00\x00'), (0x14333AC, b'%s_01\x00'))
# PERS-AWD-6: StatEvent's type record (its name, +0x18 its value table of 24-byte entries: name at +0, value at +16)
# and the kill handler 0x86DC90's event pushes before its calls of 0x8584D0 (`push imm8`).
STAT_EVENT_RECORD = 0x1C03348
KILL_EVENT_PUSHES = (0x86DF3E, 0x86DF6E, 0x86DFA5, 0x86DFD9, 0x86E07C, 0x86E0D5, 0x86E135, 0x86E18E, 0x86E2D2)
# PERS-AWD-8: the criterion factory 0x8430E0's type tests and the seven `mov dword [eax], vtable` stores of the
# criterion classes (in the factory and in the six constructors); the ownerless met slot 0x83BE70 (`xor al, al; ret`).
FACTORY_TESTS = ((0x8430E5, b'\x83\xfe\x02'), (0x843163, b'\x85\xf6'), (0x8431BD, b'\x83\xfe\x03'),
                 (0x83BE70, b'\x32\xc0\xc3'))
CRITERION_VTABLES = ((0x843142, 0x1433058), (0x83BE59, 0x1433250), (0x83BECF, 0x1433274), (0x83BF39, 0x1433298),
                     (0x83C09F, 0x1433304), (0x83BFAF, 0x14332BC), (0x83C029, 0x14332E0))


def kind(i: dbx.Element) -> str:
    return (i.attr('type') or '').rsplit('.', 1)[-1]


def text(e: dbx.Element | None) -> str:
    return '' if e is None or e.text is None else e.text


def award_set(awards: bytes, template: bytes) -> dict[str, Any]:
    a = dbx.parse(awards)
    t = dbx.parse(template)
    aw = [i for i in a.instances if kind(i) == 'AwardData']
    codes = {text(i.child('Code')) for i in aw} - {''}
    by = {(i.attr('guid') or '').lower(): i for i in t.instances}
    pd = next(i for i in t.instances if kind(i) == 'PersistenceData')
    final = pd.child('Final')
    names = {text(by[(c.attr('ref') or '').lower()].child('Name')) for c in (final.children or [] if final is not None
                                                                          else []) if (c.attr('ref') or '').lower() in by}
    return {
        'awards': len(aw), 'without_code': sum(not text(i.child('Code')) for i in aw), 'codes': len(codes),
        'repeat': dict(sorted(collections.Counter(text(i.child('Repeat')) for i in aw).items())),
        'criteria': dict(sorted(collections.Counter(text(i.child('CriteriaType')) for i in a.instances
                                                    if kind(i) == 'CriteriaData').items())),
        'final': len(names),
        'code_00_in_final': sum(c + '_00' in names for c in codes),
        'code_01_in_final': sum(c + '_01' in names for c in codes),
    }


class Image:
    """The executable's bytes by virtual address, through its section table (data read, no code run)."""

    def __init__(self, exe: bytes) -> None:
        self.b = exe
        pe = struct.unpack_from('<I', exe, 0x3C)[0]
        self.stamp = struct.unpack_from('<I', exe, pe + 8)[0]
        n, opt = struct.unpack_from('<H', exe, pe + 6)[0], struct.unpack_from('<H', exe, pe + 20)[0]
        self.base = struct.unpack_from('<I', exe, pe + 52)[0]
        self.secs = [struct.unpack_from('<IIII', exe, pe + 24 + opt + 40 * k + 8) for k in range(n)]

    def at(self, va: int, size: int) -> bytes:
        rva = va - self.base
        for vsize, start, raw_size, raw in self.secs:
            if start <= rva < start + max(vsize, raw_size) and rva - start + size <= raw_size:
                return self.b[raw + rva - start:raw + rva - start + size]
        return b''

    def u32(self, va: int) -> int:
        w = self.at(va, 4)
        return struct.unpack('<I', w)[0] if len(w) == 4 else 0

    def cstr(self, va: int) -> str:
        w = self.at(va, 80)
        return w.split(b'\0', 1)[0].decode('ascii', 'replace') if w else ''


def stat_events(img: Image) -> dict[str, Any]:
    """PERS-AWD-6: StatEvent's values from its type record, and the kill handler's pushed event numbers."""
    names, table = {}, img.u32(STAT_EVENT_RECORD + 0x18)
    for k in range(512):
        name = img.cstr(img.u32(table + 24 * k))
        if not name.startswith('StatEvent_'):
            break
        names[img.u32(table + 24 * k + 16)] = name
    pushes = [img.at(va, 2) for va in KILL_EVENT_PUSHES]
    return {'enum': img.cstr(img.u32(STAT_EVENT_RECORD)), 'values': len(names),
            'death': names.get(2), 'event_0x51': names.get(0x51),
            'kill_handler_events': [p[1] if p[:1] == b'\x6a' else None for p in pushes]}


def criterion_factory(img: Image) -> bool:
    """PERS-AWD-8: the factory's type tests and the criterion classes' vtable stores, as recorded."""
    return (all(img.at(va, len(b)) == b for va, b in FACTORY_TESTS)
            and all(img.at(va, 6) == b'\xc7\x00' + struct.pack('<I', vt) for va, vt in CRITERION_VTABLES))


def analyse(game: Path) -> dict[str, Any]:
    out: dict[str, Any] = {}
    b = game.joinpath(*BUNDLE)
    out['dice_bundle'] = time.gmtime(b.stat().st_mtime).tm_year < 2013
    got = fbrb.read(b, [n for _k, aw, tp in SETS for n in (aw, tp)])
    for k, aw, tp in SETS:
        out[k] = award_set(got[aw], got[tp])
    img = Image((game / 'BFBC2Game.exe').read_bytes())
    if img.stamp == BUILD_STAMP:
        out['grant_formats'] = all(img.at(va, len(s)) == s for va, s in FORMATS)
        out['stat_events'] = stat_events(img)
        out['criterion_factory'] = criterion_factory(img)
    return out


EXPECTED: dict[str, Any] = {
    'dice_bundle': True,
    # PERS-AWD-6 (the criteria's types), PERS-AWD-7 (the grant's stats are Final value templates)
    'mp': {'awards': 384, 'without_code': 4, 'codes': 380,
           'repeat': {'AwardType_Limited': 61, 'AwardType_OnceGlobally': 278, 'AwardType_Unlimited': 45},
           'criteria': {'CriteriaType_GLOBAL_AllTimeTotal': 315, 'CriteriaType_IAR_InARound': 47,
                        'CriteriaType_IAS_InASpawn': 6},
           'final': 1756, 'code_00_in_final': 380, 'code_01_in_final': 380},
    'sp': {'awards': 73, 'without_code': 3, 'codes': 70,
           'repeat': {'AwardType_OnceGlobally': 72, 'AwardType_Unlimited': 1},
           'criteria': {'CriteriaType_GLOBAL_AllTimeTotal': 30, 'CriteriaType_IAR_InARound': 40},
           'final': 317, 'code_00_in_final': 70, 'code_01_in_final': 70},
    'grant_formats': True,
    # PERS-AWD-6 (round SPG2-rest6): the handler's event numbers are StatEvent's; 0x51 is the suicide
    'stat_events': {'enum': 'StatEvent', 'values': 105, 'death': 'StatEvent_DeathAs_X_By_Y',
                    'event_0x51': 'StatEvent_Suicide',
                    'kill_handler_events': [0, 0xB, 2, 0xD, 1, 0xC, 3, 0xE, 0x51]},
    # PERS-AWD-8: the criterion factory and its seven classes
    'criterion_factory': True,
}
RULES = {'dice_bundle': 'PERS-AWD-7', 'mp': 'PERS-AWD-6, PERS-AWD-7', 'sp': 'PERS-AWD-6, PERS-AWD-7',
         'grant_formats': 'PERS-AWD-5, PERS-AWD-7', 'stat_events': 'PERS-AWD-6', 'criterion_factory': 'PERS-AWD-8'}


def checks(game: Path) -> list[tuple[bool, str]]:
    """(ok, description) per stable answer the rules of spec/engine/persistence.md cite."""
    got = analyse(game)
    return [(got.get(k) == want, f'{RULES[k]}: {k} = {want!r}' + ('' if got.get(k) == want else f' (got {got.get(k)!r})'))
            for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=(__doc__ or '').split('\n')[0])
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    t = time.perf_counter()
    got = analyse(game)
    for k, v in got.items():
        print(f'{k}: {v}')
    bad = [k for k, want in EXPECTED.items() if got.get(k) != want]
    print(f'checks: {len(EXPECTED) - len(bad)} of {len(EXPECTED)} as recorded' + (f'; differ: {bad}' if bad else ''))
    print(f'({time.perf_counter() - t:.1f} s)')


if __name__ == '__main__':
    main()
