r"""Times the BF1943 model / physics / effect ports in bulk (rust/BENCHMARKS.md, "E3g"): every mesh set, every
HavokPhysicsData and every effect of the user's BF1943 copy (bf1943import.default_source()), ported as Project OPFOR's
WW2 steps port them (bf1943_objects: OPFOR's mesh donors and Havok templates, the hkUFloat8 table; bf1943_fx /
material_grid: effectport.port_many on Wake's emitter copies), each output's SHA-256 recorded.

  python rust\bench_ports.py [mesh] [havok] [effects] [--backend rust|python] [--every N] [--no-cache]
                             [--record out.json] [--compare in.json] [--profile out.prof] [--top N]

  mesh      meshport.Porter.port of all 273 mesh sets (Rigid / Composite / Skinned / Tree), OPFOR's MESH_DONORS
  havok     havokport.convert of all 202 HavokPhysicsData (OPFOR's two templates, the first that names the classes)
  effects   effectport.port_many of every effects/ EffectAsset BF1943 has (157 partitions), on Wake's copies
  (none: all three)

  --backend  rust (default): E3g's indexes (meshport's layout search, effectport's emitter templates); python: their
             Python reference searches. The mesh vertex loop and the Havok walk run in the Rust core either way
             (their Python was retired on 2026-11-18: BENCHMARKS.md, "Retired Python")
  --no-cache effectport's emitter index not read from or written to disk (BFBC2_EFFECTPORT_CACHE=0): a cold run
  --every N  every N-th item only (a quick look)
  --record   the outputs' hashes (and errors) as JSON; --compare: checked against such a file (a run of the other
             backend): prints the counts and exits 1 on a difference
  --profile  under cProfile, the stats saved and the N (default 30) functions with the most own time printed

The game copy and BF1943's copy are read only; nothing is written but --record / --profile's files.
"""
import cProfile
import hashlib
import json
import os
import pstats
import re
import sys
import time

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'mesh-viewer', 'shader-tools', 'console-convert', 'anim-tools', 'core')
import bf1943import as bi  # noqa: E402
import effectport  # noqa: E402
import havok  # noqa: E402
import havokport  # noqa: E402
import meshport  # noqa: E402
import meshview  # noqa: E402
import shaderdb  # noqa: E402
from bfbc2core import dbx  # noqa: E402

# Project OPFOR's bf1943_objects.py (read only): the donors and templates its object port uses
MESH_DONORS = ['objects/objects/boat_house/boat_house_mesh',
               'objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01_mesh',
               'objects/objects/_sp01/destroyer/destroyermesh_mesh']
HAVOK_TEMPLATES = ['objects/objects/boat_house/boat_house_havok.res',
                   'objects/levelspecific_objects/ghosttown_lamppolelong/ghosttown_lamppolelong_banger_havok.res']
LEVEL = 'levels/wake_island_s'
MESH_KINDS = ('RigidMeshSet', 'CompositeMeshSet', 'SkinnedMeshSet', 'TreeMeshSet')


def sha(b):
    return hashlib.sha256(b).hexdigest()


def mesh_names(src):
    return [e['name'][:-4] for k, (_, e) in sorted(src.entries.items()) if e['type'] in MESH_KINDS and
            e['name'].lower().endswith('.res')]


def havok_names(src):
    return [k for k, (_, e) in sorted(src.entries.items()) if e['type'] == 'HavokPhysicsData']


def effect_names(src):
    return [k[:-4] for k in sorted(src.entries) if k.startswith('effects/') and k.endswith('.dbx') and
            b'Entity.EffectAsset' in src.data(k)]


def run_mesh(src, ix, names, out):
    donors = [re.findall(r'<field name="Name">([^<]+)</field>', dbx.to_xml(ix.read(d + '.dbx')))[0]
              for d in MESH_DONORS]
    p = meshport.Porter(src, ix)
    for n in names:
        try:
            r = p.port(n, donors)
        except Exception as e:                                   # noqa: BLE001 - recorded: both backends must agree
            out[f'mesh:{n}'] = f'error {type(e).__name__}: {e}'
            continue
        h = hashlib.sha256()
        for b in r['lods']:
            h.update(sha(b).encode())
        h.update(sha(r['meshset']).encode())
        for k in sorted(r['textures']):
            h.update(k.encode() + sha(r['textures'][k]).encode())
        for k in sorted(r['poses']):
            h.update(k.encode() + sha(r['poses'][k]).encode())
        h.update(json.dumps(r['plan'], sort_keys=True, default=str).encode())
        h.update(json.dumps(r['notes'], sort_keys=True).encode())
        out[f'mesh:{n}'] = h.hexdigest()


def run_havok(src, ix, names, out, exe):
    K = havok.classes(exe)
    table = havokport.ufloat8_table(open(exe, 'rb').read())
    templates = [ix.read(t) for t in HAVOK_TEMPLATES]
    for k in names:
        for i, t in enumerate(templates):
            try:
                out[f'havok:{k}'] = sha(havokport.convert(src.data(k), t, K, table))
                break
            except havokport.PortError as e:
                if i == len(templates) - 1 or 'class names lack' not in str(e):
                    out[f'havok:{k}'] = f'error PortError: {e}'
                    break
            except Exception as e:                               # noqa: BLE001
                out[f'havok:{k}'] = f'error {type(e).__name__}: {e}'
                break


def run_effects(src, ix, names, out):
    import dbxcatalogue
    cat = dbxcatalogue.load(ix)
    files, rep = effectport.port_many(src, ix, names, LEVEL, log=lambda *a: None, catalogue=cat)
    for k in sorted(files):
        out[f'effect:{k}'] = sha(files[k])
    out['effect:report'] = sha('\n'.join(rep).encode())


def peak_mb():
    """This process's peak working set in MB (Windows; None elsewhere)."""
    try:
        import ctypes
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] +                 [(n, ctypes.c_size_t) for n in ('PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage',
                                                'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
                                                'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage')]
        c = Counters()
        c.cb = ctypes.sizeof(c)
        psapi = ctypes.WinDLL('psapi')
        kernel = ctypes.WinDLL('kernel32')
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD]
        if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(c), c.cb):
            return None
        return c.PeakWorkingSetSize / 2 ** 20
    except (OSError, AttributeError, ctypes.ArgumentError):
        return None


def main(argv):
    args = argv[1:]

    def opt(k, d=None):
        if k not in args:
            return d
        i = args.index(k)
        v = args[i + 1]
        del args[i:i + 2]
        return v
    backend = opt('--backend', 'rust')
    every = int(opt('--every', '1'))
    record, compare, prof = opt('--record'), opt('--compare'), opt('--profile')
    top = int(opt('--top', '30'))
    what = args or ['mesh', 'havok', 'effects']
    native = backend == 'rust'
    # the reference search E3g replaced by an index (meshport's layouts; effectport's is the Rust core's alone since
    # round OR8)
    meshport.REFERENCE = not native
    if '--no-cache' in args:
        args.remove('--no-cache')
        os.environ['BFBC2_EFFECTPORT_CACHE'] = '0'
    root = shaderdb.find_opfor(None)
    exe = os.path.join(root, 'game', 'BFBC2Game.exe')
    t0 = time.perf_counter()
    src = bi.Source(bi.default_source())
    ix = meshview.Index(root)
    print(f'setup {time.perf_counter() - t0:.1f} s (BF1943 source, the game index)', flush=True)
    out: dict[str, str] = {}
    pr = cProfile.Profile() if prof else None
    times = {}
    for w in what:
        names = {'mesh': mesh_names, 'havok': havok_names, 'effects': effect_names}[w](src)[::every]
        t = time.perf_counter()
        if pr:
            pr.enable()
        if w == 'mesh':
            run_mesh(src, ix, names, out)
        elif w == 'havok':
            run_havok(src, ix, names, out, exe)
        else:
            run_effects(src, ix, names, out)
        if pr:
            pr.disable()
        times[w] = time.perf_counter() - t
        errs = sum(1 for k, v in out.items() if k.startswith(w.rstrip('s') + ':') and v.startswith('error'))
        print(f'{w}: {len(names)} items, {times[w]:.1f} s ({errs} errors)', flush=True)
    pk = peak_mb()
    if pk is not None:
        print(f'peak working set {pk:.0f} MB', flush=True)
    if pr:
        pr.dump_stats(prof)
        pstats.Stats(prof).sort_stats('tottime').print_stats(top)
    if record:
        with open(record, 'w') as fh:
            json.dump({'backend': backend, 'times': times, 'outputs': out}, fh, indent=0, sort_keys=True)
    if compare:
        ref = json.load(open(compare))['outputs']
        keys = sorted(set(ref) | set(out))
        keys = [k for k in keys if any(k.startswith(w.rstrip('s') + ':') for w in what)]
        same = sum(ref.get(k) == out.get(k) for k in keys)
        print(f'compare: {same} of {len(keys)} outputs identical')
        for k in [k for k in keys if ref.get(k) != out.get(k)][:20]:
            print(f'  DIFF {k}: {ref.get(k)} / {out.get(k)}')
        return 0 if same == len(keys) else 1
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
