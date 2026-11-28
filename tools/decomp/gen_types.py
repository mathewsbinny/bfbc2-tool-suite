r"""Turn the spec's reflection database (spec/typeinfo.json: 1,900 typeinfo records) into a type plan for Ghidra
(round DC0; tools/decomp/README.md). ghidra_scripts/ApplyTypes.java reads the plan and makes the data types.

    python tools/decomp/gen_types.py --out <private>\index\types_plan.json

Each record becomes one Ghidra type, named as the record is:
  - a primitive maps to Ghidra's built-in of its size (Float32 float, Uint32 uint, Boolean bool ...); String and
    FileRef (12 bytes) become the structure `String` {begin, end, capacity} (an eastl string, OPFOR's bc2.h), Guid a
    16-byte structure;
  - an enumeration becomes an enum of the record's size with its values;
  - a value type or class becomes a structure of the record's size: a class's parent first (`super_<Parent>`, the
    parent's size), then the members at their offsets. A member typed by a class is a pointer to it (a reference);
    a value type is embedded; an array member `T[]` is the 20-byte `RefArray<T>` for a class element and the 16-byte
    `Array<T>` otherwise (the slot sizes typeinfo.json's offsets give for 551 of its 564 array members; the other
    13 slots are larger, never smaller).
The plan lists the structures before anything embeds them. Facts only (names, sizes, offsets), like typeinfo.json.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
TYPEINFO = SUITE / 'spec' / 'typeinfo.json'
PRIMS = {'Boolean': 'bool', 'Int8': 'sbyte', 'Uint8': 'byte', 'Int16': 'short', 'Uint16': 'ushort', 'Int32': 'int',
         'Uint32': 'uint', 'Int64': 'longlong', 'Uint64': 'ulonglong', 'Float32': 'float', 'Float64': 'double'}
SPECIAL = {'String': 'String', 'FileRef': 'String', 'Guid': 'Guid'}
REF_ARRAY, VALUE_ARRAY = 20, 16


def field_type(t: str, recs: dict[str, Any], arrays: dict[str, dict[str, Any]]) -> dict[str, Any]:
    if t.endswith('[]'):
        el = t[:-2]
        is_ref = recs.get(el, {}).get('kind') == 'class'
        name = ('RefArray<' if is_ref else 'Array<') + el + '>'
        arrays.setdefault(name, {'name': name, 'kind': 'struct', 'size': REF_ARRAY if is_ref else VALUE_ARRAY,
                                 'members': [], 'note': f'array of {el}'})
        return {'kind': 'struct', 'name': name}
    if t in PRIMS:
        return {'kind': 'builtin', 'name': PRIMS[t]}
    if t in SPECIAL:
        return {'kind': 'struct', 'name': SPECIAL[t]}
    r = recs[t]
    if r['kind'] == 'enumeration':
        return {'kind': 'enum', 'name': t}
    if r['kind'] == 'class':
        return {'kind': 'pointer', 'name': t}
    return {'kind': 'struct', 'name': t}


def build(recs: dict[str, Any]) -> dict[str, Any]:
    arrays: dict[str, dict[str, Any]] = {}
    types: dict[str, dict[str, Any]] = {
        'String': {'name': 'String', 'kind': 'struct', 'size': 12, 'note': 'String / FileRef: an eastl string',
                   'members': [{'name': n, 'offset': o, 'type': {'kind': 'builtin', 'name': 'char *'}}
                               for n, o in (('begin', 0), ('end', 4), ('capacity', 8))]},
        'Guid': {'name': 'Guid', 'kind': 'struct', 'size': 16, 'note': 'Guid', 'members': []},
    }
    embeds: dict[str, set[str]] = {}
    for name in sorted(recs):
        r = recs[name]
        if r['kind'] == 'enumeration':
            types[name] = {'name': name, 'kind': 'enum', 'size': r['size'], 'module': r['module'],
                           'values': [[v, int(n)] for v, n in r.get('values', [])]}
            continue
        if r['kind'] not in ('class', 'value type'):
            continue
        members = []
        parent = r.get('parent')
        deps: set[str] = set()
        if parent in recs and recs[parent]['size'] <= r['size'] and recs[parent]['kind'] == 'class':
            members.append({'name': 'super_' + parent, 'offset': 0, 'type': {'kind': 'struct', 'name': parent}})
            deps.add(parent)
        for fname, ftype, off in sorted(r.get('fields', []), key=lambda f: f[2]):
            ft = field_type(ftype, recs, arrays)
            if ft['kind'] == 'struct':
                deps.add(ft['name'])
            members.append({'name': fname, 'offset': off, 'type': ft})
        types[name] = {'name': name, 'kind': 'struct', 'size': r['size'], 'module': r['module'], 'members': members,
                       'parent': parent if parent in recs and parent != name else None,
                       'note': f"{r['kind']} {name} ({r['module']}){', parent ' + parent if parent else ''}"}
        embeds[name] = deps
    types.update(arrays)
    # Order: a structure after every structure it embeds (a parent is embedded too).
    order: list[str] = []
    seen: set[str] = set()

    def visit(n: str) -> None:
        if n in seen:
            return
        seen.add(n)
        for d in sorted(embeds.get(n, ())):
            visit(d)
        order.append(n)

    for n in sorted(types):
        visit(n)
    return {'about': 'Ghidra type plan from spec/typeinfo.json (tools/decomp/gen_types.py); facts only.',
            'count': len(order), 'types': [types[n] for n in order]}


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--out', required=True)
    ap.add_argument('--typeinfo', default=str(TYPEINFO))
    a = ap.parse_args(argv)
    recs = json.loads(Path(a.typeinfo).read_text(encoding='utf-8'))['records']
    plan = build(recs)
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    with open(a.out, 'w', encoding='utf-8') as f:
        json.dump(plan, f, indent=1, sort_keys=True)
    kinds: dict[str, int] = {}
    for t in plan['types']:
        kinds[t['kind']] = kinds.get(t['kind'], 0) + 1
    print(f"{plan['count']} types {kinds} -> {a.out}")
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
