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
from collections.abc import Collection, Container, Iterable, Sequence
from typing import TYPE_CHECKING, cast

if TYPE_CHECKING:
    from bfbc2core.dbx import Element

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..'))
for _p in (os.path.join(SUITE, 'core'),):
    if _p not in sys.path:
        sys.path.insert(0, _p)

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
        one layout; it keeps dx, default 9 - FbrbTool lists it as Dx9ShaderDatabase)."""
        if len(data) < 9:
            raise ShaderDbError(f'{where} is not a shader database (too short)')
        if big_endian is None:
            big_endian = cls.is_big_endian(data)
        if big_endian:
            try:
                return cls._parse(data, dx or 9, where, True)
            except ShaderDbError as e:
                raise ShaderDbError(f'{where} is not a shader database this tool reads (as Xbox 360: {e})') from e
        layouts = [dx] if dx else [9, 10]
        errors = []
        for d in layouts:
            try:
                return cls._parse(data, d, where)
            except ShaderDbError as e:
                errors.append(f'as Dx{d}: {e}')
        raise ShaderDbError(f'{where} is not a shader database this tool reads ({"; ".join(errors)})')

    @classmethod
    def _parse(cls, data: bytes, dx: int, where: str, big_endian: bool = False) -> ShaderDb:
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
        out: set[str] = set()
        for name, _ in self.parts:
            lod = name.rsplit('/', 2)[0] if name.count('/') >= 2 else ''
            m = re.match(r'(.+)_lod\d+$', lod, re.I)
            if m:
                out.add(m.group(1).lower())
        return out

    def check(self) -> list[str]:
        """FORMAT.md section 4: the things that must hold for the game to load the file and find every entry.
        A list of problems (empty = consistent)."""
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
    m = re.search(r'dx(9|10|11)', os.path.basename(path), re.I)
    return int(m.group(1)) if m else None


def find_opfor(given: str | None = None) -> str:
    """Project OPFOR BFBC2's folder: given, else OPFOR_ROOT, else a project-opfor-bfbc2 folder next to the suite."""
    for c in (given, os.environ.get('OPFOR_ROOT'), os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2')):
        if c and os.path.isdir(os.path.join(c, 'game')):
            return os.path.abspath(c)
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
    bundle's own level folder (Levels/<folder>/Shaders/)."""
    dbs = [n for n, t in bundle_databases(path) if t.lower() == f'dx{dx}shaderdatabase']
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
        except Exception:
            continue
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
        mine = [n for n in self.need if self.all_names or n not in self.in_base]   # needed names it may add to
        for name, guid in donor.entries:
            if name in self.entry_names or not any(needed(name, [n]) for n in mine):
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
        for name, lg in donor.parts:
            pm = part_mesh(name)
            if pm is None or not any(needed(pm, [n]) for n in mine):
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
        vertex layouts are shared as add_donor shares them. Returns the number of programs."""
        self._same_kind(donor, label)
        if new_name in self.entry_names:
            raise ShaderDbError(f'{new_name}: the database has this entry already')
        groups = donor.entry_programs()
        guid_old = dict(donor.entries).get(entry)
        if guid_old is None:
            raise ShaderDbError(f'{label} has no entry {entry}')
        o = self.out
        new_guid = o.name_guid(new_name)
        frag_names = {g: n for n, g in donor.fragments}
        layouts = {r[:16]: r for r in donor.layouts}
        renames = {k.encode('latin-1'): v.encode('latin-1') for k, v in (textures or {}).items()}
        # every texture to rename must be one the entry uses (else the clone silently keeps the donor's - a case
        # mismatch alone does it): checked before anything is added
        seen: dict[bytes, int] = {}
        for pi in groups.get(guid_old, []):
            _, _, a, b = donor.program_refs(pi)
            _retexture(donor.params[a], renames, new_name, seen)
            _retexture(donor.params[b], renames, new_name, seen)
        unused = [k.decode('latin-1') for k in renames if not seen.get(k)]
        if unused:
            raise ShaderDbError(f'{new_name}: {label}:{entry} uses no texture named {", ".join(unused)} '
                                f'(names are matched exactly, case included)')
        hits = None
        progs: list[ProgRef] = []
        for pi in groups.get(guid_old, []):
            prog, key = donor.programs[pi], donor.keys[pi]
            pg = hashlib.md5(new_name.encode('latin-1') + prog[:16]).digest()
            if pg in self.prog_ix:
                raise ShaderDbError(f'{new_name}: program id {pg.hex()} exists')
            v, f, a, b = donor.program_refs(pi)
            nv = self._shader(donor, donor.vs[v], o.vs, self.vs_ix)
            nf = self._shader(donor, donor.ps[f], o.ps, self.ps_ix)
            na = self._table(_retexture(donor.params[a], renames, new_name, hits), o.params, self.param_ix)
            nb = self._table(_retexture(donor.params[b], renames, new_name, hits), o.params, self.param_ix)
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


def _retexture(block: bytes, textures: dict[bytes, bytes], where: str, hits: dict[bytes, int] | None = None) -> bytes:
    """A parameter block with texture names replaced in place: each old name - a whole field's text, starting the
    field, NUL-terminated - becomes the new one, padded with zeros to the same span (the old name, its terminator
    and the zeros after it, within the field's 0x80 bytes). hits: {old: count} is added to."""
    if not textures:
        return block
    out = bytearray(block)
    for old, new in textures.items():
        at = 0
        while True:
            i = out.find(old + b'\0', at)
            if i < 0:
                break
            if i > 0 and out[i - 1] != 0:            # the tail of a longer name: not this texture
                at = i + 1
                continue
            end = i + len(old)
            while end < len(out) and end < i + TEXTURE_FIELD and out[end] == 0:
                end += 1
            if len(new) + 1 > end - i:
                raise ShaderDbError(f'{where}: texture name {new.decode()} longer than the field of {old.decode()}')
            out[i:end] = new + b'\0' * (end - i - len(new))
            at = i + len(new)
            if hits is not None:
                hits[old] = hits.get(old, 0) + 1
    return bytes(out)


# ------------------------------------------------------------------------------------------------ parameter blocks

PARAM_HEADER = 0x20                     # FORMAT.md 'Section 1: parameter blocks': size, 5 offsets, 8 count bytes
TEXTURE_RECORD = 0x8c                   # u8 sampler, u8 cube, 2 zero bytes, 0x80-byte name, 8 zero bytes
CONSTANT_RECORD = 0x38                  # 0x20-byte name, u32 hash, u8 register, u8 components, 2 zero, float4
ENGINE_TEXTURE_RECORD = 0x28            # 0x20-byte parameter name, u32 hash, u16 sampler, 2 zero


def block_textures(block: bytes, e: str = '<') -> list[TextureRef]:
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


def block_constants(block: bytes, e: str = '<') -> list[tuple[str, int, int, tuple[float, ...]]]:
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


def block_engine_textures(block: bytes, e: str = '<') -> list[tuple[str, int]]:
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


def entry_textures(db: ShaderDb, names: Container[str] | None = None) -> dict[str, list[TextureRef]]:
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
                    cache[k] = block_textures(db.params[k], db.e)
                found.update(cache[k])
        out[name] = sorted(found)
    return out


TEXTURE_KINDS = (('normal', ('_nm', '_n', '_normal', '_norm')), ('specular', ('_sp', '_spec', '_s')),
                 ('gloss', ('_gm', '_g')), ('occlusion', ('_occlusion', '_oc', '_ao')),
                 ('color', ('_c', '_df', '_d', '_color', '_diffuse', '_cm')))


def texture_kind(name: str) -> str:
    """What a texture is for, guessed from its name's suffix (DICE's naming: _C / _Df color, _Nm / _N normal, _Sp
    specular, _Gm gloss mask, _Occlusion / _Oc / _AO occlusion), else ''. The databases do not say: a texture
    record names only the sampler register, and the shaders' constant tables call their samplers sampler_Texture<N>."""
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
    """{mesh (lower): number of entries} for the given meshes."""
    count = {m: 0 for m in meshes}
    for name, _ in db.entries:
        low = name.lower()
        parts = low.split('/')
        for i in range(1, len(parts) + 1):
            k = '/'.join(parts[:i])
            if k in count:
                count[k] += 1
    return count


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
