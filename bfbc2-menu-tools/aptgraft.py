r"""aptgraft.py - writes EA APT movies (Battlefield: Bad Company 2 menus and HUD, PC and Xbox 360) and grafts a
character with everything it uses from one movie into another.

  python aptgraft.py roundtrip <movie | folder>... [--pack] [--quiet]
  python aptgraft.py convert <movie> <out.res> [--le | --be] [--pack]
  python aptgraft.py deps <movie> <char>
  python aptgraft.py graft <target> <source> <char> <out.res> [--frame N] [--depth D] [--name NAME] [--x X] [--y Y]
  python aptgraft.py onslaught-timer <PC hudlayer_shared> <console hudlayer_shared> <out.res> [--no-lms] [--no-place]
  python aptgraft.py mode-case <movie> <out.res> --function F --mode M --key K [--fallback TEXT]
  python aptgraft.py mode-alias <movie> <out.res> --mode M --like L [--function F[,F...]]

<movie>, <target>, <source>: a menu .res (a BIGF archive of <name>.apt, .const, .geo), a .apt with its .const / .geo
beside it, or <bundle.fbrb>:<name> (read straight from a bundle). A folder: every SwfMovie of its unpacked bundles
(FbrbTool unpack folders: fbrb.manifest names the SwfMovie entries) and every .res under it that is a movie.

roundtrip  parses each movie into the model below, writes it back and compares: byte-identical .res (container and
           all three members) or the first difference. --pack also writes a packed layout with every structure moved
           (a 16-byte gap after the header) and checks that apt.py reads it as the same movie: the test that every
           pointer is known.
convert    writes a movie in the other byte order (Xbox 360 big-endian <-> PC little-endian), same layout.
deps       the characters, imports, fonts, images (texture atlas entries), geometry and constants a character uses.
graft      copies <char> of <source> into <target> (renumbered, after the target's characters) and places it on the
           target's main timeline: frame N (default 0), depth D (default: the lowest depth the timeline never uses
           above 1), instance NAME, at X, Y.
onslaught-timer  the console HUD's Onslaught stopwatch (and its "last man standing" banner unless --no-lms) grafted
           into a PC hudlayer_shared, with the console's visibility logic and event registrations (below).
           --no-place: the same without placing anything (a diagnostic variant).
mode-case  adds a game mode to a mode switch: in every function F, `if (<mode variable> == M) result = K` ahead of
           its fallback (default "Missing DESC tag!! "). For Onslaught's objective text in the in-match menu:
           mp_objectives --function getMapDesc --mode onslaught --key MP_LOADING_OBJECTIVE_ONSLAUGHT.
mode-alias makes mode M take mode L's branch in switches `<mode value> === "L" -> branch` (a copy of the compare with
           M next to it), in the functions F (default: every function with such a switch, named or anonymous). For
           Onslaught's team names (by faction, as Conquest's): hud_root --mode onslaught --like conquest (the spawn
           screen showed "Missing DESC tag!! onslaught"), loadingscreen_mp --function getTeamName,showGameModeComponents.
Nothing is ever written into the game folder; outputs go where you say.

THE MODEL. Every structure of the three members is a node: Raw (a header), Str (a C string), Words (32-bit fields; a
field is a number or a Ref to another node) or Code (an action stream as instructions). A node remembers where it was
read (`at`). Writing lays nodes out and resolves every Ref; byte order is chosen at write time (numbers, floats and
multi-byte operands are swapped, strings are not).
  layout "keep" (default): every node that kept its size stays at its offset; new and grown nodes go after the end of
         the member (4-byte aligned; code keeps its offset modulo 4, which its operand alignment depends on); the
         space a moved node leaves is zeroed. An unchanged movie is written back byte for byte, and an edited one
         keeps all its old data where it was (the rule OPFOR's menu work follows, DEVELOPMENT_NOTES.md section 3).
  layout "pack": every node in its old order, shifted by a multiple of 4 where something before it grew.
The Apt runtime converts offsets to pointers in place, walking the structures, once per load: so no list is shared
by two users (PushData lists in particular), which the model keeps true by copying, never sharing, new data.

THE FORMAT (every field 32-bit in the movie's byte order unless said; P = a file offset into the same member).
  .apt   "Apt Data:74\x1A" then data. The movie (char 0; the .const header points at it), 60 bytes: 9, 0x09876543,
         frame count, P frames, 0, character count, P character table (P per character, 0 = none / imported),
         width, height, ms per frame, import count, P imports (16 bytes: P movie name, P name, character, 0),
         export count, P exports (8 bytes: P name, character), 0.
         Frames: 8 bytes each {item count, P item list -> P per item}. Items: 1 action {1, P code}; 2 frame label
         {2, P name, flags, frame}; 3 place object (64 bytes: 3, flags, depth, character, 2x2 matrix (4 floats), x, y,
         colour, ?, ratio, P name or 0, clip depth, P clip actions or 0 -> {count, P events -> 12 bytes {flags, key,
         P code}}); 4 remove object {4, depth}; 5 background colour {5, colour}; 8 init action {8, sprite, P code}.
         Characters {type, 0x09876543, ...}: shape 28 bytes (bounds: 4 floats, geometry id = the shape's id);
         text 60 bytes (bounds, font, alignment, colour, height, read-only, multiline, word wrap, P text, P variable);
         font 20 (P name, glyph count, P glyphs: 32-bit each); sprite 20 (frame count, P frames, 0); sound 12 (id);
         image 12 (texture id = the image's id: its entry in the movie's texture atlas); static text 56 (bounds,
         2x2 matrix, x, y, record count, P records -> 56 bytes {font, colour (4 floats), 5 fields, height, ?,
         glyph count, P glyphs: 32-bit each}).
         Code: AVM1 plus EA opcodes (apt.py's table). 32-bit operands of 0x81 0x83 0x87 0x88 0x8A 0x8B 0x8C 0x8E 0x94
         0x96 0x99 0x9B 0x9D 0x9F 0xA1 0xA4-0xA7 0xB8 are 4-byte aligned from the start of the file (padding zero);
         B4 / B7 (4 bytes) and A3 / B6 (2 bytes) are not aligned. Pointers: GetURL two strings, SetTarget / GotoLabel
         / EA string opcodes one string, ConstantPool and PushData {count, P list of constant indices}.
         DefineFunction 0x9B {P name, param count, P params (P per name), body size, 0x98765432, 0x12345678};
         DefineFunction2 0x8E {P name, param count, u16 register count, u16 flags (two 16-bit fields: PC 02 00 29 00,
         Xbox 360 00 02 00 29), P params ({register, P name} each), body size, 0x98765432, 0x12345678}; the body follows inline, the outer stream resumes after it. Branches are
         relative to the end of the branch. Constant-pool operands (A2 AE AF B0-B3: a byte; A3: 16 bits) index the
         current ConstantPool (function bodies inherit it); PushData items are indices into the .const table.
  .const "Apt constant file\x1A\0\0", P movie (into .apt), count, P entries -> {type, value}: type 1 string
         (value = P C string in .const), 4 register (value = register number).
  .geo   "Apt PCGL geometry file\x1A\0" (both platforms), count, P table -> 12 bytes {geometry id, part count,
         P parts (P per part)}. Parts: 1 triangles {1, count, P vertices (6 floats per triangle), colour ARGB};
         6 lines {6, count, P vertices (4 floats per line), colour}; 4 / 5 textured, 88 bytes {type, triangle count,
         P vertices, colour, 16 texture-matrix fields, image character id, 0}. Looked up by id (the table is not
         sorted).
  BIGF   "BIGF", u32 little-endian file size, big-endian member count and header size, big-endian index {offset,
         size, name}, members 4-byte aligned.
2026-10-29: every one of the 92 different SwfMovies in the game copy's bundles (982 entries, Dist and Package: menus,
HUDs, loaders) and the 17 Xbox 360 HUD movies of OPFOR's work folder write back byte for byte, packed and through the
other byte order. All but four are read with nothing left over; those four (OPFOR's installed main_nav, main_root,
main_popups, server_browser) hold dead data their in-place patches left behind - bytes no structure points at, which
the keep layout writes back unchanged (`roundtrip` lists them as notes); their pristine copies have none.
Detecting the byte order: the .const header's movie offset can lie inside the .apt read either way (Xbox 360
hud_crosshair); the movie's signature decides.

GRAFTING (Grafter): a character is copied with everything it reaches: sprites (frames, items, code), shapes and their
geometry, text fields, static text, images (their texture atlas entries are NOT copied - reported), sounds. Character
ids are renumbered after the target's; imported characters map to the target's import of the same movie / name (an
import is added when missing); fonts map to the target's font of the same name. Code keeps its instructions: global
constant indices (PushData, ConstantPool) are re-indexed into the target's constant table (constants added when
missing), strings are copied. Code can also be taken from a range of a source stream (with a ConstantPool of the
constants it uses) or built here (Builder). Grafted movies are written with .apt / .geo in the keep layout and .const
packed (its entry table grows in place, its strings move behind it: the shape of the game's own constant files).

THE ONSLAUGHT TIMER (onslaught-timer; console = Xbox 360 hudlayer_shared of Update #3, offsets in CON_SNIPPETS):
  characters  console 184 onslaught_timer (sprite: setValue(cTime, isActive), updateTime - counts tenths locally every
              3 frames -, setBestTime(bTime); children 181 "box" -> shape 36 (166 x 74, 75 % black triangles, no
              texture), "best" / "label" (TextManager import, "label" = HUD_BEST_TIME), 183 "timer" (blink) -> 182
              "field" (TextManager)); 146 lastManStanding (TextManager "HUD_LAST_MAN_STANDING").
  timeline    frame 0: the banner placed and hidden, the timer hidden unless extern.gameModeID == "onslaught";
              init (console frame 1; on PC frame 0, ahead of its script - the PC movie skips frame 1):
              _root.catchEvent("coopRoundTime", fn(time, isActive) -> onslaught_timer.setValue(time, isActive)),
              catchEvent("coopOldBestRoundTime", fn(time) -> setBestTime(time)), isLastMan = false, lmsShow / lmsHide;
              playing: the timer placed at (96, 397); playing / playing_empty / playing_hardcore: banner shown only for
              isLastMan in onslaught, timer hidden outside onslaught; playing_hardcore: moved to (96, 560);
              notplaying: both removed; killed: the timer placed at (557, 90), without its box in onslaught, hidden
              otherwise; menu: removed. Depths: the first three the PC timeline never uses from 94 up. The console's
              victory-screen use (setVictoryData, onsMessage, ONS_* texts, eor externs) is not taken.
  externs     the console code reads extern.coopOldBestRoundTime (not in the PC exe's name table) and
              extern.isScreenHD (no PC movie reads it); both are replaced (EXTERN_FIX), and the graft refuses any
              other extern the PC movie does not read itself. The first one crashed the game in the first version.
"""
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..'))
for _p in (os.path.join(SUITE, 'core'), os.path.join(SUITE, 'bfbc2-console-readers', 'apt')):
    if _p not in sys.path:
        sys.path.insert(0, _p)

import apt  # noqa: E402  (the suite's APT reader: opcode names, the dump used to check written movies)

SIG = 0x09876543
GRAFT_LAYOUT = {'apt': 'keep', 'geo': 'keep', 'const': 'pack'}     # how grafted movies are written
FN_MAGIC = (0x98765432, 0x12345678)
CHAR_SIZE = {1: 7, 2: 15, 3: 5, 5: 5, 6: 3, 7: 3, 9: 15, 10: 14}      # characters' sizes in 32-bit words

# operand classes (as apt.Movie.decode reads them)
ALIGNED_INT = {0x81, 0x87, 0x94, 0x9F}
BRANCH = {0x99, 0x9D, 0xB8}
ONE_STR = {0x8B, 0x8C, 0xA1, 0xA4, 0xA5, 0xA6, 0xA7}
ONE_BYTE = {0x8D, 0xA2, 0xAE, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3, 0xB5, 0xB9}
POOL_BYTE = {0xA2, 0xAE, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3}
OPS = {v: k for k, v in apt.NAMES.items()}


class AptError(Exception):
    """A movie this tool cannot read or write as asked (explained in words by errors.guard)."""


# ------------------------------------------------------------------------------------------------ nodes

class Ref:
    __slots__ = ('node',)

    def __init__(self, node):
        self.node = node


class Node:
    __slots__ = ('file', 'at', 'size0', 'tag', 'new_at')

    def __init__(self, file, at, tag):
        self.file, self.at, self.tag, self.new_at = file, at, tag, None
        self.size0 = None

    def refs(self):
        return []


class Raw(Node):
    __slots__ = ('data',)

    def __init__(self, file, at, data, tag='raw'):
        super().__init__(file, at, tag)
        self.data = bytes(data)

    def size(self, phase=0):
        return len(self.data)


class Str(Node):
    __slots__ = ('text',)

    def __init__(self, file, at, text, tag='str'):
        super().__init__(file, at, tag)
        self.text = bytes(text)

    def size(self, phase=0):
        return len(self.text) + 1


class Words(Node):
    __slots__ = ('vals',)

    def __init__(self, file, at, vals, tag):
        super().__init__(file, at, tag)
        self.vals = list(vals)

    def size(self, phase=0):
        return 4 * len(self.vals)

    def refs(self):
        return [v.node for v in self.vals if isinstance(v, Ref)]


class Ins:
    """One instruction: op, args (numbers as stored, Ref for pointers; a branch's target is an Ins of its own
    stream or the stream's list itself, meaning its end), body (a function definition's instructions)."""
    __slots__ = ('op', 'args', 'body', 'at', 'target')

    def __init__(self, op, args=(), body=None, target=None, at=None):
        self.op, self.args, self.body, self.target, self.at = op, list(args), body, target, at

    @property
    def name(self):
        return apt.NAMES.get(self.op, f'op_{self.op:02X}')


class Code(Node):
    __slots__ = ('ins',)

    def __init__(self, file, at, ins, tag='code'):
        super().__init__(file, at, tag)
        self.ins = ins

    def size(self, phase=0):
        return len(assemble(self.ins, phase, '<', None))

    def refs(self):
        out = []

        def walk(lst):
            for i in lst:
                out.extend(a.node for a in i.args if isinstance(a, Ref))
                if i.body is not None:
                    walk(i.body)
        walk(self.ins)
        return out


def walk_ins(lst):
    """Every instruction of a stream, function bodies included (depth first)."""
    for i in lst:
        yield i
        if i.body is not None:
            yield from walk_ins(i.body)


# ------------------------------------------------------------------------------------------------ assembler

def _al(p):
    return (p + 3) & ~3


def assemble(stream, pos, e, resolve):
    """Bytes of an instruction stream that starts at file offset pos (alignment depends on it). resolve(node) -> its
    offset, or None to size only (pointers written as 0)."""
    posmap = {}
    _place(stream, pos, posmap)
    out = bytearray()
    _emit(stream, pos, e, resolve, posmap, out)
    return bytes(out)


def _ins_len(i, p, posmap):
    """The position after instruction i at p (records function bodies' instruction positions in posmap)."""
    op = i.op
    p += 1
    if op in (0xB7, 0xB4):
        return p + 4
    if op in ALIGNED_INT or op in BRANCH or op in ONE_STR:
        return _al(p) + 4
    if op in (0x83, 0x88, 0x96, 0x8A):
        return _al(p) + 8
    if op in ONE_BYTE:
        return p + 1
    if op in (0xA3, 0xB6):
        return p + 2
    if op in (0x9B, 0x8E):
        body = _al(p) + (24 if op == 0x9B else 28)
        return _place(i.body, body, posmap)
    if op >= 0x80 and op not in apt.NAMES:
        raise AptError(f'opcode {op:#04x} has an argument this tool does not know')
    return p


def _place(stream, pos, posmap):
    for i in stream:
        posmap[id(i)] = pos
        pos = _ins_len(i, pos, posmap)
    posmap[id(stream)] = pos
    return pos


def _emit(stream, pos, e, R, posmap, out):
    def ptr(v):
        if isinstance(v, Ref):
            return R(v.node) if R else 0
        return v

    def pad(p):
        out.extend(b'\0' * (_al(p) - p))
        return _al(p)

    for i in stream:
        op, a = i.op, i.args
        out.append(op)
        p = pos + 1
        if op in (0xB7, 0xB4):
            out += struct.pack(e + 'I', a[0]); p += 4
        elif op in ALIGNED_INT:
            p = pad(p); out += struct.pack(e + 'I', a[0]); p += 4
        elif op in BRANCH:
            p = pad(p) + 4
            tgt = posmap.get(id(i.target))
            if tgt is None:
                raise AptError(f'a branch at {i.at} has no target in its stream')
            out += struct.pack(e + 'i', tgt - p)
        elif op in ONE_STR:
            p = pad(p); out += struct.pack(e + 'I', ptr(a[0])); p += 4
        elif op in (0x83, 0x88, 0x96, 0x8A):
            p = pad(p)
            if op in (0x88, 0x96):
                n = len(a[1].node.vals) if isinstance(a[1], Ref) else a[0]
                out += struct.pack(e + 'II', n, ptr(a[1]))
            else:
                out += struct.pack(e + 'II', ptr(a[0]), ptr(a[1]))
            p += 8
        elif op in ONE_BYTE:
            out.append(a[0] & 0xFF); p += 1
        elif op in (0xA3, 0xB6):
            out += struct.pack(e + 'H', a[0] & 0xFFFF); p += 2
        elif op in (0x9B, 0x8E):
            p = pad(p)
            body = p + (24 if op == 0x9B else 28)
            size = posmap[id(i.body)] - body
            if op == 0x9B:              # name, nparams, params, size, magic x2
                out += struct.pack(e + '6I', ptr(a[0]), a[1], ptr(a[2]), size, a[3], a[4])
            else:                        # name, nparams, u16 registers, u16 flags, params, size, magic x2
                out += struct.pack(e + '2I2H4I', ptr(a[0]), a[1], a[2] & 0xFFFF, a[2] >> 16, ptr(a[3]), size,
                                   a[4], a[5])
            _emit(i.body, body, e, R, posmap, out)
            p = body + size
        pos = p
    return pos


# ------------------------------------------------------------------------------------------------ BIGF

def read_bigf_bytes(b, where='file'):
    """[(name, bytes)] in index order."""
    if b[:4] != b'BIGF':
        raise AptError(f'{where} is not a menu movie (.res, a BIGF archive)')
    n = struct.unpack_from('>I', b, 8)[0]
    p, out = 16, []
    for _ in range(n):
        off, size = struct.unpack_from('>II', b, p)
        e = b.index(b'\0', p + 8)
        out.append((b[p + 8:e].decode('latin1'), b[off:off + size]))
        p = e + 1
    return out


def write_bigf(members):
    """BIGF bytes of [(name, bytes)]: header, big-endian index, members 4-byte aligned in order."""
    index = sum(8 + len(n) + 1 for n, _ in members)
    pos, offs = 16 + index, []
    for _, d in members:
        pos = _al(pos)
        offs.append(pos)
        pos += len(d)
    out = bytearray(pos)
    out[0:4] = b'BIGF'
    struct.pack_into('<I', out, 4, pos)
    struct.pack_into('>II', out, 8, len(members), 16 + index)
    ip = 16
    for (n, d), o in zip(members, offs):
        struct.pack_into('>II', out, ip, o, len(d))
        out[ip + 8:ip + 8 + len(n)] = n.encode('latin1')
        ip += 8 + len(n) + 1
        out[o:o + len(d)] = d
    return bytes(out)


# ------------------------------------------------------------------------------------------------ the movie

class Movie:
    """A parsed movie. Movie(members=[(name, bytes)]) or Movie.load(spec)."""

    def __init__(self, members, label='movie'):
        self.label = label
        self.order = [n for n, _ in members]
        mem = dict(members)
        aptname = next((n for n in self.order if n.lower().endswith('.apt')), None)
        if aptname is None:
            raise AptError(f'{label}: no .apt member')
        self.name = aptname[:-4]
        self.names = {'apt': aptname}
        for ext in ('const', 'geo'):
            k = next((n for n in self.order if n.lower() == f'{self.name}.{ext}'.lower()), None)
            if k is None and ext == 'const':
                raise AptError(f'{label}: no {self.name}.const member')
            if k:
                self.names[ext] = k
        self.data = {f: mem[n] for f, n in self.names.items()}
        self.extra = [(n, mem[n]) for n in self.order if n not in self.names.values()]
        c = self.data['const']
        if not c.startswith(b'Apt constant file'):
            raise AptError(f'{label}: not an APT constant file')
        a = self.data['apt']
        le, be = struct.unpack_from('<I', c, 20)[0], struct.unpack_from('>I', c, 20)[0]
        def is_movie(o, e):                  # both values can lie inside the file: the movie's signature decides
            return 0 < o <= len(a) - 8 and struct.unpack_from(e + 'I', a, o + 4)[0] == SIG
        self.big = not is_movie(le, '<') and is_movie(be, '>')
        self.e = '>' if self.big else '<'
        self.cache = {}
        self.roots = {}
        self._parse()

    @classmethod
    def load(cls, spec):
        label, members = load_members(spec)
        return cls(members, label)

    # --- reading -------------------------------------------------------------------------------------------
    def _u(self, f, o):
        return struct.unpack_from(self.e + 'I', self.data[f], o)[0]

    def _mk(self, key, make):
        n = self.cache.get(key)
        if n is None:
            n = make()
            self.cache[key] = n
        return n

    def words(self, f, at, count, tag):
        """The Words node of count fields at at (shared when two pointers meet)."""
        key = (f, at, 'w' if count else 'empty')
        n = self.cache.get(key)
        if n is not None:
            if len(n.vals) != count:
                raise AptError(f'{self.label}: {f} {at:#x} read as {n.tag} ({len(n.vals)} fields) and as {tag} '
                               f'({count})')
            return n
        if at + 4 * count > len(self.data[f]):
            raise AptError(f'{self.label}: {tag} at {f} {at:#x} runs past the end of the file')
        n = Words(f, at, struct.unpack_from(self.e + f'{count}I', self.data[f], at), tag)
        n.size0 = 4 * count
        self.cache[key] = n
        return n

    def string(self, f, at):
        def make():
            b = self.data[f]
            if not 0 < at < len(b):
                raise AptError(f'{self.label}: string pointer {at:#x} outside {f}')
            e = b.find(b'\0', at)
            if e < 0:
                raise AptError(f'{self.label}: unterminated string at {f} {at:#x}')
            n = Str(f, at, b[at:e])
            n.size0 = e + 1 - at
            return n
        return self._mk((f, at, 's'), make)

    def sref(self, f, p):
        return Ref(self.string(f, p)) if p else 0

    def wref(self, f, p, count, tag, fill=None):
        """A Ref to the Words node at p (0 when p is 0); fill(node) runs once, the first time it is read."""
        if not p:
            return 0
        new = (f, p, 'w' if count else 'empty') not in self.cache
        n = self.words(f, p, count, tag)
        if new and fill:
            fill(n)
        return Ref(n)

    def _parse(self):
        a = self.data['apt']
        end = a.find(b'\x1a')
        if not a.startswith(b'Apt Data') or end < 0:
            raise AptError(f'{self.label}: not an APT data file')
        self.apt_head = Raw('apt', 0, a[:end + 1], 'header')
        self.apt_head.size0 = end + 1
        c = self.data['const']
        self.const_magic = Raw('const', 0, c[:20], 'header')
        self.const_magic.size0 = 20
        self.const_head = self.words('const', 20, 3, 'const.header')
        mv = self.const_head.vals[0]
        self.movie = self.character(mv)
        if self.movie.vals[0] != 9:
            raise AptError(f'{self.label}: the .const header does not point at the movie')
        h = self.const_head
        h.vals[0] = Ref(self.movie)
        h.vals[2] = self.wref('const', h.vals[2], 2 * h.vals[1], 'const.entries', self._fill_consts)
        self.roots = {'apt': [self.apt_head, self.movie], 'const': [self.const_magic, self.const_head]}
        if 'geo' in self.data:
            g = self.data['geo']
            if not g.startswith(b'Apt PCGL geometry file'):
                raise AptError(f'{self.label}: unknown geometry file {g[:24]!r}')
            self.geo_magic = Raw('geo', 0, g[:0x18], 'header')
            self.geo_magic.size0 = 0x18
            self.geo_head = self.words('geo', 0x18, 2, 'geo.header')
            gh = self.geo_head
            gh.vals[1] = self.wref('geo', gh.vals[1], 3 * gh.vals[0], 'geo.table', self._fill_geo)
            self.roots['geo'] = [self.geo_magic, self.geo_head]

    def _fill_consts(self, n):
        for k in range(0, len(n.vals), 2):
            if n.vals[k] == 1:
                n.vals[k + 1] = self.sref('const', n.vals[k + 1])

    def _fill_geo(self, t):
        for k in range(0, len(t.vals), 3):
            t.vals[k + 2] = self.wref('geo', t.vals[k + 2], t.vals[k + 1], 'geo.parts', self._fill_parts)

    def _fill_parts(self, n):
        for k, p in enumerate(n.vals):
            if not p:
                continue
            kind = self._u('geo', p)
            if kind in (1, 6):
                part = self.words('geo', p, 4, f'geo.part{kind}')
                per = 6 if kind == 1 else 4
            elif kind in (4, 5):
                part = self.words('geo', p, 22, f'geo.part{kind}')
                per = 6
            else:
                raise AptError(f'{self.label}: geometry part type {kind} at .geo {p:#x} is not known')
            if not isinstance(part.vals[2], Ref):
                part.vals[2] = self.wref('geo', part.vals[2], per * part.vals[1], 'geo.vertices')
            n.vals[k] = Ref(part)

    # characters
    def character(self, p):
        a = self.data['apt']
        if p + 8 > len(a) or self._u('apt', p + 4) != SIG:
            raise AptError(f'{self.label}: no character at .apt {p:#x}')
        t = self._u('apt', p)
        if t not in CHAR_SIZE:
            raise AptError(f'{self.label}: character type {t} ({apt.CHAR_TYPES.get(t, "?")}) at {p:#x} is not '
                           'supported')
        key = ('apt', p, 'w')
        known = key in self.cache
        n = self.words('apt', p, CHAR_SIZE[t], apt.CHAR_TYPES[t])
        if known:
            return n
        v = n.vals
        if t in (9, 5):
            v[3] = self.wref('apt', v[3], 2 * v[2], 'frames', self._fill_frames)
        if t == 9:
            v[6] = self.wref('apt', v[6], v[5], 'chartable', self._fill_chartable)
            v[11] = self.wref('apt', v[11], 4 * v[10], 'imports', self._fill_imports)
            v[13] = self.wref('apt', v[13], 2 * v[12], 'exports', self._fill_exports)
        elif t == 2:
            v[13], v[14] = self.sref('apt', v[13]), self.sref('apt', v[14])
        elif t == 3:
            v[2] = self.sref('apt', v[2])
            v[4] = self.wref('apt', v[4], v[3], 'glyphs')
        elif t == 10:
            v[13] = self.wref('apt', v[13], 14 * v[12], 'stext.records', self._fill_records)
        return n

    def _fill_chartable(self, n):
        for k, p in enumerate(n.vals):
            n.vals[k] = Ref(self.character(p)) if p else 0

    def _fill_imports(self, n):
        for k in range(0, len(n.vals), 4):
            n.vals[k] = self.sref('apt', n.vals[k])
            n.vals[k + 1] = self.sref('apt', n.vals[k + 1])

    def _fill_exports(self, n):
        for k in range(0, len(n.vals), 2):
            n.vals[k] = self.sref('apt', n.vals[k])

    def _fill_records(self, n):
        for k in range(0, len(n.vals), 14):
            n.vals[k + 13] = self.wref('apt', n.vals[k + 13], n.vals[k + 12], 'glyphs')

    def _fill_frames(self, n):
        for k in range(0, len(n.vals), 2):
            n.vals[k + 1] = self.wref('apt', n.vals[k + 1], n.vals[k], 'items', self._fill_items)

    def _fill_items(self, n):
        for k, p in enumerate(n.vals):
            n.vals[k] = Ref(self.item(p))

    def item(self, p):
        t = self._u('apt', p)
        size = {1: 2, 2: 4, 3: 16, 4: 2, 5: 2, 8: 3}.get(t)
        if size is None:
            raise AptError(f'{self.label}: frame item type {t} at .apt {p:#x} is not supported')
        key = ('apt', p, 'w')
        known = key in self.cache
        n = self.words('apt', p, size, {1: 'action', 2: 'label', 3: 'place', 4: 'remove', 5: 'background',
                                         8: 'initaction'}[t])
        if known:
            return n
        v = n.vals
        if t == 1:
            v[1] = Ref(self.code(v[1]))
        elif t == 8:
            v[2] = Ref(self.code(v[2]))
        elif t == 2:
            v[1] = self.sref('apt', v[1])
        elif t == 3:
            v[13] = self.sref('apt', v[13])
            v[15] = self.wref('apt', v[15], 2, 'clipactions', self._fill_clip)
        return n

    def _fill_clip(self, n):
        n.vals[1] = self.wref('apt', n.vals[1], 3 * n.vals[0], 'clipevents', self._fill_events)

    def _fill_events(self, n):
        for k in range(0, len(n.vals), 3):
            n.vals[k + 2] = Ref(self.code(n.vals[k + 2]))

    # code
    def code(self, p):
        def make():
            stream, end = self._stream(p, None)
            n = Code('apt', p, stream)
            n.size0 = end - p
            return n
        return self._mk(('apt', p, 'c'), make)

    def _stream(self, p, end):
        """Instructions from p: to the top-level End (end None) or to end exactly. -> (list, end)."""
        a, u, e = self.data['apt'], (lambda o: self._u('apt', o)), self.e
        out, start = [], p
        while True:
            if end is None and out and out[-1].op == 0:
                break
            if end is not None and p >= end:
                if p != end:
                    raise AptError(f'{self.label}: a function body ending at {end:#x} does not decode to its end')
                break
            if p >= len(a):
                raise AptError(f'{self.label}: code from {start:#x} runs past the end of the file')
            op, at = a[p], p
            p += 1
            ins = Ins(op, at=at)
            if op in (0xB7, 0xB4):
                ins.args = [u(p)]; p += 4
            elif op in ALIGNED_INT:
                p = _al(p); ins.args = [u(p)]; p += 4
            elif op in BRANCH:
                p = _al(p); v = struct.unpack_from(e + 'i', a, p)[0]; p += 4
                ins.target = p + v                         # resolved below
            elif op in ONE_STR:
                p = _al(p); ins.args = [self.sref('apt', u(p))]; p += 4
            elif op == 0x83:
                p = _al(p); ins.args = [self.sref('apt', u(p)), self.sref('apt', u(p + 4))]; p += 8
            elif op == 0x8A:
                p = _al(p); ins.args = [u(p), u(p + 4)]; p += 8
            elif op in (0x88, 0x96):
                p = _al(p)
                cnt = u(p)
                ins.args = [cnt, self.wref('apt', u(p + 4), cnt, 'poollist' if op == 0x88 else 'pushlist')]
                p += 8
            elif op in ONE_BYTE:
                ins.args = [a[p]]; p += 1
            elif op in (0xA3, 0xB6):
                ins.args = [struct.unpack_from(e + 'H', a, p)[0]]; p += 2
            elif op == 0x9B:
                p = _al(p)
                name, npar, params, size, m1, m2 = struct.unpack_from(e + '6I', a, p)
                ins.args = [self.sref('apt', name), npar,
                            self.wref('apt', params, npar, 'f1params', self._fill_f1), m1, m2]
                body = p + 24
                ins.body, _ = self._stream(body, body + size)
                p = body + size
            elif op == 0x8E:
                p = _al(p)
                name, npar = struct.unpack_from(e + '2I', a, p)
                nregs, flags = struct.unpack_from(e + '2H', a, p + 8)    # two 16-bit fields, not one 32-bit
                params, size, m1, m2 = struct.unpack_from(e + '4I', a, p + 12)
                ins.args = [self.sref('apt', name), npar, nregs | flags << 16,
                            self.wref('apt', params, 2 * npar, 'f2params', self._fill_f2), m1, m2]
                body = p + 28
                ins.body, _ = self._stream(body, body + size)
                p = body + size
            elif op >= 0x80 and op not in apt.NAMES:
                raise AptError(f'{self.label}: opcode {op:#04x} at {at:#x} has an argument this tool does not know')
            out.append(ins)
        by_at = {i.at: i for i in out}
        for i in out:
            if i.op in BRANCH:
                t = i.target
                if t == p:
                    i.target = out
                elif t in by_at:
                    i.target = by_at[t]
                else:
                    raise AptError(f'{self.label}: the branch at {i.at:#x} does not land on an instruction of its '
                                   'stream')
        return out, p

    def _fill_f1(self, n):
        n.vals = [self.sref('apt', v) for v in n.vals]

    def _fill_f2(self, n):
        for k in range(1, len(n.vals), 2):
            n.vals[k] = self.sref('apt', n.vals[k])

    # --- structure access -------------------------------------------------------------------------------------
    @property
    def chartable(self):
        return self.movie.vals[6].node

    def char(self, cid):
        v = self.chartable.vals[cid] if 0 <= cid < len(self.chartable.vals) else 0
        return v.node if isinstance(v, Ref) else None

    def char_type(self, cid):
        n = self.char(cid)
        return n.vals[0] if n else None

    def add_char(self, node):
        """Appends a character (a node, or None for an import's slot): its new id."""
        t = self.chartable
        t.vals.append(Ref(node) if node is not None else 0)
        self.movie.vals[5] = len(t.vals)
        return len(t.vals) - 1

    def imports(self):
        """[(movie, name, character)]."""
        v = self.movie.vals[11].node.vals if isinstance(self.movie.vals[11], Ref) else []
        return [(text(v[k]), text(v[k + 1]), v[k + 2]) for k in range(0, len(v), 4)]

    def exports(self):
        v = self.movie.vals[13].node.vals if isinstance(self.movie.vals[13], Ref) else []
        return [(text(v[k]), v[k + 1]) for k in range(0, len(v), 2)]

    def add_import(self, movie, name):
        cid = self.add_char(None)
        if not isinstance(self.movie.vals[11], Ref):
            self.movie.vals[11] = Ref(Words('apt', None, [], 'imports'))
        node = self.movie.vals[11].node
        node.vals += [Ref(Str('apt', None, movie.encode('latin1'))), Ref(Str('apt', None, name.encode('latin1'))),
                      cid, 0]
        self.movie.vals[10] = len(node.vals) // 4
        return cid

    def font_name(self, cid):
        n = self.char(cid)
        return text(n.vals[2]) if n is not None and n.vals[0] == 3 else None

    def frames(self, owner=None):
        """The frames node (2 fields per frame) of the movie or of a sprite node."""
        owner = owner or self.movie
        if not isinstance(owner.vals[3], Ref):
            owner.vals[3] = Ref(Words('apt', None, [], 'frames'))
        return owner.vals[3].node

    def items(self, frame, owner=None, create=False):
        """The item list node of a frame (created, when create, for a frame without one; an empty list read from
        the file is replaced, never appended to: several empty frames can share one)."""
        fr = self.frames(owner)
        if 2 * frame + 1 >= len(fr.vals):
            if not create:
                return None
            fr.vals += [0, 0] * (frame + 1 - len(fr.vals) // 2)
            (owner or self.movie).vals[2] = len(fr.vals) // 2
        v = fr.vals[2 * frame + 1]
        if not isinstance(v, Ref) or (not v.node.vals and v.node.at is not None and create):
            if not create:
                return None
            node = Words('apt', None, [], 'items')
            fr.vals[2 * frame + 1] = Ref(node)
            return node
        return v.node

    def add_item(self, frame, item, owner=None, index=None):
        lst = self.items(frame, owner, create=True)
        lst.vals.insert(len(lst.vals) if index is None else index, Ref(item))
        self.frames(owner).vals[2 * frame] = len(lst.vals)

    def frame_items(self, frame, owner=None):
        lst = self.items(frame, owner)
        return [v.node for v in lst.vals] if lst else []

    def timeline_depths(self, owner=None):
        """Every depth a timeline places or removes."""
        out = set()
        fr = self.frames(owner)
        for f in range(len(fr.vals) // 2):
            for it in self.frame_items(f, owner):
                if it.tag in ('place', 'remove'):
                    out.add(signed(it.vals[2] if it.tag == 'place' else it.vals[1]))
        return out

    def label_frame(self, label, owner=None):
        fr = self.frames(owner)
        for f in range(len(fr.vals) // 2):
            for it in self.frame_items(f, owner):
                if it.tag == 'label' and text(it.vals[1]) == label:
                    return f
        raise AptError(f'{self.label}: no frame label "{label}"')

    # constants
    @property
    def const_entries(self):
        return self.const_head.vals[2].node

    def constant(self, k):
        v = self.const_entries.vals
        t, x = v[2 * k], v[2 * k + 1]
        return ('s', x.node.text) if t == 1 and isinstance(x, Ref) else (t, x)

    def const_index(self, key, add=True):
        """The index of constant key (('s', bytes) a string, (4, n) a register), added at the end when missing."""
        idx = getattr(self, '_cidx', None)
        if idx is None:
            idx = self._cidx = {}
            for k in range(len(self.const_entries.vals) // 2):
                idx.setdefault(self.constant(k), k)
        if key in idx:
            return idx[key]
        if not add:
            return None
        v = self.const_entries.vals
        if key[0] == 's':
            v += [1, Ref(Str('const', None, key[1]))]
        else:
            v += [key[0], key[1]]
        k = len(v) // 2 - 1
        self.const_head.vals[1] = k + 1
        idx[key] = k
        return k

    def const_text(self, k):
        c = self.constant(k)
        return c[1].decode('latin1') if c[0] == 's' else f'reg{c[1]}' if c[0] == 4 else f'type{c[0]}:{c[1]}'

    # geometry
    def geo_entry(self, gid):
        """(table node, field index) of geometry gid, or None."""
        if 'geo' not in self.roots:
            return None
        t = self.geo_head.vals[1]
        if not isinstance(t, Ref):
            return None
        v = t.node.vals
        for k in range(0, len(v), 3):
            if v[k] == gid:
                return t.node, k
        return None

    def add_geometry(self, gid, parts_node):
        if 'geo' not in self.roots:
            raise AptError(f'{self.label}: no .geo member to add geometry to')
        gh = self.geo_head
        if not isinstance(gh.vals[1], Ref):
            gh.vals[1] = Ref(Words('geo', None, [], 'geo.table'))
        t = gh.vals[1].node
        t.vals += [gid, len(parts_node.vals), Ref(parts_node)]
        gh.vals[0] = len(t.vals) // 3

    # --- writing ---------------------------------------------------------------------------------------------
    def reachable(self):
        """{file: [nodes]} reachable from the roots, each once."""
        seen, out = set(), {f: [] for f in self.roots}
        stack = [n for f in self.roots for n in self.roots[f]]
        while stack:
            n = stack.pop()
            if id(n) in seen:
                continue
            seen.add(id(n))
            out.setdefault(n.file, []).append(n)
            stack.extend(n.refs())
        return out

    def write(self, order=None, layout='keep', gap=0, dead=True):
        """[(member name, bytes)] in the original member order. order '<' / '>' (default: the movie's own);
        layout 'keep' or 'pack' (module doc), or {member: layout} ('apt' / 'const' / 'geo'; default keep); gap: bytes added after each header (pack only, for tests); dead: in
        the keep layout and the movie's own byte order, bytes no structure points at are kept (else zero)."""
        e = order or self.e
        nodes = self.reachable()
        size = {}

        def lay(f):
            return layout.get(f, 'keep') if isinstance(layout, dict) else layout
        for f in ('apt', 'const', 'geo'):
            if f in nodes:
                size[f] = self._layout(nodes[f], f, lay(f), gap)
        resolve = (lambda n: n.new_at)
        data = {}
        for f in size:
            buf = bytearray(size[f])
            if dead and lay(f) == 'keep' and e == self.e:
                # bytes no structure points at (dead data an in-place patch left behind) stay as they were;
                # the places of everything read are cleared, then the reachable nodes written
                buf[:len(self.data[f])] = self.data[f]
                for n in self.cache.values():
                    if n.file == f and n.size0:
                        buf[n.at:n.at + n.size0] = bytes(n.size0)
                for n in (self.apt_head, self.const_magic, getattr(self, 'geo_magic', None)):
                    if n is not None and n.file == f:
                        buf[n.at:n.at + n.size0] = bytes(n.size0)
            for n in nodes[f]:
                if isinstance(n, Raw):
                    b = n.data
                elif isinstance(n, Str):
                    b = n.text + b'\0'
                elif isinstance(n, Words):
                    b = b''.join(struct.pack(e + 'I', resolve(v.node) if isinstance(v, Ref) else v & 0xFFFFFFFF)
                                 for v in n.vals)
                else:
                    b = assemble(n.ins, n.new_at, e, resolve)
                buf[n.new_at:n.new_at + len(b)] = b
            data[f] = bytes(buf)
        out = []
        for name in self.order:
            f = next((k for k, v in self.names.items() if v == name), None)
            out.append((name, data[f] if f else dict(self.extra)[name]))
        return out

    def _layout(self, nodes, f, layout, gap):
        """Sets new_at for every node of member f; returns the member's size."""
        old = sorted((n for n in nodes if n.at is not None), key=lambda n: (n.at, n.size0 != 0))
        new = [n for n in nodes if n.at is None]
        end0 = len(self.data[f])
        if layout == 'keep':
            moved = []
            for n in old:
                if n.size(n.at % 4) == n.size0:
                    n.new_at = n.at
                else:
                    moved.append(n)
            end = end0
            for n in moved + new:
                end = self._append(n, end)
            return end
        shift, prev_end = 0, 0
        fixed = ('header', 'const.header', 'geo.header')       # at fixed offsets: the gap goes after them
        for k, n in enumerate(old):
            if n.tag in fixed:
                n.new_at = n.at
            else:
                at = n.at + shift
                if at < prev_end:
                    shift += _al(prev_end - at)
                    at = n.at + shift
                n.new_at = at
            if n.tag in fixed and (k + 1 == len(old) or old[k + 1].tag not in fixed):
                shift += gap
            prev_end = max(prev_end, n.new_at + n.size(n.new_at % 4))
        end = max(prev_end, end0 + shift)
        for n in new:
            end = self._append(n, end)
        return end

    @staticmethod
    def _append(n, end):
        if isinstance(n, Str):
            n.new_at = end
            return end + n.size()
        at = _al(end)
        if isinstance(n, Code) and n.at is not None:
            at += n.at % 4                           # its operands are aligned for this phase
        n.new_at = at
        return at + n.size(at % 4)

    def to_res(self, order=None, layout='keep', gap=0):
        return write_bigf(self.write(order, layout, gap))

    # --- checks -----------------------------------------------------------------------------------------------
    def leftovers(self):
        """Bytes of the members no node accounts for that are not zero, and overlapping nodes: [(file, text)]."""
        out = []
        nodes = self.reachable()
        for f, lst in nodes.items():
            b = self.data[f]
            spans = sorted((n.at, n.at + n.size0, n) for n in lst if n.at is not None and n.size0)
            pos = 0
            for s, e, n in spans:
                if s < pos:
                    out.append((f, f'{n.tag} at {s:#x} overlaps the structure before it (ending {pos:#x})'))
                elif any(b[pos:s]):
                    out.append((f, f'unread bytes {pos:#x}-{s:#x}: {b[pos:min(s, pos + 16)].hex(" ")}'))
                pos = max(pos, e)
            if any(b[pos:]):
                out.append((f, f'unread bytes at the end {pos:#x}-{len(b):#x}'))
        return out


def text(v):
    return v.node.text.decode('latin1') if isinstance(v, Ref) else None


def signed(v):
    return v - (1 << 32) if v & 0x80000000 else v


def f32(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def load_members(spec):
    """(label, [(name, bytes)]) of a movie spec: a .res, a .apt (with .const / .geo beside it), <bundle.fbrb>:<name>."""
    if os.path.isfile(spec):
        if spec.lower().endswith('.apt'):
            base = os.path.splitext(spec)[0]
            name = os.path.basename(base)
            out = [(name + '.apt', open(spec, 'rb').read())]
            for ext in ('.const', '.geo'):
                if os.path.isfile(base + ext):
                    out.append((name + ext, open(base + ext, 'rb').read()))
            return spec, out
        return spec, read_bigf_bytes(open(spec, 'rb').read(), spec)
    if '.fbrb:' in spec.lower():
        from bfbc2core import fbrb
        bundle, name = spec.rsplit(':', 1)
        if not os.path.isfile(bundle):
            raise FileNotFoundError(bundle)
        names = fbrb.names(bundle)
        entry = name if name.lower() in names else f'Menu/{name}.res'
        if entry.lower() not in names:
            raise AptError(f'no {entry} in {bundle}')
        real = names[entry.lower()].name
        return spec, read_bigf_bytes(fbrb.read(bundle, [real])[real], spec)
    raise FileNotFoundError(spec)


def movie_files(paths):
    """Every movie under the paths: files as given; in folders, the SwfMovie entries their fbrb.manifest files name,
    else every .res that is a BIGF archive with a .apt member."""
    out = []
    for p in paths:
        if os.path.isfile(p) or '.fbrb:' in p.lower():
            out.append(p)
            continue
        if not os.path.isdir(p):
            raise FileNotFoundError(p)
        for dp, dirs, fs in os.walk(p):
            dirs.sort()
            if 'fbrb.manifest' in fs:
                for line in open(os.path.join(dp, 'fbrb.manifest'), encoding='utf-8', errors='replace'):
                    parts = line.rstrip('\n').split('|')
                    if not line.startswith('#') and len(parts) > 1 and parts[1] == 'SwfMovie':
                        f = os.path.join(dp, *parts[0].split('/'))
                        if os.path.isfile(f):
                            out.append(f)
                dirs[:] = []                     # the manifest's folder tree is covered
                continue
            for f in sorted(fs):
                if f.lower().endswith('.res'):
                    fp = os.path.join(dp, f)
                    with open(fp, 'rb') as fh:
                        head = fh.read(4)
                    if head == b'BIGF':
                        out.append(fp)
    return out


# ------------------------------------------------------------------------------------------------ grafting

class Grafter:
    """Copies characters, items and code from src into dst (both Movie). cmap: src character id -> dst id."""

    def __init__(self, src, dst):
        self.src, self.dst = src, dst
        self.cmap, self.memo = {}, {}
        self.notes = []                 # what the copy needs that it could not bring (texture atlas entries)
        self.added = []                 # (dst id, kind, src id)
        self.cmap_const = {}

    # constants
    def const(self, k):
        if k not in self.cmap_const:
            if k >= len(self.src.const_entries.vals) // 2:
                raise AptError(f'{self.src.label}: constant index {k} past the constant table')
            self.cmap_const[k] = self.dst.const_index(self.src.constant(k))
        return self.cmap_const[k]

    # characters
    def char(self, cid):
        """The dst id of src character cid, copying it (and what it uses) the first time."""
        cid = signed(cid)
        if cid in self.cmap:
            return self.cmap[cid]
        node = self.src.char(cid)
        if node is None:
            imp = next(((m, n) for m, n, c in self.src.imports() if c == cid), None)
            if imp is None:
                raise AptError(f'{self.src.label}: character {cid} is neither defined nor imported')
            new = next((c for m, n, c in self.dst.imports() if (m.lower(), n) == (imp[0].lower(), imp[1])), None)
            if new is None:
                new = self.dst.add_import(*imp)
                self.added.append((new, f'import {imp[0]}::{imp[1]}', cid))
            self.cmap[cid] = new
            return new
        t = node.vals[0]
        if t == 3:
            name = text(node.vals[2])
            new = next((c for c in range(len(self.dst.chartable.vals)) if self.dst.font_name(c) == name), None)
            if new is not None:
                self.cmap[cid] = new
                return new
        if t == 9:
            raise AptError('a movie cannot be grafted as a character')
        new = self.dst.add_char(None)
        self.cmap[cid] = new
        copy = self.node(node)
        self.dst.chartable.vals[new] = Ref(copy)
        self.added.append((new, apt.CHAR_TYPES.get(t, str(t)), cid))
        if t == 1:                                   # shape: geometry id = character id
            copy.vals[6] = new
            ent = self.src.geo_entry(cid)
            if ent is None:
                raise AptError(f'{self.src.label}: shape {cid} has no geometry')
            tab, k = ent
            self.dst.add_geometry(new, self.node(tab.vals[k + 2].node))
        elif t == 7:
            copy.vals[2] = new
            self.notes.append(f'image {cid} -> {new}: its pixels are an entry of the source movie\'s texture atlas '
                              f'(atlas_meta TextureFile {cid}); the target atlas needs an entry {new}')
        elif t == 3:
            self.notes.append(f'font "{text(node.vals[2])}" (char {cid}) is not in the target; copied as char {new}')
        return new

    def node(self, n):
        """A copy of src node n for dst (memoised; character ids, constants and fonts re-mapped by tag)."""
        if id(n) in self.memo:
            return self.memo[id(n)]
        if n.file == 'const':
            raise AptError('constant-file data is copied by index, not as nodes')
        if isinstance(n, Str):
            c = Str(n.file, None, n.text, n.tag)
        elif isinstance(n, Raw):
            c = Raw(n.file, None, n.data, n.tag)
        elif isinstance(n, Code):
            c = Code(n.file, None, None, n.tag)
            self.memo[id(n)] = c
            c.ins = self.stream(n.ins)
            return c
        else:
            c = Words(n.file, None, [], n.tag)
            self.memo[id(n)] = c
            c.vals = [Ref(self.node(v.node)) if isinstance(v, Ref) else v for v in n.vals]
            self._fix(c)
            return c
        self.memo[id(n)] = c
        return c

    def _fix(self, c):
        v, t = c.vals, c.tag
        if t == 'place' and v[1] & 0x02:
            v[3] = self.char(v[3])
        elif t == 'initaction':
            v[1] = self.char(v[1])
        elif t == 'text':
            v[6] = self.char(v[6])
        elif t == 'stext.records':
            for k in range(0, len(v), 14):
                v[k] = self.char(v[k])
        elif t in ('pushlist', 'poollist'):
            c.vals = [self.const(k) for k in v]
        elif t in ('geo.part4', 'geo.part5'):
            v[20] = self.char(v[20])

    def stream(self, lst, imap=None):
        """A copy of an instruction list (function bodies too), Refs copied, branch targets kept inside."""
        imap = {} if imap is None else imap
        out = []
        imap[id(lst)] = out
        for i in lst:
            j = Ins(i.op, [Ref(self.node(a.node)) if isinstance(a, Ref) else a for a in i.args], at=None)
            if i.body is not None:
                j.body = self.stream(i.body, imap)
            j.target = i.target
            imap[id(i)] = j
            out.append(j)
        for j in out:
            if j.op in BRANCH:
                j.target = imap[id(j.target)]
        return out

    def code_range(self, code, first, end):
        """A new Code node for dst: the top-level instructions of src Code node `code` from offset first up to (not
        including) offset end, after a ConstantPool of the pool constants they use, then End. Branches must stay
        inside the range; the range must not change the constant pool."""
        top = code.ins
        idx = {i.at: k for k, i in enumerate(top)}
        if first not in idx or (end not in idx and end != top[-1].at + 1):
            raise AptError(f'{first:#x} / {end:#x} are not instruction offsets of the stream at {code.at:#x}')
        a, b = idx[first], idx.get(end, len(top))
        pool = None
        for i in top[:a]:
            if i.op == 0x88:
                pool = i.args[1].node.vals if isinstance(i.args[1], Ref) else []
        part = top[a:b]
        inside = set(id(i) for i in part)
        used = []
        for i in walk_ins(part):
            if i.op == 0x88:
                raise AptError('the range sets its own constant pool')
            if i.op in BRANCH and id(i.target) not in inside and not any(i.target is x.body for x in walk_ins(part)
                                                                          if x.body is not None):
                raise AptError(f'the branch at {i.at:#x} leaves the range')
            if i.op in POOL_BYTE or i.op == 0xA3:
                if pool is None or i.args[0] >= len(pool):
                    raise AptError(f'the instruction at {i.at:#x} uses a constant-pool entry the stream has not set')
                if i.args[0] not in used:
                    used.append(i.args[0])
        used.sort()
        remap = {k: n for n, k in enumerate(used)}
        body = self.stream(part)
        for i in walk_ins(body):
            if i.op in POOL_BYTE or i.op == 0xA3:
                i.args[0] = remap[i.args[0]]
        head = Ins(0x88, [len(used), Ref(Words('apt', None, [self.const(pool[k]) for k in used], 'poollist'))])
        if len(used) > 255 and any(i.op in POOL_BYTE for i in walk_ins(body)):
            raise AptError('more than 255 pool constants')
        return Code('apt', None, [head] + body + [Ins(0)])


class Builder:
    """Writes a new action stream for a movie: b = Builder(movie); b.pool('a', 'b'); b.op('EA_PushConstant', 'a') ...;
    labels with b.label(name) / b.op('BranchIfTrue', name); b.code() -> Code node (End added)."""

    def __init__(self, movie):
        self.m = movie
        self.ins, self.labels, self.pending, self.names = [], {}, [], []

    def pool(self, *names):
        self.names = list(names)
        idx = [self.m.const_index(('s', n.encode('latin1'))) for n in names]
        self.ins.append(Ins(0x88, [len(idx), Ref(Words('apt', None, idx, 'poollist'))]))

    def label(self, name):
        self.labels[name] = len(self.ins)

    def op(self, name, *args):
        op = OPS[name] if isinstance(name, str) else name
        i = Ins(op)
        if op in POOL_BYTE:
            i.args = [self.names.index(args[0])]
        elif op in BRANCH:
            self.pending.append((i, args[0]))
        elif op in ONE_STR:
            i.args = [Ref(Str('apt', None, args[0].encode('latin1')))]
        elif op == 0xB5:
            i.args = [args[0] & 0xFF]
        elif op >= 0x80:
            raise AptError(f'Builder does not write {name}')
        self.ins.append(i)
        return self

    def code(self):
        self.ins.append(Ins(0))
        for i, name in self.pending:
            i.target = self.ins[self.labels[name]]
        return Code('apt', None, self.ins)


def place_item(movie, depth, cid, name, x, y, flags=0x26, src_item=None):
    """A new PlaceObject node (identity matrix unless copied from src_item, a dst place node)."""
    if src_item is not None:
        v = list(src_item.vals)
    else:
        v = [3, flags, 0, 0, f32(1), 0, 0, f32(1), 0, 0, 0xFFFFFFFF, 0, 0, 0, 0, 0]
    v[2], v[3] = depth & 0xFFFFFFFF, cid
    v[8], v[9] = f32(x), f32(y)
    v[13] = Ref(Str('apt', None, name.encode('latin1'))) if name else 0
    if not name:
        v[1] &= ~0x20                       # no name: no has-name flag (the game's 67,470 placements never have one)
    return Words('apt', None, v, 'place')


def free_depths(movie, count, start=2):
    used = movie.timeline_depths()
    out, d = [], start
    while len(out) < count:
        if d not in used:
            out.append(d)
        d += 1
    return out


# ------------------------------------------------------------------------------------------------ the Onslaught timer

# The console HUD (Xbox 360 hudlayer_shared, Update #3 data) - offsets of its .apt, found with apt.py's dump:
CON_TIMER, CON_LMS = 184, 146                       # characters: onslaught_timer, lastManStanding
CON_SNIPPETS = {                                     # stream offset -> [(first, end)] top-level ranges
    'timer_events': (0x3E964, 0x3EC5A, 0x3ECD5),     # frame 1: catchEvent coopRoundTime / coopOldBestRoundTime
    'lms_events': (0x3E964, 0x3F747, 0x3F79E),       # frame 1: catchEvent lmsShow / lmsHide
    'lms_init': (0x3E964, 0x3EAA7, 0x3EAAB),         # frame 1: var isLastMan = false
    'frame0_hide': (0x300DC, 0x300FD, 0x3011B),      # frame 0: lastManStanding hidden; timer hidden unless onslaught
    'frame0_hide_timer': (0x300DC, 0x30104, 0x3011B),
    'playing_vis': (0x3FAD8, 0x3FAE4, 0x3FB2B),      # frames 10 / 18 / 26 (the same code each time)
    'playing_vis_timer': (0x3FAD8, 0x3FB12, 0x3FB2B),
}


def find_place(movie, frame, name=None, depth=None):
    for it in movie.frame_items(frame):
        if it.tag == 'place' and (name is None or text(it.vals[13]) == name) and \
                (depth is None or signed(it.vals[2]) == depth):
            return it
    raise AptError(f'{movie.label}: no placement {name or depth} in frame {frame}')


def extern_reads(movie, code):
    """The extern.<name> reads of a Code node: [(instruction list, index of the push before, name)] for
    <push _global (EA_PushGlobalVar, or PushData of one register)> GetNamedMember "extern" GetNamedMember <name>."""
    out = []

    def walk(lst, pool):
        for k, i in enumerate(lst):
            if i.op == 0x88:
                pool = [movie.const_text(x) for x in i.args[1].node.vals] if isinstance(i.args[1], Ref) else []
            if i.op == 0xAF and 0 < k and k + 1 < len(lst) and lst[k + 1].op == 0xAF and i.args[0] < len(pool) \
                    and pool[i.args[0]] == 'extern' and lst[k + 1].args[0] < len(pool):
                out.append((lst, k - 1, pool[lst[k + 1].args[0]]))
            if i.body is not None:
                walk(i.body, pool)
    walk(code.ins, [])
    return out


def extern_mentions(movie, code):
    """How many instructions of a Code node name "extern" through the constant pool, in any form (PushConstant,
    PushValueOfVar, GetNamedMember, GetStringMember ...): more than extern_reads finds means reads of a shape it
    does not know."""
    n = 0

    def walk(lst, pool):
        nonlocal n
        for i in lst:
            if i.op == 0x88:
                pool = [movie.const_text(x) for x in i.args[1].node.vals] if isinstance(i.args[1], Ref) else []
            if i.op in POOL_BYTE | {0xA3} and i.args and isinstance(i.args[0], int) and i.args[0] < len(pool) \
                    and pool[i.args[0]] == 'extern':
                n += 1
            if i.body is not None:
                walk(i.body, pool)
    walk(code.ins, [])
    return n


def replace_extern(lst, k, new):
    """Replaces the three instructions of an extern read (from extern_reads) with instruction new; branches to the
    read land on new."""
    first = lst[k]
    if not (first.op == 0x71 or (first.op == 0x96 and isinstance(first.args[1], Ref)
                                 and len(first.args[1].node.vals) == 1)):
        raise AptError(f'unexpected instruction {first.name} before an extern read')
    old = set(id(i) for i in lst[k:k + 3])
    lst[k:k + 3] = [new]
    for i in lst:
        if i.op in BRANCH and id(i.target) in old:
            i.target = new


# extern.<name> reads in the console's code that the PC engine does not answer, and what replaces them. Reading an
# extern the PC exe has no entry for, then using it in a binary operator, crashed the game (2026-10-29: the timer's
# frame-0 action calls setBestTime(extern.coopOldBestRoundTime), whose first instruction compares it with undefined -
# a null object dereferenced in the VM's binary-operator handler 0x1311570, 0.24 s after the timer was placed).
EXTERN_FIX = {
    'coopOldBestRoundTime': (0x76, 'undefined: "best" shows "-" until the coopOldBestRoundTime event'),
    'isScreenHD': (0x73, 'true: the full time (the PC HUD is 1280 x 720; no PC movie reads isScreenHD)'),
}


def code_nodes(movie, new_only=False):
    return [n for n in movie.reachable()['apt'] if isinstance(n, Code) and (n.at is None or not new_only)]


# How the grafted timer is fitted to the PC HUD (2026-10-29 play test: the time spilled out of its box, the label
# showed the raw key HUD_BEST_TIME). The PC draws HUD text with bitmap fonts: header_glow = Fonts/us/font3,
# Eurostile LT Std Demi 28 with glow (us_fonts.dbx; font3_meta.dbx: digits advance 18-19 px, ":" 9 px), so
# "00:00:00:0" is about 153 px at scale 1. The console squeezes its TextManager fields non-uniformly (the time field
# 0.83 x 1.093, "best" 0.595 x 0.728, "label" 0.36 x 0.728); here the scales are uniform, so the text fits however
# the renderer treats a squeezed field, and the box is widened. (owner character, instance) -> (x scale, y scale,
# x or None); the box placement only gets a new x scale.
PC_FIT = {
    (182, 'field'): (1.0, 1.0, None),          # 153 px from x 4: ends near 160 (+ glow)
    (184, 'box'): (192 / 166, None, None),      # 166 -> 192 px wide (176 was still short on the right: play test)
    (184, 'best'): (0.65, 0.65, 176 - 198 * 0.65),   # right edge at 176, 16 px inside the box (field 198 px wide)
    (184, 'label'): (0.65, 0.65, None),         # "BEST" about 47 px from x 4
}
PC_LABEL = 'BEST'                               # no PC text says BEST / BEST TIME alone (bc2loc search "best")


def place_named(movie, owner, name):
    for f in range(len(movie.frames(owner).vals) // 2):
        for it in movie.frame_items(f, owner):
            if it.tag == 'place' and text(it.vals[13]) == name:
                return it
    raise AptError(f'no placement "{name}" in the character')


def set_clip_props(item, **props):
    """Changes the TextManager properties a placement's clip event sets: propText="..." (a string) or
    propLocalized=False (a boolean pushed with EA_PushTrue / EA_PushFalse)."""
    if not isinstance(item.vals[15], Ref):
        raise AptError('the placement has no clip events')
    ev = item.vals[15].node.vals[1].node
    for k in range(0, len(ev.vals), 3):
        lst = ev.vals[k + 2].node.ins
        for j, i in enumerate(lst[:-1]):
            if i.op == 0xA1 and text(i.args[0]) in props:
                v, nxt = props[text(i.args[0])], lst[j + 1]
                if isinstance(v, bool) and nxt.op in (0x73, 0x74):
                    nxt.op = 0x73 if v else 0x74
                elif isinstance(v, str) and nxt.op == 0xA6:
                    nxt.args = [Ref(Str('apt', None, v.encode('latin1')))]
                else:
                    raise AptError(f'unexpected code setting {text(i.args[0])}')


def set_scale(item, sx=None, sy=None, x=None):
    if sx is not None:
        item.vals[4] = f32(sx)
    if sy is not None:
        item.vals[7] = f32(sy)
    if x is not None:
        item.vals[8] = f32(x)


def add_mode_case(movie, func, mode, key, fallback='Missing DESC tag!! '):
    """Adds `if (<mode variable> == mode) result = key` ahead of the fallback of function func, a mode switch of
    the form: ... <fallback> PushConstant fallback; PushValueOfVar <mode var>; Add2; SetRegister r; Pop;
    BranchAlways <end>. Every function of that name in the movie is patched; returns their number."""
    done = 0
    for n in code_nodes(movie):
        pool = None
        for top in n.ins:
            if top.op == 0x88:
                pool = top.args[1].node
            if top.op != 0x8E or text(top.args[0]) != func or pool is None:
                continue
            names = [movie.const_text(k) for k in pool.vals]
            body = top.body
            if any(i.op == 0x88 for i in walk_ins(body)):
                # the patch reads and extends the enclosing stream's pool; a body with its own would need that one
                raise AptError(f'{func}: the function sets its own constant pool (not supported)')
            idx = next((j for j, i in enumerate(body) if i.op in (0xA2, 0xA3) and i.args[0] < len(names)
                        and names[i.args[0]] == fallback), None)
            if idx is None or idx + 5 >= len(body):
                raise AptError(f'{func}: no fallback "{fallback}"')
            var, add, setr, pop, br = body[idx + 1:idx + 6]
            if (var.op, add.op, setr.op, pop.op, br.op) != (0xAE, 0x47, 0x87, 0x17, 0x99):
                raise AptError(f'{func}: the fallback is not the expected shape')

            def pc(t):
                if t not in names:
                    pool.vals.append(movie.const_index(('s', t.encode('latin1'))))
                    names.append(t)
                k = names.index(t)
                return Ins(0xA2 if k < 256 else 0xA3, [k])
            fb = body[idx]
            block = [Ins(0xAE, list(var.args)), pc(mode), Ins(0x66), Ins(0x12), Ins(0x9D, target=fb),
                     pc(key), Ins(0x87, list(setr.args)), Ins(0x17), Ins(0x99, target=br.target)]
            if len(names) > 256 and any(i.op in POOL_BYTE and i.args[0] >= 256 for i in block):
                raise AptError('constant pool too long for a byte index')
            for i in walk_ins(body):
                if i.op in BRANCH and i.target is fb:
                    i.target = block[0]
            block[4].target = fb
            body[idx:idx] = block
            done += 1
    if not done:
        raise AptError(f'{movie.label}: no function {func}')
    return done


def add_mode_alias(movie, mode, like, funcs=None):
    """Makes mode take the branch of mode like in mode switches of the form <push the mode value: PushData of one
    register or PushValueOfVar>; PushConstant like; StrictEquals (or Equals2); BranchIfTrue X - a copy of the four
    with mode is put ahead of them. The first compare of a switch reads <value>; SetRegister r; PushConstant like ...:
    there the copy pushes register r and goes after the compare. funcs: names of the functions to patch (None: every
    function, named or not, with such a switch). Returns the number of switches patched."""
    done = seen = 0

    def patch(body, names, pool):
        nonlocal done, seen

        def pc(t):
            if t not in names:
                pool.vals.append(movie.const_index(('s', t.encode('latin1'))))
                names.append(t)
            k = names.index(t)
            return Ins(0xA2 if k < 256 else 0xA3, [k])
        j = 0
        while j + 3 < len(body):
            v, c, eq, br = body[j:j + 4]
            is_mode = lambda i: i.op in (0xA2, 0xA3) and i.args[0] < len(names) and names[i.args[0]] == mode  # noqa: E731
            if (c.op in (0xA2, 0xA3) and c.args[0] < len(names) and names[c.args[0]] == like and eq.op in (0x66, 0x49) and br.op == 0x9D and
                    (v.op in (0xAE, 0x87) or (v.op == 0x96 and v.args[0] == 1 and isinstance(v.args[1], Ref)))):
                after = v.op == 0x87                             # SetRegister r: the switch's first compare
                if (after and j + 5 < len(body) and is_mode(body[j + 5])) or \
                        (not after and j >= 4 and is_mode(body[j - 3])):
                    seen += 1                                    # already there
                    j += 4
                    continue
                if after:
                    first = Ins(0x96, [1, Ref(Words('apt', None, [movie.const_index((4, v.args[0]))], 'pushlist'))])
                elif v.op == 0xAE:
                    first = Ins(0xAE, list(v.args))
                else:
                    first = Ins(0x96, [1, Ref(Words('apt', None, list(v.args[1].node.vals), 'pushlist'))])
                block = [first, pc(mode), Ins(eq.op), Ins(0x9D, target=br.target)]
                if len(names) > 256 and any(i.op in POOL_BYTE and i.args[0] >= 256 for i in block):
                    raise AptError('constant pool too long for a byte index')
                if after:
                    body[j + 4:j + 4] = block                    # nothing branches into the middle of a compare
                else:
                    for i in walk_ins(body):
                        if i.op in BRANCH and i.target is v:
                            i.target = first
                    body[j:j] = block
                done += 1
                j += 8
            else:
                j += 1

    def walk(lst, pool, names):
        for i in lst:
            if i.op == 0x88:
                pool = i.args[1].node
                names = [movie.const_text(k) for k in pool.vals]
            if i.body is not None:
                bpool, bnames = pool, names
                if i.body and i.body[0].op == 0x88:                  # a body with its own constant pool
                    bpool = i.body[0].args[1].node
                    bnames = [movie.const_text(k) for k in bpool.vals]
                name = text(i.args[0]) if i.op == 0x8E else ''
                if bpool is not None and (funcs is None or name in funcs):
                    patch(i.body, bnames, bpool)
                walk(i.body, bpool, bnames)

    for n in code_nodes(movie):
        walk(n.ins, None, None)
    if not done and not seen:
        raise AptError(f'{movie.label}: no "{like}" switch' + (f' in {", ".join(funcs)}' if funcs else ''))
    return done


def onslaught_timer(pc, con, lms=True, log=print, place=True):
    """Grafts the console's Onslaught stopwatch (and the last-man-standing banner) into pc (a Movie, changed in
    place). place=False: the characters, events and visibility code only, nothing placed (a diagnostic variant).
    Returns (report lines, Grafter)."""
    pc_externs = {name for n in code_nodes(pc) for _, _, name in extern_reads(pc, n)}
    if pc.big or not con.big:
        raise AptError('give the PC movie (little-endian) first, then the console one (big-endian)')
    for cid, want in ((CON_TIMER, 'onslaught_timer'), (CON_LMS, 'lastManStanding')):
        if con.char_type(cid) != 5:
            raise AptError(f'{con.label}: character {cid} is not a sprite - not the expected console movie')
    con_place0 = {text(i.vals[13]): i for i in con.frame_items(0) if i.tag == 'place'}
    if 'lastManStanding' not in con_place0:
        raise AptError(f'{con.label}: frame 0 places no lastManStanding - not the expected console movie')
    for key, (stream, first, end) in CON_SNIPPETS.items():
        if ('apt', stream, 'c') not in con.cache:
            raise AptError(f'{con.label}: no action stream at {stream:#x} - not the expected console movie')
    names = {text(i.vals[13]) for f in range(len(pc.frames().vals) // 2) for i in pc.frame_items(f)
             if i.tag == 'place'}
    for n in ('onslaught_timer', 'lastManStanding'):
        if n in names:
            raise AptError(f'{pc.label} already has an instance "{n}"')
    g = Grafter(con, pc)
    report = []
    labels = {}
    for lab in ('init', 'playing', 'playing_empty', 'playing_hardcore', 'notplaying', 'killed', 'menu'):
        labels[lab] = pc.label_frame(lab)
        if con.label_frame(lab) != labels[lab]:
            raise AptError(f'frame label {lab}: frame {labels[lab]} on PC, {con.label_frame(lab)} on the console')
    # depths: the console uses 95 (playing) and 91 (killed) for the timer, 93 for the banner; PC uses other numbers
    d_play, d_kill, d_lms = free_depths(pc, 3, start=94)
    report.append(f'depths on the PC timeline: timer {d_play} (playing), {d_kill} (killed), banner {d_lms}')

    def snippet(key):
        stream, first, end = CON_SNIPPETS[key]
        return g.code_range(con.code(stream), first, end)

    def action(code):
        return Words('apt', None, [1, Ref(code)], 'action')

    # the characters
    tid = g.char(CON_TIMER)
    lid = g.char(CON_LMS) if lms else None
    # frame 0: the banner placed (hidden); the timer hidden unless onslaught (it is placed from "playing" on)
    if lms and place:
        src = g.node(con_place0['lastManStanding'])
        src.vals[2] = d_lms
        pc.add_item(0, src)
    pc.add_item(0, action(snippet('frame0_hide' if lms else 'frame0_hide_timer')))
    # event handlers (the engine sends coopRoundTime(time, running), coopOldBestRoundTime(time), lmsShow, lmsHide): the
    # console registers them in frame 1 ("init"), but the PC movie never runs frame 1 - its frame-0 script ends with
    # GotoLabel "playing" / "void" and Stop, and the PC's own catchEvent calls are all in frame 0. So in frame 0, ahead
    # of that script (the first play test of the graft: the handlers never registered, the clock stayed at zero)
    first = next(k for k, it in enumerate(pc.frame_items(0)) if it.tag == 'action')
    inits = [snippet('timer_events')] + ([snippet('lms_init'), snippet('lms_events')] if lms else [])
    for k, code in enumerate(inits):
        pc.add_item(0, action(code), index=first + k)
    # playing: placed at the console's spot; playing_empty: shown again by the same code; hardcore: moved down
    f = labels['playing']
    item = g.node(find_place(con, f, 'onslaught_timer'))
    item.vals[2] = d_play
    if place:
        pc.add_item(f, item)
    vis = 'playing_vis' if lms else 'playing_vis_timer'
    for lab in ('playing', 'playing_empty', 'playing_hardcore'):
        pc.add_item(labels[lab], action(snippet(vis)))
    mv = g.node(find_place(con, labels['playing_hardcore'], depth=95))
    mv.vals[2] = d_play
    if place:
        pc.add_item(labels['playing_hardcore'], mv)
    # notplaying: removed (the banner too, as the console does)
    pc.add_item(labels['notplaying'], Words('apt', None, [4, d_play], 'remove'))
    if lms:
        pc.add_item(labels['notplaying'], Words('apt', None, [4, d_lms], 'remove'))
    # killed: placed at the top centre, without its box in onslaught, hidden otherwise (the console also hides
    # "squad" outside onslaught there - not taken: PC's squad list is left as it is)
    f = labels['killed']
    item = g.node(find_place(con, f, 'onslaught_timer'))
    item.vals[2] = d_kill
    if place:
        pc.add_item(f, item)
    b = Builder(pc)
    b.pool('extern', 'gameModeID', 'onslaught', 'onslaught_timer', '_visible', 'box')
    (b.op('EA_PushGlobalVar').op('EA_GetNamedMember', 'extern').op('EA_GetNamedMember', 'gameModeID')
      .op('EA_PushConstant', 'onslaught').op('Equals2').op('Not').op('Not').op('BranchIfTrue', 'ons')
      .op('EA_PushValueOfVar', 'onslaught_timer').op('EA_PushConstant', '_visible').op('EA_PushFalse')
      .op('SetMember').op('BranchAlways', 'done'))
    b.label('ons')
    (b.op('EA_PushValueOfVar', 'onslaught_timer').op('EA_GetNamedMember', 'box').op('EA_PushConstant', '_visible')
      .op('EA_PushFalse').op('SetMember'))
    b.label('done')
    pc.add_item(f, action(b.code()))
    # menu: removed
    pc.add_item(labels['menu'], Words('apt', None, [4, d_kill], 'remove'))
    # fitted to the PC font, the label a literal (PC_FIT, PC_LABEL)
    for (owner, name), (sx, sy, x) in PC_FIT.items():
        set_scale(place_named(pc, pc.char(g.cmap[owner]), name), sx, sy, x)
    set_clip_props(place_named(pc, pc.char(g.cmap[CON_TIMER]), 'label'), propText=PC_LABEL, propLocalized=False)
    report.append(f'fitted to the PC font: time field uniform 1.0, box 192 px wide, best / label 0.65; label "{PC_LABEL}"')
    # extern reads the PC engine does not answer: replaced; any other new one refused
    for n in code_nodes(pc, new_only=True):
        reads = extern_reads(pc, n)
        if extern_mentions(pc, n) > len(reads):
            # a read of another shape would get past the check below (the first timer crashed the game that way)
            raise AptError('the grafted code reads extern in a form the graft does not check; refused')
        for lst, k, name in reversed(reads):
            if name in EXTERN_FIX:
                replace_extern(lst, k, Ins(EXTERN_FIX[name][0]))
                report.append(f'extern.{name} replaced by {EXTERN_FIX[name][1]}')
            elif name not in pc_externs:
                raise AptError(f'the grafted code reads extern.{name}, which no code of the PC movie reads')
    if not place:
        report.append('variant: nothing placed (characters, events and visibility code only)')
    report.append('characters added: ' + ', '.join(f'{n} ({k}, console {s})' for n, k, s in g.added))
    report += [f'needs: {n}' for n in g.notes]
    return report, g


# ------------------------------------------------------------------------------------------------ dependencies

def deps(movie, cid):
    """What character cid uses: {'chars': {id: type}, 'imports': [...], 'fonts': [...], 'images': [...],
    'geometry': [...], 'constants': [...], 'strings': n}."""
    out = {'chars': {}, 'imports': [], 'fonts': [], 'images': [], 'geometry': [], 'constants': set()}
    imps = {c: (m, n) for m, n, c in movie.imports()}
    seen = set()

    def visit_char(c):
        c = signed(c)
        if c in out['chars'] or c in [i[2] for i in out['imports']]:
            return
        node = movie.char(c)
        if node is None:
            if c in imps:
                out['imports'].append((*imps[c], c))
            return
        t = node.vals[0]
        out['chars'][c] = apt.CHAR_TYPES.get(t, str(t))
        if t == 3:
            out['fonts'].append((c, text(node.vals[2])))
        if t == 7:
            out['images'].append(c)
        if t == 1:
            ent = movie.geo_entry(c)
            if ent:
                out['geometry'].append(c)
                visit(ent[0].vals[ent[1] + 2].node)
        visit(node)

    def visit(n):
        if id(n) in seen:
            return
        seen.add(id(n))
        if isinstance(n, Words):
            v = n.vals
            if n.tag == 'place' and v[1] & 2:
                visit_char(v[3])
            elif n.tag == 'initaction':
                visit_char(v[1])
            elif n.tag == 'text':
                visit_char(v[6])
            elif n.tag == 'stext.records':
                for k in range(0, len(v), 14):
                    visit_char(v[k])
            elif n.tag in ('geo.part4', 'geo.part5'):
                visit_char(v[20])
            elif n.tag in ('pushlist', 'poollist'):
                out['constants'].update(v)
        for r in n.refs():
            visit(r)
    visit_char(cid)
    out['constants'] = sorted(out['constants'])
    return out


# ------------------------------------------------------------------------------------------------ checks

def dump_text(members):
    """apt.py's dump of a written movie, with offsets taken out (for comparing layouts)."""
    d = dict(members)
    aptname = next(n for n in d if n.lower().endswith('.apt'))
    name = aptname[:-4]
    m = apt.Movie(data=(name, d[aptname], d[name + '.const']))
    lines = m.dump()
    strip = re.compile(r'@0x[0-9a-f]+|@[0-9A-Fa-f]+|\[list 0x[0-9a-f]+\]|-> 0x[0-9a-f]+|^\s*[0-9A-F]{6}  ')
    return [strip.sub('', ln.strip()) for ln in lines]


def roundtrip(spec, pack=False):
    """(problems, notes) for one movie: no problems = it round-trips. Notes: data no structure points at (left by
    in-place patches; written back as it was)."""
    label, members = load_members(spec)
    orig = write_bigf(members)
    problems = []
    if os.path.isfile(spec) and not spec.lower().endswith('.apt'):
        raw = open(spec, 'rb').read()
        if raw != orig:
            problems.append('the BIGF container is laid out differently from this writer\'s')
    m = Movie(members, label)
    notes = [f'{f}: {t}' for f, t in m.leftovers()]
    problems += [n for n in notes if 'overlaps' in n]
    out = m.write()
    for (n, a), (_, b) in zip(members, out):
        if a != b:
            k = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), min(len(a), len(b)))
            problems.append(f'{n}: written differently from offset {k:#x} (sizes {len(a)} / {len(b)})')
    if pack and not problems:
        packed = m.write(layout='pack', gap=16)
        if any(len(b) <= len(a) for (n, a), (_, b) in zip(members, packed)
               if n in m.names.values()):
            problems.append('the packed layout did not move the data')
        if dump_text(packed) != dump_text(members):
            problems.append('packed layout: apt.py reads a different movie')
        m2 = Movie(packed, label + ' (packed)')
        if m2.leftovers() or m2.write() != packed:
            problems.append('packed layout: not read back completely')
        other = '<' if m.big else '>'
        conv = Movie(m.write(order=other), label + ' (other byte order)')
        if conv.write(order=m.e) != m.write(dead=False):
            problems.append('converting to the other byte order and back changes the movie')
    return problems, notes


# ------------------------------------------------------------------------------------------------ command line

def take(args, flag, default=None, conv=str):
    if flag in args:
        i = args.index(flag)
        if i + 1 >= len(args):
            raise AptError(f'{flag} needs a value')
        v = args[i + 1]
        del args[i:i + 2]
        return conv(v)
    return default


def take_flag(args, flag):
    if flag in args:
        args.remove(flag)
        return True
    return False


def main(argv):
    args = argv[1:]
    if not args or args[0] not in ('roundtrip', 'convert', 'deps', 'graft', 'onslaught-timer', 'mode-case', 'mode-alias'):
        print(__doc__)
        return 1
    cmd = args.pop(0)
    if cmd == 'roundtrip':
        pack, quiet = take_flag(args, '--pack'), take_flag(args, '--quiet')
        files = movie_files(args)
        if not files:
            raise AptError('no movies found')
        bad = dead = 0
        import hashlib
        seen = {}
        for f in files:
            key = hashlib.md5(open(f, 'rb').read()).hexdigest() if os.path.isfile(f) else f
            if key in seen:
                continue
            seen[key] = f
            try:
                probs, notes = roundtrip(f, pack)
            except AptError as e:
                probs, notes = [str(e)], []
            dead += bool(notes)
            bad += bool(probs)
            if probs or not quiet:
                print(f'{"FAIL" if probs else "ok  "} {f}' + ''.join(f'\n       {p}' for p in probs)
                      + ''.join(f'\n       note: {p}' for p in notes[:4]) + (' ...' if len(notes) > 4 else ''))
        print(f'{len(seen) - bad} of {len(seen)} different movies ({len(files)} files) round-trip byte for byte'
              + (' and packed' if pack else '') + (f'; {dead} hold data no structure points at (kept as it was)'
                                                    if dead else ''))
        return 1 if bad else 0
    if cmd == 'convert':
        order = '<' if take_flag(args, '--le') else '>' if take_flag(args, '--be') else None
        layout = 'pack' if take_flag(args, '--pack') else 'keep'
        if len(args) != 2:
            raise AptError('convert <movie> <out.res>')
        m = Movie.load(args[0])
        order = order or ('<' if m.big else '>')
        open(args[1], 'wb').write(m.to_res(order, layout))
        print(f'{args[1]}: {"big" if order == ">" else "little"}-endian, layout {layout}')
        return 0
    if cmd == 'deps':
        if len(args) != 2:
            raise AptError('deps <movie> <char>')
        m = Movie.load(args[0])
        d = deps(m, int(args[1], 0))
        print(f'character {args[1]} of {m.label}:')
        print('  characters: ' + ', '.join(f'{c} {t}' for c, t in sorted(d['chars'].items())))
        print('  imports:    ' + (', '.join(f'{c} {mv}::{n}' for mv, n, c in d['imports']) or 'none'))
        print('  fonts:      ' + (', '.join(f'{c} "{n}"' for c, n in d['fonts']) or 'none'))
        print('  images:     ' + (', '.join(map(str, d['images'])) or 'none (no texture atlas entries needed)'))
        print('  geometry:   ' + (', '.join(map(str, d['geometry'])) or 'none'))
        print(f'  constants:  {len(d["constants"])}: ' + ', '.join(m.const_text(k) for k in d['constants'][:60])
              + (' ...' if len(d['constants']) > 60 else ''))
        return 0
    if cmd == 'graft':
        frame = take(args, '--frame', 0, int)
        depth = take(args, '--depth', None, int)
        name = take(args, '--name', None)
        x, y = take(args, '--x', 0.0, float), take(args, '--y', 0.0, float)
        if len(args) != 4:
            raise AptError('graft <target> <source> <char> <out.res>')
        dst, src = Movie.load(args[0]), Movie.load(args[1])
        frames = len(dst.frames().vals) // 2
        if not 0 <= frame < frames:
            raise AptError(f'--frame {frame}: the target\'s main timeline has frames 0-{frames - 1}')
        g = Grafter(src, dst)
        cid = g.char(int(args[2], 0))
        depth = depth if depth is not None else free_depths(dst, 1)[0]
        dst.add_item(frame, place_item(dst, depth, cid, name, x, y))
        open(args[3], 'wb').write(dst.to_res(layout=GRAFT_LAYOUT))
        print(f'{args[3]}: character {args[2]} -> {cid}, placed in frame {frame} at depth {depth}')
        for n, k, s in g.added:
            print(f'  added {n}: {k} (source {s})')
        for n in g.notes:
            print(f'  needs: {n}')
        return 0
    if cmd == 'mode-case':
        func, mode, key = take(args, '--function'), take(args, '--mode'), take(args, '--key')
        fallback = take(args, '--fallback', 'Missing DESC tag!! ')
        if len(args) != 2 or not (func and mode and key):
            raise AptError('mode-case <movie> <out.res> --function F --mode M --key K')
        m = Movie.load(args[0])
        n = add_mode_case(m, func, mode, key, fallback)
        open(args[1], 'wb').write(m.to_res(layout=GRAFT_LAYOUT))
        print(f'{args[1]}: {func} ({n}) returns {key} for {mode}')
        return 0
    if cmd == 'mode-alias':
        mode, like, funcs = take(args, '--mode'), take(args, '--like'), take(args, '--function')
        if len(args) != 2 or not (mode and like):
            raise AptError('mode-alias <movie> <out.res> --mode M --like L [--function F[,F...]]')
        m = Movie.load(args[0])
        n = add_mode_alias(m, mode, like, funcs.split(',') if funcs else None)
        open(args[1], 'wb').write(m.to_res(layout=GRAFT_LAYOUT))
        print(f'{args[1]}: {n} switch(es) send {mode} the way of {like}')
        return 0
    if cmd == 'onslaught-timer':
        lms = not take_flag(args, '--no-lms')
        place = not take_flag(args, '--no-place')
        if len(args) != 3:
            raise AptError('onslaught-timer <PC hudlayer_shared> <console hudlayer_shared> <out.res>')
        pc, con = Movie.load(args[0]), Movie.load(args[1])
        report, _ = onslaught_timer(pc, con, lms, place=place)
        # .apt / .geo: old data where it was, new after it; .const: the entry table grows in place (its strings
        # move behind it), the shape the game's own constant files have
        open(args[2], 'wb').write(pc.to_res(layout=GRAFT_LAYOUT))
        for ln in report:
            print(ln)
        print(f'{args[2]}: written')
        return 0
    return 1


if __name__ == '__main__':
    from bfbc2core import errors
    errors.setup('aptgraft', gui=False)        # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
