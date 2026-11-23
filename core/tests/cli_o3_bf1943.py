r"""Round O3's bf1943 group: the command lines test_cli_o3.py replays for bfbc2-bf1943-import's dbxcatalogue.py,
dbxparity.py and portparity.py.

cases(): the quick tier's, on cli_o3_meshview's fake game copy (dummy bundles, the index's cache) with a catalogue cache
work\tools_out\dbx_catalogue.bin written as the Rust core writes it (round PR1b-1e: bc2_native.catalogue_cache_bytes,
made-up tables keyed for the copy; no game data); the parity tools' help and usage errors, and a run without a BF1943
source (round PR1b-1d retired their Python: bfbc2.exe answers every line, the recordings stay the Python's answers).
opfor_calls(opfor): dbxcatalogue on Project OPFOR's caches, read in place (only when both are current: a stale one
would be rebuilt into OPFOR's work\, so then the call is left out).
"""
from __future__ import annotations

import os
import sys
from collections.abc import Callable
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import cli_o3_meshview as mv  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]      # as cli_o3_meshview's
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]
# the seconds dbxcatalogue prints (its load time) and its build log's
NORMS = [(r'(executable names \()\d+( s\))', r'\1N\2'),
         (r'(catalogue: \d+/\d+ bundles, \d+ partitions, )\d+ s', r'\1N s')]
REKEY = mv.REKEY                   # the index's .bin as the meshview group compares it
CACHE = 'opfor/work/tools_out/dbx_catalogue.bin'
MAGIC, VERSION = b'BC2DBXCT', 1    # the cache file's (bc2_convert::dbxcatalogue::cache)


def _catalogue_module() -> Any:
    mv.suite_paths.use('core', 'bf1943-import')
    import dbxcatalogue
    return dbxcatalogue


def _native() -> Any:
    _catalogue_module()
    import importcore
    m = importcore.native()
    if m is None:
        raise RuntimeError('the Rust core module (bc2_native) is needed to write the catalogue cache')
    return m


def _key(ix: Any, root: str) -> str | None:
    """The catalogue cache's key for the copy at root (bc2_native.catalogue_key over dbxcatalogue._names)."""
    dc = _catalogue_module()
    return _native().catalogue_key(list(dc._names(ix).items()), os.path.join(root, 'game', 'BFBC2Game.exe'),
                                   os.path.join(root, *CACHE.split('/')[1:]))


_MADE: dict[tuple[str, bool, int, bool, bool], bytes] = {}


def catalogue_cache(stale: bool = False, version: int = VERSION, empty: bool = False) -> Callable[[str], bytes]:
    """dbx_catalogue.bin as the Rust core writes it for the copy in d (made-up tables; once per case folder)."""
    def made(d: str) -> bytes:
        key = (d, stale, version, empty, os.path.isfile(os.path.join(mv._root(d), 'game', 'BFBC2Game.exe')))
        if key not in _MADE:
            _MADE[key] = _make_catalogue(d, stale, version, empty)
        return _MADE[key]
    return made


def _make_catalogue(d: str, stale: bool, version: int, empty: bool) -> bytes:
    """The catalogue cache's bytes."""
    root = mv._root(d)

    class Ix:
        where = mv._where(root)
    k = _key(Ix, root)
    assert k is not None, 'the fake copy\'s bundles are files: keyed'
    if stale:
        k = k[:-1] + ('1' if k[-1] == '0' else '0')
    tables: dict[str, Any] = {'partitions': 0, 'classes': [], 'order': {}, 'shapes': {}, 'texts': {}, 'numeric': {},
                              'guids': {}, 'primary': {}, 'exe_names': []}
    if not empty:
        tables = {
            'partitions': 1234567,
            'classes': ['Physics.RagdollAsset', 'Render.TextureAsset', 'Audio.LfoNode'],
            'order': {'Render.TextureAsset': ['Name', 'Width']},
            'shapes': {('Render.TextureAsset', '.Name'): {'field-text': 3},
                       ('Render.TextureAsset', '.Width'): {'num4x1': 2},
                       ('Audio.LfoNode', '.Rate'): {'num4x1': 1, 'field-text': 1}},
            'texts': {('Render.TextureAsset', '.Name'): {'café': 2, 'Δ': 1}},
            'numeric': {('Render.TextureAsset', '.Width'): {'whole': 2}},
            'guids': {'0f1e': 'objects/a', '1a2b': ('objects/a', 'objects/b'), 'ffff': 'objects/c'},
            'primary': {'objects/a': '0f1e'},
            'exe_names': ['TextureAsset', 'RagdollAsset', 'EiaYaw', 'Name'],
        }
    return bytes(_native().catalogue_cache_bytes(k, tables, None if version == VERSION else version))


def game_files(cat: Any = None, index: Any = None, exe: bool = True) -> dict[str, Any]:
    files = mv.game_files(index, exe=exe)
    files[CACHE] = cat if cat is not None else (catalogue_cache(), mv.T0)
    return files


ENV = mv.ROOT_ENV


def cases() -> list[Case]:
    rows: list[Case] = []
    good = game_files()

    def add(key: str, tool: str, args: list[str], files: dict[str, Any] | None = None,
            env: dict[str, str] | None = None) -> None:
        rows.append((key, tool, good if files is None else files, args, ENV if env is None else env, False))

    add('catalogue', 'dbxcatalogue', [])
    add('catalogue, arguments ignored', 'dbxcatalogue', ['--help', 'x', '-o'])
    add('catalogue, no exe', 'dbxcatalogue', [], game_files(exe=False))
    add('catalogue, empty', 'dbxcatalogue', [], game_files((catalogue_cache(empty=True), mv.T0)))
    if not mv.ref_beside_opfor():               # else both sides find that copy (cli_o3_meshview.ref_beside_opfor)
        add('catalogue, no game copy', 'dbxcatalogue', [], env={'OPFOR_ROOT': ''})
    # a catalogue or index that is not this copy's (the catalogue built from the dummy bundles: every one a read
    # problem, so not cached), a damaged cache, another version: handed to the Python until round PR1b-1e
    add('catalogue, stale', 'dbxcatalogue', [], game_files((catalogue_cache(stale=True), mv.T0)))
    add('catalogue, damaged', 'dbxcatalogue', [], game_files((b'\x80\x05\x95broken', mv.T0)))
    add('catalogue, old version', 'dbxcatalogue', [], game_files((catalogue_cache(version=2), mv.T0)))
    add('catalogue, stale index', 'dbxcatalogue', [], game_files(index=(mv.index_cache(stale=True), mv.T0)))
    # round PR1b-1e: the retired pickle left on disk (and no .bin): read by nothing, the catalogue built again
    old = mv.game_files(None)
    old['opfor/work/tools_out/dbx_catalogue.pkl'] = (b'PK an old pickle', mv.T0)
    add('catalogue, only the retired pickle', 'dbxcatalogue', [], old)
    # every parity line with BF1943's source pointed at nothing: a line that parses runs the tool (never a real
    # parity run from the tests: it would read Project Rising Sun's workspace for minutes)
    nosrc = dict(ENV, BF1943_XENON='{d}\\nosource')
    for tool in ('dbxparity', 'portparity'):
        add(f'{tool} -h', tool, ['-h'], env=nosrc)
        add(f'{tool} --help', tool, ['--help'], env=nosrc)
        add(f'{tool} names -h', tool, ['zero', 'corsair', '-h'], env=nosrc)
        add(f'{tool} -o missing', tool, ['-o'], env=nosrc)
        add(f'{tool} -o then option', tool, ['-o', '--help'], env=nosrc)
        add(f'{tool} -o after names, missing', tool, ['zero', '-o'], env=nosrc)
        add(f'{tool} unknown option', tool, ['--bogus'], env=nosrc)
        add(f'{tool} unknown options with names', tool, ['--bogus', 'zero', '--other=1'], env=nosrc)
        add(f'{tool} -o then -h', tool, ['-o', 'r.txt', '-h'], env=nosrc)
        add(f'{tool} --predict', tool, ['--predict', '--nope'], env=nosrc)
        # recorded as the Python answered them, answered in Rust since round PR1b-1d: a run (here without BF1943's
        # source), argparse's own corners
        add(f'{tool} run without a source', tool, ['zero', '-o', '{d}\\r.txt'], env=nosrc)
        add(f'{tool} abbreviation', tool, ['--hel'], env=nosrc)
        add(f'{tool} attached -o', tool, ['-o{d}\\r.txt', '-h'], env=nosrc)
        add(f'{tool} names around an option', tool, ['a', '-o', '{d}\\r.txt', 'b'], env=nosrc)
        add(f'{tool} double dash', tool, ['--', '-h'], env=nosrc)
    return mv.with_variants(rows, ['catalogue', 'catalogue, no game copy', 'dbxparity -h', 'portparity -o missing'])


def current(opfor: str) -> bool:
    """Whether OPFOR's index and catalogue caches are its game copy's (dbxcatalogue then reads them, writes nothing)."""
    if not mv.index_current(opfor):
        return False
    try:
        ix = mv._meshview().Index(opfor)                     # its cache is current: read, nothing written
        with open(os.path.join(opfor, 'work', 'tools_out', 'dbx_catalogue.bin'), 'rb') as f:
            head = f.read(len(MAGIC) + 4 + 64)
        k = _key(ix, opfor)
        return k is not None and head == MAGIC + VERSION.to_bytes(4, 'little') + k.encode()
    except Exception:                                        # noqa: BLE001 - missing or damaged: not current
        return False


def opfor_calls(opfor: str) -> list[Call]:
    out: list[Call] = []
    if current(opfor):
        out.append(('dbxcatalogue (OPFOR\'s caches)', 'dbxcatalogue', [], {}, {'OPFOR_ROOT': opfor}, True))
    else:
        print('     (OPFOR\'s index / catalogue caches are not current: dbxcatalogue left out, nothing rebuilt)')
    for tool in ('dbxparity', 'portparity'):
        out.append((f'{tool} -h', tool, ['-h'], {}, {'OPFOR_ROOT': opfor}, True))
    return out
