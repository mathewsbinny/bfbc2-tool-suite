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
import weakref
from collections import Counter
from typing import Any

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
    """The Rust core with hkpack_parse, or None: not built, or BFBC2_PURE_PYTHON set."""
    if not _NATIVE:
        m = None
        try:
            root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
            if root not in sys.path:
                sys.path.insert(0, root)
            import suite_paths
            suite_paths.use('core')
            from bfbc2core import dbx as _dbx
            if os.environ.get(_dbx.PURE_PYTHON_ENV, '') in ('', '0'):
                m = _dbx._import_native()
        except ImportError:
            m = None
        _NATIVE.append(m if m is not None and hasattr(m, 'hkpack_parse') else None)
    return _NATIVE[0]


def use_native(on):
    """The Rust core on (when built, whatever BFBC2_PURE_PYTHON says) or off for this process; True when it runs."""
    _NATIVE.clear()
    if not on:
        _NATIVE.append(None)
        return False
    saved = os.environ.pop('BFBC2_PURE_PYTHON', None)
    try:
        return _native() is not None
    finally:
        if saved is not None:
            os.environ['BFBC2_PURE_PYTHON'] = saved


# Round F4: the packfile behind a handle (bc2_native.HkPackfile): its dicts (sections, ptr, objects, by_offset,
# classes, class_by_name) are built on their first read, in the reference's order; until then deref, cstr, objects_of,
# read_object and read_value ask the handle (the reflection walk in Rust: bc2_formats::havok::reflect). Once a dict was
# read (a caller may have changed it) or d replaced, the Python answers.
_HANDLES: 'weakref.WeakKeyDictionary[Any, Any]' = weakref.WeakKeyDictionary()
_LAZY = ('sections', 'ptr', 'objects', 'by_offset', 'classes', 'class_by_name')


class Packfile:
    """A packfile's sections, pointer fixups, objects and class descriptions. Read in the suite's Rust core when it
    is built (round F3: bc2_formats::havok, the reader NavKit's collision code has used since round F1a; round F4: a
    handle, the dicts built when first read and the reflection walk in Rust, the same fields and values in the same
    order); a file it refuses, a base given, or data that is not bytes is read by the Python (_init_reference, kept
    as the reference and the fallback), so the errors stay this module's. BFBC2_PURE_PYTHON=1 or use_native(False)
    runs the Python only."""

    def __init__(self, data, base=None):
        nat = _native() if base is None and type(data) is bytes else None
        got = nat.hkpack_open(data) if nat is not None and hasattr(nat, 'hkpack_open') else None
        if got is None:
            self._init_reference(data, base)
            return
        h, base, hd, cv = got
        self.d = data                                         # the attributes in the reference's order
        self.base = base
        (_m0, _m1, _tag, self.version, layout, self.nsections, self.contents_sec, self.contents_off) = hd
        self.ptr_size = layout & 0xFF
        self.contents_version = cv
        _HANDLES[self] = h

    def __getattr__(self, name):
        if name in _LAZY and _HANDLES.get(self) is not None:
            self._materialize()
            return self.__dict__[name]
        raise AttributeError(f'{type(self).__name__!r} object has no attribute {name!r}')

    def _materialize(self):
        """The dicts from the handle (hkpack_parse's fields), after the attributes already set; the handle let go."""
        h = _HANDLES.pop(self)
        _base, _h, _cv, sections, (slots, targets), objects, classes = h.fields()
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

    def __getstate__(self):
        if _HANDLES.get(self) is not None:                    # a copy or pickle holds the dicts
            self._materialize()
        return self.__dict__

    def _handle(self):
        """The Rust handle while no dict was read and d is the bytes it read, else None."""
        h = _HANDLES.get(self)
        return h if h is not None and self.__dict__.get('d') is h.data else None

    @classmethod
    def reference(cls, data, base=None):
        """The packfile read by the Python only (the reference)."""
        pf = cls.__new__(cls)
        pf._init_reference(data, base)
        return pf

    def _init_reference(self, data, base=None):
        if base is None:
            base = data.find(MAGIC)
            if base < 0:
                raise ValueError('no Havok packfile')
        self.d = data
        self.base = base
        h = struct.unpack_from('<8I', data, base)
        (_m0, _m1, _tag, self.version, layout, self.nsections, self.contents_sec, self.contents_off) = h
        self.ptr_size = layout & 0xFF
        self.contents_version = data[base + 0x28:base + 0x38].split(b'\0')[0].decode('ascii', 'replace')
        self.sections = []
        for i in range(self.nsections):
            o = base + 0x40 + i * 0x30
            name = data[o:o + 20].split(b'\0')[0].decode()
            start, local, glob, virt, exp, imp, end = struct.unpack_from('<7I', data, o + 20)
            self.sections.append({'name': name, 'start': base + start, 'local': local, 'global': glob,
                                  'virtual': virt, 'exports': exp, 'imports': imp, 'end': end})
        self.ptr = {}      # absolute file offset of a pointer slot -> absolute file offset of its target
        for s in self.sections:
            st = s['start']
            for k in range(s['local'], min(s['global'], len(data) - st - 7), 8):
                src, dst = struct.unpack_from('<2I', data, st + k)
                if src == 0xFFFFFFFF:
                    continue
                self.ptr[st + src] = st + dst
            for k in range(s['global'], min(s['virtual'], len(data) - st - 11), 12):
                src, sec, dst = struct.unpack_from('<3I', data, st + k)
                if src == 0xFFFFFFFF:
                    continue
                self.ptr[st + src] = self.sections[sec]['start'] + dst
        self.objects = []  # (absolute offset, class name)
        for s in self.sections:
            st = s['start']
            for k in range(s['virtual'], min(s['exports'], len(data) - st - 11), 12):
                src, sec, name_off = struct.unpack_from('<3I', data, st + k)
                if src == 0xFFFFFFFF:
                    continue
                cname = self.cstr(self.sections[sec]['start'] + name_off)
                self.objects.append((st + src, cname))
        self.by_offset = {o: c for o, c in self.objects}
        self.classes = {}
        self._read_classes()

    def cstr(self, off):
        h = self._handle()
        if h is not None and type(off) is int and 0 <= off < _U63:
            got = h.cstr(off)
            if got is not None:
                return got
        e = self.d.index(b'\0', off)
        return self.d[off:e].decode('ascii', 'replace')

    def deref(self, slot):
        h = self._handle()
        if h is not None and type(slot) is int and 0 <= slot < _U63:
            return h.deref(slot)
        return self.ptr.get(slot)

    # --- hkClass reflection --------------------------------------------------------------------------
    def _read_classes(self):
        for off, cname in self.objects:
            if cname != 'hkClass':
                continue
            name_p = self.deref(off)
            name = self.cstr(name_p) if name_p else '?'
            parent = self.deref(off + 4)
            size, _nint = struct.unpack_from('<2i', self.d, off + 8)
            members_p = self.deref(off + 24)
            nmembers = struct.unpack_from('<i', self.d, off + 28)[0]
            members = []
            for k in range(nmembers):
                mo = members_p + k * 24
                mname = self.cstr(self.deref(mo))
                mclass = self.deref(mo + 4)
                mtype, msub, carr, flags, moff = struct.unpack_from('<BBhHH', self.d, mo + 12)
                members.append({'name': mname, 'class': mclass, 'type': mtype, 'subtype': msub,
                                'carray': carr, 'offset': moff})
            self.classes[off] = {'name': name, 'parent': parent, 'size': size, 'members': members}
        self.class_by_name = {c['name']: off for off, c in self.classes.items()}

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
        h = self._handle()
        if h is not None and _walkable(at, mclass_name) and _byte(mtype) and _byte(sub) and type(carray) is int \
                and -0x8000 <= carray < 0x8000:
            got = h.read_value(at, mtype, sub, mclass_name, carray)
            if got is not None:
                return got[0]
        return self.read_value_reference(at, mtype, sub, mclass_name, carray)

    def read_value_reference(self, at, mtype, sub, mclass_name, carray=0):
        """read_value in Python (the reference; it reads the dicts)."""
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
        h = self._handle()
        if h is not None and _walkable(at, cname):
            got = h.read_object(at, cname)
            if got is not None:
                return got[0]
        return self.read_object_reference(at, cname)

    def read_object_reference(self, at, cname):
        """read_object in Python (the reference; it reads the dicts)."""
        out = {}
        for m in self.all_members(cname):
            if m['type'] in (T_ZERO, T_VOID, T_FUNCTIONPOINTER):
                continue
            out[m['name']] = self.read_value(at + m['offset'], m['type'], m['subtype'], self.member_class_name(m),
                                             m['carray'])
        return out

    def objects_of(self, cname):
        h = self._handle()
        if h is not None and type(cname) is str:
            return h.objects_of(cname)
        return [off for off, c in self.objects if c == cname]


_U63 = 1 << 63


def _byte(v):
    return type(v) is int and 0 <= v < 256


def _walkable(at, cname):
    """Arguments the handle's walk takes as the Python does: an int offset (within 64 bits) and a str or None class
    name."""
    return type(at) is int and -_U63 <= at < _U63 and (cname is None or type(cname) is str)


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
