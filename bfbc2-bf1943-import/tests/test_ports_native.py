r"""Round E3g (rust/BENCHMARKS.md, "E3g"): the BF1943 model / physics / effect ports' faster paths.

  - mesh.convert's vertex loop (the Rust core: bc2_native.mesh_vertices) and havok.convert's packfile walk
    (bc2_native.havok_convert_packfile) against results recorded from their retired Python (mesh.py's loop,
    havok.Packfile: retired on 2026-11-18 after the full test tier, rust/BENCHMARKS.md "Retired Python"), in
    ports_golden.json beside this file: each case's output SHA-1 or its exception. Where the Rust core now answers
    otherwise on purpose (an input the Python took or refused in another way, now a ConvertError saying why), the new
    answer is under "now", case by case; every other case must give the recorded answer;
  - meshport.Porter.pc_layout (cached signatures, donor lists, fallback choices) against pc_layout_reference;
  - effectport.templates (the Rust core's emitter index, its disk cache) against the search's rule written out.

  python test_ports_native.py           quick: built inputs only - 400 seeded console meshes (strips, shared and
                                        windowed regions, re-ordered and dropped elements, damaged ones), every value
                                        of the two packed formats; 600 seeded packfiles of random class layouts, each
                                        also cut at random lengths and with random bytes changed; 3,000 layout
                                        searches over a made-up part index; the emitter template search on built
                                        bundles (cold, from disk, after a bundle changed, a damaged document)
  python test_ports_native.py --game    + every mesh set (273) and HavokPhysicsData (202) of the user's BF1943 copy
                                        ported as Project OPFOR ports them (against the recording, and with E3g's
                                        indexes against meshport's reference), every effect (157) through
                                        effectport.port_many (the Rust core), every console MeshData / HavokPhysicsData of
                                        OPFOR's Onslaught builds (work\onslaught\build_*\con_*, read only) against the
                                        recording
  python test_ports_native.py --record [--game]
                                        write the "now" answers from the current code where they differ from the
                                        recording: only after a deliberate change to the conversions, proven on the
                                        game's files (bfbc2-console-convert tests\test_havok.py, test_mesh_decode.py)
                                        - the recording is the only oracle

The module is required: without it (or built from other sources) this test fails, saying how to build it.
"""
import hashlib
import json
import os
import random
import struct
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'mesh-viewer', 'shader-tools', 'console-convert', 'anim-tools', 'core')
import effectport  # noqa: E402
import havok  # noqa: E402
import mesh  # noqa: E402
import meshport  # noqa: E402
from bfbc2core import dbx, fbrb  # noqa: E402

GOLDEN = os.path.join(HERE, 'ports_golden.json')
FAILS: list[str] = []


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def outcome(f, *a, **kw):
    """f's bytes, or its exception as (type name, message)."""
    try:
        return f(*a, **kw)
    except Exception as e:                                       # noqa: BLE001 - compared, not handled
        return (type(e).__name__, str(e))


def sha1(b: bytes) -> str:
    """The first 16 hex digits of b's SHA-1."""
    return hashlib.sha1(b).hexdigest()[:16]


def recorded_form(r) -> list:
    """An outcome as the recording keeps it: ['ok', SHA-1] or ['error', exception type, message]."""
    return ['ok', sha1(r)] if isinstance(r, bytes) else ['error', *r]


# the conversions the recording is compared with
def mesh_convert(b, pairs, **kw):
    return mesh.convert(b, pairs, **kw)


def havok_convert_packfile(b, o, k):
    return havok.convert_packfile(b, o, k)


def havok_convert(b, k):
    return havok.convert(b, k)


# ------------------------------------------------------------------------------------------------ the recording

def load() -> dict:
    try:
        with open(GOLDEN, encoding='utf-8') as f:
            return dict(json.load(f))
    except FileNotFoundError:
        return {}


def write(golden: dict) -> None:
    """ports_golden.json, one case per line (a change shows as the lines of the cases it changed)."""
    lines = ['{']
    keys = sorted(golden)
    for i, k in enumerate(keys):
        v, end = golden[k], (',' if i < len(keys) - 1 else '')
        if isinstance(v, dict) and v:
            rows = [f'{json.dumps(n)}: {json.dumps(v[n])}' for n in sorted(v)]
            lines += [f'{json.dumps(k)}: {{', *[r + ',' for r in rows[:-1]], *rows[-1:], '}' + end]
        else:
            lines.append(f'{json.dumps(k)}: {json.dumps(v)}{end}')
    lines.append('}')
    with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')


GOLD: dict = {}
RECORD = [False]


class Recorded:
    """Each case's recorded answer ("now" over the reference's) against the current one; with --record, the current
    answers that differ from the reference's go to "now"."""

    def __init__(self, group: str):
        self.ref: dict = GOLD.get(group, {})
        self.now: dict = GOLD.setdefault('now', {})
        self.group = group
        self.n = self.same = self.changed = self.converted = 0
        self.diffs: list[str] = []

    def __call__(self, name: str, got: list, input_sha: str | None = None) -> None:
        key = f'{self.group}/{name}'
        ref = self.ref.get(name)
        if ref is None or (input_sha is not None and ref[0] != input_sha):
            self.diffs.append(f'{key}: ' + ('not recorded' if ref is None else 'the input is not the recorded one'))
            return
        ref = ref[1:] if input_sha is not None else ref
        self.n += 1
        self.converted += got[0] == 'ok'
        if RECORD[0]:
            if got != ref:
                self.now[key] = got
            else:
                self.now.pop(key, None)
            return
        want = self.now.get(key, ref)
        self.changed += key in self.now
        if got == want:
            self.same += 1
        else:
            self.diffs.append(f'{key}: {got} != recorded {want}')

    def report(self, what: str, at_least: int = 0) -> None:
        if RECORD[0]:
            print(f'     {what}: {self.n} cases recorded')
            return
        check(not self.diffs and self.n >= at_least, f'{what}: {self.same} of {self.n} as recorded ({self.converted} '
              f'converted; {self.changed} answered as "now" records, not as the Python did)', '; '.join(self.diffs[:5]))


# ------------------------------------------------------------------------------------------------ console meshes

CONSOLE_FORMATS = [0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0C, 0x0D, 0x2B]


def layout(elements, stride):
    """A 0x49-byte layout record body: (format, usage, offset, stream) elements, their count at 64, stride at 69."""
    b = bytearray(0x49)
    for i, e in enumerate(elements):
        b[4 * i:4 * i + 4] = bytes(e)
    b[64], b[69] = len(elements), stride
    return bytes(b)


def console_mesh(parts, vertices, indices, wide):
    """Big-endian MeshData: parts [(name, triangles, vertices, first_index, vertex_offset, stride, k, bones)]."""
    out = bytearray(struct.pack('>I', 7) + b'\x01\x02' + bytes([len(parts)]))
    for name, tri, nv, fi, vo, stride, k, bones in parts:
        out += name + b'\0' + struct.pack('>4I', tri, nv, fi, vo) + bytes([stride, k, 0, 0])
        out += struct.pack('>H%dH' % len(bones), len(bones), *bones)
    for n in range(4):
        out += struct.pack('>I', n) + bytes(range(n))
    ib = struct.pack('>%d%s' % (len(indices), 'I' if wide else 'H'), *indices)
    out += struct.pack('>3I', len(vertices), len(ib), int(wide))
    return bytes(out) + vertices + ib


def random_mesh(rnd):
    """(console bytes, layout pairs, reorder) of a random mesh: 1 - 3 regions, each with a console layout and 1 - 3
    parts drawing it (one or two PC layouts: the converter's shared and windowed regions), some strips; a few with
    a format the converter refuses."""
    regions, vb = rnd.randint(1, 3), bytearray()
    parts: list = []
    pairs: list = []
    idx: list[int] = []
    reorder = rnd.choice([False, True, 'drop'])
    for _ in range(regions):
        els, off = [], 0
        for _ in range(rnd.randint(1, 6)):
            f = rnd.choice(CONSOLE_FORMATS + ([0x01] if rnd.random() < 0.02 else []))
            els.append((f, rnd.randint(0, 6), off, 0 if rnd.random() < 0.9 else 1))
            off += mesh.SIZE.get(f, 4)
        sc = off + rnd.choice([0, 0, 4])
        n = rnd.randint(1, 40)
        start = len(vb)
        vb += bytes(rnd.getrandbits(8) for _ in range(n * sc))
        lc = layout(els, sc)

        def pc_layout(els=els):
            keep = list(els)
            if reorder == 'drop' and len(keep) > 1 and rnd.random() < 0.5:
                keep.pop(rnd.randrange(len(keep)))
            if reorder:
                rnd.shuffle(keep)
            pe, o = [], 0
            for f, u, _, s in keep:
                fp = mesh.PC_FORMAT.get(f, f)
                pe.append((fp, u, o, s))
                o += mesh.SIZE.get(fp, 4)
            return layout(pe, o + rnd.choice([0, 0, 8]))
        lps = [pc_layout() for _ in range(rnd.randint(1, 2))]
        for _ in range(rnd.randint(1, 3)):
            lo = rnd.randrange(n)
            nv = rnd.randint(1, n - lo)
            k = 5 if rnd.random() < 0.25 else 3
            tri = rnd.randint(1, 6)
            first = len(idx)
            idx += [lo + rnd.randrange(nv) for _ in range(tri + 2 if k == 5 else 3 * tri)]
            parts.append((b'part%d' % len(parts), tri, nv, first, start, sc, k, [rnd.randrange(60) for _ in range(3)]))
            pairs.append((lc, rnd.choice(lps)))
    return console_mesh(parts, bytes(vb), idx, rnd.random() < 0.2), pairs, reorder


def packed_values():
    """A mesh whose vertices hold every value of the two packed formats: the 11:11:10 normal's x, y (2,048 each)
    and z (1,024), and the four colour bytes (256)."""
    els = [(0x2B, 4, 0, 0), (0x0D, 0, 4, 0)]
    vb = bytearray()
    for i in range(2048):
        v = i | (((i * 7) % 2048) << 11) | ((i % 1024) << 22)
        vb += struct.pack('>I', v) + bytes([i % 256, (i + 85) % 256, (i * 3) % 256, 255 - i % 256])
    pe = [(0x08, 4, 0, 0), (0x08, 0, 8, 0)]
    b = console_mesh([(b'all', 1, 2048, 0, 0, 8, 3, [])], bytes(vb), [0, 1, 2047], False)
    return b, [(layout(els, 8), layout(pe, 16))]


def quick_mesh():
    rnd = random.Random(31)
    rec = Recorded('mesh-quick')
    for i in range(400):
        b, pairs, reorder = random_mesh(rnd)
        rec(f'mesh {i}', recorded_form(outcome(mesh_convert, b, pairs, reorder=reorder)))
    b, pairs = packed_values()
    rec('every packed value', recorded_form(outcome(mesh_convert, b, pairs)))
    rec.report('console meshes (400 seeded, every value of the packed normal and colour bytes)', 401)
    # the Rust loop converts the packed mesh itself
    m = mesh.parse(b, True)
    direct = mesh.require().mesh_vertices(b, m['data'], 8, 16, 0, 2048, [(0x2B, 0, 0x08, 0), (0x0D, 4, 0x08, 8)], False)
    check(direct is not None and direct in mesh.convert(b, pairs), 'mesh_vertices converts the packed mesh itself')
    # what the loop refuses says why: a format it does not know, vertices past the data
    lc = layout([(0x01, 0, 0, 0)], 4)
    r = outcome(mesh.convert, console_mesh([(b'p', 1, 3, 0, 0, 4, 3, [])], bytes(12), [0, 1, 2], False), [(lc, lc)])
    check(isinstance(r, tuple) and r[0] == 'ConvertError' and 'is not converted' in r[1], f'format 0x01: {r}')
    lc = layout([(0x04, 0, 8, 0)], 12)                          # a 16-byte element at 8 of a 12-byte vertex
    r = outcome(mesh.convert, console_mesh([(b'p', 1, 3, 0, 0, 12, 3, [])], bytes(36), [0, 1, 2], False), [(lc, lc)])
    check(isinstance(r, tuple) and r[0] == 'ConvertError' and 'past' in r[1], f'an element past the vertex: {r}')


# ------------------------------------------------------------------------------------------------ packfiles

UINT8, INT16, UINT16, INT32, UINT32, REAL, VECTOR4, MATRIX3 = 4, 5, 6, 7, 8, 11, 12, 14
PLAIN_TYPES = [havok.BOOL, UINT8, INT16, UINT16, INT32, UINT32, REAL, havok.INT64, VECTOR4, MATRIX3, havok.HALF,
               havok.CSTRING, havok.ULONG]


def pad16(b, v):
    while len(b) % 16:
        b.append(v)


def packfile(classnames, content, local, glob, objects):
    """A big-endian Havok 5.5.0 packfile (the Xbox 360's layout rules) behind a 16-byte stand-in wrapper offset."""
    cn, at = bytearray(), {}
    for c in classnames:
        cn += struct.pack('>I', 0x1234_5678) + b'\x09'
        at[c] = len(cn)
        cn += c.encode('latin-1') + b'\0'
    pad16(cn, 0xFF)
    data = bytearray(content)
    pad16(data, 0)
    loc = len(data)
    for s, t in local:
        data += struct.pack('>2I', s, t)
    pad16(data, 0xFF)
    gl = len(data)
    for s, sec, t in glob:
        data += struct.pack('>3I', s, sec, t)
    pad16(data, 0xFF)
    virt = len(data)
    for o, c in objects:
        data += struct.pack('>3I', o, 0, at[c])
    pad16(data, 0xFF)
    end = len(data)
    head = bytearray(havok.MAGIC + struct.pack('>2I', 0, 5) + b'\x04\x00\x00\x01' + struct.pack('>5I', 3, 2, 0, 0, 0))
    head += b'Havok-5.5.0-r1\0\xff' + bytes(8)
    cs, ds = 64 + 48 * 3, 64 + 48 * 3 + len(cn)
    for tag, rel in ((b'__classnames__', (cs, len(cn), len(cn), len(cn), len(cn), len(cn), len(cn))),
                     (b'__types__', (ds, 0, 0, 0, 0, 0, 0)), (b'__data__', (ds, loc, gl, virt, end, end, end))):
        head += tag + bytes(19 - len(tag)) + b'\xff' + struct.pack('>7I', *rel)
    return bytes(head + cn + data)


def random_classes(rnd):
    """A dict of random class layouts (havok.Klass): plain members, enums, structs, pointers, arrays, C arrays,
    variants, a parent; now and then a member type the converter refuses or a class the dict lacks."""
    names = [f'hkC{i}' for i in range(rnd.randint(2, 6))]
    out = {}
    for i, n in enumerate(names):
        mem, off = [], 0
        for j in range(rnd.randint(0, 5)):
            r = rnd.random()
            cls, sub, ca = None, 0, 0
            if r < 0.45:
                ty = rnd.choice(PLAIN_TYPES)
                size = havok.PLAIN[ty][0]
                ca = rnd.choice([0, 0, 0, 2, 3])
                size *= max(ca, 1)
            elif r < 0.55:
                ty, sub = rnd.choice([havok.ENUM, havok.FLAGS]), rnd.choice([UINT8, UINT16, UINT32])
                size = havok.PLAIN[sub][0]
            elif r < 0.65 and i + 1 < len(names):
                ty, cls = havok.STRUCT, rnd.choice(names[i + 1:])
                size = 64
            elif r < 0.8:
                ty, sub, cls = havok.POINTER, havok.STRUCT, rnd.choice(names + ['hkMissing'] * (rnd.random() < 0.05))
                size = 4
            elif r < 0.95:
                ty = rnd.choice([havok.ARRAY, havok.SIMPLEARRAY])
                sub = rnd.choice([UINT16, REAL, VECTOR4, havok.POINTER, havok.STRUCT, havok.VOID])
                cls = rnd.choice(names[i + 1:] or names) if sub in (havok.POINTER, havok.STRUCT) else None
                size = 12 if ty == havok.ARRAY else 8
            else:
                ty = rnd.choice([havok.VARIANT, havok.ZERO, havok.HOMOGENEOUSARRAY, 99])
                sub = UINT32 if ty == havok.ZERO else 0
                size = 8
            off = (off + 3) // 4 * 4
            mem.append(havok.Member(f'm{j}', cls, ty, sub, ca, 0, off))
            off += size
        parent = rnd.choice(names[i + 1:]) if i + 1 < len(names) and rnd.random() < 0.3 else None
        out[n] = havok.Klass(n, parent, max(64, (off + 15) // 16 * 16), mem)
    return out


def random_packfile(rnd, classes):
    """A packfile of 1 - 4 objects of the classes, with array data and pointer fixups into the objects."""
    names = list(classes)
    objs, at = [], 0
    for _ in range(rnd.randint(1, 4)):
        c = rnd.choice(names + (['hkUnknown'] if rnd.random() < 0.03 else []))
        objs.append((at, c))
        at += classes[c].size if c in classes else 64
    data_at = at
    # words below 0xF10: a count read from anywhere stays small (the reference walks an array of 2^32 elements past
    # the data without an error - for hours)
    content = bytearray(b''.join(struct.pack('>I', rnd.randrange(16) << (8 * rnd.choice([0, 0, 0, 1])))
                                 for _ in range((at + 256) // 4)))
    local = []
    for o, c in objs:
        k = classes.get(c)
        while k is not None:
            for m in k.members:
                a = o + m.offset
                if m.type in (havok.ARRAY, havok.SIMPLEARRAY):
                    count = rnd.choice([0, 1, 2, 3])
                    struct.pack_into('>I', content, a + 4, count)
                    if count and rnd.random() < 0.95:
                        local.append((a, data_at if m.subtype != havok.STRUCT else rnd.choice(objs)[0]))
                elif m.type == havok.POINTER and rnd.random() < 0.8:
                    local.append((a, rnd.choice(objs)[0]))
            k = classes.get(k.parent) if k.parent else None
    for i in range(0, 64, 4):                                     # array pointers: into the objects
        if rnd.random() < 0.5:
            local.append((data_at + i, rnd.choice(objs)[0]))
    local = list(dict(local).items())
    names_used = sorted({c for _, c in objs})
    glob = [(s, 2, t) for s, t in local[:1]] if local and rnd.random() < 0.3 else []
    return packfile(names_used, content, local[len(glob):], glob, objs)


def quick_havok():
    rnd = random.Random(7)
    rec = Recorded('havok-quick')
    for i in range(600):
        k = random_classes(rnd)
        b = bytes(16) + random_packfile(rnd, k)
        variants = [b]
        variants += [b[:rnd.randrange(16, len(b))] for _ in range(2)]
        for _ in range(3):
            m = bytearray(b)
            for _ in range(rnd.randint(1, 4)):                   # the low half of a word (counts stay < 65,536)
                j = rnd.randrange(16, len(m))
                m[j | 2 if j % 4 < 2 else j] = rnd.getrandbits(8)
            variants.append(bytes(m))
        for j, v in enumerate(variants):
            rec(f'packfile {i}.{j}', recorded_form(outcome(havok_convert_packfile, v, 16, k)))
    rec.report('packfiles (600 seeded of random class layouts; built, cut short, bytes changed)', 3600)


# ------------------------------------------------------------------------------------------------ meshport layouts

class _Ix:
    def __init__(self, parts):
        self.parts = parts


def quick_layouts():
    rnd = random.Random(5)
    pool = []
    for _ in range(14):                                          # element sets, few enough to collide
        els, off = [], 0
        for _ in range(rnd.randint(1, 5)):
            f = rnd.choice([0x03, 0x06, 0x08, 0x0C, 0x2B])
            els.append((f, rnd.choice([0, 0, 1, 1, 4, 5, 2, 3]), off, 0))
            off += mesh.SIZE[f]
        pool.append(layout(els, off))
    meshes = [f'objects/m{i}/m{i}_mesh' for i in range(30)]
    parts = {}
    for m in meshes:
        for lod in range(rnd.randint(1, 3)):
            for p in range(rnd.randint(1, 4)):
                kind = rnd.choice(['Glass', 'Body', 'Metal', 'x'])
                tech = 'standard' if rnd.random() < 0.85 else 'zonly'
                parts[f'{m}_lod{lod}/part{p}_{kind}/{tech}'.lower()] = rnd.choice(pool[:10])
    ix = _Ix(parts)
    fast, ref = meshport.Porter(None, ix), meshport.Porter(None, ix)
    same = n = errs = 0
    for _ in range(3000):
        m = rnd.choice(meshes + ['objects/new/new_mesh'])
        lod, part = rnd.randint(0, 3), f'part{rnd.randint(0, 4)}_{rnd.choice(["Glass", "Body", "Metal", "x", "y"])}'
        bf = rnd.choice(pool)
        donors = rnd.sample(meshes + ['objects/none/none_mesh'], rnd.randint(0, 3))
        meshport.REFERENCE = True
        a = outcome(ref.pc_layout, m, lod, part, bf, donors)
        meshport.REFERENCE = False
        b = outcome(fast.pc_layout, m, lod, part, bf, donors)
        n += 1
        same += a == b
        errs += isinstance(a, tuple) and a[0] == 'PortError'
    check(same == n, f'pc_layout: {same} of {n} searches as the reference ({errs} PortErrors)')


# ------------------------------------------------------------------------------------------------ emitter templates

def _doc(guid, types):
    inst = [dbx.Element('instance', dbx.CONTAINER, [('guid', f'{guid}-0000-0000-0000-00000000000{i + 1}'), ('type', t)],
                        children=[dbx.Element('field', 6, [('name', 'Enable')], raw=bytes([1, 1]))])
            for i, t in enumerate(types)]
    part = dbx.Element('partition', dbx.CONTAINER, [('guid', f'{guid}-0000-0000-0000-000000000000'),
                                                    ('primaryInstance', inst[0].attributes[0][1]), ('exportMode', 'All')],
                       children=inst)
    return dbx.write(dbx.DbxFile([''], [part]))


class _BundleIx:
    """A stand-in meshview.Index over bundles in a folder: where, read, read_many."""

    def __init__(self, folder, bundles):
        self.where = {}
        for bname, items in bundles.items():
            path = os.path.join(folder, bname)
            with open(path, 'wb') as fh:
                fh.write(fbrb.build([(n, 'dbx', d) for n, d in items]))
            for n, _ in items:
                self.where[n.lower()] = (path, n)

    def read(self, name):
        hit = self.where.get(name.lower())
        return None if hit is None else fbrb.read(hit[0], [hit[1]])[hit[1]]


def greedy(ix, need):
    """The search's rule written out (the oracle since round OR8 retired effectport's Python): TEMPLATES first, then
    each emitter document in name order that names a needed type in its bytes and holds it joins the pool, until no
    type is needed; (pool, the types still needed, sorted), or ('ValueError',) when a document it tests does not
    parse."""
    pool = list(effectport.TEMPLATES)
    have = {i.attr('type') for i in dbx.parse(ix.read(pool[0] + '.dbx')).instances}
    need = set(need) - have
    for n in sorted(k for k in ix.where if '/emitters/' in k and k.endswith('.dbx')):
        if not need:
            break
        b = ix.read(n)
        if not any(t.encode() in b for t in need):
            continue
        try:
            got = {i.attr('type') for i in dbx.parse(b).instances} & need
        except ValueError:
            return ('ValueError',)
        if got:
            pool.append(n[:-4])
            need -= got
    return pool, sorted(need)


def quick_templates():
    rnd = random.Random(3)
    types = [f'Emitter.T{i}Data' for i in range(40)]
    m = effectport._rust()
    rules = effectport._tables(None)[0]
    with tempfile.TemporaryDirectory() as tmp:
        os.environ['BFBC2_EFFECTPORT_CACHE'] = os.path.join(tmp, 'cache')
        try:
            bundles = {'b0.fbrb': [(effectport.TEMPLATES[0] + '.dbx', _doc('10000000', ['Emitter.EmitterEntityData',
                                                                                    'Entity.EffectEntityData']))]}
            for b in range(1, 6):
                bundles[f'b{b}.fbrb'] = [(f'fx/emitters/e{b}_{i:03d}.dbx',
                                          _doc(f'{b}{i:07d}', rnd.sample(types[:39], rnd.randint(1, 3))))
                                         for i in range(60)]
            # a damaged document naming a type: its bytes are tested, then it does not parse
            bundles['b6.fbrb'] = [('fx/emitters/zz_damaged.dbx', b'not a dbx ' + types[39].encode())]
            ix = _BundleIx(tmp, bundles)

            def rust(need):
                try:
                    return effectport.templates(ix, need)
                except ValueError:
                    return ('ValueError',)
            n = same = 0
            for k in range(60):
                need = set(rnd.sample(types[:39], rnd.randint(1, 8)))
                need |= {'Emitter.EmitterEntityData'} if k % 5 == 0 else set()
                need |= {'Emitter.NoDocumentData'} if k % 7 == 0 else set()     # a full scan: the index written
                for attempt in range(2):                         # cold (or from memory), then from disk (a new index)
                    m.effectport_forget()
                    if attempt == 0 and k == 0:
                        assert not os.path.isdir(os.environ['BFBC2_EFFECTPORT_CACHE'])
                    n += 1
                    same += greedy(ix, need) == rust(need)
            check(same == n, f'emitter templates: {same} of {n} searches as the rule (cold, then from disk)')
            m.effectport_forget()
            _, _, (docs, read, from_disk) = m.effectport_templates(ix, ['Emitter.NoDocumentData'], rules)
            check(from_disk and read == 0 and docs == 301,
                  f'the index read back from disk ({docs} documents, {read} batches read)')
            damaged = {types[39]}
            check(greedy(ix, damaged) == rust(damaged) == ('ValueError',),
                  'a damaged document naming a needed type: a ValueError')
            # a bundle changed: its key changes, the index is read again
            with open(os.path.join(tmp, 'b3.fbrb'), 'wb') as fh:
                fh.write(fbrb.build([(n2, 'dbx', _doc('99999999', [types[0]])) for n2, _ in bundles['b3.fbrb']]))
            need = {types[0], types[5]}
            m.effectport_forget()
            _, _, (_, read, from_disk) = m.effectport_templates(ix, sorted(need), rules)
            check(greedy(ix, need) == rust(need) and not from_disk and read > 0,
                  'a changed bundle: the index read again, the search as the rule')
        finally:
            m.effectport_forget()
            del os.environ['BFBC2_EFFECTPORT_CACHE']


# ------------------------------------------------------------------------------------------------ game data

def game():
    import bf1943import as bi
    import meshview
    import shaderdb
    try:
        root = shaderdb.find_opfor(None)
        src = bi.Source(bi.default_source())
        ix = meshview.Index(root)
    except Exception as e:                                       # noqa: BLE001 - only missing game data skips
        print(f'SKIP game part: {e}')
        return
    sys.path.insert(0, os.path.join(SUITE, 'rust'))
    import bench_ports as bp
    exe = os.path.join(root, 'game', 'BFBC2Game.exe')
    runs = {}
    for side in ('reference', 'rust'):
        set_side(side == 'rust')
        out: dict[str, str] = {}
        t = time.perf_counter()
        bp.run_mesh(src, ix, bp.mesh_names(src), out)
        bp.run_havok(src, ix, bp.havok_names(src), out, exe)
        runs[side] = out
        print(f'     {side}: {len(out)} outputs in {time.perf_counter() - t:.1f} s', flush=True)
    a, b = runs['reference'], runs['rust']
    for kind in ('mesh', 'havok'):
        keys = sorted(k for k in set(a) | set(b) if k.startswith(kind + ':'))
        same = sum(a.get(k) == b.get(k) for k in keys)
        bad = [k for k in keys if a.get(k) != b.get(k)]
        errs = sum(1 for k in keys if str(a.get(k)).startswith('error'))
        check(same == len(keys) and len(keys) > 100, f'BF1943 {kind} ports: {same} of {len(keys)} outputs identical '
              f'with E3g\'s indexes and with meshport\'s reference search ({errs} errors on both '
              'sides)', ', '.join(bad[:3]))
    # the mesh and physics ports' outputs against the recording (made with the retired Python conversions)
    for kind in ('mesh', 'havok'):
        rec = Recorded(f'bf1943-{kind}')
        for run in (a, b):
            for k in sorted(x for x in run if x.startswith(kind + ':')):
                v = run[k]
                rec(k[len(kind) + 1:], ['error', v[len('error '):]] if v.startswith('error ') else ['ok', v])
        rec.report(f'BF1943 {kind} ports (both runs)', 200)
    # every effect through the Rust effect port (its Python retired in round OR8, proven then: rust/BENCHMARKS.md
    # "OR8"): no effect fails
    out = {}
    bp.run_effects(src, ix, bp.effect_names(src), out)
    check(len(out) > 100, f'BF1943 effects: {len(out) - 1} files ported')
    onslaught(exe)


def set_side(rust: bool) -> None:
    """The ports' searches: E3g's indexes (rust) or meshport's / effectport's reference searches."""
    meshport.REFERENCE = not rust


def onslaught(exe):
    """Every console MeshData (each part in its console layout, the PC layout made of the same elements) and
    HavokPhysicsData of OPFOR's Onslaught builds, converted, against the recording."""
    import glob

    import convert
    from bfbc2core import paths
    opfor = paths.sibling(paths.OPFOR_DIR, env=paths.OPFOR_ENV, suite=SUITE)
    folders = sorted(os.path.join(opfor, 'work', 'onslaught', d, c) for d in os.listdir(os.path.join(opfor, 'work', 'onslaught'))
                     if d.startswith('build_') for c in os.listdir(os.path.join(opfor, 'work', 'onslaught', d))
                     if c.startswith('con_')) if os.path.isdir(os.path.join(opfor, 'work', 'onslaught')) else []
    if not folders:
        print('SKIP Onslaught part: no console builds in OPFOR\'s work\\onslaught')
        return
    layouts: dict[str, bytes] = {}
    meshes: dict[str, str] = {}
    physics: dict[str, str] = {}
    for f in folders:
        for p in sorted(glob.glob(os.path.join(f, 'levels', '*', 'shaders', 'single.res'))):
            for n, rec in mesh.console_layouts(open(p, 'rb').read()).items():
                layouts.setdefault(n, rec)
        for rel, p in convert.manifest(f, 'MeshData').items():
            meshes.setdefault(rel, p)
        for rel, p in convert.manifest(f, 'HavokPhysicsData').items():
            physics.setdefault(rel, p)
    k = havok.classes(exe)
    rec = Recorded('onslaught-havok')
    for rel, p in sorted(physics.items()):
        b = open(p, 'rb').read()
        rec(rel, recorded_form(outcome(havok_convert, b, k)), sha1(b))
    rec.report('Onslaught console physics', 300)

    def pc_of(lc):
        els, _ = mesh.elements(lc)
        pe, o = [], 0
        for f, u, _, s in els:
            fp = mesh.PC_FORMAT.get(f, f)
            pe.append((fp, u, o, s))
            o += mesh.SIZE.get(fp, 4)
        return layout(pe, o)
    skipped = 0
    rec = Recorded('onslaught-mesh')
    for rel, p in sorted(meshes.items()):
        b = open(p, 'rb').read()
        try:
            m = mesh.parse(b, True)
        except mesh.ConvertError:
            skipped += 1
            continue
        name = rel[:-len('_data.res')] if rel.endswith('_data.res') else rel[:-4]
        lcs = [layouts.get(f'{name}/{q["name"].decode("latin-1")}/standard'.lower()) for q in m['parts']]
        if any(x is None for x in lcs):
            skipped += 1
            continue
        pairs = [(lc, pc_of(lc)) for lc in lcs]
        rec(rel, recorded_form(outcome(mesh_convert, b, pairs)), sha1(b))
    rec.report(f'Onslaught console meshes ({skipped} without layouts in the console databases)', 200)


def main(argv):
    mesh.require()
    havok.require()
    GOLD.update(load())
    RECORD[0] = '--record' in argv
    t = time.perf_counter()
    quick_mesh()
    quick_havok()
    quick_layouts()
    quick_templates()
    print(f'     quick part: {time.perf_counter() - t:.1f} s')
    if '--game' in argv:
        game()
    if RECORD[0] and not FAILS:
        GOLD['now_recorded'] = time.strftime('%Y-%m-%d')
        write(GOLD)
        print(f'wrote {GOLDEN}')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
