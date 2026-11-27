"""The executable's reflection, record by record: every `typeinfo` record's parent, members, values and the entity
factories that make the run-time classes (read-only; spec/engine/typeinfo/*.md RTTI-*, round SPG4-rest).

    python spec/probes/typeinfo.py [--opfor <folder>] --measure   re-read BFBC2Game.exe into spec/typeinfo.json
    python spec/probes/typeinfo.py --write                         the tables of spec/engine/typeinfo/*.md
    python spec/probes/typeinfo.py --check                         (no game) the tables are current and every
                                                                   record is in one group

Each record (ENG-TI-1) is registered by a static initialiser `push <args>; push <record>; mov ecx, <class info>;
call <register>` (RTTI-REG-1): the data classes with their parent's class info, a creator and their member table,
the run-time classes with their parent only, the value types and enumerations with the member table their record
holds. A member is a 24-byte descriptor (RTTI-MEM-1). The entity factories of the server's and the client's
registries (SCR-EVT-3) are followed to the constructor they call; the last vtable it stores names the run-time class
(each class's `getType` is `mov eax, <class info>; ret`), and the factory's size slot is checked against that class's
record size (RTTI-FAC-1).

spec/typeinfo.json holds names, sizes and offsets only (identifiers of the executable's reflection, as
spec/census.json does; no game data). `checks(game)` re-reads the executable for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import re
import struct
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
SPEC = SUITE / 'spec'
TYPEINFO_JSON = SPEC / 'typeinfo.json'
TABLE_DIR = SPEC / 'engine' / 'typeinfo'
BUILD_STAMP = 0x4E55445D                     # BFBC2Game.exe build 795745
IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*')   # census.py's

# RTTI-REG-1: the registration functions and what they register
REGISTER = {0x00500880: 'data class', 0x00500660: 'class', 0x0043C390: 'value type', 0x005008E0: 'enumeration',
            0x004FE260: 'built-in'}
KINDS = {3: 'class', 2: 'value type', 0x17: 'enumeration', 4: 'array', 0: 'void'}
ARRAY_FLAG = 0x40                              # RTTI-MEM-1: a member's flags word (0x48 on arrays)
# RTTI-FAC-1 / SCR-EVT-3: the factory registries (registry object, registration function) and the vtable slot of
# each side's factory that constructs the entity (the server's create calls slot 4, the client's slot 6)
REGISTRIES = {'server': (0x0157041C, 0x0071C790, 4), 'client': (0x01572768, 0x008B6A60, 6)}

EXPECTED: dict[str, Any] = {
    'records': 1900,
    'registrations': {'built-in': 19, 'class': 400, 'data class': 961, 'enumeration': 231, 'value type': 289},
    'members': 7087, 'array_members': 564, 'enum_values': 1952,
    'factories': {'client': 100, 'server': 148}, 'factory_pairs': {'client': 75, 'server': 110}, 'made_classes': 173,
}

Records = dict[str, dict[str, Any]]


# ------------------------------------------------------------------------------------------------ the measurement

def _image(exe: Path) -> Any:
    if str(SUITE) not in sys.path:
        sys.path.insert(0, str(SUITE))
    import suite_paths  # noqa: PLC0415
    suite_paths.use('core', 'menu-tools')
    import menucommon  # noqa: PLC0415
    return menucommon.PEImage(os.fspath(exe))


class Exe:
    def __init__(self, img: Any) -> None:
        self.img, self.b = img, img.b

    def u32(self, va: int) -> int | None:
        o = self.img.off(va) if 0x400000 <= va < 0x2000000 else None
        return None if o is None else int(struct.unpack_from('<I', self.b, o)[0])

    def ident(self, va: int) -> str | None:
        if not 0x400000 <= va < 0x2000000:
            return None
        s = self.img.cstr(va, 120)
        return s if s and IDENT.fullmatch(s) else None

    def records(self) -> dict[int, tuple[str, str, int, int, bytes]]:
        """ENG-TI-1's walk: {record va: (name, module, flags, size, the 28 raw bytes)}."""
        sec = next(s for s in self.img.sections if s[0] == 'typeinfo')
        _, _sva, _vs, ra, rs, _ = sec
        out: dict[int, tuple[str, str, int, int, bytes]] = {}
        names: set[str] = set()
        for o in range(ra, ra + rs - 12, 4):          # census.py's walk
            p, sf, m = struct.unpack_from('<III', self.b, o)
            n = self.ident(p)
            mp = self.u32(m) if n else None
            mod = self.ident(mp) if mp is not None else None
            if n and mod and n not in names:
                names.add(n)
                out[self.img.va(o)] = (n, mod, sf & 0xFFFF, sf >> 16, bytes(self.b[o:o + 28]))
        return out

    def registrations(self, recs: dict[int, Any]) -> dict[int, tuple[int, int, list[int]]]:
        """RTTI-REG-1: {record va: (register function, class info, the pushes before the record's, nearest first)}."""
        out: dict[int, tuple[int, int, list[int]]] = {}
        for m in re.finditer(rb'\x68(....)\xb9(....)\xe8(....)', self.b, re.S):
            rec = struct.unpack('<I', m.group(1))[0]
            if rec not in recs:
                continue
            va = self.img.va(m.start())
            if va is None or not self.img.is_code(va):
                continue
            fn = va + 15 + struct.unpack('<i', m.group(3))[0]
            if fn not in REGISTER:
                continue
            pushes: list[int] = []
            p = m.start()
            while len(pushes) < 3:
                if self.b[p - 5] == 0x68:
                    pushes.append(struct.unpack_from('<I', self.b, p - 4)[0])
                    p -= 5
                elif self.b[p - 2] == 0x6A:
                    pushes.append(self.b[p - 1])
                    p -= 2
                else:
                    break
            out[rec] = (fn, struct.unpack('<I', m.group(2))[0], pushes)
        return out


def measure(exe: Path) -> dict[str, Any]:
    img = _image(exe)
    if img.timestamp != BUILD_STAMP:
        return {}
    x = Exe(img)
    recs = x.records()
    regs = x.registrations(recs)
    info: dict[int, str] = {ci: recs[r][0] for r, (_fn, ci, _p) in regs.items()}
    byva = {va: r[0] for va, r in recs.items()}

    def tname(p: int) -> str:
        return byva.get(p) or info.get(p) or f'0x{p:X}'

    out: Records = {}
    for va, (n, mod, flags, size, raw) in recs.items():
        fn, _ci, pushes = regs.get(va, (0, 0, []))
        d: dict[str, Any] = {'module': mod, 'kind': KINDS.get((flags >> 4) & 0x1F, 'primitive'), 'size': size,
                             'registered': REGISTER.get(fn, '')}
        table = 0
        if fn == 0x00500880 and len(pushes) == 3:
            d['parent'] = tname(pushes[0]) if pushes[0] else None
            table = pushes[2]
        elif fn == 0x00500660 and pushes:
            d['parent'] = tname(pushes[0]) if pushes[0] else None
        elif fn in (0x0043C390, 0x005008E0):
            table = struct.unpack_from('<I', raw, 24)[0]
        if d.get('parent') == n:                  # a root class names its own class info as its parent
            d['parent'] = None
        count = raw[13]
        if table and count:
            o = img.off(table)
            rows = [struct.unpack_from('<6I', x.b, o + 24 * i) for i in range(count)]
            if d['kind'] == 'enumeration':
                d['values'] = [[x.ident(w[0]), w[4]] for w in rows]
            else:
                d['fields'] = [[x.ident(w[0]), f'{tname(w[3])}[]' if w[1] & ARRAY_FLAG else tname(w[2]), w[4]]
                               for w in rows]
        out[n] = d
    made = factories(x, info, {r[0]: r[3] for r in recs.values()})
    for side, rows in made.items():
        for data, ent in rows:
            out[ent].setdefault(side, []).append(data)
    for d in out.values():
        for side in REGISTRIES:
            if side in d:
                d[side] = sorted(set(d[side]))
    return {'records': dict(sorted(out.items())),
            'factories': {side: len(v) for side, v in _factory_sites(x).items()}}


def _factory_sites(x: Exe) -> dict[str, list[tuple[int, int]]]:
    """SCR-EVT-3's registrations: {side: [(call site va, factory vtable)]}."""
    out: dict[str, list[tuple[int, int]]] = {}
    for side, (reg, fn, _slot) in REGISTRIES.items():
        rows = out[side] = []
        pat = b'\xb9' + struct.pack('<I', reg) + b'\xe8'
        for m in re.finditer(re.escape(pat), x.b):
            va = x.img.va(m.start())
            if va is None or va + 10 + struct.unpack_from('<i', x.b, m.start() + 6)[0] != fn:
                continue
            win = x.b[m.start() - 40:m.start()]
            k = win.rfind(b'\xc7\x00')
            rows.append((va, struct.unpack_from('<I', win, k + 2)[0] if k >= 0 else 0))
    return out


def factories(x: Exe, info: dict[int, str], sizes: dict[str, int]) -> dict[str, list[tuple[str, str]]]:
    """RTTI-FAC-1: {side: [(data class, run-time class)]}: the factory's slot 1 names the data class (`mov eax,
    <class info>; ret`), its construct slot's constructor stores the run-time class's vtable last; kept when the
    factory's size slot (3: `mov eax, <size>; ret 4`) is the run-time record's size, or has no constant."""
    import capstone  # noqa: PLC0415

    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    gettype: dict[int, str] = {}
    for m in re.finditer(rb'\xb8(....)\xc3', x.b, re.S):
        ci = struct.unpack('<I', m.group(1))[0]
        va = x.img.va(m.start())
        if ci in info and va is not None and x.img.is_code(va):
            gettype[va] = info[ci]

    def const(fn: int | None, tail: bytes) -> int | None:
        o = x.img.off(fn) if fn else None
        if o is None or x.b[o] != 0xB8 or x.b[o + 5:o + 5 + len(tail)] != tail:
            return None
        return int(struct.unpack_from('<I', x.b, o + 1)[0])

    def vt_class(v: int) -> str | None:
        for i in range(60):
            f = x.u32(v + 4 * i)
            if f is None or not x.img.is_code(f):
                return None
            if f in gettype:
                return gettype[f]
        return None

    def ctor_class(fn: int, depth: int = 0) -> str | None:
        o = x.img.off(fn)
        if o is None:
            return None
        best: str | None = None
        calls: list[int] = []
        for n, i in enumerate(md.disasm(x.b[o:o + 0x3000], fn)):
            ops = i.op_str
            if i.mnemonic == 'mov' and re.fullmatch(r'dword ptr \[e[a-z]{2}\], 0x[0-9a-f]+', ops):
                c = vt_class(int(ops.split(', ')[1], 16))
                best = c or best
            elif i.mnemonic in ('call', 'jmp') and ops.startswith('0x'):
                if i.mnemonic == 'jmp' and best is None and depth < 2:
                    return ctor_class(int(ops, 16), depth + 1)
                calls.append(int(ops, 16))
            if i.mnemonic == 'ret' or n >= 300:
                break
        if best is None and depth == 0:
            for t in calls:
                best = best or ctor_class(t, 1)
        return best

    out: dict[str, list[tuple[str, str, int | None]]] = {}
    for side, sites in _factory_sites(x).items():
        rows = out[side] = []
        slot = REGISTRIES[side][2]
        for _va, vt in sites:
            data = info.get(const(x.u32(vt + 4), b'\xc3') or 0)
            size = const(x.u32(vt + 12), b'\xc2\x04\x00')
            cands = [slot] + [j for j in range(2, 10) if j != slot]
            for j in cands:
                f = x.u32(vt + 4 * j)
                if f is None or not x.img.is_code(f):
                    break
                ent = ctor_class(f)
                if data and ent and not ent.endswith('Data') and ent not in ('ServerEntity', 'ClientEntity'):
                    rows.append((data, ent, size))
                    break
    return {side: [(d, e) for d, e, s in rows if s is None or s == sizes.get(e)] for side, rows in out.items()}


def counts(ti: dict[str, Any]) -> dict[str, Any]:
    recs: Records = ti['records']
    reg = collections.Counter(d['registered'] for d in recs.values())
    fields = [f for d in recs.values() for f in d.get('fields', [])]
    return {'records': len(recs), 'registrations': dict(sorted(reg.items())), 'members': len(fields),
            'array_members': sum(1 for f in fields if f[1].endswith('[]')),
            'enum_values': sum(len(d.get('values', [])) for d in recs.values()),
            'factories': ti['factories'],
            'factory_pairs': {side: sum(len(d.get(side, [])) for d in recs.values()) for side in sorted(REGISTRIES)},
            'made_classes': sum(1 for d in recs.values() if 'server' in d or 'client' in d)}


# ------------------------------------------------------------------------------------------------ the tables

def chain(recs: Records, n: str) -> list[str]:
    out: list[str] = []
    while n in recs and n not in out:
        out.append(n)
        n = recs[n].get('parent') or ''
    return out


def dbx_fields(recs: Records, n: str) -> set[str]:
    return {f[0] for c in chain(recs, n) for f in recs[c].get('fields', [])}


def _gsr(recs: Records, n: str, kind: str) -> bool:
    d = recs[n]
    return d['module'] == 'GameSharedResources' and d['kind'] == kind


THEMES: list[tuple[str, str]] = [
    ('weapons', r'Weapon|Gun|Fir(e|ing)|Recoil|Ammo|Zoom|Shot|Bolt|Reload|OverHeat|Projectile|Missile|Grenade|'
                r'Bullet|Explosion|Dispersion|Sway|Lag|Melee|Medkit|Repair|Laser|Artillery|Binocular|Locking|'
                r'PowerTool|Defib|Callback|Detonat|Tracer|HoldAndRelease|Turn|Rumble|Armament|Damage|Impact'),
    ('ai', r'^AI|Cover|Path|Edge|Behaviou?r|Urgency|Squad|Order|Bot|Sensing|Readiness|VO[A-Z]|^VO|Waypoint|'
           r'Garrison|Kynapse|Navigation|Formation|Aiming|Interlude|TargetPos|Pose'),
    ('ui', r'^UI|Hud|Menu|Credits|Loading|Subtitle|KitImage|MapMarker|Minimap|Tooltip|Intro|Eula|GameTip|'
           r'Vignette|Blur|ColorTransform|FrameEffect|HealthVignette|Version|Debug|Movie'),
    ('persistence', r'Award|Persist|Stats|Scor|Achievement|MatchMaking|Online|RichPresence|Elo|Entitlement|'
                    r'Unlock|Rpg|Collectible|Dog|Specialization|Criteria|Star|Text|Chat|AntiSpam|Setting|Mode'),
]


def theme(n: str) -> str:
    for t, pat in THEMES:
        if re.search(pat, n):
            return t
    return 'other'


def _entity(recs: Records, n: str) -> bool:
    return 'EntityData' in chain(recs, n)[1:] or n == 'EntityData'


# (file, group key) -> which records the group's table lists, in order
GROUPS: list[tuple[str, str, Callable[[Records, str], bool]]] = [
    ('core.md', 'core', lambda r, n: r[n]['module'] == 'Core'),
    ('game_shared.md', 'gsr-enums', lambda r, n: _gsr(r, n, 'enumeration')),
    ('game_shared.md', 'gsr-values', lambda r, n: _gsr(r, n, 'value type')),
    ('game_shared.md', 'gsr-entities', lambda r, n: _gsr(r, n, 'class') and _entity(r, n)),
    ('game_shared.md', 'gsr-weapons',
     lambda r, n: _gsr(r, n, 'class') and not _entity(r, n) and theme(n) == 'weapons'),
    ('game_shared.md', 'gsr-ai', lambda r, n: _gsr(r, n, 'class') and not _entity(r, n) and theme(n) == 'ai'),
    ('game_shared.md', 'gsr-ui', lambda r, n: _gsr(r, n, 'class') and not _entity(r, n) and theme(n) == 'ui'),
    ('game_shared.md', 'gsr-persistence',
     lambda r, n: _gsr(r, n, 'class') and not _entity(r, n) and theme(n) == 'persistence'),
    ('game_shared.md', 'gsr-other', lambda r, n: _gsr(r, n, 'class') and not _entity(r, n) and theme(n) == 'other'),
    ('gamedll.md', 'gamedll-server', lambda r, n: r[n]['module'] == 'GameDll' and not n.startswith('Client')),
    ('gamedll.md', 'gamedll-client', lambda r, n: r[n]['module'] == 'GameDll' and n.startswith('Client')),
]


def _module(m: str, classes: bool) -> Callable[[Records, str], bool]:
    return lambda r, n: r[n]['module'] == m and (r[n]['kind'] == 'class') == classes


for _mod in ('Animation', 'Audio', 'Decal', 'Emitter', 'EmitterRender', 'Entity', 'Game', 'Overgrowth',
             'OvergrowthRender', 'Physics', 'Render', 'Scene', 'Terrain', 'TerrainRender', 'UI', 'Weather', 'World',
             'WorldRender', 'Aidll'):
    GROUPS.append(('modules.md', f'{_mod.lower()}-types', _module(_mod, False)))
    GROUPS.append(('modules.md', f'{_mod.lower()}-classes', _module(_mod, True)))


def _fields(d: dict[str, Any]) -> str:
    fs = sorted(d.get('fields', []), key=lambda f: (f[2], f[0]))
    return ', '.join(f'{f[0]}: {f[1]} @{f[2]}' for f in fs) or '-'


def table(recs: Records, names: list[str], key: str) -> list[str]:
    kinds = {recs[n]['kind'] for n in names}
    if kinds == {'enumeration'}:
        rows = ['| Enumeration | Values |', '|---|---|']
        rows += [f"| {n} | {', '.join(f'{v[0]} {v[1]}' for v in sorted(recs[n]['values'], key=lambda v: v[1]))} |"
                 for n in names]
    elif key.startswith('gamedll'):
        rows = ['| Class | Parent | Size | Made from (factory) |', '|---|---|---|---|']
        for n in names:
            d = recs[n]
            made = ', '.join(f'{side}: {", ".join(d[side])}' for side in REGISTRIES if side in d) or '-'
            rows.append(f"| {n} | {d.get('parent') or '-'} | {d['size']} | {made} |")
    else:
        rows = ['| Type | Kind | Parent | Size | Own members (name: type @offset) or values |', '|---|---|---|---|---|']
        for n in names:
            d = recs[n]
            body = (', '.join(f'{v[0]} {v[1]}' for v in sorted(d['values'], key=lambda v: v[1]))
                    if d['kind'] == 'enumeration' else _fields(d))
            rows.append(f"| {n} | {d['kind']} | {d.get('parent') or '-'} | {d['size']} | {body} |")
    return rows


def groups(recs: Records) -> dict[str, tuple[str, list[str]]]:
    """{group key: (file, the records it lists)}; a record goes to the first group that takes it."""
    out: dict[str, tuple[str, list[str]]] = {k: (f, []) for f, k, _ in GROUPS}
    for n in sorted(recs):
        for _f, k, sel in GROUPS:
            if sel(recs, n):
                out[k][1].append(n)
                break
    return out


MARK = re.compile(r'(<!-- typeinfo:([a-z0-9-]+) -->\n)(.*?)(<!-- /typeinfo -->)', re.S)


def render_file(text: str, recs: Records, grp: dict[str, tuple[str, list[str]]]) -> str:
    def sub(m: re.Match[str]) -> str:
        k = m.group(2)
        names = grp[k][1] if k in grp else []
        body = '\n'.join(table(recs, names, k)) + '\n' if names else '(none)\n'
        return m.group(1) + body + m.group(4)
    return MARK.sub(sub, text)


def load() -> dict[str, Any]:
    with open(TYPEINFO_JSON, encoding='utf-8') as f:
        return dict(json.load(f))


def dump(ti: dict[str, Any]) -> None:
    lines = ['{', f' "factories": {json.dumps(ti["factories"], sort_keys=True)},', ' "records": {']
    keys = sorted(ti['records'])
    for j, k in enumerate(keys):
        comma = ',' if j < len(keys) - 1 else ''
        rec = json.dumps(ti['records'][k], sort_keys=True, separators=(',', ':'))
        lines.append(f'  {json.dumps(k, sort_keys=True)}: {rec}{comma}')
    lines += [' }', '}']
    TYPEINFO_JSON.write_text('\n'.join(lines) + '\n', encoding='utf-8')


def problems(ti: dict[str, Any]) -> list[str]:
    recs = ti['records']
    grp = groups(recs)
    out = []
    placed = {n for _f, names in grp.values() for n in names}
    out += [f'{n}: in no group of spec/engine/typeinfo' for n in sorted(set(recs) - placed)]
    files = sorted({f for f, _k, _s in GROUPS})
    seen: set[str] = set()
    for f in files:
        p = TABLE_DIR / f
        text = p.read_text(encoding='utf-8') if p.is_file() else ''
        keys = [m.group(2) for m in MARK.finditer(text)]
        seen.update(keys)
        out += [f'{f}: no table marker for group {k}' for k, (gf, ns) in grp.items() if gf == f and ns and k not in keys]
        if text and render_file(text, recs, grp) != text:
            out.append(f'{f}: tables not current (python spec/probes/typeinfo.py --write)')
    return out


def write() -> None:
    ti = load()
    grp = groups(ti['records'])
    for f in sorted({f for f, _k, _s in GROUPS}):
        p = TABLE_DIR / f
        p.write_text(render_file(p.read_text(encoding='utf-8'), ti['records'], grp), encoding='utf-8')
        print(p)


def check() -> int:
    bad = problems(load())
    for p in bad:
        print('FAIL  ' + p)
    print('passed' if not bad else f'FAILED {len(bad)}')
    return 1 if bad else 0


def checks(game: Path) -> list[tuple[bool, str]]:
    """The game tier: the executable's reflection is what spec/typeinfo.json holds, with RTTI-*'s counts."""
    exe = game / 'BFBC2Game.exe'
    if not exe.is_file():
        return [(False, f'typeinfo: no {exe}')]
    got, want = measure(exe), load()
    diff = sorted(k for k in set(got.get('records', {})) | set(want['records'])
                  if got.get('records', {}).get(k) != want['records'].get(k))
    out = [(not diff, f'typeinfo: records as spec/typeinfo.json holds them (differ: {diff[:8]})')]
    c = counts(got) if got else {}
    out += [(c.get(k) == v, f'typeinfo: {k} = {v!r} (got {c.get(k)!r})') for k, v in EXPECTED.items()]
    out += [(v == want, f'typeinfo: {k}') for k, v, want in dbx_agreement(got.get('records', {}))]
    lay = layout(got.get('records', {}))
    out.append((lay == {'past_end': 0, 'below_parent': 0}, f'typeinfo: RTTI-MEM-1 layout {lay} (both 0)'))
    return out


# RTTI-DBX-1: the run-time members against the fields of DICE's DBX classes (spec/census.json)
DBX_EXPECTED = {'classes': 630, 'same': 398, 'dbx_more': 230, 'differ': 2, 'editor_only_names': 118}
# RTTI-DBX-2: DBX fields the run-time classes do not have
PIPELINE_ONLY = {
    'StaticModelEntityData': ['AllowExplosiveTracking', 'AllowPathdataSeeds', 'DetailedTreeBanger',
                              'EdgeModelGenerationSettings', 'ExcludeFromPathfinding', 'ForbidPathfinding',
                              'LocalPathfinding', 'NeedsDetailedPathfinding', 'RequiresMesh'],
    'RigidModelEntityData': ['ExcludeFromPathfinding', 'ForbidPathfinding'],
    'SensingTemplateData': ['CombatFOVMultiplier', 'HearingDistance', 'PeripheralViewAngle', 'ReadyFOVMultiplier',
                            'SeeingDistance', 'SensingSkill', 'ViewAngle'],
    'TeamEntityData': ['Squads'], 'LevelData': ['Parts'],
    'GameModeData': ['Crosshairs', 'HudRoot', 'InGameMenu', 'Loader', 'PrimaryLogicLayer', 'SecondaryLogicLayer'],
}


def dbx_agreement(recs: Records) -> list[tuple[str, Any, Any]]:
    got = dbx_compare(recs)
    out: list[tuple[str, Any, Any]] = [(f'RTTI-DBX-1 {k} = {v}', got.get(k), v) for k, v in DBX_EXPECTED.items()]
    with open(SPEC / 'census.json', encoding='utf-8') as f:
        dbx = json.load(f)['dbx']
    for n, fields in PIPELINE_ONLY.items():
        only = sorted(set(dbx.get(n, {}).get('fields', [])) - dbx_fields(recs, n))
        out.append((f'RTTI-DBX-2 {n}: {", ".join(fields)} in the DBX, not at run time', [f for f in fields if f in only],
                    fields))
    return out


def layout(recs: Records) -> dict[str, int]:
    """RTTI-MEM-1: members past their record's end, and own members below the parent's size (both 0)."""
    past = below = 0
    for d in recs.values():
        parent = recs.get(d.get('parent') or '')
        for _name, ty, off in d.get('fields', []):
            t = recs.get(ty)
            size = 4 if ty.endswith('[]') or t is None or t['kind'] == 'class' else t['size']
            past += off + size > d['size']
            below += parent is not None and off < parent['size']
    return {'past_end': past, 'below_parent': below}


def dbx_compare(recs: Records) -> dict[str, Any]:
    with open(SPEC / 'census.json', encoding='utf-8') as f:
        dbx = json.load(f)['dbx']
    same = more = differ = 0
    extra: collections.Counter[str] = collections.Counter()
    odd: list[str] = []
    for n, d in dbx.items():
        df, ef = set(d['fields']), dbx_fields(recs, n)
        if df == ef:
            same += 1
        elif df > ef:
            more += 1
            extra.update(df - ef)
        else:
            differ += 1
            odd.append(n)
    return {'classes': len(dbx), 'same': same, 'dbx_more': more, 'differ': differ, 'editor_only_names': len(extra),
            '_extra': extra, '_odd': odd}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    ap.add_argument('--measure', action='store_true', help='re-read the executable into spec/typeinfo.json')
    ap.add_argument('--write', action='store_true', help='write the tables of spec/engine/typeinfo/*.md')
    ap.add_argument('--check', action='store_true', help='(no game) the tables are current')
    ap.add_argument('--dbx', action='store_true', help='print RTTI-DBX-1\'s comparison')
    a = ap.parse_args()
    if a.measure:
        sys.path.insert(0, str(SUITE / 'core'))
        from bfbc2core import paths  # noqa: PLC0415
        game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
        t = time.time()
        ti = measure(game / 'BFBC2Game.exe')
        dump(ti)
        print(f'{TYPEINFO_JSON} ({time.time() - t:.1f} s): {counts(ti)}')
    if a.dbx:
        got = dbx_compare(load()['records'])
        print({k: v for k, v in got.items() if not k.startswith('_')})
        print('editor-only names:', ', '.join(f'{k} {v}' for k, v in sorted(got['_extra'].items())))
        print('differ:', got['_odd'])
    if a.write:
        write()
    if a.check or not (a.measure or a.write or a.dbx):
        return check()
    return 0


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    sys.exit(main())
