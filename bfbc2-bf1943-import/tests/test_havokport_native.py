r"""Round F2b (rust/BENCHMARKS.md, "F2b"): mopp.py and havokport.py in the Rust core against their Python (the
*_reference functions), output for output - bytes, values, or the same exception with the same message. A refusal
(the core leaves a case to the Python) is counted; the answer is the reference's either way.

  python test_havokport_native.py          quick: built inputs only - 2,000 seeded MOPP codes (every opcode, jumps,
                                           rescales, truncated codes) queried with boxes, completeness and rays;
                                           1,500 seeded 4.6.1 packfiles restructured (rigid bodies, MOPP shapes,
                                           deactivators, class-name strings, unknown classes, damaged copies) with
                                           and without the hkUFloat8 table; 1,500 seeded PC resources (wrappers with
                                           and without a material table in either byte order, shape subparts in
                                           either field order with convex children, storage meshes, HavokMaterials
                                           lists) through every reader and writer, also cut short and with bytes
                                           changed
  python test_havokport_native.py --game   + every HavokPhysicsData of the user's BF1943 copy (202) converted as
                                           Project OPFOR converts them (its two templates, with and without the
                                           table): restructure, the swap and the subparts both ways; every physics
                                           resource of BFBC2 (objects' *_havok.res, the levels' *_havokasset.res)
                                           through the materials' readers, the subparts step with its w lanes zeroed,
                                           with_havok_materials grown by two and with_face_materials rewritten; the
                                           MOPP codes of the converted and DICE's files queried both ways

The module is required (the test fails without it, saying how to build it).
"""
import os
import random
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'console-convert', 'core')
import havok  # noqa: E402
import havokport as hp  # noqa: E402
import importcore  # noqa: E402
import mopp  # noqa: E402

FAILS: list[str] = []
REFUSED = [0]                     # importcore.call's refusals (None) so far
_call = importcore.call


def _counting_call(name, *a, **kw):
    r = _call(name, *a, **kw)
    if r is None:
        REFUSED[0] += 1
    return r


importcore.call = _counting_call
COUNTS = {'same': 0, 'differ': 0, 'refused': 0}


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def outcome(f, *a):
    try:
        return ('ok', f(*a))
    except Exception as e:                                       # noqa: BLE001 - compared: both must agree
        return ('err', type(e).__name__, str(e))


class Pair:
    """Runs a function and its reference, counts agreement; remembers the first differences."""

    def __init__(self, label):
        self.label, self.n, self.bad, self.refused = label, 0, [], 0

    def __call__(self, rust_name, f, ref, *a):
        self.n += 1
        before = REFUSED[0]
        got = outcome(f, *a)
        self.refused += REFUSED[0] > before
        want = outcome(ref, *a)
        if got != want:
            self.bad.append((a[:1], got, want))
            COUNTS['differ'] += 1
        else:
            COUNTS['same'] += 1
        return got

    def report(self, what):
        check(self.n > 0 and not self.bad, f'{self.label}: {what} - {self.n - len(self.bad)} of {self.n} the same '
              f'({self.refused} left to the Python)', '; '.join(repr(b)[:300] for b in self.bad[:2]))
        COUNTS['refused'] += self.refused


# ------------------------------------------------------------------------------------------------ MOPP

def random_code(rnd):
    """A MOPP byte code of every opcode kind, mostly well formed, sometimes cut or with a stray byte."""
    out = bytearray()

    def node(depth):
        r = rnd.random()
        if depth > 5 or r < 0.25:
            if rnd.random() < 0.5:
                out.append(0x30 + rnd.randrange(32))
            else:
                w = rnd.randint(1, 4)
                out.append(0x4F + w)
                out.extend(rnd.randbytes(w))
            return
        if r < 0.35:
            out.extend([rnd.randint(1, 4), rnd.randrange(256), rnd.randrange(256), rnd.randrange(256)])
            node(depth + 1)
        elif r < 0.45:
            w = rnd.choice([1, 2])
            out.append(0x08 + w)
            out.extend(rnd.randbytes(w))
            node(depth + 1)
        elif r < 0.5:
            out.append(0x0B)
            out.extend(struct.pack('>I', rnd.randrange(1 << 20)))
            node(depth + 1)
        elif r < 0.6:
            a = rnd.randrange(3)
            lo = rnd.randrange(256)
            out.extend([0x26 + a, lo, rnd.randint(lo, 255)])
            node(depth + 1)
        elif r < 0.8:
            c = rnd.choice([0x10 + rnd.randrange(13), 0x20 + rnd.randrange(3)])
            at = len(out)
            out.extend([c, rnd.randrange(256), rnd.randrange(256), 0] if c < 0x20 else [c, rnd.randrange(256), 0])
            node(depth + 1)
            j = len(out) - (at + (4 if c < 0x20 else 3))
            if j < 256:
                out[at + (3 if c < 0x20 else 2)] = j
            node(depth + 1)
        elif r < 0.9:
            c = 0x23 + rnd.randrange(3)
            at = len(out)
            out.extend([c, rnd.randrange(256), rnd.randrange(256), 0, 0, 0, 0])
            node(depth + 1)
            j2 = len(out) - (at + 7)
            node(depth + 1)
            struct.pack_into('>HH', out, at + 3, 0, min(j2, 0xFFFF))
        else:
            w = rnd.randint(1, 4)
            out.append(0x04 + w)
            out.extend(bytes(w))
            node(depth + 1)
    node(0)
    r = rnd.random()
    if r < 0.08:
        del out[rnd.randrange(len(out)):]
    elif r < 0.14:
        out[rnd.randrange(len(out))] = rnd.randrange(256)
    return bytes(out)


def quick_mopp():
    rnd = random.Random(11)
    q, qb, cp, rc, rb = (Pair(f'MOPP {w}') for w in ('query_int', 'query', 'complete', 'ray_cast', 'ray_cast_brute'))
    for _ in range(2000):
        code = random_code(rnd)
        lo = [rnd.randrange(-(1 << 17), 1 << 24) for _ in range(3)]
        hi = [v + rnd.randrange(1 << rnd.randint(0, 22)) for v in lo]
        q('mopp_query', mopp.query_int, mopp.query_int_reference, code, lo, hi)
        info = [rnd.uniform(-50, 0) for _ in range(3)] + [rnd.choice([1024.0, 4096.0, 65536.0, 1.7e5])]
        wlo = [rnd.uniform(-60, 60) for _ in range(3)]
        whi = [v + rnd.uniform(0, 30) for v in wlo]
        qb('mopp_query_box', mopp.query, mopp.query_reference, code, info, wlo, whi)
        tris = {}
        for k in range(rnd.randint(1, 12)):
            a = [rnd.uniform(-5, 5) for _ in range(3)]
            tris[rnd.choice([k, k + 0x30, rnd.randrange(1 << 20)])] = [a, [x + rnd.uniform(-2, 2) for x in a],
                                                                      [x + rnd.uniform(-2, 2) for x in a]]
        boxes = {k: ([min(p[i] for p in t) for i in range(3)], [max(p[i] for p in t) for i in range(3)])
                 for k, t in tris.items()}
        cp('mopp_complete', mopp.complete, mopp.complete_reference, code, info, boxes)
        o = [rnd.uniform(-8, 8) for _ in range(3)]
        if rnd.random() < 0.05:
            import numpy as np
            o = list(np.array(o))
        d = [rnd.uniform(-1, 1) for _ in range(3)]
        n = sum(x * x for x in d) ** 0.5 or 1.0
        d = [x / n for x in d]
        rc('mopp_ray_cast', mopp.ray_cast, mopp.ray_cast_reference, code, info, tris, o, d, rnd.uniform(0.1, 20))
        rb('mopp_ray_cast_brute', mopp.ray_cast_brute, mopp.ray_cast_brute_reference, tris, o, d, rnd.uniform(0.1, 20))
    for p, w in ((q, 'quantized boxes'), (qb, 'world boxes'), (cp, 'completeness'), (rc, 'rays'),
                 (rb, 'brute-force rays')):
        p.report(f'2,000 seeded codes, {w}')


# ------------------------------------------------------------------------------------------------ built packfiles

def pad16(b, v):
    while len(b) % 16:
        b.append(v)


def packfile(big, version, classnames, content, local, glob, virt, sig=0x1234_5678):
    """A packfile: the class names (signature, 0x09, name), the data section's content and its fixup tables."""
    e = '>' if big else '<'
    cn, at = bytearray(), {}
    for c in classnames:
        cn += struct.pack(e + 'I', sig) + b'\x09'
        at[c] = len(cn)
        cn += c.encode('latin-1') + b'\0'
    pad16(cn, 0xFF)
    data = bytearray(content)
    pad16(data, 0)
    loc = len(data)
    for s, t in local:
        data += struct.pack(e + '2I', s, t)
    pad16(data, 0xFF)
    gl = len(data)
    for s, sec, t in glob:
        data += struct.pack(e + '3I', s, sec, t)
    pad16(data, 0xFF)
    vt = len(data)
    for o, c in virt:
        data += struct.pack(e + '3I', o, 0, at.get(c, 0))
    pad16(data, 0xFF)
    end = len(data)
    root = at.get(classnames[0], 0) if classnames else 0
    head = bytearray(havok.MAGIC + struct.pack(e + '2I', 0, 4 if version.startswith(b'Havok-4') else 5) +
                     b'\x04\x00\x00\x01' + struct.pack(e + '5I', 3, 2, 0, 0, root))
    head += (version + b'\0\xff' + bytes(16))[:16] + bytes(8)
    cs, ds = 64 + 48 * 3, 64 + 48 * 3 + len(cn)
    for tag, rel in ((b'__classnames__', (cs, len(cn), len(cn), len(cn), len(cn), len(cn), len(cn))),
                     (b'__types__', (ds, 0, 0, 0, 0, 0, 0)), (b'__data__', (ds, loc, gl, vt, end, end, end))):
        head += tag + bytes(19 - len(tag)) + b'\xff' + struct.pack(e + '7I', *rel)
    return bytes(head + cn + data), at


SIZES = {'hkRigidBody': 512, 'hkMoppBvTreeShape': 64, 'hkMoppCode': 48, 'hkSpatialRigidBodyDeactivator': 32,
         'hkListShape': 96, 'hkStorageExtendedMeshShape': 240, 'hkExtendedMeshShape': 192, 'hkBoxShape': 48,
         'hkRootLevelContainer': 32, 'HavokMaterial': 16, 'hkWeird': 32}


def fake_classes():
    """5.5.0 layouts with the classes restructure asks (havok.Klass), aabbCenter where the shapes keep it."""
    K = {}
    for n in ('hkpRigidBody', 'hkpMoppBvTreeShape', 'hkpMoppCode', 'hkpBoxShape', 'hkRootLevelContainer'):
        K[n] = havok.Klass(n, None, 64, [])
    K['hkpShapeCollection'] = havok.Klass('hkpShapeCollection', None, 32, [havok.Member('aabbCenter', None, 0, 0, 0, 0,
                                                                                         48)])
    K['hkpListShape'] = havok.Klass('hkpListShape', 'hkpShapeCollection', 96, [])
    K['hkpExtendedMeshShape'] = havok.Klass('hkpExtendedMeshShape', None, 192,
                                            [havok.Member('aabbCenter', None, 0, 0, 0, 0, 64)])
    K['hkpStorageExtendedMeshShape'] = havok.Klass('hkpStorageExtendedMeshShape', 'hkpExtendedMeshShape', 240, [])
    return K


def random_461(rnd, template_names):
    """A 4.6.1 resource (a stand-in wrapper, then the packfile) of random objects, fixups and class-name strings."""
    kinds = list(SIZES)
    objs, at = [], 0
    for _ in range(rnd.randint(1, 6)):
        c = rnd.choice(kinds if rnd.random() < 0.1 else kinds[:-1])
        objs.append((at, c))
        at += SIZES[c]
    content = bytearray(rnd.randbytes(at + 64))
    for o, c in objs:
        if c == 'hkRigidBody':
            for off in (340, 344):
                v = rnd.choice([200.0, 200.0, 10.0, 202.8, 0.5, 1e6])
                struct.pack_into('>f', content, o + off, v)
        if c in ('hkListShape', 'hkStorageExtendedMeshShape', 'hkExtendedMeshShape') and rnd.random() < 0.7:
            w = o + (48 if c == 'hkListShape' else 64) + 12
            content[w:w + 4] = bytes(4)
    names = {c for _, c in objs}
    strings = []
    for _ in range(rnd.randint(0, 3)):
        s = rnd.choice(['hkRigidBody', 'hkBoxShape', 'hkRootLevelContainer', 'hkSomething']).encode() + b'\0'
        strings.append(len(content))
        content += s
    local = []
    for o, c in objs:
        for _ in range(rnd.randint(0, 3)):
            local.append((o + 4 * rnd.randrange(SIZES[c] // 4), rnd.choice(objs)[0]))
    for s in strings:
        local.append((rnd.choice(objs)[0] + 4 * rnd.randrange(4), s))
    local = list(dict(local).items())
    glob = [(rnd.choice(objs)[0] + 8, rnd.choice([2, 2, 0, 1]), rnd.choice(objs)[0]) for _ in range(rnd.randint(0, 3))]
    cnames = sorted(names | ({'hkWeird'} if rnd.random() < 0.05 else set()))
    b, at_ = packfile(True, b'Havok-4.6.1', cnames, content, local, [], objs)
    # global fixups into the class names: their targets are class-name offsets
    if glob:
        b2, _ = packfile(True, b'Havok-4.6.1', cnames, content, local,
                         [(s, sec, at_.get(rnd.choice(cnames), 0) if sec == 0 else t) for s, sec, t in glob], objs)
        b = b2
    if rnd.random() < 0.08:
        b = b.replace(b'Havok-4.6.1', b'Havok-4.5.0')
    return bytes(16) + b


def damaged(rnd, b):
    r = rnd.random()
    if r < 0.1:
        return b[:rnd.randrange(len(b))]
    if r < 0.2:
        x = bytearray(b)
        for _ in range(rnd.randint(1, 4)):
            x[rnd.randrange(len(x))] = rnd.randrange(256)
        return bytes(x)
    return b


def quick_restructure():
    rnd = random.Random(23)
    K = fake_classes()
    tnames = sorted(set(K) | {'HavokMaterial', 'HavokMaterials'})
    template, _ = packfile(False, b'Havok-5.5.0-r1', tnames, bytes(32), [], [], [(0, tnames[0])])
    small, _ = packfile(False, b'Havok-5.5.0-r1', ['hkpBoxShape'], bytes(32), [], [], [(0, 'hkpBoxShape')])
    table = [0.0] + [0.01 * 1.05 ** i for i in range(255)]
    p = Pair('havokport.restructure')
    for i in range(1500):
        b = damaged(rnd, random_461(rnd, tnames))
        t = small if rnd.random() < 0.05 else template
        p('havokport_restructure', hp.restructure, hp.restructure_reference, b, t, K,
          table if i % 2 else None)
    p.report('1,500 seeded 4.6.1 packfiles (with and without the table)')


def wrapper(rnd, big_table):
    """A part-list wrapper: n hashes, vectors, boxes, the part run (a material table in either order, or BFBC2's
    0-led run), the material list (1, k, hashes)."""
    n = rnd.randint(0, 5)
    b = struct.pack('<2I', n, n if rnd.random() < 0.95 else n + 1) + rnd.randbytes(4 * n)
    a = rnd.randint(0, 2)
    b += struct.pack('<H', a) + rnd.randbytes(12 * a)
    c = rnd.randint(0, 2)
    b += struct.pack('<H', c) + rnd.randbytes(24 * c)
    k = rnd.randint(0, 4)
    if big_table is None:
        idx = [rnd.randrange(max(k, 1)) for _ in range(n)]
        d = len(set(idx))
        run = b'\0\0' + bytes([d, max(idx) if idx else 0]) + (bytes(idx) if d != 1 else b'')
    else:
        m = rnd.randint(0, 3)
        e = '>' if big_table else '<'
        ends = sorted(rnd.randrange(k + 1) for _ in range(m))
        if ends:
            ends[-1] = k
        run = struct.pack(e + '2H', m, k) + struct.pack(f'{e}{k}I', *[rnd.randrange(1 << 32) for _ in range(k)]) + \
            struct.pack(f'{e}{m}H', *ends) + rnd.randbytes(rnd.randint(0, 3))
    b += run + struct.pack('<2I', 1, k) + struct.pack(f'<{k}I', *[rnd.randrange(1 << 32) for _ in range(k)])
    return b


def random_pc(rnd):
    """A PC physics resource: a wrapper, then a little-endian packfile of rigid bodies on MOPP / storage meshes,
    extended mesh shapes of shape subparts (either field order, convex children) and a HavokMaterials list at the
    end of the data."""
    content = bytearray()
    objs, local, glob = [], [], []

    def obj(cls, size):
        o = len(content)
        content.extend(bytes(size))
        objs.append((o, cls))
        return o
    boxes = []
    for _ in range(rnd.randint(0, 3)):
        cls = rnd.choice(['hkpConvexVerticesShape', 'hkpBoxShape', 'hkpSphereShape'])
        o = obj(cls, 64)
        struct.pack_into('<f', content, o + 16, rnd.uniform(0, 0.1))
        struct.pack_into('<3f', content, o + 32, *[rnd.uniform(0.1, 3) for _ in range(3)])
        struct.pack_into('<3f', content, o + 48, *[rnd.uniform(-2, 2) for _ in range(3)])
        boxes.append(o)
    shapes = []
    for _ in range(rnd.randint(0, 3)):
        cls = rnd.choice(['hkpExtendedMeshShape', 'hkpStorageExtendedMeshShape'])
        o = obj(cls, 208)
        shapes.append(o)
    for o in shapes:
        cnt = rnd.randint(0, 3)
        arr = len(content)
        content.extend(bytes(64 * cnt))
        struct.pack_into('<I', content, o + 92, cnt)
        struct.pack_into('<I', content, o + 84, rnd.choice([0, 0, 1]))
        if cnt and rnd.random() < 0.95:
            local.append((o + 88, arr))
        for k in range(cnt):
            r = arr + 64 * k
            q = [rnd.uniform(-1, 1) for _ in range(4)]
            n = sum(x * x for x in q) ** 0.5
            q = [x / n for x in q]
            tr = [rnd.uniform(-50, 50) for _ in range(3)] + [0.0]
            first, second = (tr, q) if rnd.random() < 0.5 else (q, tr)
            struct.pack_into('<4f', content, r + 32, *first)
            struct.pack_into('<4f', content, r + 48, *second)
            nch = rnd.randint(0, 3) if boxes else 0
            struct.pack_into('<I', content, r + 20, nch)
            if nch:
                carr = len(content)
                content.extend(bytes(4 * nch))
                if rnd.random() < 0.95:
                    local.append((r + 16, carr))
                for i in range(nch):
                    if rnd.random() < 0.97:
                        glob.append((carr + 4 * i, 2, rnd.choice(boxes + shapes[:1])))
    storages, bodies = [], []
    for _ in range(rnd.randint(0, 2)):
        o = obj('hkpStorageExtendedMeshShapeMeshSubpartStorage', 80)
        k = rnd.randint(0, 4)
        struct.pack_into('<I', content, o + 60, k)
        if k:
            m = len(content)
            content.extend(struct.pack(f'<{k}I', *[rnd.randrange(3) for _ in range(k)]))
            pad16(content, 0)
            local.append((o + 56, m))
        storages.append(o)
    mesh = obj('hkpStorageExtendedMeshShape', 208) if storages else None
    if mesh is not None:
        arr = len(content)
        content.extend(bytes(4 * len(storages)))
        pad16(content, 0)
        local.append((mesh + 192, arr))
        struct.pack_into('<I', content, mesh + 196, len(storages))
        for i, s in enumerate(storages):
            glob.append((arr + 4 * i, 2, s))
    for _ in range(rnd.randint(0, 3)):
        o = obj('hkpRigidBody', 128)
        if rnd.random() < 0.8:
            name = len(content)
            content.extend(rnd.choice([b'Body\0', b'Wheel_1\0', b'\0']))
            pad16(content, 0)
            local.append((o + 112, name))
        if mesh is not None and rnd.random() < 0.8:
            if rnd.random() < 0.5:
                t = obj('hkpMoppBvTreeShape', 64)
                glob.append((t + 52, 2, mesh))
                glob.append((o + 16, 2, t))
            else:
                glob.append((o + 16, 2, mesh))
        bodies.append(o)
    names = rnd.sample(['grass', 'metal', 'wood', 'concrete'], rnd.randint(0, 3))
    if names or rnd.random() < 0.5:
        pad16(content, 0)
        hm = obj('HavokMaterials', 16)
        arr = len(content)
        content.extend(bytes(4 * len(names)))
        pad16(content, 0)
        local.append((hm, arr))
        struct.pack_into('<I', content, hm + 4, len(names))
        for i, n in enumerate(names):
            m = obj('HavokMaterial', 16)
            s = len(content)
            content.extend(n.encode() + b'\0')
            pad16(content, 0)
            local.append((m, s))
            glob.append((arr + 4 * i, 2, m))
    cnames = sorted({c for _, c in objs})
    pk, _ = packfile(False, b'Havok-5.5.0-r1', cnames, content, local, glob, objs)
    w = wrapper(rnd, rnd.choice([None, None, True, False]))
    return w + pk, len(bodies), names


def quick_pc():
    rnd = random.Random(31)
    P = {w: Pair(f'havokport.{w}') for w in ('wrapper_run_table', '_run_table_to_pc', 'shape_subparts_to_550',
                                               '_wrapper', 'part_materials', 'part_run', 'with_part_materials',
                                               'havok_materials', 'with_havok_materials', 'bodies',
                                               'with_face_materials')}
    for _ in range(1500):
        b, nb, names = random_pc(rnd)
        b = damaged(rnd, b)
        P['wrapper_run_table']('havokport_run_table', hp.wrapper_run_table, hp.wrapper_run_table_reference, b)
        P['_run_table_to_pc']('havokport_run_table_to_pc', hp._run_table_to_pc, hp._run_table_to_pc_reference, b)
        P['shape_subparts_to_550']('havokport_subparts', hp.shape_subparts_to_550, hp.shape_subparts_to_550_reference,
                                   b)
        P['_wrapper']('havokport_wrapper', hp._wrapper, hp._wrapper_reference, b)
        P['part_materials']('havokport_part_materials', hp.part_materials, hp.part_materials_reference, b)
        idx = [rnd.randrange(rnd.choice([1, 3, 300])) - (rnd.random() < 0.02) for _ in range(rnd.randint(0, 5))]
        P['part_run']('havokport_part_run', hp.part_run, hp.part_run_reference, idx)
        hashes = [rnd.randrange(1 << 32) for _ in range(rnd.randint(0, 5))]
        P['with_part_materials']('havokport_with_part_materials', hp.with_part_materials,
                                 hp.with_part_materials_reference, b, idx, hashes)
        try:
            hashes, _ = hp.part_materials_reference(b)
            P['with_part_materials']('havokport_with_part_materials', hp.with_part_materials,
                                     hp.with_part_materials_reference, b, idx, hashes + [7, 9])
        except Exception:                                        # noqa: BLE001 - not a readable wrapper
            pass
        P['havok_materials']('havokport_havok_materials', hp.havok_materials, hp.havok_materials_reference, b)
        grown = names + rnd.sample(['sand', 'glass', 'Metal'], rnd.randint(0, 2))
        if rnd.random() < 0.1:
            grown = ['other'] + grown
        P['with_havok_materials']('havokport_with_havok_materials', hp.with_havok_materials,
                                  hp.with_havok_materials_reference, b, grown)
        got = P['bodies']('havokport_bodies', hp.bodies, hp.bodies_reference, b)
        changes = {}
        if got[0] == 'ok':
            for i, (_, st) in enumerate(got[1]):
                if st is not None or rnd.random() < 0.2:
                    changes[i if rnd.random() < 0.95 else -1] = [[rnd.randrange(4) for _ in x] for x in (st or [[0]])]
        P['with_face_materials']('havokport_with_face_materials', hp.with_face_materials,
                                 hp.with_face_materials_reference, b, changes)
    for p in P.values():
        p.report('1,500 seeded PC resources')


# ------------------------------------------------------------------------------------------------ game

def game():
    import bf1943import as bi
    import meshview
    import shaderdb
    try:
        root = shaderdb.find_opfor(None)
        exe = os.path.join(root, 'game', 'BFBC2Game.exe')
        folder = bi.default_source()
        if not folder or not os.path.isdir(folder) or not os.path.isfile(exe):
            raise FileNotFoundError(f'no BF1943 source or {exe}')
    except (shaderdb.ShaderDbError, FileNotFoundError) as e:
        print(f'SKIP game: {e}')
        return
    src = bi.Source(folder)
    ix = meshview.Index(root)
    K = havok.classes(exe)
    raw = open(exe, 'rb').read()
    t0 = time.perf_counter()
    table = hp.ufloat8_table(raw)
    t1 = time.perf_counter()
    ref_table = hp.ufloat8_table_reference(raw)
    t2 = time.perf_counter()
    check(table == ref_table and table is not None, f'ufloat8_table: the same 256 values ({t1 - t0:.2f} s; the '
          f'reference {t2 - t1:.2f} s)')
    templates = [ix.read(t) for t in ('objects/objects/boat_house/boat_house_havok.res',
                                      'objects/levelspecific_objects/ghosttown_lamppolelong/'
                                      'ghosttown_lamppolelong_banger_havok.res')]
    names = [k for k, (_, e) in sorted(src.entries.items()) if e['type'] == 'HavokPhysicsData']
    rs, cv = Pair('havokport.restructure (BF1943)'), Pair('havokport.convert (BF1943)')
    fin = Pair('havokport finish (BF1943)')
    converted, errors, mopps = 0, 0, []
    for k in names:
        b = src.data(k)
        for t in templates:
            for tab in (None, table):
                rs('havokport_restructure', hp.restructure, hp.restructure_reference, b, t, K, tab)
                got = cv('havokport_restructure', hp.convert, hp.convert_reference, b, t, K, tab)
                if tab is not None and got[0] == 'ok' and t is templates[0]:
                    converted += 1
                    mopps.append(got[1])
                elif tab is not None and t is templates[0]:
                    errors += 1
        try:
            pc = havok.convert(hp.restructure_reference(b, templates[0], K, table), K)
            fin('havokport_finish', lambda x: hp.shape_subparts_to_550(hp._run_table_to_pc(x))[0],
                lambda x: hp.shape_subparts_to_550_reference(hp._run_table_to_pc_reference(x))[0], pc)
        except Exception:                                        # noqa: BLE001 - refused by both above
            pass
    rs.report(f'{len(names)} HavokPhysicsData x 2 templates x with / without the table')
    cv.report(f'{len(names)} HavokPhysicsData converted ({converted} with the first template and the table, '
              f'{errors} refused by both)')
    fin.report('the console converter\'s output: the swap and the subparts')
    # BFBC2's own physics
    pcs = sorted(n for n in ix.where if (n.startswith('objects/') and n.endswith('_havok.res')) or
                 (n.startswith('levels/') and n.endswith('_havokasset.res')))
    data = ix.read_many(pcs)
    P = {w: Pair(f'havokport.{w} (BFBC2)') for w in ('_wrapper', 'part_materials', 'part_run', 'havok_materials',
                                                       'bodies', 'shape_subparts_to_550', 'with_havok_materials',
                                                       'with_face_materials', 'with_part_materials')}
    for n in pcs:
        b = data[n]
        P['_wrapper']('havokport_wrapper', hp._wrapper, hp._wrapper_reference, b)
        got = P['part_materials']('havokport_part_materials', hp.part_materials, hp.part_materials_reference, b)
        if got[0] == 'ok':
            hashes, idx = got[1]
            if idx:
                P['part_run']('havokport_part_run', hp.part_run, hp.part_run_reference, idx)
                P['with_part_materials']('havokport_with_part_materials', hp.with_part_materials,
                                         hp.with_part_materials_reference, b, idx, list(hashes) + [1234567])
        mats = P['havok_materials']('havokport_havok_materials', hp.havok_materials, hp.havok_materials_reference, b)
        if mats[0] == 'ok':
            P['with_havok_materials']('havokport_with_havok_materials', hp.with_havok_materials,
                                      hp.with_havok_materials_reference, b, mats[1] + ['aircraft_airframe', 'x'])
        bd = P['bodies']('havokport_bodies', hp.bodies, hp.bodies_reference, b)
        if bd[0] == 'ok':
            changes = {i: [list(reversed(x)) for x in st] for i, (_, st) in enumerate(bd[1]) if st is not None}
            P['with_face_materials']('havokport_with_face_materials', hp.with_face_materials,
                                     hp.with_face_materials_reference, b, changes)
        # the subparts step on DICE's own file, and with every w lane it writes zeroed (as 4.6.1's files have them)
        P['shape_subparts_to_550']('havokport_subparts', hp.shape_subparts_to_550, hp.shape_subparts_to_550_reference,
                                   b)
        if n.endswith('_havokasset.res'):
            z = bytearray(b)
            p = b.find(havok.MAGIC)
            if p >= 0:
                for i in range(p + 300, len(z) - 4, 4):
                    if z[i:i + 4] == b'\0\0\x80\x3f' and (i - p) % 16 == 12:
                        z[i:i + 4] = bytes(4)
                P['shape_subparts_to_550']('havokport_subparts', hp.shape_subparts_to_550,
                                           hp.shape_subparts_to_550_reference, bytes(z))
    for p in P.values():
        p.report(f'{len(pcs)} physics resources of BFBC2')
    # MOPP codes of the converted files (their trees as the havok test reads them)
    import numpy as np
    sys.path.insert(0, HERE)
    import test_havokport as th
    hk = th.hkpack()
    given = {n: {'size': k.size, 'parent': k.parent, 'members': [
        {'name': m.name, 'class': ('given', m.cls) if m.cls else None, 'type': m.type, 'subtype': m.subtype,
         'carray': m.carray, 'offset': m.offset} for m in k.members]} for n, k in K.items()}
    rnd = random.Random(5)
    q, cp, rc = Pair('MOPP query (game)'), Pair('MOPP complete (game)'), Pair('MOPP ray_cast (game)')
    codes = 0
    for b in mopps + [data[n] for n in pcs[::7]]:
        try:
            meshes = th.mopp_meshes(hk, given, b)
        except Exception:                                        # noqa: BLE001 - not read by hkpack
            continue
        for code, info, tris in meshes:
            codes += 1
            boxes = {k: (t.min(0), t.max(0)) for k, t in tris.items()}
            cp('mopp_complete', mopp.complete, mopp.complete_reference, code, info, boxes)
            tl = {k: t.tolist() for k, t in tris.items()}
            allv = np.concatenate(list(tris.values()))
            lo, hi = allv.min(0) - 0.5, allv.max(0) + 0.5
            for _ in range(10):
                # plain floats, as test_havokport's rays (.tolist()); now and then NumPy's (refused: Python's sum
                # adds NumPy scalars without its compensation)
                a = [rnd.uniform(lo[i], hi[i]) for i in range(3)]
                if rnd.random() < 0.9:
                    a = [float(x) for x in a]
                c = [float(rnd.uniform(lo[i], hi[i])) for i in range(3)]
                q('mopp_query_box', mopp.query, mopp.query_reference, code, info, [min(x, y) for x, y in zip(a, c)],
                  [max(x, y) for x, y in zip(a, c)])
                d = [y - x for x, y in zip(a, c)]
                n = sum(x * x for x in d) ** 0.5
                if n > 1e-6:
                    rc('mopp_ray_cast', mopp.ray_cast, mopp.ray_cast_reference, code, info, tl, a,
                       [x / n for x in d], n * 1.5)
    q.report(f'{codes} MOPP codes of converted and DICE\'s files, random boxes')
    cp.report(f'{codes} MOPP codes, every triangle\'s own box')
    rc.report(f'{codes} MOPP codes, random rays')


def main(argv):
    from bfbc2core import native
    try:
        native.require('the BF1943 importer\'s Havok ports (round F2b)', 'havokport_restructure')
    except native.NativeMissing as e:
        check(False, 'the Rust core is built from these sources', str(e))
        return 1
    if importcore.native() is None:
        check(False, 'importcore finds the module', 'BFBC2_IMPORTER_PYTHON / BFBC2_PURE_PYTHON set?')
        return 1
    t = time.perf_counter()
    quick_mopp()
    quick_restructure()
    quick_pc()
    if '--game' in argv:
        game()
    print(f'{COUNTS["same"]} outputs the same, {COUNTS["differ"]} differ, {COUNTS["refused"]} left to the Python '
          f'({time.perf_counter() - t:.1f} s)')
    if FAILS:
        print(f'{len(FAILS)} FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
