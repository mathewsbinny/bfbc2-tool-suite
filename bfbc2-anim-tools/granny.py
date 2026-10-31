r"""granny.py - read Bad Company 2's Granny resources (GrannyModel skeletons, GrannyAnimation animations) and the
animation curves in them.

  python granny.py dump <resource> [--depth N]               the file's object tree
  python granny.py bones <skeleton resource>                 bones: index, parent, bind position (world)
  python granny.py tracks <animation> [--bones a,b] [--at t1,t2,...]
                                                            per bone: its curves' formats and the local transform
                                                            at the given times (default: start, middle, end)
  python granny.py shift <animation> --bone <name> (--rest x,y,z | --by x,y,z) [--ramp] --out <file>
                                                            move a bone's position track: so its last key is at
                                                            --rest, or by a vector; --ramp moves each key by its
                                                            progress towards the end (a part carried to a mount)

<resource> is a file, or a resource name in the game copy (e.g. characters/skeletons/ske01.res or
animations/weapons/handheld/m1garand_rgl/1p/1p_fire.res; any case, the .res ending optional).

Xbox 360 resources (BFBC2's console copy, Battlefield 1943) are the same format big-endian throughout, with the
big-endian Granny magic B595110E 4BB5A56A 502828EB 04B37825; they are read (dump, bones, tracks), never written.

Format (little-endian on PC): u32 10 | u32 n, then n x {u32 bone, u32 1, f32, f32} (animations; DICE's per-bone extra
values, not read) | u32 count, count x {u16 length, bytes}: the string table | u32 size, then a Granny 2 file (magic
29DE6CC0 BAA4532B 25F5B7A5 F666E2EE, format revision 7, 32-bit): sections with relocation tables (12 bytes each:
offset in the section, target section, target offset), a root type (member definitions, 32 bytes each: type, name,
child definition, array size, 4 x u32) and a root object. DICE stores every name - member names and strings in the
data - as an index into the string table instead of a pointer. Member types: 1 inline struct, 2 reference,
3 reference to array (count, pointer), 4 array of references, 5 variant reference (type, pointer), 7 reference to
variant array (type, count, pointer), 8 string, 9 transform (flags, position 3, orientation 4, scale/shear 9 floats),
10 real32, 11-14 8-bit, 15-18 16-bit, 19-20 32-bit integers, 21 real16, 22 empty reference.

Curves (TransformTrack: OrientationCurve, PositionCurve, ScaleShearCurve; each a CurveData variant whose type name
says its format): DaIdentity, D3/D4Constant32f, DaK32fC32f (float knots and controls), D3K16uC16u / D3K8uC8u
(quantized knots and controls: value = control * scale + offset, time = knot / OneOverKnotScale, the scale given as
its top 16 bits), D3I1K8uC8u (one control per knot for all three), D4nK16uC15u / D4nK8uC7u (quaternions: 3 stored
components of 15 / 7 bits, the dropped one recomputed; which one and the sign in the top bits, scales and offsets
from a 16-entry table chosen by ScaleOffsetTableEntries). Curves are evaluated at knots (degree 0 / 1 sampled
linearly between knots; higher degrees are read the same way, which is exact at the knots).
"""
import math
import os
import struct
import sys

MAGIC = bytes.fromhex('29DE6CC0BAA4532B25F5B7A5F666E2EE')
MAGIC_BE = bytes.fromhex('B595110E4BB5A56A502828EB04B37825')     # the same format, 32-bit big-endian
SIZES = {2: 4, 3: 8, 4: 8, 5: 8, 7: 12, 8: 4, 9: 68, 10: 4, 11: 1, 12: 1, 13: 1, 14: 1, 15: 2, 16: 2, 17: 2, 18: 2,
         19: 4, 20: 4, 21: 2, 22: 4}
SCALAR = {10: '<f', 11: '<b', 12: '<B', 13: '<b', 14: '<B', 15: '<h', 16: '<H', 17: '<h', 18: '<H', 19: '<i',
          20: '<I', 21: '<e'}


class GrannyError(Exception):
    pass


class Granny:
    """A parsed resource: strings, sections, relocations; root() reads the object tree into dicts and lists."""

    def __init__(self, b, where='file'):
        self.where = where
        if len(b) < 4 or 10 not in struct.unpack_from('<I', b, 0) + struct.unpack_from('>I', b, 0):
            raise GrannyError(f'{where}: not a Granny resource of this game (version {struct.unpack_from("<I", b, 0)[0]})')
        # PC resources are little-endian; Xbox 360 ones (BFBC2's console copy, Battlefield 1943) big-endian throughout
        self.e = '<' if struct.unpack_from('<I', b, 0)[0] == 10 else '>'
        p = 4
        # a skeleton starts with the strings; animations have a per-bone table first (16-byte records; delta
        # animations other layouts), so the table is found by its first string, "<file>"
        if not self._strings_at(b, p):
            k = b.find(struct.pack(self.e + 'H', 6) + b'<file>', p)
            p = k - 4 if k >= 4 else p
        if not self._strings_at(b, p):
            raise GrannyError(f'{where}: no string table where one is expected')
        n = struct.unpack_from(self.e + 'I', b, p)[0]
        p += 4
        self.strings = []
        for _ in range(n):
            ln = struct.unpack_from(self.e + 'H', b, p)[0]
            self.strings.append(b[p + 2:p + 2 + ln].decode('latin-1'))
            p += 2 + ln
        g = b[p + 4:]
        self.data = bytearray(b)
        self.gr2 = p + 4                                    # where the Granny file starts in the resource
        if g[:16] != (MAGIC if self.e == '<' else MAGIC_BE):
            raise GrannyError(f'{where}: no Granny 2 (32-bit, {"little" if self.e == "<" else "big"}-endian) file '
                              f'after the string table')
        rev, _, _, soff, count = struct.unpack_from(self.e + '5I', g, 32)
        if rev != 7:
            raise GrannyError(f'{where}: Granny format revision {rev}; this reader knows 7')
        self.root_type = struct.unpack_from(self.e + '2I', g, 52)
        self.root_obj = struct.unpack_from(self.e + '2I', g, 60)
        self.sections, self.reloc, self.section_at = [], {}, []
        self.total = struct.unpack_from(self.e + 'I', g, 36)[0]
        self.crc_from = 32 + soff                            # the CRC covers the section table to the end
        for k in range(count):
            comp, off, size, _, _, _, _, roff, rcount, _, _ = struct.unpack_from(self.e + '11I', g, 32 + soff + 44 * k)
            if comp:
                raise GrannyError(f'{where}: section {k} is compressed (type {comp}); not read')
            self.sections.append(bytearray(g[off:off + size]))
            self.section_at.append(self.gr2 + off)
            for r in range(rcount):
                o, ts, to = struct.unpack_from(self.e + '3I', g, roff + 12 * r)
                self.reloc[(k, o)] = (ts, to)
        self._types = {}

    def _strings_at(self, b, p):
        try:
            n = struct.unpack_from(self.e + 'I', b, p)[0]
            ln = struct.unpack_from(self.e + 'H', b, p + 4)[0]
            return 0 < n < 100000 and b[p + 6:p + 6 + ln] == b'<file>'
        except struct.error:
            return False

    def name(self, i):
        return self.strings[i] if 0 <= i < len(self.strings) else f'#{i}'

    def members(self, ref):
        """[(type, name, child ref, array size)] of the type at ref (section, offset)."""
        if ref not in self._types:
            sec, off = ref
            out = []
            while True:
                t, nm, _, arr = struct.unpack_from(self.e + '4I', self.sections[sec], off)
                if t == 0:
                    break
                out.append((t, self.name(nm), self.reloc.get((sec, off + 8)), arr))
                off += 32
            self._types[ref] = out
        return self._types[ref]

    def type_name(self, ref):
        """A variant's type name: the name of its first member's struct, else its members joined."""
        ms = self.members(ref)
        return ms[0][1] if ms and ms[0][1].startswith('CurveDataHeader') else '/'.join(m[1] for m in ms)

    def size(self, ref):
        s = 0
        for t, _, child, arr in self.members(ref):
            one = self.size(child) if t == 1 else SIZES.get(t, 0)
            s += one * max(arr, 1)
        return s

    def read(self, tref, oref, depth=0, limit=64):
        if oref is None or depth > limit:
            return None
        sec, off = oref
        out = {'__at__': (sec, off), '__tref__': tref}
        for t, nm, child, arr in self.members(tref):
            vals = [self._value(t, child, sec, off + k * (self.size(child) if t == 1 else SIZES.get(t, 0)), depth, limit)
                    for k in range(max(arr, 1))]
            out[nm] = vals if arr else vals[0]
            off += (self.size(child) if t == 1 else SIZES.get(t, 0)) * max(arr, 1)
        return out

    def _ptr(self, sec, off):
        return self.reloc.get((sec, off))

    def _value(self, t, child, sec, off, depth, limit):
        d = self.sections[sec]
        if t == 1:
            return self.read(child, (sec, off), depth + 1, limit)
        if t == 2:
            return self.read(child, self._ptr(sec, off), depth + 1, limit)
        if t == 3:
            n = struct.unpack_from(self.e + 'I', d, off)[0]
            p = self._ptr(sec, off + 4)
            if p is None or n == 0:
                return []
            ms = self.members(child)
            if len(ms) == 1 and ms[0][0] in SCALAR and ms[0][3] == 0:           # an array of plain numbers
                fmt = SCALAR[ms[0][0]]
                return list(struct.unpack_from(self.e + '%d%s' % (n, fmt[1]), self.sections[p[0]], p[1]))
            sz = self.size(child)
            return [self.read(child, (p[0], p[1] + k * sz), depth + 1, limit) for k in range(n)]
        if t == 4:
            n = struct.unpack_from(self.e + 'I', d, off)[0]
            p = self._ptr(sec, off + 4)
            if p is None:
                return []
            return [self.read(child, self._ptr(p[0], p[1] + 4 * k), depth + 1, limit) for k in range(n)]
        if t == 5:
            tp, op = self._ptr(sec, off), self._ptr(sec, off + 4)
            if tp is None or op is None:
                return None
            v = self.read(tp, op, depth + 1, limit)
            if isinstance(v, dict):
                v['__type__'] = self.type_name(tp)
            return v
        if t == 7:
            tp = self._ptr(sec, off)
            n = struct.unpack_from(self.e + 'I', d, off + 4)[0]
            p = self._ptr(sec, off + 8)
            if tp is None or p is None:
                return []
            sz = self.size(tp)
            return [self.read(tp, (p[0], p[1] + k * sz), depth + 1, limit) for k in range(n)]
        if t == 8:
            p = self._ptr(sec, off)
            if p is not None:                                                 # a real pointer: a C string
                s = self.sections[p[0]]
                return s[p[1]:s.index(b'\0', p[1])].decode('latin-1')
            return self.name(struct.unpack_from(self.e + 'I', d, off)[0])
        if t == 9:
            v = struct.unpack_from(self.e + 'I16f', d, off)
            return {'Flags': v[0], 'Position': v[1:4], 'Orientation': v[4:8], 'ScaleShear': v[8:17]}
        if t in SCALAR:
            return struct.unpack_from(self.e + SCALAR[t][1:], d, off)[0]
        return None

    def root(self, limit=64):
        return self.read(self.root_type, self.root_obj, 0, limit)

    # ---- writing (values patched in place: the file keeps its size and layout)

    def field(self, obj, name):
        """(section, offset) of member `name` of a read object."""
        sec, off = obj['__at__']
        for t, nm, child, arr in self.members(obj['__tref__']):
            if nm == name:
                return sec, off
            off += (self.size(child) if t == 1 else SIZES.get(t, 0)) * max(arr, 1)
        raise GrannyError(f'{self.where}: no member {name}')

    def put(self, at, fmt, *values):
        if self.e != '<':
            raise GrannyError(f'{self.where}: a big-endian (Xbox 360) resource is read only; writing is for PC files')
        sec, off = at
        pos = self.section_at[sec] + off
        struct.pack_into(fmt, self.data, pos, *values)
        struct.pack_into(fmt, self.sections[sec], off, *values)

    def array_at(self, obj, name):
        """(section, offset) of the elements of a reference-to-array member."""
        at = self.field(obj, name)
        p = self.reloc.get((at[0], at[1] + 4))
        if p is None:
            raise GrannyError(f'{self.where}: {name} has no elements')
        return p

    def to_bytes(self):
        """The resource with its Granny CRC recomputed."""
        import zlib
        if self.e != '<':
            raise GrannyError(f'{self.where}: a big-endian (Xbox 360) resource is read only; writing is for PC files')
        g0 = self.gr2
        crc = zlib.crc32(bytes(self.data[g0 + self.crc_from:g0 + self.total])) & 0xFFFFFFFF
        struct.pack_into('<I', self.data, g0 + 40, crc)
        return bytes(self.data)


# ------------------------------------------------------------------------------------------------ curves

SCALE_TABLE = [1.4142135, 0.70710677, 0.35355338, 0.35355338, 0.35355338, 0.17677669, 0.17677669, 0.17677669,
               -1.4142135, -0.70710677, -0.35355338, -0.35355338, -0.35355338, -0.17677669, -0.17677669, -0.17677669]
OFFSET_TABLE = [-0.70710677, -0.35355338, -0.53033006, -0.17677669, 0.17677669, -0.17677669, -0.088388346, 0.0,
                0.70710677, 0.35355338, 0.53033006, 0.17677669, -0.17677669, 0.17677669, 0.088388346, -0.0]


def _trunc_scale(v):
    return struct.unpack('<f', struct.pack('<I', (v & 0xFFFF) << 16))[0]


def _quat(a, b, c, scales, offsets, top):
    i1 = ((b & top) >> (bits_of(top) - 2)) | ((c & top) >> (bits_of(top) - 1))      # 2 bits: the dropped component
    i2, i3, i4 = (i1 + 1) & 3, (i1 + 2) & 3, (i1 + 3) & 3
    mask = top - 1
    q = [0.0] * 4
    q[i2] = (a & mask) * scales[i2] + offsets[i2]
    q[i3] = (b & mask) * scales[i3] + offsets[i3]
    q[i4] = (c & mask) * scales[i4] + offsets[i4]
    q[i1] = math.sqrt(max(0.0, 1 - q[i2] ** 2 - q[i3] ** 2 - q[i4] ** 2)) * (-1 if a & top else 1)
    return tuple(q)


def bits_of(top):
    return top.bit_length()


def curve_keys(cd):
    """[(time, values)] of a CurveData variant (dict with __type__), or [] for identity / none."""
    if not cd:
        return []
    t = cd.get('__type__', '')
    if t.endswith('DaIdentity'):
        return []
    if t.endswith('Constant32f'):
        return [(0.0, tuple(cd['Controls']))]
    if t.endswith('DaK32fC32f'):
        k, c = cd['Knots'], cd['Controls']
        if not k:
            return []
        dim = len(c) // len(k)
        return [(k[i], tuple(c[i * dim:(i + 1) * dim])) for i in range(len(k))]
    if t.endswith('D3K16uC16u') or t.endswith('D3K8uC8u'):
        kc = cd['KnotsControls']
        n = len(kc) // 4
        s = _trunc_scale(cd['OneOverKnotScaleTrunc'])
        sc, of = cd['ControlScales'], cd['ControlOffsets']
        return [(kc[i] / s, tuple(kc[n + 3 * i + j] * sc[j] + of[j] for j in range(3))) for i in range(n)]
    if t.endswith(('D3I1K8uC8u', 'D3I1K16uC16u', 'D3I1K32fC32f')):
        kc = cd['KnotsControls']
        n = len(kc) // 2
        # float knots (K32f) are times already unless the header scales them too
        s = _trunc_scale(cd['OneOverKnotScaleTrunc']) if 'OneOverKnotScaleTrunc' in cd else 1.0
        sc, of = cd['ControlScales'], cd['ControlOffsets']
        return [(kc[i] / s, tuple(kc[n + i] * sc[j] + of[j] for j in range(3))) for i in range(n)]
    if t.endswith('D4nK16uC15u') or t.endswith('D4nK8uC7u'):
        kc = cd['KnotsControls']
        n = len(kc) // 4
        wide = t.endswith('D4nK16uC15u')
        top, unit = (0x8000, 1 / 32767) if wide else (0x80, 1 / 127)
        sel = cd['ScaleOffsetTableEntries']
        scales = [SCALE_TABLE[(sel >> (4 * k)) & 15] * unit for k in range(4)]
        offsets = [OFFSET_TABLE[(sel >> (4 * k)) & 15] for k in range(4)]
        s = cd['OneOverKnotScale']
        return [(kc[i] / s, _quat(kc[n + 3 * i], kc[n + 3 * i + 1], kc[n + 3 * i + 2], scales, offsets, top))
                for i in range(n)]
    raise GrannyError(f'curve format {t} is not read')


def shift_position(g, curve, delta, ramp=False):
    """Moves a position curve (a CurveData dict read from g) by delta (3 floats), in place. ramp=False: every key by
    delta. ramp=True: each key by delta times the key's progress from the first key's value to the last one's along
    delta (clamped 0..1) - a part carried from somewhere else to a mount moves only as it arrives there. Constant
    curves and quantized curves (scale / offset) are written back in their own format; returns what was done."""
    t = curve.get('__type__', '')
    if t.endswith('D3Constant32f'):
        c = curve['Controls']
        g.put(g.field(curve, 'Controls'), '<3f', *(c[i] + delta[i] for i in range(3)))
        return 'constant moved'
    quant = t.endswith(('D3K16uC16u', 'D3K8uC8u'))
    if t.endswith(('D3I1K8uC8u', 'D3I1K16uC16u', 'D3I1K32fC32f')) or (quant and not ramp):
        of = curve['ControlOffsets']
        g.put(g.field(curve, 'ControlOffsets'), '<3f', *(of[i] + delta[i] for i in range(3)))
        if ramp:
            return 'offsets moved (one control for all axes: the whole curve moves)'
        return 'offsets moved'
    if quant:
        wide = t.endswith('D3K16uC16u')
        qmax = 65535 if wide else 255
        keys = curve_keys(curve)
        n = len(keys)
        d = math.sqrt(sum(x * x for x in delta)) or 1.0
        u = [x / d for x in delta]
        a = sum(keys[0][1][i] * u[i] for i in range(3))
        b = sum(keys[-1][1][i] * u[i] for i in range(3))
        vals = []
        for _, v in keys:
            w = min(max((sum(v[i] * u[i] for i in range(3)) - a) / (b - a), 0.0), 1.0) if b != a else 1.0
            vals.append([v[i] + delta[i] * w for i in range(3)])
        scales, offsets = [], []
        for i in range(3):
            lo, hi = min(v[i] for v in vals), max(v[i] for v in vals)
            scales.append((hi - lo) / qmax if hi > lo else 0.0)
            offsets.append(lo)
        sec, off = g.array_at(curve, 'KnotsControls')
        size = 2 if wide else 1
        for k, v in enumerate(vals):
            for i in range(3):
                q = round((v[i] - offsets[i]) / scales[i]) if scales[i] else 0
                g.put((sec, off + (n + 3 * k + i) * size), '<H' if wide else '<B', max(0, min(qmax, q)))
        g.put(g.field(curve, 'ControlScales'), '<3f', *scales)
        g.put(g.field(curve, 'ControlOffsets'), '<3f', *offsets)
        return f'{n} keys re-quantized (ramp)'
    if t.endswith('DaK32fC32f'):
        k, c = curve['Knots'], curve['Controls']
        if not k:
            return 'no keys: nothing to move'
        dim = len(c) // len(k)
        keys = curve_keys(curve)
        d = math.sqrt(sum(x * x for x in delta)) or 1.0
        u = [x / d for x in delta]
        a = sum(keys[0][1][i] * u[i] for i in range(min(dim, 3)))
        b = sum(keys[-1][1][i] * u[i] for i in range(min(dim, 3)))
        sec, off = g.array_at(curve, 'Controls')
        for i in range(len(k)):
            # the same per-key weight as the quantized ramp; 1 for every key without ramp
            w = 1.0 if not ramp else (min(max((sum(keys[i][1][j] * u[j] for j in range(min(dim, 3))) - a) / (b - a), 0.0), 1.0)
                                      if b != a else 1.0)
            for j in range(min(dim, 3)):
                g.put((sec, off + 4 * (i * dim + j)), '<f', c[i * dim + j] + delta[j] * w)
        return 'float controls moved' + (' (ramp)' if ramp else '')
    raise GrannyError(f'curve format {t}: moving it is not supported')


def sample(keys, t, default):
    if not keys:
        return default
    if t <= keys[0][0] or len(keys) == 1:
        return keys[0][1]
    for (t0, v0), (t1, v1) in zip(keys, keys[1:]):
        if t <= t1:
            f = (t - t0) / (t1 - t0) if t1 > t0 else 0
            if len(v0) == 4:
                # quaternions: the nearer of q and -q (the same rotation), then back to unit length
                if sum(a * b for a, b in zip(v0, v1)) < 0:
                    v1 = tuple(-x for x in v1)
                q = [a + (b - a) * f for a, b in zip(v0, v1)]
                n = math.sqrt(sum(x * x for x in q)) or 1.0
                return tuple(x / n for x in q)
            return tuple(a + (b - a) * f for a, b in zip(v0, v1))
    return keys[-1][1]


# ------------------------------------------------------------------------------------------------ game copy

def load(arg, opfor=None):
    if os.path.isfile(arg):
        return open(arg, 'rb').read(), arg
    sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'bfbc2-mesh-viewer'))
    import meshview
    import shaderdb
    ix = meshview.Index(shaderdb.find_opfor(opfor))
    name = arg.replace('\\', '/')
    if not name.lower().endswith('.res'):
        name += '.res'
    b = ix.read(name)
    if b is None:
        raise GrannyError(f'{name}: no such file, and in no stock bundle')
    return b, name


def _dump(v, ind=0, depth=6, out=None):
    pad = '  ' * ind
    if isinstance(v, dict):
        for k, x in v.items():
            if k.startswith('__'):
                continue
            if isinstance(x, (dict, list)) and x and ind < depth:
                out.append(f'{pad}{k}:' + (f' ({len(x)})' if isinstance(x, list) else ''))
                _dump(x, ind + 1, depth, out)
            else:
                s = repr(x)
                out.append(f'{pad}{k}: {s[:160]}')
    elif isinstance(v, list):
        if v and not isinstance(v[0], (dict, list)):
            out.append(f'{pad}{repr(v)[:160]}')
        for i, x in enumerate(v[:200]):
            if isinstance(x, (dict, list)):
                out.append(f'{pad}[{i}]')
                _dump(x, ind + 1, depth, out)


def animations(g):
    r = g.root()
    return r.get('Animations') or []


def main(argv):
    args, opts = [], {}
    it = iter(argv[1:])
    for a in it:
        if a == '--ramp':
            opts[a] = True
        elif a.startswith('--'):
            opts[a] = next(it, None)
        else:
            args.append(a)
    if len(args) < 2 or args[0] not in ('dump', 'bones', 'tracks', 'shift'):
        print(__doc__)
        return 1
    b, where = load(args[1], opts.get('--opfor'))
    g = Granny(b, where)
    if args[0] == 'shift':
        if not opts.get('--bone') or not opts.get('--out') or not (opts.get('--rest') or opts.get('--by')):
            raise GrannyError('shift needs --bone, --rest x,y,z or --by x,y,z, and --out')
        print(shift(g, opts['--bone'], opts.get('--rest'), opts.get('--by'), bool(opts.get('--ramp'))))
        os.makedirs(os.path.dirname(os.path.abspath(opts['--out'])), exist_ok=True)
        open(opts['--out'], 'wb').write(g.to_bytes())
        return 0
    if args[0] == 'dump':
        out = []
        _dump(g.root(), 0, int(opts.get('--depth') or 6), out)
        print('\n'.join(out))
        return 0
    if args[0] == 'bones':
        for sk in g.root().get('Skeletons') or []:
            print(f'skeleton {sk.get("Name")}: {len(sk["Bones"])} bones')
            for i, bn in enumerate(sk['Bones']):
                m = bn['InverseWorldTransform']
                inv = [m[k * 4:k * 4 + 4] for k in range(4)]
                pos = _world_position(inv)
                print(f'  {i:3d} {bn["Name"]:24s} parent {bn["ParentIndex"]:4d}  world ({pos[0]:.4f}, {pos[1]:.4f}, {pos[2]:.4f})'
                      f'  local {tuple(round(x, 4) for x in bn["Transform"]["Position"])}')
        return 0
    want = set((opts.get('--bones') or '').split(',')) - {''}
    for an in animations(g):
        dur = an['Duration']
        times = [float(x) for x in opts['--at'].split(',')] if opts.get('--at') else [0.0, dur / 2, dur]
        print(f'animation {an.get("Name")}: duration {dur:.3f} s, time step {an.get("TimeStep")}')
        for tg in an['TrackGroups']:
            for tr in tg['TransformTracks']:
                if want and tr['Name'] not in want:
                    continue
                o, p = tr['OrientationCurve']['CurveData'], tr['PositionCurve']['CurveData']
                ok, pk = curve_keys(o), curve_keys(p)
                print(f'  {tr["Name"]}: orientation {o.get("__type__", "?").replace("CurveDataHeader_", "")} '
                      f'({len(ok)} keys), position {p.get("__type__", "?").replace("CurveDataHeader_", "")} ({len(pk)} keys)')
                for t in times:
                    pv = sample(pk, t, (0.0, 0.0, 0.0))
                    ov = sample(ok, t, (0.0, 0.0, 0.0, 1.0))
                    print(f'     t {t:6.3f}: position ({pv[0]:8.4f}, {pv[1]:8.4f}, {pv[2]:8.4f})  orientation '
                          f'({ov[0]:7.4f}, {ov[1]:7.4f}, {ov[2]:7.4f}, {ov[3]:7.4f})')
    return 0


def shift(g, bone, rest=None, by=None, ramp=False):
    """Moves bone's position track in every animation of g: by a vector, or so that its last key is at `rest`
    (each 'x,y,z'). Returns a report line."""
    out = []
    for an in animations(g):
        for tg in an['TrackGroups']:
            for tr in tg['TransformTracks']:
                if tr['Name'] != bone:
                    continue
                cd = tr['PositionCurve']['CurveData']
                keys = curve_keys(cd)
                last = keys[-1][1] if keys else (0.0, 0.0, 0.0)
                if rest:
                    target = [float(x) for x in rest.split(',')]
                    delta = [target[i] - last[i] for i in range(3)]
                else:
                    delta = [float(x) for x in by.split(',')]
                if not any(abs(x) > 1e-6 for x in delta):
                    out.append(f'{bone}: already there')
                    continue
                what = shift_position(g, cd, delta, ramp)
                after = curve_keys({**g.read(cd['__tref__'], cd['__at__']), '__type__': cd['__type__']})
                out.append(f'{bone}: {what}; last key {tuple(round(x, 4) for x in last)} -> '
                           f'{tuple(round(x, 4) for x in after[-1][1])}, first {tuple(round(x, 4) for x in after[0][1])}')
    if not out:
        raise GrannyError(f'{g.where}: no track {bone}')
    return '\n'.join(out)


def _world_position(inv):
    """The translation of the inverse of a 4x4 (row-vector convention, translation in the last row)."""
    r = [[inv[i][j] for j in range(3)] for i in range(3)]
    t = inv[3][:3]
    # inverse of [R 0; t 1] is [R^T 0; -t R^T 1]
    return tuple(-sum(t[k] * r[j][k] for k in range(3)) for j in range(3))


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except GrannyError as e:
        print(f'error: {e}', file=sys.stderr)
        sys.exit(1)
