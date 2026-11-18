"""Binary DBX files for the BFBC2 suite (bfbc2-dbx-tools/FORMAT.md), read and written by the suite's Rust core.

    f = read('Levels/MP_005.dbx')            # or parse(bytes)
    f.instances                              # list of Element (tag 'instance')
    f.get(element, 'guid')                   # first attribute of that name, or None
    value(element)                           # a value as the XML shows it (text, 'a/b/c', true/false)
    to_xml(data)                             # FORMAT.md section 3, identical to `DbxEdit xml`
    write(f)                                 # a tree as a binary .dbx

Elements keep their kind and raw value bytes, so numbers can be read exactly (numbers(element)).

parse, to_xml (and parse_many / to_xml_many, on all cores) and write run in the Rust core (rust/, module bc2_native:
bc2_formats::dbx). The Python reader and writer were retired on 2026-11-18 (round F1a, rust/BENCHMARKS.md "Retired
Python") after the Rust ones gave their answers on every corpus file; their answers are kept as recorded digests
(core/tests/dbx_golden.json, dbx_golden_corpus.json) that core/tests/test_dbx_native.py and test_dbx_write_native.py
compare with. Without the module (python rust/build_native.py, or --download) these functions raise
bfbc2core.native.NativeMissing saying how to get it.
"""
import contextlib
import gc
import math
import struct
from collections.abc import Iterator, Sequence
from dataclasses import dataclass, field
from decimal import ROUND_HALF_UP, Decimal
from os import PathLike
from types import ModuleType
from typing import cast

CONTAINER, TEXT, NUMBERS = 0xA, 2, 7
MAGIC = b'{binary}'
Bytes = bytes | bytearray | memoryview      # what parse / to_xml take (bytes is passed on without a copy)
WHOLE_NUMBER_FIELDS = {'OriginalHashedWaveName', 'HashedName', 'HashedWaveName', 'OnRoadMaterialNameHashes', 'Hash', 'Id',
                       'CompositeMeshPartNames'}


@dataclass
class Element:
    tag: str
    kind: int
    attributes: list[tuple[str, str]] = field(default_factory=list)
    text: str = ''                         # TEXT
    raw: bytes = b''                       # NUMBERS: count/size varints + values; other kinds: the two bytes
    children: list['Element'] | None = None

    def attr(self, name: str, default: str | None = None) -> str | None:
        for k, v in self.attributes:
            if k == name:
                return v
        return default

    def child(self, name: str) -> 'Element | None':
        """The first direct child with attribute name == name."""
        for c in self.children or ():
            if c.attr('name') == name:
                return c
        return None


@dataclass
class DbxFile:
    strings: list[str]
    roots: list[Element]

    @property
    def partition(self) -> Element:
        return self.roots[0]

    @property
    def instances(self) -> list[Element]:
        return [e for e in cast('list[Element]', self.roots[0].children) if e.tag == 'instance']

    def instance(self, guid: str) -> Element | None:
        g = guid.lower()
        for e in self.instances:
            if (e.attr('guid') or '').lower() == g:
                return e
        return None

    @staticmethod
    def get(e: Element, name: str) -> str | None:
        return e.attr(name)


def is_binary(data: Bytes) -> bool:
    return len(data) > 24 and data[:8] == MAGIC


def _varint(data: bytes, pos: int) -> tuple[int, int]:
    value, shift = 0, 0
    while True:
        b = data[pos]
        pos += 1
        value |= (b & 0x7F) << shift
        if b < 0x80:
            return value, pos
        shift += 7


class DeletedEntry(ValueError):
    """The file is a bundle's zero-byte *deleted* marker: there is nothing to read (it is not a broken file)."""


# ------------------------------------------------------------------------------------------------ the Rust core

# BFBC2_PURE_PYTHON (set, not '' or '0') turns the Rust core off for the readers that still have a Python twin (they
# find the module through _import_native); the DBX reader and writer have none and always need it
PURE_PYTHON_ENV = 'BFBC2_PURE_PYTHON'


def _import_native() -> ModuleType | None:
    """The Rust core: bc2_native built into this folder by rust/build_native.py (or downloaded), or an installed one;
    None when neither is there or it was built from other Rust sources than this checkout's (native.load: then it
    warns once). For the suite's readers that still have a Python twin."""
    from . import native
    return native.load()


_native: ModuleType | None = None


def _rust() -> ModuleType:
    """The Rust core, or NativeMissing saying how to get it."""
    global _native
    if _native is None:
        from . import native
        _native = native.require('the DBX reader and writer (bfbc2core.dbx)', 'dbx_write')
    return _native


def _bytes(data: Bytes) -> bytes:
    return data if type(data) is bytes else bytes(data)


def _check(data: Bytes) -> bytes:
    if not data:
        raise DeletedEntry('empty file: a *deleted* bundle entry (older FbrbTool unpacks wrote them as files)')
    if not is_binary(data):
        raise ValueError('not a binary .dbx file')
    return _bytes(data)


def parse(data: Bytes) -> DbxFile:
    """A binary .dbx file as an element tree (FORMAT.md sections 1-2). Raises DeletedEntry for an empty file and
    ValueError for one that is not a binary .dbx or is damaged."""
    m = _rust()
    return cast(DbxFile, m.dbx_parse(_check(data), Element, DbxFile))


def to_xml(data: Bytes) -> str:
    """FORMAT.md section 3: the readable XML, identical to `DbxEdit xml` (string reading tolerant to a missing
    terminator at the end, as the export is). Raises ValueError for a damaged file."""
    return cast(str, _rust().dbx_to_xml(_bytes(data)))


def parse_many(items: Sequence[Bytes]) -> list[DbxFile | ValueError]:
    """parse() of each file, in order, on all cores; a file that does not parse gives its ValueError (DeletedEntry for
    an empty one) in its place."""
    m = _rust()
    out: list[DbxFile | ValueError | None] = [None] * len(items)
    todo: list[tuple[int, bytes]] = []
    for i, d in enumerate(items):
        try:
            todo.append((i, _check(d)))
        except ValueError as e:
            out[i] = e
    with _no_gc():
        for (i, _), r in zip(todo, m.dbx_parse_many([d for _, d in todo], Element, DbxFile)):
            out[i] = r
    return cast('list[DbxFile | ValueError]', out)


@contextlib.contextmanager
def _no_gc() -> Iterator[None]:
    """The cycle collector paused while many trees are built: they hold no cycles, and its passes over the new
    objects (and, as they are kept, over every earlier tree) took longer than building them (rust/BENCHMARKS.md).
    Pausing it per parse() call does not help: the deferred passes run after it."""
    was = gc.isenabled()
    gc.disable()
    try:
        yield
    finally:
        if was:
            gc.enable()


def to_xml_many(items: Sequence[Bytes]) -> list[str | ValueError]:
    """to_xml() of each file, in order, on all cores; a damaged file gives its ValueError in its place."""
    return cast('list[str | ValueError]', _rust().dbx_to_xml_many([_bytes(d) for d in items]))


def read(path: str | PathLike[str]) -> DbxFile:
    with open(path, 'rb') as f:
        return parse(f.read())


def put_varint(out: bytearray, v: int) -> None:
    while True:
        b = v & 0x7F
        v >>= 7
        if v:
            out.append(b | 0x80)
        else:
            out.append(b)
            return


_put_varint = put_varint  # the earlier private name, kept for callers that still use it


def write(f: DbxFile) -> bytes:
    """A DbxFile (element tree) as a binary .dbx (FORMAT.md sections 1-2): a fresh string table - the empty string
    first, then each string in the order the tree first uses it -, container bytes 0, header value 0. Not
    byte-identical to a game file (DbxEdit keeps a file's own string order), but the same tree: parse(write(f))
    gives f back.

    Takes duck-typed elements (tag, kind, attributes, text, raw, children): strings that are str, attributes as a
    list or tuple of (name, value) pairs, children as any iterable or None, raw bytes as anything bytearray.extend
    takes, any nesting depth. Raises ValueError for an empty tag (it reads back as an end mark), more than 15
    attributes, a kind outside 0-15 or an attribute that is not a pair, TypeError for a tag, attribute or text that is
    not a str (rust/bc2-py writers.rs)."""
    return cast(bytes, _rust().dbx_write(f))


# ------------------------------------------------------------------------------------------------ values

def numbers(e: Element) -> tuple[int, list[bytes]]:
    """(size, [raw big-endian bytes per value]) of a NUMBERS element."""
    count, p = _varint(e.raw, 0)
    size, p = _varint(e.raw, p)
    return size, [e.raw[p + i * size:p + (i + 1) * size] for i in range(count)]


def floats(e: Element) -> list[float]:
    """The values of a 4-byte NUMBERS element read as decimals (e.g. a Transform)."""
    size, vals = numbers(e)
    if size != 4:
        raise ValueError('not 4-byte numbers')
    return [struct.unpack('>f', v)[0] for v in vals]


def _net_g(value: float, precision: int) -> str:
    """.NET's "G<precision>" formatting of a finite double (what "R" builds on): the exact binary value rounded
    to <precision> significant digits, a tie rounded away from zero; scientific notation (E+XX / E-XX) for an
    exponent below -4 or from <precision> up; trailing zeros dropped."""
    if value == 0:
        return '0'   # .NET Framework prints negative zero as 0 too
    d = Decimal(value)                     # exact
    exp = d.adjusted()
    unit = Decimal(1).scaleb(exp - precision + 1)
    r = d.quantize(unit, rounding=ROUND_HALF_UP)
    if r.adjusted() != exp:                # 9.99... rounded up to 10.0...
        exp = r.adjusted()
        r = d.quantize(Decimal(1).scaleb(exp - precision + 1), rounding=ROUND_HALF_UP)
    digits = str(abs(r.scaleb(-cast(int, r.as_tuple().exponent)))).rstrip('0') or '0'
    digits = digits[:precision]
    sign = '-' if r < 0 else ''
    if exp < -4 or exp >= precision:
        m = digits[0] + ('.' + digits[1:] if len(digits) > 1 else '')
        return f"{sign}{m}E{'+' if exp >= 0 else '-'}{abs(exp):02d}"
    if exp >= 0:
        whole = digits[:exp + 1].ljust(exp + 1, '0')
        frac = digits[exp + 1:]
        return sign + whole + ('.' + frac if frac else '')
    return sign + '0.' + '0' * (-exp - 1) + digits


def _net_r_single(bits: bytes) -> str:
    v = struct.unpack('>f', bits)[0]
    s = _net_g(v, 7)
    if struct.unpack('>f', struct.pack('>f', float(s)))[0] != v:
        s = _net_g(v, 9)
    return s


def _net_r_double(raw: bytes) -> str:
    v = struct.unpack('>d', raw)[0]
    if math.isnan(v):
        return 'NaN'
    if math.isinf(v):
        return 'Infinity' if v > 0 else '-Infinity'
    s = _net_g(v, 15)
    if float(s) != v:
        s = _net_g(v, 17)
    return s


def _number_text(raw: bytes, size: int, field_name: str) -> str:
    if size == 4:
        whole = struct.unpack('>i', raw)[0]
        if field_name in WHOLE_NUMBER_FIELDS or (whole >> 24) == 0 or ((whole >> 23) & 0xFF) == 0xFF:
            return str(whole)
        return _net_r_single(raw)
    if size == 8:
        return _net_r_double(raw)
    if len(raw) >= 2:                                   # 2, and other sizes (none known in game files): the first
        return str(raw[0] << 8 | raw[1])                # two bytes as an unsigned 16-bit number, as to_xml / DbxEdit
    return str(int.from_bytes(bytes(raw), 'big'))


def value(e: Element) -> str:
    """A non-container element's value as the XML shows it (unescaped)."""
    if e.kind == TEXT:
        return e.text
    if e.kind == NUMBERS:
        size, vals = numbers(e)
        first = e.attributes[0][1] if e.attributes else ''
        return '/'.join(_number_text(v, size, first) for v in vals)
    if len(e.raw) < 2:
        raise ValueError(f'a {len(e.raw)}-byte bool / byte value (expected 2 bytes)')
    v = e.raw[1]
    return 'false' if v == 0 else 'true' if v == 1 else str(v)
