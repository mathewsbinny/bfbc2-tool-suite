r"""havok.py on the game's files (game tier; skipped without them):

- the class layouts read from BFBC2Game.exe: every class the PC's physics files name, known members at their
  offsets (hkpMoppBvTreeShape.child at 0x30, a struct; .childSize at 0x38, an int32);
- every HavokPhysicsData resource the console and PC share converts to the PC file exactly, or differing only in
  the values FORMAT.md 1.3 lists; a wrapper is always byte-identical;
- every console HavokPhysicsData resource of OPFOR's Onslaught builds converts without an error.

  python test_havok.py
"""
import glob
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import havok  # noqa: E402
import convert  # noqa: E402

OPFOR = os.environ.get('OPFOR_ROOT') or os.path.abspath(os.path.join(HERE, '..', '..', '..', 'project-opfor-bfbc2'))
EXE = os.path.join(OPFOR, 'game', 'BFBC2Game.exe')
PC = os.path.join(OPFOR, 'work', 'audit', 'mp_common')                 # PC mp_common, unpacked
CONSOLE = sorted(glob.glob(os.path.join(OPFOR, 'work', 'onslaught', 'build_*', 'con_*')))


def main():
    if not (os.path.isfile(EXE) and os.path.isfile(os.path.join(PC, 'fbrb.manifest')) and CONSOLE):
        print('skipped: needs Project OPFOR BFBC2 with the console Onslaught builds and an unpacked PC mp_common')
        return 0
    fails = []
    k = havok.classes(EXE)
    pc = convert.manifest(PC, 'HavokPhysicsData')
    names = set()
    for p in pc.values():                       # the class-name section (the first) of each PC packfile
        b = open(p, 'rb').read()
        o = b.find(havok.MAGIC)
        start, end = struct.unpack_from('<2I', b, o + 64 + 20)
        names |= {m.group(1).decode() for m in re.finditer(rb'\x09([A-Za-z][A-Za-z0-9_]+)\x00', b[o + start:o + end])}
    missing = sorted(n for n in names if n not in k)
    if missing:
        fails.append(f'classes not read from the executable: {missing}')
    mopp = {m.name: m for m in k['hkpMoppBvTreeShape'].members}
    if (mopp['child'].offset, mopp['child'].type, mopp['childSize'].offset, mopp['childSize'].type) != \
            (0x30, havok.STRUCT, 0x38, havok.INT32):
        fails.append(f'hkpMoppBvTreeShape members read wrongly: {mopp}')
    con = {}
    for f in CONSOLE:
        for rel, p in convert.manifest(f, 'HavokPhysicsData').items():
            con.setdefault(rel, p)
    shared = equivalent = 0
    for rel in sorted(set(pc) & set(con)):
        c, p = open(con[rel], 'rb').read(), open(pc[rel], 'rb').read()
        if len(c) != len(p):
            continue
        shared += 1
        try:
            exact, why, bad = havok.verify(c, p, k)
        except havok.ConvertError as e:
            fails.append(f'{rel}: {e}')
            continue
        if bad:
            fails.append(f'{rel}: {len(bad)} bytes differ, first at {bad[0]:#x}')
        else:
            equivalent += 1
    errs = 0
    for p in con.values():
        try:
            havok.convert(open(p, 'rb').read(), k)
        except havok.ConvertError as e:
            errs += 1
            fails.append(f'{p}: {e}')
    print(f'{len(k)} classes; {equivalent} of {shared} shared files equivalent; {len(con) - errs} of {len(con)} '
          f'console files converted')
    if shared < 300:
        fails.append(f'only {shared} shared files found (expected 355 or more)')
    for f in fails[:20]:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
