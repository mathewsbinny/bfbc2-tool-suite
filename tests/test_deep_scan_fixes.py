r"""Regression tests for the suite deep scan of 2026-11-15 (no game data):

- C1  stock-MP install --apply is one transaction: a failure part way puts every file back;
- C2 / C6 / C9  whole-file writes (bfbc2core.atomicio; FbrbTool pack and DbxEdit import over an existing output);
- C3  the mesh viewer's index is not cached when a bundle could not be read;
- C4  the terrain unpack is redone when its bundle changes or its stamp is missing;
- C6  NavKit: a damaged navkit.json is said; a damaged detailed-sector cache is a miss;
- C7  the detailed-sector cache key follows the level's unpack stamps;
- C8  shaderdb.level_meshes reports a damaged partition instead of leaving its mesh out;
- C10 dbxport Templates say which template partitions would not parse;
- S3  DbxEdit refuses an element with more than 15 attributes;
- S4  dbx.parse / to_xml raise ValueError for a damaged file, never IndexError / struct.error.

  python tests\test_deep_scan_fixes.py
"""
import contextlib
import io
import json
import os
import shutil
import sys
import tempfile
import unittest.mock
from typing import Any

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit', 'navkit-tools', 'stockmp', 'shader-tools', 'bf1943-import', 'mesh-viewer')

from bfbc2core import atomicio, dbx  # noqa: E402


class Checks:
    def __init__(self):
        self.failed = 0

    def check(self, ok, what, detail=''):
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))


def atomic_writes(c, work):
    p = os.path.join(work, 'a.json')
    atomicio.write_json(p, {'a': 1})
    with contextlib.suppress(TypeError):
        atomicio.write_json(p, {'bad': object()})          # fails while serialising: the file keeps its content
    c.check(json.load(open(p)) == {'a': 1} and os.listdir(work) == ['a.json'],
            'atomicio: a failed write leaves the old file and no temporary file', str(os.listdir(work)))
    atomicio.copy_file(p, p + '.copy')
    c.check(open(p + '.copy').read() == open(p).read(), 'atomicio: copy_file copies')
    open(p, 'w').write('{broken')
    c.check(atomicio.read_json(p, 'miss') == 'miss', 'atomicio: an unreadable JSON is the default (a cache miss)')
    # a moment's lock on the target (the virus scanner reading a file just written) is waited out; a lasting one and
    # any other error are raised (2026-11-17: a WW2 rebuild stopped on one)
    calls: list[int] = []
    plan: dict[str, Any] = {}

    def flaky(a: Any, b: Any) -> None:
        calls.append(1)
        if len(calls) <= plan['fails']:
            raise plan['err']
    cases: tuple[tuple[int, OSError, bool, int], ...] = (
        (2, PermissionError(13, 'locked'), True, 3), (99, PermissionError(13, 'locked'), False, atomicio.TRIES),
        (99, FileNotFoundError(2, 'gone'), False, 1))
    with unittest.mock.patch.object(atomicio.os, 'replace', flaky):
        for fails, err, want_ok, want_calls in cases:
            calls.clear()
            plan.update(fails=fails, err=err)
            try:
                atomicio.replace('x', 'y')
                ok = True
            except OSError:
                ok = False
            c.check(ok == want_ok and len(calls) == want_calls,
                    f'atomicio.replace: {fails} failure(s) of {type(err).__name__} -> '
                    f'{"replaced" if want_ok else "raised"} after {want_calls} tries', f'{ok}, {len(calls)} tries')


def stockmp_rollback(c, work):
    import pipeline
    import stockmplib as stockmp
    sys.path.insert(0, os.path.join(SUITE, 'tests'))
    from bfbc2core import gameledger
    from test_review_fixes import write_build_report
    game = os.path.join(work, 'game')
    paths = pipeline.Paths('mp_test', opfor=work, work=os.path.join(work, 'stockmp'), game=game)
    out = os.path.join(paths.root, 'out', 'game')
    for rel in ('Dist/level.fbrb', 'Package/level.fbrb'):
        os.makedirs(os.path.dirname(os.path.join(out, rel)), exist_ok=True)
        os.makedirs(os.path.dirname(os.path.join(game, rel)), exist_ok=True)
        open(os.path.join(out, rel), 'wb').write(b'new ' + rel.encode())
        open(os.path.join(game, rel), 'wb').write(b'original ' + rel.encode())
    write_build_report(pipeline, paths)
    # the Package file's move over its target fails (a lasting lock: a handle open on it, as the game holds its
    # bundles; the install ledger is the suite's Rust since round OR15, so the lock is a real one, not a patched
    # os.replace): Dist is already in by then and the ledger's journal puts it back
    err = io.StringIO()
    held = open(os.path.join(game, 'Package', 'level.fbrb'), 'rb')  # noqa: SIM115 (held for the install)
    try:
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            code = stockmp.install(paths, ['stockmp.py', 'install', 'mp_test', '--apply'])
    finally:
        held.close()
    calls = [err.getvalue()] if 'level.fbrb' in err.getvalue() else []
    left = sorted(os.path.relpath(os.path.join(d, f), game).replace(os.sep, '/') for d, ds, fs in os.walk(game)
                  for f in fs if 'opfor' not in os.path.relpath(d, game).split(os.sep))
    c.check(code == 1 and calls and 'nothing changed' in err.getvalue(), 'stock-MP install: the failure is reported, '
            'exit 1', f'{code} {calls[:1]} {err.getvalue()[-200:]}')
    c.check(all(open(os.path.join(game, r), 'rb').read() == b'original ' + r.encode()
                for r in ('Dist/level.fbrb', 'Package/level.fbrb')),
            'stock-MP install: a failure part way puts every file back (Dist and Package from one build)')
    c.check(left == ['Dist/level.fbrb', 'Package/level.fbrb'] and not gameledger.Ledger(game).records(),
            'stock-MP install: nothing left beside the files after the rollback (no temporary, no record)', str(left))
    with contextlib.redirect_stdout(io.StringIO()):
        stockmp.install(paths, ['stockmp.py', 'install', 'mp_test', '--apply'])
    c.check(all(open(os.path.join(game, r), 'rb').read() == b'new ' + r.encode()
                for r in ('Dist/level.fbrb', 'Package/level.fbrb')),
            'stock-MP install: the next --apply installs both')


def meshview_skip(c, work):
    import meshview
    root = os.path.join(work, 'opfor')
    os.makedirs(root)
    bundle = os.path.join(work, 'x.fbrb')
    open(bundle, 'wb').write(b'not a bundle')
    cache = os.path.join(work, 'index.pkl')
    saved = meshview._bundles
    meshview._bundles = lambda r: [('lv', bundle)]
    try:
        with contextlib.redirect_stderr(io.StringIO()) as err:
            meshview.Index(root, cache=cache)
    finally:
        meshview._bundles = saved
    c.check(not os.path.isfile(cache) and 'unreadable' in err.getvalue(),
            'mesh viewer index: a bundle it cannot read is said and the index is not cached', err.getvalue()[:200])


def terrain_stamp(c, work):
    from bfbc2core import fbrbtool, level, terrain
    root = os.path.join(work, 'opfor2')
    bdir = os.path.join(root, 'game', 'Dist', 'win32', 'levels', 'mp_x')
    os.makedirs(bdir)
    bundle = os.path.join(bdir, 'terrain-00.fbrb')
    open(bundle, 'wb').write(b'v1')
    unpacks = []

    def fake_run(args, **kw):                   # fbrbtool.run(['unpack', bundle, folder], check=True)
        unpacks.append(args[0])
        os.makedirs(args[2], exist_ok=True)
        open(os.path.join(args[2], 'mp_x.heightfield-000.res'), 'wb').write(open(args[1], 'rb').read())
    def fake_find(d, n):
        p = os.path.join(d, 'mp_x.heightfield-000.res')
        return p if os.path.isfile(p) else None
    stand_ins = [(fbrbtool, 'run', fake_run), (terrain, 'find', fake_find),
                 (terrain, 'read', lambda p: open(p, 'rb').read())]
    saved = [(m, n, getattr(m, n)) for m, n, _ in stand_ins]
    for m, n, f in stand_ins:
        setattr(m, n, f)
    try:
        first = level.heightfield_of(root, 'mp_x', log=lambda *a: None, name='mp_x')
        again = level.heightfield_of(root, 'mp_x', log=lambda *a: None, name='mp_x')
        n1 = len(unpacks)
        open(bundle, 'wb').write(b'v2 patched')
        patched = level.heightfield_of(root, 'mp_x', log=lambda *a: None, name='mp_x')
        os.remove(os.path.join(root, 'work', 'levels', 'mp_x', 'terrain', '.source.stamp'))
        level.heightfield_of(root, 'mp_x', log=lambda *a: None, name='mp_x')
    finally:
        for m, n, f in saved:
            setattr(m, n, f)
    c.check(first == b'v1' and again == b'v1' and n1 == 1, 'terrain unpack: reused while its bundle is the same',
            f'{first} {again} {n1}')
    c.check(patched == b'v2 patched', 'terrain unpack: a changed bundle is unpacked again', str(patched))
    c.check(len(unpacks) == 3, 'terrain unpack: an unpack without its stamp (cut short) is redone', str(len(unpacks)))


def navkit_caches(c, work):
    import navkit

    class Pr(navkit.Project):
        def __init__(self):
            self.root, self.work = work, os.path.join(work, 'w')

        def out_dir(self, lv):
            return os.path.join(work, 'nk', lv)
    pr = Pr()
    os.makedirs(pr.out_dir('lv'))
    open(pr.settings_path('lv'), 'w').write('{"box": [1, 2')
    try:
        pr.settings('lv')
        said = False
    except navkit.NavKitError as e:
        said = 'damaged' in str(e)
    c.check(said, 'NavKit: a damaged navkit.json is said as an error, not a JSON traceback')
    lw = os.path.join(work, 'lvwork')
    os.makedirs(os.path.join(lw, 'pkg'))
    open(os.path.join(lw, 'pkg', '.source.sha256'), 'w').write('aaa')
    k1 = navkit.source_stamps(lw)
    open(os.path.join(lw, 'pkg', '.source.sha256'), 'w').write('bbb')
    c.check(navkit.source_stamps(lw) != k1, 'NavKit: the detailed-sector cache key follows the level\'s unpack stamps')


def level_meshes_damaged(c, work):
    import shaderdb
    d = os.path.join(work, 'lvl')
    os.makedirs(d)
    open(os.path.join(d, 'deleted.dbx'), 'wb').write(b'')                 # a deleted entry: skipped
    open(os.path.join(d, 'damaged.dbx'), 'wb').write(b'{binary}\x00\x00\x00\x07')
    try:
        shaderdb.level_meshes(d)
        said = False
    except shaderdb.ShaderDbError as e:
        said = 'damaged.dbx' in str(e)
    c.check(said, 'shaderdb.level_meshes: a damaged partition is an error, a deleted one skipped')


def templates_skipped(c, work):
    import dbxport
    d = os.path.join(work, 'tpl')
    os.makedirs(d)
    open(os.path.join(d, 'bad.dbx'), 'wb').write(b'not a dbx')
    t = dbxport.Templates([d])
    c.check(len(t.skipped) == 1 and 'bad.dbx' in t.skipped[0], 'dbxport: a template that will not parse is recorded',
            str(t.skipped))


def dbx_damaged(c):
    bad = 0
    for data in (b'{binary}' + bytes(8), b'{binary}' + bytes([0, 0, 0, 5, 0, 0, 0, 1]) + b'ab\x00\x05\x00'):
        for fn in (dbx.parse, dbx.to_xml):
            try:
                fn(data)
            except ValueError:
                pass
            except Exception as e:                                          # noqa: BLE001 - the check itself
                bad += 1
                print('   ', fn.__name__, type(e).__name__, e)
    c.check(not bad, 'dbx: a damaged file raises ValueError from parse and to_xml')


def tools(c, work):
    """FbrbTool's pack and DbxEdit's import (in process since the exes were retired, 2026-11-18)."""
    from bfbc2core import dbxedit, fbrbtool
    src = os.path.join(work, 'packsrc')
    os.makedirs(src)
    open(os.path.join(src, 'a.txt'), 'wb').write(b'hello')
    open(os.path.join(src, 'fbrb.manifest'), 'w').write('a.txt|Text|0|0|0\n')
    out = os.path.join(work, 'out.fbrb')
    r1 = fbrbtool.run(['pack', src, out])
    r2 = fbrbtool.run(['pack', src, out])          # over the existing bundle
    left = [f for f in os.listdir(work) if '.tmp-' in f]
    c.check(r1.returncode == 0 and r2.returncode == 0 and os.path.getsize(out) > 8 and not left,
            'FbrbTool pack: writes a new bundle and replaces an existing one, no temporary file left',
            (r1.stdout + r1.stderr + r2.stdout + r2.stderr)[-200:])
    attrs = ' '.join(f'a{i}="{i}"' for i in range(16))
    x = os.path.join(work, 'many.xml')
    open(x, 'w', encoding='utf-8').write('<?xml version="1.0"?>\n<partition guid="00000000-0000-0000-0000-000000000001" '
                                         'primaryInstance="00000000-0000-0000-0000-000000000002" exportMode="All">\n'
                                         f'\t<instance guid="00000000-0000-0000-0000-000000000002" type="X.Y">\n'
                                         f'\t\t<field name="Name" {attrs}>x</field>\n\t</instance>\n</partition>\n')
    r = dbxedit.run(['import', x, os.path.join(work, 'many.dbx')])
    c.check(r.returncode not in (0, 3) or not os.path.isfile(os.path.join(work, 'many.dbx')),
            'DbxEdit: an element with 16 attributes is refused, not written with a corrupt kind', (r.stdout + r.stderr)[-200:])


def main(argv):
    c = Checks()
    for fn in (atomic_writes, stockmp_rollback, meshview_skip, terrain_stamp, navkit_caches, level_meshes_damaged,
               templates_skipped, tools):
        work = tempfile.mkdtemp(prefix='deep_scan_')
        try:
            fn(c, work)
        finally:
            shutil.rmtree(work, ignore_errors=True)
    dbx_damaged(c)
    print('all passed' if not c.failed else f'{c.failed} check(s) failed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
