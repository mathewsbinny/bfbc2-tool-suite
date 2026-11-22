"""Physics materials as the game copy ships them: the material grids, their relation data, and the materials the
objects' physics assets name (read-only; spec/engine/materials.md).

    python spec/probes/materials.py [--opfor <folder>] [--stock]

Reads the level bundles of Project OPFOR BFBC2's game copy (`game/**/levels/*/*.fbrb`), one bundle at a time, and
takes each partition once, from the first bundle that holds it: DICE's bundles first (a bundle is DICE's when its
file date is before 2013, as the install leaves them), then OPFOR's. --stock reads DICE's bundles only.

Prints structure, counts and numeric ranges only (no names beyond DBX class, field and material names):

- grids: `Entity.MaterialGridData` partitions (`levels/<lv>/materials_win32` and BF1943's `materials_xenon`): their
  materials and relations, the relation data classes, relations with one side null;
- materials: the distinct `Entity.MaterialData` names, the flags each sets (Penetrable, SeeThrough,
  ClientDestructible, NoCollisionResponse, NoCollisionResponseCombined), names whose flags differ between grids,
  categories, and whether `Hash` is the 33-xor hash of the lower-case name;
- relation values: the numeric range of every `MaterialRelationDamageData` field (read from the binary values, not
  the XML text), the booleans' counts, the other relation and property data;
- physics assets: every object `<blueprint>_Havok` partition with its `.res`: whether the wrapper's material hashes
  are the hashes of `MaterialMappings` in order, the part run's form, whether each part's byte names its
  `PartInfoData.MaterialName`, the part materials used and how many parts sit on a Penetrable / SeeThrough material.
"""
from __future__ import annotations

import argparse
import collections
import os
import re
import struct
import sys
import time
from collections.abc import Iterator
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths

FLAGS = ('Penetrable', 'SeeThrough', 'ClientDestructible', 'NoCollisionResponse', 'NoCollisionResponseCombined')
MAGIC = bytes.fromhex('57E0E05710C0C010')


def name_hash(name: str) -> int:
    """The 33-xor hash from 5381 over the lower-case name (bc2_formats::havok::part_hash)."""
    h = 5381
    for b in name.lower().encode('latin-1'):
        h = ((h * 33) ^ b) & 0xFFFFFFFF
    return h


def bundles(game: Path, stock_only: bool) -> list[tuple[Path, bool]]:
    """The level bundles, DICE's (dated before 2013) first, each with whether it is DICE's."""
    out = []
    for p in sorted(game.glob('**/levels/*/*.fbrb')):
        stock = time.gmtime(p.stat().st_mtime).tm_year < 2013
        if stock or not stock_only:
            out.append((p, stock))
    return sorted(out, key=lambda b: (not b[1], str(b[0]).lower()))


def first_seen(items: list[tuple[Path, bool]], want: object) -> Iterator[tuple[Path, bool, list[str]]]:
    """Per bundle, the entry names `want(lower name)` accepts that no earlier bundle held."""
    seen: set[str] = set()
    for path, stock in items:
        names = []
        for e in fbrb.entries(path):
            low = e.name.lower()
            if e.deleted or low in seen or not want(low):       # type: ignore[operator]
                continue
            seen.add(low)
            names.append(e.name)
        if names:
            yield path, stock, names


def number(e: dbx.Element) -> float | None:
    """A 4-byte number read from its binary value (float), or None."""
    if e.kind != dbx.NUMBERS:
        return None
    size, vals = dbx.numbers(e)
    if size != 4 or len(vals) != 1:
        return None
    return float(struct.unpack('>f', vals[0])[0])


def flag(e: dbx.Element | None) -> bool:
    return e is not None and len(e.raw) >= 2 and e.raw[1] == 1


def items_of(e: dbx.Element | None) -> list[dbx.Element]:
    return list(e.children or ()) if e is not None else []


def text(e: dbx.Element | None) -> str:
    return '' if e is None else dbx.value(e)


def rng(vals: list[float]) -> str:
    if not vals:
        return 'none'
    c = collections.Counter(vals)
    top = ', '.join(f'{v:g} x {n}' for v, n in c.most_common(4))
    return f'{min(vals):g} .. {max(vals):g} ({len(vals)} values, {len(c)} distinct; most: {top})'


def grids(items: list[tuple[Path, bool]]) -> dict[str, dict[str, bool]]:
    """Prints the grid census; returns {lower material name: {flag: set}} from DICE's grids."""
    per_grid: list[tuple[str, bool, int, int]] = []
    data_types: collections.Counter[str] = collections.Counter()
    data_per_rel: collections.Counter[int] = collections.Counter()
    null_side = 0
    layers = 0
    mats: dict[str, dict[str, bool]] = {}
    differ: set[str] = set()
    cats: collections.Counter[str] = collections.Counter()
    hash_ok = hash_bad = 0
    dmg: dict[str, list[float]] = collections.defaultdict(list)
    dmg_bool: dict[str, collections.Counter[bool]] = collections.defaultdict(collections.Counter)
    other: dict[str, list[float]] = collections.defaultdict(list)
    src_set: collections.Counter[str] = collections.Counter()
    index_of: dict[str, set[int]] = collections.defaultdict(set)
    dmg_per_rel: collections.Counter[int] = collections.Counter()
    dmg_src_all = 0
    refs_set: collections.Counter[str] = collections.Counter()
    for path, stock, names in first_seen(items, lambda n: n.startswith('levels/') and '/materials_' in n
                                         and n.endswith('.dbx')):
        got = fbrb.read(path, names)
        for n in names:
            f = dbx.parse(got[n])
            insts = f.instances
            grid = [i for i in insts if i.attr('type') == 'Entity.MaterialGridData']
            if not grid:
                continue
            g = grid[0]
            m_arr, r_arr = g.child('Materials'), g.child('MaterialRelations')
            gl = g.child('MaterialGridLayers')
            layers += len(gl.children or ()) if gl is not None else 0
            per_grid.append((n.lower(), stock, len(m_arr.children or ()) if m_arr is not None else 0,
                             len(r_arr.children or ()) if r_arr is not None else 0))
            if not stock or n.lower().endswith('_xenon.dbx'):
                continue                                       # values: DICE's BFBC2 grids only
            by_guid = {(i.attr('guid') or '').lower(): i for i in insts}
            for k, c in enumerate((m_arr.children or ()) if m_arr is not None else ()):
                m = by_guid.get((c.attr('ref') or '').lower())
                if m is not None:
                    index_of[text(m.child('Name')).lower()].add(k)
            for i in insts:
                t = i.attr('type') or ''
                if t == 'Entity.MaterialData':
                    name = text(i.child('Name'))
                    fl = {k: flag(i.child(k)) for k in FLAGS}
                    low = name.lower()
                    if low in mats and mats[low] != fl:
                        differ.add(low)
                    mats.setdefault(low, fl)
                    cats[text(i.child('Category'))] += 1
                    h = int(text(i.child('Hash'))) & 0xFFFFFFFF
                    if h == name_hash(name):
                        hash_ok += 1
                    else:
                        hash_bad += 1
                elif t == 'Entity.MaterialRelationData':
                    m1, m2 = i.child('Material1'), i.child('Material2')
                    if any(m is None or (m.attr('ref') or 'null') == 'null' for m in (m1, m2)):
                        null_side += 1
                    d = i.child('Data')
                    data_per_rel[len(d.children or ()) if d is not None else 0] += 1
                    data = [by_guid.get((r.attr('ref') or '').lower()) for r in items_of(d)]
                    dm = [x for x in data if x is not None and (x.attr('type') or '').endswith('RelationDamageData')]
                    dmg_per_rel[len(dm)] += 1
                    if len(dm) > 1:
                        dmg_src_all += all((x.child('SourceMaterial') or dbx.Element('', 0)).attr('ref', 'null')
                                           != 'null' for x in dm)
                else:
                    data_types[t] += 1
                    for c in i.children or ():
                        field = c.attr('name') or ''
                        if field == 'SourceMaterial':
                            src_set['set' if (c.attr('ref') or 'null') != 'null' else 'null'] += 1
                            continue
                        if c.attr('ref') is not None:
                            refs_set[f'{t.split(".")[-1]}.{field} ' +
                                     ('set' if c.attr('ref') != 'null' else 'null')] += 1
                            continue
                        v = number(c)
                        if t.endswith('MaterialRelationDamageData'):
                            if v is not None:
                                dmg[field].append(v)
                            elif c.kind != dbx.NUMBERS:
                                dmg_bool[field][flag(c)] += 1
                        elif v is not None:
                            other[f'{t.split(".")[-1]}.{field}'].append(v)
    win = [g for g in per_grid if not g[0].endswith('_xenon.dbx')]
    xen = [g for g in per_grid if g[0].endswith('_xenon.dbx')]
    print('== grids')
    for label, gs in (('materials_win32 (DICE grids read for values: stock)', win), ('materials_xenon (BF1943)', xen)):
        if gs:
            print(f'{label}: {len(gs)} partitions ({sum(1 for g in gs if g[1])} from DICE bundles); materials '
                  f'{min(g[2] for g in gs)} - {max(g[2] for g in gs)}, relations {min(g[3] for g in gs)} - '
                  f'{max(g[3] for g in gs)}')
    print(f'MaterialGridLayers items over all grids: {layers}')
    print(f'relations with a null side (a material against any): {null_side}')
    print(f'data items per relation: {dict(sorted(data_per_rel.items()))}')
    print('relation / property data classes:', dict(data_types.most_common()))
    print(f'SourceMaterial on data items: {dict(src_set)}')
    print('== materials (DICE stock grids, distinct names)')
    print(f'{len(mats)} names; per flag set: ' +
          ', '.join(f'{k} {sum(1 for f in mats.values() if f[k])}' for k in FLAGS))
    print(f'names whose flags differ between grids: {len(differ)}')
    print('categories (MaterialData instances):', dict(cats.most_common()))
    print(f'Hash = 33-xor hash of the lower-case name: {hash_ok} of {hash_ok + hash_bad} MaterialData instances')
    for fk in FLAGS:
        print(f'  {fk}: ' + ', '.join(sorted(nm for nm, fl in mats.items() if fl[fk])))
    moved = sum(1 for ix in index_of.values() if len(ix) > 1)
    print(f'names whose index in MaterialGridData.Materials differs between grids: {moved} of {len(index_of)}')
    print(f'relations by their MaterialRelationDamageData count: {dict(sorted(dmg_per_rel.items()))}; '
          f'of those with 2 or more, every item with SourceMaterial set: {dmg_src_all}')
    print('== MaterialRelationDamageData values (DICE stock grids, every instance)')
    for field_name, values in sorted(dmg.items()):
        print(f'  {field_name}: {rng(values)}')
    for field_name, bools in sorted(dmg_bool.items()):
        print(f'  {field_name}: true {bools[True]}, false {bools[False]}')
    print('== other relation / property data values')
    for field_name, values in sorted(other.items()):
        print(f'  {field_name}: {rng(values)}')
    for field_name, count in sorted(refs_set.items()):
        print(f'  {field_name}: {count}')
    return mats


def wrapper(res: bytes) -> tuple[list[int], bytes, list[int]] | None:
    """(part words, part run, material hashes) of a blueprint's wrapper (HAVOK-WRAP-1), or None."""
    p = res.find(MAGIC)
    if p < 8:
        return None
    u32 = lambda at: struct.unpack_from('<I', res, at)[0]
    tail = None
    for k in range(1, 256):
        q = p - 8 - 4 * k
        if q < 8:
            break
        if u32(q) == 1 and u32(q + 4) == k:
            tail = q, k
            break
    if tail is None:
        return None
    n = u32(0)
    at = 8 + 4 * n
    words = [u32(8 + 4 * i) for i in range(n)]
    a = struct.unpack_from('<H', res, at)[0]
    at += 2 + 12 * a
    c = struct.unpack_from('<H', res, at)[0]
    at += 2 + 24 * c
    q, k = tail
    return words, res[at:q], [u32(q + 8 + 4 * i) for i in range(k)]


def assets(items: list[tuple[Path, bool]], mats: dict[str, dict[str, bool]]) -> None:
    files = tail_ok = 0
    forms: collections.Counter[str] = collections.Counter()
    parts_named = parts_total = files_all_named = 0
    part_mats: collections.Counter[str] = collections.Counter()
    face_counts: list[float] = []
    face_eq_map = 0
    unread = 0
    on_flag: collections.Counter[str] = collections.Counter()
    unknown: collections.Counter[str] = collections.Counter()
    for path, stock, names in first_seen(items, lambda n: n.endswith('_havok.dbx') and '_havokasset' not in n):
        if not stock:
            continue
        have = {e.name.lower(): e.name for e in fbrb.entries(path) if not e.deleted}
        pairs = [(n, have.get(n.lower()[:-4] + '.res')) for n in names]
        got = fbrb.read(path, [x for p in pairs for x in p if x])
        for n, r in pairs:
            if r is None:
                continue
            f = dbx.parse(got[n])
            ha = next((i for i in f.instances if i.attr('type') == 'Physics.HavokAsset'), None)
            w = wrapper(got[r])
            if ha is None or w is None:
                unread += 1
                continue
            words, run, hashes = w
            files += 1
            mm = [dbx.value(c) for c in (ha.child('MaterialMappings') or dbx.Element('', 0)).children or ()]
            if [name_hash(m) for m in mm] == hashes:
                tail_ok += 1
            fm = (ha.child('FaceMaterials') or dbx.Element('', 0)).children or ()
            face_counts.append(float(len(fm)))
            face_eq_map += [text(x).lower() for x in fm] == [m.lower() for m in mm]
            pis = (ha.child('PartInfoDatas') or dbx.Element('', 0)).children or ()
            by_hash = {}
            for pi in pis:
                pn, pm = text(pi.child('Name')), text(pi.child('MaterialName'))
                by_hash[name_hash(pn)] = pm
                part_mats[pm.lower()] += 1
                fl = mats.get(pm.lower())
                if fl is None:
                    unknown['not in any DICE grid'] += 1
                else:
                    for k in FLAGS[:3]:
                        if fl[k]:
                            on_flag[k] += 1
            if len(run) < 4 or run[0] or run[1]:
                forms['other'] += 1
                continue
            d, t = run[2], run[3]
            if d == 1 and len(run) == 4:
                forms['d = 1 (one material, 4 bytes)'] += 1
                forms['d = 1 with 2 or more parts'] += len(words) > 1
                idx = [t] * len(words)
            elif len(run) == 4 + len(words):
                forms['one byte per part'] += 1
                idx = list(run[4:])
            else:
                forms['other length'] += 1
                continue
            named = 0
            for wd, ix in zip(words, idx):
                want = by_hash.get(wd)
                if want is not None and ix < len(mm) and mm[ix].lower() == want.lower():
                    named += 1
            parts_named += named
            parts_total += len(words)
            files_all_named += named == len(words)
    print('== object physics assets (DICE bundles: <blueprint>_Havok.dbx with its .res)')
    print(f'{files} read ({unread} without a HavokAsset or a readable wrapper)')
    print(f'wrapper material hashes = 33-xor hashes of MaterialMappings, in order: {tail_ok} of {files}')
    print('part run forms:', dict(forms))
    print(f'parts whose byte names their PartInfoData.MaterialName: {parts_named} of {parts_total}; '
          f'files with every part so: {files_all_named}')
    print(f'FaceMaterials items per file: {rng(face_counts)}')
    print(f'face lists equal to MaterialMappings: {face_eq_map}')
    print(f'PartInfoDatas: {sum(part_mats.values())}; part materials (lower case), most used: '
          + ', '.join(f'{m} {c}' for m, c in part_mats.most_common(15)))
    print(f'distinct part materials: {len(part_mats)}; parts on a material with ' +
          ', '.join(f'{k} {v}' for k, v in on_flag.items()) + f'; {dict(unknown)}')


SUFFIX = re.compile(r'_([a-z]+)(?:_?\d+)?$')
SHADER_NAMES = ('DirtColor', 'DirtFactor', 'DirtColor2', 'DirtFactor2', 'Destruction_factor', 'texture_DirtMask',
                'texture_DirtMask2', 'texture_MaterialMask', 'texture_MaterialMask2', 'texture_OcclusionTexture',
                'texture_dynamicMaskTileMap', 'texture_dynamicMaskIndirectionMap', 'dynamicMaskAtlasParams',
                'SootPerPixel', 'SootPerVertex')


def textures(items: list[tuple[Path, bool]]) -> None:
    """Texture-name suffixes (DICE's ITexture entries) and the grime / mask / destruction constant and sampler names
    each DICE Dx11 shader database holds."""
    tex: set[str] = set()
    sfx: collections.Counter[str] = collections.Counter()
    dbs = 0
    has: collections.Counter[str] = collections.Counter()
    seen: set[str] = set()
    words: collections.Counter[str] = collections.Counter()
    for path, stock in items:
        if not stock:
            continue
        want = []
        for e in fbrb.entries(path):
            low = e.name.lower()
            if e.deleted:
                continue
            if e.type == 'ITexture' and low not in tex:
                tex.add(low)
                leaf = low.rsplit('/', 1)[-1][:-4]
                m = SUFFIX.search(leaf)
                sfx[m.group(1) if m else '(none)'] += 1
                for word in ('camo', 'dirt', 'grime', 'rust', 'msk_', 'dt_'):
                    words[word] += word in leaf
            if e.type == 'Dx11ShaderDatabase' and low not in seen:
                seen.add(low)
                want.append(e.name)
        for n in want:
            d = fbrb.read(path, [n])[n]
            dbs += 1
            for k in SHADER_NAMES:
                has[k] += (k.encode() + b'\0') in d
    print('== textures (DICE bundles, distinct ITexture names by their last _suffix, digits dropped)')
    print(f'{len(tex)} names; ' + ', '.join(f'_{k} {v}' for k, v in sfx.most_common(40)))
    print('names (last part) containing: ' + ', '.join(f'{k} {v}' for k, v in words.items()))
    print(f'== Dx11 shader databases holding each name (of {dbs} DICE databases)')
    print(', '.join(f'{k} {has[k]}' for k in SHADER_NAMES))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    ap.add_argument('--stock', action='store_true', help="DICE's bundles only")
    ap.add_argument('--no-shaders', action='store_true', help='skip the texture and shader-database names')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    items = bundles(game, a.stock)
    print(f'{len(items)} level bundles ({sum(1 for b in items if b[1])} DICE) under {os.fspath(game)}')
    mats = grids(items)
    assets(items, mats)
    if not a.no_shaders:
        textures(items)


if __name__ == '__main__':
    main()
