r"""Round F1a's recording (2026-11-18): the retired Python readers' and writers' answers, digested, before they were
deleted - the reference data the tests compare the Rust core with from now on (goldens.py). Kept in history only (the
commit that records; the next one deletes the Python and this script).

  python core\tests\record_f1a.py dbx [--workers N]       dbx_golden.json, dbx_golden_corpus.json
  python core\tests\record_f1a.py fbrb [--workers N]      fbrb_golden.json
  python core\tests\record_f1a.py geometry [--workers N]  geometry_golden.json
  python core\tests\record_f1a.py scene                   scene_golden.json
  python core\tests\record_f1a.py culltree [--workers N]  bfbc2-culltree\tests\culltree_golden.json

Every answer is taken from the Python code itself (use_native(False), the _py_* functions, NavKit's levelobjects);
the Rust core's digest of the same case is computed beside it and every disagreement is printed (expect none but the
deliberate ones the tests list).
"""
import os
import random
import sys
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, HERE)
import goldens  # noqa: E402
from bfbc2core import dbx, paths  # noqa: E402

NOTE = ('Recorded 2026-11-18 (round F1a) from the retired Python code itself, before it was deleted; '
        'rust/BENCHMARKS.md "Retired Python". Digests: tests/goldens.py.')


def _workers(argv: list[str]) -> int:
    return int(argv[argv.index('--workers') + 1]) if '--workers' in argv else 3


# ------------------------------------------------------------------------------------------------ dbx

def _dbx_chunk(files: list[str]) -> list[tuple[str, str, str, str, int]]:
    import hashlib

    import test_dbx_native as tdn
    py = tdn.python_impl()
    out = []
    for f in files:
        data = open(f, 'rb').read()
        sha = hashlib.sha1(data).hexdigest()
        a, _ = tdn.file_digest(data, sha, py)
        b, _ = tdn.file_digest(data, sha, tdn.RUST)
        out.append((f, sha, a, b, len(data)))
    return out


def record_dbx(argv: list[str]) -> None:
    import test_dbx_native as tdn
    import test_dbx_write_native as tdw
    py = tdn.python_impl()
    t = time.time()
    cases: dict[str, Any] = {}
    for name, files in tdn.quick_cases().items():
        cases[name] = [tdn.outcome(d, py) for d in files]
        rs = [tdn.outcome(d) for d in files]
        print(f'dbx {name}: {len(files)} cases, Rust differs on {sum(a != b for a, b in zip(cases[name], rs))}')
    w_py = tdw.quick_outcomes(dbx._py_write)  # type: ignore[attr-defined]
    w_rs = tdw.quick_outcomes(dbx.write)
    cases.update(w_py)
    print('write built: same' if w_py['write_built'] == w_rs['write_built'] else 'write built: DIFFERS')
    for k, v in w_py['write_edges'].items():
        if v != w_rs['write_edges'][k]:
            print(f'write edge differs: {k}: Python {v}, Rust {w_rs["write_edges"][k]}')
    print(f'write random: Rust differs on {sum(a != b for a, b in zip(w_py["write_random"], w_rs["write_random"]))}')
    goldens.save(tdn.GOLDEN, {'note': NOTE, 'cases': cases})
    print(f'quick recorded ({time.time() - t:.0f} s)')

    root = paths.find_opfor()
    files = tdn.corpus_files(root)
    t = time.time()
    rec: dict[str, str] = {}
    differ = []
    total = 0
    chunks = [files[i:i + 100] for i in range(0, len(files), 100)]
    with ProcessPoolExecutor(_workers(argv)) as pool:
        for k, rows in enumerate(pool.map(_dbx_chunk, chunks)):
            for f, sha, a, b, size in rows:
                if sha[:16] in rec:
                    continue
                rec[sha[:16]] = a
                total += size
                if a != b:
                    differ.append(f)
            if k % 50 == 0:
                print(f'  {k * 100} / {len(files)} files, {len(rec)} distinct ({time.time() - t:.0f} s)', flush=True)
    for k, path in enumerate(tdn.GOLDEN_CORPUS):     # four shards by the SHA-1's first hex digit (under 1 MB each)
        goldens.save(path, {'note': NOTE, 'found': len(files), 'distinct': len(rec), 'bytes': total,
                            'shard': f'{k} of 4 (the SHA-1\'s first hex digit // 4)',
                            'files': {h: v for h, v in sorted(rec.items()) if int(h[0], 16) // 4 == k}})
    print(f'corpus: {len(files)} files, {len(rec)} distinct, {total / 1e6:.0f} MB; Rust differs on {len(differ)} '
          f'{differ[:5]} ({time.time() - t:.0f} s)')


# ------------------------------------------------------------------------------------------------ fbrb

def _py_fbrb() -> Any:
    from bfbc2core import fbrb

    def header(path: str) -> Any:
        with open(path, 'rb') as fh:
            return fbrb._header(fh)  # type: ignore[attr-defined]
    return header, fbrb._py_read  # type: ignore[attr-defined]


def _fbrb_chunk(job: tuple[list[str], str]) -> list[tuple[str, str, str, str, int, int]]:
    import test_fbrb_native as tfn
    files, root = job
    out = []
    for f in files:
        a, n, b = tfn.digest(f, _py_fbrb())
        r, _, _ = tfn.digest(f)
        out.append((os.path.relpath(f, root).replace(os.sep, '/'), goldens.file_sha(f)[:16], a, r, n, b))
    return out


def record_fbrb_api(argv: list[str]) -> None:
    """The 'api' part of fbrb_golden.json again (the messages' recording was normalized after the first run)."""
    import tempfile

    import test_fbrb_native as tfn
    from bfbc2core import fbrb
    work = tempfile.mkdtemp(prefix='rec_fbrb_')
    for i, d in enumerate(tfn.quick_cases()['built']):
        tfn.digest_bytes(d, work, i)
    fbrb.use_native(False)  # type: ignore[attr-defined]
    api = tfn.api_outcomes(work, fbrb.read, fbrb.read_many, fbrb.entries)
    fbrb.use_native(True)  # type: ignore[attr-defined]
    api_rs = tfn.api_outcomes(work, fbrb.read, fbrb.read_many, fbrb.entries)
    print('api: same' if api == api_rs else f'api DIFFERS: {api} / {api_rs}')
    rec = goldens.load(tfn.GOLDEN)
    rec['api'] = api
    # test_fbrb_cache.py's built bundles: every read pattern through the Python reader (the damaged copies it
    # compared with the Python reader: every 4th)
    import test_fbrb_cache as tfc
    good, stored, damaged, _ = tfc.built_cases()
    cache: dict[str, Any] = {'damaged': {}}
    for name, data in [('zipped', good), ('stored', stored)] + [(str(i), d) for i, d in enumerate(damaged) if i % 4 == 0]:
        p = os.path.join(work, f'cache_{name}.fbrb')
        with open(p, 'wb') as fh:
            fh.write(data)
        fbrb.use_native(False)  # type: ignore[attr-defined]
        py = [tfc.recorded(tfc.outcome(fbrb.read, p, x)) for x in tfc.PATTERNS]
        fbrb.use_native(True)  # type: ignore[attr-defined]
        rs = [tfc.recorded(tfc.outcome(fbrb.read_uncached, p, x)) for x in tfc.PATTERNS]
        print(f'cache case {name}: {"same" if py == rs else "DIFFERS"}')
        if name in ('zipped', 'stored'):
            cache[name] = py
        else:
            cache['damaged'][name] = py
    rec['cache'] = cache
    goldens.save(tfn.GOLDEN, rec)


def record_fbrb(argv: list[str]) -> None:
    import tempfile

    import test_fbrb_native as tfn
    from bfbc2core import fbrb
    t = time.time()
    work = tempfile.mkdtemp(prefix='rec_fbrb_')
    cases: dict[str, Any] = {}
    for name, items in tfn.quick_cases().items():
        cases[name] = [tfn.digest_bytes(d, work, i if name == 'built' else 5000, _py_fbrb()) for i, d in enumerate(items)]
        rs = [tfn.digest_bytes(d, work, i if name == 'built' else 5000) for i, d in enumerate(items)]
        print(f'fbrb {name}: {len(items)} cases, Rust differs on {sum(a != b for a, b in zip(cases[name], rs))}')
    for i, d in enumerate(tfn.quick_cases()['built']):
        tfn.digest_bytes(d, work, i)
    fbrb.use_native(False)  # type: ignore[attr-defined]
    api = tfn.api_outcomes(work, fbrb.read, fbrb.read_many, fbrb.entries)
    fbrb.use_native(True)  # type: ignore[attr-defined]
    api_rs = tfn.api_outcomes(work, fbrb.read, fbrb.read_many, fbrb.entries)
    print('api: same' if api == api_rs else f'api DIFFERS: {api} / {api_rs}')
    root = paths.find_opfor()
    files = tfn.game_bundles(root)
    game: dict[str, Any] = {}
    differ = []
    jobs = [([f], root) for f in sorted(files, key=os.path.getsize, reverse=True)]
    with ProcessPoolExecutor(_workers(argv)) as pool:
        for k, rows in enumerate(pool.map(_fbrb_chunk, jobs)):
            for rel, sha, a, r, n, b in rows:
                game[rel] = [sha, a, n, b]
                if a != r:
                    differ.append(rel)
            if k % 50 == 0:
                print(f'  {k} / {len(files)} bundles ({time.time() - t:.0f} s)', flush=True)
    goldens.save(tfn.GOLDEN, {'note': NOTE, 'cases': cases, 'api': api, 'game': dict(sorted(game.items()))})
    print(f'game: {len(files)} bundles, {sum(g[2] for g in game.values())} entries, '
          f'{sum(g[3] for g in game.values()) / 1e9:.2f} GB; Rust differs on {len(differ)} {differ[:5]} '
          f'({time.time() - t:.0f} s)')


# ------------------------------------------------------------------------------------------------ geometry

def _py_triangles(data: bytes) -> Any:
    """levelobjects.triangles on the bytes: hkpack.load pointed at them (collision.py's Python path)."""
    suite_paths.use('navkit-tools')
    import hkpack

    from bfbc2core import collision
    load = hkpack.load
    hkpack.load = lambda _path: hkpack.Packfile(data)
    try:
        return collision._py_triangles('<bytes>')  # type: ignore[attr-defined]
    finally:
        hkpack.load = load


def _geometry_python(on: bool) -> None:
    """The Python readers (on) or the Rust core's, for terrain, water, collision and the test's packfile reader."""
    import test_geometry_native as tgn

    from bfbc2core import collision, terrain, water
    for m in (terrain, water, collision):
        m.use_native(not on)  # type: ignore[attr-defined]
    tgn.TRIANGLES[0] = _py_triangles if on else tgn._rust_triangles


def _geometry_bundle(job: tuple[str, str]) -> tuple[Any, Any]:
    import test_geometry_native as tgn
    _geometry_python(True)
    a = tgn.bundle_outcome(*job)
    _geometry_python(False)
    return a, tgn.bundle_outcome(*job)


def record_geometry(argv: list[str]) -> None:
    import tempfile

    import test_geometry_native as tgn
    t = time.time()
    cases = tgn.quick_cases()
    _geometry_python(True)
    py = [tgn.outcome(kind, d, extra) for _, kind, d, extra in cases]
    with tempfile.TemporaryDirectory() as tmp:
        index_py = tgn.built_index(tmp)
    _geometry_python(False)
    rs = [tgn.outcome(kind, d, extra) for _, kind, d, extra in cases]
    with tempfile.TemporaryDirectory() as tmp:
        index_rs = tgn.built_index(tmp)
    print(f'geometry quick: {len(cases)} cases, Rust differs on {sum(a != b for a, b in zip(py, rs))}; '
          f'built index Python {index_py}, Rust {index_rs}')
    root = paths.find_opfor()
    out: dict[str, Any] = {'note': NOTE, 'cases': py, 'index_built': index_py[0]}
    mp = tgn.mp005_packfiles(root)
    if mp is not None:
        damaged = tgn.damaged_packfiles(mp[1])
        _geometry_python(True)
        a = [tgn.havok_outcome(d) for d in damaged]
        _geometry_python(False)
        b = [tgn.havok_outcome(d) for d in damaged]
        print(f'MP_005 damaged packfiles: {len(a)}, Rust differs on {sum(x != y for x, y in zip(a, b))}')
        out['mp005'] = {'sha': mp[0], 'cases': a}
    game: dict[str, Any] = {}
    differ = []
    bundles = tgn.corpus(root)
    counts: Counter[str] = Counter()
    with ProcessPoolExecutor(_workers(argv)) as pool:
        for k, (a, b) in enumerate(pool.map(_geometry_bundle, [(x, root) for x in bundles], chunksize=2)):
            rel, sha, d, c = a
            if sha is None and not d:
                continue
            counts.update(c)
            game[rel] = [sha, d, c]
            if a != b:
                differ.append(rel)
            if k % 100 == 0:
                print(f'  {k} / {len(bundles)} bundles ({time.time() - t:.0f} s)', flush=True)
    out['game'] = dict(sorted(game.items()))
    print(f'game: {len(game)} bundles with geometry, {dict(counts)}; Rust differs on {len(differ)} {differ[:5]} '
          f'({time.time() - t:.0f} s)')
    index: dict[str, Any] = {}
    for lv in tgn.prepared_levels(root):
        bases = tgn.level_bases(root, lv)
        _geometry_python(True)
        a = tgn.index_outcome(bases, root)
        _geometry_python(False)
        b = tgn.index_outcome(bases, root)
        index[lv] = [tgn.inputs_stamp(bases, root), a]
        print(f'  index {lv}: {"same" if a == b else "DIFFERS"} ({time.time() - t:.0f} s)', flush=True)
    out['index'] = index
    goldens.save(tgn.GOLDEN, out)


# ------------------------------------------------------------------------------------------------ scene

def record_scene(argv: list[str]) -> None:
    import test_scene_native as tsn

    from bfbc2core import collision, fbrb, level, terrain, water
    root = paths.find_opfor()
    out: dict[str, Any] = {}
    for lv in tsn.DEFAULT:
        why = tsn.ready(root, lv)
        if why:
            print(f'{lv}: {why} - not recorded')
            continue
        out[lv] = {'inputs': tsn.inputs(root, lv)}
        for geometry in (False, True):
            got = []
            for python in (True, False):
                for m in (level, dbx, fbrb, collision, terrain, water):
                    m.use_native(not python)  # type: ignore[attr-defined]
                lvl, t = tsn.load(lv, root, geometry)
                got.append(tsn.parts(lvl))
                print(f'{lv} geometry={geometry} {"Python" if python else "Rust"}: {len(lvl.entities)} entities, '
                      f'{len(lvl.meshes)} meshes, {t:.1f} s', flush=True)
            diff = [k for k in got[0] if got[0][k] != got[1][k]]
            print(f'  Rust differs on: {diff}')
            out[lv]['whole' if geometry else 'entities only'] = got[0]
    goldens.save(tsn.GOLDEN, {'note': NOTE, 'levels': out})


# ------------------------------------------------------------------------------------------------ culltree

def _culltree() -> Any:
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-culltree', 'tests'))
    suite_paths.use('culltree')
    import culltree as ct
    import test_culltree_native as tcn
    return ct, tcn


def _culltree_chunk(job: tuple[list[tuple[str, str, str]], str]) -> list[tuple[str, str, str, str]]:
    ct, tcn = _culltree()
    items, root = job
    out = []
    for item in items:
        ct.use_native(False)
        label, sha, a, _, _ = tcn.file_outcome(item)
        ct.use_native(True)
        _, _, b, _, _ = tcn.file_outcome(item)
        out.append((tcn.key(label, root), sha, a, b))
    return out


def record_culltree(argv: list[str]) -> None:
    import tempfile
    ct, tcn = _culltree()
    t = time.time()
    cases = tcn.quick_cases()
    ct.use_native(False)
    py = [tcn.outcome(d)[0] for _, d in cases]
    with tempfile.TemporaryDirectory() as tmp:
        add_py = tcn.built_add(tmp)
    ct.use_native(True)
    rs = [tcn.outcome(d)[0] for _, d in cases]
    with tempfile.TemporaryDirectory() as tmp:
        add_rs = tcn.built_add(tmp)
    print(f'culltree quick: {len(cases)} cases, Rust (with the Python fallback) differs on '
          f'{sum(a != b for a, b in zip(py, rs))}; built add {"same" if add_py == add_rs else "DIFFERS"}')
    root = paths.find_opfor()
    items = tcn.corpus(root)
    game: dict[str, Any] = {}
    differ = []
    jobs = [(items[i:i + 8], root) for i in range(0, len(items), 8)]
    with ProcessPoolExecutor(_workers(argv)) as pool:
        for k, rows in enumerate(pool.map(_culltree_chunk, jobs)):
            for key, sha, a, b in rows:
                game[key] = [sha, a]
                if a != b:
                    differ.append(key)
            if k % 50 == 0:
                print(f'  {k * 8} / {len(items)} files ({time.time() - t:.0f} s)', flush=True)
    print(f'game: {len(items)} trees and levels; Rust differs on {len(differ)} {differ[:5]} ({time.time() - t:.0f} s)')
    out: dict[str, Any] = {'note': NOTE, 'cases': py, 'built_add': add_py, 'game': dict(sorted(game.items()))}
    with tempfile.TemporaryDirectory(prefix='rec_cull_') as tmp:
        t = time.time()
        got = tcn.add_run(root, tmp, {'BFBC2_PURE_PYTHON': '1', 'BFBC2_CULLTREE_CACHE': '0'})
        print(f'add run, Python: {time.time() - t:.0f} s')
    if got is not None:
        source, inputs, (code, report, files) = got
        out['add'] = {'source': source, 'inputs': inputs, 'result': [code, goldens.h(report, 16), files]}
        with tempfile.TemporaryDirectory(prefix='rec_cull_') as tmp:
            t = time.time()
            got_rs = tcn.add_run(root, tmp, {'BFBC2_PURE_PYTHON': '', 'BFBC2_CULLTREE_CACHE': '0'})
            print(f'add run, Rust: {time.time() - t:.0f} s, {"same" if got_rs == got else "DIFFERS"}; exit {code}, '
                  f'{len(report.splitlines())} report lines')
    goldens.save(tcn.GOLDEN, out)


# ------------------------------------------------------------------------------------------------ NavKit's tools

def record_navkit(argv: list[str]) -> None:
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests'))
    import test_collision_backends as tcb

    from bfbc2core import collision
    root = paths.find_opfor()
    out: dict[str, Any] = {}
    for lv in tcb.DEFAULT:
        got = []
        for python in (True, False):
            collision.use_native(not python)  # type: ignore[attr-defined]
            t = time.time()
            got.append(tcb.digests(root, lv))
            print(f'{lv} {"Python" if python else "Rust"}: {time.time() - t:.0f} s', flush=True)
        print(f'  Rust differs on: {[k for k in got[0] if got[0][k] != got[1][k]]}')
        out[lv] = {'inputs': tcb.inputs(root, lv), 'outcomes': got[0]}
    goldens.save(tcb.GOLDEN, {'note': NOTE, 'levels': out})


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    what = argv[1] if len(argv) > 1 else ''
    fn = {'dbx': record_dbx, 'fbrb': record_fbrb, 'fbrb-api': record_fbrb_api, 'geometry': record_geometry,
          'scene': record_scene, 'culltree': record_culltree, 'navkit': record_navkit}.get(what)
    if fn is None:
        print(__doc__)
        return 1
    fn(argv)
    return 0


if __name__ == '__main__':
    random.seed(0)
    Counter()
    sys.exit(main(sys.argv))
