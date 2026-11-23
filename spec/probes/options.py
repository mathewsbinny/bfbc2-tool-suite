"""The options and the input system as DICE's data and the executable define them (read-only; spec/engine/settings.md
SET-*, spec/engine/input.md INP-*, round SPG3).

    python spec/probes/options.py [--opfor <folder>]

Reads, from Project OPFOR BFBC2's game copy (DICE's files only):

- the startup bundle (`game/Dist/win32/async/startup-00.fbrb`): `Settings/GameSettings` (the defaults of
  `GameSettings.ini`), `Input/InputConfiguration` (the user-configurable action maps and the controls screen's
  layout) and the `Input/*InputConcepts` partitions (the device bindings per concept);
- `game/Dist/win32/levels/mp_common/level-00.fbrb`: the `Input/InputMappings/*` partitions (concept -> entry input
  action);
- `game/BFBC2Game.exe` (PC build 795745 only, data read, no code run): the menu name table's ids of the settings and
  controls names, the `settings.ini` and `GameSettings.ini` section and key names, the enumerations' field tables.

Prints structure, counts and identifiers only (class, field, setting and enumeration names; no values beyond the
setting types). `checks(game)` recomputes the stable answers the rules cite for core/tests/test_spec_probes.py.
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
import aptrefslib  # noqa: E402
import menucommon  # noqa: E402
from bfbc2core import dbx, fbrb, paths  # noqa: E402

BUILD_STAMP = 0x4E55445D                     # BFBC2Game.exe build 795745
STARTUP = ('Dist', 'win32', 'async', 'startup-00.fbrb')
MP_COMMON = ('Dist', 'win32', 'levels', 'mp_common', 'level-00.fbrb')
# SET-UI-1 / INP-UI-1: the settings and controls names of the menu name table and the ids the dispatchers switch on
UI_NAMES = {
    'setSetting': 0x5B, 'resetSettings': 0x5C, 'applySettings': 0x5D, 'loadSettings': 0x5E, 'saveSettings': 0x5F,
    'setRecomendedSettings': 0x60, 'changeResolution': 0x99, 'settingsAudio': 0x9C, 'settingsGraphics': 0x9D,
    'settingsRender': 0x9E, 'settingsAI': 0x9F, 'settingsGameplay': 0xA0, 'settingsInput': 0xA1, '_settings': 0xA2,
    'isSettingsDirty': 0xCA, 'adapterResolutions': 0xE7, 'isDefaultSettings': 0x151,
    'setActiveInputScheme': 0xAA, 'confirmKeySwitch': 0xC6, 'cancelKeySwitch': 0xC7, 'requestEditInput': 0xC8,
    'resetInputSettings': 0xC9, 'controlsSliderMoved': 0xCC, 'controlInvertToggle': 0xCD,
    'currentInputMapping': 0x10A, 'editableInputList': 0x13C, 'controlsSliderValues': 0x13D,
    'controlsToggleValues': 0x13E, 'controllerConnected': 0xCF,
}
# SET-INI-1: settings.ini's sections and keys (the static key table the reader and writer use)
INI_SECTIONS = {
    'WindowSettings': ['Width', 'Height', 'Fullscreen', 'RefreshRate', 'VSync'],
    'Sound': ['Quality', 'VoipEnable', 'SpeakerCount'],
    'Graphics': ['Effects', 'Soldiers', 'Vehicles', 'Overgrowth', 'Undergrowth', 'StaticObjects', 'Terrain',
                 'Shadows', 'Bloom', 'HSAO', 'MSAA', 'Water', 'MainQuality', 'Texture', 'DxVersion', 'Aniso',
                 'Detail', 'RenderAheadLimit', 'Fov'],
    'Chat': ['ChatWindowAlpha', 'SmallChatWindowTimer'],
}
INI_TABLE = (0x1570FA8, 0x1571134)           # SET-INI-1: the key table's {begin, end, capacity} strings
INI_INIT = (0x1372AA0, 0x13735C0)            # its static initialisers
# SET-GS-2: the category enumeration's names (0x80D970), in value order
CATEGORIES = ['GstAudio', 'GstGraphics', 'GstRender', 'GstInput', 'GstAI', 'GstGameplay', 'GstPlayerProfile',
              'GstPersistence', 'GstBinary']
CATEGORY_NAMES = 0x80D970
# INP-ENUM-1 / INP-ENUM-2: the enumerations' field tables (records of 0x18 bytes: name at +0, value at +0x10)
ENUMS = {'EntryInputAction': 0x01C27C58, 'InputConcept': 0x01C2F140}

EXPECTED: dict[str, Any] = {
    'gs_categories': ['GstAudio', 'GstRender', 'GstAI', 'GstInput', 'GstGameplay', 'GstPlayerProfile',
                      'GstMenuTracker', 'GstPersistence'],
    'gs_settings': {'Float': 14, 'Int': 34, 'String': 11, 'Binary': 0},
    'gs_user_exposed': 45,
    'gs_hidden': ['AimAssist', 'AlternativeHeliControls', 'CameraBobbing', 'Crouch', 'ELO', 'FlashValues',
                  'highestRankDialog', 'Scheme1FlipY', 'Scheme1Sensitivity', 'ThemeInstalled',
                  'UIMenuTrackerPage_Awards', 'UIMenuTrackerPage_HudUnlocks', 'UIMenuTrackerPage_MenuUnlocks',
                  'UIMenuTrackerPage_Store'],
    'maps': [('infantry', 'bfbc2soldierinputconcepts', 36), ('air', 'bfbc2airinputconcepts', 18),
             ('land', 'bfbc2vehicleinputconcepts', 15), ('shared', 'bfbc2sharedinputconcepts', 12)],
    'editable': {'OneOptionAxisEditableAction': 33, 'OneOptionButtonEditableAction': 49},
    'editable_listed': 81,
    'concepts': {
        'bfbc2airinputconcepts': (42, 54, 27, 29), 'bfbc2defaultinputconcepts': (36, 53, 13, 31),
        'bfbc2sharedinputconcepts': (50, 56, 18, 19), 'bfbc2soldierinputconcepts': (43, 58, 45, 53),
        'bfbc2vehicleinputconcepts': (41, 53, 29, 39), 'venicemissileinputconcepts': (35, 38, 10, 11),
        'venicesoldierinputconcepts': (38, 42, 12, 23), 'venicetankinputconcepts': (35, 36, 10, 22),
    },
    'mappings': {'bfbc2airinputmapping': 40, 'bfbc2gunnerinputmapping': 38, 'bfbc2soldierinputmapping': 48,
                 'bfbc2uavinputmapping': 19, 'bfbc2vehicleinputmapping': 42},
    'exe_build': BUILD_STAMP, 'exe_ui_ids_wrong': [], 'exe_ini_keys_missing': [], 'exe_ini_table': 33,
    'exe_categories': CATEGORIES,
    'enum_EntryInputAction': (50, 'EiaThrottle', 'EiaUndefined', 49),
    'enum_InputConcept': (77, 'ConceptMoveFB', 'ConceptUndefined', 76),
}


def kind(i: dbx.Element) -> str:
    return (i.attr('type') or '').split('.')[-1]


def member(e: dbx.Element, name: str) -> dbx.Element | None:
    return e.child(name)


def val(e: dbx.Element | None) -> str:
    return '' if e is None else dbx.value(e)


def parse(bundle: Path, names: list[str]) -> dict[str, dbx.DbxFile]:
    got = fbrb.read(bundle, names)
    return {n.lower()[:-4]: dbx.parse(b) for n, b in got.items()}


def game_settings(f: dbx.DbxFile, out: dict[str, Any]) -> None:
    """SET-GS-1: the GameSettingsAsset's categories and their typed settings."""
    asset = f.instances[0]
    cats = []
    types: collections.Counter[str] = collections.Counter()
    exposed, hidden = 0, []
    for cat in member(asset, 'Categories').children or []:  # type: ignore[union-attr]
        cats.append(val(member(cat, 'Category')))
        for arr in cat.children or []:
            if arr.tag != 'array':
                continue
            t = (arr.attr('name') or '')[:-1]
            for s in arr.children or []:
                types[t] += 1
                if val(member(s, 'UserExposed')) == 'true':
                    exposed += 1
                else:
                    hidden.append(val(member(s, 'Name')))
    out['gs_categories'] = cats
    out['gs_settings'] = {t: types[t] for t in ('Float', 'Int', 'String', 'Binary')}
    out['gs_user_exposed'] = exposed
    out['gs_hidden'] = sorted(hidden, key=str.lower)


def input_configuration(f: dbx.DbxFile, out: dict[str, Any]) -> None:
    """INP-CFG-1: the user-configurable maps, their concept partitions and the controls screen's editable actions."""
    asset = next(i for i in f.instances if kind(i) == 'InputConfigurationAsset')
    maps = []
    listed = 0
    for m in member(asset, 'UserConfigurableActionMaps').children or []:  # type: ignore[union-attr]
        am = member(m, 'ActionMap')
        ref = (am.attr('ref') if am is not None else None) or ''
        layout = member(m, 'ConfigurationLayout')
        acts = member(layout, 'Actions') if layout is not None else None
        n = len(acts.children or []) if acts is not None else 0
        listed += n
        maps.append((val(member(m, 'Id')), ref.rsplit('/', 2)[-2] if '/' in ref else ref, n))
    out['maps'] = maps
    out['editable'] = dict(sorted(collections.Counter(kind(i) for i in f.instances
                                                       if kind(i).endswith('EditableAction')).items()))
    out['editable_listed'] = listed


def concepts(parts: dict[str, dbx.DbxFile], out: dict[str, Any]) -> None:
    """INP-DATA-1: per concept partition: concepts (InputActionsData), keyboard, mouse and pad bindings."""
    got = {}
    for name, f in sorted(parts.items()):
        if not name.startswith('input/') or not name.endswith('inputconcepts'):
            continue
        c = collections.Counter(kind(i) for i in f.instances)
        got[name.split('/')[-1]] = (c['InputActionsData'], c['KeyboardInputActionData'], c['MouseInputActionData'],
                                    c['PadInputActionData'])
    out['concepts'] = got


def mappings(parts: dict[str, dbx.DbxFile], out: dict[str, Any]) -> None:
    """INP-DATA-2: per InputMappings partition: the concept -> action pairs."""
    got = {}
    for name, f in sorted(parts.items()):
        got[name.split('/')[-1]] = sum(1 for i in f.instances if kind(i) == 'EntryInputActionMappingData')
    out['mappings'] = got


def enum(img: Any, rec: int) -> tuple[int, str, str, int]:
    """An enumeration's field table from one of its records: (count, first, last name, last value)."""
    def at(va: int) -> tuple[str | None, int, int]:
        o = img.off(va)
        p, _x, typ, _y, value = struct.unpack_from('<IIIII', img.b, o)
        return img.cstr(p, 80), typ, value
    prefix = (at(rec)[0] or '')[:3]
    a = rec
    while True:
        n, typ, _ = at(a - 0x18)
        if not n or typ != 0 or not n.startswith(prefix):
            break
        a -= 0x18
    names: list[tuple[str, int]] = []
    while True:
        n, typ, value = at(a)
        if not n or typ != 0 or not n.startswith(prefix):
            break
        names.append((n, value))
        a += 0x18
    return len(names), names[0][0], names[-1][0], names[-1][1]


def engine(exe: Path, out: dict[str, Any]) -> None:
    if not exe.is_file():
        return
    img = menucommon.PEImage(os.fspath(exe))
    out['exe_build'] = img.timestamp
    if img.timestamp != BUILD_STAMP:
        return
    table = {nm: i for nm, i, _va in aptrefslib.extern_table(img)}
    out['exe_ui_ids_wrong'] = sorted(n for n, i in UI_NAMES.items() if table.get(n) != i)
    keys = [k for ks in INI_SECTIONS.values() for k in ks] + list(INI_SECTIONS) + CATEGORIES
    out['exe_ini_keys_missing'] = sorted(k for k in keys if not img.find_cstring(k))
    # the key table: {begin, end, capacity} string records of 12 bytes, one per section or key
    lo, hi = INI_TABLE
    out['exe_ini_table'] = (hi - lo) // 12
    # the category names: the switch's pushes in value order (jump table after the code)
    o = img.off(CATEGORY_NAMES)
    jt = struct.unpack_from('<I', img.b, o + 0xD)[0]
    cats = []
    for k in range(9):
        case = struct.unpack_from('<I', img.b, img.off(jt) + 4 * k)[0]
        push = img.b[img.off(case)]
        cats.append(img.cstr(struct.unpack_from('<I', img.b, img.off(case) + 1)[0]) if push == 0x68 else None)
    out['exe_categories'] = cats
    for name, rec in ENUMS.items():
        out['enum_' + name] = enum(img, rec)


def analyse(game: Path) -> dict[str, Any]:
    out: dict[str, Any] = {}
    startup = game.joinpath(*STARTUP)
    names = [e.name for e in fbrb.entries(startup) if not e.deleted and e.name.lower().endswith('.dbx')
             and (e.name.lower().startswith('input/') or e.name.lower() == 'settings/gamesettings.dbx')]
    parts = parse(startup, names)
    game_settings(parts['settings/gamesettings'], out)
    input_configuration(parts['input/inputconfiguration'], out)
    concepts(parts, out)
    common = game.joinpath(*MP_COMMON)
    mnames = [e.name for e in fbrb.entries(common) if not e.deleted
              and e.name.lower().startswith('input/inputmappings/') and e.name.lower().endswith('.dbx')]
    mappings(parse(common, mnames), out)
    engine(game / 'BFBC2Game.exe', out)
    return out


def checks(game: Path) -> list[tuple[bool, str]]:
    got = analyse(game)
    return [(got.get(k) == want, f'options probe: {k} = {want!r} (got {got.get(k)!r})') for k, want in EXPECTED.items()]


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
    main()
