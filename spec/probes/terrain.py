"""Terrain, vegetation and decals as DICE's files hold them: what a terrain edit must write and what the game draws
from (read-only; spec/transforms/terrain_edit.md, spec/engine/vegetation.md, spec/engine/decals.md).

    python spec/probes/terrain.py [--opfor <folder>]

Reads the bundles of Project OPFOR BFBC2's game copy that are DICE's (`game/**/levels/*/*.fbrb` dated before 2013,
as the install leaves them), one bundle at a time, each resource or partition once (the first bundle that holds it).

Prints structure, counts and numeric ranges only:

- height fields: main blocks' size, grid spacing and height range (max y - min y), sub-terrain rings;
- visual terrains: blocks, road strips (per shader kind, layer), patch shaders (layers, tiled count, undergrowth
  mask shader, undergrowth mesh entries and their 12 floats), patch maps (header, atlas layout, mask flags), the
  names each patch map ends with;
- textures: the mask-tile atlases' sizes and formats against their patch maps' headers;
- terrain partitions (`Terrains/<T>/<T>.dbx`): `TerrainData`, `TerrainHeightfieldData`, the decal classes, which of
  their references resolve to a partition any bundle holds;
- level partitions: the vegetation classes (forest, tree members, middlegrowth groups), their instance counts and
  fields; tree and undergrowth mesh resources;
- the level shader databases' vertex programs with a `windParams` constant (the undergrowth's): counts, the
  constant's register and whether the Dx11 bytecode holds the sway's instruction sequence (VEG-UG-10).
"""
from __future__ import annotations

import argparse
import collections
import re
import struct
import sys
import time
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths

Counter = collections.Counter


def stock_bundles(game: Path) -> list[Path]:
    return sorted((p for p in game.glob('**/levels/*/*.fbrb') if time.gmtime(p.stat().st_mtime).tm_year < 2013),
                  key=lambda p: str(p).lower())


def rng(vals: list[float]) -> str:
    if not vals:
        return 'none'
    c = Counter(vals)
    top = ', '.join(f'{v:g} x {n}' for v, n in c.most_common(5))
    return f'{min(vals):g} .. {max(vals):g} ({len(vals)} values, {len(c)} distinct; most: {top})'


def ref(e: dbx.Element | None) -> str:
    return 'null' if e is None else (e.attr('ref') or 'null')


def items(e: dbx.Element | None) -> list[dbx.Element]:
    return list(e.children or ()) if e is not None else []


def text(e: dbx.Element | None) -> str:
    return '' if e is None else dbx.value(e)


class Walk:
    def __init__(self, b: bytes) -> None:
        self.b, self.o = b, 0

    def u32(self) -> int:
        v = int(struct.unpack_from('<I', self.b, self.o)[0])
        self.o += 4
        return v

    def string(self) -> str:
        e = self.b.index(b'\0', self.o)
        s = self.b[self.o:e].decode('latin-1')
        self.o = e + 1
        return s

    def floats(self, n: int) -> tuple[float, ...]:
        v = struct.unpack_from(f'<{n}f', self.b, self.o)
        self.o += 4 * n
        return tuple(v)


def visual_terrain(d: bytes) -> dict[str, Any]:
    """TERR-VT-1 .. TERR-VT-4, every part kept that the probe counts."""
    w = Walk(d)
    n = w.u32()
    strips = []
    for k in range(2 * n):
        for _ in range(w.u32()):
            shader, second, mask = w.string(), w.string(), w.string()
            _first, tris = w.u32(), w.u32()
            w.floats(6)
            strips.append((k < n, shader, second, mask, tris, w.u32()))
        _v, vb, i = w.u32(), w.u32(), w.u32()
        w.o += vb + 4 * i
    shaders = []
    for _ in range(w.u32()):
        name, ug = w.string(), w.string()
        layers, tiled = w.u32(), w.u32()
        meshes = []
        for _ in range(w.u32()):
            for _ in range(w.u32()):
                mesh = w.string()
                inst = w.string()          # empty on DICE's MP / SP terrains; '<mesh>/<guid>' on NAM_MP_003
                sh = w.string()
                vals = w.floats(12)
                tail = w.b[w.o:w.o + 2]
                w.o += 2
                meshes.append((mesh, inst, sh, vals, tail))
        shaders.append((name, ug, layers, tiled, meshes))
    if w.u32() != n:
        raise ValueError('patch maps do not match the blocks')
    maps = []
    for _ in range(n):
        w.floats(6)
        header = tuple(w.u32() for _ in range(5))
        cells = header[4] * header[4]
        tiles = struct.unpack_from(f'<{4 * cells}H', w.b, w.o)
        w.o += 8 * cells
        flags = w.b[w.o:w.o + 4 * cells]
        w.o += 4 * cells
        s = w.u32()
        smap = w.b[w.o:w.o + s * s]
        w.o += s * s
        hf, atlas = w.string(), w.string()
        maps.append((header, tiles, flags, s, smap, hf, atlas))
    if w.o != len(d):
        raise ValueError('trailing bytes')
    return {'n': n, 'strips': strips, 'shaders': shaders, 'maps': maps}


def strip_kind(shader: str) -> str:
    s = shader.lower().rsplit('/', 1)[-1]
    for k in ('road', 'decal', 'quad', 'track', 'path', 'trail'):
        if k in s:
            return k
    return 'other'


def terrain_resources(bundles: list[Path]) -> None:
    seen: set[str] = set()
    main_n: Counter[int] = Counter()
    yrange: list[float] = []
    ymin: list[float] = []
    spacing: list[float] = []
    sub = 0
    vts = 0
    blocks: Counter[int] = Counter()
    strip_kinds: Counter[str] = Counter()
    strip_layers: Counter[int] = Counter()
    strips_per: list[float] = []
    second_pass = 0
    mask_named = 0
    shaders_per: list[float] = []
    layer_counts: Counter[int] = Counter()
    tiled_counts: Counter[int] = Counter()
    ug_named = 0
    ug_kinds: Counter[str] = Counter()
    meshes_per_shader: list[float] = []
    mesh_vals: dict[int, list[float]] = collections.defaultdict(list)
    mesh_tail: Counter[bytes] = Counter()
    ug_meshes: set[str] = set()
    inst_named: Counter[bool] = Counter()
    third_named = 0
    ug_vs_meshes: Counter[tuple[bool, bool]] = Counter()
    per_main: list[str] = []
    flags_by_w: dict[int, Counter[int]] = collections.defaultdict(Counter)
    tiled_vs_flags: Counter[tuple[int, bool]] = Counter()
    aligned: Counter[bool] = Counter()
    headers: Counter[tuple[int, ...]] = Counter()
    flags: Counter[int] = Counter()
    shader_side: Counter[tuple[int, int]] = Counter()
    hf_named_ok = 0
    atlas_names: dict[str, tuple[int, int]] = {}
    atlas_per_terrain: Counter[int] = Counter()
    texinfo: dict[str, tuple[int, int, int, int]] = {}
    shader_idx_ok = 0
    shader_idx_all = 0
    for path in bundles:
        es = [e for e in fbrb.entries(path) if not e.deleted and e.name.lower() not in seen]
        want = [e.name for e in es if e.type in ('TerrainHeightfield', 'VisualTerrain')]
        tex = [e.name for e in es if e.type == 'ITexture' and 'masktile' in e.name.lower()]
        if not want and not tex:
            continue
        got = fbrb.read(path, want + tex)
        types = {e.name: e.type for e in es}
        for n in want + tex:
            seen.add(n.lower())
            d = got[n]
            if n in tex:
                version, kind, fmt, fl, wd, ht, depth, mips = struct.unpack_from('<8I', d)
                texinfo[n.lower()[:-4] if n.lower().endswith('.res') else n.lower()] = (wd, ht, fmt, mips)
                continue
            if types[n] == 'TerrainHeightfield':
                b = struct.unpack_from('<6f', d)
                size, w20 = struct.unpack_from('<I', d, 0x18)[0], struct.unpack_from('<I', d, 0x20)[0]
                if w20:
                    main_n[size] += 1
                    yrange.append(round(b[4] - b[1], 3))
                    ymin.append(round(b[1], 3))
                    spacing.append(round((b[3] - b[0]) / size, 3))
                    per_main.append(f'{n.split("/")[1]} {size} {b[4] - b[1]:g}')
                else:
                    sub += 1
                continue
            vts += 1
            vt = visual_terrain(d)
            nb = int(vt['n'])
            blocks[nb] += 1
            strips = vt['strips']
            strips_per.append(float(len(strips)))
            for _full, shader, second, mask, _tris, layer in strips:
                strip_kinds[strip_kind(shader)] += 1
                strip_layers[layer] += 1
                second_pass += bool(second)
                mask_named += bool(mask)
            sh = vt['shaders']
            shaders_per.append(float(len(sh)))
            for _name, ug, layers, tiled, meshes in sh:
                layer_counts[layers] += 1
                tiled_counts[tiled] += 1
                ug_named += bool(ug)
                if ug:
                    ug_kinds['UgMaskShaders' if '/UgMaskShaders/' in ug else 'other'] += 1
                ug_vs_meshes[(bool(ug), bool(meshes))] += 1
                meshes_per_shader.append(float(len(meshes)))
                for mesh, inst, _s, vals, tail in meshes:
                    ug_meshes.add(mesh.lower())
                    inst_named[bool(inst)] += 1
                    third_named += bool(_s)
                    mesh_tail[tail] += 1
                    for k, v in enumerate(vals):
                        mesh_vals[k].append(round(v, 3))
            atl = set()
            for header, tiles, fl, s, smap, hf, atlas in vt['maps']:
                headers[header] += 1
                flags.update(fl)
                flags_by_w[header[4]].update(fl)
                side = header[4]
                ratio = max(1, s // max(1, side))
                blocks_of: dict[int, tuple[int, int, int]] = {}
                for c in range(side * side):
                    px, pz = c % side, c // side
                    si = smap[pz * ratio * s + px * ratio]
                    nz = sum(1 for x in fl[4 * c:4 * c + 4] if x)
                    tiled_vs_flags[(side, nz == sh[si][3] if si < len(sh) else False)] += 1
                    for k in range(4):
                        f = fl[4 * c + k]
                        if f:
                            b = (px // f, pz // f, f)
                            aligned[blocks_of.setdefault(tiles[4 * c + k], b) == b] += 1
                shader_side[(header[4], s)] += 1
                hf_named_ok += hf.lower().startswith('terrains/')
                if atlas:
                    atl.add(atlas.lower())
                    atlas_names[atlas.lower()] = (header[2], header[3])
                shader_idx_all += 1
                shader_idx_ok += all(x < len(sh) for x in smap)
            atlas_per_terrain[len(atl)] += 1
    print('== height fields (DICE terrain bundles)')
    print(f'main blocks by size: {dict(main_n)}; sub-terrain blocks: {sub}')
    print(f'main block height range (max y - min y): {rng(yrange)}')
    print(f'main block min y: {rng(ymin)}')
    print(f'main block grid spacing: {rng(spacing)}')
    print(f'main blocks (terrain, n, height range): {"; ".join(sorted(per_main))}')
    print('== visual terrains')
    print(f'{vts} VisualTerrain; blocks per terrain: {dict(blocks)}')
    print(f'road strips per terrain {rng(strips_per)}; by shader name: {dict(strip_kinds)}; layers {dict(sorted(strip_layers.items()))}; '
          f'with a second shader {second_pass}, with a mask shader {mask_named}')
    print(f'patch shaders per terrain {rng(shaders_per)}; layer counts {dict(sorted(layer_counts.items()))}; '
          f'tiled counts {dict(sorted(tiled_counts.items()))}')
    print(f'patch shaders naming an undergrowth mask shader: {ug_named}; kinds {dict(ug_kinds)}')
    print(f'undergrowth mesh entries per patch shader {rng(meshes_per_shader)}; distinct meshes {len(ug_meshes)}; '
          f'by folder: {dict(Counter("/".join(m.split("/")[:2]) if m.startswith("terrains/") else m.split("/")[0] + "/" + m.split("/")[1] for m in ug_meshes).most_common(8))}')
    for k in range(12):
        print(f'  undergrowth entry float {k}: {rng(mesh_vals[k])}')
    print(f'  entry tail bytes: {dict((t.hex(), c) for t, c in mesh_tail.items())}')
    print(f'  entries whose second string is set: {inst_named[True]} of {sum(inst_named.values())}; third string set: '
          f'{third_named}')
    print(f'patch map headers (tile, border, tiles per row - 1, rows - 1, W): {dict(headers.most_common(12))}')
    print(f'patch shaders by (names an undergrowth mask shader, has undergrowth meshes): {dict(ug_vs_meshes)}')
    print(f'mask flags by W: {dict((w, dict(sorted(c.items()))) for w, c in sorted(flags_by_w.items()))}')
    print(f'(W, cells whose non-zero flags equal the tiled count of their patch shader): {dict(tiled_vs_flags)}')
    print(f'tiled masks whose tile lies in one flag x flag block aligned to the flag: {dict(aligned)}')
    print(f'mask flags: {dict(sorted(flags.items()))}; (W, shader map side): {dict(shader_side)}')
    print(f'patch maps naming a terrains/ height field: {hf_named_ok}; shader maps whose indices are in range: '
          f'{shader_idx_ok} of {shader_idx_all}')
    print(f'distinct atlases named per terrain: {dict(atlas_per_terrain)}')
    match: Counter[str] = Counter()
    for a, (tx, ty) in atlas_names.items():
        t = texinfo.get(a)
        if t is None:
            match['atlas texture not read'] += 1
            continue
        wd, ht, fmt, mips = t
        match[f'{wd}x{ht} fmt {fmt} mips {mips}: header {(tx, ty)} '
              f'{"= size / 64 - 1" if (tx + 1) * 64 == wd and (ty + 1) * 64 == ht else "differs"}'] += 1
    print(f'mask-tile atlases (by patch map header): {dict(match)}')


def partitions(bundles: list[Path]) -> None:
    seen: set[str] = set()
    all_names: set[str] = set()
    for path in bundles:
        all_names |= {e.name.lower()[:-4] for e in fbrb.entries(path) if e.name.lower().endswith('.dbx')}
    terr_cls: Counter[str] = Counter()
    td: dict[str, Counter[str]] = collections.defaultdict(Counter)
    hfd: dict[str, Counter[str]] = collections.defaultdict(Counter)
    decal_fields: dict[str, Counter[str]] = collections.defaultdict(Counter)
    decal_refs: Counter[str] = Counter()
    veg_cls: Counter[str] = Counter()
    veg_inst: Counter[str] = Counter()
    veg_fields: dict[str, set[str]] = collections.defaultdict(set)
    veg_levels: Counter[str] = Counter()
    member_types: Counter[str] = Counter()
    tree_res = 0
    ug_res: Counter[str] = Counter()
    res_seen: set[str] = set()
    terrains = levels = 0
    for path in bundles:
        es = [e for e in fbrb.entries(path) if not e.deleted]
        for e in es:
            low = e.name.lower()
            if low in res_seen:
                continue
            res_seen.add(low)
            if e.type == 'TreeMeshSet':
                tree_res += 1
            if '/undergrowth/' in low and e.type.endswith('MeshSet'):
                ug_res[e.type] += 1
        want = [e.name for e in es if e.name.lower() not in seen and (
            re.fullmatch(r'terrains/([^/]+)/\1\.dbx', e.name.lower())
            or (re.fullmatch(r'levels/[^/]+\.dbx', e.name.lower()) and '_description' not in e.name.lower())
            or re.fullmatch(r'levels/([^/]+)/\1\.dbx', e.name.lower()))]
        if not want:
            continue
        got = fbrb.read(path, want)
        for n in want:
            seen.add(n.lower())
            f = dbx.parse(got[n])
            types = [(i.attr('type') or '').split('.')[-1] for i in f.instances]
            if n.lower().startswith('terrains/'):
                terrains += 1
                for i in f.instances:
                    t = (i.attr('type') or '').split('.')[-1]
                    terr_cls[t] += 1
                    target = td if t == 'TerrainData' else hfd if t == 'TerrainHeightfieldData' else None
                    if target is not None:
                        for c in items(i):
                            nm = c.attr('name') or ''
                            if c.kind == dbx.CONTAINER and c.tag == 'array':
                                target[nm][f'{len(items(c))} items'] += 1
                            elif c.attr('ref') is not None:
                                target[nm]['null' if ref(c) == 'null' else 'set'] += 1
                            else:
                                target[nm][text(c)[:24]] += 1
                    if 'Decal' in t:
                        for k in ('MaskTextureType', 'MaskPixelsPerMeter', 'GridCellSize', 'Enable', 'UseInnerSpline'):
                            fe = i.child(k)
                            if fe is not None:
                                decal_fields[f'{t}.{k}'][text(fe)] += 1
                        if items(i.child('MaskTextures')):
                            decal_fields[f'{t}.MaskTextures'][f'{len(items(i.child("MaskTextures")))} items'] += 1
                        for k in ('Material', 'Spline', 'InnerSpline'):
                            r = ref(i.child(k))
                            if r != 'null':
                                decal_refs[f'{t}.{k} ' + ('in a bundle' if r.rsplit('/', 1)[0] in all_names else 'in no bundle')] += 1
                continue
            if 'LevelData' not in types:
                continue
            levels += 1
            for i in f.instances:
                t = (i.attr('type') or '').split('.')[-1]
                if any(k in t for k in ('Forest', 'Tree', 'Middlegrowth', 'Undergrowth', 'Vegetation', 'Decal', 'Road',
                                        'Spline')):
                    veg_cls[t] += 1
                    veg_levels[t] += 0
                    veg_fields[t] |= {c.attr('name') or '' for c in items(i)}
                    ic = i.child('InstanceCount')
                    if ic is not None and ic.kind == dbx.NUMBERS:
                        size, vals = dbx.numbers(ic)
                        veg_inst[t] += int.from_bytes(vals[0], 'big', signed=True) if vals else 0
                    if t.endswith('MemberData'):
                        member_types[f'{t} -> {ref(i.child("MemberType")).split("/")[0]}'] += 1
            for t in {x for x in types if any(k in x for k in ('Forest', 'Middlegrowth'))}:
                veg_levels[t] += 1
    print('== terrain partitions (Terrains/<T>/<T>.dbx, DICE bundles)')
    print(f'{terrains} terrain partitions; classes: {dict(terr_cls)}')
    for k, cnt in sorted(td.items()):
        print(f'  TerrainData.{k}: {dict(cnt.most_common(6))}')
    for k, cnt in sorted(hfd.items()):
        print(f'  TerrainHeightfieldData.{k}: {dict(cnt.most_common(6))}')
    for k, cnt in sorted(decal_fields.items()):
        print(f'  {k}: {dict(cnt.most_common(8))}')
    print(f'  decal references: {dict(decal_refs)}')
    print('== vegetation and decals in level partitions (DICE)')
    print(f'{levels} level partitions; classes: {dict(veg_cls)}; levels holding: {dict(veg_levels)}')
    print(f'InstanceCount sums: {dict(veg_inst)}')
    print(f'member types: {dict(member_types.most_common(12))}')
    for t, fs in sorted(veg_fields.items()):
        print(f'  {t} fields: {sorted(fs)}')
    print(f'TreeMeshSet resources: {tree_res}; undergrowth mesh sets (in an undergrowth folder): {dict(ug_res)}')


def pyramid_sides(n: int) -> list[int]:
    """TERR-HF-11: the lower-envelope grids' sides, the coarsest first: 2, 3, 5, ..., n / 8 + 1."""
    sides = []
    k = n // 8
    while k >= 1:
        sides.append(k + 1)
        k //= 2
    return sides[::-1]


def minmax_tree(h: Any, n: int) -> Any:
    """TERR-HF-10: (min, max) per node, the root first, each level row by row (rows along z) down to 16-sample cells;
    a node covers its cells' samples and the row and column after them (inside the block)."""
    import numpy as np
    out = []
    size = n
    while size >= 16:
        k = n // size
        lo = h.reshape(k, size, k, size).min(axis=(1, 3))
        hi = h.reshape(k, size, k, size).max(axis=(1, 3))
        if k > 1:
            # widened by the first row and column of the next cells
            edge_r = h[size::size, :]
            lo[:-1, :] = np.minimum(lo[:-1, :], edge_r.reshape(k - 1, k, size).min(axis=2))
            hi[:-1, :] = np.maximum(hi[:-1, :], edge_r.reshape(k - 1, k, size).max(axis=2))
            edge_c = h[:, size::size]
            lo[:, :-1] = np.minimum(lo[:, :-1], edge_c.reshape(k, size, k - 1).min(axis=1))
            hi[:, :-1] = np.maximum(hi[:, :-1], edge_c.reshape(k, size, k - 1).max(axis=1))
            corner = h[size::size, size::size]
            lo[:-1, :-1] = np.minimum(lo[:-1, :-1], corner)
            hi[:-1, :-1] = np.maximum(hi[:-1, :-1], corner)
        out.append(np.stack([lo, hi], axis=-1).reshape(-1))
        size //= 2
    return np.concatenate(out).astype('<u2')


def main_tail(d: bytes) -> dict[str, Any]:
    """A main height-field block's tail (TERR-HF-4, TERR-HF-9 - TERR-HF-11): the depth map's rows, the padding, the
    min / max tree and the lower-envelope grids, and whether the first three re-encode to the file's bytes."""
    import numpy as np
    n = struct.unpack_from('<I', d, 0x18)[0]
    a = struct.unpack_from('<I', d, 0x2D)[0]
    end = 0x31 + 2 * n * n
    h = np.frombuffer(d, '<u2', n * n, 0x31).reshape(n, n)
    offsets = np.frombuffer(d, '<u4', n, end)
    runs = np.frombuffer(d, '<u2', offset=end + 4 * n)
    depth = np.zeros((n, n), np.uint8)
    maximal = True
    o = 0
    for r in range(n):
        if offsets[r] != o:
            raise ValueError(f'row {r}: offset {offsets[r]}, expected {o}')
        s, prev = 0, -1
        while s < n:
            c, v = int(runs[o]), int(runs[o + 1])
            if c == 0 or v > 255 or s + c > n:
                raise ValueError(f'row {r}: run ({c}, {v})')
            maximal &= v != prev
            depth[r, s:s + c] = v
            s, prev, o = s + c, v, o + 2
    rle = 2 * o
    pad = (-(4 * n + rle)) % 16
    tree = minmax_tree(h, n)
    sides = pyramid_sides(n)
    grids = sum(s * s for s in sides)
    tail_len = len(d) - end
    body = end + 4 * n + rle
    re_encoded = (a == 4 * n + rle + pad and tail_len == 4 * n + rle + pad + 2 * len(tree) + 2 * grids
                  and d[body:body + pad] == bytes(pad)
                  and d[body + pad:body + pad + 2 * len(tree)] == tree.tobytes())
    flat = np.frombuffer(d, '<u2', grids, len(d) - 2 * grids).astype(np.int64)
    levels, at = [], 0
    for s in sides:
        levels.append(flat[at:at + s * s].reshape(s, s))
        at += s * s
    return {'n': n, 'depth': depth, 'maximal': maximal, 'pad': pad, 'tree': len(tree), 'grids': levels,
            'heights': h, 're_encoded': re_encoded}


def below(level: Any, h: Any) -> float:
    """The share of height samples at or under a lower-envelope grid, each grid square split along its
    (i, j)-(i+1, j+1) diagonal (TERR-HF-3)."""
    import numpy as np
    n = h.shape[0]
    s = level.shape[0]
    step = n // (s - 1)
    z, x = np.mgrid[0:n, 0:n]
    i, j = np.minimum(z // step, s - 2), np.minimum(x // step, s - 2)
    fz, fx = (z - i * step) / step, (x - j * step) / step
    g = level.astype(np.float64)
    a, b, c, e = g[i, j], g[i, j + 1], g[i + 1, j], g[i + 1, j + 1]
    v = np.where(fx >= fz, a + (b - a) * fx + (e - b) * fz, a + (c - a) * fz + (e - c) * fx)
    return float((h >= v - 0.5).mean())


def encode_material_row(cells: Any) -> bytes:
    """TERR-MM-2: one row of a block (512 cells) as DICE writes it."""
    b = (cells[0::2] | (cells[1::2] << 4)).astype('u1').tolist()
    out = bytearray()
    k = 0
    while k < len(b):
        e = k
        while e < len(b) and b[e] == b[k]:
            e += 1
        left = e - k
        while left:
            if left == 1:
                out.append(b[k])
                left = 0
            else:
                take = min(left, 256)
                out += bytes([b[k], b[k], take - 1])
                left -= take
        k = e
    return bytes(out)


def material_map(d: bytes) -> dict[str, Any]:
    """TERR-MM-1 - TERR-MM-3: the cells' material indices (rows along z) and the names, and whether the cells
    re-encode to the file's bytes."""
    import numpy as np
    w, nb = struct.unpack_from('<IH', d, 0)
    per_row = w // 512
    cells = np.zeros(((nb // per_row) * 256, w), np.uint8)
    o = 6
    out = bytearray(d[:6])
    for k in range(nb):
        length = struct.unpack_from('<I', d, o)[0]
        data = d[o + 4:o + 4 + length]
        table = struct.unpack_from('<256H', d, o + 4 + length)
        o += 4 + length + 512
        by, bx = divmod(k, per_row)
        at = 0
        rows = []
        for r in range(256):
            row = data[at:at + table[r]]
            at += table[r]
            pairs: list[int] = []
            prev = -1
            q = 0
            while q < len(row):
                x = row[q]
                q += 1
                if x == prev:
                    pairs += [x] * row[q]
                    q += 1
                    prev = -1
                else:
                    pairs.append(x)
                    prev = x
            if len(pairs) != 256:
                raise ValueError(f'block {k} row {r}: {len(pairs)} pairs')
            line = np.array(pairs, np.uint8)
            cells[by * 256 + r, bx * 512:(bx + 1) * 512:2] = line & 15
            cells[by * 256 + r, bx * 512 + 1:(bx + 1) * 512:2] = line >> 4
            rows.append(encode_material_row(cells[by * 256 + r, bx * 512:(bx + 1) * 512]))
        block = b''.join(rows)
        out += struct.pack('<I', len(block)) + block + struct.pack('<256H', *(len(r) for r in rows))
    size = struct.unpack_from('<I', d, o)[0]
    names = [x.decode('latin-1') for x in d[o + 4:o + 4 + size].split(b'\0') if x]
    out += d[o:]
    return {'width': w, 'blocks': nb, 'cells': cells, 'names': names, 're_encoded': bytes(out) == d}


def tails_and_material_maps(bundles: list[Path]) -> None:
    import numpy as np
    print('== main height-field tails and material maps')
    seen: set[str] = set()
    for path in bundles:
        es = [e for e in fbrb.entries(path) if not e.deleted and e.name.lower() not in seen
              and e.type in ('TerrainHeightfield', 'TerrainMaterialMap')]
        for e in es:
            seen.add(e.name.lower())
            d = fbrb.read(path, [e.name])[e.name]
            t = e.name.split('/')[1]
            if e.type == 'TerrainHeightfield':
                if not struct.unpack_from('<I', d, 0x20)[0]:
                    continue
                m = main_tail(d)
                dep = m['depth']
                lows = ' '.join(f'{lv.shape[0]}:{below(lv, m["heights"]):.4f}' for lv in m['grids'])
                print(f'  {t} n {m["n"]}: re-encoded {m["re_encoded"]}, runs maximal {m["maximal"]}, pad {m["pad"]}; '
                      f'depth 255 {float((dep == 255).mean()):.3f}, 0 {float((dep == 0).mean()):.3f}; '
                      f'grid samples at or under: {lows}')
            else:
                mm = material_map(d)
                used = np.bincount(mm['cells'].ravel(), minlength=16)
                print(f'  {t} material map {mm["width"]} x {mm["cells"].shape[0]}, {mm["blocks"]} blocks: '
                      f're-encoded {mm["re_encoded"]}; {len(mm["names"])} names, cells per index '
                      f'{[int(x) for x in used[:len(mm["names"])]]}')


def dxbc_instructions(code: bytes) -> list[list[int]]:
    """The SHDR / SHEX chunk's instructions, each its tokens (opcode token first)."""
    if code[:4] != b'DXBC':
        return []
    n = struct.unpack_from('<I', code, 28)[0]
    for k in range(n):
        o = struct.unpack_from('<I', code, 32 + 4 * k)[0]
        if code[o:o + 4] in (b'SHDR', b'SHEX'):
            size = struct.unpack_from('<I', code, o + 4)[0]
            toks = struct.unpack_from(f'<{size // 4}I', code, o + 8)
            out, i = [], 2                         # after the version and length tokens
            while i < len(toks):
                op = toks[i] & 0x7FF
                ln = toks[i + 1] if op == 0x35 else (toks[i] >> 24) & 0x7F   # 0x35: dcl_customdata
                out.append(list(toks[i:i + max(ln, 1)]))
                i += max(ln, 1)
            return out
    return []


def undergrowth_shaders(bundles: list[Path]) -> None:
    """VEG-UG-10: the vertex programs whose parameter block names windParams, and their sway in the bytecode."""
    sys.path.insert(0, str(SUITE / 'bfbc2-shader-tools'))
    import shaderdb
    one_k = 0x447A0000                              # 1000.0
    seen: set[str] = set()
    dbs: Counter[str] = Counter()
    all_dbs: Counter[str] = Counter()
    vs_n: Counter[str] = Counter()
    regs: Counter[int] = Counter()
    shape: Counter[bool] = Counter()
    codes: set[bytes] = set()
    for path in bundles:
        for e in fbrb.entries(path):
            kind = e.type.replace('ShaderDatabase', '')
            if e.deleted or kind not in ('Dx9', 'Dx10', 'Dx11') or e.name.lower() + kind in seen:
                continue
            seen.add(e.name.lower() + kind)
            db = shaderdb.ShaderDb.parse(fbrb.read(path, [e.name])[e.name], int(kind[2:]))
            blocks = {}
            for i, pb in enumerate(db.params):
                for c in shaderdb.block_constants(pb, db.e):
                    if c[0] == 'windParams':
                        blocks[i] = c[1]
            hits = [s for s in db.vs if s.param in blocks]
            dbs[kind] += bool(hits)
            all_dbs[kind] += 1
            vs_n[kind] += len(hits)
            if kind != 'Dx11':
                continue
            for s in hits:
                if s.code in codes:
                    continue
                codes.add(s.code)
                reg = blocks[s.param]
                regs[reg] += 1
                ins = dxbc_instructions(s.code)
                ops = [t[0] & 0x7FF for t in ins]
                # dp3 (0x10) of the position by (1, 1, 1); mad (0x32) by 1000 plus windParams.x; mul (0x38) by .y;
                # sincos (0x4D); max (0x34) of y and 0; mul (0x38) by .z; mad (0x32) into x and z (write mask 0b0101)
                k = next((j for j, t in enumerate(ins) if ops[j] == 0x32 and one_k in t[1:]), -1)
                shape[0 < k < len(ins) - 5 and ops[k - 1] == 0x10 and ins[k - 1].count(0x3F800000) == 3
                      and ops[k + 1:k + 6] == [0x38, 0x4D, 0x34, 0x38, 0x32] and (ins[k + 5][1] >> 4) & 0xF == 5
                      and reg in ins[k] and reg in ins[k + 1] and reg in ins[k + 4]] += 1
    print('== undergrowth sway (vertex programs with a windParams constant, DICE databases)')
    print(f'databases with such a program: {dict(sorted(dbs.items()))} of {dict(sorted(all_dbs.items()))}; programs: {dict(sorted(vs_n.items()))}')
    print(f'distinct Dx11 programs {len(codes)}; windParams register: {dict(sorted(regs.items()))}')
    print(f'Dx11 programs with the sway sin(((x + y + z) x 1000 + w.x) x w.y) x max(y, 0) x w.z added to x and z: '
          f'{shape[True]} of {len(codes)}')


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    bundles = stock_bundles(game)
    print(f'{len(bundles)} DICE level bundles')
    terrain_resources(bundles)
    partitions(bundles)
    tails_and_material_maps(bundles)
    undergrowth_shaders(bundles)


if __name__ == '__main__':
    main()
