r"""Xbox 360 -> PC conversion of HavokPhysicsData resources (.res): DICE's wrapper and the Havok 5.5.0 binary
packfile in it. Format: FORMAT.md section 1.

The packfile's objects are converted field by field from the class layouts the game itself carries: the hkClass
initializers in BFBC2Game.exe (read from the user's copy each time, nothing is shipped). The two platforms use the
same layout rules apart from the byte order (pointer size 4, no padding reuse, empty-base optimisation on), so a
field keeps its offset and only its bytes are reversed by the field's width.

    import havok
    classes = havok.classes(r'...\BFBC2Game.exe')
    pc_bytes = havok.convert(console_bytes, classes)

The packfile's walk (convert_packfile; verify's record of each byte's owner, verify_packfile) runs in the suite's
Rust core (bc2_convert::havok, rounds E3g and E3o), which it needs: without the module (or with one built from other
sources) it raises bfbc2core.native.NativeMissing saying how to get it; BFBC2_PURE_PYTHON does not change that. The
walk: from the packfile's header, sections and fixup tables, each object of the data section member by member from
its class layout (parents first), arrays and pointed-to objects followed, every value's bytes reversed by its width
(a byte converted as two widths is refused). Its Python (Packfile, ignorable, verify_reference) was retired on
2026-11-18, after the port had given the same bytes on every BF1943 physics port and Onslaught console physics file
and the same verify answers on every Onslaught file, and passed the full test tier; its results are recorded in
bfbc2-bf1943-import\tests\ports_golden.json and tests\havok_verify_golden.json. classes() and the wrapper's
conversion stay here; classes() scans the executable in the Rust core (round F0: bc2_native.havok_classes). Its
Python scan (classes_reference) was retired in round RT2 against its recorded answers
(bfbc2-console-convert\tests\f0_havok_replay*.json).
"""
import collections
import os
import sys

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

Member = collections.namedtuple('Member', 'name cls type subtype carray flags offset')
Klass = collections.namedtuple('Klass', 'name parent size members')


def classes(exe):
    """{class name: Klass} from the hkClass initializers of the game executable: small functions that push the 12
    constructor arguments (name, parent, size, interfaces, count, enums, count, members, count, defaults,
    attributes, flags), load the static hkClass object into ecx and call the constructor. Members are the static
    24-byte hkClassMember records (name, class, enum, type, subtype, C array size, flags, offset, attributes).
    The executable is scanned in the Rust core (round F0: bc2_native.havok_classes). A file it cannot scan (not
    BFBC2Game.exe, or damaged) raises a ConvertError that is also a bfbc2core.native.Refused; a missing one, the
    OSError of opening it."""
    exe = os.fspath(exe)
    rows = _core().havok_classes(exe)
    if rows is None:
        open(exe, 'rb').close()                            # a missing or unreadable file: its own error
        raise _refused(f'{exe}: no Havok class initializers found (not BFBC2Game.exe, or damaged)', ConvertError)
    return {name: Klass(name, parent, size, [Member(*m) for m in members]) for name, parent, size, members in rows}


def _core():
    """The Rust core (bfbc2core.native.require): NativeMissing, saying how to get it, when it is not there."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('bfbc2-console-convert/havok.py', 'havok_classes')


def _refused(message, base):
    from bfbc2core import native
    return native.refused(message, base)


# ---------------------------------------------------------------- DICE's wrapper

def wrapper_end(b):
    """Converts nothing; the offset of the packfile (= the wrapper's length) in a console file."""
    o = b.find(MAGIC)
    if o < 0:
        raise ConvertError('no Havok packfile in the resource')
    return o


def convert_wrapper(b, o):
    """The wrapper before the packfile at o (FORMAT.md 1.1), console -> PC: in the Rust core (round F2a:
    bc2_convert::havok::convert_wrapper; the same bytes and ConvertError messages). Its Python
    (convert_wrapper_reference) was retired in round RT2 against its recorded answers
    (bfbc2-console-convert\\tests\\f2a_console_replay*.json): where it raised struct.error (a wrapper cut short) or
    read a NaN, a ConvertError that is also a bfbc2core.native.Refused says so."""
    if isinstance(b, bytearray | memoryview):
        b = bytes(b)
    got = None
    if type(b) is bytes and type(o) is int and o >= 0:
        got = require().havok_convert_wrapper(b, o)
    if got is None:
        raise _refused('wrapper: cut short or damaged (a read past the data, or a value the converter does not take)',
                       ConvertError)
    if not got[0]:
        raise ConvertError(got[1])
    return got[1]


# ---------------------------------------------------------------- the packfile (the Rust core)

def require():
    """The Rust core, which walks the packfile (bfbc2core.native.require): NativeMissing, saying how to get it, when
    it is not built or downloaded or was built from other sources. A tool that converts calls this when it starts."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('Havok conversion (bfbc2-console-convert/havok.py)', 'havok_convert_packfile')


_native_classes: list = [None, None]          # [the classes dict last handed to the Rust core, its HavokClasses]


def _rust_classes(klasses):
    """The class layouts as the Rust core's HavokClasses, converted once per dict (the dict is kept with it, so the
    identity check cannot meet another dict at the same address)."""
    if _native_classes[0] is not klasses:
        rows = [(name, k.parent, k.size, [(m.name, m.cls, m.type, m.subtype, m.carray, m.offset) for m in k.members])
                for name, k in klasses.items()]
        try:
            _native_classes[:] = [klasses, require().HavokClasses(rows)]
        except (TypeError, ValueError, OverflowError) as e:
            raise ConvertError(f'class layouts the converter does not take ({e})') from None
    return _native_classes[1]


def _stopped(b, o):
    """Why the walk stopped without a refusal of its own."""
    v = b[o + 40:o + 56]
    if not v.startswith(b'Havok-5.5.0'):
        return f'Havok version {v!r}, not 5.5.0'
    return ('the packfile is not converted: it reads past the end of the data, names a class or member type the '
            'game\'s layouts lack, or nests objects too deeply')


def convert_packfile(b, o, klasses):
    """The buffer b with the packfile at o converted to the PC's byte order (the same length; the bytes before o as
    they are), in the Rust core; ConvertError saying why for a packfile it does not convert."""
    b = bytes(b)
    rk = _rust_classes(klasses)
    try:
        out = require().havok_convert_packfile(b, o, rk)
    except ValueError as e:
        raise ConvertError(str(e)) from None
    if out is None:
        raise ConvertError(_stopped(b, o))
    return out


def convert(b, klasses):
    """A console HavokPhysicsData resource -> the PC one (same size)."""
    b = bytes(b)
    o = wrapper_end(b)
    w = convert_wrapper(b, o)
    out = convert_packfile(b, o, klasses)
    return w + out[o:]


# ---------------------------------------------------------------- verification against PC files

def verify_packfile(console, start, klasses, pc):
    """The packfile at start of console converted and compared with pc from start on: [(offset, reason)] of each
    differing byte, the reason why it may differ although the conversion is right (FORMAT.md 1.3: values the game
    never reads - a 16-bit triangle's padding index, the saved quality type of a motion with no saved motion - or
    rebuilds at load - a collidable's cached bounding volume marked invalid, a triangle subpart's triangleOffset) or
    None. In the Rust core, from the walk's record of each byte's owner, the arrays and the objects; ConvertError
    for a packfile it does not convert or a comparison it cannot answer."""
    console, pc = bytes(console), bytes(pc)
    rk = _rust_classes(klasses)
    try:
        got = require().havok_verify_packfile(console, start, rk, pc)
    except ValueError as e:
        raise ConvertError(str(e)) from None
    if got is None:
        convert_packfile(console, start, klasses)            # the walk's own reason, when it is the walk that stops
        if len(pc) > len(console):
            raise ConvertError(f'the PC file is longer than the console one ({len(pc)} > {len(console)} bytes)')
        raise ConvertError('not verified: a cached bounding volume lies past the end of the data, or a byte lies in '
                           'two bounding volumes that answer differently')
    return got


def verify(console, pc, klasses):
    """Converts console and compares with pc: (exact, {reason: bytes}, [unexplained offsets]) - the wrapper's
    first differing byte, then the packfile's bytes (verify_packfile)."""
    console, pc = bytes(console), bytes(pc)
    o = wrapper_end(console)
    w = convert_wrapper(console, o)
    got = verify_packfile(console, o, klasses, pc)
    why: collections.Counter[str] = collections.Counter()
    bad: list[int] = []
    if w != pc[:o]:
        bad.append(next(i for i in range(o) if i >= len(w) or w[i] != pc[i]))
    for i, r in got:
        if r:
            why[r] += 1
        else:
            bad.append(i)
    return not why and not bad, dict(why), bad
