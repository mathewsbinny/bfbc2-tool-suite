"""The award data against the value templates the career writes (read-only; spec/engine/persistence.md PERS-AWD-6,
PERS-AWD-7, round SPG2-rest5).

    python spec/probes/persistence.py [--opfor <folder>]

Reads `Persistence/MP_Awards`, `MP_Template`, `SP_Awards` and `SP_Template` from DICE's start-up bundle of Project
OPFOR BFBC2's game copy (`Dist/win32/async/startup-00.fbrb`, dated before 2013): per award set the awards, their
`Code`s and `Repeat`s, the criteria's `CriteriaType`s, and whether `<Code>_00` and `<Code>_01` (the two stats a grant
writes: PERS-AWD-5) are among the template's `Final` value templates. From `game/BFBC2Game.exe` (PC build 795745 only,
data read, no code run) it reads the grant's two name formats.

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


def analyse(game: Path) -> dict[str, Any]:
    out: dict[str, Any] = {}
    b = game.joinpath(*BUNDLE)
    out['dice_bundle'] = time.gmtime(b.stat().st_mtime).tm_year < 2013
    got = fbrb.read(b, [n for _k, aw, tp in SETS for n in (aw, tp)])
    for k, aw, tp in SETS:
        out[k] = award_set(got[aw], got[tp])
    exe = (game / 'BFBC2Game.exe').read_bytes()
    pe = struct.unpack_from('<I', exe, 0x3C)[0]
    if struct.unpack_from('<I', exe, pe + 8)[0] == BUILD_STAMP:
        out['grant_formats'] = all(exe[va - 0x400000:va - 0x400000 + len(s)] == s for va, s in FORMATS)
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
}
RULES = {'dice_bundle': 'PERS-AWD-7', 'mp': 'PERS-AWD-6, PERS-AWD-7', 'sp': 'PERS-AWD-6, PERS-AWD-7',
         'grant_formats': 'PERS-AWD-5, PERS-AWD-7'}


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
