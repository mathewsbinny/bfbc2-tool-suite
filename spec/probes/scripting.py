"""The level event graph as DICE authored it, against the entity classes the executable defines (read-only;
spec/engine/scripting.md SCR-*, round SPG3).

    python spec/probes/scripting.py [--opfor <folder>]

Reads each DICE level's partition `Levels/<LV>.dbx` once (as spec/probes/level.py finds it: Package's copy when it is
DICE's, else Dist's) and, per level, its `EventConnections` (LEVEL-EVT-1): the class of each connection's source and
target instance (a local reference resolved in the partition; `external` for another partition's instance), the
event names on both ends, and the classes of every instance of the partition. From `game/BFBC2Game.exe` (PC build
795745, data read only) it lists the class names ending in `EntityData` (the type names in the executable's strings).

Prints structure, counts and identifiers only (class and event names; no values). `checks(game)` recomputes the
stable answers the rules cite for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import importlib.util
import os
import re
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

BUILD_STAMP = 0x4E55445D
ENTITY_DATA = re.compile(rb'(?<=\x00)[A-Z][A-Za-z0-9]*EntityData(?=\x00)')
# SCR-CMD-1: the data entities that reach the engine's settings or a console
SPECIAL = ('ConsoleCommandEntityData', 'GameSettingEntityData', 'DebugMenuItemEntityData', 'DebugMenuSubMenuEntityData',
           'DebugTextEntityData', 'DebugTestEntityData')

EXPECTED: dict[str, Any] = {
    'levels': 74, 'connections': 57667, 'external_ends': 1739,
    'node_classes': 77, 'source_classes': 56, 'target_classes': 72,
    'event_names': 186, 'source_events': 82, 'target_events': 104,
    'top_classes': ['ReferenceObjectData', 'EventSplitterEntityData', 'DelayTriggerEntityData', 'GameEffectEntityData',
                    'VoiceOverEntityData', 'GameInteractionEntityData', 'AreaTriggerEntityData',
                    'MultipleTriggerEntityData'],
    'special_instances': {'ConsoleCommandEntityData': 0, 'DebugMenuItemEntityData': 104,
                          'DebugMenuSubMenuEntityData': 5, 'DebugTestEntityData': 0, 'DebugTextEntityData': 9,
                          'GameSettingEntityData': 0},
    'exe_entity_data': 180, 'exe_entity_data_in_graphs': 73,
    # SCR-EVT-3: the entity factories, the unused classes with and without one, the handlers' input events
    'exe_factories': {'client': 100, 'server': 148}, 'exe_factory_data_classes': 123,
    'unused_with_factory': 62, 'unused_without_factory': 45,
    'exe_handler_events_missing': [], 'exe_console_command_handler_returns': True,
}
# SCR-EVT-3: the factory registries (registry object, registration function) of the server and the client
REGISTRIES = {'server': (0x0157041C, 0x0071C790), 'client': (0x01572768, 0x008B6A60)}
DATA_CLASS_REGISTER = 0x00500880                 # the static initialisers' `push rec; mov ecx, info; call` for data
# the event handlers (vtable slot 6) and the input event names whose hashes they compare (SCR-EVT-3, SCR-CMD-2)
HANDLER_EVENTS = {
    0x0067A400: ['ChangeSetting'],                                   # ServerGameSettingEntity
    0x0066D9B0: ['LoadMemory', 'ClearMemory', 'FireMemory'],         # ServerEventMemoryEntity
    0x006FB890: ['Detonate'],                                        # ServerAmmoBoxEntity, ServerMedicBagEntity
    0x006C73B0: ['Pickup'],                                          # ServerPickupEntity
    0x00673870: ['Start', 'Stop'],                                   # ServerDisableTeamEntity
    0x00724FC0: ['Trigger'],                                         # ServerEventSequenceTriggerEntity
    0x00674EB0: ['Impulse'],                                         # ServerSpeedEventGateEntity
    0x006C3F10: ['Continue', 'FakeSave'],                            # ServerContinueEntity
    0x00714C90: ['Destroy', 'Heal', 'Hide', 'Show', 'PlayAnimation', 'StopAnimation', 'Disable'],  # static models
    0x008DD7F0: ['PlayAnimation', 'StopAnimation'],                  # ClientStaticModelEntity
    0x00674F90: ['BriefPlayer'],                                     # ServerTacticalObjectiveEntity
    0x006987E0: ['StartImpulseTest'],                                # ServerProjectileImpulseTestEntity
    0x00FFE580: ['Enable', 'Disable'],                               # every entity's base handler
}
CONSOLE_COMMAND_HANDLER = 0x0068EFF0             # ServerConsoleCommandEntity's slot 6: `ret 4`


def name_hash(s: str) -> int:
    h = 5381
    for c in s.encode('latin-1'):
        h = ((h * 33) ^ c) & 0xFFFFFFFF
    return h


def factories(img: Any) -> dict[str, set[str]]:
    """SCR-EVT-3: {side: the data class names with a factory}. Each registration is `mov [eax], <factory vtable>; ...
    push eax; mov ecx, <registry>; call <register>`; the factory's slot 1 is `mov eax, <data class info>; ret`, and the
    class info's static initialiser `push <type record>; mov ecx, <class info>; call 0x500880` names it."""
    names: dict[int, str] = {}
    for m in re.finditer(rb'\x68(....)\xb9(....)\xe8(....)', img.b, re.S):
        va = img.va(m.start())
        if va is None or not img.is_code(va):
            continue
        if va + 15 + struct.unpack('<i', m.group(3))[0] != DATA_CLASS_REGISTER:
            continue
        rec, info = struct.unpack('<I', m.group(1))[0], struct.unpack('<I', m.group(2))[0]
        o = img.off(rec)
        nm = img.cstr(struct.unpack_from('<I', img.b, o)[0]) if o is not None else None
        if nm:
            names[info] = nm
    out: dict[str, set[str]] = {}
    for side, (reg, fn) in REGISTRIES.items():
        got = out[side] = set()
        out['_' + side] = set()
        pat = b'\xb9' + struct.pack('<I', reg) + b'\xe8'
        for m in re.finditer(re.escape(pat), img.b):
            va = img.va(m.start())
            if va is None or va + 10 + struct.unpack_from('<i', img.b, m.start() + 6)[0] != fn:
                continue
            out['_' + side].add(va)
            win = img.b[m.start() - 40:m.start()]
            k = win.rfind(b'\xc7\x00')
            if k < 0:
                continue
            vt = struct.unpack_from('<I', win, k + 2)[0]
            slot1 = struct.unpack_from('<I', img.b, img.off(vt + 4))[0]
            o = img.off(slot1)
            if o is not None and img.b[o] == 0xB8 and img.b[o + 5] == 0xC3:
                info = struct.unpack_from('<I', img.b, o + 1)[0]
                if info in names:
                    got.add(names[info])
    return out


def handlers(img: Any) -> list[str]:
    """SCR-EVT-3: the names whose hash is not an immediate within 0x300 bytes of the handler that compares it."""
    missing = []
    for va, events in HANDLER_EVENTS.items():
        body = img.b[img.off(va):img.off(va) + 0x300]
        missing += [f'{va:#x} {e}' for e in events if struct.pack('<I', name_hash(e)) not in body]
    return missing


def level_probe() -> Any:
    spec = importlib.util.spec_from_file_location('spec_probe_level_for_scripting', SUITE / 'spec' / 'probes' /
                                                  'level.py')
    assert spec is not None and spec.loader is not None
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def kind(i: dbx.Element) -> str:
    return (i.attr('type') or '').split('.')[-1]


def analyse(game: Path) -> dict[str, Any]:
    lp = level_probe()
    levels = lp.level_bundles(game)
    out: dict[str, Any] = {'levels': len(levels)}
    conns = ext = 0
    src: collections.Counter[str] = collections.Counter()
    dst: collections.Counter[str] = collections.Counter()
    sev: set[str] = set()
    tev: set[str] = set()
    classes: collections.Counter[str] = collections.Counter()
    for lv, path in sorted(levels.items()):
        name = f'levels/{lv}.dbx'
        f = dbx.parse(fbrb.read(path, [name])[name])
        types = {}
        for i in f.instances:
            types[(i.attr('guid') or '').lower()] = kind(i)
            classes[kind(i)] += 1
        ld = next((i for i in f.instances if kind(i) == 'LevelData'), None)
        ec = ld.child('EventConnections') if ld is not None else None
        for c in (ec.children or []) if ec is not None else []:
            conns += 1
            for end, counter in (('Source', src), ('Target', dst)):
                r = (c.child(end).attr('ref') if c.child(end) is not None else '') or ''  # type: ignore[union-attr]
                if '/' in r:
                    ext += 1
                    counter['external'] += 1
                else:
                    counter[types.get(r.lower(), 'null')] += 1
            for end, names in (('SourceEvent', sev), ('TargetEvent', tev)):
                e = c.child(end)
                n = e.child('Name') if e is not None else None
                names.add(dbx.value(n) if n is not None else '')
    nodes = (set(src) | set(dst)) - {'external', 'null'}
    out.update(connections=conns, external_ends=ext, node_classes=len(nodes),
               source_classes=len(set(src) - {'external', 'null'}), target_classes=len(set(dst) - {'external', 'null'}),
               event_names=len(sev | tev), source_events=len(sev), target_events=len(tev))
    both = src + dst
    del both['external']
    out['top_classes'] = [k for k, _ in both.most_common(8)]
    out['special_instances'] = {k: classes[k] for k in sorted(SPECIAL)}
    out['_both'] = both
    exe = game / 'BFBC2Game.exe'
    if exe.is_file():
        img = menucommon.PEImage(os.fspath(exe))
        if img.timestamp == BUILD_STAMP:
            names = {m.group().decode() for m in ENTITY_DATA.finditer(img.b)}
            out['exe_entity_data'] = len(names)
            out['exe_entity_data_in_graphs'] = len(nodes & names)
            out['_exe_names'] = names
            out['_nodes'] = nodes
            fac = factories(img)
            out['exe_factories'] = {side: len(fac['_' + side]) for side in REGISTRIES}
            made = fac['server'] | fac['client']
            out['exe_factory_data_classes'] = len({n for n in made if n.endswith('EntityData')})
            unused = names - nodes
            out['unused_with_factory'] = len(unused & made)
            out['unused_without_factory'] = len(unused - made)
            out['_unused_without_factory'] = sorted(unused - made)
            out['exe_handler_events_missing'] = handlers(img)
            o = img.off(CONSOLE_COMMAND_HANDLER)
            out['exe_console_command_handler_returns'] = img.b[o:o + 3] == bytes.fromhex('c20400')
    return out


def checks(game: Path) -> list[tuple[bool, str]]:
    got = analyse(game)
    return [(got.get(k) == want, f'scripting probe: {k} = {want!r} (got {got.get(k)!r})')
            for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    t = time.time()
    got = analyse(game)
    for k, v in sorted(got.items()):
        if not k.startswith('_'):
            print(f'{k}: {v}')
    print('classes by connection ends:', ', '.join(f'{k} {v}' for k, v in got['_both'].most_common(40)))
    if '_nodes' in got:
        nodes, names = got['_nodes'], got['_exe_names']
        print('graph classes that are EntityData in the exe:', len({c for c in nodes if c in names}),
              '; not EntityData:', sorted(c for c in nodes if not c.endswith('EntityData')))
    print(f'({time.time() - t:.1f} s)')


if __name__ == '__main__':
    main()
