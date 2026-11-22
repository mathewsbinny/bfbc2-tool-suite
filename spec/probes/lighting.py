"""Lights, baked lighting data, effects and emitters, weather data and sky props as the game copy ships them
(read-only; spec/engine/lighting.md, spec/engine/weather.md, spec/engine/effects.md).

    python spec/probes/lighting.py [--opfor <folder>] [--stock] [section ...]

Sections (all when none is given): lights, emitters, baked, weather. Reads the level bundles of Project OPFOR
BFBC2's game copy (`game/**/levels/*/*.fbrb`), one bundle at a time, DICE's bundles first (a bundle is DICE's when its
file date is before 2013, as the install leaves them), Package before Dist within each (the copy the game reads).
--stock reads DICE's bundles only. Prints structure, counts and numeric ranges only:

- lights: per level partition (`levels/<lv>.dbx`), the `WorldRender.PointLightEntityData` instances and their fields'
  ranges, the `StaticEnvmapEntityData` instances, the placed effects (`GameEffectEntityData`, camera-proximity ones)
  and the placements of moon and god-ray blueprints;
- emitters: every effect / emitter partition (names holding `fx/` or `emitters/`), each name once with its
  `levels/<lv>/` prefix taken off: the primary classes, the emitter templates acting as point lights and their
  `PointLight*` fields, the effects' `MaxInstanceCount` / `CullDistance`, and the levels' two emitter atlases;
- baked: the texture names by DICE's occlusion suffixes, the `IrradianceVolume` resources (header and voxel bytes),
  the reflection cubes the envmap entities name;
- weather: the `weather/` partitions by primary class, states per system, and the `WeatherSystem` resources.
"""
from __future__ import annotations

import argparse
import collections
import re
import struct
import sys
import time
from collections.abc import Callable, Iterator
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths

LEVEL_PARTITION = re.compile(r'levels/[^/]+\.dbx')
LEVEL_PREFIX = re.compile(r'^levels/[^/]+/')
WEATHER_FIELDS = {
    'WeatherSystemState': frozenset({'ViewDistance', 'WorldViewDistance', 'ShadowViewDistance', 'ApplyFog'}),
    'DynamicOutdoorLightData': frozenset({'KeyColor', 'KeyColorIntensity', 'TopColor', 'TopColorIntensity',
                                          'BottomColor', 'BottomColorIntensity', 'KeySpecularScale', 'ShadowScale',
                                          'ReflectionEnvmapScale', 'DiffuseEnvmapScale'}),
    'DynamicSunData': frozenset({'Theta', 'Phi', 'Gain', 'Size', 'Color'}),
    'DynamicFogData': frozenset({'Start', 'End', 'LinearPoint1', 'LinearPoint2', 'LinearPoint3', 'Color'}),
    'DynamicPostProcessData': frozenset({'MiddleGray', 'MinExposure', 'MaxExposure', 'BloomScale', 'Brightness',
                                         'Contrast', 'Saturation'}),
}
WHOLE = {'MaxCount', 'MaxInstanceCount', 'ShadowbufferResolution', 'ShadowmapResolution', 'Enumeration'}


def bundles(game: Path, stock_only: bool) -> list[tuple[Path, bool]]:
    """The level bundles, DICE's first, Package before Dist, each with whether it is DICE's."""
    out = []
    for p in sorted(game.glob('**/levels/*/*.fbrb')):
        stock = time.gmtime(p.stat().st_mtime).tm_year < 2013
        if stock or not stock_only:
            out.append((p, stock))
    return sorted(out, key=lambda b: (not b[1], 'package' not in str(b[0]).lower(), str(b[0]).lower()))


def first_seen(items: list[tuple[Path, bool]], want: Callable[[str], bool],
               key: Callable[[str], str] = lambda n: n) -> Iterator[tuple[Path, bool, list[str]]]:
    """Per bundle, the entry names `want(lower name)` accepts whose key no earlier bundle held."""
    seen: set[str] = set()
    for path, stock in items:
        names = []
        for e in fbrb.entries(path):
            low = e.name.lower()
            if e.deleted or not want(low) or key(low) in seen:
                continue
            seen.add(key(low))
            names.append(e.name)
        if names:
            yield path, stock, names


def scalar(e: dbx.Element | None) -> float | None:
    """A 4-byte number (float) read from its binary value, or None."""
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


def vector(e: dbx.Element | None) -> tuple[float, ...] | None:
    """A Vec2 / Vec3 / Vec4 field as floats (the first three components of a Vec4: the fourth is padding)."""
    if e is not None and e.kind == dbx.NUMBERS:
        size, vals = dbx.numbers(e)
        if size == 4 and len(vals) > 1:
            return tuple(float(struct.unpack('>f', v)[0]) for v in vals[:3])
        return None
    if e is None or not e.children:
        return None
    out = [scalar(c) for c in e.children]
    return None if any(v is None for v in out) else tuple(v for v in out if v is not None)


def ref_of(e: dbx.Element, name: str) -> str:
    """The reference a field holds ('null' when none, '' when the field is absent)."""
    c = e.child(name)
    return '' if c is None else (c.attr('ref') or 'null')


def flag(e: dbx.Element | None) -> bool:
    return e is not None and len(e.raw) >= 2 and e.raw[1] == 1


def rng(vals: list[float]) -> str:
    if not vals:
        return 'none'
    c = collections.Counter(round(v, 4) for v in vals)
    top = ', '.join(f'{v:g} x {n}' for v, n in c.most_common(4))
    return f'{min(vals):g} .. {max(vals):g} ({len(vals)} values, {len(c)} distinct; most: {top})'


def fields(insts: list[dbx.Element], skip: tuple[str, ...] = ('Transform', 'Name'),
           only: frozenset[str] | None = None) -> None:
    """Every field of the instances: numbers as ranges, vectors as per-component ranges, the rest as counts."""
    nums: dict[str, list[float]] = collections.defaultdict(list)
    vecs: dict[str, list[tuple[float, ...]]] = collections.defaultdict(list)
    other: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    for i in insts:
        for c in i.children or ():
            n = c.attr('name') or ''
            if n in skip or (only is not None and n not in only):
                continue
            v = float(w) if n in WHOLE and (w := whole(c)) is not None else scalar(c)
            if v is not None:
                nums[n].append(v)
                continue
            vv = vector(c)
            if vv is not None:
                vecs[n].append(vv)
                continue
            if c.attr('ref') is not None:
                other[n]['null' if c.attr('ref') in (None, '', 'null') else 'a reference'] += 1
            elif c.kind != dbx.CONTAINER:
                other[n][dbx.value(c)[:12]] += 1
    for n in sorted(nums):
        print(f'      {n}: {rng(nums[n])}')
    for n in sorted(vecs):
        comps = list(zip(*vecs[n]))
        print(f'      {n}: ' + '; '.join(f'[{k}] {min(x):g} .. {max(x):g}' for k, x in enumerate(comps))
              + f' ({len(vecs[n])} values, {len(set(vecs[n]))} distinct)')
    for n in sorted(other):
        print(f'      {n}: {dict(other[n].most_common(4))}')


def of_type(f: dbx.DbxFile, suffix: str) -> list[dbx.Element]:
    return [i for i in f.instances if (i.attr('type') or '').endswith(suffix)]


def lights(items: list[tuple[Path, bool]]) -> None:
    print('== lights: level partitions')
    pl_levels: dict[str, int] = {}
    pl_all: list[dbx.Element] = []
    env_levels: dict[str, list[tuple[float | None, str]]] = {}
    env_all: list[dbx.Element] = []
    fx_placed: collections.Counter[str] = collections.Counter()
    fx_prox: collections.Counter[str] = collections.Counter()
    moons: dict[str, int] = {}
    godrays: dict[str, int] = {}
    levels = 0
    for path, stock, names in first_seen(items, lambda n: bool(LEVEL_PARTITION.fullmatch(n))
                                         and '_description' not in n):
        got = fbrb.read(path, names)
        for n in names:
            try:
                f = dbx.parse(got[n])
            except ValueError:
                continue
            if not of_type(f, '.LevelData'):
                continue
            levels += 1
            lv = n.lower()[7:-4] + ('' if stock else ' (OPFOR)')
            pls = of_type(f, '.PointLightEntityData')
            if pls:
                pl_levels[lv] = len(pls)
                pl_all.extend(pls)
            envs = of_type(f, '.StaticEnvmapEntityData')
            env_all.extend(envs)
            if envs:
                env_levels[lv] = [(scalar(e.child('Radius')), ref_of(e, 'Texture')) for e in envs]
            for e in of_type(f, '.GameEffectEntityData'):
                fx_placed[lv] += 1
                if flag(e.child('IsCameraProximityTriggered')):
                    fx_prox[lv] += 1
            for e in of_type(f, '.ReferenceObjectData'):
                r = ref_of(e, 'ReferencedObject').lower()
                if '/moon/' in r:
                    moons[lv] = moons.get(lv, 0) + 1
                if 'godray' in r:
                    godrays[lv] = godrays.get(lv, 0) + 1
    print(f'  level partitions: {levels}')
    print(f'  PointLightEntityData: {len(pl_all)} in {len(pl_levels)} levels: {dict(sorted(pl_levels.items()))}')
    fields(pl_all)
    print(f'  StaticEnvmapEntityData: {len(env_all)} in {len(env_levels)} levels; per level count: '
          f'{dict(collections.Counter(len(v) for v in env_levels.values()))}')
    fields(env_all, skip=('Transform', 'Name', 'Texture'))
    print(f'  placed GameEffectEntityData: {sum(fx_placed.values())} in {len(fx_placed)} levels; camera-proximity '
          f'{sum(fx_prox.values())} in {len(fx_prox)} levels: {dict(sorted(fx_prox.items()))}')
    print(f'  moon placements: {dict(sorted(moons.items()))}')
    print(f'  god-ray placements: {sum(godrays.values())} in {len(godrays)} levels')


def emitters(items: list[tuple[Path, bool]]) -> None:
    print('== emitters: effect and emitter partitions (each name once, level prefix taken off)')
    primary: collections.Counter[str] = collections.Counter()
    pl: list[dbx.Element] = []
    pl_names: list[str] = []
    templates = 0
    eff: list[dbx.Element] = []
    count = 0
    for path, _stock, names in first_seen(items, lambda n: n.endswith('.dbx') and ('fx/' in n or 'emitters/' in n),
                                          key=lambda n: LEVEL_PREFIX.sub('', n)):
        for i in range(0, len(names), 1500):
            part = names[i:i + 1500]
            got = fbrb.read(path, part)
            for n in part:
                try:
                    f = dbx.parse(got[n])
                except ValueError:
                    continue
                count += 1
                ins = f.instances
                if ins:
                    primary[ins[0].attr('type') or '?'] += 1
                for t in of_type(f, '.EmitterTemplateData'):
                    templates += 1
                    if flag(t.child('ActAsPointLight')):
                        pl.append(t)
                        pl_names.append(LEVEL_PREFIX.sub('', n.lower()))
                for e in of_type(f, '.EffectEntityData'):
                    eff.append(e)
    print(f'  partitions: {count}; primary classes: {dict(primary.most_common(12))}')
    print(f'  EmitterTemplateData: {templates}; acting as point lights: {len(pl)}')
    print('  folders of the point-light emitters: '
          + str(dict(collections.Counter('/'.join(n.split('/')[:3]) for n in pl_names).most_common(8))))
    sub = [c for c in (pl[0].children or ()) if 'Light' in (c.attr('name') or '')] if pl else []
    print(f'  point-light fields: {[c.attr("name") for c in sub]}')
    lightish = [dbx.Element(t.tag, t.kind, t.attributes, t.text, t.raw,
                            [c for c in (t.children or ()) if 'PointLight' in (c.attr('name') or '')
                             or c.attr('name') in ('MaxCount', 'LifeTime', 'Emissive')]) for t in pl]
    fields(lightish)
    print(f'  EffectEntityData: {len(eff)}')
    fields([dbx.Element(e.tag, e.kind, e.attributes, e.text, e.raw,
                        [c for c in (e.children or ()) if c.attr('name') in
                         ('MaxInstanceCount', 'CullDistance', 'MinScreenArea', 'ParticleCullingFactor')])
            for e in eff])
    print('  emitter atlases per level bundle:')
    atl: collections.Counter[str] = collections.Counter()
    sizes: dict[str, collections.Counter[tuple[int, int, int]]] = collections.defaultdict(collections.Counter)
    for path, _stock, names in first_seen(items, lambda n: n.endswith(('emittertexturealpha.res',
                                                                        'emittertexturenormals.res'))):
        got = fbrb.read(path, names)
        for n in names:
            raw = got[n]
            kind = 'alpha' if 'alpha' in n.lower() else 'normals'
            atl[kind] += 1
            if len(raw) >= 92:
                fmt, w, h = struct.unpack_from('<3I', raw, 8)[0], *struct.unpack_from('<2I', raw, 16)
                sizes[kind][(fmt, w, h)] += 1
    print(f'    {dict(atl)}; (format, width, height): ' + '; '.join(f'{k} {dict(v.most_common(4))}'
                                                                   for k, v in sizes.items()))


def baked(items: list[tuple[Path, bool]]) -> None:
    print('== baked')
    suffix: collections.Counter[str] = collections.Counter()
    seen: set[str] = set()
    irr: list[tuple[str, bytes]] = []
    for path, _stock in items:
        want = []
        for e in fbrb.entries(path):
            low = e.name.lower()
            if e.deleted or low in seen:
                continue
            if e.type == 'ITexture':
                seen.add(low)
                stem = low.rsplit('/', 1)[-1].removesuffix('.res')
                for s in ('_skyoc', '_ocrgb', '_oc', '_occlusion', '_ambientocclusion', '_ao', '_lightmap',
                          '_lightmapcolor', '_envmap'):
                    if stem.endswith(s):
                        suffix[s] += 1
                        break
            elif e.type == 'IrradianceVolume':
                seen.add(low)
                want.append(e.name)
        if want:
            got = fbrb.read(path, want)
            irr.extend((n, got[n]) for n in want)
    print(f'  distinct ITexture names by suffix: {dict(suffix.most_common())}')
    fit = 0
    vox: collections.Counter[int] = collections.Counter()
    cells: list[float] = []
    dims: list[int] = []
    for _n, raw in irr:
        if len(raw) < 36:
            continue
        lo, hi = struct.unpack_from('<3f', raw, 0), struct.unpack_from('<3f', raw, 12)
        nx, ny, nz = struct.unpack_from('<3I', raw, 24)
        if 36 + nx * ny * nz == len(raw):
            fit += 1
            body = raw[36:]
            vox.update(body)
            dims.append(nx * ny * nz)
            for k, d in enumerate((nx, ny, nz)):
                if d > 1:
                    cells.append((hi[k] - lo[k]) / (d - 1))
    print(f'  IrradianceVolume resources: {len(irr)}; 36-byte header + nx*ny*nz bytes exactly: {fit}')
    if vox:
        print(f'    voxel bytes {min(vox)} .. {max(vox)}; at the floor ({min(vox)}): {vox[min(vox)]}, at 255: {vox[255]} '
              f'of {sum(vox.values())}; voxels per volume {min(dims)} .. {max(dims)}')
        print(f'    cell size, extent / (n - 1): {rng(cells)}')
        print(f'    cell size, extent / n: see the extent and counts; distinct stems: {len({n for n, _ in irr})}')


def weather(items: list[tuple[Path, bool]]) -> None:
    print('== weather')
    prim: collections.Counter[str] = collections.Counter()
    states: dict[str, int] = {}
    comp: dict[str, list[dbx.Element]] = collections.defaultdict(list)
    res: collections.Counter[tuple[int, int]] = collections.Counter()
    for path, _stock, names in first_seen(items, lambda n: n.startswith('weather/')):  # noqa: B007
        dbxs = [n for n in names if n.lower().endswith('.dbx')]
        rs = [n for n in names if n.lower().endswith('.res')]
        got = fbrb.read(path, names)
        for n in rs:
            raw = got[n]
            res[(len(raw), int.from_bytes(raw[:4], 'little') if len(raw) >= 4 else -1)] += 1
        for n in dbxs:
            try:
                f = dbx.parse(got[n])
            except ValueError:
                continue
            ins = f.instances
            if not ins:
                continue
            prim[ins[0].attr('type') or '?'] += 1
            if _stock:
                comp[(ins[0].attr('type') or '?').split('.')[-1]].append(ins[0])
            for s in of_type(f, '.WeatherSystemData'):
                arr = s.child('States')
                states[n.lower()] = len(arr.children or ()) if arr is not None else 0
    print(f'  primary classes: {dict(prim.most_common())}')
    print(f'  systems: {len(states)}; states listed: {sum(states.values())}; per system '
          f'{min(states.values()) if states else 0} .. {max(states.values()) if states else 0}')
    print(f'  resources (bytes, first u32 LE): {dict(res)}')
    print("  the values VW3's editor offers, over DICE's state partitions:")
    for cls, keep in WEATHER_FIELDS.items():
        print(f'    {cls} ({len(comp[cls])}):')
        fields(comp[cls], only=keep)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 (default: found beside the suite)')
    ap.add_argument('--stock', action='store_true', help="DICE's bundles only")
    ap.add_argument('sections', nargs='*', default=['lights', 'emitters', 'baked', 'weather'])
    a = ap.parse_args()
    opfor = paths.find_opfor(a.opfor)
    if opfor is None:
        sys.exit('Project OPFOR BFBC2 not found (--opfor)')
    items = bundles(Path(paths.game_dir(opfor)), a.stock)
    print(f'level bundles: {len(items)} ({sum(1 for _, s in items if s)} DICE\'s)')
    for s in a.sections:
        {'lights': lights, 'emitters': emitters, 'baked': baked, 'weather': weather}[s](items)


if __name__ == '__main__':
    main()
