r"""FbrbTool's commands in process (bfbc2core.fbrbtool, rust\bc2-formats\src\fbrb\tool.rs) against FbrbTool.exe: the
same files written, byte for byte, the same messages and exit codes.

  python core\tests\test_fbrbtool.py          quick: built bundles (zipped, stored, DICE's padded layouts), every
                                              command's usage, path, manifest and damaged-file errors, BIG archives;
                                              100 randomly damaged bundles through list and unpack; .NET's gzip
                                              reader against the port on 500 streams (a C# probe, when the C#
                                              compiler is there); the drop-in runner, the batch, BFBC2_FBRBTOOL_EXE,
                                              the log
  python core\tests\test_fbrbtool.py --game   + 600 damaged bundles and 3,000 gzip streams; a sample of Project OPFOR
                                              BFBC2's game bundles (unpack, list, pack of the unpacked folder) and a
                                              replay of the logged FbrbTool invocations whose inputs still exist
                                              (fbrbtool-<date>.log, today and yesterday)
     --sample N    game bundles (default 40; the smallest first of a seeded pick, under --max-mb MB, default 120)
     --replay N    logged invocations replayed (default 200)

Both ways need FbrbTool.exe (bfbc2-fbrb-tool\build.bat); without it the in-process half alone is checked. Skipped
(exit 0) when the Rust core is not built (python rust\build_native.py).
"""
import datetime
import gzip
import hashlib
import os
import random
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import zlib
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import fbrbtool, native  # noqa: E402

NATIVE: Any = native.load()
NATIVE = NATIVE if NATIVE is not None and hasattr(NATIVE, 'fbrbtool_run') else None
EXE = fbrbtool.EXE
HAVE_EXE = os.path.isfile(EXE)
CSC = r'C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe'
DELETED = '*deleted*'


# ------------------------------------------------------------------------------------------- built bundles

def bundle(entries: list[tuple], zipped: bool = True, padding: str | None = None, hdr: int = 1,
           plen_size2: bool = False, gz: Any = None, tail: bytes = b'') -> bytes:
    """A bundle of [(name, type, flags, data[, size2])], laid out as FbrbTool's pack (or DICE's packer) writes it.
    padding: 'align4096' / 'gap64' between live entries; hdr: the header region padded with zeros."""
    gz = gz or (lambda b: gzip.compress(b, mtime=0))
    strings, types, names = bytearray(), {}, []
    for e in entries:
        names.append(len(strings))
        strings += e[0].encode('latin-1') + b'\0'
        if e[1] not in types:
            types[e[1]] = len(strings)
            strings += e[1].encode('latin-1') + b'\0'
    payload, table, first, last_end = bytearray(), bytearray(), True, None
    for e, off in zip(entries, names):
        name, typ, flags, data = e[:4]
        live = not (typ == DELETED and not data)
        if padding and live:
            if not first and padding == 'align4096':
                payload += b'\xBF' * (-len(payload) % 4096)
            if not first and padding == 'gap64':
                payload += b'\xBF' * 64
            first = False
        size2 = e[4] if len(e) > 4 else len(data)
        table += struct.pack('>6I', off, flags, len(payload), len(data), size2, types[typ])
        if live:
            last_end = len(payload) + size2
        payload += data
    plen = last_end if plen_size2 and last_end is not None else len(payload)
    header = struct.pack('>2I', 2, len(strings)) + strings + struct.pack('>I', len(entries)) + table
    header += bytes([1 if zipped else 0]) + struct.pack('>I', plen)
    part1 = gz(bytes(header))
    if hdr > 1:
        part1 += bytes(-(8 + len(part1)) % hdr)
    return b'FbRB' + struct.pack('>I', len(part1)) + part1 + (gz(bytes(payload)) if zipped else bytes(payload)) + tail


def raw_bundle(header: bytes, payload: bytes, zipped: bool = True) -> bytes:
    """A bundle around a header given as bytes (for damaged headers)."""
    part1 = gzip.compress(header, mtime=0)
    return b'FbRB' + struct.pack('>I', len(part1)) + part1 + (gzip.compress(payload, mtime=0) if zipped else payload)


def header_of(entries: list[tuple[int, int, int, int, int, int]], strings: bytes, zipped: int = 1,
              plen: int = 0, count: int | None = None, strlen: int | None = None) -> bytes:
    h = struct.pack('>2I', 2, len(strings) if strlen is None else strlen & 0xFFFFFFFF) + strings
    h += struct.pack('>I', (len(entries) if count is None else count) & 0xFFFFFFFF)
    for e in entries:
        h += struct.pack('>6I', *[v & 0xFFFFFFFF for v in e])
    return h + bytes([zipped]) + struct.pack('>I', plen)


BASIC = [('Levels/Test/Test.dbx', '<non-resource>', 0x10000, b'{binary}' + bytes(range(200))),
         ('Shared/Gone.dbx', DELETED, 0, b''),
         ('Terrains/Test/Test.Heightfield-000.res', 'TerrainHeightfield', 0x10000, bytes(range(256)) * 20),
         ('Shared/AlsoGone.res', DELETED, 0, b''),
         ('Levels/Test/Empty.txt', 'Text', 0, b'')]
PADDED = [('a.res', 'T', 0x10000, b'A' * 100, 36), ('gone', DELETED, 0, b''), ('b.res', 'T', 0x10000, b'B' * 5000, 4936),
          ('c.res', 'U', 0x30000, b'C' * 70, 6)]


def big_archive(files: list[tuple[str, bytes]], magic: bytes = b'BIGF', count: int | None = None,
                fix: Any = None) -> bytes:
    index = sum(8 + len(n) + 1 for n, _ in files)
    data_at = 16 + index
    rows, blob = bytearray(), bytearray()
    for n, d in files:
        off, size = data_at + len(blob), len(d)
        if fix:
            off, size = fix(n, off, size)
        rows += struct.pack('>2I', off & 0xFFFFFFFF, size & 0xFFFFFFFF) + n.encode('latin-1') + b'\0'
        blob += d
    total = 16 + len(rows) + len(blob)
    return magic + struct.pack('<I', total) + struct.pack('>2I', len(files) if count is None else count & 0xFFFFFFFF,
                                                           index) + bytes(rows) + bytes(blob)


def write(path: str, data: bytes | str) -> None:
    os.makedirs(os.path.dirname(path) or '.', exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data.encode('utf-8') if isinstance(data, str) else data)


# ------------------------------------------------------------------------------------------- both ways

def norm(s: str | bytes) -> str:
    if isinstance(s, bytes):
        s = s.decode('utf-8', 'replace')
    return s.replace('\r\n', '\n')


def run_exe(args: list[str]) -> tuple[int, str, str]:
    r = subprocess.run([EXE, *args], capture_output=True)
    return r.returncode, norm(r.stdout), norm(r.stderr)


def run_native(args: list[str]) -> tuple[int, str, str]:
    code, out, err, _ = NATIVE.fbrbtool_run(args, fbrbtool.log_path())
    return code, out, err


def tmp_names(s: str) -> str:
    """Temporary files' names without their process and counter numbers (<out>.tmp-<pid>[-<n>])."""
    return re.sub(r'\.tmp-\d+(-\d+)?', '.tmp-N', s)


def snapshot(folder: str) -> dict[str, str]:
    """Every file under folder: its SHA-1 (empty folders too, as '<dir>')."""
    out = {}
    for d, ds, fs in os.walk(folder):
        if not ds and not fs:
            out[os.path.relpath(d, folder) + os.sep] = '<dir>'
        for f in fs:
            h = hashlib.sha1()
            with open(os.path.join(d, f), 'rb') as fh:
                for b in iter(lambda: fh.read(1 << 20), b''):
                    h.update(b)
            out[os.path.relpath(os.path.join(d, f), folder)] = h.hexdigest()
    return out


def both_ways(setup: Any, args_of: Any, root: str, files_of: Any = None) -> tuple[Any, Any]:
    """A command run both ways on two copies of the same folder: (exe, native), each (code, stdout, stderr, files)."""
    results = []
    for way, fn in (('exe', run_exe), ('rust', run_native)):
        d = os.path.join(root, way)
        shutil.rmtree(d, ignore_errors=True)
        os.makedirs(d)
        setup(d)
        t = time.perf_counter()
        code, out, err = fn(args_of(d))
        dt = time.perf_counter() - t
        files = snapshot(files_of(d) if files_of else d)
        results.append((code, out.replace(d, '<dir>'), err.replace(d, '<dir>'), files, dt))
    return results[0], results[1]


def same(a: Any, b: Any) -> tuple[bool, str]:
    if a[0] != b[0]:
        return False, f'exit code {a[0]} / {b[0]}: {a[2][:300]!r} / {b[2][:300]!r}'
    if tmp_names(a[1]) != tmp_names(b[1]):
        return False, f'stdout {a[1][-300:]!r} / {b[1][-300:]!r}'
    if tmp_names(a[2]) != tmp_names(b[2]):
        return False, f'stderr {a[2]!r} / {b[2]!r}'
    if a[3] != b[3]:
        diff = sorted(k for k in set(a[3]) | set(b[3]) if a[3].get(k) != b[3].get(k))
        return False, f'files differ: {diff[:4]}'
    return True, ''


def run_case(root: str, files: dict[str, Any], args: list[str]) -> tuple[Any, Any]:
    def setup(d: str) -> None:
        for name, data in files.items():
            p = os.path.join(d, name)
            if data is None:
                os.makedirs(p, exist_ok=True)
            else:
                write(p, data)
    return both_ways(setup, lambda d: [a.replace('{d}', d) for a in args], root)


# ------------------------------------------------------------------------------------------- the cases

def cases() -> list[tuple[str, dict[str, Any], list[str]]]:
    rows: list[tuple[str, dict[str, Any], list[str]]] = []

    def add(name: str, files: dict[str, Any], *args: str) -> None:
        rows.append((name, files, list(args)))

    built = {f'b{z}.fbrb': bundle(BASIC, zipped=bool(z)) for z in (0, 1)}
    for z in (0, 1):
        add(f'unpack zipped={z}', built, 'unpack', f'{{d}}\\b{z}.fbrb', '{d}\\u')
        add(f'list zipped={z}', built, 'list', f'{{d}}\\b{z}.fbrb')
    for padding in ('align4096', 'gap64'):
        for hdr in (1, 4, 4096):
            for plen in (False, True):
                b = bundle(PADDED, zipped=False, padding=padding, hdr=hdr, plen_size2=plen)
                add(f'unpack {padding} hdr={hdr} plen={plen}', {'p.fbrb': b}, 'unpack', '{d}\\p.fbrb', '{d}\\u')
    add('unpack into an existing folder over older files', {
        'b1.fbrb': built['b1.fbrb'], 'u\\Levels\\Test\\Test.dbx': b'old', 'u\\stale.txt': b'x'},
        'unpack', '{d}\\b1.fbrb', '{d}\\u')
    add('unpack: names with dots, spaces, odd bytes', {'x.fbrb': bundle([
        ('a/./b/../c.res', 'T', 1, b'1'), ('d. /e .res', 'T', 1, b'2'), ('f\xe9.res', 'T\xff', 1, b'3'),
        ('dup.res', 'T', 1, b'first'), ('DUP.res', 'T', 1, b'second'), ('x\\y.res', 'T', 1, b'4')])},
        'unpack', '{d}\\x.fbrb', '{d}\\u')
    add('unpack: two entries on one file, in another case', {'x.fbrb': bundle([
        ('A.dbx', 'T', 1, b'1'), ('d/x.res', 'T', 1, b'x'), ('a.DBX', 'T', 1, b'22'), ('D/X.RES', 'T', 1, b'yy')])},
        'unpack', '{d}\\x.fbrb', '{d}\\u')
    add('unpack over files and folders named in another case', {
        'b1.fbrb': built['b1.fbrb'], 'u\\LEVELS\\test\\TEST.DBX': b'old', 'u\\FBRB.MANIFEST': b'old'},
        'unpack', '{d}\\b1.fbrb', '{d}\\u')
    add('unpack: a file where a folder must be', {'x.fbrb': bundle([('a', 'T', 1, b'1'), ('a/b', 'T', 1, b'2')])},
        'unpack', '{d}\\x.fbrb', '{d}\\u')
    add('unpack: an entry that is a folder already', {'x.fbrb': bundle([('a', 'T', 1, b'1')]), 'u\\a': None},
        'unpack', '{d}\\x.fbrb', '{d}\\u')
    for bad in ('../x.res', '..\\..\\x.res', '/abs.res', '\\abs.res', 'C:x.res', 'C:\\x.res', 'a<b.res', 'a?b.res',
                'a*b', 'a"b', 'a|b', 'a\x01b', 'a:b.res', 'a/' + 'n' * 300, 'q/' + 'n' * 240 + '/f.res', ' ', '.',
                'a/..../b.res', 'a/. ./b.res', 'a/ /b.res', 'a/b /c.res', 'x/.. /y.res', 'x/ ../y.res', '...',
                'a/... /b.res', ' a/b.res', 'a/  x  /b.res', 'a//b.res', 'a/.', 'a/b/...', 'a/b/. .', '..x/b.res',
                'a/b ', 'z/' + 'n' * 236 + '/f', 'z/' + 'n' * 200 + '/' + 'm' * 40,
                'con', 'con.dbx', 'NUL.txt', 'com1.res', 'lpt1', 'a/aux/b.res', 'conin$', 'con .res'):
        add(f'unpack: entry name {bad[:20]!r}', {'x.fbrb': bundle([('ok.res', 'T', 1, b'1'), (bad, 'T', 1, b'2')])},
            'unpack', '{d}\\x.fbrb', '{d}\\u')
    add('unpack: an empty entry name', {'x.fbrb': bundle([('ok.res', 'T', 1, b'1'), ('', 'T', 1, b'2')])},
        'unpack', '{d}\\x.fbrb', '{d}\\u')
    # the header's values out of range
    strings = b'a.res\0T\0b.res\0'
    for label, ents, count, strlen in (
            ('offset past the payload', [(0, 1, 0, 3, 3, 6), (8, 1, 90, 3, 3, 6)], None, None),
            ('size past the payload', [(0, 1, 0, 300, 3, 6)], None, None),
            ('offset 0x80000000', [(0, 1, 0x80000000, 3, 3, 6)], None, None),
            ('size 0x80000000', [(0, 1, 0, 0x80000000, 3, 6)], None, None),
            ('name offset negative', [(-4, 1, 0, 3, 3, 6)], None, None),
            ('name offset past the header', [(5000, 1, 0, 3, 3, 6)], None, None),
            ('count past the header', [(0, 1, 0, 3, 3, 6)], 5, None),
            ('count negative', [(0, 1, 0, 3, 3, 6)], -1, None),
            ('count huge', [(0, 1, 0, 3, 3, 6)], 0x7FFFFFFF, None),
            ('strlen huge', [(0, 1, 0, 3, 3, 6)], None, 0x7FFFFFF0),
            ('strlen negative', [(0, 1, 0, 3, 3, 6)], None, -16)):
        h = header_of(ents, strings, count=count, strlen=strlen)
        for z in (1, 0):
            b = raw_bundle(h, b'xyzabcdef', zipped=bool(z))
            add(f'damaged header: {label} (zipped={z}), unpack', {'x.fbrb': b}, 'unpack', '{d}\\x.fbrb', '{d}\\u')
            add(f'damaged header: {label} (zipped={z}), list', {'x.fbrb': b}, 'list', '{d}\\x.fbrb')
    add('a header with no zipped byte', {'x.fbrb': b'FbRB' + struct.pack('>I', 30) +
                                          gzip.compress(struct.pack('>3I', 2, 0, 0), mtime=0)[:30]}, 'list', '{d}\\x.fbrb')
    add('a header without the payload length', {'x.fbrb': raw_bundle(header_of([], b'')[:-4], b'', False)},
        'list', '{d}\\x.fbrb')
    # the file itself
    add('not a bundle', {'x.fbrb': b'NOPE' + bytes(20)}, 'list', '{d}\\x.fbrb')
    add('shorter than 8 bytes', {'x.fbrb': b'FbRB'}, 'unpack', '{d}\\x.fbrb', '{d}\\u')
    add('an empty file', {'x.fbrb': b''}, 'list', '{d}\\x.fbrb')
    add('a header length past the file', {'x.fbrb': b'FbRB' + struct.pack('>I', 999) + b'x' * 20}, 'list', '{d}\\x.fbrb')
    add('a negative header length', {'x.fbrb': b'FbRB' + struct.pack('>I', 0x80000001) + b'x' * 20},
        'list', '{d}\\x.fbrb')
    add('a header of garbage', {'x.fbrb': b'FbRB' + struct.pack('>I', 20) + bytes(range(20))}, 'list', '{d}\\x.fbrb')
    add('a zero-length header', {'x.fbrb': b'FbRB' + struct.pack('>I', 0)}, 'list', '{d}\\x.fbrb')
    b1 = built['b1.fbrb']
    for cut in (12, 30, len(b1) - 2000, len(b1) - 9, len(b1) - 4, len(b1) - 1):
        add(f'a zipped bundle cut at {cut}', {'x.fbrb': b1[:cut]}, 'unpack', '{d}\\x.fbrb', '{d}\\u')
    flipped = bytearray(b1)
    flipped[-8] ^= 1
    add('a payload CRC flipped', {'x.fbrb': bytes(flipped)}, 'list', '{d}\\x.fbrb')
    add('junk after the payload', {'x.fbrb': b1 + b'junk' * 10}, 'unpack', '{d}\\x.fbrb', '{d}\\u')
    add('a stored bundle cut short', {'x.fbrb': built['b0.fbrb'][:-300]}, 'unpack', '{d}\\x.fbrb', '{d}\\u')
    add('a missing bundle', {}, 'unpack', '{d}\\missing.fbrb', '{d}\\u')
    add('a bundle in a missing folder', {}, 'list', '{d}\\no\\such\\x.fbrb')
    add('a folder as the bundle', {'f': None}, 'list', '{d}\\f')
    add('an empty bundle path', {}, 'list', '')
    add('illegal characters in the bundle path', {}, 'list', '{d}\\a<b.fbrb')
    add('a wildcard in the bundle path', {}, 'list', '{d}\\a*.fbrb')
    add('unpack: an empty output folder name', built, 'unpack', '{d}\\b1.fbrb', '')
    add('unpack: the output folder is a file', {**built, 'file': b'x'}, 'unpack', '{d}\\b1.fbrb', '{d}\\file')
    add('unpack: illegal characters in the output folder', built, 'unpack', '{d}\\b1.fbrb', '{d}\\o|ut')
    # pack
    man = '# fbrb.manifest - entry order matters. name|type|flags(hex)|size2|size  zipped=1\r\n'
    good: dict[str, Any] = {'f\\a.res': b'hello', 'f\\b\\c.res': b'x' * 100}

    def pk(name: str, manifest: str | bytes | None, extra: dict[str, Any] | None = None, out: str = '{d}\\o.fbrb') -> None:
        files = dict(good)
        if manifest is not None:
            files['f\\fbrb.manifest'] = manifest
        files.update(extra or {})
        add('pack: ' + name, files, 'pack', '{d}\\f', out)

    pk('a plain folder', man + 'a.res|T|00010000|5|5\r\nb/c.res|U|00020000|100|100\r\ngone|*deleted*|00000000|0|0\r\n')
    pk('an edited entry, size2 kept and not', man + 'a.res|T|00010000|3|4\r\nb/c.res|U|1|7|100\r\n')
    pk('an old manifest (no size column)', man + 'a.res|T|00010000|5\r\n')
    pk('an old manifest with a differing size2', man + 'a.res|T|00010000|9\r\n')
    pk('a stored payload', man.replace('zipped=1', 'zipped=0') + 'a.res|T|1|5|5\r\nb/c.res|T|1|100|100\r\n')
    for tokens in ('layout=align4096 hdr=4096 plen=size2 size2=minus64', 'layout=gap64 hdr=4', 'layout=other hdr=1',
                   'layout=gap64 hdr=0', 'layout=gap64 hdr=-4'):
        pk(f'tokens {tokens}', man.replace('zipped=1', 'zipped=0 ' + tokens) +
           'a.res|T|1|5|5\r\ngone|*deleted*|0|0|0\r\nb/c.res|T|1|36|100\r\n')
    pk('minus64 with an edited entry', man.replace('zipped=1', 'zipped=0 layout=gap64 hdr=4 size2=minus64') +
       'a.res|T|1|5|500\r\nb/c.res|T|1|36|99\r\n')
    pk('a byte order mark, LF line ends, comments and blank lines',
       '\ufeff' + man.replace('\r\n', '\n') + '\n# note\na.res|T|1|5|5\n')
    pk('UTF-16 manifest', (man + 'a.res|T|1|5|5\r\n').encode('utf-16'))
    pk('a non-ASCII name and type', man + 'ä.res|Tü|1|5|5\r\n', {'f\\ä.res': b'hello'})
    pk('a name outside the BMP', man + '\U0001F600.res|T|1|5|5\r\n', {'f\\\U0001F600.res': b'hello'})
    pk('a marker with an empty file', man + 'gone|*deleted*|0|0|0\r\n', {'f\\gone': b''})
    pk('a marker with a non-empty file', man + 'gone|*deleted*|0|0|0\r\n', {'f\\gone': b'data'})
    pk('no manifest', None)
    pk('an empty manifest', '')
    pk('only the first line', man)
    pk('one field', man + 'a.res\r\n')
    pk('two fields, a missing file', man + 'nofile|T\r\n')
    pk('three fields', man + 'a.res|T|1\r\n')
    for flags in ('', 'g', '1g', ' 1', '-1', '+1', '0x10', '0X', 'FFFFFFFF', '100000000', '000000001', '+'):
        pk(f'flags {flags!r}', man + f'a.res|T|{flags}|5|5\r\n')
    for size2 in ('', 'x', '-1', '-0', ' 5 ', '4294967296', '+5', '5\0'):
        pk(f'size2 {size2!r}', man + f'a.res|T|1|{size2}|5\r\n')
    for size in ('', 'x', '99999999999', '-0'):
        pk(f'size {size!r}', man + f'a.res|T|1|5|{size}\r\n')
    for hdr_text in ('x', '99999999999', ''):
        pk(f'hdr={hdr_text}', man.replace('zipped=1', f'zipped=0 hdr={hdr_text}') + 'a.res|T|1|5|5\r\n')
    pk('an unsafe name', man + '../x.res|T|1|5|5\r\n')
    pk('names normalised as .NET does', man + 'b/./c.res|T|1|100|100\r\nb. /c.res . |T|1|100|100\r\n'
       'x/../a.res|T|1|5|5\r\n')
    pk('a name of dots and spaces', man + 'b/. ./c.res|T|1|100|100\r\n')
    pk('an empty name', man + '|T|1|5|5\r\n')
    pk('a rooted name', man + 'C:\\x.res|T|1|5|5\r\n')
    pk('illegal characters in a name', man + 'a?.res|T|1|5|5\r\n')
    pk('a missing entry file', man + 'a.res|T|1|5|5\r\nmissing.res|T|1|5|5\r\n')
    pk('a missing entry folder', man + 'nope/missing.res|T|1|5|5\r\n')
    pk('a folder as an entry', man + 'b|T|1|5|5\r\n')
    pk('a file error before a field error', man + 'missing.res|T|zz|x\r\n')
    pk('a field error after the file', man + 'a.res|T|zz|5|5\r\n')
    pk('the bundle into a missing folder', man + 'a.res|T|1|5|5\r\n', out='{d}\\no\\such\\o.fbrb')
    pk('the bundle over an existing one', man + 'a.res|T|1|5|5\r\n', {'o.fbrb': b'old'})
    pk('the bundle over an existing one named in another case', man + 'a.res|T|1|5|5\r\n', {'O.FBRB': b'old'})
    pk('the bundle path a folder', man + 'a.res|T|1|5|5\r\n', {'odir': None}, out='{d}\\odir')
    pk('illegal characters in the bundle path', man + 'a.res|T|1|5|5\r\n', out='{d}\\o<.fbrb')
    add('pack: a missing folder', {}, 'pack', '{d}\\nothing', '{d}\\o.fbrb')
    add('pack: an empty folder path', {}, 'pack', '', '{d}\\o.fbrb')
    # big
    files = [('data/a.apt', b'APT' * 10), ('b.const', b'\x01\x02'), ('tex/c.tga', bytes(100))]
    add('big: BIGF', {'x.big': big_archive(files)}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: BIG4', {'x.big': big_archive(files, b'BIG4')}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: another magic', {'x.big': big_archive(files, b'BIG\x01')}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: a magic with a high byte', {'x.big': big_archive(files, b'BI\xffF')}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: three bytes', {'x.big': b'BIG'}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: no count', {'x.big': b'BIGF1234'}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: count past the index', {'x.big': big_archive(files, count=9)}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: count negative', {'x.big': big_archive(files, count=-3)}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: data past the end', {'x.big': big_archive(files, fix=lambda n, o, s: (o, s + 999) if n == 'b.const'
                                                        else (o, s))}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: a negative offset', {'x.big': big_archive(files, fix=lambda n, o, s: (-1, s) if n == 'b.const'
                                                        else (o, s))}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: a name leading out', {'x.big': big_archive([('../x', b'1')])}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: an empty name', {'x.big': big_archive([('', b'1')])}, 'big', '{d}\\x.big', '{d}\\out')
    add('big: a missing file', {}, 'big', '{d}\\x.big', '{d}\\out')
    # the command line
    for args in ([], ['unpack'], ['unpack', 'a'], ['unpack', 'a', 'b', 'c'], ['list'], ['list', 'a', 'b'],
                 ['pack', 'a'], ['big', 'a'], ['LIST', 'a'], ['help']):
        add(f'usage {args}', {}, *args)
    return rows


def quick(check: Any, root: str) -> None:
    rows = cases()
    bad = []
    for name, files, args in rows:
        a, b = run_case(root, files, args)
        if HAVE_EXE:
            ok, why = same(a, b)
            if not ok:
                bad.append(f'{name}: {why}')
    if HAVE_EXE:
        check(not bad, f'{len(rows)} command lines (built bundles, layouts, every error) both ways: the same files, '
              'messages and exit codes in process as FbrbTool.exe', ' | '.join(bad[:4]))
    else:
        print('     (FbrbTool.exe not built: the parity half skipped)')


def damaged(check: Any, root: str, n: int = 300, seed: int = 5) -> None:
    """Built bundles damaged at random (cut, bits flipped, bytes replaced), through list and unpack both ways."""
    if not HAVE_EXE:
        return
    rnd = random.Random(seed)
    sources = [bundle(BASIC, True), bundle(BASIC, False), bundle(PADDED, False, 'align4096', 4096, True),
               bundle([(f'e{i}.res', 'T', 0x10000, bytes(rnd.randrange(256) for _ in range(rnd.randrange(0, 3000))))
                       for i in range(12)], True)]
    bad: list[str] = []
    kinds: dict[Any, int] = {}
    for i in range(n):
        b = bytearray(rnd.choice(sources))
        k = rnd.randrange(4)
        if k == 0:
            del b[rnd.randrange(8, len(b)):]
        elif k == 1:
            for _ in range(rnd.randrange(1, 4)):
                j = rnd.randrange(len(b))
                b[j] ^= 1 << rnd.randrange(8)
        elif k == 2:
            j = rnd.randrange(8, len(b))
            b[j] = rnd.randrange(256)
        else:
            b += bytes(rnd.randrange(256) for _ in range(rnd.randrange(1, 50)))
        cmd = ['list', '{d}\\x.fbrb'] if i % 2 else ['unpack', '{d}\\x.fbrb', '{d}\\u']
        x, y = run_case(root, {'x.fbrb': bytes(b)}, cmd)
        kinds[x[0]] = kinds.get(x[0], 0) + 1
        ok, why = same(x, y)
        if not ok:
            bad.append(f'#{i} {cmd[0]}: {why}')
    check(not bad, f'{n} randomly damaged bundles through list / unpack both ways (exit codes {kinds}): identical',
          ' | '.join(bad[:3]))


# ------------------------------------------------------------------------------------------- .NET's gzip reader

GZPROBE = r'''
using System; using System.IO; using System.IO.Compression; using System.Security.Cryptography;
static class GzProbe {
    static void Main() {
        var sha = SHA1.Create(); string path;
        while ((path = Console.ReadLine()) != null) {
            byte[] d = File.ReadAllBytes(path);
            try {
                byte[] o;
                using (var input = new MemoryStream(d, 0, d.Length))
                using (var gz = new GZipStream(input, CompressionMode.Decompress))
                using (var output = new MemoryStream()) { gz.CopyTo(output); o = output.ToArray(); }
                Console.WriteLine(o.Length + " " + BitConverter.ToString(sha.ComputeHash(o)).Replace("-", "").ToLowerInvariant());
            } catch (Exception e) { Console.WriteLine("EXC " + e.GetType().Name + ": " + e.Message.Replace("\r\n", "\\n")); }
        }
    }
}
'''


def gzip_streams(rnd: random.Random, n: int) -> list[bytes]:
    """Valid and damaged gzip streams of every kind of block, header field and damage."""
    out = []
    for _ in range(n):
        size = rnd.choice([0, 1, 10, 300, 5000, 70000, 200000])
        k = rnd.randrange(4)
        data = (bytes(rnd.randrange(256) for _ in range(min(size, 5000))) * (size // 5000 + 1))[:size] if k == 0 else \
            (b'the quick brown fox ' * (size // 20 + 1))[:size] if k == 1 else \
            bytes(rnd.choice(b'ab') for _ in range(size)) if k == 2 else bytes(size)
        co = zlib.compressobj(rnd.choice([0, 1, 6, 9]), zlib.DEFLATED, -15, 9, rnd.choice([0, 1, 2, 3, 4]))
        flag = rnd.choice([0, 0, 8, 4, 16, 2, 0xE0, 0x1E])
        hdr = b'\x1f\x8b\x08' + bytes([flag]) + bytes(6)
        hdr += (b'\x03\x00abc' if flag & 4 else b'') + (b'name\0' if flag & 8 else b'') + \
            (b'comment\0' if flag & 16 else b'') + (b'\x12\x34' if flag & 2 else b'')
        g = bytearray(hdr + co.compress(data) + co.flush() + struct.pack('<2I', zlib.crc32(data), len(data) & 0xFFFFFFFF))
        m = rnd.randrange(8)
        if m == 0:
            del g[rnd.randrange(len(g) + 1):]
        elif m == 1:
            for _ in range(rnd.randrange(1, 4)):
                j = rnd.randrange(len(g))
                g[j] ^= 1 << rnd.randrange(8)
        elif m == 2:
            g[rnd.randrange(len(g))] = rnd.randrange(256)
        elif m == 3:
            g += bytes(rnd.randrange(256) for _ in range(rnd.randrange(1, 20)))
        elif m == 4:
            g = bytearray(g[:10]) + bytes(rnd.randrange(256) for _ in range(rnd.randrange(1, 400)))
        elif m == 5 and len(g) > 12:
            g[rnd.randrange(10, min(len(g), 40))] ^= 1 << rnd.randrange(8)
        elif m == 7 and len(g) > 8:
            g[len(g) - 1 - rnd.randrange(8)] ^= 1 << rnd.randrange(8)
        out.append(bytes(g))
    return out


def gzip_reader(check: Any, root: str, n: int = 1500) -> None:
    """.NET's GZipStream (a C# probe built here) against the port (bc2_native.fbrbtool_gunzip: zlib, else the managed
    inflater's port), on valid and damaged streams."""
    if not os.path.isfile(CSC) or not hasattr(NATIVE, 'fbrbtool_gunzip'):
        print('     (no C# compiler: .NET\'s gzip reader not compared)')
        return
    d = os.path.join(root, 'gz')
    os.makedirs(d, exist_ok=True)
    src, exe = os.path.join(d, 'probe.cs'), os.path.join(d, 'probe.exe')
    write(src, GZPROBE)
    subprocess.run([CSC, '-nologo', '-optimize', '-out:' + exe, src], check=True, capture_output=True)
    streams = gzip_streams(random.Random(11), n)
    paths = []
    for i, g in enumerate(streams):
        p = os.path.join(d, f'{i}.gz')
        write(p, g)
        paths.append(p)
    theirs = subprocess.run([exe], input='\n'.join(paths) + '\n', capture_output=True, text=True).stdout.splitlines()
    bad: list[str] = []
    kinds: dict[Any, int] = {}
    for p, g, t in zip(paths, streams, theirs):
        for managed in (False, True):
            ok, value = NATIVE.fbrbtool_gunzip(g, managed)
            ours = f'{len(value)} {hashlib.sha1(value).hexdigest()}' if ok else f'EXC {value}'
            if ours != t:
                bad.append(f'{os.path.basename(p)} {"managed" if managed else "zlib"}: {t} / {ours}')
        kinds[t.split(':')[0] if t.startswith('EXC') else 'read'] = kinds.get(
            t.split(':')[0] if t.startswith('EXC') else 'read', 0) + 1
    check(not bad and len(theirs) == n, f'{n} gzip streams (valid, cut, flipped, junk; {kinds}): .NET\'s GZipStream '
          'and the port give the same bytes or exception, by zlib and by the managed inflater\'s port',
          ' | '.join(bad[:3]))


# ------------------------------------------------------------------------------------------- the Python API

def api(check: Any, root: str) -> None:
    d = os.path.join(root, 'api')
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    b = os.path.join(d, 'b.fbrb')
    write(b, bundle(BASIC))
    u = os.path.join(d, 'u')
    r = fbrbtool.unpack(b, u)
    check(r.returncode == 0 and r.stdout.startswith('unpacked 5 entries') and r.stdout.endswith('\n')
          and os.path.isfile(os.path.join(u, 'fbrb.manifest')), 'fbrbtool.unpack: message (text, \\n) and files', repr(r))
    r = fbrbtool.run_cmd([EXE, 'list', b], capture_output=True, text=True)
    check(r.returncode == 0 and r.stdout.endswith('5 entries, payload 5328 bytes, zipped=True\n'),
          'fbrbtool.run_cmd: an FbrbTool.exe command line runs in process', repr(r))
    r = fbrbtool.run_cmd([EXE, 'list', b], capture_output=True)
    check(isinstance(r.stdout, bytes) and r.stdout.endswith(b'zipped=True\r\n'), 'run_cmd without text: bytes, \\r\\n')
    r = fbrbtool.run_cmd([EXE, 'list', b], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    check(r.stdout is not None and r.stdout.endswith('zipped=True\n') and r.stderr is None,
          'run_cmd with stdout=PIPE, stderr=STDOUT: as subprocess.run gives them', repr(r)[:200])
    r = fbrbtool.run_cmd([EXE, 'list', b], stdout=subprocess.DEVNULL)
    check(r.returncode == 0 and r.stdout is None, 'run_cmd with stdout=DEVNULL: nothing printed, no output kept')
    r = fbrbtool.run_cmd([EXE, 'list', os.path.join(d, 'none.fbrb')], capture_output=True, text=True)
    check(r.returncode == 1 and 'A file the tool needs is missing' in r.stderr, 'run_cmd: a failure, exit code 1')
    try:
        fbrbtool.run_cmd([EXE, 'list', os.path.join(d, 'none.fbrb')], check=True, capture_output=True)
        check(False, 'check=True raises on a non-zero exit code')
    except subprocess.CalledProcessError as e:
        check(e.returncode == 1, 'run_cmd(check=True) raises CalledProcessError with the exit code')
    r = fbrbtool.pack(u, os.path.join(d, 're.fbrb'))
    check(r.returncode == 0 and 'packed 5 entries (5328 bytes)' in r.stdout, 'fbrbtool.pack', repr(r))
    runs = [['unpack', b, os.path.join(d, f'm{i}')] for i in range(16)] + [['list', b]] * 8
    rs = fbrbtool.run_many(runs)
    check(all(x.returncode == 0 for x in rs) and all(snapshot(os.path.join(d, f'm{i}')) == snapshot(u) for i in range(16)),
          'fbrbtool.run_many: 24 command lines on all cores, the results of one by one')
    old = os.environ.get('LOCALAPPDATA')
    os.environ['LOCALAPPDATA'] = os.path.join(d, 'local')
    try:
        fbrbtool.list_entries(os.path.join(d, 'none.fbrb'))
        logs = os.listdir(os.path.join(d, 'local', 'bfbc2-tool-suite', 'logs'))
        text = open(os.path.join(d, 'local', 'bfbc2-tool-suite', 'logs', logs[0]), encoding='utf-8').read()
        check(logs[0].startswith('fbrbtool-') and 'started (in process): list' in text and 'ERROR   fbrbtool error' in text,
              'each run logged in fbrbtool-<date>.log, its errors too', text[-300:])
    finally:
        if old is None:
            os.environ.pop('LOCALAPPDATA', None)
        else:
            os.environ['LOCALAPPDATA'] = old
    if HAVE_EXE:
        os.environ[fbrbtool.EXE_ENV] = '1'
        try:
            r = fbrbtool.list_entries(b)
            check(r.returncode == 0 and 'zipped=True' in r.stdout and fbrbtool.use_exe(),
                  'BFBC2_FBRBTOOL_EXE=1 runs FbrbTool.exe')
        finally:
            del os.environ[fbrbtool.EXE_ENV]
    r = fbrbtool.run_cmd([sys.executable, '-c', 'print(5)'], capture_output=True, text=True)
    check(r.stdout.strip() == '5', 'fbrbtool.run_cmd runs other programs as subprocess.run')


# ------------------------------------------------------------------------------------------- the game (--game)

def game_bundles(n: int, max_mb: float) -> list[str]:
    suite_paths.use('navkit')
    import navkit
    game = os.path.join(navkit.find_opfor(None), 'game')
    found = sorted(os.path.join(dp, f) for top in ('Dist', 'Package') for dp, _, fs in os.walk(os.path.join(game, top))
                   for f in fs if f.lower().endswith('.fbrb'))
    small = [p for p in found if os.path.getsize(p) <= max_mb * 1e6]
    rnd = random.Random(3)
    return sorted(rnd.sample(small, min(n, len(small))), key=os.path.getsize)


def game(check: Any, root: str, n: int, max_mb: float, timings: dict[str, list[tuple[float, float, int]]]) -> None:
    bad, entries, done = [], 0, 0
    for p in game_bundles(n, max_mb):
        for args_of, files_of, cmd in (
                (lambda d, p=p: ['unpack', p, os.path.join(d, 'u')], None, 'unpack'),
                (lambda d, p=p: ['list', p], None, 'list')):
            a, b = both_ways(lambda d: None, args_of, root, files_of)
            ok, why = same(a, b)
            timings.setdefault(cmd, []).append((a[4], b[4], os.path.getsize(p)))
            if not ok:
                bad.append(f'{os.path.basename(p)} {cmd}: {why}')
            if cmd == 'list':
                m = re.search(r'(\d+) entries, payload', a[1])
                entries += int(m.group(1)) if a[0] == 0 and m else 0
        # pack the exe's unpacked folder both ways
        src = os.path.join(root, 'src')
        shutil.rmtree(src, ignore_errors=True)
        r = subprocess.run([EXE, 'unpack', p, src], capture_output=True)
        if r.returncode == 0:
            a, b = both_ways(lambda d: None, lambda d, src=src: ['pack', src, os.path.join(d, 'o.fbrb')], root)
            ok, why = same(a, b)
            timings.setdefault('pack', []).append((a[4], b[4], os.path.getsize(p)))
            if not ok:
                bad.append(f'{os.path.basename(p)} pack: {why}')
        shutil.rmtree(src, ignore_errors=True)
        done += 1
    check(not bad, f'{done} game bundles ({entries:,} entries) unpacked, listed and packed again both ways: identical '
          'files, messages and exit codes', ' | '.join(bad[:3]))


def replay(check: Any, root: str, n: int, max_mb: float) -> None:
    """The logged FbrbTool invocations (today's and yesterday's log) whose inputs still exist, both ways, with every
    output redirected to a temporary folder (inputs only read)."""
    logs = os.path.join(os.environ.get('LOCALAPPDATA', ''), 'bfbc2-tool-suite', 'logs')
    today = datetime.date.today()
    seen, runs, total = set(), [], 0
    for day in (today - datetime.timedelta(days=1), today):
        p = os.path.join(logs, f'fbrbtool-{day.isoformat()}.log')
        if not os.path.isfile(p):
            continue
        for line in open(p, encoding='utf-8', errors='replace'):
            m = re.search(r'--- fbrbtool started: (.*) ---$', line.rstrip())
            if not m:
                continue
            total += 1
            args = m.group(1).split(' ')
            if len(args) < 2 or tuple(args) in seen:
                continue
            seen.add(tuple(args))
            readable = args[0] in ('unpack', 'list') and os.path.isfile(args[1])                 and os.path.getsize(args[1]) <= max_mb * 1e6
            packable = args[0] == 'pack' and len(args) == 3 and os.path.isfile(os.path.join(args[1], 'fbrb.manifest'))
            if readable or packable:
                runs.append(args)
    rnd = random.Random(7)
    pick = rnd.sample(runs, min(n, len(runs)))
    bad = []
    for args in pick:
        if args[0] == 'unpack':
            def args_of(d: str, a: list[str] = args) -> list[str]:
                return ['unpack', a[1], os.path.join(d, 'u')]
        elif args[0] == 'pack':
            def args_of(d: str, a: list[str] = args) -> list[str]:
                return ['pack', a[1], os.path.join(d, 'o.fbrb')]
        else:
            def args_of(d: str, a: list[str] = args) -> list[str]:
                return list(a)
        a, b = both_ways(lambda d: None, args_of, root)
        ok, why = same(a, b)
        if not ok:
            bad.append(f'{" ".join(args)[:120]}: {why}')
    kinds = {k: sum(1 for a in pick if a[0] == k) for k in ('unpack', 'list', 'pack')}
    check(not bad, f'replay: {total:,} logged invocations, {len(seen):,} distinct, {len(runs)} with their inputs still '
          f'there; {len(pick)} replayed both ways ({kinds}): identical', ' | '.join(bad[:3]))


def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    def opt(name: str, default: float) -> float:
        return float(argv[argv.index(name) + 1]) if name in argv else default

    root = tempfile.mkdtemp(prefix='fbrbtool_test_')
    try:
        quick(check, root)
        game_tier = '--game' in argv
        damaged(check, root, n=600 if game_tier else 100)
        gzip_reader(check, root, n=3000 if game_tier else 500)
        api(check, root)
        if '--game' in argv and HAVE_EXE:
            timings: dict[str, list[tuple[float, float, int]]] = {}
            game(check, root, int(opt('--sample', 40)), opt('--max-mb', 120), timings)
            for cmd, rows in timings.items():
                e, r = sum(x[0] for x in rows), sum(x[1] for x in rows)
                print(f'     {cmd}: {len(rows)} bundles, FbrbTool.exe {e:.1f} s, in process {r:.1f} s ({e / r:.1f}x)')
            replay(check, root, int(opt('--replay', 200)), opt('--max-mb', 120))
    finally:
        shutil.rmtree(root, ignore_errors=True)
    print(f'\n{"ALL PASSED" if not failed else f"{failed} FAILED"}')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
