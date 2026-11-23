"""The mode entities' events and counters as DICE's data names them, against the executable's event hashes (read-only;
spec/engine/bot_ai.md BOT-GS-4, BOT-GS-6 - BOT-GS-9, round SPG2-rest3).

    python spec/probes/modes.py [--opfor <folder>]

The server's mode entities (ticket counter, life counter, mission objective, the base entity's `Enable` / `Disable`)
compare an event's name hash (h = 5381; h = h * 33 ^ c per byte, BOT-GS-3) with constants in their code. This probe
reads every partition of DICE's level bundles in Project OPFOR BFBC2's game copy (`game/**/levels/*/*.fbrb` dated
before 2013, each partition once): it looks for the event names the rules give in each partition's strings, and
parses the partitions that hold a `TicketCounterEntityData`, `LifeCounterEntityData`, `MissionObjectiveEntityData`
or `KillCounterEntityData` for the names of the event connections that start or end at one, and the life counters'
fields. From `game/BFBC2Game.exe` (PC build 795745 only, data read, no code run) it reads
each function's bytes and tests that the 32-bit hash of each name the rules give is there.

Prints structure, counts and identifiers only (event and field names, numeric ranges; no text, no bytes).
`checks(game)` recomputes the stable answers the rules cite for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import os
import struct
import sys
import time
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
if str(SUITE) not in sys.path:
    sys.path.insert(0, str(SUITE))
import suite_paths  # noqa: E402

suite_paths.use('core', 'menu-tools')
import menucommon  # noqa: E402
from bfbc2core import dbx, fbrb, paths  # noqa: E402

BUILD_STAMP = 0x4E55445D                     # BFBC2Game.exe build 795745
COUNTERS = ('TicketCounterEntityData', 'LifeCounterEntityData', 'MissionObjectiveEntityData', 'KillCounterEntityData')

# (rule, function start, end, event name): the name's hash is a constant inside [start, end) of build 795745.
SITES: tuple[tuple[str, int, int, str], ...] = (
    # ServerMissionObjectiveEntity: the constructor 0x66E190 subscribes the four, slot 6 0x674970 tests two
    ('BOT-GS-4', 0x66E190, 0x66E245, 'OnShowObjective'), ('BOT-GS-4', 0x66E190, 0x66E245, 'OnSuccess'),
    ('BOT-GS-4', 0x66E190, 0x66E245, 'Success'), ('BOT-GS-4', 0x66E190, 0x66E245, 'ShowObjective'),
    # the base entity's event handler 0xFFE580: Enable -> vtable +0x7C, Disable -> +0x80
    ('BOT-GS-4', 0xFFE580, 0xFFE5DD, 'Enable'), ('BOT-GS-4', 0xFFE580, 0xFFE5DD, 'Disable'),
    # ServerTicketCounterEntity: reach zero 0x6805B0, team wins 0x67E3F0
    ('BOT-GS-6', 0x6805B0, 0x68061D, 'OnReachZero'), ('BOT-GS-6', 0x67E3F0, 0x67E5FF, 'OnTeamWins'),
    # ServerLifeCounterEntity: the input events 0x6897D0, the life loss 0x689200
    ('BOT-GS-8', 0x6897D0, 0x689AF0, 'Winner'), ('BOT-GS-8', 0x6897D0, 0x689AF0, 'DestructBase'),
    ('BOT-GS-8', 0x6897D0, 0x689AF0, 'IncreaseReinforce'), ('BOT-GS-8', 0x6897D0, 0x689AF0, 'DecreaseLife'),
    ('BOT-GS-8', 0x689200, 0x689311, 'OnTeamLoss'), ('BOT-GS-8', 0x689200, 0x689311, 'OnFightHarder'),
)


def name_hash(s: str) -> int:
    h = 5381
    for c in s.encode('latin-1'):
        h = ((h * 33) ^ c) & 0xFFFFFFFF
    return h


def stock_bundles(game: Path) -> list[Path]:
    return sorted((p for p in game.glob('**/levels/*/*.fbrb') if time.gmtime(p.stat().st_mtime).tm_year < 2013),
                  key=lambda p: str(p).lower())


def kind(e: dbx.Element) -> str:
    return (e.attr('type') or '').split('.')[-1]


def local(ref: str | None) -> str:
    return (ref or 'null').rsplit('/', 1)[-1].lower()


def walk(e: dbx.Element) -> list[dbx.Element]:
    out = [e]
    for c in e.children or ():
        out.extend(walk(c))
    return out


def number(e: dbx.Element | None) -> float | None:
    if e is None:
        return None
    try:
        return float(dbx.value(e))
    except ValueError:
        return None


class Stats:
    def __init__(self) -> None:
        self.partitions = 0
        self.parsed = 0
        self.events: collections.Counter[tuple[str, str, str]] = collections.Counter()   # (type, in/out, name)
        self.names: set[str] = set()
        self.life: list[dict[str, Any]] = []
        self.exe: dict[str, Any] = {}


def data_side(game: Path, st: Stats) -> None:
    """Every DICE partition once: the site names looked for in its bytes (a binary DBX keeps its strings whole), and
    the partitions that hold a counter type parsed for their event connections and life counters."""
    seen: set[str] = set()
    wanted = {n: n.encode('latin-1') + b'\x00' for _r, _lo, _hi, n in SITES}
    types = [t.encode('latin-1') for t in COUNTERS]
    for b in stock_bundles(game):
        names = [e.name for e in fbrb.entries(b) if not e.deleted and e.name.lower().endswith('.dbx')
                 and e.name.lower() not in seen]
        for k in range(0, len(names), 500):
            got = fbrb.read(b, names[k:k + 500])
            for n, raw in got.items():
                seen.add(n.lower())
                st.partitions += 1
                st.names.update(x for x, w in wanted.items() if w in raw)
                if any(t in raw for t in types):
                    events(raw, st)


def events(raw: bytes, st: Stats) -> None:
    try:
        f = dbx.parse(raw)
    except ValueError:
        return
    st.parsed += 1
    types = {(i.attr('guid') or '').lower(): kind(i) for i in f.instances}
    for i in f.instances:
        if kind(i) == 'LifeCounterEntityData':
            st.life.append({k: (dbx.value(c) if (c := i.child(k)) is not None else None)
                            for k in ('Attacker', 'StartingLifeCount', 'BaseCount', 'ReinforceThreshold',
                                      'FightHarderTreshold', 'TeamId')})
        for e in walk(i):
            src, dst = e.child('Source'), e.child('Target')
            sev, tev = e.child('SourceEvent'), e.child('TargetEvent')
            if src is None or dst is None or sev is None or tev is None:
                continue
            sn, tn = sev.child('Name'), tev.child('Name')
            s_type, t_type = types.get(local(src.attr('ref')), ''), types.get(local(dst.attr('ref')), '')
            if s_type in COUNTERS:
                st.events[(s_type, 'out', dbx.value(sn) if sn is not None else '')] += 1
            if t_type in COUNTERS:
                st.events[(t_type, 'in', dbx.value(tn) if tn is not None else '')] += 1


def exe_side(game: Path, st: Stats) -> None:
    exe = game / 'BFBC2Game.exe'
    if not exe.is_file():
        return
    img = menucommon.PEImage(os.fspath(exe))
    st.exe['build'] = img.timestamp
    if img.timestamp != BUILD_STAMP:
        return
    found: dict[str, bool] = {}
    for _rule, lo, hi, name in SITES:
        o = img.off(lo)
        assert o is not None
        found[name] = struct.pack('<I', name_hash(name)) in img.b[o:o + hi - lo]
    st.exe['found'] = found


def analyse(game: Path) -> Stats:
    st = Stats()
    data_side(game, st)
    exe_side(game, st)
    return st


def summary(st: Stats) -> dict[str, Any]:
    life = st.life
    attackers = [x for x in life if x['Attacker'] == 'true']
    defenders = [x for x in life if x['Attacker'] != 'true']

    def rng(rows: list[dict[str, Any]], k: str) -> tuple[float, float] | None:
        vals = [v for x in rows if (v := number_text(x[k])) is not None]
        return (min(vals), max(vals)) if vals else None

    found = st.exe.get('found')
    return {
        'site_names_not_in_data': sorted(n for _r, _lo, _hi, n in SITES if n not in st.names),
        'site_hashes_not_in_exe': sorted(n for n, ok in found.items() if not ok) if found is not None else None,
        'life_counter_inputs': sorted({n for (t, d, n) in st.events if t == 'LifeCounterEntityData' and d == 'in'}),
        'life_counter_outputs': sorted({n for (t, d, n) in st.events if t == 'LifeCounterEntityData' and d == 'out'}),
        'ticket_counter_inputs': sorted({n for (t, d, n) in st.events if t == 'TicketCounterEntityData' and d == 'in'}),
        'ticket_counter_outputs': sorted({n for (t, d, n) in st.events
                                          if t == 'TicketCounterEntityData' and d == 'out'}),
        'life_counters': (len(attackers), len(defenders)),
        'attacker_lives': rng(attackers, 'StartingLifeCount'),
        'defender_bases': rng(defenders, 'BaseCount'),
    }


def number_text(v: str | None) -> float | None:
    try:
        return float(v) if v is not None else None
    except ValueError:
        return None


# The 2026-11-22 run (game copy of Project OPFOR BFBC2, PC build 795745); spec/engine/bot_ai.md cites these.
EXPECTED: dict[str, Any] = {
    # DecreaseLife and OnReachZero are named from the game copy's other data (BF1943's and Onslaught's graphs)
    'site_names_not_in_data': ['DecreaseLife', 'OnReachZero'], 'site_hashes_not_in_exe': [],
    'life_counter_inputs': ['DestructBase', 'IncreaseReinforce', 'Winner'],
    'life_counter_outputs': ['OnFightHarder', 'OnTeamLoss'],
    'ticket_counter_inputs': [], 'ticket_counter_outputs': ['OnReachTicketPercentage', 'OnTeamWins'],
    'life_counters': (30, 30), 'attacker_lives': (20.0, 100.0), 'defender_bases': (2.0, 5.0),
}

# The rules each recorded answer backs.
RULES: dict[str, str] = {
    'site_names_not_in_data': 'BOT-GS-4, BOT-GS-6, BOT-GS-8', 'site_hashes_not_in_exe': 'BOT-GS-4, BOT-GS-6, BOT-GS-8',
    'life_counter_inputs': 'BOT-GS-8', 'life_counter_outputs': 'BOT-GS-8',
    'ticket_counter_inputs': 'BOT-GS-6', 'ticket_counter_outputs': 'BOT-GS-6',
    'life_counters': 'BOT-GS-8', 'attacker_lives': 'BOT-GS-8', 'defender_bases': 'BOT-GS-8',
}


def checks(game: Path) -> list[tuple[bool, str]]:
    """(ok, description) per stable answer the rules cite; each description names its rules."""
    got = summary(analyse(game))
    return [(got.get(k) == want, f'{RULES.get(k, "modes")}: {k} = {want!r}'
             + ('' if got.get(k) == want else f' (got {got.get(k)!r})')) for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    st = analyse(game)
    print(f'== {st.partitions} DICE partitions, {st.parsed} with a counter type; site names in the data: '
          f'{len(st.names)} of {len(SITES)}')
    for (t, d, n), c in sorted(st.events.items()):
        print(f'  {t:28s} {d:3s} {n:24s} {c}')
    print('== exe:', st.exe)
    for k, v in summary(st).items():
        print(f'{k}: {v!r}')


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    main()
