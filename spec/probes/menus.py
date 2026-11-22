"""The menu and HUD movies' action script as DICE ships it, and the engine's side of it: the opcodes, constant pools,
classes, imports, engine names, movie loads and the HUD's movies (read-only; spec/engine/menus.md, MENU-AS-*,
MENU-RT-* and MENU-HUD-*).

    python spec/probes/menus.py [--opfor <folder>]

Reads every `SwfMovie` resource (`Menu/<name>.res`, a BIGF archive of `.apt` / `.const` / `.geo`) of DICE's bundles
in Project OPFOR BFBC2's game copy: `game/**/*.fbrb` dated before 2013 (as the install leaves DICE's files), plus
OPFOR's pristine copy of DICE's Package main menu (`original/Package/menu/main/mainmenu-00.fbrb`; the game's own copy
is OPFOR's patched one). Each distinct movie (by content) is decoded once with the suite's APT reader
(`bfbc2-console-readers/apt/apt.py`, the Rust core when built) and scanned with the suite's `aptrefs` scan
(`bfbc2-menu-tools/aptrefslib.py`: `extern.<name>` reads, FSCommands, engine events). From the same bundles, the
first copy of each `Menu/<name>.dbx` (`UI.UIAsset`) and `GameModes/*.dbx` (`GameSharedResources.GameModeData`)
partition. From `game/BFBC2Game.exe` (PC build 795745 only, data read, no code run): the action interpreter's handler
table and dispatch bytes, the FSCommand prefix, the event function name and the extern / FSCommand name table.

Prints structure, counts and identifiers only (opcode, class, property, event and movie names; no text, no bytes):

- the corpus: SwfMovie entries, distinct contents, movie names;
- byte code: instructions per opcode (each distinct movie once), function definitions, unknown opcodes, opcodes the
  interpreter has a handler for;
- constants: `.const` entry types, ConstantPool instructions and their sizes, PushData lists and their constants;
- classes: the `__Packages.<class>` exports (an AS2 class's registration sprite) per root package, classes defined
  in more than one movie;
- imports: the movie-to-movie character imports (the movies a movie needs loaded first);
- the engine interface: `extern.<name>` reads, parameterised reads and writes, FSCommands, engine events and the
  movies using each, names missing from the exe's name table; the root `gameEvent` functions; the `MOVIE_*` /
  `HUD_STATE_*` ids;
- movie loads: `loadMovie` / `loadClip` / `attachMovie` ... call sites and their literal targets, the `<name>.swf`
  names each movie's constants hold, the `UI.UIAsset` per movie and its source file, the HUD movies per game mode.

`checks(game)` recomputes the stable counts the rules cite (round SPG2-rest) for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
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

suite_paths.use('core', 'apt', 'menu-tools')
import apt  # noqa: E402
import aptrefslib  # noqa: E402
import menucommon  # noqa: E402
from bfbc2core import dbx, fbrb, paths  # noqa: E402

PRISTINE_MENU = ('original', 'Package', 'menu', 'main', 'mainmenu-00.fbrb')
LOADERS = ('loadMovie', 'loadMovieNum', 'loadClip', 'attachMovie', 'unloadMovie', 'unloadClip', 'removeMovieClip',
           'createEmptyMovieClip', 'duplicateMovieClip')
NAMED_CALLS = (0xB0, 0xB1, 0xB2, 0xB3)                    # EA_CallNamed{Func,Method}{Pop,}: the name is the operand
NAME_ON_STACK = (0x3D, 0x52, 0x5D)                        # CallFunction, CallMethod, EA_CallMethodPop: name pushed last
COUNTS = {0x59: 0, 0x5A: 1}                               # EA_PushZero / EA_PushOne; EA_PushByte (0xB5) carries it
SWF = re.compile(r'^[A-Za-z0-9_]+\.swf$')
ID_NAME = re.compile(r'^(MOVIE|HUD_STATE)_[A-Z0-9_]+$')
HUD_FIELDS = ('HudRootName', 'PrimaryLogicLayerName', 'SecondaryLogicLayerName', 'CrosshairsName', 'InGameMenuName')

# BFBC2Game.exe build 795745 (PE time stamp 0x4E55445D): the action interpreter (0x1310F90) reads an opcode byte
# (`movzx edi, byte [eax]` at 0x1311041) and calls the handler table at 0x1545B00 with no bound check
# (`mov edx, [edi*4 + 0x1545B00]` at 0x1311056); GetURL / GetURL2 (0x131F5A0 / 0x131F660) compare the URL with the
# prefix the pointer at 0x1545AA4 names; the menu events call the string at 0x143C1E0 (0x9299C8, 0x929ABC).
BUILD_STAMP = 0x4E55445D
HANDLERS = 0x1545B00
DISPATCH = ((0x1311041, bytes.fromhex('0fb638')), (0x1311056, bytes.fromhex('8b14bd005b5401')))
FSCOMMAND_PTR = 0x1545AA4
EVENT_NAME = 0x143C1E0


def stock(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def bundles(game: Path) -> list[Path]:
    """DICE's bundles: the game copy's dated before 2013, and OPFOR's pristine Package main menu when it is there."""
    out = [p for p in sorted(game.glob('**/*.fbrb')) if stock(p)]
    pristine = game.parent.joinpath(*PRISTINE_MENU)
    if pristine.is_file() and stock(pristine):
        out.append(pristine)
    return out


class Found:
    """A distinct movie: its name (lower case, Menu/<name>.res), content hash, the bundles holding it, its bytes.
    (Plain classes, no dataclasses: core/tests/test_spec_probes.py loads probes without registering the module.)"""

    def __init__(self, name: str, sha: str, data: bytes) -> None:
        self.name, self.sha, self.data = name, sha, data
        self.bundles: list[str] = []


def wanted_partition(low: str) -> bool:
    return low.endswith('.dbx') and (low.startswith('gamemodes/') or re.fullmatch(r'menu/[^/]+\.dbx', low) is not None)


def corpus(game: Path) -> tuple[int, int, list[Found], dict[str, bytes]]:
    """(SwfMovie entries, bundles holding one, the distinct movies by content, the first copy of each
    Menu/<name>.dbx and GameModes/*.dbx partition {lower-case entry name: bytes})."""
    by_sha: dict[str, Found] = {}
    parts: dict[str, bytes] = {}
    entries = holding = 0
    for p in bundles(game):
        es = [e for e in fbrb.entries(p) if not e.deleted]
        movies = [e.name for e in es if e.type == 'SwfMovie']
        dbxs = [e.name for e in es if e.name.lower() not in parts and wanted_partition(e.name.lower())]
        if not movies and not dbxs:
            continue
        holding += bool(movies)
        got = fbrb.read(p, movies + dbxs)
        for n in dbxs:
            parts[n.lower()] = got[n]
        for n in movies:
            d = got[n]
            entries += 1
            sha = hashlib.sha1(d).hexdigest()
            f = by_sha.get(sha)
            if f is None:
                f = by_sha[sha] = Found(n.rsplit('/', 1)[-1][:-4].lower(), sha, d)
            f.bundles.append(os.fspath(p.relative_to(game.parent)))
    return entries, holding, sorted(by_sha.values(), key=lambda f: (f.name, f.sha)), parts


def members(data: bytes) -> tuple[str, bytes, bytes]:
    m = menucommon.parse_bigf(data)
    aptname = next(k for k in m if k.lower().endswith('.apt'))
    return aptname[:-4], m[aptname], m[aptname[:-4] + '.const']


def table(m: Any, count_at: int, ptr_at: int, size: int) -> list[int]:
    """The offsets of a movie character's import (16-byte) or export (8-byte) entries (APT-MOV-1)."""
    n, p = m.u(m.movie + count_at), m.u(m.movie + ptr_at)
    return [p + size * k for k in range(n)] if p else []


def init_actions(m: Any) -> dict[int, int]:
    """{sprite character id: byte code offset} of the init actions on the movie's own timeline (APT-FRM-1, type 8)."""
    out: dict[int, int] = {}
    frames, count = m.u(m.movie + 12), m.u(m.movie + 8)
    for k in range(count if frames else 0):
        n, items = m.u(frames + 8 * k), m.u(frames + 8 * k + 4)
        for j in range(n):
            it = m.u(items + 4 * j)
            if m.u(it) == 8:
                out[m.u(it + 4)] = m.u(it + 8)
    return out


def guarded(code: list[Any], cls: str) -> bool:
    """The class registration starts `if (!_global.<package>.<Class>)`: within its first instructions a read of the
    class's last name followed by Not, Not, BranchIfTrue (the AS2 compiler's guard)."""
    last = cls.rsplit('.', 1)[-1]
    for k, ins in enumerate(code[:150]):
        if ins.op in (0xAF, 0xA2, 0xA3) and ins.arg == f'"{last}"':
            nxt = [c.op for c in code[k + 1:k + 5]]
            if nxt[:3] == [0x12, 0x12, 0x9D] or nxt[1:4] == [0x12, 0x12, 0x9D]:
                return True
    return False


def string_value(m: Any, ins: Any) -> str | None:
    """The last value an instruction pushes when it is a constant string, else None."""
    if ins.op in (0xA1, 0xA2, 0xA3) and ins.arg.startswith('"') and ins.arg.endswith('"'):
        return str(ins.arg[1:-1])
    if ins.op == 0x96 and ins.items:
        c = m.const[ins.items[-1]] if ins.items[-1] < len(m.const) else ''
        return c[1:-1] if c.startswith('"') else None
    return None


def count_value(ins: Any) -> int | None:
    if ins.op in COUNTS:
        return COUNTS[ins.op]
    return int(ins.arg) if ins.op == 0xB5 else None


class Stats:
    """What analyse() measures; sets of movie names are per name (a name's versions together)."""

    def __init__(self) -> None:
        self.entries = self.holding = 0
        self.movies: list[Found] = []
        self.ops: collections.Counter[int] = collections.Counter()
        self.unknown = self.instructions = 0
        self.functions: collections.Counter[str] = collections.Counter()
        self.const_types: collections.Counter[str] = collections.Counter()
        self.pools: list[int] = []
        self.word_pool = 0                                # 0xA3 uses (a pool index past 255)
        self.push_items: collections.Counter[str] = collections.Counter()   # PushData items by constant kind
        self.push_lists: collections.Counter[int] = collections.Counter()   # PushData lists by item count
        self.reg_uses: collections.Counter[int] = collections.Counter()     # register constants by PushData uses
        self.classes: dict[str, set[str]] = collections.defaultdict(set)
        self.registrations: collections.Counter[bool] = collections.Counter()   # class init actions by guard
        self.unguarded: collections.Counter[str] = collections.Counter()
        self.exports = 0
        self.imports: dict[tuple[str, str], set[str]] = collections.defaultdict(set)
        self.refs: dict[str, dict[str, set[str]]] = collections.defaultdict(lambda: collections.defaultdict(set))
        self.handlers: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
        self.root_event: dict[str, set[int]] = collections.defaultdict(set)
        self.ids: dict[str, set[tuple[str, int]]] = collections.defaultdict(set)
        self.loaders: collections.Counter[str] = collections.Counter()
        self.load_targets: dict[tuple[str, str], set[str]] = collections.defaultdict(set)
        self.swf: dict[str, set[str]] = collections.defaultdict(set)
        self.exported: dict[str, set[str]] = collections.defaultdict(set)    # movie -> the names it exports
        self.assets: dict[str, tuple[tuple[str, ...], str]] = {}     # movie -> (instance types, SourceFile)
        self.modes: dict[str, dict[str, str]] = {}                   # GameModes/<mode> -> HUD movie names
        self.exe: dict[str, Any] = {}

    @property
    def names(self) -> set[str]:
        return {f.name for f in self.movies}


def scan_code(st: Stats, name: str, m: Any, classes: dict[int, str]) -> None:
    """classes: {sprite id: class} of the movie's `__Packages.<class>` exports."""
    recs = m.instructions()
    streams: dict[int, list[Any]] = collections.defaultdict(list)
    uses: collections.Counter[int] = collections.Counter()
    for ins, start, _limit, _depth in recs:
        st.ops[ins.op] += 1
        st.instructions += 1
        st.unknown += bool(ins.unknown)
        streams[start].append(ins)
        if ins.func:
            st.functions['DefineFunction2' if ins.op == 0x8E else 'DefineFunction'] += 1
            if ins.func.name == 'gameEvent':
                st.root_event[name].add(ins.func.nparams)
        if ins.op == 0x88 and ins.items is not None:
            st.pools.append(len(ins.items))
        if ins.op == 0x96 and ins.items is not None:
            st.push_lists[len(ins.items)] += 1
            for g in ins.items:
                uses[g] += 1
                st.push_items['register' if g < len(m.const) and m.const[g].startswith('reg') else 'string'] += 1
        st.word_pool += ins.op == 0xA3
    for k, c in enumerate(m.const):
        if c.startswith('reg'):
            st.reg_uses[min(uses[k], 2)] += 1
    for code in streams.values():
        code.sort(key=lambda i: i.at)
    inits = init_actions(m)
    for sprite, cls in classes.items():
        ok = guarded(streams.get(inits.get(sprite, -1), []), cls)
        st.registrations[ok] += 1
        if not ok:
            st.unguarded[cls] += 1
    for code in streams.values():
        for k, ins in enumerate(code):
            # MOVIE_* / HUD_STATE_* ids: `NAME = <small int>` (push name, push int, DefineLocal / SetVariable / SetMember)
            s = string_value(m, ins)
            if s and ID_NAME.match(s) and k + 2 < len(code) and code[k + 2].op in (0x3C, 0x1D, 0x4F):
                v = count_value(code[k + 1])
                if v is not None:
                    st.ids['MOVIE' if s.startswith('MOVIE') else 'HUD_STATE'].add((s, v))
            if ins.op in NAMED_CALLS:
                callee = ins.arg.strip('"')
            elif ins.op in NAME_ON_STACK and k:
                callee = string_value(m, code[k - 1]) or ''
            else:
                continue
            if callee not in LOADERS:
                continue
            st.loaders[callee] += 1
            target = '<dynamic>'
            back = code[max(0, k - 8):k]
            for j in range(len(back) - 1, 0, -1):
                n = count_value(back[j])
                if n is not None and n >= 1:
                    target = string_value(m, back[j - 1]) or '<dynamic>'
                    break
            st.load_targets[(callee, target)].add(name)


def partitions(st: Stats, parts: dict[str, bytes]) -> None:
    for n, d in sorted(parts.items()):
        f = dbx.parse(d)
        types = tuple(i.attr('type') or '' for i in f.instances)
        if n.startswith('gamemodes/'):
            for i in f.instances:
                if i.attr('type') == 'GameSharedResources.GameModeData':
                    fields = {k: i.child(k) for k in HUD_FIELDS}
                    st.modes[n[len('gamemodes/'):-4]] = {k: e.text if e is not None else '' for k, e in fields.items()}
        elif 'UI.UIAsset' in types:
            src = next((i.child('SourceFile') for i in f.instances if i.attr('type') == 'UI.UIAsset'), None)
            st.assets[n[len('menu/'):-4]] = (types, src.text if src is not None else '')


def engine(st: Stats, exe: Path) -> None:
    """The executable's side (build 795745 only): handler table, dispatch bytes, FSCommand prefix, event name, the
    name table, cross-checked against the movies."""
    if not exe.is_file():
        return
    img = menucommon.PEImage(os.fspath(exe))
    st.exe['build'] = img.timestamp
    if img.timestamp != BUILD_STAMP:
        return
    o = img.off(HANDLERS)
    assert o is not None
    handled, n = set(), 0
    while True:
        v = struct.unpack_from('<I', img.b, o + 4 * n)[0]
        if v and not img.is_code(v):
            break                                       # the first word that is neither 0 nor code ends the table
        if v:
            handled.add(n)
        n += 1
    st.exe['table'] = n
    st.exe['handled'] = handled
    st.exe['dispatch'] = all(img.b[img.off(va):img.off(va) + len(b)] == b for va, b in DISPATCH)
    st.exe['fscommand'] = img.cstr(struct.unpack_from('<I', img.b, img.off(FSCOMMAND_PTR))[0])
    st.exe['event'] = img.cstr(EVENT_NAME)
    names = {nm for nm, _i, _va in aptrefslib.extern_table(img)}
    st.exe['names'] = len(names)
    st.exe['missing'] = {kind: sorted(x for x in st.refs[kind] if x not in names and x not in ('prototype',)
                                      and not x.startswith('__'))
                         for kind in ('extern', 'extern_param', 'fscommand')}


def analyse(game: Path) -> Stats:
    st = Stats()
    st.entries, st.holding, st.movies, parts = corpus(game)
    partitions(st, parts)
    movies = [apt.Movie(data=members(f.data)) for f in st.movies]
    for f, m in zip(st.movies, apt.decoded(movies)):
        f.data = b''                                       # the bytes are no longer needed
        for c in m.const:
            st.const_types['string' if c.startswith('"') else 'register' if c.startswith('reg') else 'other'] += 1
            if c.startswith('"') and SWF.match(c[1:-1]):
                st.swf[f.name].add(c[1:-1].lower())
        for at in table(m, 40, 44, 16):
            st.imports[(m.s(m.u(at)).lower(), m.s(m.u(at + 4)))].add(f.name)
        classes: dict[int, str] = {}
        for at in table(m, 48, 52, 8):
            st.exports += 1
            n = m.s(m.u(at))
            st.exported[f.name].add(n)
            if n.startswith('__Packages.'):
                st.classes[n[len('__Packages.'):]].add(f.name)
                classes[m.u(at + 4)] = n[len('__Packages.'):]
        scan_code(st, f.name, m, classes)
    for f, r in zip(st.movies, aptrefslib.refs_many(movies)):
        for kind in ('extern', 'extern_param', 'extern_write', 'fscommand'):
            for n in r[kind]:
                st.refs[kind][n].add(f.name)
        for n, uses in r['events'].items():
            st.refs['events'][n].add(f.name)
            for u in uses:
                st.handlers[n][str(u[2]).split(' ')[0]] += 1
    engine(st, game / 'BFBC2Game.exe')
    return st


# ------------------------------------------------------------------------------------------------ the checks

def summary(st: Stats) -> dict[str, Any]:
    """The stable counts the rules cite."""
    roots = collections.Counter(c.split('.')[0] for c in st.classes)
    sources = sorted({src for src, _sym in st.imports})
    handled = st.exe.get('handled')
    swf_ok = all(s[:-4] in st.names for ss in st.swf.values() for s in ss)
    return {
        'entries': st.entries, 'bundles': st.holding, 'distinct': len(st.movies), 'names': len(st.names),
        'instructions': st.instructions, 'opcodes': len(st.ops), 'unknown': st.unknown,
        'functions': dict(sorted(st.functions.items())),
        'const_types': dict(sorted(st.const_types.items())),
        'pools': len(st.pools), 'pool_max': max(st.pools, default=0), 'word_pool': st.word_pool,
        'push_lists_over_1': sum(n for size, n in st.push_lists.items() if size > 1),
        'push_register_items': st.push_items['register'],
        'registers_not_used_once': st.reg_uses[0] + st.reg_uses[2],
        'classes': len(st.classes), 'class_roots': dict(sorted(roots.items())),
        'dialog_movies': len(st.classes.get('dice.Dialog', ())),
        'registrations': {'guarded': st.registrations[True], 'unguarded': st.registrations[False]},
        'import_sources': sources,
        'imports_unresolved': sorted(k for k in st.imports if k[1] not in st.exported.get(k[0], ())),
        'externs': len(st.refs['extern']), 'extern_params': len(st.refs['extern_param']),
        'extern_writes': sorted(st.refs['extern_write']), 'fscommands': len(st.refs['fscommand']),
        'events': len(st.refs['events']),
        'root_event': {n: sorted(v) for n, v in sorted(st.root_event.items())},
        'movie_ids': sorted(st.ids['MOVIE'], key=lambda x: x[1]),
        'hud_states': (len(st.ids['HUD_STATE']), min((v for _n, v in st.ids['HUD_STATE']), default=None),
                       max((v for _n, v in st.ids['HUD_STATE']), default=None)),
        'swf_names_are_movies': swf_ok,
        'ui_assets': len(st.assets), 'movies_without_asset': sorted(st.names - set(st.assets)),
        'source_not_own': sorted(n for n, (_t, s) in st.assets.items() if s.lower() != f'menu/{n}.swf'),
        'config_roots': sorted(n for n, (t, _s) in st.assets.items() if 'UI.UIConfig' in t),
        'modes': len(st.modes),
        'mode_hud_movies': sorted({v for f in st.modes.values() for v in f.values()}),
        'mode_hud_known': all(v in st.names for f in st.modes.values() for v in f.values()),
        'handler_table': st.exe.get('table'), 'handled': len(handled) if handled is not None else None,
        'used_unhandled': sorted(set(st.ops) - handled) if handled is not None else None,
        'dispatch_bytes': st.exe.get('dispatch'), 'fscommand_prefix_len': len(st.exe.get('fscommand') or ''),
        'event_function': st.exe.get('event') == '_root.gameEvent',
        'missing_names': {k: len(v) for k, v in st.exe.get('missing', {}).items()},
    }


# The 2026-11-22 run (game copy of Project OPFOR BFBC2, PC build 795745); the rules of spec/engine/menus.md cite these.
EXPECTED: dict[str, Any] = {
    'entries': 898, 'bundles': 93, 'distinct': 91, 'names': 55,
    'instructions': 1854963, 'opcodes': 91, 'unknown': 0,
    'functions': {'DefineFunction': 5392, 'DefineFunction2': 28108},
    'const_types': {'register': 320102, 'string': 160526},
    'pools': 3563, 'pool_max': 804, 'word_pool': 27247,
    'push_lists_over_1': 0, 'push_register_items': 320102, 'registers_not_used_once': 0,
    'classes': 53, 'dialog_movies': 23, 'registrations': {'guarded': 1285, 'unguarded': 0},
    'imports_unresolved': [],
    'import_sources': ['bartextures_shared', 'insignias', 'menutextures_shared', 'pins', 'platform_buttons',
                       'ranktextures_shared', 'sharedgraphics', 'textmanagercomponent', 'weaponicon'],
    'externs': 198, 'extern_params': 15, 'extern_writes': ['chId', 'chZA'], 'fscommands': 133, 'events': 148,
    'root_event': {'hud_root': [8], 'loading_mp': [6], 'main_root': [6], 'nam_hud_root': [8]},
    'movie_ids': [('MOVIE_HUD_SHARED', 0), ('MOVIE_HUD_GAMEMODE', 1), ('MOVIE_HUD_CROSSHAIR', 2), ('MOVIE_HUD_MENU', 3)],
    'hud_states': (16, 2, 19),
    'swf_names_are_movies': True,
    'ui_assets': 55, 'movies_without_asset': [], 'source_not_own': ['nam_hud_root'],
    'config_roots': ['hud_root', 'main_root', 'nam_hud_root'],
    'modes': 10, 'mode_hud_known': True,
    'handler_table': 185, 'handled': 132, 'used_unhandled': [], 'dispatch_bytes': True, 'fscommand_prefix_len': 10,
    'event_function': True, 'missing_names': {'extern': 9, 'extern_param': 0, 'fscommand': 6},
}


# The rules each recorded count backs (spec/engine/menus.md).
RULES: dict[str, str] = {
    'entries': 'MENU-AS-1', 'bundles': 'MENU-AS-1', 'distinct': 'MENU-AS-1', 'names': 'MENU-AS-1',
    'ui_assets': 'MENU-AS-1', 'movies_without_asset': 'MENU-AS-1', 'source_not_own': 'MENU-AS-1',
    'config_roots': 'MENU-AS-1',
    'instructions': 'MENU-AS-2', 'opcodes': 'MENU-AS-2', 'unknown': 'MENU-AS-2', 'functions': 'MENU-AS-2',
    'const_types': 'MENU-AS-3', 'pools': 'MENU-AS-3', 'pool_max': 'MENU-AS-3', 'word_pool': 'MENU-AS-3',
    'push_lists_over_1': 'MENU-AS-4', 'push_register_items': 'MENU-AS-4', 'registers_not_used_once': 'MENU-AS-4',
    'classes': 'MENU-AS-5', 'registrations': 'MENU-AS-5', 'dialog_movies': 'MENU-AS-5, MENU-DLG-1',
    'imports_unresolved': 'MENU-AS-6', 'import_sources': 'MENU-AS-6',
    'externs': 'MENU-AS-7', 'extern_params': 'MENU-AS-7', 'extern_writes': 'MENU-AS-7', 'fscommands': 'MENU-AS-7',
    'events': 'MENU-AS-7', 'missing_names': 'MENU-AS-7',
    'swf_names_are_movies': 'MENU-AS-8',
    'handler_table': 'MENU-RT-1', 'dispatch_bytes': 'MENU-RT-1',
    'handled': 'MENU-RT-2', 'used_unhandled': 'MENU-RT-2, MENU-ED-2',
    'fscommand_prefix_len': 'MENU-RT-4',
    'event_function': 'MENU-EVT-2', 'root_event': 'MENU-EVT-2',
    'modes': 'MENU-HUD-1', 'mode_hud_known': 'MENU-HUD-1',
    'movie_ids': 'MENU-HUD-2', 'hud_states': 'MENU-HUD-3',
}


def checks(game: Path) -> list[tuple[bool, str]]:
    """(ok, description) per stable count the rules of spec/engine/menus.md cite; each description names its rules."""
    got = summary(analyse(game))
    return [(got.get(k) == want, f'{RULES.get(k, "menus")}: {k} = {want!r}'
             + ('' if got.get(k) == want else f' (got {got.get(k)!r})')) for k, want in EXPECTED.items()]


# ------------------------------------------------------------------------------------------------ the report

def report(st: Stats) -> None:
    s = summary(st)
    print(f'== corpus: {st.entries} SwfMovie entries in {st.holding} DICE bundles, {len(st.movies)} distinct movies, '
          f'{len(st.names)} names')
    versions = collections.Counter(f.name for f in st.movies)
    print('names with more than one version:', sum(1 for v in versions.values() if v > 1))
    handled = st.exe.get('handled', set())
    print(f'== byte code: {st.instructions} instructions, {len(st.ops)} opcodes used, {st.unknown} unknown')
    for op, n in sorted(st.ops.items(), key=lambda kv: -kv[1]):
        print(f'  {op:02X} {apt.NAMES.get(op, "?"):<22} {n}')
    print('  named by apt.NAMES, never used:', ' '.join(f'{op:02X}' for op in sorted(set(apt.NAMES) - set(st.ops))))
    if handled:
        print(f'  interpreter: handler table of {st.exe["table"]} opcodes, {len(handled)} with a handler; dispatch '
              f'bytes as read: {st.exe["dispatch"]}')
        print('  used without a handler:', s['used_unhandled'])
        print('  named, no handler:', ' '.join(f'{op:02X}' for op in sorted(set(apt.NAMES) - handled)))
        print('  handler, not named:', ' '.join(f'{op:02X}' for op in sorted(handled - set(apt.NAMES))))
        print('  handler, never used:', ' '.join(f'{op:02X}' for op in sorted(handled - set(st.ops))))
    print('function definitions:', dict(st.functions))
    print(f'== constants: entry types {dict(st.const_types)}; ConstantPool instructions {len(st.pools)}, sizes '
          f'max {s["pool_max"]}, median {sorted(st.pools)[len(st.pools) // 2] if st.pools else 0}; '
          f'pools over 256: {sum(1 for p in st.pools if p > 256)}; EA_PushWordConstant uses {st.word_pool}')
    print(f'  PushData lists by item count {dict(st.push_lists)}; items by constant kind {dict(st.push_items)}; '
          f'register constants by PushData uses (0, 1, 2+) {[st.reg_uses[k] for k in range(3)]}')
    print(f'== classes: {len(st.classes)} __Packages exports of {st.exports} exports; by root package {s["class_roots"]}; '
          f'registrations (per distinct movie) {s["registrations"]}, unguarded {dict(st.unguarded)}')
    for c, ms in sorted(st.classes.items()):
        print(f'  {c}: {len(ms)} movies')
    print('== imports (source movie: symbols, importing movies)')
    by_src: dict[str, set[str]] = collections.defaultdict(set)
    users: dict[str, set[str]] = collections.defaultdict(set)
    for (src, sym), ms in st.imports.items():
        by_src[src].add(sym)
        users[src] |= ms
    for src in sorted(by_src):
        print(f'  {src}: {len(by_src[src])} symbols, imported by {len(users[src])} movies: {", ".join(sorted(users[src]))}')
    print('== engine interface')
    for kind in ('extern', 'extern_param', 'extern_write', 'fscommand', 'events'):
        print(f'  {kind}: {len(st.refs[kind])} names')
        for name, ms in sorted(st.refs[kind].items()):
            how = dict(st.handlers[name]) if kind == 'events' else ''
            print(f'    {name} ({len(ms)}: {", ".join(sorted(ms))}) {how}')
    if 'missing' in st.exe:
        print(f'  the exe name table: {st.exe["names"]} names; FSCommand prefix {st.exe["fscommand"]!r}; '
              f'event function {st.exe["event"]!r}')
        for kind, ms in st.exe['missing'].items():
            print(f'  {kind} names not in the exe table: {len(ms)}: {", ".join(ms)}')
    print('  root gameEvent(n params):', s['root_event'])
    for k, v in sorted(st.ids.items()):
        print(f'  {k} ids:', sorted(v, key=lambda x: x[1]))
    print('== movie loads')
    print('  call sites:', dict(st.loaders))
    for (callee, target), ms in sorted(st.load_targets.items()):
        print(f'    {callee} {target}: {", ".join(sorted(ms))}')
    print(f'  imports naming a symbol the source movie does not export: {s["imports_unresolved"]}')
    print('  <name>.swf constants per movie (all name a movie of the corpus: %s):' % s['swf_names_are_movies'])
    for name, sw in sorted(st.swf.items()):
        print(f'    {name}: {", ".join(sorted(sw))}')
    print(f'  UI.UIAsset partitions: {len(st.assets)}; movies without one: {s["movies_without_asset"]}; source file '
          f'not Menu/<own name>.swf: {s["source_not_own"]}; with a UI.UIConfig (root movies): {s["config_roots"]}')
    print(f'== HUD movies per game mode ({len(st.modes)} GameModeData)')
    for mode, f in sorted(st.modes.items()):
        print(f'  {mode}: ' + ', '.join(f'{k[:-4]}={v}' for k, v in f.items()))


def main() -> None:
    ap = argparse.ArgumentParser(description=(__doc__ or '').split('\n')[0])
    ap.add_argument('--opfor')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor, start=str(SUITE))))
    t = time.perf_counter()
    st = analyse(game)
    report(st)
    bad = [d for ok, d in [(summary(st).get(k) == v, k) for k, v in EXPECTED.items()] if not ok]
    print(f'checks: {len(EXPECTED) - len(bad)} of {len(EXPECTED)} as recorded' + (f'; differ: {bad}' if bad else ''))
    print(f'({time.perf_counter() - t:.1f} s, {apt.backend()} decoder)')


if __name__ == '__main__':
    main()
