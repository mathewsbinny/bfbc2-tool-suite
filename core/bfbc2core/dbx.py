"""Binary DBX reader for the BFBC2 suite, written from bfbc2-dbx-tools/FORMAT.md.

    f = read('Levels/MP_005.dbx')            # or parse(bytes)
    f.instances                              # list of Element (tag 'instance')
    f.get(element, 'guid')                   # first attribute of that name, or None
    value(element)                           # a value as the XML shows it (text, 'a/b/c', true/false)
    to_xml(data)                             # FORMAT.md section 3, identical to `DbxEdit xml`

Elements keep their kind and raw value bytes, so numbers can be read exactly (numbers(element)).

parse and to_xml run in the suite's Rust core (rust/, module bc2_native) when it is built, else in Python here; both
give the same trees, text and errors (core/tests/test_dbx_native.py: every corpus file). BFBC2_PURE_PYTHON=1 (or
use_native(False)) forces the Python reader; backend() says which one runs. parse_many / to_xml_many take many files
at once (on all cores with the Rust core).

write runs in the Rust core too (bc2_native.dbx_write walks the Python tree): the Python writer's bytes, proven on
every corpus file and on edited trees (core/tests/test_dbx_write_native.py); a tree the Rust writer leaves to the
Python one (a shape or value the Python writer writes differently or refuses) is written by _py_write, so the
result is always the Python writer's.
"""
import contextlib
import gc
import math
import os
import struct
from collections.abc import Callable, Iterator, Sequence
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


def _strings(data: bytes, tolerant: bool) -> tuple[list[str], int]:
    area, count = struct.unpack_from('>II', data, 16)
    offsets = struct.unpack_from(f'>{count}I', data, 24)
    base = 24 + 4 * count
    out: list[str] = []
    for off in offsets:
        start = base + off
        end = data.find(b'\0', start)
        if end < 0:
            if not tolerant:
                raise ValueError('string without end')
            end = len(data)
        out.append(data[start:end].decode('utf-8', errors='replace'))
    return out, 24 + area


class DeletedEntry(ValueError):
    """The file is a bundle's zero-byte *deleted* marker: there is nothing to read (it is not a broken file)."""


def _damaged[T](fn: Callable[[bytes], T]) -> Callable[[bytes], T]:
    """A damaged file read by fn raises ValueError (what callers catch for a file that is not a .dbx), never an
    IndexError or struct.error from an index past its table or a read past the end (suite deep scan 2026-11-15, S4)."""
    import functools

    @functools.wraps(fn)
    def wrapper(data: bytes) -> T:
        try:
            return fn(data)
        except (IndexError, struct.error) as e:
            raise ValueError(f'damaged .dbx file ({type(e).__name__}: {e})') from e
    return wrapper


# ------------------------------------------------------------------------------------------------ backend

PURE_PYTHON_ENV = 'BFBC2_PURE_PYTHON'      # set (not '' or '0'): the Python reader even when the Rust core is built


def _import_native() -> ModuleType | None:
    """The Rust core: bc2_native built into this folder by rust/build_native.py (or downloaded), or an installed one;
    None when neither is there or it was built from other Rust sources than this checkout's (native.load: then it
    warns once)."""
    from . import native
    return native.load()


def _load_native() -> ModuleType | None:
    """The Rust core unless BFBC2_PURE_PYTHON is set."""
    if os.environ.get(PURE_PYTHON_ENV, '') not in ('', '0'):
        return None
    return _import_native()


_native = _load_native()


def backend() -> str:
    """'rust' when parse and to_xml run in the Rust core, else 'python'."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """The Rust core on (when it is built, whatever BFBC2_PURE_PYTHON says) or off for this process; returns
    backend()."""
    global _native
    _native = _import_native() if on else None
    return backend()


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
    data = _check(data)
    if _native is not None:
        return cast(DbxFile, _native.dbx_parse(data, Element, DbxFile))
    return _py_parse(data)


def to_xml(data: Bytes) -> str:
    """FORMAT.md section 3: the readable XML, identical to `DbxEdit xml` (string reading tolerant to a missing
    terminator at the end, as the export is). Raises ValueError for a damaged file."""
    data = _bytes(data)
    if _native is not None:
        return cast(str, _native.dbx_to_xml(data))
    return _py_to_xml(data)


def parse_many(items: Sequence[Bytes]) -> list[DbxFile | ValueError]:
    """parse() of each file, in order; a file that does not parse gives its ValueError (DeletedEntry for an empty
    one) in its place. On all cores with the Rust core."""
    out: list[DbxFile | ValueError | None] = [None] * len(items)
    todo: list[tuple[int, bytes]] = []
    for i, d in enumerate(items):
        try:
            todo.append((i, _check(d)))
        except ValueError as e:
            out[i] = e
    with _no_gc():
        if _native is not None:
            for (i, _), r in zip(todo, _native.dbx_parse_many([d for _, d in todo], Element, DbxFile)):
                out[i] = r
        else:
            for i, d in todo:
                try:
                    out[i] = _py_parse(d)
                except ValueError as e:
                    out[i] = e
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
    """to_xml() of each file, in order; a damaged file gives its ValueError in its place. On all cores with the
    Rust core."""
    if _native is not None:
        return cast('list[str | ValueError]', _native.dbx_to_xml_many([_bytes(d) for d in items]))
    out: list[str | ValueError] = []
    for d in items:
        try:
            out.append(_py_to_xml(_bytes(d)))
        except ValueError as e:
            out.append(e)
    return out


# ------------------------------------------------------------------------------------------------ the Python reader

@_damaged
def _py_parse(data: bytes) -> DbxFile:
    """parse() in Python: the reference the Rust core is checked against."""
    data = _check(data)
    strings, pos = _strings(data, tolerant=False)
    roots: list[Element] = []
    stack: list[Element] = []
    n = len(data)
    while pos < n:
        tag, pos = _varint(data, pos)
        if tag == 0:
            if not stack:
                break
            stack.pop()
            continue
        head = data[pos]
        pos += 1
        e = Element(strings[tag], head >> 4)
        for _ in range(head & 0xF):
            k, pos = _varint(data, pos)
            v, pos = _varint(data, pos)
            e.attributes.append((strings[k], strings[v]))
        if e.kind == CONTAINER:
            pos += 1
            e.children = []
            (cast('list[Element]', stack[-1].children) if stack else roots).append(e)
            stack.append(e)
            continue
        if e.kind == TEXT:
            idx, pos = _varint(data, pos)
            e.text = strings[idx]
        elif e.kind == NUMBERS:
            start = pos
            count, pos = _varint(data, pos)
            size, pos = _varint(data, pos)
            end = pos + count * size
            if end > n:
                raise ValueError(f'truncated file: {count} numbers of {size} bytes run past the end')
            pos = end
            e.raw = data[start:end]
        else:
            if pos + 2 > n:
                raise ValueError('truncated file: value cut short')
            e.raw = data[pos:pos + 2]
            pos += 2
        (cast('list[Element]', stack[-1].children) if stack else roots).append(e)
    if stack:
        raise ValueError('truncated file: unclosed container(s)')
    return DbxFile(strings, roots)


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
    gives f back. In the Rust core when it is built (the same bytes; module doc), else _py_write."""
    native = _native
    if native is not None and hasattr(native, 'dbx_write'):
        try:
            return cast(bytes, native.dbx_write(f))
        except Exception:
            pass                       # a tree the Rust writer leaves to the Python one: its bytes or its error
    return _py_write(f)


def _py_write(f: DbxFile) -> bytes:
    """write() in Python: the reference the Rust writer is checked against."""
    index = {'': 0}
    order = ['']

    def sid(s: str) -> int:
        if s not in index:
            index[s] = len(order)
            order.append(s)
        return index[s]

    body = bytearray()

    def emit(e: Element) -> None:
        if len(e.attributes) > 15:
            raise ValueError(f'{e.tag}: {len(e.attributes)} attributes (15 at most)')
        put_varint(body, sid(e.tag))
        body.append((e.kind << 4) | len(e.attributes))
        for k, v in e.attributes:
            put_varint(body, sid(k))
            put_varint(body, sid(v))
        if e.kind == CONTAINER:
            body.append(0)
            for c in e.children or ():
                emit(c)
            body.append(0)
        elif e.kind == TEXT:
            put_varint(body, sid(e.text))
        else:
            body.extend(e.raw)

    for r in f.roots:
        emit(r)
    offsets: list[int]
    offsets, blob = [], bytearray()
    for s in order:
        offsets.append(len(blob))
        blob += s.encode('utf-8') + b'\0'
    area = 4 * len(order) + len(blob)
    head = MAGIC + struct.pack('>4I', 24 + area, 0, area, len(order))
    return bytes(head + struct.pack(f'>{len(order)}I', *offsets) + blob + body)


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


# ------------------------------------------------------------------------------------------------ XML

def _escape(s: str) -> str:
    return s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;').replace('"', '&quot;')


@_damaged
def _py_to_xml(data: bytes) -> str:
    """to_xml() in Python: the reference the Rust core is checked against."""
    strings, pos = _strings(data, tolerant=True)
    out = ['<?xml version="1.0"?>\r\n']
    open_tags: list[str] = []
    n = len(data)
    while pos < n:
        tag, pos = _varint(data, pos)
        if tag == 0:
            if not open_tags:
                break
            closing = open_tags.pop()
            out.append('\t' * len(open_tags) + '</' + closing + '>\r\n')
            continue
        name = strings[tag]
        head = data[pos]
        pos += 1
        kind = head >> 4
        line = ['\t' * len(open_tags), '<', name]
        first = ''
        for i in range(head & 0xF):
            k, pos = _varint(data, pos)
            v, pos = _varint(data, pos)
            if i == 0:
                first = strings[v]
            line.append(' ' + strings[k] + '="' + _escape(strings[v]) + '"')
        if kind == CONTAINER:
            pos += 1
            out.append(''.join(line) + '>\r\n')
            open_tags.append(name)
            continue
        if kind == TEXT:
            idx, pos = _varint(data, pos)
            content = _escape(strings[idx])
            if not content:
                out.append(''.join(line) + ' />\r\n')
                continue
        elif kind == NUMBERS:
            count, pos = _varint(data, pos)
            size, pos = _varint(data, pos)
            vals = [_number_text(data[pos + i * size:pos + i * size + (4 if size == 4 else 8 if size == 8 else 2)], size, first)
                    for i in range(count)]
            pos += count * size
            content = '/'.join(vals)
        else:
            v = data[pos + 1]
            pos += 2
            content = 'false' if v == 0 else 'true' if v == 1 else str(v)
        out.append(''.join(line) + '>' + content + '</' + name + '>\r\n')
    return ''.join(out)
