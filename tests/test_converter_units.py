r"""Unit tests of the converters' Stage 3 fixes, without game data (quick tier):

- havokport: a model group's shape subparts get translation.w 1.0 and the shape's aabbCenter.w / aabbHalfExtents.w
  as DICE's exporter writes them (values from two of DICE's own levels: mp_006's rigid group, sp_008's static grid);
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


def main() -> int:
    havok_model_group()
    texcoords()
    print('all passed' if not FAILED[0] else f'{FAILED[0]} check(s) failed')
    return 1 if FAILED[0] else 0


if __name__ == '__main__':
    sys.exit(main())
