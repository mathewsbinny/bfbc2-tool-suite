r"""Round F2b (rust/BENCHMARKS.md, "F2b"): dbxcatalogue.build in the Rust core against build_reference - every
table key for key and in the same insertion order (the Counters' too), the kept elements by value, and which kept
elements are one object (a prototype inside a kept instance).

  python test_dbxcatalogue_native.py          quick: seeded partitions (random instance types, fields, arrays,
                                              references, numbers whole and float, texts past TEXTS_CAP, types past
                                              PER_TYPE, guids in several partitions) packed into built bundles of
                                              more than one 400-name chunk, with a *deleted* marker and a file that
                                              is no binary .dbx (a problem recorded); 6 seeded games
  python test_dbxcatalogue_native.py --game   + the catalogue of the user's game copy built both ways (84,739
                                              partitions; the reference takes about three minutes)

The module is required (the test fails without it, saying how to build it).
"""
import os
import random
import struct
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import dbxcatalogue  # noqa: E402
import importcore  # noqa: E402
from bfbc2core import dbx, fbrb  # noqa: E402

FAILS: list[str] = []


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def el(e):
    return (e.tag, e.kind, tuple(e.attributes), e.text, e.raw,
            None if e.children is None else tuple(el(c) for c in e.children))


def canon(c):
    """Every table in its order, elements by value."""
    out = {'partitions': c.partitions, 'classes': sorted(c.classes), 'order': list(c.order.items())}
    for k in ('shapes', 'texts', 'numeric'):
        out[k] = [(kk, list(v.items())) for kk, v in getattr(c, k).items()]
    out['protos'] = [(k, el(v)) for k, v in c.protos.items()]
    out['guids'] = list(c.guids.items())
    out['primary'] = list(c.primary.items())
    out['defaults'] = [(k, n, el(v)) for k, (n, v) in c.defaults.items()]
    out['others'] = [(k, n, el(v)) for k, (n, v) in c.others.items()]
    out['exe_names'] = sorted(c.exe_names)
    out['problems'] = c.problems
    return out


def sharing(c):
    """For each prototype, the kept instance it is an element of (None: its own tree)."""
    ids = {}

    def walk(e, where):
        ids.setdefault(id(e), where)
        for ch in e.children or ():
            walk(ch, where)
    for table in ('defaults', 'others'):
        for k, (_, v) in getattr(c, table).items():
            walk(v, (table, k))
    return [(k, ids.get(id(v))) for k, v in c.protos.items()]


def compare(a, b, what):
    ca, cb = canon(a), canon(b)
    bad = [k for k in ca if ca[k] != cb[k]]
    sa, sb = sharing(a), sharing(b)
    check(not bad and sa == sb, f'{what}: every table the same ({a.partitions} partitions, {len(a.shapes)} paths, '
          f'{len(a.guids)} guids, {len(a.protos)} prototypes, {len(a.problems)} problems)',
          f'differ: {bad}{"; sharing" if sa != sb else ""}')


# ------------------------------------------------------------------------------------------------ built partitions

def varint(v):
    out = bytearray()
    dbx.put_varint(out, v)
    return bytes(out)


def field(rnd, name, texts):
    r = rnd.random()
    if r < 0.3:
        v = rnd.choice(texts)
        return dbx.Element('field', dbx.TEXT, [('name', name)], v)
    if r < 0.55:
        n = rnd.choice([1, 1, 3, 16, 0])
        vals = b''.join(struct.pack('>f', rnd.uniform(-9, 9)) if rnd.random() < 0.5 else
                        struct.pack('>i', rnd.randrange(-300, 300)) for _ in range(n))
        size = 4 if rnd.random() < 0.9 else 8
        if size == 8:
            vals = vals + vals
        return dbx.Element('field', dbx.NUMBERS, [('name', name)], '', varint(n) + varint(size) + vals)
    if r < 0.7:
        return dbx.Element('field', 6, [('name', name)], '', bytes([0x60, rnd.randrange(2)]))
    if r < 0.8:
        return dbx.Element('field', dbx.TEXT, [('ref', 'x/y/' + str(rnd.randrange(9))), ('name', name)], 'null')
    if r < 0.9:
        kids = [field(rnd, f'F{i}', texts) for i in range(rnd.randint(0, 3))]
        return dbx.Element('complex', dbx.CONTAINER, [('name', name)], children=kids)
    items = [dbx.Element('item', dbx.TEXT, [], rnd.choice(texts)) if rnd.random() < 0.5 else
             dbx.Element('complex', dbx.CONTAINER, [], children=[field(rnd, 'A', texts)])
             for _ in range(rnd.randint(0, 3))]
    return dbx.Element('array', dbx.CONTAINER, [('name', name)], children=items)


def partition(rnd, types, guids, texts):
    insts = []
    first = None
    for _ in range(rnd.randint(1, 6)):
        g = rnd.choice(guids) if rnd.random() < 0.3 else f'{rnd.getrandbits(64):016X}-G'
        first = first or g
        t = rnd.choice(types)
        kids = [field(rnd, rnd.choice(['Name', 'Value', 'Mode', 'Count', 'Items', 'Data', 'Flag']), texts)
                for _ in range(rnd.randint(0, 6))]
        attrs = [('guid', g), ('type', t)] if rnd.random() < 0.95 else [('type', t)]
        insts.append(dbx.Element('instance', dbx.CONTAINER, attrs, children=kids))
    prim = [('primaryInstance', first)] if rnd.random() < 0.9 else []
    root = dbx.Element('partition', dbx.CONTAINER, [('guid', 'P'), ('exportMode', 'All')] + prim, children=insts)
    return dbx.write(dbx.DbxFile([], [root]))


def game_copy(rnd, folder):
    """Bundles of built partitions and an index over them (meshview.Index's `where`)."""
    types = [f'Ns.T{i}' for i in range(rnd.randint(3, 12))]
    guids = [f'{rnd.getrandbits(64):016X}-S' for _ in range(20)]
    texts = [f'Enum{i}' for i in range(rnd.choice([5, 600]))] + ['', 'true']
    where = {}
    for bi in range(rnd.randint(1, 4)):
        items = []
        for k in range(rnd.choice([3, 30, 450])):
            name = f'Fx/B{bi}/P{k:04d}.dbx' if rnd.random() < 0.9 else f'fx/b{bi}/q{k}.dbx'
            r = rnd.random()
            data = b'' if r < 0.02 else b'<xml/>' if r < 0.04 else partition(rnd, types, guids, texts)
            items.append((name, 'dbx', data))
            where[f'{name.lower()}'] = (os.path.join(folder, f'b{bi}.fbrb'), name)
        items.append(('fx/res.itexture', 'ITexture', b'x'))
        where['fx/res.itexture'] = (os.path.join(folder, f'b{bi}.fbrb'), 'fx/res.itexture')
        with open(os.path.join(folder, f'b{bi}.fbrb'), 'wb') as fh:
            fh.write(fbrb.build(items))
    return where


class Ix:
    def __init__(self, where):
        self.where = where
        self.root = None


def quick():
    rnd = random.Random(3)
    for i in range(6):
        with tempfile.TemporaryDirectory() as d:
            ix = Ix(game_copy(rnd, d))
            exe = os.path.join(d, 'game.exe')
            with open(exe, 'wb') as fh:
                fh.write(b'MZ\0Enum1\0\0Ns\0x\0' + bytes(rnd.randbytes(64)) + b'\0T0\0')
            got = dbxcatalogue.build(ix, exe)
            ref = dbxcatalogue.build_reference(ix, exe)
            compare(got, ref, f'built game {i + 1}')


def game():
    import meshview
    import shaderdb
    try:
        root = shaderdb.find_opfor(None)
    except shaderdb.ShaderDbError as e:
        print(f'SKIP game: {e}')
        return
    ix = meshview.Index(root)
    exe = os.path.join(root, 'game', 'BFBC2Game.exe')
    t = time.perf_counter()
    got = dbxcatalogue.build(ix, exe)
    t1 = time.perf_counter()
    ref = dbxcatalogue.build_reference(ix, exe)
    t2 = time.perf_counter()
    compare(got, ref, f'the game copy (Rust {t1 - t:.0f} s, the reference {t2 - t1:.0f} s)')


def main(argv):
    from bfbc2core import native
    try:
        native.require('the BF1943 importer\'s catalogue (round F2b)', 'catalogue_build')
    except native.NativeMissing as e:
        check(False, 'the Rust core is built from these sources', str(e))
        return 1
    if importcore.native() is None:
        check(False, 'importcore finds the module', 'BFBC2_IMPORTER_PYTHON / BFBC2_PURE_PYTHON set?')
        return 1
    quick()
    if '--game' in argv:
        game()
    if FAILS:
        print(f'{len(FAILS)} FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
