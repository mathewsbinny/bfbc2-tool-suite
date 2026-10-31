r"""Xbox 360 -> PC conversion of HavokPhysicsData resources (.res): DICE's wrapper and the Havok 5.5.0 binary
packfile in it. Format: FORMAT.md section 1.

The packfile's objects are converted field by field from the class layouts the game itself carries: the hkClass
initializers in BFBC2Game.exe (read from the user's copy each time, nothing is shipped). The two platforms use the
same layout rules apart from the byte order (pointer size 4, no padding reuse, empty-base optimisation on), so a
field keeps its offset and only its bytes are reversed by the field's width.

    import havok
    classes = havok.classes(r'...\BFBC2Game.exe')
    pc_bytes = havok.convert(console_bytes, classes)
"""
import collections
import struct

MAGIC = bytes.fromhex('57e0e05710c0c010')        # the same bytes on both platforms

(VOID, BOOL, CHAR, INT8, UINT8, INT16, UINT16, INT32, UINT32, INT64, UINT64, REAL, VECTOR4, QUATERNION, MATRIX3,
 ROTATION, QSTRANSFORM, MATRIX4, TRANSFORM, ZERO, POINTER, FUNCTIONPOINTER, ARRAY, INPLACEARRAY, ENUM, STRUCT,
 SIMPLEARRAY, HOMOGENEOUSARRAY, VARIANT, CSTRING, ULONG, FLAGS, HALF, STRINGPTR) = range(34)
# hkClassMember types with a fixed size: (size, width of each value)
PLAIN = {BOOL: (1, 1), CHAR: (1, 1), INT8: (1, 1), UINT8: (1, 1), INT16: (2, 2), UINT16: (2, 2), HALF: (2, 2),
         INT32: (4, 4), UINT32: (4, 4), ULONG: (4, 4), REAL: (4, 4), INT64: (8, 8), UINT64: (8, 8),
         VECTOR4: (16, 4), QUATERNION: (16, 4), MATRIX3: (48, 4), ROTATION: (48, 4), QSTRANSFORM: (48, 4),
         MATRIX4: (64, 4), TRANSFORM: (64, 4), FUNCTIONPOINTER: (4, 4), POINTER: (4, 4), CSTRING: (4, 4),
         STRINGPTR: (4, 4)}


class ConvertError(Exception):
    """The file is not what the converter knows (it converts nothing rather than guess)."""


# ---------------------------------------------------------------- class layouts from the executable

class _PE:
    def __init__(self, path):
        d = self.d = open(path, 'rb').read()
        pe = struct.unpack_from('<I', d, 0x3C)[0]
        nsec = struct.unpack_from('<H', d, pe + 6)[0]
        opt = struct.unpack_from('<H', d, pe + 20)[0]
        self.base = struct.unpack_from('<I', d, pe + 52)[0]
        self.secs = []
        for i in range(nsec):
            vsize, va, rsize, raw = struct.unpack_from('<IIII', d, pe + 24 + opt + 40 * i + 8)
            self.secs.append((self.base + va, max(vsize, rsize), raw, rsize))

    def off(self, va):
        for s, size, raw, rsize in self.secs:
            if s <= va < s + size:
                return raw + va - s if va - s < rsize else None
        return None

    def va(self, off):
        for s, size, raw, rsize in self.secs:
            if raw <= off < raw + rsize:
                return s + off - raw
        return None

    def cstr(self, va):
        o = self.off(va) if va else None
        if o is None:
            return None
        s = self.d[o:self.d.find(b'\0', o)]
        return s.decode('latin-1') if s and all(32 <= c < 127 for c in s) else None


Member = collections.namedtuple('Member', 'name cls type subtype carray flags offset')
Klass = collections.namedtuple('Klass', 'name parent size members')


def _pushes(d, start, end):
    """The immediates pushed from start to end (push imm8 / imm32 only), first argument first; None otherwise."""
    args, p = [], start
    while p < end:
        if d[p] == 0x6A:
            args.append(struct.unpack_from('<b', d, p + 1)[0] & 0xFFFFFFFF)
            p += 2
        elif d[p] == 0x68:
            args.append(struct.unpack_from('<I', d, p + 1)[0])
            p += 5
        else:
            return None
    return args[::-1]


def classes(exe):
    """{class name: Klass} from the hkClass initializers of the game executable: small functions that push the 12
    constructor arguments (name, parent, size, interfaces, count, enums, count, members, count, defaults,
    attributes, flags), load the static hkClass object into ecx and call the constructor. Members are the static
    24-byte hkClassMember records (name, class, enum, type, subtype, C array size, flags, offset, attributes)."""
    pe = _PE(exe)
    d = pe.d
    sites, ctors = [], collections.Counter()
    i = 0
    while True:
        i = d.find(b'\xB9', i, len(d) - 10)                    # mov ecx, imm32 ; call rel32
        if i < 0:
            break
        here = pe.va(i)
        if d[i + 5] == 0xE8 and here is not None:
            for s in range(i - 1, max(i - 80, 0), -1):         # the stub's start (an immediate may hold 0xCC)
                if d[s - 1] in (0xCC, 0xC3):
                    a = _pushes(d, s, i)
                    if a and len(a) == 12 and pe.cstr(a[0]):
                        target = here + 10 + struct.unpack_from('<i', d, i + 6)[0]
                        if pe.cstr(a[0]).startswith('hk'):
                            ctors[target] += 1
                        sites.append((target, struct.unpack_from('<I', d, i + 1)[0], a))
                        break
        i += 1
    if not ctors:
        raise ConvertError(f'{exe}: no Havok class initializers found (not BFBC2Game.exe?)')
    ctor = ctors.most_common(1)[0][0]
    raw = {}
    for target, obj, a in sites:
        if target != ctor:
            continue
        name, parent, size, _, _, _, _, members, count = a[:9]
        mem = []
        for k in range(count):
            o = pe.off(members + 24 * k)
            nm, cl, _ = struct.unpack_from('<3I', d, o)
            ty, st, ca, fl, off = struct.unpack_from('<BBhHH', d, o + 12)
            mem.append(Member(pe.cstr(nm), cl, ty, st, ca, fl, off))
        raw[obj] = (pe.cstr(name), parent, size, mem)
    out = {}
    for name, parent, size, mem in raw.values():
        mem = [m._replace(cls=raw[m.cls][0] if m.cls in raw else None) for m in mem]
        out[name] = Klass(name, raw[parent][0] if parent in raw else None, size, mem)
    return out


# ---------------------------------------------------------------- DICE's wrapper

def wrapper_end(b):
    """Converts nothing; the offset of the packfile (= the wrapper's length) in a console file."""
    o = b.find(MAGIC)
    if o < 0:
        raise ConvertError('no Havok packfile in the resource')
    return o


def convert_wrapper(b, o):
    """The wrapper before the packfile at o (FORMAT.md 1.1), console -> PC."""
    out, p = bytearray(), 0

    def num(fmt, n=1):
        nonlocal p
        size = struct.calcsize(fmt)
        for _ in range(n):
            out.extend(struct.pack('<' + fmt, *struct.unpack_from('>' + fmt, b, p)))
            p += size

    def be(fmt, at):
        return struct.unpack_from('>' + fmt, b, at)[0]

    n = be('I', 0)
    num('I', 2)
    num('I', be('I', 4))                       # part name hashes
    a = be('H', p)
    num('H')
    num('f', 3 * a)                            # a vectors
    c = be('H', p)
    num('H')
    num('f', 6 * c)                            # c boxes (min, max)
    # bytes, then 1, k and k hashes ending at the packfile: the byte run's length is where that holds
    tail = [q for q in range(p, o - 7) if be('I', q) == 1 and q + 8 + 4 * be('I', q + 4) == o]
    if len(tail) != 1:
        raise ConvertError(f'wrapper: the end of the byte run is not unique ({len(tail)} candidates; {n} parts)')
    out.extend(b[p:tail[0]])
    p = tail[0]
    num('I', 2 + be('I', p + 4))
    return bytes(out)


# ---------------------------------------------------------------- the packfile

class Packfile:
    """One packfile at offset start of data; convert() returns the whole buffer with it converted. The converted
    bytes are recorded (.claim: offset -> value width, .owner: offset -> (class, member)) for verification."""

    def __init__(self, data, start, klasses):
        self.src, self.out, self.p0, self.classes = data, bytearray(data), start, klasses
        self.claim, self.owner = {}, {}
        self.seen = set()
        self.arrays = []                    # (class, member, data start, count, element size, array header)
        self.cur = self.note = None

    def swap(self, at, width, count=1):
        for k in range(count):
            o = at + k * width
            if o in self.claim and self.claim[o] == width and self.owner.get(o, self.note) == self.note:
                continue
            for b in range(o, o + width):
                if self.claim.get(b, width) != width:
                    raise ConvertError(f'byte {b - self.p0:#x} converted as a {self.claim[b]}-byte and a '
                                       f'{width}-byte value')
            if o not in self.claim:
                self.out[o:o + width] = self.src[o:o + width][::-1]
            for b in range(o, o + width):
                self.claim[b] = width
                self.owner[b] = self.note

    def be32(self, at):
        return struct.unpack_from('>I', self.src, at)[0]

    def convert(self):
        p = self.p0
        if self.src[p:p + 8] != MAGIC:
            raise ConvertError('no packfile magic')
        rules = self.src[p + 16:p + 20]
        if rules != b'\x04\x00\x00\x01':
            raise ConvertError(f'layout rules {rules.hex()}, not the Xbox 360\'s 04 00 00 01')
        if not self.src[p + 40:p + 56].startswith(b'Havok-5.5.0'):
            raise ConvertError(f'Havok version {self.src[p + 40:p + 56]!r}, not 5.5.0')
        self.note = ('header', '')
        self.swap(p + 8, 4, 2)                         # user tag, file version
        self.out[p + 17] = 1                           # little endian
        nsec = self.be32(p + 20)
        self.swap(p + 20, 4, 5)                        # sections, contents section / offset, class names / offset
        self.swap(p + 56, 4, 2)                        # flags, pad
        self.sections = []
        for i in range(nsec):
            h = p + 64 + 48 * i
            self.sections.append((self.src[h:h + 19].split(b'\0')[0].decode('latin-1'),
                                  struct.unpack_from('>7I', self.src, h + 20)))
            self.swap(h + 20, 4, 7)
        for i, (tag, (start, loc, glob, virt, exp, imp, end)) in enumerate(self.sections):
            base = p + start
            if tag == '__classnames__':
                self._classnames(base, base + loc)
                continue
            if loc == end:
                continue                               # empty (__types__)
            if exp != end or imp != end:
                raise ConvertError(f'{tag}: exports or imports (not handled)')
            self.note = ('fixups', tag)
            self.local, self.glob = {}, {}
            for o in self._entries(base + loc, base + glob, 8):
                self.swap(o, 4, 2)
                self.local[self.be32(o)] = self.be32(o + 4)
            for o in self._entries(base + glob, base + virt, 12):
                self.swap(o, 4, 3)
                self.glob[self.be32(o)] = (self.be32(o + 4), self.be32(o + 8))
            objs = []
            for o in self._entries(base + virt, base + exp, 12):
                self.swap(o, 4, 3)
                sec, off = self.be32(o + 4), self.be32(o + 8)
                if self.sections[sec][0] != '__classnames__' or off not in self.names:
                    raise ConvertError(f'{tag}: an object\'s class name is not in the class-name section')
                objs.append((self.be32(o), self.names[off]))
            self.base, self.secindex = base, i
            for off, name in objs:
                self._struct(name, off)
        return bytes(self.out)

    def _entries(self, a, end, size):
        o = a
        while o + size <= end and self.be32(o) != 0xFFFFFFFF:
            yield o
            o += size
        if any(x != 0xFF for x in self.src[o:end]):
            raise ConvertError(f'a fixup table has data after its 0xFF padding at {o - self.p0:#x}')

    def _classnames(self, a, end):
        self.names = {}
        self.note = ('class names', '')
        o = a
        while o + 5 <= end and self.src[o + 4] == 0x09:
            e = self.src.index(b'\0', o + 5)
            self.names[o + 5 - a] = self.src[o + 5:e].decode('latin-1')
            self.swap(o, 4)                            # the class signature
            o = e + 1
        if any(x != 0xFF for x in self.src[o:end]):
            raise ConvertError('class-name section: unknown data after the names')

    def _members(self, name):
        k = self.classes.get(name)
        if k is None:
            raise ConvertError(f'class {name} is not among the game\'s Havok classes')
        chain = []
        while k:
            chain.append(k)
            k = self.classes.get(k.parent) if k.parent else None
        return [m for k in reversed(chain) for m in k.members]

    def _target(self, at):
        if at in self.local:
            return self.local[at]
        g = self.glob.get(at)
        return g[1] if g and g[0] == self.secindex else None

    def _struct(self, name, off):
        if (off, name) in self.seen:
            return
        self.seen.add((off, name))
        prev, self.cur = self.cur, name
        for m in self._members(name):
            self.note = (name, m.name)
            self._member(m, off + m.offset)
        self.cur = prev

    def _size(self, ty, cls):
        return self.classes[cls].size if ty == STRUCT else PLAIN[ty][0]

    def _value(self, ty, sub, cls, at, ctx):
        """One value of type ty at section offset at; returns its size."""
        A = self.base + at
        if ty in (ENUM, FLAGS):
            size, w = PLAIN[sub]
            self.swap(A, w, size // w)
            return size
        if ty == STRUCT:
            if cls not in self.classes:
                raise ConvertError(f'{ctx}: struct class {cls} unknown')
            note = self.note
            self._struct(cls, at)
            self.note = note
            return self.classes[cls].size
        if ty == ZERO:
            return self._value(sub, VOID, cls, at, ctx)
        if ty == VARIANT:
            self.swap(A, 4, 2)
            return 8
        if ty not in PLAIN:
            raise ConvertError(f'{ctx}: member type {ty} not handled')
        size, w = PLAIN[ty]
        self.swap(A, w, size // w)
        if ty == POINTER and sub == STRUCT and cls:
            t = self._target(at)
            if t is not None:
                note = self.note
                self._struct(cls, t)
                self.note = note
        return size

    def _member(self, m, at):
        ctx = f'{self.cur}.{m.name}'
        if m.type in (ARRAY, SIMPLEARRAY):
            head = 12 if m.type == ARRAY else 8
            for k in range(max(m.carray, 1)):
                a = at + k * head
                count = self.be32(self.base + a + 4)
                self.swap(self.base + a, 4, head // 4)
                if not count or m.subtype == VOID:
                    continue
                t = self._target(a)
                if t is None:
                    raise ConvertError(f'{ctx}: an array of {count} without a fixup')
                es = self._size(m.subtype, m.cls)
                self.arrays.append((self.cur, m.name, self.base + t, count, es, self.base + a))
                for e in range(count):
                    self._value(m.subtype, VOID, m.cls, t + e * es, ctx)
            return
        if m.type in (HOMOGENEOUSARRAY, INPLACEARRAY):
            raise ConvertError(f'{ctx}: member type {m.type} not handled')
        size = self._value(m.type, m.subtype, m.cls, at, ctx)
        for k in range(1, max(m.carray, 1)):
            self._value(m.type, m.subtype, m.cls, at + k * size, ctx)


def convert(b, klasses):
    """A console HavokPhysicsData resource -> the PC one (same size)."""
    o = wrapper_end(b)
    w = convert_wrapper(b, o)
    out = Packfile(b, o, klasses).convert()
    return w + out[o:]


# ---------------------------------------------------------------- verification against PC files

def ignorable(pf, out, pc, i):
    """Why byte i may differ from the PC file although the conversion is right (FORMAT.md 1.3), or None: values
    the game never reads (padding index of a 16-bit triangle, the saved quality type of a motion with no saved
    motion) or rebuilds at load (a collidable's cached bounding volume marked invalid, a triangle subpart's
    triangleOffset)."""
    cls, mem = pf.owner.get(i, (None, None))
    if cls == 'hkpStorageExtendedMeshShapeMeshSubpartStorage' and mem == 'indices16':
        for c, m, st, n, es, _ in pf.arrays:
            if m == 'indices16' and st <= i < st + 2 * n:
                return 'triangle padding index' if (i - st) // 2 % 4 == 3 else None
    if cls == 'hkpCollidableBoundingVolumeData':
        for off, name in pf.seen:
            A = pf.base + off
            if name == cls and A <= i < A + 0x20:
                ours = struct.unpack_from('<I', out, A)[0] > struct.unpack_from('<I', out, A + 0x10)[0]
                theirs = struct.unpack_from('<I', pc, A)[0] > struct.unpack_from('<I', pc, A + 0x10)[0]
                marker = A <= i < A + 4 or A + 0x10 <= i < A + 0x14     # min[0] > max[0]: must be the same
                return 'cached bounding volume marked invalid' if ours and theirs and not marker else None
    if cls == 'hkpExtendedMeshShapeTrianglesSubpart' and mem == 'triangleOffset':
        return 'triangle offset (computed at load)'
    if cls in ('hkpMaxSizeMotion', 'hkpKeyframedRigidMotion', 'hkpMotion') and mem == 'savedQualityTypeIndex':
        for off, name in pf.seen:
            A = pf.base + off
            if A + 0x11C <= i < A + 0x11E and (off + 0x118) not in pf.local and (off + 0x118) not in pf.glob:
                return 'saved quality type without a saved motion'
    return None


def verify(console, pc, klasses):
    """Converts console and compares with pc: (exact, {reason: bytes}, [unexplained offsets])."""
    o = wrapper_end(console)
    w = convert_wrapper(console, o)
    pf = Packfile(console, o, klasses)
    out = pf.convert()
    why, bad = collections.Counter(), []
    if w != pc[:o]:
        bad.append(next(i for i in range(o) if i >= len(w) or w[i] != pc[i]))
    for i in range(o, len(pc)):
        if out[i] != pc[i]:
            r = ignorable(pf, out, pc, i)
            if r:
                why[r] += 1
            else:
                bad.append(i)
    return not why and not bad, dict(why), bad
