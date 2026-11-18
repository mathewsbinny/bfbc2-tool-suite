r"""Round F2b (rust/BENCHMARKS.md, "F2b"): emitteratlas.build / prove / same_but_uv in the Rust core against
build_reference / prove_reference / same_but_uv_reference - the atlases and partitions byte for byte, the report, the
fields left, every use (rectangle, move, old and new fields, its cell), every kept cell (place, core, what it is drawn
from), the proof row for row, the tree differences.

  python test_emitteratlas_native.py          quick: 40 seeded synthetic levels (8-bit and BC5 sources, copies of an
                                              image at grid offsets, flipbooks, spawned frames, fields of no image,
                                              flat images, small normal images whose probe window is cut)
  python test_emitteratlas_native.py --game   + the inputs of Project OPFOR's four emitter_atlas runs (Wake, Coral Sea,
                                              Guadalcanal, Iwo Jima: rebuilt read only from their manifests as
                                              rust\bench_bf1943.py capture does)

The module is required (the test fails without it, saying how to build it).
"""
import json
import os
import random
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'console-convert', 'model-tools', 'core')
import emitteratlas as ea  # noqa: E402
import importcore  # noqa: E402
import numpy as np  # noqa: E402

sys.path.insert(0, HERE)
import test_emitteratlas as te  # noqa: E402

FAILS: list[str] = []
COUNT = {'built': 0, 'refused': 0}


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def cell_view(c):
    chain = []
    while c is not None:
        chain.append((c.source, c.x, c.y, c.w, c.h, c.dx, c.dy, c.at, c.core))
        c = c.into
    return chain


def result_view(r):
    return {'alpha': r.alpha, 'normals': r.normals, 'partitions': list(r.partitions.items()),
            'uses': [(u.emitter, u.instance, u.info, u.channel, u.source, u.rect, cell_view(u.cell), u.move, u.old,
                      u.new) for u in r.uses],
            'cells': {k: [cell_view(c) for c in v] for k, v in r.cells.items()}, 'sizes': r.sizes,
            'report': r.report, 'left': r.left}


def both(emitters, sources, like_a, like_n, label):
    """build / prove / same_but_uv both ways; True when the core built it (not refused)."""
    t0 = time.perf_counter()
    try:
        got = ('ok', ea._build_rust(emitters, sources, like_a, like_n))
    except ea.EmitterAtlasError as e:
        got = ('err', str(e))
    t1 = time.perf_counter()
    try:
        want = ('ok', ea.build_reference(emitters, sources, like_a, like_n))
    except ea.EmitterAtlasError as e:
        want = ('err', str(e))
    t2 = time.perf_counter()
    if got[0] == 'ok' and got[1] is None:
        COUNT['refused'] += 1
        return None
    COUNT['built'] += 1
    if got[0] == 'err' or want[0] == 'err':
        check(got == want, f'{label}: the same error', f'{got} / {want}')
        return None
    a, b = result_view(got[1]), result_view(want[1])
    bad = [k for k in a if a[k] != b[k]]
    check(not bad, f'{label}: build the same - atlases, {len(a["partitions"])} partitions, {len(a["uses"])} uses, '
          f'cells, report', f'differ: {bad}')
    t3 = time.perf_counter()
    pg = ea._prove_rust(want[1], emitters, sources)
    t4 = time.perf_counter()
    pr = ea.prove_reference(want[1], emitters, sources)
    t5 = time.perf_counter()
    if pg is None:
        COUNT['refused'] += 1
        print(f'     {label}: prove left to the Python')
    else:
        check(json.dumps(pg, sort_keys=True) == json.dumps(pr, sort_keys=True),
              f'{label}: prove the same ({len(pr)} rows, {sum(p["ok"] for p in pr)} ok)')
    tg = [ea.same_but_uv(emitters[n][0], d) for n, d in want[1].partitions.items()]
    tr = [ea.same_but_uv_reference(emitters[n][0], d) for n, d in want[1].partitions.items()]
    check(tg == tr, f'{label}: same_but_uv the same ({sum(map(len, tr))} differences)')
    return {'build': (t1 - t0, t2 - t1), 'prove': (t4 - t3, t5 - t4)}


def quick_level(rnd, i):
    N = rnd.choice([64, 128])
    a = (rnd.random() * 255 * np.ones((N, N))).astype(np.uint8)
    for k in range(rnd.randint(2, 6)):                         # images on DICE's 8-texel slots
        y, x, s = rnd.randrange(0, N - 24, 8), rnd.randrange(0, N - 24, 8), rnd.choice([8, 16, 24])
        a[y:y + s, x:x + s] = np.array([[rnd.randrange(256) for _ in range(s)] for _ in range(s)], np.uint8)
    b = a.copy() if rnd.random() < 0.5 else np.roll(a, 8 * rnd.randint(1, 3), 1)
    nrng = np.random.default_rng(i)
    nm = te.bc5_atlas(nrng, N)
    sources = {'a': ea.Source(te.l8_atlas(a), nm), 'b': ea.Source(te.l8_atlas(b), te.bc5_atlas(nrng, N))}
    if rnd.random() < 0.3:
        sources['c'] = ea.Source(te.l8_atlas(a[::-1].copy()), None)
    em = {}
    for k in range(rnd.randint(1, 8)):
        src = rnd.choice(list(sources))
        s = rnd.choice([8, 16, 24]) / N
        uv = (rnd.randrange(0, N - 32, 8) / N + rnd.choice([0, 3 / N]), rnd.randrange(0, N - 32, 8) / N, s, s)
        nm_uv = (0.0, 0.0, 1.0, 1.0) if src == 'c' or rnd.random() < 0.3 else \
            (rnd.randrange(0, N - 32, 8) / N, rnd.randrange(0, N - 32, 8) / N, rnd.choice([4, 8, 16]) / N, 8 / N)
        if rnd.random() < 0.2:
            uv = (0.0, 0.0, 1.0, 1.0)
        frames, cols = rnd.choice([(1, 1), (4, 2), (2, 2)])
        em[f'fx/e{k}.dbx'] = (te.emitter(f'G{k}', uv, nm_uv, frames, cols, rnd.random() < 0.5, rnd.random() < 0.2),
                              src)
    like_a = te.l8_atlas(np.zeros((N * 2, N * 2), np.uint8))
    like_n = te.bc5_atlas(nrng, N * 2)
    return em, sources, like_a, like_n


def quick():
    rnd = random.Random(17)
    for i in range(40):
        try:
            em, sources, la, ln = quick_level(rnd, i)
        except Exception as e:                                   # noqa: BLE001 - a case the builders cannot make
            print(f'     case {i}: not built ({e})')
            continue
        both(em, sources, la, ln, f'synthetic level {i + 1}')


def game():
    import pickle
    sys.path.insert(0, os.path.join(_ROOT, 'rust'))
    import bench_bf1943
    import shaderdb
    try:
        shaderdb.find_opfor(None)
    except shaderdb.ShaderDbError as e:
        print(f'SKIP game: {e}')
        return
    with tempfile.TemporaryDirectory() as d:
        for folder in ('emitter_atlas', 'emitter_atlas_coral_sea', 'emitter_atlas_guadal_canal',
                       'emitter_atlas_iwo_jima_s'):
            path = os.path.join(d, folder + '.pkl')
            try:
                bench_bf1943.capture_atlas(path, folder)
            except (OSError, SystemExit, KeyError) as e:
                print(f'SKIP {folder}: {e}')
                continue
            with open(path, 'rb') as fh:
                x = pickle.load(fh)
            sources = {k: ea.Source(a, n) for k, (a, n) in x['sources'].items()}
            t = both(x['emitters'], sources, x['like'][0], x['like'][1], folder)
            if t:
                print(f'     {folder}: build {t["build"][0]:.1f} s (reference {t["build"][1]:.1f} s), prove '
                      f'{t["prove"][0]:.1f} s (reference {t["prove"][1]:.1f} s)')


def main(argv):
    from bfbc2core import native
    try:
        native.require('the BF1943 importer\'s emitter atlases (round F2b)', 'emitteratlas_build')
    except native.NativeMissing as e:
        check(False, 'the Rust core is built from these sources', str(e))
        return 1
    if importcore.native() is None:
        check(False, 'importcore finds the module', 'BFBC2_IMPORTER_PYTHON / BFBC2_PURE_PYTHON set?')
        return 1
    quick()
    if '--game' in argv:
        game()
    print(f'{COUNT["built"]} built by the core, {COUNT["refused"]} left to the Python')
    if FAILS:
        print(f'{len(FAILS)} FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
