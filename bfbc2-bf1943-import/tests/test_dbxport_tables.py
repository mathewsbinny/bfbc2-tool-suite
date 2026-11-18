r"""dbxport's conversion tables in the Rust port against the Python reference (round E4d's fix; quick tier).

The Rust port reads dbxport's data tables (ONE_TO_ARRAY, RENAMED, NESTED, ENUM_NUMBERS, NUMBERS_TO_ITEMS, TYPE_AS,
DICE_MOVED_CLASSES, HINT_ACTIONS, HINT_TEXTS) from the caller's process. Every entry of every table is ported both
ways (BFBC2_DBXPORT_DUAL: the same bytes, report lines, records or exception), then again with the tables changed
as a caller changes them - in place, as Project OPFOR's bf1943_vehicles puts the planes' gear input into
ENUM_NUMBERS (16 -> EiaDynamicGadget2, the bug this test was written for: built-in tables ported it as EiaUndefined)
- and with entries added and removed. The template roles keep the tables of when their partitions were added (as
the reference reads them), and a change to what the Rust port has built in (a constant, a converter) ports in
Python.

  python test_dbxport_tables.py
"""
import contextlib
import copy
import os
import sys
from collections.abc import Iterator
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
sys.path.insert(0, HERE)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import dbxport  # noqa: E402
from bfbc2core import dbx  # noqa: E402

# isort: split
from test_dbxport_rules import arr, boolf, cx, inst, item, num, part, ref, txt  # noqa: E402

E = dbx.Element
A = 'aaaaaaaa-0000-0000-0000-00000000000'
B = 'bbbbbbbb-0000-0000-0000-00000000000'
fails: list[str] = []
compared = [0]


def check(ok: Any, what: str, detail: Any = '') -> None:
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        fails.append(what)


def ints_raw(vals: list[int]) -> bytes:
    """A 4-byte integer array's raw bytes (count, size, values)."""
    raw = bytearray()
    dbx.put_varint(raw, len(vals))
    dbx.put_varint(raw, 4)
    for v in vals:
        raw += v.to_bytes(4, 'big', signed=True)
    return bytes(raw)


def tpl(*insts: dbx.Element, name: str = 'tpl') -> dbxport.Templates:
    return dbxport.Templates([(name, part(insts[0].attr('guid') or '', list(insts),
                                          '22222222-2222-2222-2222-222222222222'))])


def both(data: bytes, T: dbxport.Templates, **kw: Any) -> tuple[str, Any]:
    """The port both ways (DUAL raises DualMismatch on a difference): ('ok', (bytes, report)) or ('error', exc)."""
    before = dbxport.STATS['dual']
    try:
        got: Any = ('ok', dbxport.port(data, T, **kw))
    except dbxport.DualMismatch:
        raise
    except Exception as e:                                      # noqa: BLE001 - the reference's own, compared
        got = ('error', e)
    if dbxport.STATS['dual'] == before:
        raise AssertionError(f'not compared: left to Python ({dict(dbxport.STATS["why"])})')
    compared[0] += 1
    return got


def lines(got: tuple[str, Any]) -> list[str]:
    return list(got[1][1]) if got[0] == 'ok' else [f'{type(got[1]).__name__}: {got[1]}']


@contextlib.contextmanager
def changed(table: Any, update: Any) -> Iterator[None]:
    """A table changed in place for the duration (as a caller changes it), then restored."""
    saved = copy.deepcopy(table)
    update(table)
    try:
        yield
    finally:
        table.clear()
        table.update(saved)


def run(what: str, fn: Any) -> None:
    try:
        n = compared[0]
        detail = fn()
        check(True, f'{what}: {compared[0] - n} ports the same both ways' + (f' ({detail})' if detail else ''))
    except (dbxport.DualMismatch, AssertionError) as e:
        check(False, what, str(e)[:400])


# ---------------------------------------------------------------------------------------------------- the tables

def enum_numbers() -> str:
    seen = []
    for (t, n), table in list(dbxport.ENUM_NUMBERS.items()):
        for number in sorted(table) + [1, 2, 3, 99, -1, 2 ** 31 - 1]:
            for with_template in (True, False):
                body = [num(n, [number], '>i')]
                data = part(A + '1', [inst(A + '1', f'Physics.{t}', body)])
                T = tpl(inst(B + '1', f'Physics.{t}', [txt(n, 'EiaFire')])) if with_template else \
                    tpl(inst(B + '1', 'Other.Type', [txt('Name', 'x')]))
                got = both(data, T, schema='report')
                seen.append((t, number, with_template, any('value' in x or 'UNMAPPED' in x for x in lines(got))))
    return f'{len(seen)} (type, number, template) cases over {len(dbxport.ENUM_NUMBERS)} tables'


def enum_numbers_changed() -> str:
    key = ('ChildRotationBodyData', 'RotationInput')
    gear = dbxport.ENUM_NUMBERS[key]
    data = part(A + '1', [inst(A + '1', 'Physics.ChildRotationBodyData', [num('RotationInput', [16], '>i')]),
                          inst(A + '2', 'Physics.ChildRotationBodyData', [num('RotationInput', [5], '>i')])])
    T = tpl(inst(B + '1', 'Physics.ChildRotationBodyData', [txt('RotationInput', 'EiaFire')]))
    plain = lines(both(data, T))
    saved = gear[16]
    gear[16] = 'EiaDynamicGadget2'                     # exactly as bf1943_vehicles does, on the table it holds
    try:
        opfor = lines(both(data, T))
    finally:
        gear[16] = saved
    again = lines(both(data, T))
    assert any('16 -> EiaDynamicGadget2' in x for x in opfor), opfor
    assert any('16 -> EiaUndefined' in x for x in plain) and again == plain, (plain, again)
    # entries added, removed, a table emptied, a new table
    with changed(gear, lambda d: d.update({3: 'EiaBrake'})):
        both(part(A + '1', [inst(A + '1', 'Physics.ChildRotationBodyData', [num('RotationInput', [3], '>i')])]), T)
    with changed(gear, lambda d: d.pop(5)):
        both(data, T, schema='report')
    with changed(gear, lambda d: d.clear()):
        both(data, T, schema='report')
    with changed(dbxport.ENUM_NUMBERS, lambda d: d.update({('Thing', 'InputAction'): {1: 'EiaZoom'}})):
        both(part(A + '1', [inst(A + '1', 'X.Thing', [num('InputAction', [1], '>i')])]),
             tpl(inst(B + '1', 'X.Thing', [txt('InputAction', 'EiaFire')])))
    return 'the gear input (16 -> EiaDynamicGadget2) and back, entries added and removed, a table emptied, a new one'


def hints() -> str:
    n = 0
    for spelling in list(dbxport.HINT_ACTIONS) + ['EIAFire', 'EIAThrottle', 'EIA', 'EiaFire', ' EIAPitch ']:
        for field in ('HintAction1', 'FireInputAction', 'Name'):
            data = part(A + '1', [inst(A + '1', 'X.Hint', [txt(field, spelling), num('Seat', [1], '>i')])])
            both(data, tpl(inst(B + '1', 'X.Hint', [txt(field, 'x'), num('Seat', [0], '>i')])))
            n += 1
    for text in list(dbxport.HINT_TEXTS) + ['VH_OTHER']:
        data = part(A + '1', [inst(A + '1', 'X.Hint', [txt('HintLine1', text), txt('HintLine2', text)])])
        both(data, tpl(inst(B + '1', 'X.Hint', [txt('HintLine1', 'x'), txt('HintLine2', 'y')])))
        n += 1
    data = part(A + '1', [inst(A + '1', 'X.Hint', [txt('HintAction1', 'EIACustom'), txt('HintLine1', 'VH_NEW')])])
    T = tpl(inst(B + '1', 'X.Hint', [txt('HintAction1', 'x'), txt('HintLine1', 'y')]))
    with changed(dbxport.HINT_ACTIONS, lambda d: d.update({'EIACustom': 'EiaFire'})), \
            changed(dbxport.HINT_TEXTS, lambda d: d.update({'VH_NEW': 'VEHICLE_HINT_NEW'})):
        got = lines(both(data, T))
    assert any('EIACustom -> EiaFire' in x for x in got) and any('VH_NEW -> VEHICLE_HINT_NEW' in x for x in got), got
    return f'{n} spellings and texts; added entries honoured'


def fields() -> str:
    """ONE_TO_ARRAY, RENAMED, NESTED, NUMBERS_TO_ITEMS: every entry, then changed."""
    n = 0
    for many, single in list(dbxport.ONE_TO_ARRAY.items()):
        for r in ('null', B + '9', 'other/thing/' + B + '8'):
            data = part(A + '1', [inst(A + '1', 'X.State', [ref(single, r)])])
            both(data, tpl(inst(B + '1', 'X.State', [arr(many, [item('null')])]), inst(B + '9', 'X.Mesh', [])),
                 resolve=lambda p: True)
            n += 1
    for new, old in list(dbxport.RENAMED.items()):
        data = part(A + '1', [inst(A + '1', 'X.Body', [num(old, [0.1]), num('Keep', [1.0])])])
        both(data, tpl(inst(B + '1', 'X.Body', [num(new, [0.5]), num('Keep', [2.0])])))
        n += 1
    for (here, new), moved in list(dbxport.NESTED.items()):
        for conventions in (True, False):
            data = part(A + '1', [inst(A + '1', 'X.Weapon', [cx(here, [num(m, [2.0]) for m in moved])])])
            T = tpl(inst(B + '1', 'X.Weapon', [cx(here, [cx(new, [num(m, [0.5]) for m in moved])])]))
            both(data, T, conventions=conventions)
            n += 1
    for (t, field), (key, others) in list(dbxport.NUMBERS_TO_ITEMS.items()):
        proto = cx('', [num(key, [0], '>i')] + [num(f, [5], '>i') for f in others])
        T = tpl(inst(B + '1', f'X.{t}', [arr(field, [proto])]))
        for vals in ([], [0, 1, 2, 3], [7]):
            data = part(A + '1', [inst(A + '1', f'X.{t}', [E('array', dbx.NUMBERS, [('name', field)], '',
                                                             ints_raw(vals))])])
            both(data, T, schema='report')
            n += 1
    # changed: a new one-to-array field, a renamed field, a nested struct, an item map with other defaults
    data = part(A + '1', [inst(A + '1', 'X.Body', [ref('Wheel', 'null'), num('OldName', [3.0]),
                                                   cx('Outer', [num('Deep', [4.0])]),
                                                   E('array', dbx.NUMBERS, [('name', 'Slots')], '',
                                                     ints_raw([1, 2]))])])
    T = tpl(inst(B + '1', 'X.Body', [arr('Wheels', []), num('NewName', [0.0]), cx('Outer', [cx('Inner', [num(
        'Deep', [0.0])])]), arr('Slots', [cx('', [num('Slot', [0], '>i'), num('Spare', [0], '>i')])])]))
    with changed(dbxport.ONE_TO_ARRAY, lambda d: d.update({'Wheels': 'Wheel'})), \
            changed(dbxport.RENAMED, lambda d: d.update({'NewName': 'OldName'})), \
            changed(dbxport.NESTED, lambda d: d.update({('Outer', 'Inner'): ('Deep',)})), \
            changed(dbxport.NUMBERS_TO_ITEMS, lambda d: d.update({('Body', 'Slots'): ('Slot', {'Spare': 9})})):
        got = lines(both(data, T, schema='report', conventions=False))
    assert any('renamed' in x for x in got) and any('nested' in x for x in got) and \
        any('Slot the number; Spare 9' in x for x in got), got
    return f'{n} entries; added entries honoured'


def classes() -> str:
    """TYPE_AS and DICE_MOVED_CLASSES: every entry, then changed."""
    n = 0
    for old, new in list(dbxport.TYPE_AS.items()):
        data = part(A + '1', [inst(A + '1', old, [txt('Name', 'seat'), boolf('Retract', True)])])
        both(data, tpl(inst(B + '1', new, [txt('Name', 'x')])), schema='report')
        n += 1
    for moved in sorted(dbxport.DICE_MOVED_CLASSES):
        for conventions in (True, False):
            data = part(A + '1', [inst(A + '1', 'X.Weapon', [ref('Persistent', A + '2')]),
                                  inst(A + '2', moved, [txt('Name', 'p')])])
            both(data, tpl(inst(B + '1', 'X.Weapon', [ref('Persistent', 'null')]), inst(B + '2', moved, [])),
                 conventions=conventions)
            n += 1
    data = part(A + '1', [inst(A + '1', 'X.Weapon', [ref('Part', A + '2')]), inst(A + '2', 'X.OldSeat', [txt('N', 'a')]),
                          inst(A + '3', 'X.Gone', [])])
    T = tpl(inst(B + '1', 'X.Weapon', [ref('Part', 'null')]), inst(B + '2', 'X.NewSeat', [txt('N', 'b')]))
    with changed(dbxport.TYPE_AS, lambda d: d.update({'X.OldSeat': 'X.NewSeat'})), \
            changed(dbxport.DICE_MOVED_CLASSES, lambda s: s.add('X.Gone')):
        got = lines(both(data, T, schema='report'))
    assert any('written as X.NewSeat' in x for x in got) and any('X.Gone' in x and 'left out' in x for x in got), got
    return f'{n} entries; added entries honoured'


def roles_at_add() -> str:
    """Template roles read ENUM_NUMBERS as it was when their partition was added (the reference's eager tables):
    the Rust side's, the Python port's lazy tables, and a pool whose Python tables were built at once agree."""
    gear = dbxport.ENUM_NUMBERS[('ChildRotationBodyData', 'RotationInput')]
    pool = [inst(B + '1', 'Physics.ChildRotationBodyData', [num('RotationInput', [16], '>i'), txt('Tag', 'one')]),
            inst(B + '2', 'Physics.ChildRotationBodyData', [num('RotationInput', [5], '>i'), txt('Tag', 'two')])]
    lazy, eager = tpl(*pool), tpl(*pool)
    assert eager.candidates                                     # the Python tables now, as the reference built them
    data = part(A + '1', [inst(A + '1', 'Physics.ChildRotationBodyData', [num('RotationInput', [16], '>i')])])
    saved = gear[16]
    gear[16] = 'EiaDynamicGadget2'
    try:
        got = lines(both(data, lazy))
        native, dbxport.NATIVE_PORT, dbxport.DUAL = dbxport.NATIVE_PORT, False, False
        try:
            ref_lines = list(dbxport.port(data, eager)[1])
        finally:
            dbxport.NATIVE_PORT, dbxport.DUAL = native, True
    finally:
        gear[16] = saved
    assert got == ref_lines, (got, ref_lines)
    return 'templates added before the change keep their roles'


def fixed_rules() -> str:
    """A change to what the Rust port has built in (a constant, a converter, a guard) ports in Python."""
    data = part(A + '1', [inst(A + '1', 'X.T', [txt('Name', 'a')])])
    T = tpl(inst(B + '1', 'X.T', [txt('Name', 'b')]))
    left = []
    for name, value in (('FIREARM_SPEED', 50.0), ('AUDIO_ID_SIZE', 4)):
        saved = getattr(dbxport, name)
        setattr(dbxport, name, value)
        before = dbxport.STATS['fallback']
        try:
            dbxport.port(data, T)
        finally:
            setattr(dbxport, name, saved)
        left.append(dbxport.STATS['fallback'] - before)
    saved_guards = list(dbxport.ROLE_GUARDS)
    dbxport.ROLE_GUARDS.append(saved_guards[0])
    before = dbxport.STATS['fallback']
    try:
        dbxport.port(data, T)
    finally:
        dbxport.ROLE_GUARDS[:] = saved_guards
    left.append(dbxport.STATS['fallback'] - before)
    assert left == [1, 1, 1], left
    both(data, T)                                               # and back in Rust once restored
    return 'FIREARM_SPEED, AUDIO_ID_SIZE, ROLE_GUARDS changed: each left to Python'


def main() -> int:
    native = dbxport._native()
    if native is None:
        print('ok   no Rust core built: nothing to compare (python rust\\build_native.py)')
        print('all passed')
        return 0
    args = dbxport._tables_args(dbxport._tables())
    check(args is not None and native.DbxRules(*args).is_default(),
          "the Rust side's own tables are dbxport.py's (DbxRules.is_default)")
    dbxport.DUAL = True
    run('ENUM_NUMBERS: every number of every table, mapped and not, with and without a template', enum_numbers)
    run('ENUM_NUMBERS changed in place (Project OPFOR\'s gear input)', enum_numbers_changed)
    run('HINT_ACTIONS / HINT_TEXTS: every spelling and text', hints)
    run('ONE_TO_ARRAY / RENAMED / NESTED / NUMBERS_TO_ITEMS: every entry', fields)
    run('TYPE_AS / DICE_MOVED_CLASSES: every entry', classes)
    run('template roles keep the tables of when they were added', roles_at_add)
    run('built-in rules changed', fixed_rules)
    print('all passed' if not fails else f'{len(fails)} failed')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
