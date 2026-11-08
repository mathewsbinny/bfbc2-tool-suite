"""Generated aim poses (AimAnimation resources, `*_generatedaimanimation.res`): read, written for the PC, bones
renumbered - Battlefield 1943's (Xbox 360, big-endian) to BFBC2's.

A third-person weapon's aim pose: the upper body and the weapon bones over a grid of aim directions (25 poses: 5 x 5).
Layout (decoded 2026-11-08 from BFBC2's and BF1943's files; every file of both games reads to its exact size):

  0x00  u32 1 (version)
  0x04  u32 an address (the build tool's; the data's at 0x0c - kept as is)
  0x08  u32 data size (the file less 12 bytes)
  0x0c  u32 the address + 0x10
  0x10  u32 bone count
  0x14  u8 pose count (25), 3 bytes 0
  0x18  u32 0
  0x1c  bone count x (u8 skeleton bone index, u8 kind)
  then 0 bytes to a 16-byte boundary counted from 0x0c, and per bone in table order:
    kind 0 (fixed)      position (x, y, z, 1) and rotation quaternion (x, y, z, w)          32 bytes
    kind 1 (rotation)   position (x, y, z, 0), then pose count x quaternion                 16 + 16 n
    kind 2 (moving)     pose count x (position (x, y, z, 1), quaternion)                    32 n
All numbers 32-bit floats. Nothing else (the "memory addresses" the importer listed were the two header words).

    python aimanim.py <file.res> [--big]          prints the bones and the first pose
"""
import struct
import sys

KIND_NAMES = {0: 'fixed', 1: 'rotation', 2: 'moving'}


class AimError(Exception):
    pass


def _data_at(nb):
    o = 0x1c + 2 * nb
    return o + (-(o - 0x0c)) % 16


def parse(b, big_endian=False):
    """{'addr', 'poses', 'bones': [(bone, kind, block)]}; block: kind 0 (pos4, quat4), kind 1 (pos4, [quat4]),
    kind 2 ([(pos4, quat4)])."""
    e = '>' if big_endian else '<'
    if len(b) < 0x1c:
        raise AimError('too short for an aim animation')
    ver, addr, size, addr2, nb = struct.unpack_from(e + '5I', b, 0)
    if ver != 1 or size != len(b) - 12 or addr2 != (addr + 0x10) & 0xffffffff:
        raise AimError(f'not an aim animation of the known layout (version {ver}, size {size} of {len(b) - 12})')
    n = b[0x14]
    table = [(b[0x1c + 2 * i], b[0x1d + 2 * i]) for i in range(nb)]
    o = _data_at(nb)
    f4 = lambda at: struct.unpack_from(e + '4f', b, at)                                 # noqa: E731
    bones = []
    for bone, kind in table:
        if kind == 0:
            block = (f4(o), f4(o + 16))
            o += 32
        elif kind == 1:
            block = (f4(o), [f4(o + 16 + 16 * k) for k in range(n)])
            o += 16 + 16 * n
        elif kind == 2:
            block = [(f4(o + 32 * k), f4(o + 32 * k + 16)) for k in range(n)]
            o += 32 * n
        else:
            raise AimError(f'bone {bone}: unknown kind {kind}')
        bones.append((bone, kind, block))
    if o != len(b):
        raise AimError(f'{len(b) - o} bytes left after the bones')
    return {'addr': addr, 'poses': n, 'bones': bones}


def write(a):
    """PC (little-endian) bytes."""
    nb, n = len(a['bones']), a['poses']
    head = bytearray(_data_at(nb))
    struct.pack_into('<5I', head, 0, 1, a['addr'], 0, (a['addr'] + 0x10) & 0xffffffff, nb)
    head[0x14] = n
    for i, (bone, kind, _) in enumerate(a['bones']):
        head[0x1c + 2 * i], head[0x1d + 2 * i] = bone, kind
    body = bytearray()
    v = lambda q: struct.pack('<4f', *q)                                               # noqa: E731
    for bone, kind, block in a['bones']:
        if kind == 0:
            body += v(block[0]) + v(block[1])
        elif kind == 1:
            if len(block[1]) != n:
                raise AimError(f'bone {bone}: {len(block[1])} rotations for {n} poses')
            body += v(block[0]) + b''.join(v(q) for q in block[1])
        else:
            if len(block) != n:
                raise AimError(f'bone {bone}: {len(block)} poses for {n}')
            body += b''.join(v(p) + v(q) for p, q in block)
    out = bytes(head) + bytes(body)
    return out[:8] + struct.pack('<I', len(out) - 12) + out[12:]


def renumbered(a, bones):
    """The aim animation with its bone table renumbered ({old index: new index}); the bones sorted by their new
    index as BFBC2's files are. A bone the map lacks raises."""
    out = []
    for bone, kind, block in a['bones']:
        if bone not in bones:
            raise AimError(f'bone {bone} has no counterpart in the target skeleton')
        out.append((bones[bone], kind, block))
    return dict(a, bones=sorted(out, key=lambda t: t[0]))


def to_pc(b, bones=None):
    """BF1943's (big-endian) aim animation as a PC resource, bones renumbered with `bones` when given."""
    a = parse(b, True)
    return write(renumbered(a, bones) if bones else a)


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    a = parse(open(argv[1], 'rb').read(), '--big' in argv)
    print(f'{len(a["bones"])} bones, {a["poses"]} poses')
    for bone, kind, block in a['bones']:
        first = block[0] if kind == 0 else (block[0], block[1][0]) if kind == 1 else block[0]
        print(f'  bone {bone:3d} {KIND_NAMES[kind]:8s} {first}')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except AimError as er:
        print(f'error: {er}', file=sys.stderr)
        sys.exit(1)
