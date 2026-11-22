"""Grime, dirt and camouflage as the game copy ships them: the shader databases' dirt and material-mask slots and
dirt constants, the terrain materials' dirt triggers, the meshes' shader sets and who picks one, and the camouflage
textures each level binds (read-only; spec/engine/materials.md section 5, MAT-TEX-*).

    python spec/probes/grime.py [--opfor <folder>]

Reads the level bundles of Project OPFOR BFBC2's game copy (`game/**/levels/*/*.fbrb`) dated before 2013 (DICE's
files, as the install leaves them), each resource or partition once from the first bundle that holds it, and the
retail `BFBC2Game.exe` beside them:

- shader databases: every DICE `Dx11ShaderDatabase` (one per level). Each entry (`<mesh>/<shader>`) is taken once,
  from the first database that holds it, except where a count is per database (the camouflage per level). A program's
  textures are paired with its pixel shader's own names for them (the DXBC `RDEF` texture bindings: bind point ->
  `texture_<name>`), so a texture record's sampler register reads as `texture_DirtMask2`, `texture_Texture3` ...; the
  dirt constants are read from both parameter blocks of every program;
- DBX: every DICE partition naming a shader set (`ShaderSets`, `MeshShaderSet`, `MeshShaderSetNumber` ...): the mesh
  assets' `ShaderSets` and their `MeshShaderSetAsset` items, the entity data fields that pick a set, and the
  `materials_win32` grids' `MaterialPropertyTerrainData` (the dirt triggers) with the relation each hangs from, and
  how many partitions reference a soldier camo texture's partition at all;
- the executable: whether the dirt constants' names stand in the run of per-instance shader constant names.

Prints structure, counts, value ranges and identifiers only (class, field, slot, constant, material, level and
texture names; no text, no bytes). `checks(game)` recomputes the stable counts the MAT-TEX rules cite, for
core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import re
import struct
import sys
import time
from collections.abc import Iterator
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
sys.path.insert(0, str(SUITE / 'bfbc2-shader-tools'))
import shaderdb  # noqa: E402
from bfbc2core import dbx, fbrb, paths  # noqa: E402

DIRT_SLOT = re.compile(r'^texture_(Dirt|DirtMask|MaterialMask)\d*$')
DIRT_CONSTS = ('DirtColor', 'DirtFactor', 'DirtColor2', 'DirtFactor2', 'Destruction_factor', 'Destruction_factor2')
SET_FIELDS = ('MeshShaderSetNumber', 'MeshShaderSetNumberOverride', 'UseTeamShaderSet')
US_CAMO = ('Camo_Digital', 'Camo_US1981Woodland', 'Camo_Snow_1')
RU_CAMO = ('Camo_RUS_04', 'Camo_RUS_03', 'Camo_RUS_gorod')
# a DBX reference names the partition (lower case, '/'): the soldiers' camo textures' partitions
CAMO_REFS = tuple(f'characters/{s}_textures/{n.lower()}'.encode() for s, ns in (('us/us', US_CAMO), ('rus/rus', RU_CAMO))
                  for n in ns)
DT_CAMO = ('Dt_Camo_Desert', 'Dt_Camo_Wood', 'Dt_Camo_Snow')
THEME_OF = {'Camo_Digital': 'Dt_Camo_Desert', 'Camo_US1981Woodland': 'Dt_Camo_Wood', 'Camo_Snow_1': 'Dt_Camo_Snow'}
EXE_RUN = (b'destruction_factor', b'trackOffset', b'dirtColor', b'dirtFactor')


def bundles(game: Path) -> list[Path]:
    """DICE's level bundles (dated before 2013), in path order."""
    return sorted((p for p in game.glob('**/levels/*/*.fbrb') if time.gmtime(p.stat().st_mtime).tm_year < 2013),
                  key=lambda p: str(p).lower())


def first_seen(items: list[Path], want: Any) -> Iterator[tuple[Path, list[str]]]:
    """Per bundle, the entry names `want(lower name)` accepts that no earlier bundle held."""
    seen: set[str] = set()
    for path in items:
        names = []
        for e in fbrb.entries(path):
            low = e.name.lower()
            if e.deleted or low in seen or not want(low):
                continue
            seen.add(low)
            names.append(e.name)
        if names:
            yield path, names


def kind(name: str) -> str:
    """The class of a mesh or entry by its path: soldiers, vehicles, weapons, buildings, objects, terrain, vegetation,
    other."""
    low = name.lower()
    for prefix, k in (('characters/', 'soldiers'), ('objects/vehicles/', 'vehicles'), ('objects/weapons/', 'weapons'),
                      ('objects/buildings/', 'buildings'), ('objects/', 'objects'), ('terrains/', 'terrain'),
                      ('vegetation/', 'vegetation')):
        if low.startswith(prefix):
            return k
    return 'other'


def leaf(texture: str) -> str:
    return texture.rsplit('/', 1)[-1]


def rdef_textures(dxbc: bytes) -> dict[int, str]:
    """{bind point: name} of the texture resources (type 2) a DXBC shader's RDEF chunk binds (renderqa.resources)."""
    if len(dxbc) < 32 or dxbc[:4] != b'DXBC':
        return {}
    n = struct.unpack_from('<I', dxbc, 28)[0]
    for i in range(n):
        off = struct.unpack_from('<I', dxbc, 32 + 4 * i)[0]
        if dxbc[off:off + 4] != b'RDEF':
            continue
        r = dxbc[off + 8:off + 8 + struct.unpack_from('<I', dxbc, off + 4)[0]]
        if len(r) < 28:
            return {}
        nb, bo = struct.unpack_from('<2I', r, 8)
        out: dict[int, str] = {}
        for k in range(nb):
            no, typ, _, _, _, bp, _, _ = struct.unpack_from('<8I', r, bo + 32 * k)
            if typ == 2:
                out.setdefault(bp, r[no:r.index(b'\0', no)].decode('latin-1'))
        return out
    return {}


def number(e: dbx.Element | None) -> float | None:
    """A 4-byte number read from its binary value (float), or None."""
    if e is None or e.kind != dbx.NUMBERS:
        return None
    size, vals = dbx.numbers(e)
    if size != 4 or len(vals) != 1:
        return None
    return float(struct.unpack('>f', vals[0])[0])


def rng(vals: list[float]) -> str:
    if not vals:
        return 'none'
    c = collections.Counter(round(v, 4) for v in vals)
    top = ', '.join(f'{v:g} x {n}' for v, n in c.most_common(4))
    return f'{min(vals):g} .. {max(vals):g} ({len(vals)} values, {len(c)} distinct; most: {top})'


class Stats:
    """What the probe counts (filled by shader_dbs, dbx_side, grid and exe_side)."""

    def __init__(self) -> None:
        self.dbs: int = 0
        self.entries: int = 0
        self.entries_by_kind: collections.Counter[str] = collections.Counter()
        self.slot_entries: dict[str, collections.Counter[str]] = {}     # slot -> kind -> entries
        self.slot_textures: dict[str, collections.Counter[str]] = {}    # slot -> texture -> entries
        self.slot_dbs: collections.Counter[str] = collections.Counter()     # slot -> databases using it
        self.const_entries: dict[str, collections.Counter[str]] = {}    # constant -> kind -> entries
        self.const_values: dict[str, collections.Counter[tuple[float, ...]]] = {}
        self.dirt_named: dict[str, collections.Counter[str]] = {}       # dirt texture -> slot
        self.camo_named: dict[str, collections.Counter[str]] = {}       # camo texture -> kind
        self.camo_slots: collections.Counter[str] = collections.Counter()
        self.level_camo: dict[str, collections.Counter[str]] = {}       # database level -> camo
        self.soldier_camo: dict[str, set[str]] = {}                     # soldier entry -> camos
        self.weapon_camo: dict[str, set[str]] = {}                      # weapon entry -> Dt_Camo_* tiles
        self.level_weapon_camo: dict[str, collections.Counter[str]] = {}  # database level -> Dt_Camo_* tiles
        self.vehicle_variant: collections.Counter[tuple[str, str]] = collections.Counter()
        # DBX
        self.sets_by_kind: dict[str, collections.Counter[int]] = {}
        self.set_theme: collections.Counter[tuple[str, int, str]] = collections.Counter()
        self.vehicle_sets: dict[str, list[str]] = {}                   # mesh -> sets 1.. say
        self.set_fields: dict[tuple[str, str], collections.Counter[str]] = {}
        self.set_refs: dict[str, collections.Counter[str]] = {}
        self.vehicle_levels: dict[str, set[str]] = {}                  # set number -> levels
        self.soldier_levels: dict[str, set[str]] = {}
        self.terrain_dirt: int = 0
        self.dirt_factor: list[float] = []
        self.dirt_color: list[list[float]] = [[], [], []]
        self.dirt_materials: collections.Counter[str] = collections.Counter()
        self.dirt_factor_of: collections.Counter[tuple[str, float]] = collections.Counter()
        self.dirt_sides: collections.Counter[str] = collections.Counter()
        self.dirt_grids: int = 0
        self.grids: int = 0
        self.exe_run: list[str] = []
        self.exe_names: dict[str, int] = {}
        self.dbx_camo: int = 0          # partitions naming a soldier camo texture's partition
        self.camo_names: int = 0        # shader texture names and block constant names containing camo


def theme(names: list[str]) -> str:
    """desert / snow / kitupgrade / all_white / merc / other: what a shader set's (or entry's) shader names say."""
    low = ' '.join(names).lower()
    for t in ('desert', 'snow', 'kitupgrade', 'all_white', 'merc'):
        if t in low:
            return t
    return 'other'


def shader_dbs(items: list[Path], st: Stats) -> None:
    seen_dbs: set[str] = set()
    seen_entries: set[str] = set()
    camo_seen: set[tuple[str, str]] = set()
    for path in items:
        for e in fbrb.entries(path):
            if e.deleted or e.type != 'Dx11ShaderDatabase' or e.name.lower() in seen_dbs:
                continue
            seen_dbs.add(e.name.lower())
            st.dbs += 1
            level = e.name.split('/')[1].lower()
            db = shaderdb.ShaderDb.parse(fbrb.read(path, [e.name])[e.name], 11)
            progs = db.entry_programs()
            rd: dict[int, dict[int, str]] = {}
            tex: dict[int, list[tuple[int, int, str]]] = {}
            con: dict[int, list[tuple[str, int, int, tuple[float, ...]]]] = {}
            in_db: set[str] = set()
            camo_here: collections.Counter[str] = collections.Counter()
            for name, g in db.entries:
                low = name.lower()
                k = kind(name)
                pairs: set[tuple[str, str]] = set()      # (slot name, texture) over the entry's programs
                consts: dict[str, set[tuple[float, ...]]] = collections.defaultdict(set)
                for i in progs.get(g, []):
                    _, ps, b1, b2 = db.program_refs(i)
                    if ps not in rd:
                        rd[ps] = rdef_textures(db.ps[ps].code)
                        st.camo_names += sum('camo' in x.lower() for x in rd[ps].values())
                    for b in (b1, b2):
                        if b not in tex:
                            tex[b] = shaderdb.block_textures(db.params[b], db.e)
                            con[b] = shaderdb.block_constants(db.params[b], db.e)
                            st.camo_names += sum('camo' in c[0].lower() for c in con[b])
                        for s, _, t in tex[b]:
                            pairs.add((rd[ps].get(s, '(not read)'), t))
                        for cn, _, comps, vals in con[b]:
                            if cn in DIRT_CONSTS:
                                consts[cn].add(tuple(round(v, 4) for v in vals[:comps]))
                for slot, _ in pairs:
                    if DIRT_SLOT.match(slot):
                        in_db.add(slot)
                camos = sorted({leaf(t) for _, t in pairs if 'camo' in leaf(t).lower()})
                for slot, t in pairs:
                    lt = leaf(t)
                    if 'camo' in lt.lower() and (lt, low) not in camo_seen:
                        camo_seen.add((lt, low))
                        st.camo_named.setdefault(lt, collections.Counter())[k] += 1
                        st.camo_slots[slot] += 1
                if k == 'soldiers':
                    for c in camos:
                        camo_here[c] += 1
                    if camos:
                        st.soldier_camo.setdefault(low, set()).update(camos)
                dt = [c for c in camos if c in DT_CAMO]
                if k == 'weapons' and dt:
                    st.weapon_camo.setdefault(low, set()).update(dt)
                    st.level_weapon_camo.setdefault(level, collections.Counter()).update(dt)
                if low in seen_entries:
                    continue
                seen_entries.add(low)
                st.entries += 1
                st.entries_by_kind[k] += 1
                for slot in sorted({s for s, _ in pairs if DIRT_SLOT.match(s)}):
                    st.slot_entries.setdefault(slot, collections.Counter())[k] += 1
                    for t in sorted({leaf(t) for s, t in pairs if s == slot}):
                        st.slot_textures.setdefault(slot, collections.Counter())[t] += 1
                for cn, vs in consts.items():
                    st.const_entries.setdefault(cn, collections.Counter())[k] += 1
                    for v in vs:
                        st.const_values.setdefault(cn, collections.Counter())[v] += 1
                for slot, t in pairs:
                    lt = leaf(t)
                    if 'dirt' in lt.lower():
                        st.dirt_named.setdefault(lt, collections.Counter())[slot] += 1
                if k == 'vehicles':
                    tt = theme([leaf(t) for _, t in pairs if re.search(r'(?i)_desert|_snow|snowmask', leaf(t))])
                    if tt != 'other':
                        st.vehicle_variant[(theme([low.rsplit('/', 1)[-1]]), tt)] += 1
            for slot in in_db:
                st.slot_dbs[slot] += 1
            if camo_here:
                st.level_camo[level] = camo_here


def dbx_side(items: list[Path], st: Stats) -> None:
    for path, names in first_seen(items, lambda n: n.endswith('.dbx')):
        got = fbrb.read(path, names)
        for n in names:
            b = got[n]
            low = n.lower()
            if low.endswith('/materials_win32.dbx') and low.startswith('levels/'):
                grid(dbx.parse(b), st)
                continue
            if any(c in b for c in CAMO_REFS):
                st.dbx_camo += 1
            if b'ShaderSet' not in b:
                continue
            f = dbx.parse(b)
            by = {(i.attr('guid') or '').lower(): i for i in f.instances}
            level = low[len('levels/'):-4] if low.startswith('levels/') and low.count('/') == 1 else None
            for i in f.instances:
                ty = (i.attr('type') or '').split('.')[-1]
                for fn in SET_FIELDS:
                    c = i.child(fn)
                    if c is not None and c.attr('ref') is None:
                        v = dbx.value(c)
                        st.set_fields.setdefault((ty, fn), collections.Counter())[v] += 1
                        if level and fn == 'MeshShaderSetNumber' and v != '0':
                            to = st.vehicle_levels if ty == 'VehicleSpawnEntityData' else st.soldier_levels
                            to.setdefault(v, set()).add(level)
                c = i.child('MeshShaderSet')
                if c is not None and c.attr('ref') is not None:
                    st.set_refs.setdefault(ty, collections.Counter())[
                        'set' if c.attr('ref') not in ('', 'null') else 'null'] += 1
                ss = i.child('ShaderSets')
                if ss is None or ss.tag != 'array' or not ty.endswith('MeshAsset'):
                    continue
                k = kind(n)
                sets = ss.children or []
                st.sets_by_kind.setdefault(k, collections.Counter())[len(sets)] += 1
                if len(sets) < 2 or k not in ('vehicles', 'soldiers'):
                    continue
                shaders = [set_shaders(by.get((it.attr('ref') or '').lower())) for it in sets]
                row = []
                for j in range(1, len(sets)):
                    t = 'same as set 0' if shaders[j] == shaders[0] else 'empty' if not shaders[j] else \
                        theme(sorted(shaders[j]))
                    st.set_theme[(k, j, t)] += 1
                    row.append(t)
                if k == 'vehicles':
                    st.vehicle_sets[leaf(n)[:-4]] = row


def set_shaders(s: dbx.Element | None) -> set[str]:
    """The shader names (the partition folder each SurfaceShaders item's Shader names) of a MeshShaderSetAsset."""
    surf = s.child('SurfaceShaders') if s is not None else None
    refs = [((x.child('Shader') or dbx.Element('', 0)).attr('ref') or '').lower()
            for x in ((surf.children or []) if surf is not None else [])]
    return {r.rsplit('/', 2)[-2] if r.count('/') >= 2 else r for r in refs if r}


def grid(f: dbx.DbxFile, st: Stats) -> None:
    """The dirt triggers of one DICE material grid: MaterialPropertyTerrainData and the relation each hangs from."""
    st.grids += 1
    by = {(i.attr('guid') or '').lower(): i for i in f.instances}
    owner: dict[str, tuple[str, str]] = {}
    for i in f.instances:
        if i.attr('type') == 'Entity.MaterialRelationData':
            sides = []
            for m in ('Material1', 'Material2'):
                c = i.child(m)
                md = by.get(((c.attr('ref') if c is not None else None) or '').lower())
                nm = md.child('Name') if md is not None else None
                sides.append(dbx.value(nm) if nm is not None else '(any)')
            for r in (i.child('Data') or dbx.Element('', 0)).children or ():
                owner[(r.attr('ref') or '').lower()] = (sides[0], sides[1])
    found = 0
    for i in f.instances:
        if not (i.attr('type') or '').endswith('MaterialPropertyTerrainData'):
            continue
        found += 1
        st.terrain_dirt += 1
        v = number(i.child('DirtTriggerFactor'))
        if v is not None:
            st.dirt_factor.append(v)
        col = i.child('DirtTriggerColor')            # a Vec3: 4-byte numbers, the fourth padding
        if col is not None and col.kind == dbx.NUMBERS and dbx.numbers(col)[0] == 4:
            for k, x in enumerate(dbx.floats(col)[:3]):
                st.dirt_color[k].append(x)
        pair = owner.get((i.attr('guid') or '').lower())
        if pair is None:
            st.dirt_sides['no relation'] += 1
            continue
        st.dirt_sides['one side any' if '(any)' in pair else 'two materials'] += 1
        for s in pair:
            if s != '(any)':
                st.dirt_materials[s] += 1
                if v is not None:
                    st.dirt_factor_of[(s, round(v, 4))] += 1
    st.dirt_grids += found > 0


def exe_side(game: Path, st: Stats) -> None:
    exe = game / 'BFBC2Game.exe'
    if not exe.is_file():
        return
    b = exe.read_bytes()
    for n in EXE_RUN:
        st.exe_names[n.decode()] = len(re.findall(re.escape(n) + rb'\0', b))
    at = b.find(b'destruction_factor\0')
    end = b.find(b'dirtFactor\0', at)
    if at >= 0 and 0 < end - at < 1024:
        st.exe_run = [m.group().decode() for m in re.finditer(rb'[A-Za-z_]{4,}', b[at:end + 10])]


def analyse(game: Path) -> Stats:
    st = Stats()
    items = bundles(game)
    shader_dbs(items, st)
    dbx_side(items, st)
    exe_side(game, st)
    return st


def weapon_soldier_agree(st: Stats) -> tuple[int, list[str]]:
    """Over the databases binding both: how many bind the weapons' Dt_Camo_* tile of the US soldiers' camo's theme
    (desert: Camo_Digital, wood: Camo_US1981Woodland, snow: Camo_Snow_1), and the levels (weapon tile / US camo)
    that do not."""
    agree, differ = 0, []
    for lv, wc in sorted(st.level_weapon_camo.items()):
        sc = st.level_camo.get(lv, collections.Counter())
        us = [x for x in US_CAMO if sc.get(x)]
        if not us:
            continue
        top, tile = max(us, key=lambda x: sc[x]), max(wc, key=wc.__getitem__)
        if THEME_OF[top] == tile:
            agree += 1
        else:
            differ.append(f'{lv} ({tile} / {top})')
    return agree, differ


def camo_pairs(st: Stats) -> collections.Counter[tuple[str, str]]:
    """(US camo, RUS camo) each database's soldier entries bind most, over the databases (BFBC2's soldiers only)."""
    out: collections.Counter[tuple[str, str]] = collections.Counter()
    for c in st.level_camo.values():
        us = max(US_CAMO, key=lambda x: c.get(x, 0))
        ru = max(RU_CAMO, key=lambda x: c.get(x, 0))
        out[(us if c.get(us) else '-', ru if c.get(ru) else '-')] += 1
    return out


def summary(st: Stats) -> dict[str, Any]:
    """The stable counts the MAT-TEX rules cite."""
    return {
        'dbs': st.dbs,
        'entries': st.entries,
        'slot_entries': {s: sum(c.values()) for s, c in sorted(st.slot_entries.items())},
        'slot_textures': {s: sorted(c) for s, c in sorted(st.slot_textures.items())},
        'dirt_consts_entries': {c: sum(v.values()) for c, v in sorted(st.const_entries.items())},
        'dirt_factor_values': sorted({v for c in ('DirtFactor', 'DirtFactor2') for v in st.const_values.get(c, {})}),
        'dirt_color_values': sorted({v for c in ('DirtColor', 'DirtColor2') for v in st.const_values.get(c, {})}),
        'exe_run_has_dirt': bool(st.exe_run) and 'dirtColor' in st.exe_run and 'dirtFactor' in st.exe_run,
        'terrain_dirt': st.terrain_dirt,
        'dirt_trigger_factors': sorted({round(v, 4) for v in st.dirt_factor}),
        'dirt_trigger_materials': sorted(st.dirt_materials),
        'dirt_trigger_sides': dict(st.dirt_sides),
        'dirt_grids': st.dirt_grids,
        'grids': st.grids,
        'vehicle_set_themes': sorted((j, t, n) for (k, j, t), n in st.set_theme.items() if k == 'vehicles'),
        'vehicle_spawn_numbers': dict(sorted(st.set_fields.get(('VehicleSpawnEntityData', 'MeshShaderSetNumber'),
                                                                 collections.Counter()).items())),
        'soldier_spawn_numbers': dict(sorted(st.set_fields.get(('SoldierSpawnEntityData', 'MeshShaderSetNumber'),
                                                                 collections.Counter()).items())),
        'vehicle_levels': {k: len(v) for k, v in sorted(st.vehicle_levels.items())},
        'soldier_levels': {k: sorted(v) for k, v in sorted(st.soldier_levels.items())},
        'camo_levels': len(st.level_camo),
        'camo_pairs': sorted((a, b, n) for (a, b), n in camo_pairs(st).items()),
        'soldier_camo_entries': len(st.soldier_camo),
        'soldier_camo_varying': sum(1 for v in st.soldier_camo.values() if len(v) > 1),
        'dbx_naming_soldier_camo': st.dbx_camo,
        'camo_named_slots_or_constants': st.camo_names,
        'weapon_camo_entries': len(st.weapon_camo),
        'weapon_camo_varying': sum(1 for v in st.weapon_camo.values() if len(v) > 1),
        'weapon_camo_levels': {t: sum(1 for c in st.level_weapon_camo.values() if max(c, key=c.__getitem__) == t)
                               for t in DT_CAMO},
        'weapon_soldier_camo_agree': weapon_soldier_agree(st),
    }


EXPECTED: dict[str, Any] = {    # the 2026-11-22 run (DICE's bundles of the game copy)
    'dbs': 74,
    'entries': 9747,
    'slot_entries': {'texture_Dirt': 7, 'texture_Dirt2': 3, 'texture_DirtMask': 26, 'texture_DirtMask2': 51,
                     'texture_DirtMask3': 9, 'texture_MaterialMask': 60, 'texture_MaterialMask2': 55,
                     'texture_MaterialMask3': 7},
    'slot_textures': {'texture_Dirt': ['Dt_Dirt_002'], 'texture_Dirt2': ['Dt_Dirt_002'],
                      'texture_DirtMask': ['DirtMaskRGB', 'Huey_Rotors_01_A'], 'texture_DirtMask2': ['DirtMaskRGB'],
                      'texture_DirtMask3': ['DirtMaskRGB'], 'texture_MaterialMask': ['Msk_Plaster_Rust'],
                      'texture_MaterialMask2': ['Msk_Plaster_Rust'], 'texture_MaterialMask3': ['Msk_Plaster_Rust']},
    'dirt_consts_entries': {'Destruction_factor': 23, 'Destruction_factor2': 2, 'DirtColor': 16, 'DirtColor2': 4,
                            'DirtFactor': 16, 'DirtFactor2': 2},
    'dirt_factor_values': [(0.0,)],
    'dirt_color_values': [(0.0, 0.0, 0.0), (0.334, 0.334, 0.334)],
    'exe_run_has_dirt': True,
    'terrain_dirt': 363,
    'dirt_trigger_factors': [0.001, 0.1],
    'dirt_trigger_materials': ['Dirt', 'Field', 'Grass', 'Gravel', 'Mud', 'Rock', 'Sand', 'SeaBed', 'Snow', 'Tarmac'],
    'dirt_trigger_sides': {'one side any': 363},
    'dirt_grids': 74,
    'grids': 74,
    'vehicle_set_themes': [(1, 'desert', 6), (1, 'merc', 2), (1, 'other', 1), (1, 'same as set 0', 5), (2, 'snow', 9),
                           (3, 'empty', 2)],
    'vehicle_spawn_numbers': {'0': 1226, '1': 99, '2': 83},
    'soldier_spawn_numbers': {'0': 1910, '1': 15, '2': 20},
    'vehicle_levels': {'1': 13, '2': 13},
    'soldier_levels': {'1': ['sp_003_b', 'sp_006', 'sp_007'], '2': ['sp_005']},
    'camo_levels': 74,
    'camo_pairs': [('-', '-', 20), ('-', 'Camo_RUS_03', 1), ('Camo_Digital', 'Camo_RUS_04', 14),
                   ('Camo_Snow_1', 'Camo_RUS_gorod', 15), ('Camo_US1981Woodland', '-', 2),
                   ('Camo_US1981Woodland', 'Camo_RUS_03', 22)],
    'soldier_camo_entries': 58,
    'soldier_camo_varying': 38,
    'dbx_naming_soldier_camo': 0,
    'camo_named_slots_or_constants': 0,
    'weapon_camo_entries': 26,
    'weapon_camo_varying': 17,
    'weapon_camo_levels': {'Dt_Camo_Desert': 13, 'Dt_Camo_Wood': 28, 'Dt_Camo_Snow': 12},
    'weapon_soldier_camo_agree': (49, ['sp_004_b (Dt_Camo_Wood / Camo_Snow_1)', 'sp_006 (Dt_Camo_Wood / Camo_Digital)',
                                       'sp_02 (Dt_Camo_Wood / Camo_Snow_1)', 'sp_04 (Dt_Camo_Wood / Camo_Snow_1)']),
}


def checks(game: Path) -> list[tuple[bool, str]]:
    """(ok, description) per stable count the MAT-TEX rules of spec/engine/materials.md cite."""
    got = summary(analyse(game))
    return [(got.get(k) == want, f'grime probe: {k} = {want!r} (got {got.get(k)!r})') for k, want in EXPECTED.items()]


# ------------------------------------------------------------------------------------------------ the report

def report(st: Stats) -> None:
    s = summary(st)
    print(f'== shader databases: {st.dbs} DICE Dx11 databases, {st.entries} distinct entries; by class '
          + ', '.join(f'{k} {v}' for k, v in st.entries_by_kind.most_common()))
    print('dirt / material-mask slots (distinct entries; by class; databases using the slot; textures bound):')
    for slot, c in sorted(st.slot_entries.items()):
        print(f'  {slot}: {sum(c.values())} entries ({", ".join(f"{k} {v}" for k, v in c.most_common())}); '
              f'{st.slot_dbs[slot]} databases; textures: '
              + ', '.join(f'{t} {n}' for t, n in st.slot_textures[slot].most_common()))
    print('dirt constants (distinct entries carrying each; by class; block values):')
    for cn in DIRT_CONSTS:
        c = st.const_entries.get(cn, collections.Counter())
        vals = st.const_values.get(cn, collections.Counter())
        print(f'  {cn}: {sum(c.values())} entries ({", ".join(f"{k} {v}" for k, v in c.most_common())}); values '
              + ', '.join(f'{v} x {n}' for v, n in vals.most_common(6)))
    print('textures named *dirt* (slot they are bound to, entries):')
    for t, c in sorted(st.dirt_named.items()):
        print(f'  {t}: ' + ', '.join(f'{k} {v}' for k, v in c.most_common()))
    print('textures named *camo* (class of the entries binding them; distinct entries over every database):')
    for t, c in sorted(st.camo_named.items()):
        print(f'  {t}: ' + ', '.join(f'{k} {v}' for k, v in c.most_common()))
    print('  slots holding a camo texture:', dict(st.camo_slots.most_common()))
    print('vehicle entries binding a _Desert / _Snow / snow-mask texture, (entry shader name says, texture says):',
          dict(st.vehicle_variant))
    print(f'== soldier camouflage per database: {len(st.level_camo)} databases bind camo on soldier entries')
    print(f'  soldier entries binding camo: {s["soldier_camo_entries"]}; binding another camo texture in another '
          f'database: {s["soldier_camo_varying"]}')
    print('  (US camo, RUS camo) the soldier entries bind most, databases:', s['camo_pairs'])
    for lv, c in sorted(st.level_camo.items()):
        print(f'    {lv}: ' + ', '.join(f'{t} {n}' for t, n in c.most_common()))
    print(f'== weapon camouflage per database: {s["weapon_camo_entries"]} weapon entries bind a Dt_Camo_* tile, '
          f'{s["weapon_camo_varying"]} another one in another database; the tile weapons bind most, databases: '
          f'{s["weapon_camo_levels"]}; against the US soldiers\' camo theme: {s["weapon_soldier_camo_agree"]}')
    print('== shader sets (DBX)')
    for cls, sizes in sorted(st.sets_by_kind.items()):
        print(f'  {cls} mesh assets by ShaderSets count: {dict(sorted(sizes.items()))}')
    print('  extra sets (class, set number, what its shader names say):',
          sorted((k, j, t, n) for (k, j, t), n in st.set_theme.items()))
    print('  vehicle meshes with more than one set (sets 1, 2 ...):',
          '; '.join(f'{m}: {", ".join(v)}' for m, v in sorted(st.vehicle_sets.items())))
    for (ty, fn), c in sorted(st.set_fields.items()):
        print(f'  {ty}.{fn}: {dict(c.most_common())}')
    for ty, c in sorted(st.set_refs.items()):
        print(f'  {ty}.MeshShaderSet: {dict(c)}')
    for k, v in sorted(st.vehicle_levels.items()):
        print(f'  VehicleSpawnEntityData.MeshShaderSetNumber = {k} on {len(v)} levels: {", ".join(sorted(v))}')
    for k, v in sorted(st.soldier_levels.items()):
        print(f'  SoldierSpawnEntityData.MeshShaderSetNumber = {k} on {len(v)} levels: {", ".join(sorted(v))}')
    print(f'== terrain dirt triggers: {st.terrain_dirt} MaterialPropertyTerrainData in {st.dirt_grids} of {st.grids} '
          f'DICE materials_win32 grids; relations: {dict(st.dirt_sides)}')
    print(f'  DirtTriggerFactor: {rng(st.dirt_factor)}')
    for n_ch, ch in enumerate('RGB'):
        print(f'  DirtTriggerColor {ch}: {rng(st.dirt_color[n_ch])}')
    print(f'  materials on those relations ({len(st.dirt_materials)}): '
          + ', '.join(f'{m} {n}' for m, n in st.dirt_materials.most_common()))
    print('  DirtTriggerFactor per material: '
          + ', '.join(f'{m} {v:g} x {n}' for (m, v), n in sorted(st.dirt_factor_of.items())))
    print(f'== executable: names {st.exe_names}; the run from destruction_factor to dirtFactor: {st.exe_run}')


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    t = time.perf_counter()
    st = analyse(game)
    report(st)
    print(f'({time.perf_counter() - t:.1f} s)')
    print('summary:', summary(st))


if __name__ == '__main__':
    main()
