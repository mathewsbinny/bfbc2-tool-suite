r"""Shared helpers of the menu tools: finding Project OPFOR BFBC2's game copy, reading menu movies from files or straight from
the game's bundles (read only), and OPFOR's menu patch format.

Movie names on the command line:
  a path            a menu .res (BIGF archive of <name>.apt / .const / .geo), or a .apt with its .const beside it
  <name>            Menu/<name>.res of the main menu bundle the game loads (Package\menu\main\mainmenu-00.fbrb over
                    Dist\win32\menu\main\mainmenu-00.fbrb), e.g. main_nav
  pristine:<name>   the same from OPFOR's pristine copy (original\Package\menu\main\mainmenu-00.fbrb, kept by
                    build_menus.bat), i.e. before OPFOR's menu patches
  <bundle.fbrb>:<name>   Menu/<name>.res of any bundle
Nothing is ever written into the game folder.
"""
import os
import struct
import sys
from collections.abc import Iterable, Mapping
from typing import Any, cast

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('apt', 'core')

import apt  # noqa: E402  (the suite's APT reader)
from bfbc2core import paths  # noqa: E402

MENU_BUNDLE = 'menu/main/mainmenu-00.fbrb'
# bundles with menu movies: (label, path under Dist\win32 and Package); Package entries win over Dist's
MOVIE_SETS = [('menu', 'menu/main/mainmenu-00.fbrb'), ('mp_hud', 'levels/mp_common/ingame-00.fbrb'),
              ('mp_loader', 'levels/mp_common/loader-00.fbrb'), ('sp_hud', 'levels/sp_common/ingame-00.fbrb'),
              ('sp_loader', 'levels/sp_common/loader-00.fbrb'), ('mp2_hud', 'levels/mp2_common/ingame-00.fbrb'),
              ('nam_hud', 'levels/nam_common/ingame-00.fbrb')]

NewByte = int | tuple[str, str]               # a patch line's new byte, or ('label', name): an appended string's offset
Section = tuple[str, int, int, int, int, int]  # PEImage.sections: (name, va, virtual size, file offset, raw size, flags)


class MenuToolError(Exception):
    """A problem with the input the user can fix (explained in words by errors.guard)."""


def find_opfor(given: str | None = None) -> str:
    """The Project OPFOR BFBC2 folder: given, else OPFOR_ROOT, else a "project-opfor-bfbc2" folder next to one of the folders
    this tool sits in. bfbc2core.paths.find_opfor, walking up from the suite; the message does not list the folders."""
    try:
        return paths.find_opfor(given, start=SUITE)
    except paths.GameCopyNotFound:
        raise MenuToolError('Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT') from None


def out_dir(root: str) -> str:
    """Where outputs made from game data go: <OPFOR>\\work\\tools_out (never the game folder)."""
    d = os.path.join(root, 'work', 'tools_out')
    os.makedirs(d, exist_ok=True)
    return d


def bundle_layers(root: str, rel: str) -> list[str]:
    """The existing copies of a bundle, lowest first: Dist\\win32\\<rel>, then Package\\<rel>."""
    out = []
    for p in (os.path.join(root, 'game', 'Dist', 'win32', *rel.split('/')), os.path.join(root, 'game', 'Package', *rel.split('/'))):
        if os.path.isfile(p):
            out.append(p)
    return out


def bundle_movies(paths: Iterable[str]) -> dict[str, tuple[str, str]]:
    """{movie name: (bundle path, entry name)} of the SwfMovie entries Menu/<name>.res in the bundles (later win;
    a *deleted* marker removes)."""
    from bfbc2core import fbrb
    out: dict[str, tuple[str, str]] = {}
    for p in paths:
        for e in fbrb.entries(p):
            low = e.name.lower()
            if low.startswith('menu/') and low.endswith('.res') and '/' not in low[5:]:
                name = e.name[5:-4]
                if e.deleted:
                    out.pop(name.lower(), None)
                elif e.type == 'SwfMovie':
                    out[name.lower()] = (p, e.name)
    return out


def read_entry(bundle: str, entry: str) -> bytes:
    from bfbc2core import fbrb
    return fbrb.read(bundle, [entry])[entry]


def parse_bigf(b: bytes, where: str = 'file') -> dict[str, bytes]:
    """BIGF members {name: bytes} and their order (apt.read_bigf, from bytes), read in the Rust core (round F0:
    bc2_native.bigf_parse). Its Python reader (parse_bigf_reference) was retired in round RT2 against its recorded
    answers (bfbc2-menu-tools\\tests\\f0_bigf_replay*.json): an archive the core does not read raises a MenuToolError
    that is also a bfbc2core.native.Refused."""
    if isinstance(b, bytearray | memoryview):
        b = bytes(b)
    got = _core('bigf_parse').bigf_parse(b) if type(b) is bytes else None
    if got is None:
        if b[:4] != b'BIGF':
            raise MenuToolError(f'{where} is not a menu movie (.res, a BIGF archive)')
        raise _refused(f'{where}: a BIGF archive cut short or damaged (its index runs past the data)')
    return cast(dict[str, bytes], got)


def _core(attr: str) -> Any:
    """The Rust core (bfbc2core.native.require): NativeMissing, saying how to get it, when it is not there."""
    from bfbc2core import native
    return native.require('the menu tools (bfbc2-menu-tools/menucommon.py)', attr)


def _refused(message: str) -> Exception:
    from bfbc2core import native
    return native.refused(message, MenuToolError)


def resolve(spec: str, root: str | None = None) -> tuple[str, dict[str, bytes]]:
    """A movie spec (module doc) -> (label, {member: bytes}). Members of a .apt given alone: .apt and .const."""
    if os.path.isfile(spec):
        if spec.lower().endswith('.apt'):
            base = os.path.splitext(spec)[0]
            name = os.path.basename(base)
            if not os.path.isfile(base + '.const'):
                raise MenuToolError(f'{spec}: its constant file {name}.const is missing beside it')
            return spec, {name + '.apt': open(spec, 'rb').read(), name + '.const': open(base + '.const', 'rb').read()}
        return spec, parse_bigf(open(spec, 'rb').read(), spec)
    if '.fbrb:' in spec.lower():
        bundle, name = spec.rsplit(':', 1)
        if not os.path.isfile(bundle):
            raise FileNotFoundError(bundle)
        movies = bundle_movies([bundle])
    elif spec.lower().startswith('pristine:'):
        root = root or find_opfor()
        bundle = os.path.join(root, 'original', *('Package/' + MENU_BUNDLE).split('/'))
        if not os.path.isfile(bundle):
            raise MenuToolError(f'no pristine menu bundle at {bundle} (OPFOR\'s build_menus.bat keeps it there)')
        name = spec.split(':', 1)[1]
        movies = bundle_movies([bundle])
    elif os.sep in spec or '/' in spec or spec.lower().endswith(('.res', '.apt')):
        raise FileNotFoundError(spec)
    else:
        root = root or find_opfor()
        name = spec
        movies = bundle_movies(bundle_layers(root, MENU_BUNDLE))
    key = name.lower()[:-4] if name.lower().endswith('.res') else name.lower()
    if key not in movies:
        raise MenuToolError(f'no menu movie "{name}" in the bundle (known: {", ".join(sorted(movies)[:12])} ...)')
    bundle, entry = movies[key]
    return f'{bundle}:{entry}', parse_bigf(read_entry(bundle, entry), entry)


def movie_from_members(members: Mapping[str, bytes], label: str = 'movie') -> apt.Movie:
    """An apt.Movie from BIGF members (or a .apt / .const pair)."""
    aptname = next((k for k in members if k.lower().endswith('.apt')), None)
    if aptname is None:
        raise MenuToolError(f'{label} has no .apt member (members: {", ".join(members)})')
    name = aptname[:-4]
    const = next((k for k in members if k.lower() == (name + '.const').lower()), None)
    if const is None:
        raise MenuToolError(f'{label} has no {name}.const member')
    try:
        return apt.Movie(data=(name, members[aptname], members[const]))
    except (ValueError, struct.error) as e:
        raise MenuToolError(f'{label} is not a readable APT movie: {e}') from e


# ------------------------------------------------------------------------------------------------ patch files

class PatchLine:
    def __init__(self, number: int, raw: str, kind: str, member: str, offset: int | None = None, expected: bytes = b'',
                 new: list[NewByte] | None = None, label: str | None = None, text: str | None = None,
                 comment: str = '') -> None:
        self.number, self.raw, self.kind, self.member = number, raw, kind, member
        self.offset, self.expected, self.new, self.label, self.text, self.comment = offset, expected, new, label, text, comment

    def where(self) -> str:
        return f'line {self.number}'


def parse_patch(path: str) -> tuple[list[PatchLine], list[tuple[int, str]]]:
    """OPFOR's menu patch format (src\\tools\\apt\\AptTool.cs, the reference):
      <member> <hex offset> <expected hex bytes> -> <new hex bytes | @label>   # comment
      append <member> <label> "text"                                          # comment
    Returns ([PatchLine], [(line number, problem)]); new bytes may hold ('label', name) items."""
    lines: list[PatchLine]
    problems: list[tuple[int, str]]
    lines, problems = [], []
    above = ''                                     # the comment block just above a line (OPFOR comments groups so)
    for n, raw in enumerate(open(path, encoding='utf-8', errors='replace').read().splitlines(), 1):
        line = raw.strip()
        if not line:
            above = ''
            continue
        if line.startswith('#'):
            above = line[1:].strip() or above
            continue
        comment_from = line.rfind('"') if line.startswith('append ') else 0
        h = line.find('#', max(comment_from, 0))
        comment = line[h + 1:].strip() if h >= 0 else above
        if h >= 0:
            line = line[:h].strip()
        if line.startswith('append '):
            q1, q2 = line.find('"'), line.rfind('"')
            head = line[:q1].split() if q1 > 0 else line.split()
            if q1 < 0 or q2 <= q1 or len(head) != 3:
                problems.append((n, 'an append line is: append <member> <label> "text"'))
                continue
            text = line[q1 + 1:q2]
            if any(ord(ch) > 127 for ch in text):
                problems.append((n, 'append text must be ASCII (AptTool writes it as ASCII)'))
            lines.append(PatchLine(n, raw, 'append', head[1], label=head[2], text=text, comment=comment))
            continue
        if line.count('->') != 1:
            problems.append((n, 'a patch line is: <member> <hex offset> <expected bytes> -> <new bytes>'))
            continue
        left, right = (s.split() for s in line.split('->'))
        if len(left) < 3:
            problems.append((n, 'missing member, offset or expected bytes'))
            continue
        try:
            offset = int(left[1], 16)
            expected = bytes(int(t, 16) for t in left[2:])
            if any(len(t) != 2 for t in left[2:]):
                raise ValueError
        except ValueError:
            problems.append((n, f'offset and expected bytes must be hex (bytes as two digits): {" ".join(left[1:])}'))
            continue
        new: list[NewByte] = []
        bad = False
        for t in right:
            if t.startswith('@'):
                new.append(('label', t[1:]))
            elif len(t) == 2 and all(ch in '0123456789abcdefABCDEF' for ch in t):
                new.append(int(t, 16))
            else:
                bad = True
        if bad or not new:
            problems.append((n, f'new bytes must be hex bytes (two digits) or @label: {" ".join(right)}'))
            continue
        lines.append(PatchLine(n, raw, 'patch', left[0], offset, expected, new, comment=comment))
    return lines, problems


def apply_patch(members: Mapping[str, bytes], lines: Iterable[PatchLine]
                ) -> tuple[dict[str, bytes], dict[str, set[int]], list[tuple[int, str]]]:
    """Applies parsed patch lines to BIGF members as AptTool does (appends 4-byte aligned, @label = the appended
    string's offset, little-endian; every expected byte checked). Returns (new members, {member: set of changed
    offsets the lines name}, [(line number, problem)])."""
    data = {k: bytearray(v) for k, v in members.items()}
    lower = {k.lower(): k for k in data}
    labels: dict[str, int]
    named: dict[str, set[int]]
    problems: list[tuple[int, str]]
    labels, named, problems = {}, {}, []
    for pl in lines:
        m = lower.get(pl.member.lower())
        if m is None:
            problems.append((pl.number, f'no member {pl.member} in the movie (members: {", ".join(data)})'))
            continue
        buf = data[m]
        if pl.kind == 'append':
            assert pl.label is not None and pl.text is not None   # parse_patch sets both on an append line
            while len(buf) % 4:
                buf.append(0)
            if pl.label in labels:
                problems.append((pl.number, f'label {pl.label} defined twice'))
            labels[pl.label] = len(buf)
            buf += pl.text.encode('ascii', 'replace') + b'\0'
            continue
        assert pl.offset is not None and pl.new is not None    # ... and offset and new bytes on a patch line
        new = bytearray()
        for t in pl.new:
            if isinstance(t, tuple):
                if t[1] not in labels:
                    problems.append((pl.number, f'undefined label @{t[1]} (an append line must define it first)'))
                    new += b'\0\0\0\0'
                else:
                    new += struct.pack('<I', labels[t[1]])
            else:
                new.append(t)
        if len(new) != len(pl.expected):
            problems.append((pl.number, f'size change: {len(pl.expected)} expected byte(s), {len(new)} new - patches must keep '
                                        'the size (AptTool refuses it)'))
            continue
        if pl.offset + len(pl.expected) > len(buf):
            problems.append((pl.number, f'offset {pl.offset:X} + {len(pl.expected)} is past the end of {m} ({len(buf):X})'))
            continue
        found = bytes(buf[pl.offset:pl.offset + len(pl.expected)])
        if found != pl.expected:
            problems.append((pl.number, f'{m} {pl.offset:X}: expected {pl.expected.hex(" ").upper()}, found '
                                        f'{found.hex(" ").upper()} - wrong source movie, or the line overlaps an earlier one'))
            continue
        buf[pl.offset:pl.offset + len(new)] = new
        named.setdefault(m, set()).update(range(pl.offset, pl.offset + len(new)))
    return {k: bytes(v) for k, v in data.items()}, named, problems


# ------------------------------------------------------------------------------------------------ the exe (data only)

def _pe_native() -> Any:
    """The Rust core with the executable reader (round F2b: bc2_harness::pe::raw): NativeMissing, saying how to get
    it, when it is not there."""
    return _core('pe_raw_strings')


class PEImage:
    """A 32-bit Windows exe's sections, for reading its data (strings, tables) - no code is decoded here.

    The header, strings, find_cstring and find_dword run in the suite's Rust core (round F2b,
    rust/bc2-harness/src/pe.rs, raw). Their Python (the *_reference methods) was retired in round RT2 against its
    recorded answers (bfbc2-menu-tools\\tests\\f2b_pe_replay*.json): a header the core cannot read (cut short or
    damaged) raises a MenuToolError that is also a bfbc2core.native.Refused, as does an argument it does not take
    (a text that is not ASCII, a value outside 32 bits)."""

    def __init__(self, path: str) -> None:
        self.path = path
        b = self.b = open(path, 'rb').read()
        h = _pe_native().pe_raw_header(b)
        if h is None:
            raise _refused(f'{path}: a Windows program cut short or damaged (its headers run past the file)')
        if h[0] == 'not_exe':
            raise MenuToolError(f'{path} is not a Windows program (.exe)')
        if h[0] == 'not_32':
            raise MenuToolError(f'{path} is not a 32-bit Windows program')
        _, self.timestamp, self.base, secs = h
        self.sections: list[Section] = [cast(Section, tuple(s)) for s in secs]

    def off(self, va: int) -> int | None:
        for _, sva, vs, ra, rs, _ in self.sections:
            if sva <= va < sva + min(vs, rs) if vs else sva <= va < sva + rs:
                return va - sva + ra
        return None

    def va(self, off: int) -> int | None:
        for _, sva, _vs, ra, rs, _ in self.sections:
            if ra <= off < ra + rs:
                return sva + off - ra
        return None

    def is_code(self, va: int) -> bool:
        return any(sva <= va < sva + vs and fl & 0x20000000 for _, sva, vs, _, _, fl in self.sections)

    def cstr(self, va: int, limit: int = 200) -> str | None:
        """The printable ASCII C string at va, or None."""
        o = self.off(va) if va else None
        if o is None:
            return None
        e = self.b.find(b'\0', o, o + limit + 1)
        if e < 0:
            return None
        s = self.b[o:e]
        return s.decode('ascii') if s and all(32 <= ch < 127 for ch in s) else None

    def data_sections(self) -> list[Section]:
        return [s for s in self.sections if not s[5] & 0x20000000 and s[0] not in ('.rsrc', '.tls')]

    def strings(self, minimum: int = 3) -> dict[str, list[int]]:
        """The printable C strings of the data sections: {text: [va, ...]}. A string starts after a 00 byte or, inside
        a longer printable run (data before it that happens to be printable), at a 4-byte aligned offset."""
        if type(minimum) is int and minimum >= 1:
            secs = self._secs()
            if secs is not None:
                return cast(dict[str, list[int]], _pe_native().pe_raw_strings(self.b, secs, minimum))
        raise _refused(f'{self.path}: strings({minimum!r}): a minimum of 1 or more, and sections within 32 bits')

    def _secs(self) -> list[tuple[int, int, int]] | None:
        secs = [(s[1], s[3], s[4]) for s in self.data_sections()]
        if all(0 <= s[1] < 1 << 32 and 0 <= s[2] < 1 << 32 and 0 <= s[0] < 1 << 63 for s in secs):
            return secs
        return None

    def find_cstring(self, text: str) -> list[int]:
        """The virtual addresses of the C string text (whole string) in the data sections."""
        if isinstance(text, str) and text.isascii():
            secs = self._secs()
            if secs is not None:
                return cast(list[int], _pe_native().pe_raw_find_cstring(self.b, secs, text.encode('ascii')))
        raise _refused(f'{self.path}: find_cstring({text!r}): an ASCII text, and sections within 32 bits')

    def find_dword(self, value: int, sections: Iterable[Section] | None = None) -> list[int]:
        """File offsets in the data sections where the little-endian 32-bit value is stored, 4-byte aligned."""
        if type(value) is int and 0 <= value < 1 << 32:
            rows = [(s[3], s[4]) for s in (sections or self.data_sections())]
            if all(0 <= a < 1 << 32 and 0 <= n < 1 << 32 for a, n in rows):
                return cast(list[int], _pe_native().pe_raw_find_dword(self.b, rows, value))
        raise _refused(f'{self.path}: find_dword({value!r}): a value of 32 bits, and sections within 32 bits')


def find_exe(root: str | None = None, given: str | None = None) -> str:
    if given:
        if not os.path.isfile(given):
            raise FileNotFoundError(given)
        return given
    root = root or find_opfor()
    p = paths.game_exe(root)
    if not os.path.isfile(p):
        raise MenuToolError(f'no BFBC2Game.exe in {os.path.dirname(p)} - pass --exe <path>')
    return p
