r"""Unit tests of the converters' Stage 3 fixes, without game data (quick tier):

- havokport: a model group's shape subparts get translation.w 1.0 and the shape's aabbCenter.w / aabbHalfExtents.w
  as DICE's exporter writes them (values from two of DICE's own levels: mp_006's rigid group, sp_008's static grid);
- mopp: a hand-made byte code (bounds, a split, a one-value split, a 16-bit split, a rescale, terminal offsets, a
  jump) answers box queries cell by cell; the ray cast on it equals brute force; an unknown opcode is refused;
- texture: console format 10 (L8) untiled byte for byte, not byte-swapped, each PC row padded to 4 bytes, the
  console's flags kept; console 3 (DXT5A) -> PC 10 keeps the console's flags (bit 1 clear) and pads its rows too;
- meshport rename: a prefix map renames any case, the longest prefix first, keeps other names; a mesh set's LOD
  settings, shader set and shader names renamed and listed, its bytes otherwise the same;
- mesh.texcoord_element: the texture coordinates are the first half2 / float2 usage-0 element (soldiers, trees),
  else the first other float one (material-mask meshes), never a colour byte element; decode reads that one;

  python tests\test_converter_units.py
"""
import os
import struct
import sys

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'bf1943-import', 'console-convert')
import havokport  # noqa: E402
import mesh  # noqa: E402
import meshport  # noqa: E402
import meshset  # noqa: E402
import mopp  # noqa: E402
import texture  # noqa: E402

FAILED = [0]


def check(ok: bool, what: str, detail: str = '') -> None:
    FAILED[0] += not ok
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))


def model_group_packfile(q, centre, half, child='hkpConvexVerticesShape'):
    """A minimal PC packfile: one hkpExtendedMeshShape of two shape subparts (one placing a convex child with the
    given local box, then Havok's empty last subpart) and its fixup tables; every w lane 0, as 4.6.1 leaves them.
    Returns (bytes, the data section's start)."""
    names = ['hkpExtendedMeshShape', child]
    cn, at = bytearray(), {}
    for n in names:
        at[n] = len(cn) + 5
        cn += struct.pack('<I', 0) + bytes([9]) + n.encode() + bytes(1)
    cn += bytes([255]) * 4
    while len(cn) % 16:
        cn += bytes([255])
    data = bytearray(432)
    struct.pack_into('<I', data, 92, 2)                                   # shapesSubparts count
    struct.pack_into('<I', data, 192 + 20, 1)                             # subpart 0: one child
    struct.pack_into('<4f', data, 192 + 32, *q)
    struct.pack_into('<4f', data, 192 + 48, 10.0, 20.0, 30.0, 0.0)
    struct.pack_into('<4f', data, 256 + 32, 0.0, 0.0, 0.0, 1.0)           # the empty subpart
    struct.pack_into('<3f', data, 336 + 32, *half)
    struct.pack_into('<3f', data, 336 + 48, *centre)
    pad = bytes([255])
    local = struct.pack('<4I', 88, 192, 192 + 16, 320).ljust(16, pad)
    glob = struct.pack('<3I', 320, 1, 336).ljust(16, pad)
    virt = struct.pack('<6I', 0, 0, at[names[0]], 336, 0, at[names[1]]).ljust(32, pad)
    head = bytearray(64)
    head[0:len(havokport.MAGIC)] = havokport.MAGIC
    struct.pack_into('<I', head, 20, 2)
    cstart = 64 + 96
    dstart = cstart + len(cn)
    loc = len(data)
    sec = bytearray()
    for tag, rel in ((b'__classnames__', (cstart,) + (len(cn),) * 6),
                     (b'__data__', (dstart, loc, loc + 16, loc + 32, loc + 64, loc + 64, loc + 64))):
        sec += tag.ljust(20, bytes(1)) + struct.pack('<7I', *rel)
    return bytes(head + sec + cn + data + local + glob + virt), dstart


def havok_model_group() -> None:
    import numpy as np
    # DICE's mp_006 rigid group: one box child, a -90 degree turn about y -> (0.52728, 0.50294)
    s = 0.70710677
    w = havokport.aabb_w([((0.0, -s, 0.0, s), [((0.003, 0.473, 0.0), (0.03, 0.503, 1.925), 0.0)])] * 2)
    check(w is not None and abs(w[0] - 0.52728) < 2e-3 and abs(w[1] - 0.50294) < 2e-3,
          'havokport aabb_w: mp_006\'s rigid group (centre.w 0.52728, half.w 0.50294)', str(w))
    # sp_008's static grid: two children under one turn about y -> min 0.93913, max 1.39051
    q = (0.0, 0.1685, 0.0, 0.9857)
    w = havokport.aabb_w([(q, [((-0.005, 0.571, 0.0), (1.666, 0.605, 0.453), 0.0),
                               ((-0.005, 0.105, 0.0), (2.5, 0.288, 1.188), 0.0)])])
    check(w is not None and abs(w[0] - w[1] - 0.93913) < 2e-3 and abs(w[0] + w[1] - 1.39051) < 2e-3,
          'havokport aabb_w: sp_008\'s static grid (min 0.93913, max 1.39051)', str(w))
    check(havokport.aabb_w([((0, 0, 0, 1), [])]) is None, 'havokport aabb_w: no children, no value')
    lanes = havokport.subpart_w_lanes((0.0, 1.0, 0.0, 0.0))
    check(bool(np.all(lanes == 0)), 'havokport: a half turn about y has zero w lanes (oasis: centre.w 1, half.w 0)')
    # the byte path: shape_subparts_to_550 on a packfile with 4.6.1's zero w lanes
    pf, d = model_group_packfile((0.0, -s, 0.0, s), (0.003, 0.473, 0.0), (0.03, 0.503, 1.925))
    out, swapped = havokport.shape_subparts_to_550(pf)
    trw = struct.unpack_from('<f', out, d + 192 + 60)[0], struct.unpack_from('<f', out, d + 256 + 60)[0]
    cw, hw = struct.unpack_from('<f', out, d + 64 + 12)[0], struct.unpack_from('<f', out, d + 48 + 12)[0]
    check(swapped == 0 and trw == (1.0, 0.0), 'havokport: translation.w 1.0 on a subpart with children, 0 on the '
          'empty one (as on all of DICE\'s)', f'{swapped} {trw}')
    check(abs(cw - 0.52728) < 2e-3 and abs(hw - 0.50294) < 2e-3, 'havokport: the shape\'s aabbCenter.w / '
          'aabbHalfExtents.w written', f'{cw} {hw}')
    pf2, d2 = model_group_packfile((0.0, -s, 0.0, s), (0, 0, 0), (1, 1, 1), child='hkpCylinderShape')
    out2, _ = havokport.shape_subparts_to_550(pf2)
    check(struct.unpack_from('<f', out2, d2 + 48 + 12)[0] == 0.0 and struct.unpack_from('<f', out2, d2 + 192 + 60)[0]
          == 1.0, 'havokport: a child without a known local box leaves the shape\'s w lanes alone')


def texcoords() -> None:
    import numpy as np
    soldier = [(7, 1, 0, 0), (5, 7, 6, 0), (12, 2, 8, 0), (12, 3, 12, 0), (8, 0, 16, 0), (8, 4, 24, 0), (8, 5, 32, 0),
               (6, 0, 40, 0), (6, 0, 44, 0), (6, 0, 48, 0)]
    tree = [(7, 1, 0, 0), (5, 7, 6, 0), (8, 0, 8, 0), (8, 4, 16, 0), (8, 5, 24, 0), (6, 0, 32, 0), (4, 0, 36, 0)]
    matmask = [(7, 1, 0, 0), (5, 7, 6, 0), (12, 2, 8, 0), (12, 0, 12, 0), (12, 3, 16, 0), (12, 0, 20, 0),
               (8, 4, 24, 0), (8, 0, 32, 0), (8, 0, 40, 0)]
    plain = [(3, 1, 0, 0), (6, 0, 12, 0), (8, 0, 16, 0)]
    got = [mesh.texcoord_element(e) for e in (soldier, tree, matmask, plain, [(3, 1, 0, 0)])]
    check(got == [7, 5, 7, 1, None], 'mesh.texcoord_element: soldier half2 at 40, tree half2 at 32, material mask '
          'half4 at 32, plain first half2, none', str(got))
    # decode on a little-endian part: position float3, a grey half4 (usage 0), the half2 coordinates
    layout = bytearray(0x49)
    els = [(3, 1, 0, 0), (8, 0, 12, 0), (6, 0, 20, 0)]
    for i, e in enumerate(els):
        layout[4 * i:4 * i + 4] = bytes(e)
    layout[64], layout[69] = len(els), 24
    verts = b''.join(struct.pack('<3f', i, 0, 0) + np.array([0.5, 0.5, 0.5, 1], '<f2').tobytes()
                     + np.array([i / 4, 1 - i / 4], '<f2').tobytes() for i in range(3))
    name = b'p'
    part = name + bytes(1) + struct.pack('<4I', 1, 3, 0, 0) + bytes([24, 3, 0, 0]) + struct.pack('<H', 0)
    head = struct.pack('<I', 0) + bytes(2) + bytes([1])
    lists = struct.pack('<I', 0) * 4
    idx = struct.pack('<3H', 0, 1, 2)
    b = head + part + lists + struct.pack('<3I', len(verts), len(idx), 0) + verts + idx
    try:
        d = mesh.decode(b, bytes(layout), 0, big_endian=False)
        ok = np.allclose(d['uvs'], np.array([[0, 1], [0.25, 0.75], [0.5, 0.5]]))
        detail = str(d['uvs'].tolist())
    except Exception as ex:                                       # noqa: BLE001
        ok, detail = False, repr(ex)
    check(ok, 'mesh.decode: uvs from the half2, not the half4 before it', detail)


def console_texture(fmt, w, h, mips, flags, texel):
    """A console texture of fmt (L8 or DXT5A) whose level m's texel / block (x, y) holds texel(m, x, y) (bytes)."""
    bpb, edge, _ = texture.FORMATS[fmt]
    where, size = texture.layout(w, h, mips, fmt, 1)
    data = bytearray(size)
    for m in range(mips):
        start, pitch, ox, oy = where[0][m]
        bw, bh = max((max(w >> m, 1) + edge - 1) // edge, 1), max((max(h >> m, 1) + edge - 1) // edge, 1)
        for y in range(bh):
            for x in range(bw):
                at = start + texture.tiled(x + ox, y + oy, pitch, {1: 0, 8: 3}[bpb]) * bpb
                data[at:at + bpb] = texel(m, x, y)
    if bpb > 1:                                           # 16-bit words byte-swapped, as the console keeps them
        for i in range(0, len(data) - 1, 2):
            data[i], data[i + 1] = data[i + 1], data[i]
    sizes = [size] + [0] * 14
    return struct.pack('>8I', 3, 0, fmt, flags, w, h, 1, mips) + struct.pack('>15I', *sizes) + bytes(data)


def textures() -> None:
    val = lambda m, x, y: bytes([(m * 64 + y * 8 + x) & 255])          # noqa: E731
    con = console_texture(texture.L8, 8, 8, 4, 1, val)
    try:
        pc = texture.convert(con)
    except texture.ConvertError as ex:
        check(False, 'texture: console 10 (L8) converts', str(ex))
        return
    head = struct.unpack_from('<8I', pc, 0)
    sizes = struct.unpack_from('<15I', pc, 32)[:4]
    lv = texture.levels(pc)
    want0 = bytes((y * 8 + x) & 255 for y in range(8) for x in range(8))
    want2 = bytes([128, 129, 0, 0, 136, 137, 0, 0])                      # level 2 (2 x 2): rows padded to 4 bytes
    check(head[2] == 10 and head[3] == 1 and sizes == (64, 16, 8, 4), 'texture: console 10 -> PC 10, the console\'s '
          'flags, level sizes 64 / 16 / 8 / 4 (rows of the 2 x 2 and 1 x 1 levels padded to 4 bytes)', f'{head} {sizes}')
    check(lv[0][4] == want0 and lv[2][4] == want2 and lv[3][4] == bytes([192, 0, 0, 0]),
          'texture: console 10 texels untiled as they are (no byte swap)', f'{lv[2][4].hex()} {lv[3][4].hex()}')
    block = lambda m, x, y: bytes([200, 100, 0, 0, 0, 0, 0, 0])         # noqa: E731 - every texel 200
    con3 = console_texture(texture.DXT5A, 8, 8, 2, 0, block)
    pc3 = texture.convert(con3)
    h3 = struct.unpack_from('<8I', pc3, 0)
    check(h3[2] == texture.L8 and h3[3] & texture.HALVED_ON_CONSOLE == 0 and set(texture.levels(pc3)[0][4]) == {200},
          'texture: console 3 (DXT5A) -> PC 10 keeps the console\'s flags (no bit 1: it is at the console\'s size)',
          str(h3))


def mesh_rename() -> None:
    R = meshport.renamer({'Characters/Beach/': 'Characters/WW2/', 'characters/beach/3p/us_shared/': 'Shared/'})
    got = [R('characters/BEACH/3P/Us_Scout/US_Scout_Mesh'), R('Characters/Beach/3P/Us_Shared/Eyeball'),
           R('Objects/Beach/x'), meshport.renamer(None)('a/b'), meshport.renamer(str.upper)('a/b')]
    check(got == ['Characters/WW2/3P/Us_Scout/US_Scout_Mesh', 'Shared/Eyeball', 'Objects/Beach/x', 'a/b', 'A/B'],
          'meshport.renamer: prefixes in any case, the longest first, other names kept; None; a function', str(got))
    s = {'a': 42, 'z': 0, 'lod_settings': b'Characters/Beach/3P/Jap_Shared/Jap_LodSettings', 'lods': [
        {'x': 0, 'shaders': 2, 'sets': [(b'Characters/Beach/3P/Us_Scout/US_Scout_Mesh/7F9DD04A-A5C3-11DD-9159-9004097745C1',
                                         [b'Characters/Beach/3P/Us_Scout/shaders/scout_head', b'Textures/x'])],
         'bounds': (0.0,) * 6, 'size': 12, 'extra': 0, 'bones': [0, 1], 'bone_hashes': [5, 6]}]}
    before = meshset.write(s, 'SkinnedMeshSet')
    done = meshport.rename_meshset(s, meshport.renamer({'Characters/Beach/': 'Characters/WW2/'}))
    after = meshset.write(s, 'SkinnedMeshSet')
    back = meshset.parse(after, 'SkinnedMeshSet', False)
    check(len(done) == 3 and back['lod_settings'] == b'Characters/WW2/3P/Jap_Shared/Jap_LodSettings'
          and back['lods'][0]['sets'][0][0].startswith(b'Characters/WW2/3P/Us_Scout/US_Scout_Mesh/7F9DD04A')
          and back['lods'][0]['sets'][0][1] == [b'Characters/WW2/3P/Us_Scout/shaders/scout_head', b'Textures/x']
          and len(after) == len(before) - 6 and back['lods'][0]['bones'] == [0, 1],
          'meshport.rename_meshset: LOD settings, shader set and shader names renamed, the rest kept', str(done))


def mopp_vm() -> None:
    info = (0.0, 0.0, 0.0, 65536.0)                      # 1 m = one byte at the start (q = p * 65536, shift 16)
    code = bytes([0x28, 0, 10,                             # 0: z within 0..10
                  0x10, 4, 5, 5,                           # 3: x <= 4 -> 7; x >= 5 -> 12
                  0x21, 2, 1,                              # 7: y <= 2 -> 10; y >= 2 -> 11
                  0x30, 0x31,                              # 10, 11: terminals 0, 1
                  0x09, 8,                                 # 12: terminal offset 8
                  0x23, 6, 7, 0, 1, 0, 2,                  # 14: x <= 6 -> 22; x >= 7 -> 23
                  0x00, 0x32,                              # 21: (pad) 22: terminal 2 + 8
                  0x01, 6, 0, 0,                           # 23: rescale into x cell 6, one more bit (half metres)
                  0x05, 1, 0x00,                           # 27: jump over a pad
                  0x26, 3, 255,                            # 30: x >= 3 half metres past 6 m (7.5 m)
                  0x50, 40])                               # 33: terminal 40 + 8
    q = lambda lo, hi: sorted(mopp.query(code, info, lo, hi))                                   # noqa: E731
    got = [q((1, 1, 1), (1.5, 1.5, 1.5)), q((1, 3, 1), (1, 3.5, 1)), q((5.2, 0, 0), (6.5, 0, 0)),
           q((7.2, 0, 0), (7.4, 0, 0)), q((8.5, 0, 0), (9, 0, 0)), q((0, 0, 11), (1, 1, 12))]
    check(got == [[0], [1], [10], [], [48], []], 'mopp: bounds, splits, terminal offsets, rescale and jumps answer '
          'box queries cell by cell', str(got))
    tris = {0: [[0, 0, 1], [3, 0, 1], [0, 2, 1]], 1: [[0, 3, 2], [3, 3, 2], [0, 4, 2]], 10: [[5.5, 0, 0], [6, 1, 0], [5.5, 1, 1]]}
    rays = [((1, 1, 5), (0, 0, -1)), ((1, 3.2, 5), (0, 0, -1)), ((5.7, 0.5, 5), (0, 0, -1)), ((9, 9, 9), (0, 0, 1))]
    same = [mopp.ray_cast(code, info, tris, o, d, 20) == mopp.ray_cast_brute(tris, o, d, 20) for o, d in rays]
    check(all(same) and mopp.ray_cast(code, info, tris, (1, 1, 5), (0, 0, -1), 20) == (4.0, 0),
          'mopp: the ray cast through the tree equals brute force', str(same))
    try:
        mopp.query_int(bytes([0x7F]), [0, 0, 0], [1, 1, 1])
        refused = False
    except mopp.MoppError:
        refused = True
    check(refused, 'mopp: an unknown opcode is refused')


def main() -> int:
    havok_model_group()
    mopp_vm()
    textures()
    mesh_rename()
    texcoords()
    print('all passed' if not FAILED[0] else f'{FAILED[0]} check(s) failed')
    return 1 if FAILED[0] else 0


if __name__ == '__main__':
    sys.exit(main())
