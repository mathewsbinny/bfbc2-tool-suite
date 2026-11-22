"""Destruction as DICE's files describe it: destructible parts and their health states, the meshes and collision
parts a state shows, bangers, the level's model-group members, and the terrain's destruction layers (read-only;
spec/engine/destruction.md).

    python spec/probes/destruction.py [--opfor <folder>]

Reads the level bundles of Project OPFOR BFBC2's game copy that are DICE's (`game/**/levels/*/*.fbrb` dated before
2013, as the install leaves them), one bundle at a time, each partition once (the first bundle that holds it).

Prints structure, counts and numeric ranges only:

- blueprints: every `<blueprint>.dbx` beside a `<blueprint>_Havok.dbx`: the class of its object, its
  `PartComponentData` parts, their `HealthStateData` states (count, order, health, `MeshName` against the part's
  name and the physics asset's `PartInfoDatas`), what a state spawns (banger, explosion, effects, loose parts), the
  part flags and links, and the object's collapse percentages;
- bangers: blueprints named `*_banger` and the `SpawnedBanger` references;
- model groups: the level partitions' group members (`PhysicsPartCountPerInstance`, the health-state arrays) and
  which member types are destructible blueprints;
- terrain: `VisualTerrain` patch shaders with a `_Destruction` layer, `TerrainData` destruction fields.
"""
from __future__ import annotations

import argparse
import collections
import re
import struct
import sys
import time
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths

Counter = collections.Counter


def stock_bundles(game: Path) -> list[Path]:
    return sorted((p for p in game.glob('**/levels/*/*.fbrb') if time.gmtime(p.stat().st_mtime).tm_year < 2013),
                  key=lambda p: str(p).lower())


def text(e: dbx.Element | None) -> str:
    return '' if e is None else dbx.value(e)


def number(e: dbx.Element | None) -> float | None:
    if e is None or e.kind != dbx.NUMBERS:
        return None
    size, vals = dbx.numbers(e)
    if size != 4 or len(vals) != 1:
        return None
    return float(struct.unpack('>f', vals[0])[0])


def whole(e: dbx.Element | None) -> int | None:
    if e is None or e.kind != dbx.NUMBERS:
        return None
    size, vals = dbx.numbers(e)
    return int.from_bytes(vals[0], 'big', signed=True) if size == 4 and len(vals) == 1 else None


def flag(e: dbx.Element | None) -> bool:
    return e is not None and len(e.raw) >= 2 and e.raw[1] == 1


def ref(e: dbx.Element | None) -> str:
    return 'null' if e is None else (e.attr('ref') or 'null')


def items(e: dbx.Element | None) -> list[dbx.Element]:
    return list(e.children or ()) if e is not None else []


def rng(vals: list[float]) -> str:
    if not vals:
        return 'none'
    c = Counter(vals)
    top = ', '.join(f'{v:g} x {n}' for v, n in c.most_common(5))
    return f'{min(vals):g} .. {max(vals):g} ({len(vals)} values, {len(c)} distinct; most: {top})'


def blueprints(bundles: list[Path]) -> set[str]:
    """Prints the destructible-part census; returns the lower-case paths of blueprints that have parts."""
    seen: set[str] = set()
    classes: Counter[str] = Counter()
    with_parts = 0
    parts_per: list[float] = []
    states_per: Counter[int] = Counter()
    healthiest_first = lowest_first = parts_total = 0
    multi: Counter[str] = Counter()
    first_health: list[float] = []
    last_health: list[float] = []
    mesh_kind: Counter[str] = Counter()
    suffixes: Counter[str] = Counter()
    mesh_in_havok = mesh_named = 0
    spawn: Counter[str] = Counter()
    part_flags: Counter[str] = Counter()
    links: list[float] = []
    pct: dict[str, list[float]] = collections.defaultdict(list)
    banger_refs: Counter[str] = Counter()
    banger_targets: set[str] = set()
    banger_names: set[str] = set()
    destructible: set[str] = set()
    ruin_parts = 0
    kinds: Counter[str] = Counter()
    for path in bundles:
        have = {e.name.lower(): e.name for e in fbrb.entries(path) if not e.deleted}
        pairs = []
        for low, name in have.items():
            if not low.endswith('_havok.dbx') or '_havokasset' in low:
                continue
            bp = low[:-len('_havok.dbx')] + '.dbx'
            if bp in seen or bp not in have:
                continue
            seen.add(bp)
            pairs.append((have[bp], name))
        if not pairs:
            continue
        got = fbrb.read(path, [x for p in pairs for x in p])
        for bp_name, hv_name in pairs:
            low = bp_name.lower()[:-4]
            if low.endswith('_banger'):
                banger_names.add(low)
            leaf = low.rsplit('/', 1)[-1]
            for word in ('banger', 'wreck', 'ruin', 'destroyed', 'debris'):
                kinds[word] += word in leaf
            f = dbx.parse(got[bp_name])
            by = {(i.attr('guid') or '').lower(): i for i in f.instances}
            ob = next((i for i in f.instances if i.attr('type') == 'Entity.ObjectBlueprint'), None)
            obj = by.get(ref(ob.child('Object')).lower()) if ob is not None else None
            classes[(obj.attr('type') or '?') if obj is not None else '(no object)'] += 1
            comps = [i for i in f.instances if i.attr('type') == 'GameSharedResources.PartComponentData']
            if not comps:
                continue
            with_parts += 1
            destructible.add(low)
            parts_per.append(float(len(comps)))
            if obj is not None:
                for k in ('DestroyRuinPartPercentage', 'ManyPartsDestroyedPercentage', 'AboutToBeDestroyedTime'):
                    v = number(obj.child(k))
                    if v is None:
                        w = whole(obj.child(k))
                        v = None if w is None else float(w)
                    if v is not None:
                        pct[k].append(v)
                links.append(float(len(items(obj.child('PartLinks')))))
            hv = dbx.parse(got[hv_name])
            ha = next((i for i in hv.instances if i.attr('type') == 'Physics.HavokAsset'), None)
            pnames = {text(p.child('Name')).lower() for p in items(ha.child('PartInfoDatas') if ha else None)}
            for c in comps:
                parts_total += 1
                cname = text(c.child('Name')).lower()
                ruin_parts += cname.startswith('ruin')
                for k in ('IsSupported', 'IsFragile', 'IsLoadBearing', 'CanTriggerCollapse', 'CanCollapse'):
                    part_flags[k] += flag(c.child(k))
                states = [by.get(ref(s).lower()) for s in items(c.child('HealthStates'))]
                st = [s for s in states if s is not None]
                states_per[len(st)] += 1
                hs = [number(s.child('Health')) or 0.0 for s in st]
                if hs:
                    healthiest_first += hs == sorted(hs, reverse=True)
                    lowest_first += hs == sorted(hs)
                    first_health.append(max(hs))
                    last_health.append(min(hs))
                if len(st) >= 2:
                    order = sorted(zip(hs, st), key=lambda x: -x[0])
                    top_m = text(order[0][1].child('MeshName')).lower()
                    low_m = text(order[-1][1].child('MeshName')).lower()
                    multi['parts with 2+ states'] += 1
                    multi['healthiest state shows the part name'] += top_m == cname
                    multi['lowest state at health 0'] += order[-1][0] == 0
                    multi['lowest state shows nothing'] += not low_m
                    multi['lowest state has Effects'] += bool(items(order[-1][1].child('Effects')))
                    multi['healthiest state has Effects'] += bool(items(order[0][1].child('Effects')))
                for s in st:
                    m = text(s.child('MeshName')).lower()
                    if not m:
                        mesh_kind['empty (part gone)'] += 1
                    elif m == cname:
                        mesh_kind['the part name'] += 1
                    elif m.startswith(cname + '_'):
                        mesh_kind['part name + suffix'] += 1
                        suffixes[re.sub(r'\d+', 'N', m[len(cname) + 1:])] += 1
                    else:
                        mesh_kind['other name'] += 1
                    if m:
                        mesh_named += 1
                        mesh_in_havok += m in pnames
                    b = ref(s.child('SpawnedBanger'))
                    if b != 'null':
                        spawn['SpawnedBanger'] += 1
                        target = b.rsplit('/', 1)[0].lower()
                        banger_targets.add(target)
                        banger_refs['target named *_banger' if target.endswith('_banger') else 'other name'] += 1
                    for k in ('Explosion', 'LoosePartPhysics'):
                        spawn[k] += ref(s.child(k)) != 'null'
                    spawn['LoosePartPhysicsArray items'] += len(items(s.child('LoosePartPhysicsArray')))
                    spawn['Effects items'] += len(items(s.child('Effects')))
                    spawn['states with Effects'] += bool(items(s.child('Effects')))
                    spawn['CopyDamageToBanger true'] += flag(s.child('CopyDamageToBanger'))
                    spawn[f'PreferedShapeType {text(s.child("PreferedShapeType"))}'] += 1
    print('== blueprints (DICE bundles: <blueprint>.dbx beside <blueprint>_Havok.dbx)')
    print(f'{len(seen)} blueprints; object classes: {dict(classes.most_common())}')
    print(f'blueprint names (last part) containing: {dict(kinds)}')
    print(f'with PartComponentData parts: {with_parts}; parts per blueprint {rng(parts_per)}')
    print(f'parts: {parts_total} (named ruin*: {ruin_parts}); health states per part: {dict(sorted(states_per.items()))}')
    print(f'parts whose states run healthiest first in the file: {healthiest_first}, lowest first: {lowest_first} '
          f'(of {parts_total}; one state counts as both)')
    print(f'parts with 2+ states: {dict(multi)}')
    print(f'healthiest state health: {rng(first_health)}')
    print(f'lowest state health: {rng(last_health)}')
    print(f'MeshName: {dict(mesh_kind)}; suffixes after the part name: {dict(suffixes.most_common(10))}')
    print(f'non-empty MeshNames that are a PartInfoDatas name of the physics asset: {mesh_in_havok} of {mesh_named}')
    print(f'state contents: {dict(sorted(spawn.items()))}')
    print(f'part flags set: {dict(part_flags)}')
    print(f'PartLinks per destructible object: {rng(links)}')
    for field_name, values in sorted(pct.items()):
        print(f'{field_name}: {rng(values)}')
    print('== bangers')
    print(f'blueprints named *_banger: {len(banger_names)}; SpawnedBanger references: {dict(banger_refs)}; '
          f'distinct targets {len(banger_targets)}, of them read as blueprints here: '
          f'{len(banger_targets & banger_names)}')
    return destructible


def groups(bundles: list[Path], destructible: set[str]) -> None:
    seen: set[str] = set()
    members: Counter[str] = Counter()
    per_inst: dict[str, Counter[int]] = collections.defaultdict(Counter)
    instances: Counter[str] = Counter()
    health_arrays: Counter[str] = Counter()
    destr_types: Counter[str] = Counter()
    destr_instances = 0
    triggers = levels = 0
    net_vs_parts: Counter[bool] = Counter()
    for path in bundles:
        names = [e.name for e in fbrb.entries(path) if not e.deleted
                 and re.fullmatch(r'levels/[^/]+\.dbx', e.name.lower()) and '_description' not in e.name.lower()
                 and e.name.lower() not in seen]
        if not names:
            continue
        got = fbrb.read(path, names)
        for n in names:
            seen.add(n.lower())
            f = dbx.parse(got[n])
            if not any((i.attr('type') or '').endswith('.LevelData') for i in f.instances):
                continue
            levels += 1
            for i in f.instances:
                t = (i.attr('type') or '').split('.')[-1]
                if t == 'DamageAreaTriggerEntityData':
                    triggers += 1
                if not t.endswith('MemberData'):
                    continue
                members[t] += 1
                pp = whole(i.child('PhysicsPartCountPerInstance'))
                ic = whole(i.child('InstanceCount')) or 0
                instances[t] += ic
                if pp is not None:
                    per_inst[t][pp] += 1
                ni = whole(i.child('NetworkIdCountPerInstance'))
                if ni is not None:
                    net_vs_parts[(ni == pp) if pp is not None else False] += 1
                for k in ('FirstPartHealthStateIndices', 'FirstPartHealthStateNetworkIds'):
                    if items(i.child(k)):
                        health_arrays[f'{t}.{k} non-empty'] += 1
                mt = ref(i.child('MemberType')).rsplit('/', 1)[0].lower()
                if mt in destructible:
                    destr_types[t] += 1
                    if t == 'StaticModelGroupMemberData':
                        destr_instances += ic
    print('== model groups (DICE level partitions)')
    print(f'{levels} level partitions; member types: {dict(members)}; instances: {dict(instances)}')
    for t, c in sorted(per_inst.items()):
        print(f'  {t} PhysicsPartCountPerInstance: {dict(sorted(c.items()))}')
    print(f'members with NetworkIdCountPerInstance: equal to PhysicsPartCountPerInstance {net_vs_parts[True]}, '
          f'not {net_vs_parts[False]}')
    print(f'health-state arrays: {dict(health_arrays) or "empty in every member"}')
    print(f'member types whose blueprint has destructible parts: {dict(destr_types)}; '
          f'static group instances of them: {destr_instances}')
    print(f'DamageAreaTriggerEntityData instances: {triggers}')


def terrain(bundles: list[Path]) -> None:
    seen: set[str] = set()
    vt = 0
    shaders_per: list[float] = []
    destr_per: list[float] = []
    all_destr = 0
    fields: dict[str, Counter[str]] = collections.defaultdict(Counter)
    for path in bundles:
        es = [e for e in fbrb.entries(path) if not e.deleted and e.name.lower() not in seen]
        want = [e.name for e in es if e.type == 'VisualTerrain']
        want += [e.name for e in es if re.fullmatch(r'terrains/([^/]+)/\1\.dbx', e.name.lower())]
        if not want:
            continue
        got = fbrb.read(path, want)
        for n in want:
            seen.add(n.lower())
            d = got[n]
            if n.lower().endswith('.dbx'):
                f = dbx.parse(d)
                for i in f.instances:
                    if i.attr('type') == 'Terrain.TerrainData':
                        for k in ('DefaultFullDepthEnable', 'MaskTileSize', 'MaterialMaskStepSize'):
                            c = i.child(k)
                            if c is not None:
                                fields[k][str(flag(c)) if c.kind != dbx.NUMBERS else str(whole(c))] += 1
                continue
            vt += 1
            names = {m.group().decode('latin-1').lower() for m in re.finditer(rb'[ -~]*/PatchMaskShaders/[ -~]+', d)}
            shaders_per.append(float(len(names)))
            nd = sum(1 for s in names if s.endswith('_destruction'))
            destr_per.append(float(nd))
            all_destr += nd == len(names) and nd > 0
    print('== terrain (DICE terrain bundles)')
    print(f'{vt} VisualTerrain resources; patch mask shaders per terrain {rng(shaders_per)}')
    print(f'of them ending _Destruction: {rng(destr_per)}; terrains where every patch shader does: {all_destr}')
    for field_name, counts in sorted(fields.items()):
        print(f'TerrainData.{field_name}: {dict(counts)}')


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    bundles = stock_bundles(game)
    print(f'{len(bundles)} DICE level bundles')
    destructible = blueprints(bundles)
    groups(bundles, destructible)
    terrain(bundles)


if __name__ == '__main__':
    main()
