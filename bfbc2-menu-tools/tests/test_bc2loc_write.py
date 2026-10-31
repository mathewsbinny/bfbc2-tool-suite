r"""bc2loc.py writing language files, on the game's files (game tier; skipped without them):

- every language of the game copy is written back byte-identical when nothing is set;
- set adds a new key and changes an existing one: both read back, every other entry keeps its text, the entry
  table stays sorted by hash and the header matches the file;
- text that is not ASCII is refused.

  python test_bc2loc_write.py
"""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import bc2loc  # noqa: E402
import menucommon as mc  # noqa: E402


def main():
    try:
        root = mc.find_opfor()
        langs = bc2loc.languages(root)
    except (mc.MenuToolError, FileNotFoundError):
        langs = {}
    if not langs:
        print('SKIP: no Project OPFOR BFBC2 game copy')
        return 0
    fails = []
    for lang in sorted(langs):
        loc = bc2loc.load(lang, root)
        if loc.to_bytes() != loc.data:
            fails.append(f'{lang}: not written back identical')
        old = dict(loc.raw)
        h_new = loc.set('OPFOR_TEST_NEW_KEY', 'NEW TEXT')
        h_m1 = loc.set('NAME_M1', 'CHANGED')
        b = loc.to_bytes()
        back = bc2loc.LocFile(b, None, lang)
        if back.raw.get(h_new) != b'NEW TEXT' or back.raw.get(h_m1) != b'CHANGED':
            fails.append(f'{lang}: the set texts do not read back')
        others = [h for h in old if h not in (h_new, h_m1) and back.raw.get(h) != old[h]]
        if others:
            fails.append(f'{lang}: {len(others)} other entries changed')
        _, size, count, table, _ = struct.unpack_from('<5I', b, 0)
        hashes = [struct.unpack_from('<I', b, 8 + table + 8 * i)[0] for i in range(count)]
        if hashes != sorted(hashes) or size != len(b) - 8 or count != len(old) + 1:
            fails.append(f'{lang}: table or header wrong')
        try:
            loc.set('X', 'café')
            fails.append(f'{lang}: non-ASCII text accepted')
        except mc.MenuToolError:
            pass
    print(f'{len(langs)} languages written back and edited')
    for f in fails[:20]:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
