# mypy: disable-error-code="attr-defined"
# (round RT2: the retired references are named here; at run time tests/replay.py stands in for them)
r"""Round F2a: the rest of the console converter in the Rust core against its Python reference - terrain.py
(heightfield, water_mesh, material_map, visual_terrain, visual_water and the PC readers visual_terrain_parts,
road_strips, visual_water_parts, material_names), soot.py (soot_mesh), havok.convert_wrapper, mesh.py
(console_layouts, decode, relayout with and without an edit) and texture.check.

Compared both ways: the bytes, dicts, arrays (dtype, shape, every value bit for bit) and floats, or the same error
(type and message).

  python test_console_f2a_native.py          quick: inputs built here of every kind (both byte orders, both sources),
                                             their damaged copies; 40 of Battlefield 1943's terrain, soot, physics,
                                             mesh and shader database resources when found
  python test_console_f2a_native.py --game   + every such resource of Battlefield 1943, of BFBC2's unpacked Xbox
                                             bundles (OPFOR work\xbox) and of the game copy (the PC readers, decode,
                                             relayout, texture.check), 2 damaged copies of each; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.

The terrain, soot, havok.convert_wrapper, mesh.console_layouts / decode / relayout and texture.check references were
retired in round RT2: their answers were recorded first (--record, and --game --record, while they were there:
f2a_console_replay*.json beside this test) and are replayed by tests/replay.py where they are gone.
"""
import contextlib
import os
import random
import struct
import sys
import time
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core', 'model-tools')
import havok  # noqa: E402
import mesh  # noqa: E402
import soot  # noqa: E402
import terrain  # noqa: E402
import texture  # noqa: E402
import texwrite  # noqa: E402
from bfbc2core import native, paths  # noqa: E402
from f0_parity import Checks, outcome, stdout_utf8  # noqa: E402
from f0_parity import canon as canon0  # noqa: E402
from f2a_parity import Tally, bf1943_entries, both, bundle_entries, damaged, watch, watched  # noqa: E402
from replay import Replay  # noqa: E402

N = native.require('terrain.py / soot.py / mesh.py / havok.py / texture.py (round F2a)', 'terrain_convert')
REPLAY: list[Any] = [None]
TERRAIN = {'TerrainHeightfield', 'WaterMesh', 'TerrainMaterialMap', 'VisualTerrain', 'VisualWater'}
TERRAIN_REFS = ('heightfield_reference', 'water_mesh_reference', 'material_map_reference', 'visual_terrain_reference',
                'visual_water_reference', 'road_strips_reference', 'visual_terrain_parts_reference',
                'visual_water_parts_reference', 'material_names_reference')
MESH_REFS = ('console_layouts_reference', 'decode_reference', 'relayout_reference')


def arrays(v: Any) -> Any:
    """A result with NumPy arrays as (dtype, shape, bytes) for comparing."""
    if isinstance(v, np.ndarray):
        return ('ndarray', v.dtype.str, v.shape, v.tobytes())
    if isinstance(v, dict):
        return {k: arrays(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return type(v)(arrays(x) for x in v)
    return v


# ------------------------------------------------------------------------------------------------ built inputs

def f32s(rng: random.Random, n: int, e: str) -> bytes:
    return struct.pack(f'{e}{n}f', *(rng.uniform(-1e4, 1e4) for _ in range(n)))


def cstr(rng: random.Random) -> bytes:
    return bytes(rng.randrange(1, 256) for _ in range(rng.randrange(8))) + b'\0'


def heightfield_x(rng: random.Random, n: int, bf1943: bool) -> bytes:
    head = struct.pack('>6f3I2HIB', *(rng.uniform(-1e3, 1e3) for _ in range(6)), n,
                       n.bit_length() - 4, 0 if n in terrain.SUB_SIZES else n.bit_length() - 3, 0, n // 256, 0, 0)
    if n in terrain.SUB_SIZES:
        tail = rng.randbytes(4 * terrain.quadtree_nodes(n))
        body = rng.randbytes(2 * n * n)
        return (head[:0x2D] if bf1943 else head + b'\0\0\0\0') + body + tail
    a = 2 * rng.randrange(0, 200)
    region = a + terrain.FIXED[n]
    h = head + (b'' if bf1943 else struct.pack('>I', a))
    return h + rng.randbytes(2 * n * n + 4 * n + region)


def material_map_x(rng: random.Random) -> bytes:
    b = rng.randrange(4)
    out = struct.pack('>IH', 2048, b)
    for _ in range(b):
        L = rng.randrange(20)
        out += struct.pack('>I', L) + rng.randbytes(L) + rng.randbytes(512)
    names = b'\0'.join(cstr(rng)[:-1] for _ in range(rng.randrange(1, 5))) + b'\0'
    return out + struct.pack('>I', len(names)) + names


def visual_terrain_x(rng: random.Random, e: str = '>') -> bytes:
    n = rng.randrange(1, 3)
    out = struct.pack(e + 'I', n)
    for _ in range(2 * n):
        k = rng.randrange(3)
        out += struct.pack(e + 'I', k)
        for _ in range(k):
            out += cstr(rng) + cstr(rng) + cstr(rng) + struct.pack(e + '2I', rng.randrange(99), rng.randrange(99))
            out += f32s(rng, 6, e) + struct.pack(e + 'I', rng.randrange(8))
        v, i = rng.randrange(4), rng.randrange(6)
        out += struct.pack(e + '3I', v, 28 * v, i) + rng.randbytes(28 * v) + rng.randbytes(4 * i)
    m = rng.randrange(3)
    out += struct.pack(e + 'I', m)
    for _ in range(m):
        out += cstr(rng) + cstr(rng) + struct.pack(e + '2I', rng.randrange(7), 0)
        g = rng.randrange(3)
        out += struct.pack(e + 'I', g)
        for _ in range(g):
            k = rng.randrange(3)
            out += struct.pack(e + 'I', k)
            for _ in range(k):
                out += cstr(rng) + b'\0' + cstr(rng) + f32s(rng, 12, e) + b'\1\0'
    out += struct.pack(e + 'I', n)
    for _ in range(n):
        w = rng.randrange(1, 4)
        out += f32s(rng, 6, e) + struct.pack(e + '5I', 66, 1, 15, 7, w) + rng.randbytes(8 * w * w + 4 * w * w)
        out += struct.pack(e + 'I', w) + rng.randbytes(w * w) + cstr(rng) + cstr(rng)
    return out


def visual_water_x(rng: random.Random) -> bytes:
    v = rng.randrange(6)
    out = struct.pack('>3I', 10, 28, 28 * v)
    for _ in range(v):
        vec = [rng.choice([0, rng.randrange(1 << 32), 0x40000000]) for _ in range(2)]
        out += rng.randbytes(8) + struct.pack('>2I', *vec) + rng.randbytes(12)
    i = rng.randrange(5)
    out += struct.pack('>I', 4 * i) + rng.randbytes(4 * i)
    m = rng.randrange(3)
    out += struct.pack('>I', m) + b''.join(cstr(rng) + cstr(rng) for _ in range(m))
    r = rng.randrange(3)
    return out + struct.pack('>I', r) + rng.randbytes(16 * r) + struct.pack('>f', 8192.0)


def soot_x(rng: random.Random) -> bytes:
    k = rng.randrange(4)
    if k == 0:
        return struct.pack('>4I', 18, 0, 0, 0)
    n = rng.randrange(3)
    stride = 12 if k < 3 else 16
    v, i = rng.randrange(4), rng.randrange(6)
    return (struct.pack('>5I', 18, stride, 0, 0, n) + f32s(rng, 6 * n, '>') + struct.pack('>4I', 1, stride * v, 2 * i, 0)
            + rng.randbytes(stride * v + 2 * i + 256))


def wrapper_x(rng: random.Random) -> tuple[bytes, int]:
    k = rng.randrange(4)
    a, c = rng.randrange(3), rng.randrange(3)
    out = struct.pack('>2I', rng.randrange(9), k) + rng.randbytes(4 * k)
    out += struct.pack('>H', a) + f32s(rng, 3 * a, '>') + struct.pack('>H', c) + f32s(rng, 6 * c, '>')
    out += bytes(rng.randrange(2, 256) for _ in range(rng.randrange(12)))
    j = rng.randrange(4)
    out += struct.pack('>2I', 1, j) + rng.randbytes(4 * j)
    return out + havok.MAGIC + rng.randbytes(64), len(out)


def layout(els: list[tuple[int, int, int, int]], stride: int) -> bytes:
    rec = bytearray(0x49)
    for i, e in enumerate(els):
        rec[4 * i:4 * i + 4] = bytes(e)
    rec[64], rec[69] = len(els), stride
    return bytes(rec)


LAYOUTS = [layout([(3, 1, 0, 0), (6, 0, 12, 0), (8, 4, 16, 0)], 24),
           layout([(8, 1, 0, 0), (6, 0, 8, 0), (8, 4, 12, 0)], 20),
           layout([(3, 1, 0, 0), (2, 0, 12, 0), (0x0C, 2, 20, 0), (0x0C, 3, 24, 0)], 28),
           layout([(3, 1, 0, 0), (6, 0, 12, 0), (0x0C, 2, 16, 0), (0x0C, 3, 20, 0), (8, 4, 24, 0)], 32),
           layout([(8, 1, 0, 0), (8, 0, 8, 0), (6, 0, 16, 0)], 20),
           layout([(0x2B, 4, 0, 0), (0x0D, 3, 4, 0), (3, 1, 8, 0)], 20)]


def mesh_pc(rng: random.Random, lays: list[bytes], big: bool = False) -> bytes:
    """A MeshData of parts in the given layouts, parts sharing a window now and then."""
    parts: list[dict[str, Any]] = []
    idx: list[int] = []
    verts = b''
    for k, lay in enumerate(lays):
        stride = lay[69]
        if parts and rng.random() < 0.3 and lays[k - 1] == lay:
            p = dict(parts[-1])
            p.update(name=b'part%d' % k, first_index=rng.randrange(max(len(idx) - 2, 1)), triangles=1)
            parts.append(p)
            continue
        nv = rng.randrange(1, 9)
        tri = rng.randrange(1, 5)
        first = len(idx)
        idx += [rng.randrange(nv) for _ in range(3 * tri)]
        parts.append({'name': b'part%d' % k, 'triangles': tri, 'vertices': nv, 'first_index': first,
                      'vertex_offset': len(verts), 'stride': stride, 'k': 3, 'fa': 0, 'fb': 2, 'bones': [3, 7]})
        verts += rng.randbytes(nv * stride)
    m = {'h0': 1, 'h1': b'\0\0', 'parts': parts, 'lists': [[0], [], [], []]}
    if big:
        sys.path.insert(0, HERE)
        from test_meshdata_native import to_big
        return to_big(m, verts, struct.pack('>%dH' % len(idx), *idx))
    return mesh.write(m, verts, struct.pack('<%dH' % len(idx), *idx))


def shader_db(rng: random.Random) -> tuple[bytes, list[str]]:
    out, names = bytearray(rng.randbytes(rng.randrange(40))), []
    for i in range(rng.randrange(1, 6)):
        n = f'objects/x/m{i}_lod{rng.randrange(3)}/Part{i}/standard'
        names.append(n)
        guid = rng.randbytes(16) if rng.random() < 0.5 else bytes(rng.randrange(0x41, 0x5A) for _ in range(16))
        out += n.encode() + b'\0' + guid
        if rng.random() < 0.6:
            out += rng.randbytes(rng.randrange(30))
        out += guid + rng.choice(LAYOUTS)
    return bytes(out), names


def textures(rng: random.Random) -> list[bytes]:
    out = []
    for kind in ('color', 'normal', 'dxt5'):
        w, h = rng.choice([(8, 8), (16, 8), (32, 32), (4, 64)])
        img = np.random.default_rng(rng.randrange(1 << 30)).integers(0, 256, (h, w, 4 if kind == 'dxt5' else 3)).astype(np.uint8)
        out.append(texwrite.texture(img, kind))
    l8 = struct.pack('<8I', 3, 0, 10, 0, 16, 16, 0, 2) + struct.pack('<15I', 256, 64, *[0] * 13) + rng.randbytes(320)
    out.append(l8)
    out.append(struct.pack('<8I', 3, 0, 13, 0, 4, 4, 0, 1) + bytes(60) + rng.randbytes(128))
    return out


def check_texture(t: Tally, label: str, pc: bytes, ref: bytes | None) -> None:
    both(t, label, texture.check, texture.check_reference, pc, ref)


def terrain_all(t: Tally, label: str, kind: str, x: bytes, bf1943: bool) -> None:
    """Every conversion of a console terrain resource of its kind, then the PC readers on the result."""
    if kind == 'TerrainHeightfield':
        src = 'bf1943' if bf1943 else 'bfbc2'
        both(t, label, terrain.heightfield, terrain.heightfield_reference, x, src)
        both(t, label + ' (other source)', terrain.heightfield, terrain.heightfield_reference, x,
             'bfbc2' if bf1943 else 'bf1943')
        return
    if kind == 'WaterMesh':
        both(t, label, terrain.water_mesh, terrain.water_mesh_reference, x)
        return
    if kind == 'TerrainMaterialMap':
        pc = both(t, label, terrain.material_map, terrain.material_map_reference, x)
        both(t, label + ' names', terrain.material_names, terrain.material_names_reference, pc if pc is not None else x)
        return
    if kind == 'VisualTerrain':
        pc = both(t, label, terrain.visual_terrain, terrain.visual_terrain_reference, x)
        pc = pc if pc is not None else x
        both(t, label + ' parts', terrain.visual_terrain_parts, terrain.visual_terrain_parts_reference, pc)
        both(t, label + ' roads', terrain.road_strips, terrain.road_strips_reference, pc, canon=arrays)
        return
    if kind == 'VisualWater':
        pc = both(t, label, terrain.visual_water, terrain.visual_water_reference, x)
        both(t, label + ' parts', terrain.visual_water_parts, terrain.visual_water_parts_reference,
             pc if pc is not None else x, canon=arrays)


def pc_terrain(t: Tally, label: str, kind: str, pc: bytes) -> None:
    if kind == 'TerrainMaterialMap':
        both(t, label, terrain.material_names, terrain.material_names_reference, pc)
    elif kind == 'VisualTerrain':
        both(t, label, terrain.visual_terrain_parts, terrain.visual_terrain_parts_reference, pc)
        both(t, label + ' roads', terrain.road_strips, terrain.road_strips_reference, pc, canon=arrays)
    elif kind == 'VisualWater':
        both(t, label, terrain.visual_water_parts, terrain.visual_water_parts_reference, pc, canon=arrays)


def wrapper_both(t: Tally, label: str, b: bytes) -> None:
    try:
        o = havok.wrapper_end(b)
    except havok.ConvertError:
        return
    both(t, label, havok.convert_wrapper, havok.convert_wrapper_reference, b, o)


def decode_all(t: Tally, label: str, b: bytes, lays: list[bytes | None], big: bool) -> None:
    for i, lay in enumerate(lays):
        if lay is not None:
            both(t, f'{label} part {i}', mesh.decode, mesh.decode_reference, b, lay, i, big, canon=arrays)


def editor(rng_seed: int) -> Any:
    def edit(parts: list[dict[str, Any]], cols: dict[int, np.ndarray]) -> None:
        r = random.Random(rng_seed + len(parts))
        for _k, c in cols.items():
            if c.dtype == np.float32 and r.random() < 0.5:
                c[:] = c * np.float32(1.5) + np.float32(r.random())
        parts[0]['bones'] = list(parts[0]['bones']) + [r.randrange(200)]
    return edit


def relayout_all(t: Tally, label: str, b: bytes, old: list[bytes], new: list[bytes], seed: int) -> None:
    both(t, label, mesh.relayout, mesh.relayout_reference, b, old, new)
    # edited (a callback): recorded under its own name with the seed (round RT2)
    a, native = watched(lambda: outcome(lambda: arrays(mesh.relayout(b, old, new, editor(seed)))))
    live = REPLAY[0].live(mesh, 'relayout_reference')
    ok = REPLAY[0].pair('mesh.relayout_reference edited', (b, old, new, seed), a,
                        (lambda: outcome(lambda: arrays(mesh.relayout_reference(b, old, new, editor(seed)))))
                        if live else None, refused=not native)
    t.add(ok, label + ' edited' if ok else f'{label} edited: {repr(a)[:100]}', a[0] == 'error', native)


def quick(c: Checks, rng: random.Random) -> None:
    t, td = Tally(), Tally()
    for i in range(30):
        for n, bf in ((1024, False), (1024, True), (128, False), (128, True), (64, True)):
            x = heightfield_x(rng, n, bf)
            terrain_all(t, f'heightfield {n} {bf} {i}', 'TerrainHeightfield', x, bf)
            terrain_all(td, f'damaged heightfield {i}', 'TerrainHeightfield', damaged(x, rng), bf)
    for i in range(300):
        for kind, x in (('WaterMesh', rng.randbytes(rng.randrange(40))), ('TerrainMaterialMap', material_map_x(rng)),
                        ('VisualTerrain', visual_terrain_x(rng)), ('VisualWater', visual_water_x(rng))):
            terrain_all(t, f'{kind} {i}', kind, x, False)
            terrain_all(td, f'damaged {kind} {i}', kind, damaged(x, rng), False)
        pc_terrain(t, f'PC VisualTerrain {i}', 'VisualTerrain', visual_terrain_x(rng, '<'))
        s = soot_x(rng)
        both(t, f'soot {i}', soot.soot_mesh, soot.soot_mesh_reference, s)
        both(td, f'damaged soot {i}', soot.soot_mesh, soot.soot_mesh_reference, damaged(s, rng))
        w, o = wrapper_x(rng)
        both(t, f'wrapper {i}', havok.convert_wrapper, havok.convert_wrapper_reference, w, o)
        wrapper_both(td, f'damaged wrapper {i}', damaged(w, rng))
    c.check(not t.ndiff, f'built terrain, soot and wrappers both ways: {t.text()}', '; '.join(t.differ))
    c.check(not td.ndiff, f'damaged terrain, soot and wrappers: {td.text()}', '; '.join(td.differ))

    tm, tmd = Tally(), Tally()
    for i in range(200):
        db, _ = shader_db(rng)
        both(tm, f'console_layouts {i}', mesh.console_layouts, mesh.console_layouts_reference, db)
        both(tmd, f'damaged console_layouts {i}', mesh.console_layouts, mesh.console_layouts_reference, damaged(db, rng))
        lays = [rng.choice(LAYOUTS[:5]) for _ in range(rng.randrange(1, 4))]
        b = mesh_pc(rng, lays)
        decode_all(tm, f'mesh {i}', b, list(lays), False)
        both(tm, f'mesh {i} by name', mesh.decode, mesh.decode_reference, b, lays[0], 'part0', False, canon=arrays)
        bb = mesh_pc(rng, [LAYOUTS[5]], big=True)
        decode_all(tm, f'Xbox mesh {i}', bb, [LAYOUTS[5]], True)
        new = [rng.choice(LAYOUTS[:5]) for _ in lays]
        relayout_all(tm, f'relayout {i}', b, list(lays), new, i)
        relayout_all(tmd, f'damaged relayout {i}', damaged(b, rng), list(lays), new, i)
        decode_all(tmd, f'damaged mesh {i}', damaged(b, rng), list(lays), False)
        decode_all(tmd, f'damaged layouts {i}', b, [damaged(x, rng) for x in lays], False)
    c.check(not tm.ndiff, f'built shader databases and meshes (console_layouts, decode, relayout, edited): {tm.text()}',
            '; '.join(tm.differ))
    c.check(not tmd.ndiff, f'damaged databases, meshes and layouts: {tmd.text()}', '; '.join(tmd.differ))

    tt = Tally()
    for i in range(60):
        for pc in textures(rng):
            check_texture(tt, f'texture {i}', pc, None)
            check_texture(tt, f'texture {i} vs itself', pc, pc)
            check_texture(tt, f'texture {i} vs damaged', pc, damaged(pc, rng))
            check_texture(tt, f'damaged texture {i}', damaged(pc, rng), pc)
    c.check(not tt.ndiff, f'texture.check on built textures, with references and damaged: {tt.text()}', '; '.join(tt.differ))


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    watch(terrain, *TERRAIN_REFS)
    watch(soot, 'soot_mesh_reference')
    watch(havok, 'convert_wrapper_reference')
    watch(mesh, *MESH_REFS)
    watch(texture, 'check_reference')
    # round RT2: the references' answers recorded (--record) and replayed once they are retired (after watch: the
    # port's own calls of a reference reach the recorder first, which marks them as refused inputs)
    REPLAY[0] = Replay('f2a_console', HERE, [(terrain, n) for n in TERRAIN_REFS] + [(mesh, n) for n in MESH_REFS] + [
        (soot, 'soot_mesh_reference'), (havok, 'convert_wrapper_reference'), (texture, 'check_reference')],
        canon=lambda v: canon0(arrays(v)))
    rng = random.Random(0xF2A)
    quick(c, rng)

    limit = None if game else 40
    opfor = paths.sibling(paths.OPFOR_DIR, suite=SUITE)
    xbox = os.path.join(opfor, 'work', 'xbox', '**', '*.fbrb')
    pcg = os.path.join(opfor, 'game', '**', '*.fbrb')
    tg, tgd = Tally(), Tally()
    tn = tr = 0.0
    kinds: dict[str, int] = {}
    sources = [('bf1943', bf1943_entries(TERRAIN | {'SootMesh', 'HavokPhysicsData'}, limit))]
    if game:
        sources.append(('xbox', bundle_entries(xbox, TERRAIN | {'SootMesh', 'HavokPhysicsData'})))
    for origin, items in sources:
        for nm, kind, x in items:
            kinds[kind] = kinds.get(kind, 0) + 1
            label = f'{origin} {nm}'
            frng = random.Random(len(x))
            for use, cost in ((True, 'n'),):                 # the Python side was retired in round RT2
                t0 = time.perf_counter()
                # the Python side timed on purpose: not a refused input (round RT2's recording)
                with contextlib.suppress(Exception), (REPLAY[0].python() if not use else contextlib.nullcontext()):
                    if kind == 'SootMesh':
                        soot.soot_mesh(x)
                    elif kind == 'HavokPhysicsData':
                        havok.convert_wrapper(x, havok.wrapper_end(x))
                    elif kind == 'TerrainHeightfield':
                        terrain.heightfield(x, origin if origin == 'bf1943' else 'bfbc2')
                    else:
                        {'WaterMesh': terrain.water_mesh, 'TerrainMaterialMap': terrain.material_map,
                         'VisualTerrain': terrain.visual_terrain, 'VisualWater': terrain.visual_water}[kind](x)
                if cost == 'n':
                    tn += time.perf_counter() - t0
                else:
                    tr += time.perf_counter() - t0
            if kind == 'SootMesh':
                both(tg, label, soot.soot_mesh, soot.soot_mesh_reference, x)
                both(tgd, label, soot.soot_mesh, soot.soot_mesh_reference, damaged(x, frng))
            elif kind == 'HavokPhysicsData':
                wrapper_both(tg, label, x)
                wrapper_both(tgd, label, damaged(x, frng))
            else:
                terrain_all(tg, label, kind, x, origin == 'bf1943')
                for _ in range(2 if game else 1):
                    terrain_all(tgd, label + ' damaged', kind, damaged(x, frng), origin == 'bf1943')
    if game:
        for nm, kind, pc in bundle_entries(pcg, {'TerrainMaterialMap', 'VisualTerrain', 'VisualWater'}):
            kinds['PC ' + kind] = kinds.get('PC ' + kind, 0) + 1
            pc_terrain(tg, 'game ' + nm, kind, pc)
    what = ', '.join(f'{v:,} {k}' for k, v in sorted(kinds.items()))
    c.check(not tg.ndiff, f'game terrain, soot and physics resources ({what}) both ways: {tg.text()}', '; '.join(tg.differ))
    c.check(not tgd.ndiff, f'damaged game resources: {tgd.text()}', '; '.join(tgd.differ))
    print(f'timing (conversions): Python {tr:.2f} s, Rust {tn:.2f} s')
    meshes(c, game, limit, xbox, pcg)
    if game:
        textures_game(c, opfor)
    c.check(REPLAY[0].finish(), 'the references: recorded, or replayed as recorded')
    return c.done()


def meshes(c: Checks, game: bool, limit: int | None, xbox: str, pcg: str) -> None:
    """console_layouts on the console shader databases, decode on every console part with a layout, decode and
    relayout (into its own layouts and edited) on the game copy's meshes."""
    tm = Tally()
    t_cl = [0.0, 0.0]
    layouts: dict[str, bytes] = {}
    dbs = list(bf1943_entries({'Dx9ShaderDatabase'}, None if game else 4))
    for nm, _, db in dbs:
        for k, (_use, fn) in enumerate(((True, mesh.console_layouts), (False, mesh.console_layouts_reference))[
                :1 + REPLAY[0].live(mesh, 'console_layouts_reference')]):
            t0 = time.perf_counter()
            fn(db)
            t_cl[k] += time.perf_counter() - t0
        got = both(tm, nm, mesh.console_layouts, mesh.console_layouts_reference, db)
        for n, r in (got or {}).items():
            layouts.setdefault(n, r)
    parts = 0
    t_dec = [0.0, 0.0]
    for nm, _, b in bf1943_entries({'MeshData'}, None if game else limit):
        stem = nm[:-len('_data.res')] if nm.endswith('_data.res') else nm
        try:
            m = mesh.parse(b, True)
        except Exception:                                        # noqa: BLE001
            continue
        for i, p in enumerate(m['parts']):
            lay = layouts.get(f'{stem}/{p["name"].decode("latin-1")}/standard'.lower())
            if lay is None:
                continue
            parts += 1
            for k, dec in enumerate((mesh.decode, mesh.decode_reference)[:1 + REPLAY[0].live(mesh, 'decode_reference')]):
                t0 = time.perf_counter()
                with contextlib.suppress(Exception):
                    dec(b, lay, i, True)
                t_dec[k] += time.perf_counter() - t0
            both(tm, f'{nm} part {i}', mesh.decode, mesh.decode_reference, b, lay, i, True, canon=arrays)
    c.check(not tm.ndiff, f'console shader databases ({len(dbs)}) and Battlefield 1943 mesh parts ({parts:,}) decoded '
            f'both ways: {tm.text()}', '; '.join(tm.differ))
    print(f'timing: console_layouts Python {t_cl[1]:.2f} s, Rust {t_cl[0]:.2f} s; console decode Python {t_dec[1]:.2f} s, '
          f'Rust {t_dec[0]:.2f} s')
    if not game:
        return
    suite_paths.use('mesh-viewer')
    import meshview
    import shaderdb
    ix = meshview.Index(shaderdb.find_opfor(None))
    tp = Tally()
    t_rel = [0.0, 0.0]
    count = relaid = 0
    for nm, _, b in bundle_entries(pcg, {'MeshData'}):
        stem = nm[:-len('_data.res')] if nm.endswith('_data.res') else nm
        try:
            m = mesh.parse(b, False)
        except Exception:                                        # noqa: BLE001
            continue
        lays = [ix.parts.get(f'{stem}/{p["name"].decode("latin-1")}/standard'.lower()) for p in m['parts']]
        count += 1
        decode_all(tp, nm, b, lays, False)
        if all(x is not None for x in lays):
            relaid += 1
            for k, rel in enumerate((mesh.relayout, mesh.relayout_reference)[
                    :1 + REPLAY[0].live(mesh, 'relayout_reference')]):
                t0 = time.perf_counter()
                with contextlib.suppress(Exception):
                    rel(b, lays, lays)
                t_rel[k] += time.perf_counter() - t0
            relayout_all(tp, nm, b, lays, lays, count)
    c.check(not tp.ndiff, f'game meshes: {count:,} decoded, {relaid:,} relaid (plain and edited) both ways: {tp.text()}',
            '; '.join(tp.differ))
    print(f'timing (relayout): Python {t_rel[1]:.2f} s, Rust {t_rel[0]:.2f} s')


def textures_game(c: Checks, opfor: str) -> None:
    """texture.check on every PC texture of the game copy (alone and against itself), timed both ways."""
    tt = Tally()
    t = [0.0, 0.0]
    n = 0
    for nm, _, pc in bundle_entries(os.path.join(opfor, 'game', '**', '*.fbrb'), {'ITexture'}):
        n += 1
        for k, fn in enumerate((texture.check, texture.check_reference)[:1 + REPLAY[0].live(texture, 'check_reference')]):
            t0 = time.perf_counter()
            with contextlib.suppress(Exception):
                fn(pc, pc)
            t[k] += time.perf_counter() - t0
        check_texture(tt, nm, pc, None)
        check_texture(tt, nm + ' vs itself', pc, pc)
    c.check(not tt.ndiff, f'texture.check on {n:,} game textures both ways: {tt.text()}', '; '.join(tt.differ))
    print(f'timing (texture.check with a reference): Python {t[1]:.2f} s, Rust {t[0]:.2f} s')


if __name__ == '__main__':
    sys.exit(main(sys.argv))
