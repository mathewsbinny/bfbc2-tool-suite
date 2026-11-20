"""NavKit: reader for Havok binary packfiles (32-bit, little-endian, as embedded in BC2 *_HavokAsset.res).

Generic: objects are found through the packfile's virtual fixups, and their fields are decoded with the
hkClass reflection data stored in the file's __types__ section, so no class layouts are hard-coded.

Usage:
  hkpack.py classes <file>              object counts per class
  hkpack.py dump <file> <class> [n]     decoded fields of the first n objects of a class
"""
import os
import struct
import sys
from collections import Counter

MAGIC = bytes.fromhex('57e0e05710c0c010')

# hkClassMember::Type (Havok 6.x)
T_VOID, T_BOOL, T_CHAR, T_INT8, T_UINT8, T_INT16, T_UINT16, T_INT32, T_UINT32, T_INT64, T_UINT64, \
    T_REAL, T_VECTOR4, T_QUATERNION, T_MATRIX3, T_ROTATION, T_QSTRANSFORM, T_MATRIX4, T_TRANSFORM, \
    T_ZERO, T_POINTER, T_FUNCTIONPOINTER, T_ARRAY, T_INPLACEARRAY, T_ENUM, T_STRUCT, T_SIMPLEARRAY, \
    T_HOMOGENEOUSARRAY, T_VARIANT, T_CSTRING, T_ULONG, T_FLAGS = range(32)
SIMPLE = {T_BOOL: ('<?', 1), T_CHAR: ('<b', 1), T_INT8: ('<b', 1), T_UINT8: ('<B', 1), T_INT16: ('<h', 2),
          T_UINT16: ('<H', 2), T_INT32: ('<i', 4), T_UINT32: ('<I', 4), T_INT64: ('<q', 8), T_UINT64: ('<Q', 8),
          T_REAL: ('<f', 4), T_ULONG: ('<I', 4)}
VEC = {T_VECTOR4: 4, T_QUATERNION: 4, T_MATRIX3: 12, T_ROTATION: 12, T_QSTRANSFORM: 12, T_MATRIX4: 16,
       T_TRANSFORM: 16}


_NATIVE: list = []


def _native():
    """The Rust core with hkpack_parse (required since round RT1: the Python reader was retired), or NativeMissing
    saying how to get it."""
    if not _NATIVE:
        root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        if root not in sys.path:
            sys.path.insert(0, root)
        import suite_paths
        suite_paths.use('core')
        from bfbc2core import native
        _NATIVE.append(native.require('reading Havok packfiles (hkpack, round F3)', 'hkpack_parse'))
    return _NATIVE[0]


class Packfile:
    """A packfile's sections, pointer fixups, objects and class descriptions, read in the suite's Rust core (round
    F3: bc2_native.hkpack_parse over bc2_formats::havok, the reader NavKit's collision code has used since round
    F1a: the same fields, the dicts in the same order; the Python reader it was proven against was retired in round
    RT1, its answers kept in tests\\f3_golden_hkpack.json). A file the core refuses raises ValueError; base, when
    given, must be where the first packfile starts (the core reads that one)."""

    def __init__(self, data, base=None):
        data = bytes(data)
        at = data.find(MAGIC)
        if at < 0:
            raise ValueError('no Havok packfile')
        if base is not None and base != at:
            raise ValueError(f'a packfile at byte {base} is not read (the reader takes the first one, at byte {at})')
        got = _native().hkpack_parse(data)
        if got is None:
            raise ValueError(f'the Havok packfile at byte {at} is cut or damaged (the reader refuses it)')
        base, h, cv, sections, (slots, targets), objects, classes = got
        self.d = data                                         # the attributes in the reference's order
        self.base = base
        (_m0, _m1, _tag, self.version, layout, self.nsections, self.contents_sec, self.contents_off) = h
        self.ptr_size = layout & 0xFF
        self.contents_version = cv
        self.sections = [{'name': n, 'start': st, 'local': lo, 'global': gl, 'virtual': vi, 'exports': ex,
                          'imports': im, 'end': en} for n, st, lo, gl, vi, ex, im, en in sections]
        self.ptr = dict(zip(slots, targets))
        self.objects = objects
        self.by_offset = {o: c for o, c in self.objects}
        self.classes = {off: {'name': name, 'parent': parent, 'size': size,
                              'members': [{'name': mn, 'class': mc, 'type': mt, 'subtype': ms, 'carray': ca,
                                           'offset': mo} for mn, mc, mt, ms, ca, mo in members]}
                        for off, name, parent, size, members in classes}
        self.class_by_name = {c['name']: off for off, c in self.classes.items()}

    def cstr(self, off):
        e = self.d.index(b'\0', off)
        return self.d[off:e].decode('ascii', 'replace')

    def deref(self, slot):
        return self.ptr.get(slot)

    # --- hkClass reflection --------------------------------------------------------------------------
    def all_members(self, cname):
        """Members of a class including inherited ones, as (name, member)."""
        off = self.class_by_name.get(cname)
        chain = []
        while off is not None:
            chain.append(self.classes[off])
            off = self.classes[off]['parent']
        out = []
        for c in reversed(chain):
            out.extend(c['members'])
        return out

    def member_class_name(self, m):
        return self.classes[m['class']]['name'] if m['class'] in self.classes else None

    def struct_size(self, cname):
        off = self.class_by_name.get(cname)
        return self.classes[off]['size'] if off is not None else None

    def read_value(self, at, mtype, sub, mclass_name, carray=0):
        d = self.d
        if carray:
            size = self.elem_size(mtype, mclass_name)
            return [self.read_value(at + i * size, mtype, sub, mclass_name) for i in range(carray)]
        if mtype in SIMPLE:
            return struct.unpack_from(SIMPLE[mtype][0], d, at)[0]
        if mtype in VEC:
            return struct.unpack_from(f'<{VEC[mtype]}f', d, at)
        if mtype in (T_ENUM, T_FLAGS):
            return struct.unpack_from({T_INT8: '<b', T_UINT8: '<B', T_INT16: '<h', T_UINT16: '<H'}.get(sub, '<i'),
                                      d, at)[0]
        if mtype == T_POINTER:
            target = self.deref(at)
            return ('ptr', target, self.by_offset.get(target)) if target else None
        if mtype == T_CSTRING:
            t = self.deref(at)
            return self.cstr(t) if t else None
        if mtype == T_STRUCT:
            return self.read_object(at, mclass_name)
        if mtype in (T_ARRAY, T_SIMPLEARRAY):
            data_p = self.deref(at)
            n = struct.unpack_from('<i', d, at + 4)[0]
            if not data_p or n <= 0:
                return []
            size = self.elem_size(sub, mclass_name)
            if size is None:
                return [('?', n)]
            return [self.read_value(data_p + i * size, sub, 0, mclass_name) for i in range(n)]
        return None

    def elem_size(self, t, mclass_name):
        if t in SIMPLE:
            return SIMPLE[t][1]
        if t in VEC:
            return VEC[t] * 4
        if t in (T_POINTER, T_CSTRING, T_ULONG):
            return 4
        if t == T_STRUCT:
            return self.struct_size(mclass_name)
        return None

    def read_object(self, at, cname):
        out = {}
        for m in self.all_members(cname):
            if m['type'] in (T_ZERO, T_VOID, T_FUNCTIONPOINTER):
                continue
            out[m['name']] = self.read_value(at + m['offset'], m['type'], m['subtype'], self.member_class_name(m),
                                             m['carray'])
        return out

    def objects_of(self, cname):
        return [off for off, c in self.objects if c == cname]


def load(path):
    return Packfile(open(path, 'rb').read())


def short(v, depth=0):
    if isinstance(v, dict):
        return '{' + ', '.join(f'{k}: {short(x, depth + 1)}' for k, x in v.items()) + '}'
    if isinstance(v, list):
        if len(v) > 6:
            return f'[{len(v)} items: ' + ', '.join(short(x, depth + 1) for x in v[:3]) + ', ...]'
        return '[' + ', '.join(short(x, depth + 1) for x in v) + ']'
    if isinstance(v, tuple) and v and v[0] == 'ptr':
        return f'->{v[2] or hex(v[1])}'
    if isinstance(v, tuple):
        return '(' + ', '.join(f'{x:.3f}' if isinstance(x, float) else str(x) for x in v) + ')'
    if isinstance(v, float):
        return f'{v:.4g}'
    return str(v)


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 1
    pf = load(argv[2])
    if argv[1] == 'classes':
        print(f'Havok packfile at 0x{pf.base:X}, version {pf.version}, {pf.contents_version}, '
              f'{len(pf.sections)} sections, {len(pf.classes)} class descriptions')
        for c, n in Counter(c for _, c in pf.objects).most_common():
            print(f'{n:7d}  {c}')
    elif argv[1] == 'dump':
        n = int(argv[4]) if len(argv) > 4 else 2
        for off in pf.objects_of(argv[3])[:n]:
            print(f'@0x{off:X}', short(pf.read_object(off, argv[3])))
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == '__main__':
    _ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    if _ROOT not in sys.path:
        sys.path.insert(0, _ROOT)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('hkpack', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
