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

parse, write and to_pc run in the suite's Rust core (round F3: bc2_native.aimanim_parse / aimanim_write /
aimanim_to_pc, bc2_formats::aimanim), which they need (bfbc2core.native.NativeMissing says how to get it). Their
Python reference was retired in round RT1 against its recorded answers (tests/f3_golden_aim.json); a file or a value
the core does not take (cut or damaged, a NaN, a value out of range or of another type) raises AimError.
"""
import os
import struct
import sys
from typing import Any

KIND_NAMES = {0: 'fixed', 1: 'rotation', 2: 'moving'}


class AimError(Exception):
    pass


# Round RT1 (2026-11-20): parse, write and to_pc are the Rust core's alone (bc2_native.aimanim_parse / aimanim_write
# / aimanim_to_pc, bc2_formats::aimanim). The Python reader / writer they were proven against (parse_reference,
# write_reference, to_pc_reference) was retired; its answers are kept in tests\f3_golden_aim.json. What the core
# does not take raises AimError (a cut or damaged file, a NaN, a value out of range or of another type). The text
# above is the command line's usage (bfbc2.exe aimanim prints it): round RT2 brought it up to date, and O2's
# recording of it (core/tests/cli_o2_nav_golden.json) with it.
_NATIVE: list[Any] = []


def _native() -> Any:
    """The Rust core with the aimanim functions, or NativeMissing saying how to get it."""
    if not _NATIVE:
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        if root not in sys.path:
            sys.path.insert(0, root)
        import suite_paths
        suite_paths.use('core')
        from bfbc2core import native
        _NATIVE.append(native.require('aim animations (aimanim, round F3)', 'aimanim_parse'))
    return _NATIVE[0]


def parse(b, big_endian=False):
    """{'addr', 'poses', 'bones': [(bone, kind, block)]}; block: kind 0 (pos4, quat4), kind 1 (pos4, [quat4]),
    kind 2 ([(pos4, quat4)]). AimError when it is not an aim animation the reader takes."""
    b = bytes(b)
    got = _native().aimanim_parse(b, bool(big_endian))
    if got is None:
        raise AimError(_refused(b, big_endian))
    return got


def _refused(b, big_endian):
    """Why the reader refused b, in the header's words where the header is the problem (the retired reader's
    messages, which the command line prints)."""
    if len(b) < 0x1c:
        return 'too short for an aim animation'
    ver, addr, size, addr2, _nb = struct.unpack_from(('>' if big_endian else '<') + '5I', b, 0)
    if ver != 1 or size != len(b) - 12 or addr2 != (addr + 0x10) & 0xffffffff:
        return f'not an aim animation of the known layout (version {ver}, size {size} of {len(b) - 12})'
    return 'not an aim animation the reader takes (a cut or damaged file, or a value it does not read)'


def write(a):
    """PC (little-endian) bytes. AimError when the animation has something the writer does not take (a NaN, a value
    out of range or of another type, a block of another shape, rotations or poses not matching the pose count)."""
    got = _native().aimanim_write(a)
    if got is None:
        raise AimError('the aim animation cannot be written: a NaN, a value out of range or of another type, a block of '
                       'another shape, or rotations / poses not matching the pose count')
    return got


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
    """BF1943's (big-endian) aim animation as a PC resource, bones renumbered with `bones` ({old index: new index})
    when given. AimError where parse or write would raise, or a bone the map lacks."""
    got = _native().aimanim_to_pc(bytes(b), bones)
    if got is None:
        raise AimError('the aim animation cannot be converted: not one the reader takes, a bone map that is not '
                       '{old index: new index} covering every bone, or a value the writer does not take')
    return got


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
