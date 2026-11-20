r"""Round O5's bf1943 group: the command lines test_cli_o5.py replays for the BF1943 importer's command line
(bfbc2-bf1943-import\bf1943import.py: list / import / check, the help and the options).

cases(): the quick tier's, on BF1943-layout bundles built here (stored and gzip payloads, bundles the reader refuses,
names outside ASCII; no game data), read through the BF1943 tool suite's own bundle reader (fbrb1943.py, copied into
the case folder from bf1943-tool-suite when it is on this PC; without it these cases are left out): the source index
off (BF1943_SOURCE_INDEX=0), written on first use (the Python's pickle), and read when it is current.
opfor_calls(opfor): the --game tier's: the real BF1943 source (Project Rising Sun's extracted dist\xenon) listed with
several filters, its index off (OPFOR imports bf1943import; it runs no command line of it).
"""
from __future__ import annotations

import gzip
import hashlib
import io
import os
import pickle
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import cli_o2_menu as menu  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

READER = r'C:\Programming\Projects\bf1943-tool-suite\bf1943-fbrb-tool\fbrb1943.py'
XENON = r'C:\Programming\Projects\operation-rising-sun\work\extracted\dist\xenon'
T0 = 1_700_000_000                       # the bundles' modification time (whole seconds: mtime_ns exact)


def gz(data: bytes) -> bytes:
    buf = io.BytesIO()
    with gzip.GzipFile(fileobj=buf, mode='wb', mtime=0) as f:
        f.write(data)
    return buf.getvalue()


def bundle(entries: list[tuple[str, str, bytes]], compressed: bool = True, version: int = 2, size_fix: int = 0,
           raw_names: bool = False) -> bytes:
    """A BF1943 FbRB bundle (fbrb1943's layout): entries (name, type, data)."""
    strings = bytearray()
    offs: dict[str, int] = {}
    for n, t, _ in entries:
        for s in (n, t):
            if s not in offs:
                offs[s] = len(strings)
                strings += (s.encode('latin-1') if raw_names else s.encode('ascii')) + b'\0'
    payload = b''.join(d for _, _, d in entries)
    d = struct.pack('>II', version, len(strings)) + bytes(strings) + struct.pack('>I', len(entries))
    pos = 0
    for n, t, data in entries:
        d += struct.pack('>6I', offs[n], 0, pos, len(data), len(data), offs[t])
        pos += len(data)
    d += bytes([1 if compressed else 0]) + struct.pack('>I', len(payload) + size_fix)
    gdir = gz(d)
    return b'FbRB' + struct.pack('>I', len(gdir)) + gdir + (gz(payload) if compressed else payload)


A = [('levels/wake/a.res', 'XenonTexture', b'A' * 300), ('animations/x/3p/standdeploy.res', 'GrannyAnimation', b'B' * 70),
     ('Characters/Skeletons/ske01.res', 'GrannyModel', b'C' * 9)]
B = [('levels/wake/b.res', 'MeshData', b'D' * 40), ('levels/wake/a.res', 'XenonTexture', b'E' * 5),
     ('sounds/s.res', 'SoundData', b'F' * 3), ('physics/p.res', 'HavokPhysicsData', b'G')]
C = [('menu/m.res', '<non-resource>', b'H' * 33), ('aim/aim.res', 'AimAnimation', b'I' * 17)]


def source_files() -> dict[str, Any]:
    """{case file: (bytes, time)} of a source folder src\\ (three good bundles, one stored; four refused)."""
    good = {'src\\levels\\wake\\level-00.fbrb': bundle(A), 'src\\levels\\wake\\zz-00.fbrb': bundle(B, compressed=False),
            'src\\menu\\menu-00.fbrb': bundle(C)}
    bad = {'src\\a_cut-00.fbrb': bundle(A)[:30], 'src\\b_version-00.fbrb': bundle(A, version=3),
           'src\\c_size-00.fbrb': bundle(A, size_fix=1), 'src\\d_magic-00.fbrb': b'RBbF' + bundle(A)[4:],
           'src\\e_names-00.fbrb': bundle([('caf\xe9.res', 'X', b'1')], raw_names=True),
           'src\\notes.txt': b'not a bundle'}
    return {k: (v, T0) for k, v in {**good, **bad}.items()}


def index_pickle(d: str, reader: bytes, changed: str | None = None) -> bytes:
    """The source index Source would save for src\\ (bf1943import._save_index's content), each bundle's row as
    _check gives it; changed: a bundle whose row names another modification time (stale)."""
    folder = os.path.normcase(os.path.abspath(os.path.join(d, 'src')))
    rows: dict[str, Any] = {}
    for name, (raw, t) in source_files().items():
        if not name.endswith('.fbrb'):
            continue
        rel = name[4:].replace('\\', '/')
        try:
            ok = check(raw)
        except ValueError:
            ok = None
        mt = (t + (1 if rel == changed else 0)) * 10 ** 9
        rows[rel] = (len(raw), mt, None, False, 0, 0) if ok is None else (len(raw), mt, *ok)
    return pickle.dumps({'version': 1, 'reader': hashlib.sha1(reader).hexdigest(), 'folder': folder,
                         'bundles': rows}, protocol=pickle.HIGHEST_PROTOCOL)


def check(raw: bytes) -> tuple[list[dict[str, Any]], bool, int, int]:
    """fbrb1943.Bundle's verdict (entries, compressed, payload start, payload length), ValueError when refused."""
    sys.path.insert(0, os.path.dirname(READER))
    try:
        import fbrb1943
    finally:
        sys.path.pop(0)
    bd = fbrb1943.Bundle(raw)
    return bd.entries, bool(bd.compressed), 8 + struct.unpack_from('>I', raw, 4)[0], len(bd.payload)


def cases() -> list[Case]:
    if not os.path.isfile(READER):
        return []
    with open(READER, 'rb') as f:
        reader = f.read()
    src = source_files()
    fb = {'fb\\fbrb1943.py': reader}
    base = {**src, **fb}
    off = {'BF1943_SUITE_FBRB': '{d}\\fb', 'BF1943_SOURCE_INDEX': '0'}
    ix = {'BF1943_SUITE_FBRB': '{d}\\fb'}
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('--help', {}, ['--help'], {}),
        ('/? among others', {}, ['list', '/?'], {}),
        ('no arguments', {}, [], {}),
        ('an unknown command', {}, ['show'], {}),
        ('an option without its value', {}, ['list', '--source'], {}),
        ('no source folder', fb, ['list', '--source', '{d}\\nowhere'], off),
        ('the default source (BF1943_XENON)', base, ['list', 'wake'], dict(off, BF1943_XENON='{d}\\src')),
        ('no bundle reader', src, ['list', '--source', '{d}\\src'], {'BF1943_SOURCE_INDEX': '0'}),
        ('list everything', base, ['list', '--source', '{d}\\src'], off),
        ('list a filter', base, ['list', 'wake', '--source', '{d}\\src'], off),
        ('list two filters, any case, backslashes', base, ['list', 'LEVELS\\WAKE', 'A.res', '--source', '{d}\\src'],
         off),
        ('list a name with capitals', base, ['list', 'skeletons', '--source', '{d}\\src'], off),
        ('list nothing found', base, ['list', 'zzz', '--source', '{d}\\src'], off),
        ('a folder of refused bundles only', {**{k: v for k, v in src.items() if '_' in k or k.endswith('.txt')},
                                              **fb}, ['list', '--source', '{d}\\src'], off),
        ('an option given twice', base, ['list', '--source', '{d}\\nowhere', '--source', '{d}\\src', 'menu'], off),
        ('import without a filter', base, ['import', '--source', '{d}\\src', '--out', '{d}\\out'], off),
        ('check', base, ['check', '--source', '{d}\\src'], off),
        ('list, the index written on first use', base, ['list', 'wake', '--source', '{d}\\src'], ix),
    ]
    out: list[Case] = [(f'bf1943import: {n}', 'bf1943import', f, a, e, False) for n, f, a, e in rows]
    out = menu.with_variants(out, ['bf1943import: list everything', 'bf1943import: no source folder'])
    # the index read when current (a stale row: checked again, so written again - the Python's): in the case folder
    # (BF1943_SOURCE_INDEX={d}\ix), its file named by the source folder's hash (_IndexName) and made from it (_Index)
    for label, changed in (('current', None), ('stale', 'menu/menu-00.fbrb')):
        files: dict[Any, Any] = dict(base)
        files[_IndexName()] = _Index(reader, changed)
        out.append((f'bf1943import: list, the index {label}', 'bf1943import', files,
                    ['list', '--source', '{d}\\src'], dict(ix, BF1943_SOURCE_INDEX='{d}\\ix'), False))
    return out


class _Index:
    """The index file's contents for a case folder (test_cli_o2.setup calls it with the folder)."""

    def __init__(self, reader: bytes, changed: str | None) -> None:
        self.reader, self.changed = reader, changed

    def __call__(self, d: str) -> bytes:
        return index_pickle(d, self.reader, self.changed)


class _IndexName(str):
    """The index file's name, {d}\\ix\\<the hash of {d}\\src>.pkl: test_cli_o2.setup makes a case file's path with
    name.replace('{d}', <the case folder>), which this answers with the whole path."""

    def __new__(cls) -> _IndexName:
        return super().__new__(cls, '{d}\\ix\\<index>.pkl')

    def replace(self, old: str, new: str, count: int = -1) -> str:  # type: ignore[override]
        if old != '{d}':
            return str.replace(self, old, new, count)
        key = hashlib.sha1(os.path.normcase(os.path.abspath(os.path.join(new, 'src'))).encode('utf-8')).hexdigest()
        return os.path.join(new, 'ix', f'{key[:20]}.pkl')


def opfor_calls(opfor: str) -> list[Call]:
    """The real BF1943 source listed (read in place; the index off: nothing written outside the case folder)."""
    if not (os.path.isdir(XENON) and os.path.isfile(READER)):
        return []
    env = {'BF1943_SOURCE_INDEX': '0', 'BF1943_SUITE_FBRB': os.path.dirname(READER)}
    out: list[Call] = []
    for args in (['garand'], ['ske01'], ['levels/wake_island_s', 'shaders'], ['m1garand', '3p'], ['XenonTexture'],
                 ['beach/k98'], ['type97'], ['zzzz']):
        out.append((f'bf1943import list {" ".join(args)}', 'bf1943import', ['list', *args, '--source', XENON], {},
                    env, True))
    out.append(('bf1943import list (every resource)', 'bf1943import', ['list', '--source', XENON], {}, env, True))
    return out
