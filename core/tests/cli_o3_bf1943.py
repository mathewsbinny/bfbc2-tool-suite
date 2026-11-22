r"""Round O3's bf1943 group: the command lines test_cli_o3.py replays for bfbc2-bf1943-import's dbxcatalogue.py,
dbxparity.py and portparity.py.

cases(): the quick tier's, on cli_o3_meshview's fake game copy (dummy bundles, the index's cache) with a catalogue cache
work\tools_out\dbx_catalogue.pkl written as dbxcatalogue.load writes it (a Catalogue of made-up tables; no game data);
the parity tools' help and usage errors, and a run handed to the Python (no BF1943 source there).
opfor_calls(opfor): dbxcatalogue on Project OPFOR's caches, read in place (only when both are current: a stale one
would be rebuilt into OPFOR's work\, so then the call is left out).
"""
from __future__ import annotations

import collections
import os
import pickle
import subprocess
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
# the catalogue cache holds a SHA-1 over paths in the case folder: compared by size (its bytes are the inputs')
LOOSE = [r'^opfor/work/tools_out/dbx_catalogue\.pkl$']
REKEY = mv.REKEY                   # the index's .bin as the meshview group compares it


def _catalogue_module() -> Any:
    mv.suite_paths.use('core', 'bf1943-import')
    import dbxcatalogue
    return dbxcatalogue


_MADE: dict[tuple[str, bool, int, bool, bool], bytes] = {}


def catalogue_cache(stale: bool = False, version: int = 3, empty: bool = False) -> Callable[[str], bytes]:
    """dbx_catalogue.pkl as dbxcatalogue._build_and_cache writes it for the copy in d: made in a Python with a fixed
    string hash (PYTHONHASHSEED=0), so its sets pickle in the same order every run (once per case folder)."""
    def made(d: str) -> bytes:
        key = (d, stale, version, empty, os.path.isfile(os.path.join(mv._root(d), 'game', 'BFBC2Game.exe')))
        if key not in _MADE:
            env = dict(os.environ, PYTHONHASHSEED='0')
            _MADE[key] = subprocess.run([sys.executable, os.path.abspath(__file__), '--make-catalogue', d,
                                         str(int(stale)), str(version), str(int(empty))],
                                        env=env, capture_output=True, check=True).stdout
        return _MADE[key]
    return made


def _make_catalogue(d: str, stale: bool, version: int, empty: bool) -> bytes:
    """The catalogue cache's bytes (catalogue_cache, in its own process)."""
    dc = _catalogue_module()
    from bfbc2core import dbx
    root = mv._root(d)

    class Ix:
        where = mv._where(root)
    sig = dc._signature(Ix, os.path.join(root, 'game', 'BFBC2Game.exe'))
    cat = dc.Catalogue()
    if not empty:
        cat.classes = {'Physics.RagdollAsset', 'Render.TextureAsset', 'Audio.LfoNode'}
        cat.order = {'Render.TextureAsset': ['Name', 'Width']}
        cat.shapes = {('Render.TextureAsset', '.Name'): collections.Counter({'field-text': 3}),
                      ('Render.TextureAsset', '.Width'): collections.Counter({'num4x1': 2}),
                      ('Audio.LfoNode', '.Rate'): collections.Counter({'num4x1': 1, 'field-text': 1})}
        cat.texts = {('Render.TextureAsset', '.Name'): collections.Counter({'café': 2, 'Δ': 1})}
        cat.numeric = {('Render.TextureAsset', '.Width'): collections.Counter({'whole': 2})}
        el = dbx.Element('field', 2, [('name', 'Name')], 'tex', b'', None)
        cat.protos = {('Render.TextureAsset', '.Items[]'): el}
        cat.guids = {'0f1e': 'objects/a', '1a2b': ('objects/a', 'objects/b'), 'ffff': 'objects/c'}
        cat.primary = {'objects/a': '0f1e'}
        cat.defaults = {'Render.TextureAsset': ('objects/a', el)}
        cat.exe_names = frozenset({'TextureAsset', 'RagdollAsset', 'EiaYaw', 'Name'})
        cat.partitions = 1234567
    return pickle.dumps({'version': version, 'sig': sig + ('0' if stale else ''), 'catalogue': cat},
                        protocol=pickle.HIGHEST_PROTOCOL)


def game_files(cat: Any = None, index: Any = None, exe: bool = True) -> dict[str, Any]:
    files = mv.game_files(index, exe=exe)
    files['opfor/work/tools_out/dbx_catalogue.pkl'] = cat if cat is not None else (catalogue_cache(), mv.T0)
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
    # handed to the Python: a catalogue or index that is not this copy's (the catalogue built from the dummy
    # bundles: every one a read problem, so not cached), a damaged cache, another version
    add('catalogue, stale', 'dbxcatalogue', [], game_files((catalogue_cache(stale=True), mv.T0)))
    add('catalogue, damaged', 'dbxcatalogue', [], game_files((b'\x80\x05\x95broken', mv.T0)))
    add('catalogue, old version', 'dbxcatalogue', [], game_files((catalogue_cache(version=2), mv.T0)))
    add('catalogue, stale index', 'dbxcatalogue', [], game_files(index=(mv.index_cache(stale=True), mv.T0)))
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
        # handed to the Python: a run (here without BF1943's source), argparse's own corners
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
    dc = _catalogue_module()
    try:
        ix = mv._meshview().Index(opfor)                     # its cache is current: read, nothing written
        sig = dc._signature(ix, os.path.join(opfor, 'game', 'BFBC2Game.exe'))
        with open(os.path.join(opfor, 'work', 'tools_out', 'dbx_catalogue.pkl'), 'rb') as f:
            c = pickle.load(f)
        return bool(c['version'] == 3 and c['sig'] == sig)
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


if __name__ == '__main__' and sys.argv[1:2] == ['--make-catalogue']:
    sys.stdout.buffer.write(_make_catalogue(sys.argv[2], sys.argv[3] == '1', int(sys.argv[4]), sys.argv[5] == '1'))
