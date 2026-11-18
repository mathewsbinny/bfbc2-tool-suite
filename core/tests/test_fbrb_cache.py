r"""The bundle cache (round E3e, rust/bc2-formats fbrb::cache through bc2_native) against the reader without it and
what the retired Python reader answered (recorded in fbrb_golden.json, round F1a 2026-11-18): the same bytes and the
same errors for every read, in any order, from memory, from a continued session and from the disk cache, in one
process and in several at once, on whole, damaged and changed bundles.

  python core\tests\test_fbrb_cache.py          quick: built bundles (zipped, several 1 MiB chunks; stored), damaged
                                                 ones, a changed bundle, 4 processes sharing a disk cache, read_many on
                                                 one bundle, the controls; 3 game bundles (40 entries each, one at a
                                                 time in a random order) when the game copy is there
  python core\tests\test_fbrb_cache.py --game   + every 3rd bundle of the game copy by size (read only): up to 1,500
                                                 entries each, one to five at a time in a random order, through the
                                                 disk cache or memory, against the reader without the cache; up to
                                                 300 entries of each stored bundle, a few at a time, against the
                                                 reader without the cache reading them all at once

Every cache here is a folder of its own under the system's temporary folder, deleted afterwards. Needs the module.
"""
import hashlib
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, HERE)
import goldens  # noqa: E402
from bfbc2core import fbrb, native, paths  # noqa: E402

CHUNK = 1 << 20
GOLDEN = os.path.join(HERE, 'fbrb_golden.json')


def noise(n: int, seed: int) -> bytes:
    return random.Random(seed).randbytes(n)


def text(n: int, seed: int) -> bytes:
    rng = random.Random(seed)
    words = [b'Levels/', b'Mesh ', b'guid=', b'<instance ', b'0.5 ', b'\n']
    out = bytearray()
    while len(out) < n:
        out += rng.choice(words) + bytes([97 + rng.randrange(26)])
    return bytes(out[:n])


def items() -> list[tuple[str, str, bytes]]:
    """~6 MB of entries (~4 MB zipped: several chunks), a deleted marker, an empty entry, a name listed twice."""
    v = [(f'Levels/Part{i:02}.dbx', '<non-resource>', noise(450_000 + i * 1000, i) if i % 3 == 0 else text(120_000, i))
         for i in range(24)]
    v.insert(5, ('Gone.dbx', fbrb.DELETED, b''))
    v.insert(9, ('Empty.txt', 'Text', b''))
    v.append(('Levels/Part03.dbx', '<non-resource>', b'the later one'))
    return v


NAMES = [f'Levels/Part{i:02}.dbx' for i in range(24)]
PATTERNS: list[list[str]] = [[NAMES[0]], [NAMES[12]], [NAMES[2]], [NAMES[23], NAMES[1]], ['Empty.txt'],
                             [NAMES[7], 'EMPTY.TXT'], [NAMES[3]], [], ['Gone.dbx'], [NAMES[18], 'missing'],
                             NAMES, NAMES[::-1]]


def outcome(fn: Any, path: str, names: list[str]) -> tuple[str, Any]:
    """('ok', {name: sha1}) or (exception type, message) of fn(path, names)."""
    try:
        return 'ok', {k: hashlib.sha1(v).hexdigest() for k, v in fn(path, names).items()}
    except (KeyError, ValueError, OSError) as e:
        return type(e).__name__, str(e)


def configure(folder: str | None, memory: int = 256 << 20, disk_min: int = 0) -> None:
    if folder is None:
        fbrb.cache_configure(no_disk=True, memory=memory, disk_min=disk_min, disk_cap=16 << 30)
    else:
        fbrb.cache_configure(folder=folder, memory=memory, disk_min=disk_min, disk_cap=16 << 30)


SETUPS = [('memory', None, 256 << 20), ('memory, small budget', None, 1 << 20), ('nothing kept', None, 0),
          ('disk', 'disk', 0), ('disk and memory', 'disk', 256 << 20)]


def recorded(o: tuple[str, Any]) -> str:
    """An outcome as recorded from the Python reader: the bytes' digests, or the exception's type alone (the readers
    word their errors differently: zlib's and the Rust core's texts)."""
    return goldens.rh(o, 10) if o[0] == 'ok' else o[0]


def compare(path: str, work: str, label: str, python: list[str] | None = None) -> list[str]:
    """Every pattern twice through each setup against read_uncached (and the recorded Python reader's answers):
    the differences."""
    want = [outcome(fbrb.read_uncached, path, p) for p in PATTERNS]
    bad = []
    if python is not None:
        bad += [f'{label}: {p}: the Python reader differs ({a[0]} / {b})' for p, a, b in zip(PATTERNS, want, python)
                if recorded(a) != b]
    for name, disk, memory in SETUPS:
        folder = os.path.join(work, 'cache') if disk else None
        shutil.rmtree(os.path.join(work, 'cache'), ignore_errors=True)
        configure(folder, memory)
        for _ in range(2):
            for p, w in zip(PATTERNS, want):
                got = outcome(fbrb.read, path, p)
                if got != w:
                    bad.append(f'{label}, {name}: {p}: {got[0]} {str(got[1])[:80]} != {w[0]} {str(w[1])[:80]}')
    return bad


WORKER = r'''
import hashlib, json, os, sys
sys.path.insert(0, {core!r})
from bfbc2core import fbrb
out = []
for path, names in json.loads(sys.argv[1]):
    try:
        out.append({{k: hashlib.sha1(v).hexdigest() for k, v in fbrb.read(path, names).items()}})
    except (KeyError, ValueError, OSError) as e:
        out.append(type(e).__name__)
print(json.dumps([out, fbrb.cache_stats()]))
'''


def processes(work: str, bundles: list[str], n: int) -> tuple[bool, str]:
    """n processes read the same bundles at once through one disk cache: the same results as without it, one whole
    payload per bundle in the cache, no temporary file left."""
    import json
    folder = os.path.join(work, 'shared')
    shutil.rmtree(folder, ignore_errors=True)
    jobs = [(b, p) for _ in range(2) for b in bundles for p in PATTERNS if 'missing' not in p and 'Gone.dbx' not in p]
    want = [{k: hashlib.sha1(v).hexdigest() for k, v in fbrb.read_uncached(b, p).items()} for b, p in jobs]
    env = dict(os.environ, BFBC2_FBRB_CACHE=folder, BFBC2_FBRB_CACHE_MIN_MB='0', BFBC2_FBRB_MEMORY_MB='4')
    script = os.path.join(work, 'worker.py')
    open(script, 'w').write(WORKER.format(core=os.path.join(SUITE, 'core')))
    procs = [subprocess.Popen([sys.executable, script, json.dumps(jobs)], env=env, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, text=True) for _ in range(n)]
    results = [p.communicate() for p in procs]
    errs = [e[-300:] for (o, e), p in zip(results, procs) if p.returncode]
    if errs:
        return False, errs[0]
    got = [json.loads(o) for o, _ in results]
    same = all(r[0] == want for r in got)
    files = os.listdir(folder)
    payloads = [f for f in files if f.endswith('.payload')]
    tmp = [f for f in files if f.endswith('.tmp')]
    written = sum(r[1]['written'] for r in got)
    hits = sum(r[1]['disk_hits'] for r in got)
    ok = same and len(payloads) == len(bundles) and not tmp
    return ok, f'same {same}, {len(payloads)} payloads, {len(tmp)} temporary, {written} written, {hits} disk hits'


def game_sample(files: list[str], work: str, rng: random.Random, per: int, check: Any, label: str) -> None:
    """Up to `per` entries of each bundle, one to five at a time in a random order, through the disk cache (and,
    for a payload under 200 MiB, every other bundle through memory only), against read_uncached of the same entries
    taken in offset order (one pass per ~64 MB of entries)."""
    t = time.time()
    n_bundles = n_entries = n_bytes = n_reads = 0
    bad: list[str] = []
    stats = {'disk_hits': 0, 'memory_hits': 0, 'written': 0}
    for i, path in enumerate(files):
        ents = [e for e in fbrb.entries(path) if not e.deleted]
        by = {e.name.lower(): e for e in ents}
        names = sorted(by)
        pick = rng.sample(names, min(per, len(names)))
        if not pick:
            continue
        want: dict[str, str] = {}
        group: list[str] = []
        size = 0
        order = sorted(pick, key=lambda n: by[n].offset)
        for j, n in enumerate(order):
            group.append(n)
            size += by[n].size
            if size > 64 << 20 or j == len(order) - 1:
                want.update({k: hashlib.sha1(v).hexdigest() for k, v in fbrb.read_uncached(path, group).items()})
                n_bytes += size
                group, size = [], 0
        payload = max(e.offset + e.size for e in ents)
        memory_only = payload < 200 << 20 and i % 2
        shutil.rmtree(os.path.join(work, 'cache'), ignore_errors=True)
        configure(None if memory_only else os.path.join(work, 'cache'), 256 << 20)
        fbrb.cache_stats(reset=True)
        rng.shuffle(pick)
        k = 0
        while k < len(pick):
            step = pick[k:k + rng.randint(1, 5)]
            k += len(step)
            got = {n: hashlib.sha1(v).hexdigest() for n, v in fbrb.read(path, step).items()}
            n_reads += 1
            for n in step:
                if got[n] != want[n]:
                    bad.append(f'{os.path.basename(os.path.dirname(path))}/{os.path.basename(path)}: {by[n].name}')
        for key, v in fbrb.cache_stats().items():
            if key in stats:
                stats[key] += v
        n_bundles += 1
        n_entries += len(pick)
    shutil.rmtree(os.path.join(work, 'cache'), ignore_errors=True)
    check(not bad, f'{label}: {n_bundles} bundles, {n_entries} entries ({n_bytes / 1e9:.2f} GB) in {n_reads} reads in '
          f'a random order: the same bytes through the cache ({stats["disk_hits"]} disk hits, {stats["memory_hits"]} '
          f'memory hits, {stats["written"]} payloads written; {time.time() - t:.0f} s)', '; '.join(bad[:3]))


def stored_sample(files: list[str], rng: random.Random, check: Any) -> None:
    """Up to 300 entries of each stored bundle, a few at a time in a random order (read where they are), against the
    reader without the cache (which streams the payload from its start; its bytes are the retired Python reader's on
    every bundle: test_fbrb_native.py --game) reading them all at once."""
    t = time.time()
    bad, n = [], 0
    for path in files:
        names = sorted(e.name for e in fbrb.entries(path) if not e.deleted)
        pick = rng.sample(names, min(300, len(names)))
        want = {k: hashlib.sha1(v).hexdigest() for k, v in fbrb.read_uncached(path, pick).items()}
        rng.shuffle(pick)
        for k in range(0, len(pick), 4):
            got = fbrb.read(path, pick[k:k + 4])
            bad += [f'{os.path.basename(path)}: {x}' for x, v in got.items() if hashlib.sha1(v).hexdigest() != want[x]]
        n += len(pick)
    check(not bad, f'{len(files)} stored bundles, {n} entries read where they are: the bytes of the reader without '
          f'the cache ({time.time() - t:.0f} s)', '; '.join(bad[:3]))


def built_cases() -> tuple[bytes, bytes, list[bytes], random.Random]:
    """(the built bundle zipped, stored, its damaged copies - cut at and near chunk ends, bits changed -, the seeded
    generator the game samples continue with)."""
    rng = random.Random(3)
    good = fbrb.build(items(), True)
    cases = [good[:c] for c in (CHUNK, CHUNK + 8 + 17, 2 * CHUNK + 40, len(good) - 9, len(good) - 4)]
    for at in sorted(rng.sample(range(200, len(good)), 18)) + [len(good) - 6, len(good) - 2]:
        b = bytearray(good)
        b[at] ^= 1 << rng.randrange(8)
        cases.append(bytes(b))
    return good, fbrb.build(items(), False), cases, rng


def main(argv: list[str]) -> int:
    native.require('the bundle cache', 'fbrb_cache_stats')
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    saved = fbrb.cache_config()
    work = tempfile.mkdtemp(prefix='fbrb_cache_test_')
    rec = goldens.load(GOLDEN)['cache']
    try:
        zipped = os.path.join(work, 'zipped.fbrb')
        stored = os.path.join(work, 'stored.fbrb')
        good, stored_data, cases, rng = built_cases()
        open(zipped, 'wb').write(good)
        open(stored, 'wb').write(stored_data)
        check(len(good) > 3 * CHUNK, f'the built bundle spans {len(good) / CHUNK:.1f} chunks of compressed payload')
        bad = compare(zipped, work, 'zipped', rec['zipped']) + compare(stored, work, 'stored', rec['stored'])
        check(not bad, f'built bundles, zipped and stored: {len(PATTERNS)} read patterns twice through {len(SETUPS)} '
              'setups (memory, a 1 MiB budget, nothing kept, disk, disk and memory): the bytes and errors of the '
              'reader without the cache and of the Python reader (recorded)', '; '.join(bad[:3]))

        # damaged: chunk boundaries and flips; the cut and changed bundles through every setup
        bad, failures = [], 0
        for i, case in enumerate(cases):
            p = os.path.join(work, f'damaged{i}.fbrb')
            open(p, 'wb').write(case)
            failures += sum(outcome(fbrb.read_uncached, p, x)[0] != 'ok' for x in PATTERNS)
            bad += compare(p, work, f'damaged {i}', rec['damaged'].get(str(i)))
        check(not bad and failures > 40, f'{len(cases)} damaged bundles (cut at and near chunk ends, bits changed): the '
              f'same bytes and errors through every setup ({failures} of {len(cases) * len(PATTERNS)} reads refused)',
              '; '.join(bad[:3]))

        # a changed bundle: the same path, other bytes, a new time
        folder = os.path.join(work, 'cache')
        shutil.rmtree(folder, ignore_errors=True)
        configure(folder, 256 << 20)
        fbrb.cache_stats(reset=True)
        for _ in range(3):
            fbrb.read(zipped, [NAMES[20]])
        before = fbrb.cache_stats()
        changed = [(n, t, bytes((x + 1) & 255 for x in d) if n == NAMES[20] else d) for n, t, d in items()]
        open(zipped, 'wb').write(fbrb.build(changed, True))
        os.utime(zipped, (time.time() + 5, time.time() + 5))
        want = fbrb.read_uncached(zipped, [NAMES[20]])
        got = [fbrb.read(zipped, [NAMES[20]]) for _ in range(3)]
        check(before['written'] == 1 and all(g == want for g in got) and want[NAMES[20]] != dict(
            (n, d) for n, _, d in items())[NAMES[20]],
              'a bundle rewritten in place (new bytes, new time) is read again, never answered from the cache')
        open(zipped, 'wb').write(good)

        # several processes at once through one disk cache
        others = []
        for k in range(2):
            p = os.path.join(work, f'other{k}.fbrb')
            open(p, 'wb').write(fbrb.build([(n, t, d[::-1] if k else d[1:]) for n, t, d in items()], True))
            others.append(p)
        ok, detail = processes(work, [zipped] + others, 4)
        check(ok, '4 processes at once through one disk cache: the same bytes, one whole payload per bundle, no '
              'temporary file left', detail)
        print(f'       ({detail})')

        # read_many: the same bundle several times in one batch (threads side by side on one bundle)
        configure(os.path.join(work, 'many'), 256 << 20)
        jobs = [(zipped, p) for p in PATTERNS * 3]
        many = fbrb.read_many(jobs)
        ref = [outcome(fbrb.read_uncached, zipped, p) for p in PATTERNS * 3]
        conv = [('ok', {k: hashlib.sha1(v).hexdigest() for k, v in r.items()}) if not isinstance(r, Exception)
                else (type(r).__name__, str(r)) for r in many]
        check(conv == ref, f'read_many: {len(jobs)} reads of one bundle side by side, the same results')

        # the controls
        configure(None, 1 << 20)
        c = fbrb.cache_config() or {}
        fbrb.cache_configure(env=True)
        env = fbrb.cache_config() or {}
        check(c.get('folder') is None and c.get('memory') == 1 << 20 and set(fbrb.cache_stats()) >= {
            'reads', 'disk_hits', 'memory_hits', 'decompressed_bytes', 'written'}
              and env.get('memory') == int(float(os.environ.get('BFBC2_FBRB_MEMORY_MB', 128)) * (1 << 20)),
              'cache_config / cache_configure / cache_stats', f'{c} {env}')
        fbrb.cache_clear()

        try:
            root: str | None = paths.find_opfor()
        except paths.PathsError:
            root = None
        if root is None:
            print('skipped: the game bundles (no Project OPFOR BFBC2 game copy)')
        else:
            game = paths.game_dir(root)
            files = sorted((os.path.join(d, n) for d, _, ns in os.walk(game) for n in ns
                            if n.lower().endswith('.fbrb')), key=os.path.getsize)
            zipped_files: list[str] = []
            stored_files: list[str] = []
            header = native.require('the bundle reader').fbrb_header
            for f in files:
                (stored_files, zipped_files)[bool(header(f, fbrb.Entry)[1])].append(f)
            if '--game' in argv:
                game_sample(zipped_files[::3], work, rng, 1500, check, 'every 3rd zipped game bundle by size')
                stored_sample(stored_files, rng, check)
            else:
                mid = [f for f in zipped_files if 8 << 20 < os.path.getsize(f) < 60 << 20]
                game_sample(mid[:: max(1, len(mid) // 3)][:3], work, rng, 40, check, 'game sample')
    finally:
        if saved is not None:
            fbrb.cache_configure(env=True)
        shutil.rmtree(work, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
