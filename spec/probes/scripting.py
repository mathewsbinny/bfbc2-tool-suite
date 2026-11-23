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
}


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
