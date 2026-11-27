r"""Project OPFOR's physics-material edits in the Rust core (round OR4, "OPFOR in Rust" stage 4: bc2_convert::physmat
through bc2_native.physmat_*), on built inputs: seeded PC physics resources (test_havokport_native's builder) paired
with built HavokAsset partitions whose MaterialMappings and PartInfoDatas match their wrappers.

- havok_lists / list_hashes read the two lists and pair them with the wrapper's hashes;
- set_parts moves a part to an existing material (the dbx field and the part byte together), appends a new material
  to the three lists (MaterialMappings, the wrapper's hashes, the packfile's HavokMaterials) and refuses a missing
  part, a material without a hash, and slots that disagree before the edit;
- group_faces remaps a detail body's faces and refuses a material the group lacks and a count of members other than
  the detail bodies'.

Project OPFOR's test_ww2_pictures_parity checks the real assets against the retired Python's answers.

  python test_physmat_native.py
"""
from __future__ import annotations

import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
sys.path.insert(0, HERE)
import havokport  # noqa: E402
from bfbc2core import dbx, native  # noqa: E402
from test_havokport_native import random_pc  # noqa: E402

N = native.require('the physics materials test', 'physmat_set_parts')
FAILS: list[str] = []


def check(ok: bool, what: str) -> None:
    if not ok:
        print(f'FAIL {what}')
        FAILS.append(what)


def asset(maps: list[str], parts: list[tuple[str, str]]) -> bytes:
    """A HavokAsset partition: MaterialMappings and PartInfoDatas (Name, MaterialName)."""
    def text(name: str | None, value: str) -> dbx.Element:
        return dbx.Element('item' if name is None else 'field', dbx.TEXT,
                           [] if name is None else [('name', name)], value, b'', None)
    mm = dbx.Element('array', dbx.CONTAINER, [('name', 'MaterialMappings')], '', b'', [text(None, m) for m in maps])
    pid = dbx.Element('array', dbx.CONTAINER, [('name', 'PartInfoDatas')], '', b'',
                      [dbx.Element('complex', dbx.CONTAINER, [], '', b'', [text('Name', n), text('MaterialName', m)])
                       for n, m in parts])
    inst = dbx.Element('instance', dbx.CONTAINER, [('guid', 'A' * 8 + '-0000-0000-0000-000000000000'),
                                                   ('type', 'Physics.HavokAsset')], '', b'', [mm, pid])
    part = dbx.Element('partition', dbx.CONTAINER, [('guid', 'B' * 8 + '-0000-0000-0000-000000000000')], '', b'',
                       [inst])
    return dbx.write(dbx.DbxFile([], [part]))


def outcome(f, *a):
    try:
        return 'ok', f(*a)
    except SystemExit as e:
        return 'exit', str(e)
    except AssertionError as e:
        return 'assert', str(e)
    except ValueError as e:
        return 'value', str(e)


def main() -> int:
    rnd = random.Random(1943)
    cases = edits = grown = faces = 0
    for _ in range(600):
        res, _, names = random_pc(rnd)
        try:
            hashes, idx = havokport.part_materials(res)
            hm = havokport.havok_materials(res)
        except ValueError:
            check(outcome(N.physmat_list_hashes, res, asset([], []))[0] in ('value', 'exit'),
                  'a resource that does not read is refused')
            continue
        maps = list(hm) if len(hm) == len(hashes) else [f'mat_{i}' for i in range(len(hashes))]
        if not maps or not idx or max(idx) >= len(maps):
            continue
        parts = [(f'part_{i}', maps[k]) for i, k in enumerate(idx)]
        data = asset(maps, parts)
        cases += 1
        lists = N.physmat_havok_lists(data)
        check(list(lists[0]) == maps and [tuple(p) for p in lists[1]] == parts, 'havok_lists: the two lists')
        check(dict(N.physmat_list_hashes(res, data)) == {m.lower(): h for m, h in zip(maps, hashes)},
              'list_hashes: the names beside the wrapper\'s hashes')
        to = rnd.choice(maps)
        r2, d2, ch = N.physmat_set_parts(res, data, {'part_0': to}, {})
        _, idx2 = havokport.part_materials(r2)
        check(maps[idx2[0]].lower() == to.lower() and N.physmat_havok_lists(d2)[1][0][1] == to,
              'set_parts: the part byte and the dbx field moved together')
        check([tuple(c) for c in ch] == ([] if parts[0][1] == to else [('part_0', parts[0][1], to)]),
              'set_parts: the change listed')
        edits += 1
        if list(hm) == maps:
            r3, d3, _ = N.physmat_set_parts(res, data, {'part_0': 'Brand_New'}, {'brand_new': 0x1234})
            h3, idx3 = havokport.part_materials(r3)
            check(list(h3) == list(hashes) + [0x1234] and idx3[0] == len(maps) and
                  havokport.havok_materials(r3) == maps + ['brand_new'] and
                  list(N.physmat_havok_lists(d3)[0]) == maps + ['brand_new'],
                  'set_parts: a new material appended to the three lists')
            grown += 1
        check(outcome(N.physmat_set_parts, res, data, {'no_such': maps[0]}, {})[0] == 'exit', 'a missing part refused')
        check(outcome(N.physmat_set_parts, res, data, {'part_0': 'Unhashed'}, {})[0] == 'exit',
              'a material without a hash refused')
        if len(maps) > 1:
            other = [(n, maps[(maps.index(m) + 1) % len(maps)]) for n, m in parts]
            if [m.lower() for _, m in other] != [m.lower() for _, m in parts]:
                check(outcome(N.physmat_set_parts, res, asset(maps, other), {'part_0': maps[0]}, {})[0] == 'exit',
                      'slots that disagree refused')
        bodies = havokport.bodies(res)
        details = [m for _, m in bodies if m is not None]
        faces_ok = all(x < len(hm) for s in details for lst in s for x in lst)
        if hm and details and faces_ok and any(any(s) for s in details):
            members = [f'g/member_{i}' for i in range(len(details))]
            src_face = next(hm[x] for s in details for lst in s for x in lst)
            rule = {f'member_{i}': {src_face: hm[-1]} for i in range(len(details))}
            r4, changes = N.physmat_group_faces(res, members, rule)
            after = [m for _, m in havokport.bodies(r4) if m is not None]
            want = [[[len(hm) - 1 if hm[x] == src_face else x for x in lst] for lst in s] for s in details]
            check(after == want and len(changes) == sum(hm[x] == src_face for s in details for lst in s for x in lst),
                  'group_faces: the faces remapped, each change listed')
            check(outcome(N.physmat_group_faces, res, members + ['g/extra'], rule)[0] == 'exit',
                  'group_faces: a member count other than the detail bodies refused')
            check(outcome(N.physmat_group_faces, res, members, {'member_0': {src_face: 'nothing'}})[0] == 'exit',
                  'group_faces: a material the group lacks refused')
            faces += 1
    cut = asset(['a'], [('p', 'a')])[:-9]
    check(outcome(N.physmat_havok_lists, cut)[0] == 'value', 'a partition cut short refused')
    check(cases > 100 and edits > 100 and grown > 10 and faces > 10,
          f'enough cases: {cases} resources, {edits} edits, {grown} grown, {faces} face remaps')
    print(f'{cases} resources, {edits} part edits, {grown} lists grown, {faces} face remaps; '
          f'{len(FAILS)} failed' if FAILS else f'{cases} resources, {edits} part edits, {grown} lists grown, '
          f'{faces} face remaps: all ok')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
