r"""pcbuild's compile cache and batches (round E3d): compile_hlsl remembers compiles (memory, disk), compile_many /
prefetch compile on all cores; every result must be the bytes a plain compile gives.

Quick (no game data; Windows' d3dcompiler_47): 48 made-up shaders (vertex and pixel, 4_0 / 5_0, defines, loops,
texture fetches) compiled twice one at a time (the compiler is deterministic), then by compile_many (the same bytes,
in input order, a repeated job compiled once, a failing job's CompileError in its place or raised first in input
order), by prefetch + compile_hlsl (joined, not compiled twice); the disk cache: written whole, read back the same,
a damaged or foreign entry a miss (compiled and rewritten), keys apart for every input (source, entry, profile,
defines and their order, name), BFBC2_HLSL_CACHE=0 / a folder.
With --game: every pixel shader of Battlefield 1943's Wake database translated (xdiff's translation) and compiled by
compile_many on all cores and one at a time without the cache: the same bytes or the same error for each (ps_5_0 and
ps_4_0).

  python test_pcbuild_batch.py [--game]
"""
import os
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'console-convert', 'core')
import pcbuild as P  # noqa: E402

fails: list[str] = []


def check(ok: bool, what: str) -> None:
    print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)
    if not ok:
        fails.append(what)


PS = '''Texture2D t0 : register(t0); SamplerState s0 : register(s0);
float4 k[%(n)d];
float4 main(float4 p : SV_Position, float4 uv : TEXCOORD0) : SV_Target {
    float4 a = t0.Sample(s0, uv.xy * %(scale)s);
    [unroll] for (int i = 0; i < %(n)d; i++) a = a * k[i] + sin(a.yzwx * %(f)s);
#ifdef EXTRA
    a = sqrt(abs(a)) * EXTRA;
#endif
    return a;
}'''
VS = '''float4x4 wvp; float4 k[%(n)d];
struct O { float4 p : SV_Position; float4 t : TEXCOORD0; };
O main(float4 p : POSITION, float4 t : TEXCOORD0) {
    O o; o.p = mul(p, wvp); o.t = t;
    [unroll] for (int i = 0; i < %(n)d; i++) o.t = o.t * k[i] + %(f)s;
    return o;
}'''


def made_up() -> list[P.CompileJob]:
    jobs = []
    for i in range(24):
        v = {'n': 1 + i % 7, 'scale': f'{1 + i * 0.25:.2f}', 'f': f'{0.5 + i * 0.125:.3f}'}
        defines = {'EXTRA': f'{i}.5'} if i % 3 == 0 else None
        prof = '5_0' if i % 2 else '4_0'
        jobs.append(P.CompileJob(PS % v, 'main', 'ps_' + prof, defines))
        jobs.append(P.CompileJob(VS % v, 'main', 'vs_' + prof))
    return jobs


def quick(tmp: str) -> None:
    P.use_cache(os.path.join(tmp, 'cache'))
    P.clear_memory()
    jobs = made_up()
    t = time.perf_counter()
    one = [P.compile_hlsl_uncached(*j) for j in jobs]
    seq = time.perf_counter() - t
    two = [P.compile_hlsl_uncached(*j) for j in jobs]
    check(one == two and all(one), f'the compiler is deterministic: {len(jobs)} shaders compiled twice, the same bytes')

    P.stats.clear()
    bad = P.CompileJob('float4 main() : SV_Target { return undefined_thing; }', 'main', 'ps_5_0')
    batch = jobs + [jobs[0], bad, jobs[5]]
    t = time.perf_counter()
    got = P.compile_many(batch, errors='return')
    par = time.perf_counter() - t
    check(got[:len(jobs)] == one and got[len(jobs)] == one[0] and got[-1] == one[5],
          f'compile_many: every result the plain compile\'s, in input order ({seq:.2f} s one at a time, '
          f'{par:.2f} s batched on {P.workers()} threads)')
    check(isinstance(got[-2], P.CompileError) and 'undefined_thing' in str(got[-2]),
          'compile_many: a failing job\'s CompileError in its place (errors=\'return\')')
    check(P.stats['compiled'] == len(jobs), f'a job repeated in the batch is compiled once ({P.stats["compiled"]} '
          f'compiles for {len(jobs)} distinct jobs)')
    try:
        P.compile_many([jobs[1], bad, P.CompileJob('float4 main() : SV_Target { return nope2; }')])
        check(False, 'compile_many raises the first failure')
    except P.CompileError as e:
        check('undefined_thing' in str(e), 'compile_many raises the first failure in input order (errors=\'raise\')')
    check(P.compile_many([]) == [], 'compile_many of nothing: nothing')

    # memory, then disk
    P.stats.clear()
    check(P.compile_hlsl(*jobs[3]) == one[3] and P.stats['memory'] == 1 and not P.stats['compiled'],
          'compile_hlsl: a compiled job taken from memory')
    P.clear_memory()
    P.stats.clear()
    check(P.compile_hlsl(*jobs[3]) == one[3] and P.stats['disk'] == 1 and not P.stats['compiled'],
          'compile_hlsl: a compiled job taken from the disk cache')
    folder = P.cache_dir()
    assert folder is not None
    files = [os.path.join(r, f) for r, _, fs in os.walk(folder) for f in fs]
    check(len(files) == len(jobs) and not any('.tmp-' in f for f in files),
          f'the disk cache: one whole file per distinct job ({len(files)}), no temporary files left')
    key = P._key(*jobs[3])
    path = P._disk_path(folder, key)
    data = open(path, 'rb').read()
    for what, damaged in (('a damaged entry', data[:-1] + bytes([data[-1] ^ 1])), ('a cut entry', data[:40]),
                          ('another key\'s entry', open(P._disk_path(folder, P._key(*jobs[4])), 'rb').read())):
        with open(path, 'wb') as fh:
            fh.write(damaged)
        P.clear_memory()
        P.stats.clear()
        check(P.compile_hlsl(*jobs[3]) == one[3] and P.stats['compiled'] == 1 and open(path, 'rb').read() == data,
              f'{what} is a miss: compiled, and written again')

    # prefetch: compile_hlsl joins a compile under way instead of compiling again
    P.clear_memory()
    P.use_cache(False)
    P.stats.clear()
    P.prefetch(jobs[:16])
    got = [P.compile_hlsl(*j) for j in jobs[:16]]
    check(got == one[:16] and P.stats['compiled'] == 16, f'prefetch, then compile_hlsl: the same bytes, 16 compiles '
          f'({P.stats["compiled"]}; {P.stats["joined"]} joined under way, {P.stats["memory"]} done before)')

    # keys
    j = jobs[0]
    variants = [j, j._replace(src=j.src + ' '), j._replace(entry='main2'), j._replace(profile='ps_4_1'),
                j._replace(name='other.hlsl'), j._replace(defines={'A': 1, 'B': 2}),
                j._replace(defines={'B': 2, 'A': 1}), j._replace(defines={'A': '1', 'B': 2.0})]
    keys = {P._key(*v) for v in variants}
    check(len(keys) == len(variants),
          f'cache keys apart for each input (source, entry, profile, name, defines, their order): {len(keys)} keys '
          f'for {len(variants)} variants')
    check(P._key(*j._replace(defines={'A': 1})) == P._key(*j._replace(defines={'A': '1'})),
          'defines keyed as D3DCompile sees them (str of the value)')

    old = os.environ.get('BFBC2_HLSL_CACHE')
    try:
        P.use_cache(None)
        os.environ['BFBC2_HLSL_CACHE'] = '0'
        check(P.cache_dir() is None, 'BFBC2_HLSL_CACHE=0: no disk cache')
        os.environ['BFBC2_HLSL_CACHE'] = os.path.join(tmp, 'env')
        d = P.cache_dir()
        check(d is not None and d.startswith(os.path.join(tmp, 'env')) and d.endswith(P.compiler_id()[:16]),
              'BFBC2_HLSL_CACHE=<folder>: that folder, one subfolder per compiler')
        del os.environ['BFBC2_HLSL_CACHE']
        d = P.cache_dir()
        check(d is not None and os.path.join('bfbc2-tool-suite', 'cache', 'hlsl') in d,
              r'the default: %LOCALAPPDATA%\bfbc2-tool-suite\cache\hlsl')
    finally:
        if old is None:
            os.environ.pop('BFBC2_HLSL_CACHE', None)
        else:
            os.environ['BFBC2_HLSL_CACHE'] = old
        P.use_cache(None)


def game() -> None:
    suite_paths.use('bf1943-import')
    import numpy as np
    import xdiff
    import xenos
    try:
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
        data = src.data(xdiff.DBS['wake'])
    except Exception as e:                                      # noqa: BLE001 - only missing game data skips
        print(f'SKIP game: no BF1943 source ({type(e).__name__}: {e})')
        return
    w = xdiff.S.ShaderDb.parse(data, where='wake')
    named = xdiff.engine_named(w)
    jobs = []
    untranslated = 0
    for p in range(len(w.ps)):
        try:
            sh = xenos.parse_shader(w.ps[p].code, True)
            cs = xdiff.case(sh, np.random.default_rng(100003 + p), 16, named.get(p))
            hlsl = xdiff.translated(sh, cs)
        except Exception:                                       # noqa: BLE001 - counted: not this test's subject
            untranslated += 1
            continue
        jobs.append(P.CompileJob(hlsl, 'main', 'ps_5_0' if p % 2 else 'ps_4_0'))
    P.use_cache(False)
    P.clear_memory()
    P.stats.clear()
    t = time.perf_counter()
    got = P.compile_many(jobs, errors='return')
    par = time.perf_counter() - t
    t = time.perf_counter()
    ref: list = []
    for j in jobs:
        try:
            ref.append(P.compile_hlsl_uncached(*j))
        except P.CompileError as e:
            ref.append(e)
    seq = time.perf_counter() - t
    same = sum((a == b) if isinstance(b, bytes) else (isinstance(a, P.CompileError) and str(a) == str(b))
               for a, b in zip(got, ref))
    errs = sum(isinstance(b, P.CompileError) for b in ref)
    check(same == len(jobs) and len(got) == len(jobs),
          f'Wake\'s {len(jobs)} translated pixel shaders ({untranslated} not translated, {errs} compile errors): '
          f'compile_many gives each the plain compile\'s bytes or error ({same} of {len(jobs)}); {seq:.1f} s one at a '
          f'time, {par:.1f} s batched on {P.workers()} threads ({len(set(P._key(*j) for j in jobs))} distinct)')
    P.use_cache(None)


if __name__ == '__main__':
    tmp = tempfile.mkdtemp(prefix='pcbuild_batch_')
    try:
        quick(tmp)
        if '--game' in sys.argv:
            game()
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not fails else f'{len(fails)} failed')
    sys.exit(1 if fails else 0)
