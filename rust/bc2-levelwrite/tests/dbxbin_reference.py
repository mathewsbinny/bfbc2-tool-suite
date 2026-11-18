r"""The Python reference of levelwrite's edits (rust\bc2-levelwrite\tests\real_levels.rs): the same edits made with
bfbc2-stockmp-ai's dbxbin, in the calls author.py makes them (set_transform; clone, set_attr('guid'), keep the guid
and type attributes, an Objects item cloned and set_ref; set_text / set_numbers / set_bool / set_ref), so the Rust
writer's bytes can be compared with the reference's.

    python dbxbin_reference.py <base.dbx> <edits.json> <out.dbx>

edits.json: a list of {"kind": "transform", "guid", "rows"}, {"kind": "duplicate", "source", "guid", "rows"},
{"kind": "field", "guid", "field", "value": {"type", "value"}}; rows are four rows of three singles' bits.
"""
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
sys.path.insert(0, os.path.join(SUITE, 'bfbc2-stockmp-ai'))
import dbxbin  # noqa: E402


def single(bits):
    return struct.unpack('>f', struct.pack('>I', bits))[0]


def rows_of(bits):
    return [[single(b) for b in row] for row in bits]


def refers_to(r, guid):
    if r is None:
        return False
    r, g = r.upper(), guid.upper()
    return r == g or r.endswith('/' + g)


def connects(c, guid):
    for f in ('Source', 'Target'):
        x = c.child(f)
        if x is not None and refers_to(x.attr('ref'), guid):
            return True
    return False


def connection_arrays(d):
    out = []
    for inst in d.instances():
        a = inst.child('EventConnections')
        if a is not None and a.kind == dbxbin.CONTAINER:
            out.append(a)
    return out


def duplicate(d, source, guid, rows):
    src = d.instance(source)
    e = d.clone(src, d)
    d.set_attr(e, 'guid', guid.upper())
    e.attrs = [(k, v) for k, v in e.attrs if d.strings[k] in ('guid', 'type')]
    if rows is not None:
        d.set_transform(e.child('Transform'), rows)
    d.partition.children.append(e)
    level = d.of_type('.LevelData')[0]
    objs = level.child('Objects')
    if objs is not None:
        for it in objs.children or ():
            if refers_to(it.attr('ref'), source):
                n = d.clone(it, d)
                d.set_ref(n, guid.lower())
                objs.children.append(n)
                break
    for a in connection_arrays(d):
        found = [c for c in a.children if connects(c, source)]
        for c in found:
            n = d.clone(c, d)
            for f in ('Source', 'Target'):
                x = n.child(f)
                if x is not None and refers_to(x.attr('ref'), source):
                    d.set_ref(x, guid.lower())
            a.children.append(n)


def field(d, guid, path, value):
    e = d.instance(guid)
    for part in path.split('.'):
        e = e.child(part)
    t, v = value['type'], value['value']
    if t == 'text':
        d.set_text(e, v)
    elif t == 'ref':
        d.set_ref(e, v)
    elif t == 'bool':
        d.set_bool(e, bool(v))
    elif t == 'byte':
        d.set_bool(e, int(v))
    elif t == 'int':
        d.set_numbers(e, [int(v)], 'i')
    elif t == 'float':
        d.set_numbers(e, [float(v)], 'f')
    else:
        raise ValueError(f'no reference for {t}')


def main():
    base, job, out = sys.argv[1:4]
    d = dbxbin.parse(open(base, 'rb').read())
    for x in json.load(open(job, encoding='utf-8')):
        if x['kind'] == 'transform':
            d.set_transform(d.instance(x['guid']).child('Transform'), rows_of(x['rows']))
        elif x['kind'] == 'duplicate':
            duplicate(d, x['source'], x['guid'], rows_of(x['rows']))
        elif x['kind'] == 'field':
            field(d, x['guid'], x['field'], x['value'])
        else:
            raise ValueError(x['kind'])
    with open(out, 'wb') as f:
        f.write(dbxbin.write(d))


if __name__ == '__main__':
    main()
