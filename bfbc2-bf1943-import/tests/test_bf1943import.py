r"""bf1943import.py, the Battlefield 1943 import pipeline.

Without game data (quick tier):
- an output name that would leave the output folder is refused; the containment test is case- and
  separator-blind and does not take a sibling folder with a common prefix for a subfolder;
- each resource type is routed to its converter, or listed with the reason it is not converted;
- an import into a folder inside the source, or with no filter, or with a filter that matches nothing, is refused
  before anything is written;
- same_clean (check()'s tree comparison, round E3r) answers 6,000 seeded random tree pairs as clean() and == do;
- Source (round E3r, lazy) against WholeSource on built bundles: every entry in a random order through restart points
  4 KiB apart and almost no span budget, a two-member gzip payload, a stored payload, a refused bundle, the on-disk
  index used and refreshed, a bundle changed after the open refused (needs the BF1943 suite's bundle reader).

With --game (game tier; skipped without the Rising Sun workspace and the BF1943 suite next to this one):
- check: every BF1943 animation, delta animation and skeleton converts, reads as a PC file with the source's object
  tree, and every renumbered bone table names the same bones in BFBC2's soldier; every tree comparison it makes is
  answered by same_clean as by the reference;
- Source on the user's copy: the same bundles, names and bytes for every entry as WholeSource reads them;
- import of the Garand bayonet's resources into a temporary folder: the animations land under their names and read
  as little-endian with the bayonet bone (jntWpn_6) at the muzzle in the 1P thrust; import.json records each
  resource (converted or why not); a second import with another filter adds to the records instead of replacing
  them; the source bundles are unchanged;
- 3p undeploys: the 22 BFBC2 shipped get DICE's 25 added tracks (88 in all, as DICE's) without a track of theirs
  changing; the 19 weapon parts reproduce DICE's exactly in at least 12 of the 22 and 380 of the 418 tracks (the
  rest DICE re-animated: M18, expack, detonator, the Shin Gunto standing, the crouching Garand's clip), the thumbs
  within 15 degrees on average (DICE's are animated); the 14 BF1943-only undeploys get their 25; thumb_donors: the
  Garand's thumbs from Vietnam's Garand undeploys are DICE's, the K98's from the M24's still give 88 tracks.

  python test_bf1943import.py [--game]
"""
import hashlib
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import')
import bf1943import as bi  # noqa: E402


class FakeSource:
    def __init__(self, folder):
        self.folder = folder
        self.entries = {'a/b.res': ('x.fbrb', {'name': 'A/B.res', 'type': 'GrannyAnimation', 'size': 1})}

    def find(self, filters):
        return [k for k in self.entries if all(f.lower() in k for f in filters)]


def units(check):
    out = tempfile.mkdtemp(prefix='bf1943import_')
    try:
        check(bi.target(out, 'Animations/X/1P_Melee1.res').startswith(os.path.abspath(out)),
              'target: a resource name lands inside the output folder')
        for bad in ('../escape.res', 'a/../../escape.res', '..\\escape.res'):
            try:
                bi.target(out, bad)
                check(False, f'target: {bad} is refused')
            except bi.Bf1943ImportError:
                check(True, f'target: {bad} is refused')
        check(bi._inside(os.path.join(out, 'Sub', 'f'), out.upper()), '_inside: case-blind')
        check(not bi._inside(out + '_sibling', out), '_inside: a sibling with a common prefix is not inside')
        routes = {'GrannyAnimation': 'animation to PC', 'DeltaAnimation': 'animation to PC',
                  'GrannyModel': 'skeleton to PC', 'XenonTexture': 'texture to PC',
                  'MeshData': 'not converted', 'HavokPhysicsData': 'not converted', '<non-resource>': 'not converted',
                  'AimAnimation': 'aim animation to PC', 'Wave': 'not converted'}
        check(all(bi.plan(k).startswith(v) for k, v in routes.items()), 'plan: every type routed or explained')
        src = FakeSource(os.path.join(out, 'source'))
        for args, what in ((([], os.path.join(out, 'o')), 'no filter'),
                           ((['nothing-matches'], os.path.join(out, 'o')), 'a filter that matches nothing'),
                           ((['a/b'], os.path.join(src.folder, 'inside')), 'an output folder inside the source')):
            try:
                bi.run_import(src, args[0], args[1], log=lambda s: None)
                check(False, f'run_import: {what} is refused')
            except bi.Bf1943ImportError:
                check(True, f'run_import: {what} is refused')
        check(not os.path.exists(os.path.join(out, 'o')) and not os.path.exists(src.folder),
              'run_import: nothing written by a refused import')
    finally:
        shutil.rmtree(out, ignore_errors=True)


def _bundle(entries, compressed=True, members=1):
    """A BF1943 bundle (fbrb1943's layout): [(name, type, bytes)], its payload gzip (in `members` gzip members) or
    stored."""
    import gzip
    import struct
    strings, offs = bytearray(), {}
    for n, t, _ in entries:
        for s in (n, t):
            if s not in offs:
                offs[s] = len(strings)
                strings += s.encode() + b'\0'
    payload = b''.join(d for _, _, d in entries)
    d = bytearray(struct.pack('>II', 2, len(strings)) + strings + struct.pack('>I', len(entries)))
    pos = 0
    for n, t, data in entries:
        d += struct.pack('>6I', offs[n], 0, pos, len(data), len(data), offs[t])
        pos += len(data)
    d += bytes([1 if compressed else 0]) + struct.pack('>I', len(payload))
    gd = gzip.compress(bytes(d), mtime=0)
    if not compressed:
        body = payload
    elif members == 1:
        body = gzip.compress(payload, compresslevel=6, mtime=0)
    else:
        cut = len(payload) // members
        body = b''.join(gzip.compress(payload[i * cut:(i + 1) * cut if i < members - 1 else None], mtime=0)
                        for i in range(members))
    return b'FbRB' + struct.pack('>I', len(gd)) + gd + body


def lazy_source(check):
    """Source's reading by its Python reference (lazy_source_python) and by the suite's Rust reader
    (lazy_source_rust, bc2_formats::bf1943), each against WholeSource."""
    from bfbc2core import native
    was = native.use_backend('bf1943source', False)
    try:
        lazy_source_python(check)
    finally:
        native.use_backend('bf1943source', None)
    if native.use_backend('bf1943source', True):
        try:
            lazy_source_rust(check)
        finally:
            native.use_backend('bf1943source', None)
    elif was:
        check(False, 'lazy source: the Rust reader is built but could not be turned on')
    else:
        print('SKIP lazy source (Rust): bc2_native has no bf1943_read (python rust\build_native.py)')


def lazy_source_rust(check):
    """Source on the Rust reader (round PR1b-1-rest) against WholeSource on built bundles: a payload of several
    MiB (restart points 1 MiB apart, most inside a byte) read in a random order, a two-member payload (read whole), a
    stored one, a refused one; the points made once per process, kept on disk by the payload's content and read back
    by a later process (RESTARTS cleared), none kept with BF1943_RESTARTS_MB=0; four Sources on four threads; a
    bundle changed after the open refused."""
    import random
    import threading
    try:
        bi.fbrb1943()
    except bi.Bf1943ImportError as e:
        print(f'SKIP lazy source (Rust): {e}')
        return
    rng = random.Random(2043)
    tmp = tempfile.mkdtemp(prefix='bf1943rs_')
    old = os.environ.get(bi.INDEX_ENV)
    letters = bytes.maketrans(bytes(range(256)), bytes(b'abcdefghijklmnopq'[i % 17] for i in range(256)))
    try:
        folder = os.path.join(tmp, 'xenon')
        os.makedirs(os.path.join(folder, 'levels'))
        files = {
            'levels/huge-00.fbrb': _bundle([(f'Huge/E{i}.res', 'Mesh', rng.randbytes(rng.choice((0, 9, 70000, 200000)))
                                             .translate(letters)) for i in range(60)]),
            'aa-00.fbrb': _bundle([(f'Two/E{i}.res', 'Mesh', rng.randbytes(3000)) for i in range(20)], members=2),
            'stored-00.fbrb': _bundle([(f'S/E{i}.res', 'Wave', rng.randbytes(rng.randrange(0, 4000))) for i in range(30)],
                                      compressed=False),
            'bad-00.fbrb': _bundle([('Bad/E.res', 'Mesh', b'abc')])[:-3],
        }
        for rel, raw in files.items():
            with open(os.path.join(folder, *rel.split('/')), 'wb') as fh:
                fh.write(raw)
        os.environ[bi.INDEX_ENV] = os.path.join(tmp, 'index')
        bi.RESTARTS.clear()
        ref = bi.WholeSource(folder)
        src = bi.Source(folder)
        check(src.entries == ref.entries and list(src.bundles) == list(ref.bundles) and
              'bad-00.fbrb' not in src.bundles and src.stats['checked'] == 4,
              'lazy source (Rust): the same entries and bundles as WholeSource (the damaged bundle refused by both)')
        keys = sorted(ref.entries) * 2
        rng.shuffle(keys)
        same = sum(src.data(k) == ref.data(k) for k in keys)
        check(same == len(keys) and src.stats['indexed'] == 2 and src.stats['spans'] >= 4 and
              src.stats['whole'] > 0 and src.stats['stored_reads'] > 0,
              f'lazy source (Rust): {same} of {len(keys)} reads in a random order the WholeSource bytes (points made '
              f'{src.stats["indexed"]}, spans {src.stats["spans"]}, hits {src.stats["hits"]}, whole '
              f'{src.stats["whole"]}, stored {src.stats["stored_reads"]})')
        points = [f for f in os.listdir(os.path.join(tmp, 'index')) if f.endswith('.points')]
        check(len(points) == 1, f'lazy source (Rust): one points file on disk (the one-member payload): {points}')
        again = bi.Source(folder)
        ok = all(again.data(k) == ref.data(k) for k in ref.entries)
        check(ok and again.stats['from_index'] == 4 and again.stats['indexed'] == 0 and
              again.stats['shared_points'] > 0,
              'lazy source (Rust): a second Source reuses the process points, the same bytes')
        bi.RESTARTS.clear()                             # a later process: the points from disk
        later = bi.Source(folder)
        ok = all(later.data(k) == ref.data(k) for k in ref.entries)
        check(ok and later.stats['from_disk'] == 1 and later.stats['indexed'] == 1,
              f'lazy source (Rust): a later process reads the points from disk (from disk {later.stats["from_disk"]}, '
              f'made {later.stats["indexed"]}: the two-member payload, read whole)')
        old_mb = os.environ.get(bi.RESTARTS_ENV)
        try:
            os.environ[bi.RESTARTS_ENV] = '0'
            bi.RESTARTS.clear()
            off = bi.Source(folder)
            ok = all(off.data(k) == ref.data(k) for k in ref.entries)
            check(ok and bi.RESTARTS.held() == (0, 0),
                  'lazy source (Rust): BF1943_RESTARTS_MB=0 keeps no points in the process')
        finally:
            if old_mb is None:
                os.environ.pop(bi.RESTARTS_ENV, None)
            else:
                os.environ[bi.RESTARTS_ENV] = old_mb
            bi.RESTARTS.clear()
        shared = [bi.Source(folder) for _ in range(4)]
        errors: list[str] = []

        def read_all(s, order):
            for k in order:
                if s.data(k) != ref.data(k):
                    errors.append(k)
        threads = [threading.Thread(target=read_all, args=(s, rng.sample(sorted(ref.entries), len(ref.entries))))
                   for s in shared]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        check(not errors, f'lazy source (Rust): four Sources reading at once on four threads get the WholeSource '
              f'bytes ({len(errors)} wrong)')
        again = bi.Source(folder)
        huge = os.path.join(folder, 'levels', 'huge-00.fbrb')
        raw = bytearray(files['levels/huge-00.fbrb'])
        raw[-4000] ^= 0xFF                                       # inside the payload's deflate data
        k = max((k for k in ref.entries if k.startswith('huge/') and ref.entries[k][1]['size'] > 4000),
                key=lambda k: ref.entries[k][1]['offset'])
        st = os.stat(huge)
        with open(huge, 'wb') as fh:
            fh.write(raw)
        os.utime(huge, ns=(st.st_atime_ns, st.st_mtime_ns))      # the same size and time: only the span's CRC-32 sees it
        bi.RESTARTS.clear()                             # (spans decoded before the change are answered as read)
        try:
            again.data(k)
            check(False, 'lazy source (Rust): a bundle changed in place (same size and time) is refused')
        except bi.Bf1943ImportError:
            check(True, 'lazy source (Rust): a bundle changed in place (same size and time) is refused')
    finally:
        bi.RESTARTS.clear()
        if old is None:
            os.environ.pop(bi.INDEX_ENV, None)
        else:
            os.environ[bi.INDEX_ENV] = old
        shutil.rmtree(tmp, ignore_errors=True)


def lazy_source_python(check):
    """Source (round E3r: entries checked once and indexed on disk, data read through restart points) against
    WholeSource on built bundles: every entry in a random order, through restart points 4 KiB apart and a span budget
    that keeps almost nothing; a bundle of two gzip members (read whole each time), a stored one, one fbrb1943
    refuses; the index written, used and refreshed; a bundle changed after the open refused, never read."""
    import random
    try:
        bi.fbrb1943()
    except bi.Bf1943ImportError as e:
        print(f'SKIP lazy source: {e}')
        return
    rng = random.Random(1943)
    tmp = tempfile.mkdtemp(prefix='bf1943src_')
    old = (bi.SPAN, bi.CHUNK, os.environ.get(bi.INDEX_ENV))
    try:
        folder = os.path.join(tmp, 'xenon')
        os.makedirs(os.path.join(folder, 'levels', 'a'))

        def blob(n):
            return bytes(rng.getrandbits(8) for _ in range(n)) if rng.random() < 0.3 else \
                bytes(rng.choice(b'abcdefgh') for _ in range(n))
        files = {
            'levels/a/big-00.fbrb': _bundle([(f'Big/E{i}.dbx', '<non-resource>', blob(rng.choice((0, 1, 7, 300, 5000,
                                                                                                    20000))))
                                             for i in range(80)] + [('Shared/X.res', 'Mesh', b'first copy')]),
            'aa-00.fbrb': _bundle([('Shared/X.res', 'Mesh', b'xx'), ('Shared/x.RES', 'Mesh', b'yy')] +
                                  [(f'Two/E{i}.res', 'Mesh', blob(3000)) for i in range(20)], members=2),
            'stored-00.fbrb': _bundle([(f'S/E{i}.res', 'Wave', blob(rng.randrange(0, 4000))) for i in range(30)],
                                      compressed=False),
            'bad-00.fbrb': _bundle([('Bad/E.res', 'Mesh', b'abc')])[:-3],
        }
        for rel, raw in files.items():
            with open(os.path.join(folder, *rel.split('/')), 'wb') as fh:
                fh.write(raw)
        os.environ[bi.INDEX_ENV] = os.path.join(tmp, 'index')
        bi.SPAN, bi.CHUNK = 4096, 512
        ref = bi.WholeSource(folder)
        src = bi.Source(folder, memory_mb=0.01)
        check(src.entries == ref.entries and list(src.bundles) == list(ref.bundles) and
              'bad-00.fbrb' not in src.bundles and src.stats['checked'] == 4,
              'lazy source: the same entries and bundles as WholeSource (the damaged bundle refused by both)')
        keys = sorted(ref.entries) * 3
        rng.shuffle(keys)
        same = sum(src.data(k) == ref.data(k) for k in keys)
        check(same == len(keys) and src.stats['spans'] > 50 and src.stats['whole'] > 0 and
              src.stats['stored_reads'] > 0,
              f'lazy source: {same} of {len(keys)} reads in a random order WholeSource\'s bytes '
              f'(spans {src.stats["spans"]}, whole {src.stats["whole"]}, stored {src.stats["stored_reads"]})')
        again = bi.Source(folder)
        check(again.stats['from_index'] == 4 and again.stats['checked'] == 0 and again.entries == ref.entries and
              all(again.data(k) == ref.data(k) for k in ref.entries),
              'lazy source: a second open takes every bundle from the index, the same entries and bytes')
        # round S1: the restart points are the process's - the second Source decompressed no payload whole
        check(again.stats['indexed'] == 0 and again.stats['shared_points'] == 2 and src.stats['indexed'] == 2,
              f'lazy source: a second Source reuses the process\'s restart points (made {src.stats["indexed"]} times '
              f'by the first, {again.stats["indexed"]} by the second; shared {again.stats["shared_points"]})')
        old_mb = os.environ.get(bi.RESTARTS_ENV)
        try:
            os.environ[bi.RESTARTS_ENV] = '0'
            bi.RESTARTS.clear()
            off = bi.Source(folder)
            ok = all(off.data(k) == ref.data(k) for k in ref.entries)
            check(ok and off.stats['indexed'] == 2 and bi.RESTARTS.held() == (0, 0),
                  'lazy source: BF1943_RESTARTS_MB=0 keeps no points for the process (each Source makes its own)')
            os.environ[bi.RESTARTS_ENV] = str(1.5 * bi.POINT_BYTES / 2 ** 20)    # room for about one point
            bi.RESTARTS.clear()
            small = bi.Source(folder)
            ok = all(small.data(k) == ref.data(k) for k in ref.entries)
            n, held = bi.RESTARTS.held()
            check(ok and n <= 2 and held <= bi.RESTARTS.budget() + 1024,
                  f'lazy source: the process\'s points kept within BF1943_RESTARTS_MB ({n} entries, {held} bytes)')
        finally:
            if old_mb is None:
                os.environ.pop(bi.RESTARTS_ENV, None)
            else:
                os.environ[bi.RESTARTS_ENV] = old_mb
            bi.RESTARTS.clear()
        import threading
        shared = [bi.Source(folder) for _ in range(4)]
        errors: list[str] = []

        def read_all(s, order):
            for k in order:
                if s.data(k) != ref.data(k):
                    errors.append(k)
        threads = [threading.Thread(target=read_all, args=(s, rng.sample(sorted(ref.entries), len(ref.entries))))
                   for s in shared]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        check(not errors and sum(s.stats['indexed'] for s in shared) >= 2,
              f'lazy source: four Sources reading at once on four threads get WholeSource\'s bytes ({len(errors)} wrong)')
        again = bi.Source(folder)                       # opened, nothing read yet (what is read is kept as read)
        big = os.path.join(folder, 'levels', 'a', 'big-00.fbrb')
        raw = bytearray(files['levels/a/big-00.fbrb'])
        raw[-40] ^= 0xFF                                         # inside the payload's deflate data
        k = next(k for k in ref.entries if k.startswith('big/'))
        st = os.stat(big)
        with open(big, 'wb') as fh:
            fh.write(raw)
        os.utime(big, ns=(st.st_atime_ns, st.st_mtime_ns + 10 ** 9))
        try:
            again.data(k)
            check(False, 'lazy source: a bundle changed after the open is refused')
        except bi.Bf1943ImportError:
            check(True, 'lazy source: a bundle changed after the open is refused')
        third = bi.Source(folder)
        ref2 = bi.WholeSource(folder)
        check(third.stats['checked'] == 1 and third.entries == ref2.entries and list(third.bundles) == list(ref2.bundles),
              'lazy source: the changed bundle checked again on the next open, as WholeSource reads it')
    finally:
        bi.SPAN, bi.CHUNK = old[0], old[1]
        if old[2] is None:
            os.environ.pop(bi.INDEX_ENV, None)
        else:
            os.environ[bi.INDEX_ENV] = old[2]
        shutil.rmtree(tmp, ignore_errors=True)


def same_clean_fuzz(check):
    """same_clean against the reference (clean(a) == clean(b)) on seeded random trees and changed copies."""
    import copy
    import random
    rng = random.Random(7)
    nan = float('nan')

    def tree(d):
        r = rng.random()
        if d > 3 or r < 0.35:
            return rng.choice([0, 1, 2.5, -0.0, 0.0, nan, 'a', 'b', None, True, (1, 2), b'x', [], {}])
        if r < 0.65:
            return [tree(d + 1) for _ in range(rng.randrange(0, 4))]
        keys = ['A', 'B', 'C', '__Ptr', '__Type', 'Padding', 'PaddingX']
        return {k: tree(d + 1) for k in rng.sample(keys, rng.randrange(0, len(keys) + 1))}

    def mutate(t):
        if isinstance(t, dict) and t:
            k = rng.choice(list(t))
            if rng.random() < 0.3:
                del t[k]
            else:
                t[k] = mutate(t[k]) if isinstance(t[k], (dict, list)) and rng.random() < 0.7 else tree(2)
        elif isinstance(t, list) and t:
            i = rng.randrange(len(t))
            t[i] = mutate(t[i]) if isinstance(t[i], (dict, list)) and rng.random() < 0.7 else tree(2)
        return t
    n = agree = differ = 0
    for _ in range(6000):
        a = tree(0)
        b = copy.deepcopy(a) if rng.random() < 0.8 else tree(0)
        if rng.random() < 0.7:
            b = mutate(b)
        n += 1
        want = bi.clean(a) == bi.clean(b)
        agree += bi.same_clean(a, b) == want
        differ += not want
    check(agree == n and 0 < differ < n, f'same_clean: {agree} of {n} random tree pairs as clean() compares them '
          f'({differ} unequal)')


def lazy_game(check, source):
    """Source against WholeSource's reading on the user's BF1943 copy, one bundle read whole at a time: the same
    bundles and entries, every entry's bytes."""
    import time
    fb = bi.fbrb1943()
    entries: dict[str, tuple] = {}
    bundles: list[str] = []
    same = n = 0
    t = time.perf_counter()
    by: dict[str, list[str]] = {}
    for k, (r, _e) in source.entries.items():
        by.setdefault(r, []).append(k)
    for rel, path in bi._bundle_files(source.folder):
        try:
            bd = fb.Bundle(open(path, 'rb').read())
        except ValueError:
            continue
        bundles.append(rel)
        for e in bd.entries:
            entries.setdefault(e['name'].lower(), (rel, e))
        for k in by.get(rel, ()):
            n += 1
            same += source.data(k) == bd.data(source.entries[k][1]['name'])
        del bd
    check(entries == source.entries and bundles == list(source.bundles),
          f'lazy source: the {len(bundles)} bundles and {len(entries)} names WholeSource takes')
    check(same == n == len(entries), f'lazy source: {same} of {n} entries WholeSource\'s bytes '
          f'({time.perf_counter() - t:.0f} s)')


def game(check):
    folder = bi.default_source()
    try:
        source = bi.Source(folder)
    except bi.Bf1943ImportError as e:
        print(f'SKIP game part: {e}')
        return
    before = {r: hashlib.sha256(open(os.path.join(folder, *r.split('/')), 'rb').read()).hexdigest()
              for r in source.bundles}
    # every tree comparison check() makes, by same_clean and by the reference (clean() both, ==): the reference's
    # answer decides, and the two must agree on every pair (round E3r)
    pairs = [0, 0]

    def both(a, b):
        want = bi.clean(a) == bi.clean(b)
        pairs[0] += 1
        pairs[1] += bi.same_clean(a, b) == want
        return want
    fails = bi.check(source, log=lambda s: None, cleaned=both)
    check(not fails, 'check: every BF1943 animation and skeleton converts and reads back', '; '.join(fails[:3]))
    check(pairs[1] == pairs[0] > 3000, f'check: same_clean answers {pairs[1]} of {pairs[0]} tree comparisons as the '
          'reference')
    lazy_game(check, source)
    out = tempfile.mkdtemp(prefix='bf1943import_')
    try:
        recs = bi.run_import(source, ['M1Garand_Bayonet/'], out, log=lambda s: None)
        conv = [r for r in recs if 'out' in r]
        check(len(conv) >= 15 and all(os.path.isfile(os.path.join(out, *r['out'].split('/'))) for r in conv),
              f'import: {len(conv)} of {len(recs)} bayonet resources written under their names')
        thrust = os.path.join(out, 'Animations', 'Weapons', 'Handheld', 'Beach', 'M1Garand_Bayonet', '1P', '1P_Melee1.res')
        g = bi.granny.Granny(open(thrust, 'rb').read(), thrust)
        pos = None
        for an in bi.granny.animations(g):
            for tg in an['TrackGroups']:
                for tr in tg['TransformTracks']:
                    if tr['Name'] == 'jntWpn_6':
                        pos = bi.granny.curve_keys(tr['PositionCurve']['CurveData'])[0][1]
        check(g.e == '<' and pos is not None and 1.1 < pos[2] < 1.35,
              f'import: the 1P thrust reads as a PC file, bayonet bone at z {pos[2] if pos else None}')
        man = json.load(open(os.path.join(out, 'import.json'), encoding='utf-8'))
        check(len(man['resources']) == len(recs) and all('done' in r and 'sha256' in r for r in man['resources']),
              'import.json: one record per resource, each saying what was done')
        bi.run_import(source, ['US_LongKnife_Knife/1P/'], out, log=lambda s: None)
        man2 = json.load(open(os.path.join(out, 'import.json'), encoding='utf-8'))
        check(len(man2['resources']) > len(man['resources']) and
              any('longknife' in r['name'].lower() for r in man2['resources']) and
              any('bayonet' in r['name'].lower() for r in man2['resources']),
              'import.json: a second import adds its records to the first one\'s')
    finally:
        shutil.rmtree(out, ignore_errors=True)
    after = {r: hashlib.sha256(open(os.path.join(folder, *r.split('/')), 'rb').read()).hexdigest()
             for r in source.bundles}
    check(before == after, 'the source bundles are unchanged')


def undeploys(check):
    """The added undeploy tracks against DICE's (bf1943import.Importer.undeploy_tracks)."""
    import math
    granny = bi.granny
    try:
        source = bi.Source(bi.default_source())
        imp = bi.Importer(source)
        ix = imp.index()
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP undeploys: {e}')
        return

    def tracks(b):
        a = granny.animations(granny.Granny(b))[0]
        return a['Duration'], {t['Name']: t for t in a['TrackGroups'][0]['TransformTracks']}

    def at(t, curve, time, default):
        return granny.sample(granny.curve_keys(t[curve]['CurveData']), time, default)

    def angle(a, b):
        return math.degrees(2 * math.acos(min(1.0, abs(sum(x * y for x, y in zip(a, b))))))
    keys = sorted(k for k, (_, e) in source.entries.items() if bi.UNDEPLOY.search(k) and e['type'] == 'GrannyAnimation')
    shared = [k for k in keys if k in ix.where]
    H = 'animations/weapons/handheld/'
    # DICE's undeploys read at once, each bundle once (round E3r: one read each, 0.16 s apiece)
    dice = ix.read_many(shared + [f'{H}m1garand/3p/{st}undeploy.res' for st in ('stand', 'crouch')])
    exact = tracks_ok = total = 0
    thumbs, bad = [], []
    for k in shared:
        out, _ = imp.convert(k)
        dur, d = tracks(dice[k])
        _, o = tracks(out)
        _, p = tracks(imp.convert(k, extra_tracks=False)[0])
        if len(o) != len(d) or any(granny.curve_keys(o[n][c]['CurveData']) != granny.curve_keys(p[n][c]['CurveData'])
                                   for n in p for c in ('PositionCurve', 'OrientationCurve')):
            bad.append(k)
        ok = n_w = 0
        for n in set(d) - set(p):
            ep = er = 0.0
            for f in (0, 0.25, 0.5, 0.75, 1):
                ep = max(ep, math.dist(at(d[n], 'PositionCurve', dur * f, (0.0, 0.0, 0.0)),
                                       at(o[n], 'PositionCurve', dur * f, (0.0, 0.0, 0.0))))
                q = angle(at(d[n], 'OrientationCurve', dur * f, (0, 0, 0, 1)), at(o[n], 'OrientationCurve', dur * f, (0, 0, 0, 1)))
                er = max(er, q)
                if 'Thumb' in n:
                    thumbs.append(q)
            if 'Thumb' not in n:
                n_w += 1
                ok += ep < 2e-3 and er < 0.2
        exact += ok == n_w
        tracks_ok += ok
        total += n_w
    mean = sum(thumbs) / max(len(thumbs), 1)
    check(len(shared) == 22 and not bad, f'undeploys: the {len(shared)} BFBC2 shipped get DICE\'s tracks, their own kept',
          str(bad[:3]))
    check(exact >= 12 and tracks_ok >= 380 and mean <= 15, f'undeploys: weapon parts as DICE\'s in {exact} of 22 '
          f'({tracks_ok} of {total} tracks), thumbs {mean:.1f} degrees from DICE\'s on average')
    own = [k for k in keys if k not in ix.where]
    counts = [len(tracks(imp.convert(k)[0])[1]) for k in own]
    check(len(own) >= 10 and set(counts) == {88}, f'undeploys: the {len(own)} BF1943-only ones get 88 tracks',
          str(counts))
    # thumb_donors: the Garand's thumbs copied from Vietnam's Garand undeploys (the same animation DICE shipped twice)
    # come out as DICE's (0 degrees), its other tracks as without a donor; the K98's from the M24's: still 88 tracks
    don = bi.Importer(source, thumb_donors={H + 'm1garand': H + 'vietnam/nam_rif_m1garand',
                                            H + 'beach/k98': H + 'us_sni_m24'})
    don._ix, don._bones = ix, imp._bones
    worst, same = 0.0, True
    for st in ('stand', 'crouch'):
        k = f'{H}m1garand/3p/{st}undeploy.res'
        dur, d = tracks(dice[k])
        out, rec = don.convert(k)
        _, o = tracks(out)
        _, plain = tracks(imp.convert(k)[0])
        same &= 'thumb tracks copied' in rec['done'] and set(o) == set(d)
        for n in d:
            if 'Thumb' in n:
                for f in (0, 0.25, 0.5, 0.75, 1):
                    worst = max(worst, angle(at(d[n], 'OrientationCurve', dur * f, (0, 0, 0, 1)),
                                             at(o[n], 'OrientationCurve', dur * f, (0, 0, 0, 1))))
            elif n in plain:
                same &= all(granny.curve_keys(o[n][c]['CurveData']) == granny.curve_keys(plain[n][c]['CurveData'])
                            for c in ('PositionCurve', 'OrientationCurve'))
    k98 = len(tracks(don.convert(H + 'beach/k98/3p/standundeploy.res')[0])[1])
    check(same and worst < 0.1 and k98 == 88, f'undeploys: thumbs from a donor undeploy - the Garand\'s from Vietnam\'s '
          f'within {worst:.2f} degrees of DICE\'s, its other tracks unchanged; the K98\'s from the M24\'s: {k98} tracks')


def main(argv):
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    units(check)
    same_clean_fuzz(check)
    lazy_source(check)
    if '--game' in argv:
        game(check)
        undeploys(check)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
