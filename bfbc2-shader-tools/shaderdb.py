r"""shaderdb.py - read, check, list and merge Battlefield: Bad Company 2's per-level compiled shader databases
(Levels/<LV>/Shaders/Dx9_Single.res, Dx10_Single.res, Dx11_Single.res; FbrbTool types Dx9ShaderDatabase,
Dx10ShaderDatabase, Dx11ShaderDatabase). The format is described in FORMAT.md beside this file.

  python shaderdb.py info <db>
        the sections and their counts, the consistency checks (FORMAT.md section 4), and whether writing the file
        again gives the same bytes
  python shaderdb.py list <db> [filter] [--parts]
        the entries: mesh | shader, their programs and the shader bytes they use (filter: a case-insensitive part
        of the entry's name); --parts lists the mesh-part names instead
  python shaderdb.py textures <db> [filter] [--constants]
        for each entry, the textures its parameter blocks name: sampler register, texture name and (guessed from the
        name's suffix) what it is for; --constants also the named shader constants and their values, and the
        textures the engine binds (heightmap ...)
  python shaderdb.py merge <base db> <donor db>... --need <names.txt | unpacked level folder> -o <out.res>
                     [--all-names]
        the base kept entry for entry; for each needed mesh (or name prefix) the base has no entry for, every donor
        entry of it (all its shader sets) is added with everything it uses (programs, shaders, parameter blocks,
        tables, vertex layouts, fragments, mesh-part names); an entry name more than one donor has is taken from
        the first in the order given. --all-names: also add donor entries the file lacks for meshes it already has
        (other shader sets). The result is read back and checked; a report is written beside it (<out>.merge.txt)
  python shaderdb.py cover <db> <unpacked level folder> [--all]
        the meshes the level's .dbx files use that have no entry in the database (--all: also those that have)
  python shaderdb.py needs <unpacked level folder> [-o names.txt]
        those meshes, one per line (the list merge --need takes; add lines for meshes the folder does not name)
  python shaderdb.py find <names.txt | level folder> [--base <db>]
        for each needed mesh (with --base: each one the base lacks), the game copy's levels whose database has it
  python shaderdb.py roundtrip <db>... [--levels]
        parse and write each database again; the bytes must be identical and the checks pass (--levels: every
        database in the game copy's level bundles)

A <db> is a .res file (FbrbTool unpack), <bundle.fbrb>[:<entry>] or level:<name>[:<entry>] (the game copy's
levels\<name>\level-00.fbrb, Package over Dist). Bundles are read in place; the database of the kind asked for is
taken (the bundle's own Levels/<name>/Shaders/ one when there are several).

Xbox 360 databases (BFBC2's Xbox levels' Levels/<LV>/Shaders/Single.res, Battlefield 1943's; big-endian, FORMAT.md
section 8) are recognised by their first bytes and read by info, list, textures and roundtrip; merging is for files
of one kind only (an Xbox file is never merged with a PC one).

Options: --dx 9|10|11 (default: from the file name, Dx9_* / Dx10_* / Dx11_*; for bundles 9, or the base's kind for
donors), --opfor <Project OPFOR BFBC2 folder> (default: OPFOR_ROOT, or project-opfor-bfbc2 next to the suite).

The tool writes only the file you name and never writes into the game folder. A merged database is a new file:
put it into a copy of the level bundle yourself (FbrbTool unpack / pack).
"""
from __future__ import annotations

import argparse
import hashlib
import os
import re
import struct
import sys
from collections.abc import Callable, Collection, Container, Iterable, Iterator, Sequence
from types import ModuleType
from typing import TYPE_CHECKING, Any, cast

if TYPE_CHECKING:
    from bfbc2core.dbx import Element

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx as _dbx  # noqa: E402
from bfbc2core import paths  # noqa: E402

PROGRAM_SIZE = 0x30        # section 6a record
KEY_SIZE = 0x58            # section 6b record (the program's key)
KEY_SIZES = {117: 0x5c}    # other versions' key sizes: 117 = Battlefield 1943 (Xbox 360), 4 permutation bytes more
LAYOUT_SIZE = 0x59         # section 7 record (GUID + 0x49 bytes)
MAX_PROGRAMS = 0x10000     # entry groups index programs with u16
ZERO_GUID = bytes(16)

Named = tuple[str, bytes]                   # (name, GUID): an entry, fragment or mesh-part name
ProgRef = tuple[str, int]                   # a program while merging: ('old', base index) or ('new', new index)
ShaderKey = tuple[bytes, bytes, bytes, bytes, bytes, bytes]   # a shader by content (Merger._shader_key)
TextureRef = tuple[int, int, str]           # (sampler, cube, texture name)
EntryContents = dict[str, frozenset[tuple[bytes, bytes, ShaderKey, ShaderKey, bytes, bytes]]]
MeshUse = dict[str, tuple[str, list[str]]]  # {lower-case path: (shown name, [partitions using it])}


def swap_guid(g: bytes) -> bytes:
    """A GUID's bytes in the other byte order: the first three fields (u32, u16, u16) reversed, the last 8 bytes
    kept. Xbox 360 databases store GUIDs with big-endian fields, PC databases little-endian (FORMAT.md section 8)."""
    return g[3::-1] + g[5:3:-1] + g[7:5:-1] + g[8:]


class ShaderDbError(Exception):
    """A problem with the input the user can fix (explained in words by errors.guard)."""


# ------------------------------------------------------------------------------------------------ backend
# ShaderDb.parse (round SD1: its dispatch too, bc2_formats::shaderdb::open, as bfbc2.exe shaderdb), pick_database,
# dx_from_name and ShaderDb.to_bytes run in the suite's Rust core (rust/, module bc2_native: bc2_formats::shaderdb)
# when it is built, else in Python here (parse_reference, pick_database_reference, dx_from_name_reference); both give
# the same objects, bytes and messages (tests/test_shaderdb_native.py, tests/test_sd1_native.py; the game tier:
# every database of the game copy and OPFOR's work folders).
# BFBC2_PURE_PYTHON=1 (or use_native(False)) forces Python; backend() says which one runs.

def _with_shaderdb(m: ModuleType | None) -> ModuleType | None:
    """m when it has the shader database reader (a module built before it was added does not)."""
    return m if m is not None and hasattr(m, 'shaderdb_parse') else None


# Round E4b: a database parsed by the Rust core keeps its file in a bc2_native.ShaderDbHandle (`_h` in the instance
# dict) and makes a section's Python list only when the attribute is first read (ShaderDb.__getattr__); the queries
# below (entry_textures, entry_signatures, entry_layouts, coverage, mesh_names, the merge's needed names, the writer)
# ask the handle, which reads a section from the file while it was never made and from the caller's list once it was.
# The handle answers None where the Python would do anything but the plain case; the Python (the *_reference
# functions) then runs, so the answers and errors are always the reference's (tests/test_shaderdb_queries.py).
SECTIONS = ('params', 'bindings', 'regs', 'vs', 'ps', 'programs', 'keys', 'layouts', 'entries', 'fragments', 'parts',
            'groups')
_SECTION_SET = frozenset(SECTIONS)
_RECORD_SECTIONS = ('params', 'bindings', 'regs', 'vs', 'ps', 'programs', 'keys', 'layouts')   # the bulk of a file


def _handle(db: object) -> Any:
    """The Rust handle answering queries about db (made for a ShaderDb built in Python), or None (no Rust core, or not a
    ShaderDb)."""
    if _native is None or not isinstance(db, ShaderDb) or not hasattr(_native, 'ShaderDbHandle'):
        return None
    d = db.__dict__
    h = d.get('_h')
    if h is None:
        h = d['_h'] = _native.ShaderDbHandle()
    return h


def _order(db: ShaderDb) -> bool | None:
    """True / False: big- / little-endian (db.e), None for another struct prefix (the reference handles it)."""
    e = db.e
    return True if e == '>' else False if e == '<' else None


def _load_native() -> ModuleType | None:
    if os.environ.get(_dbx.PURE_PYTHON_ENV, '') not in ('', '0'):
        return None
    return _with_shaderdb(_dbx._import_native())


_native = _load_native()


def backend() -> str:
    """'rust' when databases are read and written in the Rust core, else 'python'."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """The Rust core on (when it is built, whatever BFBC2_PURE_PYTHON says) or off for this process; returns
    backend()."""
    global _native
    _native = _with_shaderdb(_dbx._import_native()) if on else None
    return backend()


# ------------------------------------------------------------------------------------------------ the file model

class Shader:
    """A vertex or pixel shader (sections 4 and 5): GUID, the compiled code (first byte a flag), the indices of its
    parameter block (section 1), binding table (section 2) and register table (section 3), and for vertex shaders
    the input description that follows (Dx9: u32 n + n D3DVERTEXELEMENT9; Dx10/11: u32 n + n bytes, u32 m + m 0x1c
    input elements, u32 k + k semantic names; Xbox 360: u32 n + n 12-byte vertex elements + 16 stream strides), kept
    as raw bytes."""
    __slots__ = ('guid', 'code', 'param', 'binding', 'regs', 'tail')

    def __init__(self, guid: bytes, code: bytes, param: int, binding: int, regs: int, tail: bytes = b'') -> None:
        self.guid, self.code, self.param, self.binding, self.regs, self.tail = guid, code, param, binding, regs, tail

    def write(self, out: bytearray, e: str = '<') -> None:
        out += self.guid
        out += struct.pack(e + 'I', len(self.code))
        out += self.code
        out += struct.pack(e + '3I', self.param, self.binding, self.regs)
        out += self.tail


class ShaderDb:
    """One shader database. Lists of raw records, in file order (FORMAT.md section 3):

    params    section 1: parameter blocks (u32 size included)
    bindings  section 2: binding tables (u16 count included)
    regs      section 3: register tables (u32 count included)
    vs, ps    sections 4, 5: Shader objects
    programs  section 6a: 0x30-byte program records (GUID first; sorted by it)
    keys      section 6b: 0x58-byte program keys, one per program (entry GUID, fragment GUID, layout GUID, flags)
    layouts   section 7: 0x59-byte vertex layouts (GUID first)
    entries   section 8.1: (name, GUID) - name = mesh path + "/" + shader path; GUID = MD5(name)
    fragments section 8.2: (name, GUID) - shader fragments (Systems/...) named by program keys
    parts     section 8.3: (name, GUID) - mesh lod part + technique -> vertex layout GUID
    groups    section 8.4: (GUID, [program indices]) - an entry's programs (u16 each)

    big_endian: an Xbox 360 database (BFBC2's Xbox levels' Shaders/Single.res, Battlefield 1943's): the same
    sections with big-endian numbers and GUIDs, Xenon shader code and Xenon vertex elements (FORMAT.md section 8).
    The raw records (parameter blocks, programs, keys, layouts, GUIDs) are kept in the file's byte order.
    """
    params: list[bytes]
    bindings: list[bytes]
    regs: list[bytes]
    vs: list[Shader]
    ps: list[Shader]
    programs: list[bytes]
    keys: list[bytes]
    layouts: list[bytes]
    entries: list[Named]
    fragments: list[Named]
    parts: list[Named]
    groups: list[tuple[bytes, list[int]]]

    def __init__(self, dx: int, big_endian: bool = False) -> None:
        self.dx = dx
        self.big_endian = big_endian
        self.e = '>' if big_endian else '<'
        self.key_size = KEY_SIZE
        self.label = ''
        self.version = 0
        self.flag = 0
        self.params, self.bindings, self.regs = [], [], []
        self.vs, self.ps = [], []
        self.programs, self.keys, self.layouts = [], [], []
        self.entries, self.fragments, self.parts = [], [], []
        self.groups = []

    def __getattr__(self, name: str) -> list:
        """A section of a database the Rust core read (round E4b): made as a Python list on its first read and kept
        in the instance (later reads are plain attribute reads; the caller's changes to it are what the queries see).
        Once every record section is made, the name sections are made too and the file is let go: the lists hold
        everything, and a database is not held twice (a merge's base and donors)."""
        if name in _SECTION_SET:
            d = self.__dict__
            h = d.get('_h')
            if h is not None and h.has_file:
                got = cast(list, h.materialize(d, name, Shader))
                if all(s in d for s in _RECORD_SECTIONS):
                    for s in SECTIONS:
                        if s not in d:
                            h.materialize(d, s, Shader)
                    if _native is not None and hasattr(_native, 'ShaderDbHandle'):
                        d['_h'] = _native.ShaderDbHandle()      # the queries' handle, without the file
                    else:
                        del d['_h']
                return got
        raise AttributeError(f'{type(self).__name__!r} object has no attribute {name!r}')

    def __getstate__(self) -> dict[str, object]:
        """Every section made (a pickle or copy holds plain lists, never the Rust handle)."""
        for s in SECTIONS:
            getattr(self, s)
        state = dict(self.__dict__)
        state.pop('_h', None)
        return state

    @property
    def platform(self) -> str:
        return 'Xbox 360' if self.big_endian else f'Dx{self.dx}'

    def name_guid(self, name: str) -> bytes:
        """The GUID an entry of this name has: MD5 of the name, in the file's byte order."""
        g = hashlib.md5(name.encode('latin-1')).digest()
        return swap_guid(g) if self.big_endian else g

    def pc_guid(self, g: bytes) -> bytes:
        """A GUID of this file in PC byte order (as a PC database stores the same GUID)."""
        return swap_guid(g) if self.big_endian else g

    # ---- reading

    @staticmethod
    def is_big_endian(data: bytes) -> bool:
        """Does the data look like an Xbox 360 (big-endian) database? The version (120 BFBC2, 117 BF1943) is a small
        number: read little-endian it is huge."""
        return len(data) >= 4 and data[0] == 0 and data[3] != 0

    @classmethod
    def parse(cls, data: bytes, dx: int | None = None, where: str = 'file', big_endian: bool | None = None) -> ShaderDb:
        """big_endian: None = from the data (an Xbox 360 database is read whatever dx says: its vertex shaders have
        one layout; it keeps dx, default 9 - FbrbTool lists it as Dx9ShaderDatabase). Read in the Rust core in one
        call (round SD1: the dispatch below and the reader, bc2_formats::shaderdb::open) when it is built and the
        arguments are plain (bytes, an int or None, a bool or None, a str); else parse_reference's dispatch over
        _parse."""
        db = cls._parse_native(data, dx, where, big_endian)
        if db is not None:
            return db
        return cls._dispatch(data, dx, where, big_endian, cls._parse)

    @classmethod
    def parse_reference(cls, data: bytes, dx: int | None = None, where: str = 'file',
                        big_endian: bool | None = None) -> ShaderDb:
        """parse in Python (the reference): the dispatch and the reader."""
        return cls._dispatch(data, dx, where, big_endian, cls._parse_reference)

    @classmethod
    def _parse_native(cls, data: bytes, dx: int | None, where: str, big_endian: bool | None) -> ShaderDb | None:
        """parse in the Rust core (the database kept in a ShaderDbHandle, its sections made when first read), or
        None: no core, or arguments only the Python takes."""
        m = _native
        if (m is None or not hasattr(m, 'shaderdb_open') or type(data) is not bytes or type(where) is not str
                or not (dx is None or (type(dx) is int and 0 <= dx < 1 << 32))
                or not (big_endian is None or type(big_endian) is bool)):
            return None
        try:
            got = m.shaderdb_open(data, dx, big_endian, where)
        except ValueError as e:
            raise ShaderDbError(str(e)) from None
        if got is None:
            return None
        h, d, big = got
        db = cls(d, big)
        for sec in SECTIONS:                # made from the file when first read (__getattr__)
            del db.__dict__[sec]
        db.__dict__['_h'] = h
        db.version, db.flag = h.version, h.flag
        db.key_size = KEY_SIZES.get(db.version, KEY_SIZE)
        return db

    @classmethod
    def _dispatch(cls, data: bytes, dx: int | None, where: str, big_endian: bool | None,
                  reader: Callable[[bytes, int, str, bool], ShaderDb]) -> ShaderDb:
        """parse's choice of byte order and layout, over a reader (_parse, or _parse_reference)."""
        if len(data) < 9:
            raise ShaderDbError(f'{where} is not a shader database (too short)')
        if big_endian is None:
            big_endian = cls.is_big_endian(data)
        if big_endian:
            try:
                return reader(data, dx or 9, where, True)
            except ShaderDbError as e:
                raise ShaderDbError(f'{where} is not a shader database this tool reads (as Xbox 360: {e})') from e
        layouts = [dx] if dx else [9, 10]
        errors = []
        for d in layouts:
            try:
                return reader(data, d, where, False)
            except ShaderDbError as e:
                errors.append(f'as Dx{d}: {e}')
        raise ShaderDbError(f'{where} is not a shader database this tool reads ({"; ".join(errors)})')

    @classmethod
    def _parse(cls, data: bytes, dx: int, where: str, big_endian: bool = False) -> ShaderDb:
        db = cls(dx, big_endian)
        # the Rust reader for bytes and a kind it takes (round SD1: a float, negative or huge dx went to the handle and
        # raised TypeError / OverflowError where the Python reads it - with the Dx10 layout, as any dx but 9)
        m = _native if type(data) is bytes and type(dx) is int and 0 <= dx < 1 << 32 else None
        if m is not None and hasattr(m, 'ShaderDbHandle'):
            try:
                h = m.ShaderDbHandle.open(data, dx, big_endian)
            except ValueError as e:
                raise ShaderDbError(f'{where}: {e}') from None
            for sec in SECTIONS:                # made from the file when first read (__getattr__)
                del db.__dict__[sec]
            db.__dict__['_h'] = h
            db.version, db.flag = h.version, h.flag
            db.key_size = KEY_SIZES.get(db.version, KEY_SIZE)
            return db
        if m is not None:                        # (other buffers: Python, records keep their type)
            try:
                got = m.shaderdb_parse(data, dx, big_endian, Shader)
            except ValueError as e:
                raise ShaderDbError(f'{where}: {e}') from None
            (db.version, db.flag, db.params, db.bindings, db.regs, db.vs, db.ps, db.programs, db.keys, db.layouts,
             db.entries, db.fragments, db.parts, db.groups) = got
            db.key_size = KEY_SIZES.get(db.version, KEY_SIZE)
            return db
        return cls._parse_reference(data, dx, where, big_endian)

    @classmethod
    def _parse_reference(cls, data: bytes, dx: int, where: str, big_endian: bool = False) -> ShaderDb:
        """The reader in Python (the reference): one layout, one byte order."""
        db = cls(dx, big_endian)
        r = _Reader(data, where, db.e)
        db.version = r.u32()
        db.flag = r.u8()
        db.key_size = KEY_SIZES.get(db.version, KEY_SIZE)
        n = r.count(8)
        for _ in range(n):
            start = r.o
            size = r.u32()
            if size < 4:
                raise ShaderDbError(f'{where}: parameter block at 0x{start:x} has size {size}')
            r.skip(size - 4)
            db.params.append(data[start:r.o])
        n = r.count(6)
        for _ in range(n):
            start = r.o
            k = r.u16()
            r.skip(4 * k + 2)
            db.bindings.append(data[start:r.o])
        n = r.count(4)
        for _ in range(n):
            start = r.o
            k = r.u32()
            r.skip(4 * k)
            db.regs.append(data[start:r.o])
        n = r.count(32)
        for _ in range(n):
            guid, code, refs = r.raw(16), r.blob(), r.refs()
            start = r.o
            if big_endian:
                r.skip(12 * r.u32() + 16)          # Xenon vertex elements, then 16 stream strides (u8)
            elif dx == 9:
                r.skip(8 * r.u32())
            else:
                r.skip(r.u32())
                r.skip(0x1c * r.u32())
                for _ in range(r.count(1)):
                    r.cstr()
            db.vs.append(Shader(guid, code, *refs, tail=data[start:r.o]))
        n = r.count(32)
        for _ in range(n):
            guid, code, refs = r.raw(16), r.blob(), r.refs()
            db.ps.append(Shader(guid, code, *refs))
        n = r.count(PROGRAM_SIZE)
        db.programs = [r.raw(PROGRAM_SIZE) for _ in range(n)]
        n2 = r.count(db.key_size)
        if n2 != n:
            raise ShaderDbError(f'{where}: {n} programs but {n2} program keys')
        db.keys = [r.raw(db.key_size) for _ in range(n2)]
        n = r.count(LAYOUT_SIZE)
        db.layouts = [r.raw(LAYOUT_SIZE) for _ in range(n)]
        for lst in (db.entries, db.fragments, db.parts):
            for _ in range(r.count(17)):
                name = r.cstr()
                lst.append((name, r.raw(16)))
        for _ in range(r.count(20)):
            guid = r.raw(16)
            k = r.u32()
            idx = list(struct.unpack_from(f'{db.e}{k}H', data, r.o)) if r.o + 2 * k <= len(data) else None
            if idx is None:
                raise ShaderDbError(f'{where}: entry group runs past the end of the file')
            r.o += 2 * k
            db.groups.append((guid, idx))
        if r.o != len(data):
            raise ShaderDbError(f'{where}: {len(data) - r.o} bytes after the last section')
        for s in db.vs + db.ps:
            if not (s.param < len(db.params) and s.binding < len(db.bindings) and s.regs < len(db.regs)):
                raise ShaderDbError(f'{where}: a shader names a table past the end of its section')
        return db

    # ---- writing

    def to_bytes(self) -> bytes:
        e = self.e
        h = self.__dict__.get('_h')
        if _native is not None and h is not None and h.has_file and e in ('<', '>'):
            # the sections never made written from the file's records, the others from their lists (None: an item
            # only the Python writer takes - every section is then made and written below)
            got = h.write(self.__dict__, e == '>', self.version, self.flag)
            if got is not None:
                return cast(bytes, got)
        if _native is not None:
            # None: an item only the Python writer takes (not bytes, a name not latin-1, a number out of range)
            got = _native.shaderdb_write(self.version, self.flag, e == '>', self.params, self.bindings, self.regs,
                                         self.vs, self.ps, self.programs, self.keys, self.layouts, self.entries,
                                         self.fragments, self.parts, self.groups)
            if got is not None:
                return cast(bytes, got)
        out = bytearray()
        out += struct.pack(e + 'IB', self.version, self.flag)
        for lst in (self.params, self.bindings, self.regs):
            out += struct.pack(e + 'I', len(lst))
            for rec in lst:
                out += rec
        for shaders in (self.vs, self.ps):
            out += struct.pack(e + 'I', len(shaders))
            for s in shaders:
                s.write(out, e)
        out += struct.pack(e + 'I', len(self.programs))
        out += b''.join(self.programs)
        out += struct.pack(e + 'I', len(self.keys))
        out += b''.join(self.keys)
        out += struct.pack(e + 'I', len(self.layouts))
        out += b''.join(self.layouts)
        for names in (self.entries, self.fragments, self.parts):
            out += struct.pack(e + 'I', len(names))
            for name, guid in names:
                out += name.encode('latin-1') + b'\0' + guid
        out += struct.pack(e + 'I', len(self.groups))
        for guid, idx in self.groups:
            out += guid + struct.pack(f'{e}I{len(idx)}H', len(idx), *idx)
        return bytes(out)

    # ---- views

    def program_refs(self, i: int) -> tuple[int, int, int, int]:
        """(vertex shader, pixel shader, parameter block, parameter block) indices of program i."""
        return struct.unpack_from(self.e + '4I', self.programs[i], 0x20)

    def entry_programs(self) -> dict[bytes, list[int]]:
        """{entry GUID: [program indices]}"""
        return {g: idx for g, idx in self.groups}

    def mesh_names(self) -> set[str]:
        """Mesh paths (lower case) the mesh-part names name: '<mesh>_lod<N>/<part>/<technique>'."""
        h = _handle(self)
        got = h.mesh_names(self.__dict__) if h is not None else None
        if got is not None:
            return cast(set[str], got)
        return self.mesh_names_reference()

    def mesh_names_reference(self) -> set[str]:
        """mesh_names in Python (the reference)."""
        out: set[str] = set()
        for name, _ in self.parts:
            lod = name.rsplit('/', 2)[0] if name.count('/') >= 2 else ''
            m = re.match(r'(.+)_lod\d+$', lod, re.I)
            if m:
                out.add(m.group(1).lower())
        return out

    def check(self) -> list[str]:
        """FORMAT.md section 4: the things that must hold for the game to load the file and find every entry.
        A list of problems (empty = consistent). Checked by the Rust handle (sections never made read from the
        file), else by check_reference."""
        h = _handle(self)
        be = _order(self)
        if h is not None and be is not None and type(self.big_endian) is bool:
            got = h.check(self.__dict__, self.big_endian, be)
            if got is not None:
                return cast(list[str], got)
        return self.check_reference()

    def check_reference(self) -> list[str]:
        """check in Python (the reference)."""
        p = []
        n = len(self.programs)
        if n >= MAX_PROGRAMS:
            p.append(f'{n} programs: entry groups can only index {MAX_PROGRAMS}')
        if len(self.keys) != n:
            p.append(f'{n} programs but {len(self.keys)} keys')
        guids = [r[:16] for r in self.programs]
        if guids != sorted(guids, key=self.pc_guid):       # Xbox 360 files too sort by the PC byte order
            p.append('programs not sorted by GUID')
        if len(set(guids)) != n:
            p.append(f'{n - len(set(guids))} program GUIDs repeated')
        for i in range(n):
            v, f, a, b = self.program_refs(i)
            if v >= len(self.vs) or f >= len(self.ps) or a >= len(self.params) or b >= len(self.params):
                p.append(f'program {i} names a shader or parameter block past the end of its section')
                break
        bad = [nm for nm, g in self.entries if self.name_guid(nm) != g]
        if bad:
            p.append(f'{len(bad)} entry GUIDs are not MD5(name), e.g. {bad[0]}')
        names = [nm for nm, _ in self.entries]
        if len(set(names)) != len(names):
            p.append(f'{len(names) - len(set(names))} entry names repeated')
        entry_g = {g for _, g in self.entries}
        frag_g = {g for _, g in self.fragments}
        lay_g = {r[:16] for r in self.layouts}
        if len(lay_g) != len(self.layouts):
            p.append('vertex layout GUIDs repeated')
        group_g = [g for g, _ in self.groups]
        if group_g != sorted(group_g, key=self.pc_guid):
            p.append('entry groups not sorted by GUID')
        if set(group_g) != entry_g or len(group_g) != len(entry_g):
            p.append(f'entry groups ({len(group_g)}) do not match the entries ({len(entry_g)}) one to one')
        owner: dict[int, bytes] = {}
        for g, idx in self.groups:
            if idx != sorted(idx):
                p.append('an entry group\'s program list is not sorted')
                break
            for i in idx:
                if i >= n:
                    p.append(f'an entry group names program {i} of {n}')
                    break
                if i in owner:
                    p.append(f'program {i} in two entry groups')
                owner[i] = g
        if len(owner) != n:
            p.append(f'{n - len(owner)} programs in no entry group')
        for i, k in enumerate(self.keys):
            if owner.get(i, k[:16]) != k[:16]:
                p.append(f'program {i}\'s key names another entry than its group')
                break
        missing_f = {k[16:32] for k in self.keys} - frag_g - {ZERO_GUID}
        if missing_f:
            p.append(f'{len(missing_f)} fragment GUIDs used by program keys have no fragment name')
        missing_l = ({k[32:48] for k in self.keys} | {g for _, g in self.parts}) - lay_g
        if missing_l:
            p.append(f'{len(missing_l)} vertex layout GUIDs used by keys or mesh parts are not in section 7')
        return p


class _Reader:
    def __init__(self, data: bytes, where: str, e: str = '<') -> None:
        self.d, self.o, self.where = data, 0, where
        self.h, self.i = e + 'H', e + 'I'

    def need(self, n: int) -> None:
        if self.o + n > len(self.d):
            raise ShaderDbError(f'{self.where}: ends inside a record at 0x{self.o:x}')

    def u8(self) -> int:
        self.need(1)
        self.o += 1
        return self.d[self.o - 1]

    def u16(self) -> int:
        self.need(2)
        self.o += 2
        return struct.unpack_from(self.h, self.d, self.o - 2)[0]

    def u32(self) -> int:
        self.need(4)
        self.o += 4
        return struct.unpack_from(self.i, self.d, self.o - 4)[0]

    def count(self, min_size: int) -> int:
        n = self.u32()
        if n * min_size > len(self.d) - self.o:
            raise ShaderDbError(f'{self.where}: count {n} at 0x{self.o - 4:x} does not fit in the file')
        return n

    def skip(self, n: int) -> None:
        self.need(n)
        self.o += n

    def raw(self, n: int) -> bytes:
        self.need(n)
        self.o += n
        return self.d[self.o - n:self.o]

    def blob(self) -> bytes:
        return self.raw(self.u32())

    def refs(self) -> tuple[int, int, int]:
        return self.u32(), self.u32(), self.u32()

    def cstr(self) -> str:
        e = self.d.find(b'\0', self.o)
        if e < 0:
            raise ShaderDbError(f'{self.where}: unterminated name at 0x{self.o:x}')
        s = self.d[self.o:e].decode('latin-1')
        self.o = e + 1
        return s


def dx_from_name(path: str) -> int | None:
    """The kind a file or entry name says: dx9 / dx10 / dx11 in its base name, any case (the Rust core's search,
    bc2_formats::shaderdb::open::dx_in_name, when it is built; else dx_from_name_reference)."""
    got = _dx_from_name_native(path)
    if got is not None:
        return got or None
    return dx_from_name_reference(path)


def dx_from_name_reference(path: str) -> int | None:
    """dx_from_name in Python (the reference)."""
    m = re.search(r'dx(9|10|11)', os.path.basename(path), re.I)
    return int(m.group(1)) if m else None


def _dx_from_name_native(path: str) -> int | None:
    """9, 10, 11 or 0 (none) from the Rust core; None: no core, or a name only the Python takes."""
    if _native is None or type(path) is not str or not hasattr(_native, 'shaderdb_dx_in_name'):
        return None
    return cast('int | None', _native.shaderdb_dx_in_name(os.path.basename(path)))


def find_opfor(given: str | None = None) -> str:
    """Project OPFOR BFBC2's folder: given, else OPFOR_ROOT, else a project-opfor-bfbc2 folder next to the suite
    (only there: bfbc2core.paths.find_opfor with walk=False; NavKit's also looks further up)."""
    try:
        return paths.find_opfor(given, walk=False, suite=SUITE)
    except paths.GameCopyNotFound:
        pass
    raise ShaderDbError('Project OPFOR BFBC2 not found (no game copy): put the suite next to the project-opfor-bfbc2 '
                        'folder, set OPFOR_ROOT, or give --opfor')


def level_bundles(root: str) -> dict[str, str]:
    """{level folder name (lower case): level-00.fbrb path} of the game copy, Package over Dist."""
    out: dict[str, str] = {}
    for top in (os.path.join(root, 'game', 'Dist', 'win32', 'levels'), os.path.join(root, 'game', 'Package', 'levels')):
        if os.path.isdir(top):
            for lv in os.listdir(top):
                p = os.path.join(top, lv, 'level-00.fbrb')
                if os.path.isfile(p):
                    out[lv.lower()] = p
    return out


def bundle_databases(path: str) -> list[tuple[str, str]]:
    """[(entry name, FbrbTool type)] of the shader databases in a bundle."""
    from bfbc2core import fbrb
    return [(e.name, e.type) for e in fbrb.entries(path) if e.type.endswith('ShaderDatabase') and not e.deleted]


def pick_database(path: str, dx: int, entry: str | None = None) -> str:
    """The entry name of the Dx<dx> database of a bundle: the one named, else the only one, else the one in the
    bundle's own level folder (Levels/<folder>/Shaders/). Chosen by the Rust core (round SD1:
    bc2_formats::shaderdb::open::pick_database, bfbc2.exe's) when it is built, else by pick_database_reference."""
    return _pick(path, dx, entry, None)


def _pick(path: str, dx: int, entry: str | None, dbs: list[tuple[str, str]] | None) -> str:
    """pick_database with the bundle's bundle_databases already read (dbs; None: read here)."""
    if dbs is None:
        dbs = bundle_databases(path)
    got = _pick_native(path, dx, entry, dbs)
    if got is not None:
        return got
    return _pick_python(path, dx, entry, dbs)


def pick_database_reference(path: str, dx: int, entry: str | None = None) -> str:
    """pick_database in Python (the reference)."""
    return _pick_python(path, dx, entry, None)


def _pick_native(path: str, dx: int, entry: str | None = None, dbs: list[tuple[str, str]] | None = None) -> str | None:
    """The Rust core's choice (raises its ShaderDbError), or None: no core, or arguments / names only the Python
    takes."""
    if (_native is None or not hasattr(_native, 'shaderdb_pick') or type(path) is not str or type(dx) is not int
            or not 0 <= dx < 1 << 32 or not (entry is None or type(entry) is str)):
        return None
    if dbs is None:
        dbs = bundle_databases(path)
    got = _native.shaderdb_pick(path, dbs, dx, entry, os.path.basename(os.path.dirname(path)))
    if got is None:
        return None
    ok, text = got
    if not ok:
        raise ShaderDbError(text)
    return cast(str, text)


def _pick_python(path: str, dx: int, entry: str | None, dbs_read: list[tuple[str, str]] | None) -> str:
    dbs = [n for n, t in (bundle_databases(path) if dbs_read is None else dbs_read)
           if t.lower() == f'dx{dx}shaderdatabase']
    if entry:
        hit = [n for n in dbs if n.lower() == entry.lower() or n.lower().startswith(entry.lower().rstrip('/') + '/')]
        if len(hit) != 1:
            raise ShaderDbError(f'{path} has {len(hit)} Dx{dx} shader databases matching {entry}: {", ".join(dbs)}')
        return hit[0]
    if len(dbs) == 1:
        return dbs[0]
    lv = os.path.basename(os.path.dirname(path)).lower()
    own = [n for n in dbs if n.lower().startswith(f'levels/{lv}/shaders/')]
    if len(own) == 1:
        return own[0]
    raise ShaderDbError(f'{path} has {len(dbs)} Dx{dx} shader databases; name one (<bundle>:<entry>): ' + ', '.join(dbs))


def resolve(spec: str, dx: int | None = None, opfor: str | None = None) -> tuple[bytes, str, int | None]:
    """(bytes, label, dx) of a database given as: a .res file; <bundle.fbrb>[:<entry or level folder in it>];
    level:<name>[:<entry>] (the game copy's levels/<name>/level-00.fbrb, Package over Dist). Bundles are read in
    place, never written."""
    from bfbc2core import fbrb
    bundle: str | None = None
    entry: str | None = None
    if spec.lower().startswith('level:'):
        name, _, entry = spec[6:].partition(':')
        bundles = level_bundles(find_opfor(opfor))
        if name.lower() not in bundles:
            raise ShaderDbError(f'unknown level {name}: the game copy has no levels\\{name}\\level-00.fbrb')
        bundle = bundles[name.lower()]
    else:
        m = re.match(r'(.+?\.fbrb)(?::(.*))?$', spec, re.I)
        if m and not os.path.isfile(spec):
            bundle, entry = m.group(1), m.group(2)
        elif spec.lower().endswith('.fbrb'):
            bundle = spec
    if bundle is None:
        if not os.path.isfile(spec):
            raise FileNotFoundError(2, 'No such file', spec)
        named = dx_from_name(spec)
        if dx and named and dx != named:
            # Dx10 and Dx11 databases share a layout: a wrong kind would parse, and merge the wrong programs
            raise ShaderDbError(f'{spec} is a Dx{named} database by its name, not Dx{dx}')
        return open(spec, 'rb').read(), spec, dx or named
    if not os.path.isfile(bundle):
        raise FileNotFoundError(2, 'No such file', bundle)
    dx = dx or dx_from_name(entry or '') or 9
    name = pick_database(bundle, dx, entry or None)
    return fbrb.read(bundle, [name])[name], f'{bundle}:{name}', dx


def load(spec: str, dx: int | None = None, opfor: str | None = None) -> tuple[ShaderDb, bytes]:
    data, label, dx = resolve(spec, dx, opfor)
    db = ShaderDb.parse(data, 10 if dx == 11 else dx, where=label)
    if dx:
        db.dx = dx
    db.label = label
    return db, data


LevelDbs = dict[int, 'ShaderDb | ShaderDbError']


def load_levels(levels: Iterable[str], dxs: Sequence[int] = (9, 10, 11), opfor: str | None = None,
                chunk: int = 8) -> Iterator[tuple[str, LevelDbs]]:
    """(level, {dx: ShaderDb, or the ShaderDbError load raised}) for each level, in order: what
    load(f'level:{level}', dx, opfor) gives for each dx, with each level's databases read from its bundle in one pass
    (an fbrb.read decompresses the payload up to the entries it reads: one call per Dx decompressed it once per Dx) and
    `chunk` bundles at a time on all cores (fbrb.read_many). A bundle that cannot be read raises as load does. Only
    `chunk` levels' file bytes are held at once; the databases are made one level at a time, as the caller takes
    them (a caller that keeps every Dx of every level holds three times what a per-Dx loop held)."""
    from bfbc2core import fbrb
    levels = list(levels)
    bundles = level_bundles(find_opfor(opfor)) if levels else {}
    step = max(1, chunk)
    for i in range(0, len(levels), step):
        plans: list[tuple[str, str | None, LevelDbs, dict[int, str]]] = []
        for lv in levels[i:i + step]:
            out: LevelDbs = {}
            names: dict[int, str] = {}
            bundle = bundles.get(lv.lower())
            dbs: list[tuple[str, str]] | None = None       # the bundle's database list, read once for every Dx
            for dx in dxs:
                if bundle is None:
                    out[dx] = ShaderDbError(f'unknown level {lv}: the game copy has no levels\\{lv}\\level-00.fbrb')
                    continue
                if dbs is None:
                    dbs = bundle_databases(bundle)
                try:
                    names[dx] = _pick(bundle, dx, None, dbs)
                except ShaderDbError as e:
                    out[dx] = e
            plans.append((lv, bundle, out, names))
        jobs: list[tuple[str, list[str]]] = [(b, sorted(set(names.values()))) for _, b, _, names in plans
                                             if b is not None and names]
        read = fbrb.read_many(jobs)
        k = 0
        for lv, bundle, out, names in plans:
            if bundle is not None and names:
                data, read[k] = read[k], {}         # each level's bytes let go once its databases are made
                k += 1
                if isinstance(data, Exception):
                    raise data
                for dx, name in names.items():
                    label = f'{bundle}:{name}'
                    try:
                        db = ShaderDb.parse(data[name], 10 if dx == 11 else dx, where=label)
                    except ShaderDbError as e:
                        out[dx] = e
                        continue
                    db.dx, db.label = dx, label
                    out[dx] = db
                data = {}
            yield lv, {dx: out[dx] for dx in dxs}


def load_level(level: str, dxs: Sequence[int] = (9, 10, 11), opfor: str | None = None) -> LevelDbs:
    """load_levels for one level: {dx: ShaderDb or ShaderDbError}, its databases read in one pass."""
    return next(load_levels([level], dxs, opfor))[1]


# ------------------------------------------------------------------------------------------------ names

def split_name(name: str, meshes: Container[str]) -> tuple[str, str]:
    """(mesh, shader) of an entry name '<mesh path>/<shader path>': the longest prefix that is a mesh the database's
    part names know (meshes: lower case), else the prefix up to the first path part with 'mesh' in it, else ('', name)."""
    parts = name.split('/')
    for i in range(len(parts) - 1, 0, -1):
        if '/'.join(parts[:i]).lower() in meshes:
            return '/'.join(parts[:i]), '/'.join(parts[i:])
    for i in range(1, len(parts) - 1):
        if 'mesh' in parts[i].lower():
            return '/'.join(parts[:i + 1]), '/'.join(parts[i + 1:])
    return '', name


def part_mesh(name: str) -> str | None:
    """The mesh (lower case) a mesh-part name belongs to, or None (standalone names such as 'Decal')."""
    if name.count('/') < 2:
        return None
    m = re.match(r'(.+)_lod\d+$', name.rsplit('/', 2)[0], re.I)
    return m.group(1).lower() if m else None


MESH_FIELDS = re.compile(r'mesh', re.I)
NOT_MESH_FIELDS = {'lodsettings', 'shadersets', 'shader', 'emitter', 'asset'}
# tree meshes (forests' and middle-growth groups' TreeMeshEntityData: trees, bushes) draw with database entries too
MESH_TYPES = ('Render.SkinnedMeshAsset', 'Render.RigidMeshAsset', 'Render.CompositeMeshAsset', 'Render.TreeMeshAsset')


class LevelMeshes(MeshUse):
    """{lower-case mesh path: (shown name, [partitions using it])}, plus .unresolved: the same for references to
    partitions outside the folder that are not named like a mesh (shader sets, edge models, blueprints ...)."""
    unresolved: MeshUse | None = None


def level_meshes(folder: str) -> LevelMeshes:
    """The meshes an unpacked level folder uses (a LevelMeshes). A mesh is
    - a partition of the folder whose primary instance is a mesh asset, or
    - the target of a reference from a field with 'Mesh' in its name (Mesh, Meshes1p, Meshes3p, MeshZoom1p,
      WeaponMesh, AnimatedMesh, MeshShaderSet ...): a mesh asset of the folder as above, or, outside the folder,
      a partition whose last path part has 'mesh' in it. Other targets outside the folder go to .unresolved;
      targets in the folder of other types (shader sets, edge models) are left out."""
    from bfbc2core import dbx
    if not os.path.isdir(folder):
        raise FileNotFoundError(2, 'No such folder', folder)
    files = []
    for dp, _, fs in os.walk(folder):
        files += [os.path.join(dp, f) for f in fs if f.lower().endswith('.dbx')]
    if not files:
        raise ShaderDbError(f'{folder} has no .dbx files: give an unpacked level bundle folder (FbrbTool unpack)')
    parts: dict[str, tuple[str | None, str]] = {}
    refs: list[tuple[str, str]] = []
    for path in sorted(files):
        rel = os.path.relpath(path, folder).replace(os.sep, '/')[:-4]
        try:
            f = dbx.read(path)
        except dbx.DeletedEntry:
            continue                    # a deleted bundle entry (older FbrbTool unpacks wrote them as empty files)
        except (OSError, ValueError, IndexError) as e:
            # a damaged or locked partition is an error, not a mesh left out of the merge's needs (suite deep scan
            # 2026-11-15, C8: every failure was skipped, and the merged database then lacked the mesh's entries)
            raise ShaderDbError(f'{path}: cannot read ({type(e).__name__}: {e})') from e
        insts = f.instances
        kind, name = (insts[0].attr('type') if insts else None), rel
        if kind in MESH_TYPES:
            name = next((dbx.value(c) for c in insts[0].children or () if c.attr('name') == 'Name'), None) or rel
        parts[rel.lower()] = (kind, name)
        for inst in insts:
            stack: list[tuple[Element, str | None]] = [(inst, None)]
            while stack:
                el, field = stack.pop()
                for ch in el.children or ():
                    fname = ch.attr('name') if ch.tag != 'item' else field
                    ref = ch.attr('ref')
                    if ref and ref != 'null' and fname and MESH_FIELDS.search(fname) \
                            and fname.lower() not in NOT_MESH_FIELDS and '/' in ref:
                        target = ref.rsplit('/', 1)[0]
                        if not target.lower().startswith('systems/mesh/'):
                            refs.append((target, rel))
                    if ch.children:
                        stack.append((ch, fname))
    found = LevelMeshes()
    unresolved: MeshUse = {}

    def add(to: MeshUse, path: str, shown: str, where: str) -> None:
        e = to.setdefault(path.lower(), (shown, []))
        if where not in e[1]:
            e[1].append(where)

    for rel, (kind, name) in sorted(parts.items()):
        if kind in MESH_TYPES:
            add(found, rel, name, name)
    for target, where in refs:
        low = target.lower()
        if low in parts:
            kind, name = parts[low]
            if kind in MESH_TYPES:
                add(found, low, name, where)
        elif 'mesh' in low.rsplit('/', 1)[-1]:
            add(found, low, target, where)
        else:
            add(unresolved, low, target, where)
    found.unresolved = unresolved
    return found


def read_need(spec: str) -> list[str]:
    """Needed names from a text file (one mesh path or name prefix per line, '#' comments) or a level folder."""
    if os.path.isdir(spec):
        return sorted(level_meshes(spec))
    if not os.path.isfile(spec):
        raise FileNotFoundError(2, 'No such file or folder', spec)
    out = []
    for line in open(spec, encoding='utf-8', errors='replace'):
        line = line.split('#', 1)[0].strip().strip('/')
        if line:
            out.append(line.lower())
    if not out:
        raise ShaderDbError(f'{spec} names nothing (one mesh path per line)')
    return out


def needed(name: str, need: Iterable[str]) -> bool:
    """Does entry name start with one of the needed paths (lower case) as a whole path, or equal it?"""
    low = name.lower()
    return any(low == n or low.startswith(n + '/') for n in need)


def needed_test(need: Iterable[str]) -> Callable[[str], bool]:
    """needed(name, need) for many names against one list: each name's whole-path prefixes (up to each '/', and the
    whole name) looked up in a set of the needed paths. The same answers; the merge's donor scan tested every donor
    entry and mesh-part name against every needed path (2026-11-16 profile: 7.5 of a level's 40 s of shader merging,
    5 million calls of needed)."""
    want = set(need)

    def test(name: str) -> bool:
        low = name.lower()
        if low in want:
            return True
        i = low.find('/')
        while i >= 0:
            if low[:i] in want:
                return True
            i = low.find('/', i + 1)
        return False
    return test


# ------------------------------------------------------------------------------------------------ merging

class Merger:
    """Builds base + donor entries (FORMAT.md section 5). Base records keep their order and indices except
    programs, which are re-sorted by GUID (entry groups are renumbered); new records are appended."""

    def __init__(self, base: ShaderDb, need: Collection[str], all_names: bool = False) -> None:
        self.out = base
        self.need = need
        self.all_names = all_names
        self.in_base = {n for n, c in coverage(base, need).items() if c}   # needed names the base has entries for
        self.part_conflicts: list[str] = []
        self.report: list[str] = []
        self.added: list[tuple[str, str]] = []  # (entry name, donor label)
        self.shared_programs = 0                # donor programs the merged file already has, identical
        self.conflicts: list[str] = []          # donor programs whose GUID the merged file has with other content
        b = base
        self.param_ix = _first_index(b.params)
        self.binding_ix = _first_index(b.bindings)
        self.regs_ix = _first_index(b.regs)
        self.vs_ix = _first_index(self._shader_key(b, s) for s in b.vs)
        self.ps_ix = _first_index(self._shader_key(b, s) for s in b.ps)
        self.prog_ix: dict[bytes, ProgRef] = {p[:16]: ('old', i) for i, p in enumerate(b.programs)}
        self.prog_body = {p[:16]: (p[:0x20], k) for p, k in zip(b.programs, b.keys)}
        self.entry_names = {n for n, _ in b.entries}
        self.frag_g = {g for _, g in b.fragments}
        self.lay_g = {r[:16] for r in b.layouts}
        self.part_names = {n: g for n, g in b.parts}
        self.groups: dict[bytes, list[ProgRef]] = {g: [('old', i) for i in idx] for g, idx in b.groups}
        self.new_programs: list[tuple[bytes, bytes]] = []  # (program bytes with new refs, key)

    @staticmethod
    def _shader_key(db: ShaderDb, s: Shader) -> ShaderKey:
        return (s.guid, s.code, s.tail, db.params[s.param], db.bindings[s.binding], db.regs[s.regs])

    def _table(self, rec: bytes, lst: list[bytes], ix: dict[bytes, int]) -> int:
        i = ix.get(rec)
        if i is None:
            i = ix[rec] = len(lst)
            lst.append(rec)
        return i

    def _shader(self, donor: ShaderDb, s: Shader, lst: list[Shader], ix: dict[ShaderKey, int]) -> int:
        key = self._shader_key(donor, s)
        i = ix.get(key)
        if i is None:
            o = self.out
            new = Shader(s.guid, s.code, self._table(donor.params[s.param], o.params, self.param_ix),
                         self._table(donor.bindings[s.binding], o.bindings, self.binding_ix),
                         self._table(donor.regs[s.regs], o.regs, self.regs_ix), s.tail)
            i = ix[key] = len(lst)
            lst.append(new)
        return i

    def _same_kind(self, donor: ShaderDb, label: str) -> None:
        o = self.out
        if (donor.dx, donor.big_endian, donor.key_size) != (o.dx, o.big_endian, o.key_size):
            raise ShaderDbError(f'{label} is a {donor.platform} database (version {donor.version}) and the base is '
                                f'{o.platform} (version {o.version}): merge files of the same kind')

    def add_donor(self, donor: ShaderDb, label: str) -> None:
        self._same_kind(donor, label)
        o = self.out
        groups = donor.entry_programs()
        frag_names = {g: n for n, g in donor.fragments}
        layouts = {r[:16]: r for r in donor.layouts}
        took = 0
        want = [n for n in self.need if self.all_names or n not in self.in_base]   # names it may add to
        mine = needed_test(want)
        h = _handle(donor)
        picked = h.select_needed(donor.__dict__, want) if h is not None else None
        # the donor's entries and mesh-part names the needed names take: picked by the Rust core (the same test,
        # without making the donor's name lists), else tested here one by one
        cand_entries = picked[0] if picked is not None else [(n, g) for n, g in donor.entries if mine(n)]
        cand_parts = picked[1] if picked is not None else donor.parts
        for name, guid in cand_entries:
            if name in self.entry_names:
                continue
            progs: list[ProgRef] = []
            for pi in groups.get(guid, []):
                prog, key = donor.programs[pi], donor.keys[pi]
                pg = prog[:16]
                if pg in self.prog_ix:
                    if self.prog_body[pg] == (prog[:0x20], key):
                        self.shared_programs += 1
                    else:
                        self.conflicts.append(f'{name}: program {pg.hex()} is in the merged file with other '
                                              f'content (that one kept)')
                    progs.append(self.prog_ix[pg])
                    continue
                v, f, a, b = donor.program_refs(pi)
                nv = self._shader(donor, donor.vs[v], o.vs, self.vs_ix)
                nf = self._shader(donor, donor.ps[f], o.ps, self.ps_ix)
                na = self._table(donor.params[a], o.params, self.param_ix)
                nb = self._table(donor.params[b], o.params, self.param_ix)
                new = prog[:0x20] + struct.pack(o.e + '4I', nv, nf, na, nb)
                self.prog_ix[pg] = ('new', len(self.new_programs))
                self.prog_body[pg] = (prog[:0x20], key)
                progs.append(self.prog_ix[pg])
                self.new_programs.append((new, key))
                fg, lg = key[16:32], key[32:48]
                if fg != ZERO_GUID and fg not in self.frag_g:
                    if fg not in frag_names:
                        raise ShaderDbError(f'{label}: a program of {name} names a fragment the file has no name for')
                    o.fragments.append((frag_names[fg], fg))
                    self.frag_g.add(fg)
                self._layout(lg, layouts, label)
            o.entries.append((name, guid))
            self.entry_names.add(name)
            self.groups[guid] = progs
            self.added.append((name, label))
            took += 1
        parts = 0
        for name, lg in cand_parts:
            pm = part_mesh(name)
            if pm is None or not mine(pm):
                continue
            if name in self.part_names:
                if self.part_names[name] != lg:
                    self.part_conflicts.append(f'{name}: {label} has another vertex layout (the existing one kept)')
                continue
            self._layout(lg, layouts, label)
            o.parts.append((name, lg))
            self.part_names[name] = lg
            parts += 1
        self.report.append(f'{label}: {took} entries, {parts} mesh-part names')

    def clone_entry(self, donor: ShaderDb, entry: str, new_name: str, textures: dict[str, str] | None = None,
                    label: str = 'donor') -> int:
        """Adds donor's entry `entry` again under `new_name`: its programs with new ids (MD5 of the new name and the
        old id: a program id must not repeat), their keys naming the new entry, and copies of their parameter blocks
        with texture names replaced (textures: {old name: new name}; the names sit in zero-padded fields, FORMAT.md
        section 1, so a new name may be as long as the old one plus its padding). The shaders, tables, fragments and
        vertex layouts are shared as add_donor shares them. Returns the number of programs.

        Atomic: everything that can refuse the clone (the entry, its programs' indices and new ids, the texture
        renames, its fragments' names, its vertex layouts) is checked before anything is added - a ShaderDbError
        leaves the merger as it was, so a caller that goes on after one writes no half-cloned entry."""
        self._same_kind(donor, label)
        if new_name in self.entry_names:
            raise ShaderDbError(f'{new_name}: the database has this entry already')
        groups = donor.entry_programs()
        guid_old = dict(donor.entries).get(entry)
        if guid_old is None:
            raise ShaderDbError(f'{label} has no entry {entry}')
        o = self.out
        new_guid = o.name_guid(new_name)
        if new_guid in self.groups:
            raise ShaderDbError(f'{new_name}: the database has an entry group with this name\'s id already')
        frag_names = {g: n for n, g in donor.fragments}
        layouts = {r[:16]: r for r in donor.layouts}
        try:
            renames = {k.encode('latin-1'): v.encode('latin-1') for k, v in (textures or {}).items()}
        except UnicodeEncodeError as ex:
            raise ShaderDbError(f'{new_name}: a texture name is not latin-1 ({ex})') from None
        # every texture to rename must be one the entry uses (else the clone silently keeps the donor's - a case
        # mismatch alone does it), and every program must be whole: checked before anything is added
        seen: dict[bytes, int] = {}
        ids: set[bytes] = set()
        for pi in groups.get(guid_old, []):
            where = f'{new_name}: {label}:{entry} program {pi}'
            if not 0 <= pi < min(len(donor.programs), len(donor.keys)):
                raise ShaderDbError(f'{where}: not in the file')
            if len(donor.programs[pi]) < 0x30 or len(donor.keys[pi]) < 48:
                raise ShaderDbError(f'{where}: a short program or key record')
            v, f, a, b = donor.program_refs(pi)
            if v >= len(donor.vs) or f >= len(donor.ps) or a >= len(donor.params) or b >= len(donor.params):
                raise ShaderDbError(f'{where}: a shader or parameter block index past its table')
            for s in (donor.vs[v], donor.ps[f]):
                if s.param >= len(donor.params) or s.binding >= len(donor.bindings) or s.regs >= len(donor.regs):
                    raise ShaderDbError(f'{where}: a shader\'s table index past its table')
            pg = hashlib.md5(new_name.encode('latin-1') + donor.programs[pi][:16]).digest()
            if pg in self.prog_ix or pg in ids:
                raise ShaderDbError(f'{new_name}: program id {pg.hex()} exists')
            ids.add(pg)
            key = donor.keys[pi]
            fg, lg = key[16:32], key[32:48]
            if fg != ZERO_GUID and fg not in self.frag_g and fg not in frag_names:
                raise ShaderDbError(f'{where}: names a fragment the file has no name for')
            if lg not in self.lay_g and lg not in layouts:
                raise ShaderDbError(f'{label}: a vertex layout it uses is not in the file ({where})')
            retexture(donor.params[a], renames, new_name, seen)
            retexture(donor.params[b], renames, new_name, seen)
        unused = [k.decode('latin-1') for k in renames if not seen.get(k)]
        if unused:
            raise ShaderDbError(f'{new_name}: {label}:{entry} uses no texture named {", ".join(unused)} '
                                f'(names are matched exactly, case included)')
        hits = None
        progs: list[ProgRef] = []
        for pi in groups.get(guid_old, []):
            prog, key = donor.programs[pi], donor.keys[pi]
            pg = hashlib.md5(new_name.encode('latin-1') + prog[:16]).digest()
            v, f, a, b = donor.program_refs(pi)
            nv = self._shader(donor, donor.vs[v], o.vs, self.vs_ix)
            nf = self._shader(donor, donor.ps[f], o.ps, self.ps_ix)
            na = self._table(retexture(donor.params[a], renames, new_name, hits), o.params, self.param_ix)
            nb = self._table(retexture(donor.params[b], renames, new_name, hits), o.params, self.param_ix)
            new = pg + prog[16:0x20] + struct.pack(o.e + '4I', nv, nf, na, nb)
            nkey = new_guid + key[16:]
            self.prog_ix[pg] = ('new', len(self.new_programs))
            self.prog_body[pg] = (new[:0x20], nkey)
            progs.append(self.prog_ix[pg])
            self.new_programs.append((new, nkey))
            fg, lg = key[16:32], key[32:48]
            if fg != ZERO_GUID and fg not in self.frag_g:
                o.fragments.append((frag_names[fg], fg))
                self.frag_g.add(fg)
            self._layout(lg, layouts, label)
        o.entries.append((new_name, new_guid))
        self.entry_names.add(new_name)
        self.groups[new_guid] = progs
        self.added.append((new_name, f'{label}:{entry}'))
        return len(progs)

    def add_part(self, name: str, layout_guid: bytes, donor: ShaderDb, label: str = 'donor') -> None:
        """A mesh-part name -> vertex layout (the layout taken from donor when the database lacks it)."""
        if name in self.part_names:
            if self.part_names[name] != layout_guid:
                raise ShaderDbError(f'{name}: the database has this part with another layout')
            return
        self._layout(layout_guid, {r[:16]: r for r in donor.layouts}, label)
        self.out.parts.append((name, layout_guid))
        self.part_names[name] = layout_guid

    def _layout(self, lg: bytes, layouts: dict[bytes, bytes], label: str) -> None:
        if lg in self.lay_g:
            return
        if lg not in layouts:
            raise ShaderDbError(f'{label}: a vertex layout it uses is not in the file')
        self.out.layouts.append(layouts[lg])
        self.lay_g.add(lg)

    def finish(self) -> ShaderDb:
        o = self.out
        total = len(o.programs) + len(self.new_programs)
        if total >= MAX_PROGRAMS:
            raise ShaderDbError(f'the merged file would have {total} programs; entry groups index at most '
                                f'{MAX_PROGRAMS - 1}: take fewer donor entries')
        allp = [(p, k, ('old', i)) for i, (p, k) in enumerate(zip(o.programs, o.keys))]
        allp += [(p, k, ('new', i)) for i, (p, k) in enumerate(self.new_programs)]
        allp.sort(key=lambda t: o.pc_guid(t[0][:16]))
        where = {t[2]: i for i, t in enumerate(allp)}
        o.programs = [t[0] for t in allp]
        o.keys = [t[1] for t in allp]
        groups = [(g, sorted(where[x] for x in idx)) for g, idx in self.groups.items()]
        groups.sort(key=lambda t: o.pc_guid(t[0]))
        o.groups = groups
        return o


TEXTURE_FIELD = 0x80                    # a texture name's zero-padded field in a parameter block


def retexture(block: bytes, textures: dict[bytes, bytes], where: str, hits: dict[bytes, int] | None = None) -> bytes:
    """A parameter block with texture names replaced in place: each old name - a whole field's text, starting the
    field, NUL-terminated - becomes the new one, padded with zeros to the same span (the old name, its terminator
    and the zeros after it, within the field's 0x80 bytes). The renames are simultaneous: every field is matched
    against the block's original names ({A: B, B: A} swaps them; {A: B, B: C} turns A into B, not C). hits: {old:
    count} is added to (only once every rename fits: a refused block adds nothing)."""
    if not textures:
        return block
    found: list[tuple[int, int, bytes]] = []
    for old in textures:
        at = 0
        while True:
            i = block.find(old + b'\0', at)
            if i < 0:
                break
            at = i + 1
            if i > 0 and block[i - 1] != 0:          # the tail of a longer name: not this texture
                continue
            end = i + len(old)
            while end < len(block) and end < i + TEXTURE_FIELD and block[end] == 0:
                end += 1
            new = textures[old]
            if len(new) + 1 > end - i:
                raise ShaderDbError(f'{where}: texture name {new.decode()} longer than the field of {old.decode()}')
            found.append((i, end, old))
    out = bytearray(block)
    for i, end, old in found:
        new = textures[old]
        out[i:end] = new + b'\0' * (end - i - len(new))
    if hits is not None:
        for _i, _end, old in found:
            hits[old] = hits.get(old, 0) + 1
    return bytes(out)


_retexture = retexture  # the earlier private name, kept for callers that still use it


# ------------------------------------------------------------------------------------------------ parameter blocks

PARAM_HEADER = 0x20                     # FORMAT.md 'Section 1: parameter blocks': size, 5 offsets, 8 count bytes
TEXTURE_RECORD = 0x8c                   # u8 sampler, u8 cube, 2 zero bytes, 0x80-byte name, 8 zero bytes
CONSTANT_RECORD = 0x38                  # 0x20-byte name, u32 hash, u8 register, u8 components, 2 zero, float4
ENGINE_TEXTURE_RECORD = 0x28            # 0x20-byte parameter name, u32 hash, u16 sampler, 2 zero


def _block_native(block: object, e: str) -> bool:
    """Does the Rust core read this block (a bytes block, '<' or '>')? Else the reference does."""
    return _native is not None and type(block) is bytes and e in ('<', '>') and hasattr(_native, 'ShaderDbHandle')


def block_textures(block: bytes, e: str = '<') -> list[TextureRef]:
    """[(sampler, cube, texture name)] of a parameter block (FORMAT.md 'Section 1: parameter blocks'): the texture
    records, in block order. sampler: the shader's sampler register the texture is bound to (s<N>); cube: 1 for a
    cube map (environment maps, sky), else 0. Empty for a block without textures (or one not laid out so). Read by
    the Rust core (bc2_formats::shaderdb::query) for a bytes block; block_textures_reference is the reference."""
    if _block_native(block, e):
        return cast(list[TextureRef], _native.shaderdb_block_textures(block, e == '>'))   # type: ignore[union-attr]
    return block_textures_reference(block, e)


def block_constants(block: bytes, e: str = '<') -> list[tuple[str, int, int, tuple[float, ...]]]:
    """[(name, register, components, (4 floats))] of a parameter block: its named shader constants (c<register>,
    the first `components` of the four values used) with the values the block sets (block_constants_reference in
    Python)."""
    if _block_native(block, e):
        return cast(list[tuple[str, int, int, tuple[float, ...]]],
                    _native.shaderdb_block_constants(block, e == '>'))   # type: ignore[union-attr]
    return block_constants_reference(block, e)


def block_engine_textures(block: bytes, e: str = '<') -> list[tuple[str, int]]:
    """[(parameter name, sampler)] of a parameter block: the textures the engine binds itself (heightmap,
    maskTileMap, DepthBuffer ...), named by parameter, no texture name (block_engine_textures_reference in Python)."""
    if _block_native(block, e):
        return cast(list[tuple[str, int]], _native.shaderdb_block_engine_textures(block, e == '>'))  # type: ignore[union-attr]
    return block_engine_textures_reference(block, e)


def entry_textures(db: ShaderDb, names: Container[str] | None = None) -> dict[str, list[TextureRef]]:
    """{entry name: [(sampler, cube, texture name)]}: the textures the parameter blocks of an entry's programs
    name (both blocks of every program), each (sampler, cube, name) once, sorted by sampler. An entry's programs
    (permutations: lighting, shadows, instancing ...) normally bind the same textures to the same samplers; a
    sampler holding different textures in different programs is listed once per texture. Answered by the Rust
    handle (its entry and group lookups kept between calls, each block read once), else entry_textures_reference."""
    h = _handle(db)
    be = _order(db)
    if h is not None and be is not None:
        got = h.entry_textures(db.__dict__, be, names)
        if got is not None:
            return cast(dict[str, list[TextureRef]], got)
    return entry_textures_reference(db, names)


def entry_signatures(db: ShaderDb, names: Container[str] | None = None) -> dict[str, tuple[tuple[str, ...], int]]:
    """{entry name: (its parameter blocks' constant names, sorted; the number of samplers they bind textures to)}:
    the shader template an entry is built on, as far as the file shows it (OPFOR's wake_shaders.signatures, one
    call). Answered by the Rust handle, else entry_signatures_reference."""
    h = _handle(db)
    be = _order(db)
    if h is not None and be is not None:
        got = h.entry_signatures(db.__dict__, be, names)
        if got is not None:
            return cast(dict[str, tuple[tuple[str, ...], int]], got)
    return entry_signatures_reference(db, names)


def entry_signatures_reference(db: ShaderDb, names: Container[str] | None = None) \
        -> dict[str, tuple[tuple[str, ...], int]]:
    """entry_signatures in Python (the reference: wake_shaders.signatures as it is)."""
    progs = db.entry_programs()
    out: dict[str, tuple[tuple[str, ...], int]] = {}
    cache: dict[int, tuple[set[str], set[int]]] = {}
    for name, g in db.entries:
        if names is not None and name not in names:
            continue
        consts: set[str] = set()
        samplers: set[int] = set()
        for i in progs.get(g, []):
            for k in db.program_refs(i)[2:]:
                if k not in cache:
                    cache[k] = ({c[0] for c in block_constants_reference(db.params[k], db.e)},
                                {t[0] for t in block_textures_reference(db.params[k], db.e)})
                consts |= cache[k][0]
                samplers |= cache[k][1]
        out[name] = (tuple(sorted(consts)), len(samplers))
    return out


def entry_layouts(db: ShaderDb, names: Container[str] | None = None) -> dict[str, set[bytes]]:
    """{entry name: the vertex layout GUIDs (key bytes 32-48) its programs are compiled for}. Answered by the Rust
    handle, else entry_layouts_reference."""
    h = _handle(db)
    if h is not None:
        got = h.entry_layouts(db.__dict__, names)
        if got is not None:
            return cast(dict[str, set[bytes]], got)
    return entry_layouts_reference(db, names)


def entry_layouts_reference(db: ShaderDb, names: Container[str] | None = None) -> dict[str, set[bytes]]:
    """entry_layouts in Python (the reference: OPFOR's wake_shaders.entry_layouts, with names)."""
    progs = db.entry_programs()
    return {name: {db.keys[i][32:48] for i in progs.get(g, [])} for name, g in db.entries
            if names is None or name in names}


def block_textures_reference(block: bytes, e: str = '<') -> list[TextureRef]:
    """[(sampler, cube, texture name)] of a parameter block (FORMAT.md 'Section 1: parameter blocks'): the texture
    records, in block order. sampler: the shader's sampler register the texture is bound to (s<N>); cube: 1 for a
    cube map (environment maps, sky), else 0. Empty for a block without textures (or one not laid out so)."""
    if len(block) < PARAM_HEADER:
        return []
    size, _, _, tex_end, _, _ = struct.unpack_from(e + '6I', block, 0)
    n = block[0x1b]
    if size != len(block) or tex_end != PARAM_HEADER + n * TEXTURE_RECORD or tex_end > size:
        return []
    out = []
    for i in range(n):
        o = PARAM_HEADER + i * TEXTURE_RECORD
        name = block[o + 4:o + 4 + TEXTURE_FIELD].split(b'\0', 1)[0].decode('latin-1')
        out.append((block[o], block[o + 1], name))
    return out


def block_constants_reference(block: bytes, e: str = '<') -> list[tuple[str, int, int, tuple[float, ...]]]:
    """[(name, register, components, (4 floats))] of a parameter block: its named shader constants (c<register>,
    the first `components` of the four values used) with the values the block sets."""
    if len(block) < PARAM_HEADER:
        return []
    size, _, _, start, end, _ = struct.unpack_from(e + '6I', block, 0)
    n = block[0x1c]
    if size != len(block) or end != start + n * CONSTANT_RECORD or end > size:
        return []
    out = []
    for i in range(n):
        o = start + i * CONSTANT_RECORD
        name = block[o:o + 0x20].split(b'\0', 1)[0].decode('latin-1')
        out.append((name, block[o + 0x24], block[o + 0x25], struct.unpack_from(e + '4f', block, o + 0x28)))
    return out


def block_engine_textures_reference(block: bytes, e: str = '<') -> list[tuple[str, int]]:
    """[(parameter name, sampler)] of a parameter block: the textures the engine binds itself (heightmap,
    maskTileMap, DepthBuffer ...), named by parameter, no texture name (0x28-byte records after the constants)."""
    if len(block) < PARAM_HEADER:
        return []
    size, _, _, _, start, end = struct.unpack_from(e + '6I', block, 0)
    n = block[0x1d]
    if size != len(block) or end != start + n * ENGINE_TEXTURE_RECORD or end > size:
        return []
    return [(block[o:o + 0x20].split(b'\0', 1)[0].decode('latin-1'), struct.unpack_from(e + 'H', block, o + 0x24)[0])
            for o in range(start, end, ENGINE_TEXTURE_RECORD)]


def entry_textures_reference(db: ShaderDb, names: Container[str] | None = None) -> dict[str, list[TextureRef]]:
    """{entry name: [(sampler, cube, texture name)]}: the textures the parameter blocks of an entry's programs
    name (both blocks of every program), each (sampler, cube, name) once, sorted by sampler. An entry's programs
    (permutations: lighting, shadows, instancing ...) normally bind the same textures to the same samplers; a
    sampler holding different textures in different programs is listed once per texture."""
    progs = db.entry_programs()
    cache: dict[int, list[TextureRef]] = {}
    out = {}
    for name, g in db.entries:
        if names is not None and name not in names:
            continue
        found: set[TextureRef] = set()
        for i in progs.get(g, []):
            for k in db.program_refs(i)[2:]:
                if k not in cache:
                    cache[k] = block_textures_reference(db.params[k], db.e)
                found.update(cache[k])
        out[name] = sorted(found)
    return out


TEXTURE_KINDS = (('normal', ('_nm', '_n', '_normal', '_norm')), ('specular', ('_sp', '_spec', '_s')),
                 ('gloss', ('_gm', '_g')), ('occlusion', ('_occlusion', '_oc', '_ao')),
                 ('color', ('_c', '_df', '_d', '_color', '_diffuse', '_cm')))


def texture_kind(name: str) -> str:
    """What a texture is for, guessed from its name's suffix (DICE's naming: _C / _Df color, _Nm / _N normal, _Sp
    specular, _Gm gloss mask, _Occlusion / _Oc / _AO occlusion), else ''. The databases do not say: a texture
    record names only the sampler register, and the shaders' constant tables call their samplers sampler_Texture<N>.
    A latin-1 name is answered by the Rust core, any other by texture_kind_reference."""
    if _native is not None and hasattr(_native, 'shaderdb_texture_kind'):
        got = _native.shaderdb_texture_kind(name)
        if got is not None:
            return cast(str, got)
    return texture_kind_reference(name)


def texture_kind_reference(name: str) -> str:
    """texture_kind in Python (the reference)."""
    low = name.lower()
    for kind, ends in TEXTURE_KINDS:
        if low.endswith(ends):
            return kind
    return ''


def part_layouts(db: ShaderDb) -> dict[str, bytes]:
    """{mesh-part name: its vertex layout's 0x49-byte record body} (section 8.3 -> section 7)."""
    lay = {r[:16]: r[16:] for r in db.layouts}
    return {name: lay[g] for name, g in db.parts if g in lay}


def _first_index[T](items: Iterable[T]) -> dict[T, int]:
    ix: dict[T, int] = {}
    for i, it in enumerate(items):
        ix.setdefault(it, i)
    return ix


def entry_contents(db: ShaderDb, names: Container[str] | None = None) -> EntryContents:
    """{entry name: frozenset of its programs by content} - each program as (record up to its indices, key, vertex
    shader, pixel shader, both parameter blocks), shaders with their tables resolved: what an entry draws with,
    independent of where the records sit in the file."""
    progs = db.entry_programs()
    out = {}
    for name, g in db.entries:
        if names is not None and name not in names:
            continue
        items = []
        for i in progs.get(g, []):
            v, f, x, y = db.program_refs(i)
            items.append((db.programs[i][:0x20], db.keys[i], Merger._shader_key(db, db.vs[v]),
                          Merger._shader_key(db, db.ps[f]), db.params[x], db.params[y]))
        out[name] = frozenset(items)
    return out


def verify_merge(merged: ShaderDb, sources: Sequence[tuple[ShaderDb, EntryContents | None]], m: Merger) -> list[str]:
    """Problems (empty = none): every entry of the merged file must draw with exactly what it drew with in its
    source (sources: [(base, its entry_contents before merging)] + [(donor, None)...]); mesh-part names must keep
    their layout."""
    p = []
    got = entry_contents(merged)
    base, base_c = sources[0]
    by_donor: dict[str, set[str]] = {}
    for name, label in m.added:
        by_donor.setdefault(label, set()).add(name)
    expect = dict(cast(EntryContents, base_c))      # the base's: never None
    for db, _ in sources[1:]:
        names = by_donor.get(db.label)
        if names:
            expect.update(entry_contents(db, names))
    if set(got) != set(expect):
        p.append(f'{len(set(got) ^ set(expect))} entries differ between the merged file and its sources')
    bad = [n for n in expect if n in got and got[n] != expect[n]]
    if bad:
        p.append(f'{len(bad)} entries draw with other programs than in their source, e.g. {bad[0]}')
    return p


def merge(base: ShaderDb, donors: Iterable[tuple[ShaderDb, str]], need: Collection[str],
          all_names: bool = False) -> Merger:
    """base: ShaderDb (changed in place); donors: [(ShaderDb, label)]; need: lower-case paths. The Merger."""
    m = Merger(base, need, all_names)
    for db, label in donors:
        m.add_donor(db, label)
    m.finish()
    return m


# ------------------------------------------------------------------------------------------------ commands

def entry_rows(db: ShaderDb) -> list[tuple[str, str, int, int]]:
    """[(mesh, shader, programs, shader bytes)] in file order."""
    meshes = db.mesh_names()
    progs = db.entry_programs()
    rows = []
    for name, g in db.entries:
        mesh, shader = split_name(name, meshes)
        idx = progs.get(g, [])
        vs: set[int] = set()
        ps: set[int] = set()
        for i in idx:
            v, f, _, _ = db.program_refs(i)
            vs.add(v)
            ps.add(f)
        size = sum(len(db.vs[v].code) for v in vs) + sum(len(db.ps[f].code) for f in ps)
        rows.append((mesh, shader, len(idx), size))
    return rows


def cmd_info(a: argparse.Namespace) -> int:
    db, data = load(a.res, a.dx, a.opfor)
    print(f'{db.label}: {db.platform} shader database, {len(data):,} bytes, version {db.version}, flag {db.flag}')
    for label, lst in (('1 parameter blocks', db.params), ('2 binding tables', db.bindings),
                       ('3 register tables', db.regs), ('4 vertex shaders', db.vs), ('5 pixel shaders', db.ps),
                       ('6 programs (and keys)', db.programs), ('7 vertex layouts', db.layouts),
                       ('8.1 entries (mesh + shader)', db.entries), ('8.2 fragments', db.fragments),
                       ('8.3 mesh-part names', db.parts), ('8.4 entry groups', db.groups)):
        print(f'  section {label:30s} {len(lst):7,}')
    code = sum(len(s.code) for s in db.vs) + sum(len(s.code) for s in db.ps)
    print(f'  shader code {code:,} bytes; {len(db.mesh_names())} meshes named by mesh parts')
    same = db.to_bytes() == data
    print(f'  written again: {"identical bytes" if same else "DIFFERENT BYTES"}')
    problems = db.check()
    print('  checks: ' + ('consistent' if not problems else f'{len(problems)} problem(s)'))
    for p in problems:
        print('    ' + p)
    return 0 if same and not problems else 1


def cmd_list(a: argparse.Namespace) -> int:
    db, _ = load(a.res, a.dx, a.opfor)
    flt = (a.filter or '').lower()
    if a.parts:
        n = 0
        for name, g in db.parts:
            if flt in name.lower():
                print(f'{name}  -> layout {g.hex()}')
                n += 1
        print(f'{n} of {len(db.parts)} mesh-part names')
        return 0
    rows = [r for r, (name, _) in zip(entry_rows(db), db.entries) if flt in name.lower()]
    print(f'{"programs":>8} {"bytes":>9}  mesh  |  shader')
    for mesh, shader, n, size in rows:
        print(f'{n:8} {size:9,}  {mesh or "-"}  |  {shader}')
    print(f'{len(rows)} of {len(db.entries)} entries, {len({r[0].lower() for r in rows if r[0]})} meshes')
    return 0


def cmd_textures(a: argparse.Namespace) -> int:
    db, _ = load(a.res, a.dx, a.opfor)
    flt = (a.filter or '').lower()
    tex = entry_textures(db)
    progs = db.entry_programs()
    n = 0
    for name, g in db.entries:
        if flt not in name.lower():
            continue
        n += 1
        print(name)
        for s, cube, t in tex[name]:
            kind = 'cube map' if cube else texture_kind(t)
            print(f'    s{s:<2}  {t}{f"   [{kind}]" if kind else ""}')
        if a.constants:
            seen: set[tuple[str, object]] = set()
            for i in progs.get(g, []):
                for k in db.program_refs(i)[2:]:
                    for cname, _, comps, vals in block_constants(db.params[k], db.e):
                        if (cname, vals[:comps]) not in seen:
                            seen.add((cname, vals[:comps]))
                            print(f'    {cname} = {", ".join(f"{v:g}" for v in vals[:comps])}')
                    for pname, s in block_engine_textures(db.params[k], db.e):
                        if (pname, s) not in seen:
                            seen.add((pname, s))
                            print(f'    s{s:<2}  ({pname}: bound by the engine)')
    print(f'{n} of {len(db.entries)} entries; {len({t for v in tex.values() for _, _, t in v})} textures named in all')
    return 0


def coverage(db: ShaderDb, meshes: Iterable[str]) -> dict[str, int]:
    """{mesh (lower): number of entries} for the given meshes: the entries whose lower-case name is the mesh or
    starts with it and '/'. Counted by the Rust handle (no entry list made), else coverage_reference."""
    if not isinstance(meshes, (list, tuple, set, frozenset, dict)):
        meshes = list(meshes)                  # read once, for either side
    h = _handle(db)
    if h is not None:
        got = h.coverage(db.__dict__, meshes)
        if got is not None:
            return cast(dict[str, int], got)
    return coverage_reference(db, meshes)


def coverage_reference(db: ShaderDb, meshes: Iterable[str]) -> dict[str, int]:
    """coverage in Python (the reference)."""
    count = {m: 0 for m in meshes}
    for name, _ in db.entries:
        low = name.lower()
        parts = low.split('/')
        for i in range(1, len(parts) + 1):
            k = '/'.join(parts[:i])
            if k in count:
                count[k] += 1
    return count


# ------------------------------------------------------------------------------------------------ coverage index
# Round S1: choosing donor levels (OPFOR's WW2 install: the stock levels covering the meshes a level still lacks)
# loaded every stock level's database for each WW2 level, only to count entry names. The index keeps those counts:
# for each stock level's database, {every leading '/'-joined part of an entry's lower-case name: the entries under
# it} - coverage(db, meshes) is then {m: prefixes.get(m, 0)}, for any meshes, without the database.

COVER_VERSION = 2           # the index's layout, rules and key (2: by content, round PR1b-1c): a change makes every entry stale


def cover_prefixes(db: ShaderDb) -> dict[str, int]:
    """{lower-case prefix: entries} of db: for every entry, each '/'-joined leading part of its lower-case name (the
    whole name too) counted once - coverage_reference's count of every possible mesh."""
    count: dict[str, int] = {}
    for name, _ in db.entries:
        parts = name.lower().split('/')
        for i in range(1, len(parts) + 1):
            k = '/'.join(parts[:i])
            count[k] = count.get(k, 0) + 1
    return count


def coverage_from(prefixes: dict[str, int], meshes: Iterable[str]) -> dict[str, int]:
    """coverage(db, meshes) from db's cover_prefixes."""
    return {m: prefixes.get(m, 0) for m in meshes}


def cover_cache_dir() -> str | None:
    """The index's folder: bfbc2core.caches 'shadercover' (BFBC2_SHADERCOVER_CACHE; 0: off)."""
    from bfbc2core import caches
    return caches.folder('shadercover')


_COVER_DIGESTS: dict[tuple[str, int, int], str] = {}


def _bundle_digest(bundle: str) -> str:
    """The SHA-256 of a bundle's bytes, once per process for a file whose size and time stay as they were."""
    st = os.stat(bundle)
    k = (os.path.normcase(os.path.abspath(bundle)), st.st_size, st.st_mtime_ns)
    if k not in _COVER_DIGESTS:
        h = hashlib.sha256()
        with open(bundle, 'rb') as f:
            for chunk in iter(lambda: f.read(1 << 20), b''):
                h.update(chunk)
        _COVER_DIGESTS[k] = h.hexdigest()
    return _COVER_DIGESTS[k]


def _cover_key(bundle: str, dx: int) -> tuple[str, str]:
    """(file name, key) of a level bundle's index entry: the file name by the bundle's path, the key by its content
    (its SHA-256, round PR1b-1c: before, its size and modification time), the Dx and COVER_VERSION. A bundle copied,
    restored or touched with the same bytes keeps its entry; one rewritten in place within the same second with the
    same size no longer keeps a stale one."""
    key = repr((COVER_VERSION, _bundle_digest(bundle), dx))
    name = repr((COVER_VERSION, os.path.normcase(os.path.abspath(bundle)), dx))
    return hashlib.sha1(name.encode('utf-8')).hexdigest()[:24] + '.pkl', key


def _cover_load(folder: str | None, bundle: str, dx: int) -> dict[str, Any] | None:
    import pickle
    if folder is None:
        return None
    try:
        name, key = _cover_key(bundle, dx)
        p = os.path.join(folder, name)
        with open(p, 'rb') as fh:
            got = pickle.load(fh)
        if not isinstance(got, dict) or got.get('key') != key:
            return None
        from bfbc2core import caches
        caches.touch(p)
        return got
    except Exception:                                   # noqa: BLE001 - missing, old or damaged: made again
        return None


def _cover_save(folder: str | None, bundle: str, dx: int, row: dict[str, Any]) -> None:
    import pickle
    if folder is None:
        return
    try:
        from bfbc2core import atomicio, caches
        name, key = _cover_key(bundle, dx)
        os.makedirs(folder, exist_ok=True)
        atomicio.write_bytes(os.path.join(folder, name), pickle.dumps(dict(row, key=key),
                                                                     protocol=pickle.HIGHEST_PROTOCOL))
        caches.maybe_prune('shadercover', folder)
    except OSError:
        pass                                            # only a cache: the next run loads the database again


COVER_STATS = {'index_hits': 0, 'loaded': 0}


def level_cover_prefixes(levels: Iterable[str], dx: int = 9, opfor: str | None = None,
                         cache: bool = True) -> Iterator[tuple[str, dict[str, int] | ShaderDbError]]:
    """(level, cover_prefixes of its Dx<dx> database, or the ShaderDbError load_levels gives for it) for each level,
    in order: from the index where the level's bundle is unchanged since it was indexed, the others loaded
    (load_levels: several bundles at a time) and indexed. A bundle that cannot be read raises as load_levels does."""
    levels = list(levels)
    root = find_opfor(opfor) if levels else ''
    bundles = level_bundles(root) if levels else {}
    folder = cover_cache_dir() if cache else None
    got: dict[str, dict[str, int] | ShaderDbError] = {}
    todo: list[str] = []
    for lv in levels:
        b = bundles.get(lv.lower())
        row = _cover_load(folder, b, dx) if b is not None else None
        if row is None:
            todo.append(lv)
        elif 'error' in row:
            got[lv] = ShaderDbError(row['error'])
            COVER_STATS['index_hits'] += 1
        else:
            got[lv] = row['prefixes']
            COVER_STATS['index_hits'] += 1
    for lv, read in load_levels(todo, [dx], opfor):
        db = read[dx]
        COVER_STATS['loaded'] += 1
        b = bundles.get(lv.lower())
        if isinstance(db, ShaderDbError):
            got[lv] = db
            if b is not None and not str(db).startswith('unknown level'):
                _cover_save(folder, b, dx, {'error': str(db)})
            continue
        got[lv] = prefixes = cover_prefixes(db)
        if b is not None:
            _cover_save(folder, b, dx, {'label': db.label, 'prefixes': prefixes})
    for lv in levels:
        yield lv, got[lv]


def level_coverage(levels: Iterable[str], meshes: Iterable[str], dx: int = 9, opfor: str | None = None,
                   cache: bool = True) -> Iterator[tuple[str, dict[str, int] | ShaderDbError]]:
    """(level, coverage(its Dx<dx> database, meshes)) for each level - what coverage gives on load_levels' databases
    (the ShaderDbError in place of a database load_levels could not make) - through the coverage index
    (level_cover_prefixes): an unchanged stock level's database is not loaded at all."""
    meshes = list(meshes)
    for lv, p in level_cover_prefixes(levels, dx, opfor, cache):
        yield lv, p if isinstance(p, ShaderDbError) else coverage_from(p, meshes)


def donor_levels(levels: Iterable[str], need: Iterable[str], dx: int = 9, opfor: str | None = None,
                 cache: bool = True) -> list[str]:
    """The donor levels for the meshes in need (lower case): in the given order, each level whose database has an
    entry for a needed mesh no earlier donor had - OPFOR's WW2 install's choice (the fewest that cover the most, in
    level order), from the coverage index. Levels without a Dx<dx> database are skipped."""
    need = list(need)
    donors: list[str] = []
    have: set[str] = set()
    for lv, cover in level_coverage(levels, need, dx, opfor, cache):
        if isinstance(cover, ShaderDbError):
            continue
        got = {n for n, c in cover.items() if c} - have
        if got:
            donors.append(lv)
            have |= got
    return donors


def cmd_cover(a: argparse.Namespace) -> int:
    db, _ = load(a.res, a.dx, a.opfor)
    meshes = level_meshes(a.level)
    count = coverage(db, meshes)
    missing = sorted(m for m, n in count.items() if n == 0)
    print(f'{db.label}: {len(meshes)} meshes used by {a.level}; {len(meshes) - len(missing)} have entries, '
          f'{len(missing)} have none')
    for m in sorted(meshes):
        if count[m] == 0 or a.all:
            shown, where = meshes[m]
            print(f'  {"MISSING" if count[m] == 0 else f"{count[m]:7}"}  {shown}   (used by {where[0]}'
                  f'{f" and {len(where) - 1} more" if len(where) > 1 else ""})')
    if meshes.unresolved:
        print(f'not checked: {len(meshes.unresolved)} referenced partitions outside the folder that are not named like '
              f'a mesh (shader sets, edge models ...):')
        for m in sorted(meshes.unresolved):
            shown, where = meshes.unresolved[m]
            print(f'  {shown}   (used by {where[0]}{f" and {len(where) - 1} more" if len(where) > 1 else ""})')
    return 0


def cmd_needs(a: argparse.Namespace) -> int:
    meshes = level_meshes(a.level)
    text = ''.join(f'{meshes[m][0]}\n' for m in sorted(meshes))
    if a.out:
        open(a.out, 'w', encoding='utf-8').write(text)
        print(f'{len(meshes)} meshes written to {a.out}')
    else:
        sys.stdout.write(text)
    return 0


def cmd_roundtrip(a: argparse.Namespace) -> int:
    bad = 0
    specs = list(a.res)
    if a.levels:
        for _lv, path in sorted(level_bundles(find_opfor(a.opfor)).items()):
            specs += [(f'{path}:{name}', int(typ[2:-14])) for name, typ in bundle_databases(path)]
    if not specs:
        raise ShaderDbError('roundtrip needs databases: files, bundles, level:<name>, or --levels')
    for spec in specs:
        path, dx = spec if isinstance(spec, tuple) else (spec, a.dx)
        db, data = load(path, dx, a.opfor)
        same = db.to_bytes() == data
        problems = db.check()
        bad += not same or bool(problems)
        print(f'{"ok  " if same else "FAIL"} {db.label}: Dx{db.dx}, {len(data):,} bytes, {len(db.entries)} entries, '
              f'{len(db.programs)} programs{"" if not problems else "; checks: " + "; ".join(problems)}')
    return 1 if bad else 0


def cmd_merge(a: argparse.Namespace) -> int:
    need = read_need(a.need)
    base, _ = load(a.base, a.dx, a.opfor)
    donors = []
    for spec in a.donors:
        # the donor's own kind (given, or its name's) - a contradiction is refused, not merged; else the base's
        db, _ = load(spec, a.dx or dx_from_name(spec.rsplit(':', 1)[-1]) or base.dx, a.opfor)
        donors.append((db, db.label))
    before = len(base.entries), len(base.programs), len(base.vs), len(base.ps), len(base.params)
    sources = [(base, entry_contents(base))] + [(db, None) for db, _ in donors]
    m = merge(base, donors, need, a.all_names)
    out = base.to_bytes()
    again = ShaderDb.parse(out, 10 if base.dx == 11 else base.dx, where='the merged file')
    problems = again.check() + verify_merge(again, sources, m)
    if again.to_bytes() != out or problems:
        raise ShaderDbError('the merged file does not read back consistently: ' + '; '.join(problems[:5]))
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    open(a.out, 'wb').write(out)
    still = coverage(again, need)
    lines = [f'merged {a.out} (Dx{base.dx}, {len(out):,} bytes)', f'base {base.label}: {before[0]} entries, '
             f'{before[1]} programs', *m.report,
             f'result: {len(again.entries)} entries (+{len(again.entries) - before[0]}), {len(again.programs)} '
             f'programs (+{len(again.programs) - before[1]}), {len(again.vs)} vertex shaders (+{len(again.vs) - before[2]}), '
             f'{len(again.ps)} pixel shaders (+{len(again.ps) - before[3]}), {len(again.params)} parameter blocks '
             f'(+{len(again.params) - before[4]})',
             'read back: consistent, writes identical bytes; every entry uses the same programs, shaders, '
             'parameter blocks and tables (by content) as in the file it came from']
    lines.append(f'{m.shared_programs} donor programs were already in the file (same GUID and content): shared')
    if m.part_conflicts:
        lines.append(f'{len(m.part_conflicts)} mesh-part names a donor has with another vertex layout:')
        lines += ['  ' + c for c in m.part_conflicts[:50]]
    if m.conflicts:
        lines.append(f'{len(m.conflicts)} donor programs have a GUID the file has with other content (the existing one kept):')
        lines += ['  ' + c for c in m.conflicts[:50]]
    lines.append('entries added:')
    lines += [f'  {n}   <- {lbl}' for n, lbl in m.added]
    nomesh = sorted(k for k, v in still.items() if v == 0)
    lines.append(f'needed names with no entry in the result: {len(nomesh)}')
    lines += ['  ' + k for k in nomesh]
    open(a.out + '.merge.txt', 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    print('\n'.join(lines[:len(m.report) + 5 + (1 if m.conflicts else 0)]))
    print(f'{len(m.added)} entries added; {len(nomesh)} needed names still have no entry; report: {a.out}.merge.txt')
    return 0


def cmd_find(a: argparse.Namespace) -> int:
    """For each needed mesh: the game copy's levels whose Dx<dx> database has entries for it."""
    need = read_need(a.need)
    root = find_opfor(a.opfor)
    dx = a.dx or 9
    if a.base:
        base, _ = load(a.base, dx, a.opfor)
        skip = {m for m, n in coverage(base, need).items() if n}
        need = [m for m in need if m not in skip]
        print(f'{len(skip)} needed meshes already have entries in {base.label}; looking for the other {len(need)}')
    have: dict[str, list[str]] = {m: [] for m in need}
    for lv, path in sorted(level_bundles(root).items()):
        for name, typ in bundle_databases(path):
            if typ.lower() != f'dx{dx}shaderdatabase' or not name.lower().startswith(f'levels/{lv}/'):
                continue                            # other kinds, and copies of other levels' databases
            db, _ = load(f'{path}:{name}', dx, a.opfor)
            for m, n in coverage(db, need).items():
                if n:
                    have[m].append(f'{lv}({n})')
    for m in sorted(have):
        print(f'{m}: {", ".join(have[m]) if have[m] else "NO LEVEL HAS IT"}')
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog='shaderdb.py', description=__doc__.split('\n\n')[0],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--dx', type=int, choices=(9, 10, 11), help='the database kind (default: from the file name)')
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (for level:<name> and find)')
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('info')
    p.add_argument('res')
    p = sub.add_parser('list')
    p.add_argument('res')
    p.add_argument('filter', nargs='?')
    p.add_argument('--parts', action='store_true')
    p = sub.add_parser('textures')
    p.add_argument('res')
    p.add_argument('filter', nargs='?')
    p.add_argument('--constants', action='store_true', help='also the named constants and their values')
    p = sub.add_parser('merge')
    p.add_argument('base')
    p.add_argument('donors', nargs='+')
    p.add_argument('--need', required=True)
    p.add_argument('-o', '--out', required=True)
    p.add_argument('--all-names', action='store_true',
                   help='also add donor entry names for meshes the base already has (other shader sets)')
    p = sub.add_parser('cover')
    p.add_argument('res')
    p.add_argument('level')
    p.add_argument('--all', action='store_true')
    p = sub.add_parser('roundtrip')
    p.add_argument('res', nargs='*')
    p.add_argument('--levels', action='store_true', help="every shader database of the game copy's level bundles")
    p = sub.add_parser('find')
    p.add_argument('need')
    p.add_argument('--base')
    p = sub.add_parser('needs')
    p.add_argument('level')
    p.add_argument('-o', '--out')
    a = ap.parse_args(argv)
    return {'info': cmd_info, 'list': cmd_list, 'textures': cmd_textures, 'merge': cmd_merge, 'cover': cmd_cover, 'roundtrip': cmd_roundtrip,
            'needs': cmd_needs, 'find': cmd_find}[a.cmd](a)


if __name__ == '__main__':
    sys.stdout.reconfigure(errors='replace')  # type: ignore[union-attr]
    from bfbc2core import errors
    errors.setup('shaderdb', gui=False)
    with errors.guard():
        sys.exit(main())
