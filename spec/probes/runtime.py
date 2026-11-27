"""The run-time code behind SPG4's data-side files and the resources' insides, as fixed facts of the executable,
checked against DICE's data (read-only; round SPG4-rest: SOLD-PHYS-2, SQD-PLAY-2, HUD-KILL-2, CAMP-SAVE-1,
SGR-NODE-2, OCC-TRI-1, SOOT-FLAG-1, IR-PART-1).

    python spec/probes/runtime.py [--opfor <folder>]

From `game/BFBC2Game.exe` (PC build 795745, data read only) it checks the instructions and constants the rules cite
(EXE: an address and the bytes found there: the character states' ids, the squad spawn's limits, the kill cam's
switches, the save writer's words, the resource loaders' factories), and from DICE's partitions (dated before 2013,
each name once) it counts the values of the data fields those code paths read (DATA). Prints structure and counts
only. `checks(game)` recomputes them for core/tests/test_spec_probes.py.
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
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths  # noqa: E402

BUILD_STAMP = 0x4E55445D
IMAGE_BASE = 0x400000

# (rule, address, the bytes there, what they are)
EXE: list[tuple[str, int, str, str]] = [
    # SOLD-PHYS-2: the states' ids (each state's slot 3) and the state list's first class test
    ('SOLD-PHYS-2', 0xAB7B70, '33c0c3', 'OnGround: id 0'),
    ('SOLD-PHYS-2', 0xA9D590, 'b801000000c3', 'Jump: id 1'),
    ('SOLD-PHYS-2', 0xA9D580, 'b802000000c3', 'InAir: id 2'),
    ('SOLD-PHYS-2', 0xA9D150, 'b803000000c3', 'Climbing: id 3'),
    ('SOLD-PHYS-2', 0xA9D570, 'b804000000c3', 'Falling: id 4'),
    ('SOLD-PHYS-2', 0xA9DAC0, 'b805000000c3', 'Parachute: id 5'),
    ('SOLD-PHYS-2', 0xA9DAE0, 'b806000000c3', 'Swimming: id 6'),
    ('SOLD-PHYS-2', 0xA9CCC0, 'b807000000c3', 'AnimationControlled: id 7'),
    ('SOLD-PHYS-2', 0xA9DAD0, 'b808000000c3', 'Sliding: id 8'),
    ('SOLD-PHYS-2', 0xB08D5C, '68d890bf01', 'the state builder tests the entry against OnGroundStateData'),
    ('SOLD-PHYS-2', 0xA9D700, '83f802', 'Jump leaves for InAir after two updates'),
    # SQD-PLAY-2: spawn on a squad mate
    ('SQD-PLAY-2', 0x6C9796, '83f818', 'squad 24 (SquadNone) refused'),
    ('SQD-PLAY-2', 0x5735E9, '83f80d', '13 candidate positions'),
    ('SQD-PLAY-2', 0x1418B70, '00007ac4', 'the ground ray down to -1000.0'),
    ('SQD-PLAY-2', 0x14D9638, '0000000000005940', 'height difference squared against 100.0'),
    # HUD-KILL-2: the kill cam
    ('HUD-KILL-2', 0x67AF22, '80b8ac00000000', 'SpectateEntityData.UseInHardcoreMode (+0xAC) tested'),
    ('HUD-KILL-2', 0x67AF44, '6868dc4101', 'pushes "Server.KillCam" (0x141DC68)'),
    ('HUD-KILL-2', 0x67AF56, '8b8294000000', 'SpectateEntityData.Target (+0x94) read'),
    # CAMP-SAVE-1: the save writer
    ('CAMP-SAVE-1', 0x67AB11, 'c7465046000000', 'the save entity\'s body version 70'),
    ('CAMP-SAVE-1', 0x67AB37, '6884078a7c', 'the Save event\'s hash'),
    ('CAMP-SAVE-1', 0x50941A, '81c10012a41883d231', '_time64() + 210,866,803,200'),
    ('CAMP-SAVE-1', 0x6882A0, '6a03', 'the header\'s last word: 3'),
    # SGR-NODE-2: SimpleTransformNode's factory and node
    ('SGR-NODE-2', 0xFE5CC0, 'b8e401c101c3', "the factory's slot 1: SimpleTransformNode's class info"),
    ('SGR-NODE-2', 0xFEDE70, '6a20', "the factory's slot 2 allocates 0x20 bytes"),
    ('SGR-NODE-2', 0xFEDE83, 'c700703e4b01', "... and stores the node's vtable 0x14B3E70"),
    # OCC-TRI-1, SOOT-FLAG-1, IR-PART-1: the loaders' factories (vtables: type, load)
    ('OCC-TRI-1', 0x141B644, '601c9700', 'OccluderMesh factory: type 0x971C60'),
    ('OCC-TRI-1', 0x141B64C, '00699700', 'OccluderMesh factory: load 0x976900'),
    ('OCC-TRI-1', 0x976808, '855660', 'the triangle\'s first mask word tested against the instance\'s +0x60'),
    ('SOOT-FLAG-1', 0x147E990, 'e009c400', 'SootMesh factory: type 0xC409E0'),
    ('SOOT-FLAG-1', 0x147E998, '7086c400', 'SootMesh factory: load 0xC48670'),
    ('IR-PART-1', 0x14B2288, 'd08cfc00', 'ImpulseResponse factory: type 0xFC8CD0'),
    ('IR-PART-1', 0x14B2290, '6022fd00', 'ImpulseResponse factory: load 0xFD2260'),
]
# SOOT-FLAG-1: the four declaration names the loader pushes between 0xC4EE44 and 0xC4EEA4
SOOT_DECLS = {0x147EAE8: 'SootPerVertex', 0x147EAD4: 'SootPerVertex_Half', 0x147EAC4: 'SootPerPixel',
              0x147EAB0: 'SootPerPixel_Half'}
STRINGS = {0x141DC68: 'Server.KillCam'}

# the DICE data the code paths read: class -> fields whose values are counted
DATA_FIELDS = {
    'SpectateEntityData': ['Target', 'Mode', 'UseInHardcoreMode'],                          # HUD-KILL-2
    'TeamData': ['MaxPlayersPerSquad'],                                                      # SQD-PLAY-2
    'SaveEntityData': ['SaveDelayTime'],                                                     # CAMP-SAVE-1
}
STATE_CLASSES = ['OnGroundStateData', 'JumpStateData', 'InAirStateData', 'ClimbingStateData', 'FallingStateData',
                 'ParachuteStateData', 'SwimmingStateData', 'AnimationControlledStateData', 'SlidingStateData']

EXPECTED: dict[str, Any] = {
    'exe_bytes_differ': [], 'soot_decls': True, 'strings': True,
    'SpectateEntityData.Target': {'StBestSquadPlayer': 101, 'StNone': 2, 'StOwnKiller': 62,
                                  'StOwnPlayerAndVehicleRagdoll': 62},
    'SpectateEntityData.Mode': {'SmFollow': 185, 'SmStatic': 42},
    'SpectateEntityData.UseInHardcoreMode': {'false': 123, 'true': 104},
    'TeamData.MaxPlayersPerSquad': {'4': 10},
    'SaveEntityData.SaveDelayTime': {'0': 179, '0.1': 1, '1': 1, '1.5': 2},
    # SOLD-PHYS-2: every state class in each of DICE's two CharacterPhysicsData
    'state_classes': {c: 2 for c in STATE_CLASSES},
}


def dice(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def exe_facts(exe: Path) -> dict[str, Any]:
    b = exe.read_bytes()
    pe = struct.unpack_from('<I', b, 0x3C)[0]
    if struct.unpack_from('<I', b, pe + 8)[0] != BUILD_STAMP:
        return {'exe_bytes_differ': ['not build 795745']}
    nsec, opt = struct.unpack_from('<H', b, pe + 6)[0], struct.unpack_from('<H', b, pe + 20)[0]
    secs = [struct.unpack_from('<8sIIII', b, pe + 24 + opt + 40 * i) for i in range(nsec)]

    def off(va: int) -> int:
        rva = va - IMAGE_BASE
        for _n, vs, sva, rs, ra in secs:
            if sva <= rva < sva + max(vs, rs):
                return rva - sva + ra
        raise ValueError(hex(va))

    differ = [f'{rule} 0x{va:X} {what}' for rule, va, want, what in EXE
              if b[off(va):off(va) + len(want) // 2].hex() != want]
    seg = b[off(0xC4EE44):off(0xC4EEA4)]
    decls = all(b'\x68' + struct.pack('<I', va) in seg for va in SOOT_DECLS)
    names = all(b[off(va):off(va) + len(s) + 1] == s.encode() + b'\x00' for va, s in {**SOOT_DECLS, **STRINGS}.items())
    return {'exe_bytes_differ': differ, 'soot_decls': decls, 'strings': names}


def data_facts(game: Path) -> dict[str, Any]:
    counts: dict[str, collections.Counter[str]] = {f'{c}.{f}': collections.Counter()
                                                   for c, fs in DATA_FIELDS.items() for f in fs}
    states: collections.Counter[str] = collections.Counter()
    wanted = [*DATA_FIELDS, 'CharacterPhysicsData']
    seen: set[str] = set()
    for p in sorted((p for p in game.glob('**/*.fbrb') if dice(p)), key=lambda p: str(p).lower()):
        es = [e for e in fbrb.entries(p) if not e.deleted and e.name.lower().endswith('.dbx')
              and e.name.lower() not in seen]
        seen.update(e.name.lower() for e in es)
        names = [e.name for e in es]
        for i in range(0, len(names), 300):
            got = fbrb.read(p, names[i:i + 300])
            sel = [got[n] for n in names[i:i + 300] if any(c.encode() in got[n] for c in wanted)]
            for f in dbx.parse_many(sel) if sel else []:
                if isinstance(f, ValueError):
                    continue
                kinds = {(i.attr('guid') or '').lower(): (i.attr('type') or '').split('.')[-1] for i in f.instances}
                for inst in f.instances:
                    c = (inst.attr('type') or '').split('.')[-1]
                    for fld in DATA_FIELDS.get(c, []):
                        e = inst.child(fld)
                        counts[f'{c}.{fld}'][dbx.value(e) if e is not None else ''] += 1
                    if c == 'CharacterPhysicsData':
                        st = inst.child('States')
                        for m in (st.children or []) if st is not None else []:
                            states[kinds.get((m.attr('ref') or '').lower(), 'external')] += 1
    out: dict[str, Any] = {k: dict(sorted(v.items())) for k, v in counts.items()}
    out['state_classes'] = {c: states[c] for c in STATE_CLASSES}
    return out


def analyse(game: Path) -> dict[str, Any]:
    out = exe_facts(game / 'BFBC2Game.exe')
    out.update(data_facts(game))
    return out


def checks(game: Path) -> list[tuple[bool, str]]:
    got = analyse(game)
    return [(got.get(k) == want, f'runtime probe: {k} = {want!r} (got {got.get(k)!r})') for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    t = time.time()
    for k, v in sorted(analyse(game).items()):
        print(f'{k}: {v}')
    print(f'({time.time() - t:.1f} s)')


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    main()
