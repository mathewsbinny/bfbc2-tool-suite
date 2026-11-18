r"""Times the rest of the BF1943 importer (rust/BENCHMARKS.md, "E3r") on the user's BF1943 copy, each output's SHA-256
recorded: run it in two checkouts (before / after) with --record, then --compare the files.

  python rust\bench_bf1943.py [source] [import] [fair] [parity] [atlas <inputs.pkl>]
                              [--record out.json] [--compare in.json] [--profile out.prof] [--top N]

  source   bf1943import.Source: the open (twice: the second from the bundle index where there is one) and every entry
           read in name order (the entries' SHA-256)
  import   bf1943import.Importer.convert of every animation, delta animation, skeleton and aim animation (each
           output's SHA-256 and its record), the undeploys with thumb donors, then check()
  fair     dbxport.port of every partition both games ship, as tests\test_dbxport_fair.py ports it (the fair pool and
           catalogue, BFBC2's soldier skeleton) and with DICE's version in the pool (the counterpart): each output's
           SHA-256 and its report's
  parity   dbxparity.compare over the same partitions and dbxparity.predict over the weapons and vehicles (their
           reports' SHA-256)
  atlas    emitteratlas.build, prove and same_but_uv on the inputs of Project OPFOR's last emitter_atlas run (a
           pickle made by `capture <inputs.pkl>`: its 615 emitter partitions, 14 authoring atlases and mp_005's,
           rebuilt from its manifest - the build must give the atlases that run recorded): the atlases', partitions',
           report's and proof's SHA-256

  python rust\bench_bf1943.py capture <inputs.pkl>      (reads Project OPFOR's work\ww2\emitter_atlas\manifest.json)

Runs the checkout it lies in (copy it into another checkout's rust\ to time that one). The game copy and BF1943's
copy are read only; nothing is written but --record / --profile's files.
"""
import cProfile
import hashlib
import json
import os
import pickle
import pstats
import sys
import time

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'mesh-viewer', 'shader-tools', 'console-convert', 'anim-tools', 'model-tools', 'core')
import bf1943import as bi  # noqa: E402
import meshview  # noqa: E402
import shaderdb  # noqa: E402

H = 'animations/weapons/handheld/'


def sha(b):
    return hashlib.sha256(b if isinstance(b, bytes) else str(b).encode()).hexdigest()


def peak_mb():
    try:
        import ctypes
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + \
                [(n, ctypes.c_size_t) for n in ('PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage',
                                                'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
                                                'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage')]
        c = Counters()
        c.cb = ctypes.sizeof(c)
        psapi, kernel = ctypes.WinDLL('psapi'), ctypes.WinDLL('kernel32')
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD]
        if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(c), c.cb):
            return None
        return c.PeakWorkingSetSize / 2 ** 20
    except (OSError, AttributeError):
        return None


def run_source(src, out, times):
    t = time.perf_counter()
    again = bi.Source(src.folder)
    times['source: second open'] = time.perf_counter() - t
    t = time.perf_counter()
    for k in sorted(again.entries):
        out[f'source:{k}'] = sha(again.data(k))
    times['source: every entry'] = time.perf_counter() - t


def run_import(src, out, times):
    imp = bi.Importer(src)
    kinds = bi.ANIMATIONS + ('GrannyModel', 'AimAnimation')
    keys = [k for k, (_, e) in sorted(src.entries.items()) if e['type'] in kinds]
    for k in keys:
        try:
            b, rec = imp.convert(k)
            out[f'import:{k}'] = sha(b or b'') + ' ' + json.dumps(rec, sort_keys=True)
        except Exception as e:                                      # noqa: BLE001 - recorded: both must agree
            out[f'import:{k}'] = f'error {type(e).__name__}: {e}'
    don = bi.Importer(src, thumb_donors={H + 'm1garand': H + 'vietnam/nam_rif_m1garand',
                                         H + 'beach/k98': H + 'us_sni_m24'})
    for k in [k for k in keys if bi.UNDEPLOY.search(k) and ('/m1garand/' in k or '/k98/' in k)]:
        b, rec = don.convert(k)
        out[f'import-thumbs:{k}'] = sha(b or b'') + ' ' + json.dumps(rec, sort_keys=True)
    t = time.perf_counter()
    out['import:check'] = json.dumps(bi.check(src, log=lambda s: None))
    times['import: check'] = time.perf_counter() - t


def run_fair(src, ix, out, times):
    import dbxcatalogue
    import dbxparity
    import dbxport
    cat = dbxcatalogue.load(ix)
    bones = dbxport.BoneTables.ske01(src, ix)
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')}
    both = sorted(k for k in src.entries if k.endswith('.dbx') and k in ix.where and
                  'build_overlay' not in ix.where[k][0].lower())
    tcache = {}

    def templates(key):
        key = tuple(key)
        if key not in tcache:
            tcache[key] = dbxport.Templates.from_index(ix, key)
        return tcache[key]
    by_folder: dict[str, set[str]] = {}
    for e in src.entries:
        if e.endswith('.dbx'):
            by_folder.setdefault(e.rsplit('/', 1)[0], set()).add(e[:-4])
    for k in both:
        name = k[:-4]
        ported = set(by_folder.get(name.rsplit('/', 1)[0], ()))
        ours = {p: dbxport.guids_of(src.data(p + '.dbx')) for p in ported}

        def resolve(p, ported=ported):
            q = p.lower()
            return q in ported or q in bc2 or '/shaders/' in q or '/shader/' in q
        pool = dbxparity.fair_pool(k)
        kw = dict(drop_types=dbxport.NOT_IN_BFBC2, drop_untemplated=True, bones=bones, name=name, ours=ours)
        for what, T, c in (('fair', [p for p in pool if p != name], dbxparity.fair_catalogue(cat, name)),
                           ('counterpart', [name] + [p for p in pool if p != name], cat)):
            if what == 'counterpart' and b'FiringFunctionData' not in src.data(k):
                continue
            try:
                data, rep = dbxport.port(src.data(k), templates(T), resolve, catalogue=c, **kw)
                out[f'{what}:{k}'] = sha(data) + ' ' + sha('\n'.join(rep))
            except dbxport.PortError as e:
                out[f'{what}:{k}'] = f'error {type(e).__name__}: {e}'


def run_parity(src, ix, out, times):
    import dbxcatalogue
    import dbxparity
    import dbxport
    both = sorted(k for k in src.entries if k.endswith('.dbx') and k in ix.where)
    t = time.perf_counter()
    partitions, matched, stats, examples, enums = dbxparity.compare(src, ix, both, log=lambda s: None)
    times['parity: compare'] = time.perf_counter() - t
    out['parity:compare'] = sha(repr((partitions, matched, sorted((k, sorted(v.items())) for k, v in stats.items()),
                                      sorted(examples.items()), sorted((k, sorted(v.items())) for k, v in enums.items()))))
    cat = dbxcatalogue.load(ix)
    bones = dbxport.BoneTables.ske01(src, ix)
    names = sorted(k for k in both if dbxparity.family(k) not in ('objects', 'effects', 'sounds', 'other'))
    t = time.perf_counter()
    got, ex = dbxparity.predict(src, ix, names, cat, bones, log=lambda s: None)
    times['parity: predict'] = time.perf_counter() - t
    out['parity:predict'] = sha(repr((sorted((k, sorted(v.items())) for k, v in got.items()), sorted(ex.items()))))


def capture_atlas(path):
    """The inputs of Project OPFOR's last emitter_atlas run, from its manifest (work\\ww2\\emitter_atlas, read only):
    each emitter partition as the install took it (a stock bundle's entry, or the WW2 build's file - its SHA-1 checked
    against the manifest's), each authoring atlas (a BFBC2 level's from its level bundle, BF1943's converted), and
    mp_005's atlases (the `like` ones). Pickled to path; the build on them must give the manifest's atlases."""
    import texture
    from bfbc2core import fbrb
    root = shaderdb.find_opfor(None)
    man = json.load(open(os.path.join(root, 'work', 'ww2', 'emitter_atlas', 'manifest.json'), encoding='utf-8'))
    atlases = ('emittertexturealpha.res', 'emittertexturenormals.res')
    emitters, bad = {}, []
    for low, m in man['emitters'].items():
        f = m['from']
        if isinstance(f, list):
            data = fbrb.read(f[0], [f[1]])[f[1]]
        else:
            data = open(os.path.join(root, 'work', 'ww2', f), 'rb').read()
        if hashlib.sha1(data).hexdigest() != m['sha1_in']:
            bad.append(low)
        emitters[low] = (data, m['source'])
    src = bi.Source(bi.default_source())
    game = os.path.join(root, 'game')

    def level_atlases(lv):
        for top in (os.path.join(game, 'Package', 'levels'), os.path.join(game, 'Dist', 'win32', 'levels')):
            b = os.path.join(top, lv, 'level-00.fbrb')
            if os.path.isfile(b):
                names = {e.name.lower(): e.name for e in fbrb.entries(b) if not e.deleted}
                want = [names.get(f'levels/{lv}/{a}') or '' for a in atlases]
                if all(want):
                    got = fbrb.read(b, want)
                    return tuple(got[w] for w in want)
        raise SystemExit(f'no atlases for {lv}')
    sources = {}
    for key in sorted({m['source'] for m in man['emitters'].values()}):
        if key.startswith('bf1943:'):
            lv = key.split(':')[1]
            sources[key] = tuple(texture.convert(src.data(f'levels/{lv}/{a}')) for a in atlases)
        else:
            sources[key] = level_atlases(key)
    like = level_atlases('mp_005')
    with open(path, 'wb') as fh:
        pickle.dump({'emitters': emitters, 'sources': sources, 'like': like,
                     'want': {k: v['sha1'] for k, v in man['atlases'].items()}}, fh)
    print(f'{len(emitters)} emitters ({len(bad)} not as the manifest recorded them: {bad[:5]}), {len(sources)} '
          f'sources -> {path}')


def run_atlas(inputs, out, times):
    import emitteratlas as ea
    with open(inputs, 'rb') as fh:
        d = pickle.load(fh)
    em, like = d['emitters'], d['like']
    sources = {k: ea.Source(a, n) for k, (a, n) in d['sources'].items()}
    t = time.perf_counter()
    r = ea.build(em, sources, like[0], like[1])
    times['atlas: build'] = time.perf_counter() - t
    t = time.perf_counter()
    proof = ea.prove(r, em, sources)
    times['atlas: prove'] = time.perf_counter() - t
    t = time.perf_counter()
    trees = {low: ea.same_but_uv(em[low][0], data) for low, data in r.partitions.items()}
    times['atlas: same_but_uv'] = time.perf_counter() - t
    out['atlas:alpha'] = sha(r.alpha)
    out['atlas:normals'] = sha(r.normals)
    want = d.get('want') or {}
    if want:
        got = sorted([hashlib.sha1(r.alpha).hexdigest(), hashlib.sha1(r.normals).hexdigest()])
        print(f'atlas: the atlases {"are" if got == sorted(want.values()) else "are NOT"} the ones Project OPFOR\'s '
              f'run recorded', flush=True)
    out['atlas:report'] = sha('\n'.join(r.report + r.left))
    out['atlas:proof'] = sha(json.dumps(proof, sort_keys=True))
    out['atlas:trees'] = sha(json.dumps(trees, sort_keys=True))
    for k, v in sorted(r.partitions.items()):
        out[f'atlas:{k}'] = sha(v)


def main(argv):
    args = argv[1:]

    def opt(k, d=None):
        if k not in args:
            return d
        i = args.index(k)
        v = args[i + 1]
        del args[i:i + 2]
        return v
    record, compare, prof = opt('--record'), opt('--compare'), opt('--profile')
    top = int(opt('--top', '30'))
    if args[:1] == ['capture']:
        capture_atlas(args[1])
        return 0
    atlas_inputs = None
    if 'atlas' in args:
        i = args.index('atlas')
        atlas_inputs = args[i + 1]
        del args[i + 1]
    what = args or ['source', 'import', 'fair', 'parity']
    out: dict[str, str] = {}
    times: dict[str, float] = {}
    t0 = time.perf_counter()
    src = bi.Source(bi.default_source())
    times['source: open'] = time.perf_counter() - t0
    ix = meshview.Index(shaderdb.find_opfor(None))
    print(f'setup {time.perf_counter() - t0:.1f} s (BF1943 source {times["source: open"]:.1f} s, the game index)',
          flush=True)
    pr = cProfile.Profile() if prof else None
    for w in what:
        t = time.perf_counter()
        if pr:
            pr.enable()
        if w == 'source':
            run_source(src, out, times)
        elif w == 'import':
            run_import(src, out, times)
        elif w == 'fair':
            run_fair(src, ix, out, times)
        elif w == 'parity':
            run_parity(src, ix, out, times)
        elif w == 'atlas':
            run_atlas(atlas_inputs, out, times)
        if pr:
            pr.disable()
        times[w] = time.perf_counter() - t
        n = sum(1 for k in out if k.split(':', 1)[0].startswith(w[:4]) or (w == 'fair' and k.startswith('counter')))
        print(f'{w}: {times[w]:.1f} s ({n} outputs)', flush=True)
    for k, v in times.items():
        if ':' in k:
            print(f'  {k}: {v:.1f} s')
    pk = peak_mb()
    if pk is not None:
        print(f'peak working set {pk:.0f} MB', flush=True)
    if pr:
        pr.dump_stats(prof)
        pstats.Stats(prof).sort_stats('tottime').print_stats(top)
    if record:
        with open(record, 'w') as fh:
            json.dump({'times': times, 'outputs': out}, fh, indent=0, sort_keys=True)
    if compare:
        ref = json.load(open(compare))['outputs']
        prefixes = tuple({k.split(':', 1)[0] for k in out})
        keys = sorted(k for k in set(ref) | set(out) if k.split(':', 1)[0] in prefixes)
        same = sum(ref.get(k) == out.get(k) for k in keys)
        print(f'compare: {same} of {len(keys)} outputs identical')
        for k in [k for k in keys if ref.get(k) != out.get(k)][:20]:
            print(f'  DIFF {k}: {str(ref.get(k))[:120]} / {str(out.get(k))[:120]}')
        return 0 if same == len(keys) else 1
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
