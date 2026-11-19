r"""Round F3: the rest of the suite's Python format code in the Rust core, each port against its Python reference
(kept as *_reference until a full test tier retires it) and against the reference's answers recorded in
tests\f3_golden_*.json (so the comparison survives the Python's retirement).

  aim       aimanim.parse / write / to_pc                  (bfbc2-anim-tools/aimanim.py)
  hpd       hpdtool.mesh_layout / mesh_coverage            (bfbc2-auto-navkit/tools/hpdtool.py)
  audio     audio.xma_parts / xma2_riff / _xas_stream      (bfbc2-console-convert/audio.py)
  navsample navsample.load (version 3), navsample_offline.file_bytes (the writer)
  hkpack    hkpack.Packfile                                (bfbc2-auto-navkit/tools/hkpack.py)
  disasm    xenos.disasm                                   (bfbc2-shader-tools/xenos.py)

Compared: every result (types, order, float bits, array dtype / shape / bytes / writability) or the same error
(type and message). Counted: how many inputs the Rust core answered and how many it handed back to the Python; a
real input handed back fails the test (built and damaged inputs may be handed back, each then the reference's).

  python tests\test_f3_native.py                    quick: built, seeded and damaged inputs, a few real files
  python tests\test_f3_native.py --game             + every real input: the game copy, OPFOR work\xbox, xbox_dlc,
                                                     navsample*, navkit / navbuild / navgen path data, Battlefield
                                                     1943 (WW2's source); timings
  python tests\test_f3_native.py --game --record    the same, writing the goldens from the reference's answers
  --only aim,hpd,...                                 some areas only

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import gc
import glob
import hashlib
import json
import os
import random
import struct
import sys
import tempfile
import time
from collections.abc import Callable, Iterator
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
for p in (SUITE, HERE):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core', 'anim-tools', 'navkit-tools', 'console-convert', 'shader-tools', 'bf1943-import')
import aimanim  # noqa: E402
import audio  # noqa: E402
import hkpack  # noqa: E402
import hpdtool  # noqa: E402
import navsample  # noqa: E402
import navsample_offline  # noqa: E402
import xenos  # noqa: E402
from bfbc2core import fbrb, native  # noqa: E402
from f0_parity import Checks, stdout_utf8  # noqa: E402

N = native.require('round F3 formats', 'hkpack_parse')
GAME = '--game' in sys.argv
RECORD = '--record' in sys.argv
ONLY = next((a.split('=', 1)[1] for a in sys.argv if a.startswith('--only=')), '')
GOLDEN = os.path.join(HERE, 'f3_golden_{}.json')
SCRATCH = os.environ.get('BFBC2_TEST_SCRATCH') or tempfile.gettempdir()


def wanted(area: str) -> bool:
    return not ONLY or area in ONLY.split(',')


# ------------------------------------------------------------------------------------------------ comparison

def canon(v: Any, depth: int = 0) -> Any:
    """v as a comparable nested tuple: types by name, floats by bits, arrays by dtype / shape / writability / bytes,
    objects by their attributes in order, dicts by their items in order."""
    if depth > 200:
        raise RecursionError('canon: nested too deep')
    t = type(v).__name__
    if isinstance(v, bool) or v is None or isinstance(v, int | str):
        return (t, v)
    if isinstance(v, float):
        return (t, struct.pack('<d', v))
    if isinstance(v, bytes | bytearray):
        return (t, bytes(v)) if len(v) <= 65536 else (t, len(v), hashlib.sha1(v).hexdigest())
    if isinstance(v, np.ndarray):                # hashed through the buffer: a large run is not copied
        return (t, v.dtype.str, v.shape, bool(v.flags.writeable), hashlib.sha1(np.ascontiguousarray(v)).hexdigest())
    if isinstance(v, list | tuple):
        return (t, tuple(canon(x, depth + 1) for x in v))
    if isinstance(v, dict):
        return (t, tuple((canon(k, depth + 1), canon(x, depth + 1)) for k, x in v.items()))
    if hasattr(v, '__dict__'):
        return (t, tuple((k, canon(x, depth + 1)) for k, x in vars(v).items()))
    return (t, repr(v))


def outcome(fn: Callable[..., Any], *args: Any) -> tuple[Any, ...]:
    try:
        return ('ok', canon(fn(*args)))
    except RecursionError:
        raise
    except Exception as ex:          # noqa: BLE001 - the error itself is what is compared
        return ('error', type(ex).__name__, str(ex))


def digest(o: Any) -> str:
    return hashlib.sha1(repr(o).encode()).hexdigest()[:16]


class Golden:
    """The reference's answers per group of inputs (a source file, or a quick-tier set): (stamp, digest of every
    answer in order, count). Recording writes them; checking compares the Rust's answers with them; a group whose
    source changed since (another stamp) is counted, not compared."""

    def __init__(self, area: str) -> None:
        self.area = area
        self.path = GOLDEN.format(area)
        self.old: dict[str, list[Any]] = {}
        for p in sorted(glob.glob(self.path.replace('.json', '*.json'))):
            with open(p, encoding='utf-8') as f:
                self.old.update(json.load(f))
        self.rec = {} if RECORD else self.old
        self.new: dict[str, list[Any]] = {}
        self.cur: dict[str, Any] = {}
        self.same = self.differ = self.changed = self.missing = 0
        self.bad: list[str] = []
        self.strip: list[str] = []        # text taken out of the answers before digesting (a temporary path)

    def add(self, group: str, stamp: Any, ref: Any, got: Any) -> None:
        h = self.cur.setdefault(group, [stamp, hashlib.sha1(), hashlib.sha1(), 0])
        r, a = repr(ref), repr(got)
        for s in self.strip:
            r, a = r.replace(s, '<tmp>'), a.replace(s, '<tmp>')
        h[1].update(r.encode())
        h[2].update(a.encode())
        h[3] += 1

    def close(self) -> None:
        for group, (stamp, ref, got, n) in self.cur.items():
            self.new[group] = [stamp, ref.hexdigest()[:20], n]
            want = self.rec.get(group)
            if want is None:
                self.missing += 1
            elif want[0] != stamp:
                self.changed += 1
            elif want[1] == got.hexdigest()[:20] and want[2] == n:
                self.same += 1
            else:
                self.differ += 1
                if len(self.bad) < 5:
                    self.bad.append(group)
        self.cur.clear()

    def save(self) -> None:
        """The shards (each under 1 MB), when recording (without --game, the quick groups replace theirs and the
        game groups recorded before stay)."""
        if not RECORD:
            return
        for p in glob.glob(self.path.replace('.json', '*.json')):
            os.remove(p)
        items = sorted((self.new if GAME else {**self.old, **self.new}).items())
        shards: list[dict[str, Any]] = [{}]
        size = 0
        for k, v in items:
            n = len(json.dumps({k: v})) + 2
            if size + n > 900_000:
                shards.append({})
                size = 0
            shards[-1][k] = v
            size += n
        for i, s in enumerate(shards):
            p = self.path if len(shards) == 1 else self.path.replace('.json', f'_{i}.json')
            with open(p, 'w', encoding='utf-8', newline='\n') as f:
                json.dump(s, f, indent=0, sort_keys=True)
                f.write('\n')

    def text(self) -> str:
        if RECORD:
            return f'{len(self.new)} groups recorded'
        return (f'goldens: {self.same} groups as recorded, {self.differ} different, {self.changed} changed since '
                f'recorded, {self.missing} not recorded')


class Tally:
    """One comparison's counts: same / different, answered in Rust / handed to the reference (and how many of those
    the reference answered without an error)."""

    def __init__(self, golden: Golden) -> None:
        self.g = golden
        self.same = self.ndiff = self.rust = self.handed = self.handed_ok = 0
        self.real_handed: list[str] = []
        self.differ: list[str] = []
        self.t_nat = self.t_ref = 0.0

    def both(self, label: str, group: str, stamp: Any, fn: Callable[..., Any], ref: Callable[..., Any],
             nat: Callable[..., Any] | None, *args: Any, real: bool = False) -> tuple[Any, ...]:
        t0 = time.perf_counter()
        a = outcome(fn, *args)
        t1 = time.perf_counter()
        r = outcome(ref, *args)
        self.t_nat += t1 - t0
        self.t_ref += time.perf_counter() - t1
        if nat is not None:
            answered = nat(*args) is not None
            self.rust += answered
            self.handed += not answered
            self.handed_ok += (not answered) and r[0] == 'ok'
            if real and not answered and len(self.real_handed) < 10:
                self.real_handed.append(label)
            if real and not answered:
                self.real_handed_n = getattr(self, 'real_handed_n', 0) + 1
        if a == r:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 8:
                self.differ.append(f'{label}: {str(a)[:120]} / {str(r)[:120]}')
        self.g.add(group, stamp, r, a)
        return r

    def text(self) -> str:
        return (f'{self.same:,} identical, {self.ndiff} different; {self.rust:,} answered in Rust, {self.handed:,} '
                f'handed to the reference ({self.handed_ok:,} of them answered by it)')

    def report(self, c: Checks, what: str, real: bool) -> None:
        self.g.close()
        n_real = getattr(self, 'real_handed_n', 0)
        ok = not self.ndiff and (not real or n_real == 0) and (RECORD or not self.g.differ)
        c.check(ok, f'{what}: {self.text()}; {self.g.text()}',
                '; '.join(self.differ + [f'handed back (real): {n_real}: {self.real_handed}'] + self.g.bad))


def damaged(b: bytes, rng: random.Random) -> bytes:
    x = bytearray(b)
    k = rng.randrange(3)
    if k in (1, 2) and x:
        for _ in range(rng.randint(1, 4)):
            x[rng.randrange(len(x))] = rng.randrange(256)
    if k in (0, 2) and x:
        del x[rng.randrange(len(x)):]
    return bytes(x)


def opfor_root() -> str | None:
    try:
        import run_tests
        return str(run_tests.opfor())
    except Exception as ex:          # noqa: BLE001 - no OPFOR: built inputs only
        print(f'skip the game files ({ex})')
        return None


def bundle_entries(pattern: str, types: set[str]) -> Iterator[tuple[str, int, list[tuple[str, str, bytes]]]]:
    """Per bundle (path, size, [(type, name, bytes)]) of the entries of those types: one bundle in memory at a time."""
    for p in sorted(glob.glob(pattern, recursive=True)):
        try:
            es = [e for e in fbrb.entries(p) if e.type in types and not e.deleted]
        except Exception:                                        # noqa: BLE001 - not a bundle this reader takes
            continue
        if not es:
            continue
        data = fbrb.read(p, [e.name for e in es])
        yield p, os.path.getsize(p), [(e.type, e.name, data[e.name]) for e in es]
        del data


def bf1943_source() -> Any:
    try:
        import bf1943import
        return bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                       # noqa: BLE001
        print(f'skip Battlefield 1943 ({e})')
        return None


def rel(p: str, root: str | None) -> str:
    return os.path.relpath(p, root).replace('\\', '/') if root else p


# ------------------------------------------------------------------------------------------------ aimanim

def aim_dict(rng: random.Random) -> dict[str, Any]:
    n = rng.choice([1, 3, 25])
    f = lambda: rng.choice([0.0, -0.0, 1.0, rng.uniform(-3, 3), 1e-42, float('inf')])     # noqa: E731
    q = lambda: (f(), f(), f(), f())                                                        # noqa: E731
    bones = []
    for b in rng.sample(range(80), rng.randint(0, 6)):
        k = rng.choice([0, 1, 2])
        blk = (q(), q()) if k == 0 else (q(), [q() for _ in range(n)]) if k == 1 else [(q(), q()) for _ in range(n)]
        bones.append((b, k, blk))
    return {'addr': rng.randrange(1 << 32), 'poses': n, 'bones': bones}


def big_aim(b: bytes) -> bytes:
    nb = struct.unpack_from('<I', b, 0x10)[0]
    out = bytearray(b)
    for o in range(0, len(b) - 3, 4):
        if 0x14 <= o < 0x1c + 2 * nb:
            continue
        out[o:o + 4] = b[o:o + 4][::-1]
    return bytes(out)


def changed_aim(a: dict[str, Any], rng: random.Random) -> Any:
    import copy
    g = copy.deepcopy(a)
    k = rng.randrange(7)
    bad = [-1, 256, 1 << 32, 1.5, None, 'x', True, float('nan'), 1e40]
    if k == 0:
        g[rng.choice(['addr', 'poses'])] = rng.choice(bad)
    elif k == 1 and g['bones']:
        i = rng.randrange(len(g['bones']))
        b, kind, blk = g['bones'][i]
        g['bones'][i] = rng.choice([(rng.choice(bad), kind, blk), (b, rng.choice(bad + [3, 7]), blk), (b, kind),
                                    [b, kind, blk], (b, kind, blk, 0)])
    elif k == 2 and g['bones']:
        i = rng.randrange(len(g['bones']))
        b, kind, blk = g['bones'][i]
        if kind == 2 and blk:
            blk[0] = (blk[0][0], (1.0, 2.0, rng.choice(bad), 4.0))
        elif kind == 1:
            blk = (blk[0], blk[1][:-1]) if rng.random() < 0.5 else (list(blk[0]) + [0.0], blk[1])
        else:
            blk = (blk[0], [1.0, 2.0, 3.0])
        g['bones'][i] = (b, kind, blk)
    elif k == 3:
        g['bones'] = tuple(g['bones'])
    elif k == 4:
        del g[rng.choice(['addr', 'poses', 'bones'])]
    elif k == 5:
        from collections import OrderedDict
        return OrderedDict(g)
    return g


def aim_check(t: Tally, label: str, group: str, stamp: Any, b: bytes, big: bool, real: bool,
              bones: Any = None) -> None:
    r = t.both(label, group, stamp, aimanim.parse, aimanim.parse_reference, lambda b, e: N.aimanim_parse(b, e), b, big,
               real=real)
    if r[0] == 'ok':
        a = aimanim.parse_reference(b, big)
        t.both(label + ' write', group, stamp, aimanim.write, aimanim.write_reference, N.aimanim_write, a, real=real)
    if big:
        t.both(label + ' to_pc', group, stamp, aimanim.to_pc, aimanim.to_pc_reference,
               lambda b, m: N.aimanim_to_pc(b, m), b, bones, real=real and r[0] == 'ok')


def area_aim(c: Checks) -> None:
    g = Golden('aim')
    t = Tally(g)
    rng = random.Random(0xF3A1)
    built = []
    for i in range(400):
        a = aim_dict(rng)
        b = aimanim.write_reference(a)
        built.append((a, b))
        aim_check(t, f'built {i}', 'quick:built', 0, b, False, False)
        bb = big_aim(b)
        maps = [None, {}, {x[0]: x[0] for x in a['bones']}, {x[0]: 200 - x[0] for x in a['bones']},
                {x[0]: 3 for x in a['bones']}, dict(list({x[0]: x[0] for x in a['bones']}.items())[1:]),
                {x[0]: 300 for x in a['bones']}, [1, 2]]
        aim_check(t, f'built {i} big', 'quick:built', 0, bb, True, False, rng.choice(maps))
    for i in range(1500):
        a, b = rng.choice(built)
        big = rng.random() < 0.5
        aim_check(t, f'damaged {i}', 'quick:damaged', 0, damaged(big_aim(b) if big else b, rng), big, False)
        t.both(f'changed {i}', 'quick:changed', 0, aimanim.write, aimanim.write_reference, N.aimanim_write,
               changed_aim(a, rng))
    if GAME:
        root = opfor_root()
        if root:
            for pat, big in ((os.path.join(root, 'game', '**', '*.fbrb'), False),
                             (os.path.join(root, 'work', 'xbox', '**', '*.fbrb'), True),
                             (os.path.join(root, 'work', 'xbox_dlc', '**', '*.fbrb'), True)):
                for p, size, ents in bundle_entries(pat, {'AimAnimation'}):
                    for _, name, b in ents:
                        aim_check(t, f'{rel(p, root)}/{name}', rel(p, root), size, b, big, True)
        src = bf1943_source()
        if src is not None:
            bones = None
            try:
                import bf1943import
                import granny
                import meshview
                import shaderdb
                pc = meshview.Index(shaderdb.find_opfor(None)).read(bf1943import.SOLDIER)
                bones = granny.bone_map(src.data(bf1943import.SOLDIER), pc)
            except Exception as e:                               # noqa: BLE001
                print(f'no soldier bone map ({e}): to_pc without renumbering')
            ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') == 'AimAnimation')
            for k in ks:
                aim_check(t, f'BF1943 {k}', 'bf1943', len(ks), src.data(k), True, True, bones)
                aim_check(t, f'BF1943 {k} (no map)', 'bf1943', len(ks), src.data(k), True, True, None)
    print(f'aimanim timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'aimanim parse / write / to_pc', GAME)
    g.save()


# ------------------------------------------------------------------------------------------------ hpdtool

def area_hpd(c: Checks) -> None:
    import test_e3o_native as e3o
    g = Golden('hpd')
    t = Tally(g)
    rng = random.Random(0xF3B2)

    def one(label: str, group: str, stamp: Any, m: bytes, real: bool) -> None:
        t.both(label, group, stamp, hpdtool.mesh_layout, hpdtool.mesh_layout_reference, N.hpd_mesh_layout, m,
               real=real)
        t.both(label + ' coverage', group, stamp, hpdtool.mesh_coverage, hpdtool.mesh_coverage_reference,
               N.hpd_mesh_coverage, m, real=real)
    meshes = []
    for i in range(300):
        m = e3o.kymesh.write_mesh_reference(e3o.random_mesh(rng, finite=rng.random() < 0.7))
        meshes.append(m)
        one(f'built {i}', 'quick:built', 0, m, False)
    for i in range(2000):
        one(f'damaged {i}', 'quick:damaged', 0, damaged(rng.choice(meshes), rng), False)
    root = opfor_root()
    if root:
        import run_tests
        files = list(run_tests.pathdata_files(root)) if GAME else sorted(
            glob.glob(os.path.join(root, 'game', 'Output', 'win32', 'ai', '**', 'pathdata.hpd'), recursive=True))[:3]
        sectors = 0
        for p in files:
            data = open(p, 'rb').read()
            try:
                f = hpdtool.parse(data)
            except Exception as e:                               # noqa: BLE001 - not a path data file: noted
                print(f'  skipped {p}: {type(e).__name__}: {e}')
                continue
            frng = random.Random(len(data))
            for s in f.sectors:
                sectors += 1
                one(f'{rel(p, root)} sector {s.id:#x}', rel(p, root), len(data), s.mesh, True)
                if GAME:
                    one(f'{rel(p, root)} sector {s.id:#x} damaged', rel(p, root) + ' damaged', len(data),
                        damaged(s.mesh, frng), False)
        print(f'path data: {len(files)} files, {sectors:,} sectors')
    print(f'mesh_layout + coverage timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'hpdtool mesh_layout / mesh_coverage', bool(root))
    g.save()


# ------------------------------------------------------------------------------------------------ audio

def xma_stream(rng: random.Random) -> Any:
    ch = rng.choice([1, 2, 3, 6, 0, -1])
    k = max(0, (ch + 1) // 2)
    blocks = []
    for _ in range(rng.randint(0, 4)):
        b = b''
        for _ in range(k):
            n = rng.choice([4, 5, 100, 2052, 4100])
            b += struct.pack('>I', n << 2 | rng.randrange(4)) + bytes(rng.randrange(256) for _ in range(n - 4))
        b += rng.choice([b'', b'\0\0\0', b'\0\x01'])
        blocks.append((rng.randrange(9000), b))
    return audio.Stream(1, 3, ch, 48000, 1, 0, blocks)


def audio_stream_checks(t: Tally, label: str, group: str, stamp: Any, st: Any, real: bool) -> None:
    r = t.both(label + ' parts', group, stamp, audio.xma_parts, audio.xma_parts_reference, N.audio_xma_parts, st,
               real=real)
    if r[0] != 'ok':
        return
    for i, p in enumerate(audio.xma_parts_reference(st)):
        ch = min(2, st.channels - 2 * i)
        t.both(f'{label} riff {i}', group, stamp, audio.xma2_riff, audio.xma2_riff_reference,
               lambda *a: N.audio_xma2_riff(*a), bytes(p), ch, st.rate, st.samples, real=real)


def xas_args(st: Any) -> tuple[bytes, int, int, int]:
    return b''.join(b for _, b in st.blocks), st.samples, st.channels, st.rate


def area_audio(c: Checks) -> None:
    g = Golden('audio')
    t = Tally(g)
    rng = random.Random(0xF3C3)
    blocks_of = lambda *a: audio._xas_stream(*a, 1).blocks                         # noqa: E731
    for i in range(600):
        st = xma_stream(rng)
        audio_stream_checks(t, f'built {i}', 'quick:built', 0, st, False)
        if st.blocks and rng.random() < 0.5:
            j = rng.randrange(len(st.blocks))
            st.blocks[j] = (st.blocks[j][0], damaged(st.blocks[j][1], rng))
            t.both(f'damaged {i}', 'quick:damaged', 0, audio.xma_parts, audio.xma_parts_reference, N.audio_xma_parts,
                   st)
    bad = [0, -1, 1 << 16, 1 << 32, True, 2.0, None]
    for i in range(600):
        args = [bytes(rng.randrange(256) for _ in range(rng.randrange(50))), rng.choice([1, 2, 1, 2] + bad),
                rng.choice([48000, 44100, 22050] + bad), rng.choice([0, 1000, (1 << 32) - 1] + bad)]
        t.both(f'riff {i}', 'quick:riff', 0, audio.xma2_riff, audio.xma2_riff_reference,
               lambda *a: N.audio_xma2_riff(*a), *args)
        frames = bytes(rng.randrange(256) for _ in range(rng.randrange(0, 40000)))
        xa = (frames, rng.choice([0, 1, 5000, 19456, 40000, -1]), rng.choice([1, 2, 6, 0]),
              rng.choice([48000, 44100, 11025, 1, 0, -5, 1280 * 7]))
        t.both(f'xas {i}', 'quick:xas', 0, blocks_of, lambda *a: audio._xas_blocks_reference(*a),
               lambda *a: N.audio_xas_blocks(*a), *xa)
    if GAME:
        root = opfor_root()
        n_xma = n_xas = 0
        if root:
            for pat, source in ((os.path.join(root, 'work', 'xbox', '**', '*.fbrb'), 'bfbc2-xbox'),
                                (os.path.join(root, 'work', 'xbox_dlc', '**', '*.fbrb'), 'bfbc2-xbox'),
                                (os.path.join(root, 'game', '**', '*.fbrb'), 'bfbc2-pc')):
                for p, size, ents in bundle_entries(pat, {'Wave'}):
                    for _, name, b in ents:
                        try:
                            w = audio.read_wave(b, source)
                        except audio.AudioError:
                            continue
                        for i in range(3):
                            if w.segments[i] is None:
                                continue
                            try:
                                st = audio.segment_stream(w, i)
                            except audio.AudioError:
                                continue                         # streamed: its audio is not in the bundle
                            label = f'{rel(p, root)}/{name} {i}'
                            if st.codec == 3:
                                n_xma += 1
                                audio_stream_checks(t, label, rel(p, root), size, st, True)
                            elif st.codec == 4:
                                n_xas += 1
                                t.both(label + ' xas', rel(p, root), size, blocks_of,
                                       lambda *a: audio._xas_blocks_reference(*a),
                                       lambda *a: N.audio_xas_blocks(*a), *xas_args(st), real=True)
        src = bf1943_source()
        if src is not None:
            ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') == 'Wave')
            for k in ks:
                streams = {i: src.data(n) for i in range(3)
                           for n in [f'{k[:-len("_wave.res")]}_wavedatasns_{i}.res'] if n in src.entries}
                try:
                    w = audio.read_wave(src.data(k), 'bf1943')
                except audio.AudioError:
                    continue
                for i in range(3):
                    if w.segments[i] is None:
                        continue
                    try:
                        st = audio.segment_stream(w, i, streams)
                    except audio.AudioError:
                        continue
                    if st.codec == 3:
                        n_xma += 1
                        audio_stream_checks(t, f'BF1943 {k} {i}', 'bf1943', len(ks), st, True)
            # OPFOR's calls: audio.convert of BF1943 Waves (the WW2 install's sound port) both ways end to end
            try:
                audio.ffmpeg_path()
                sample = ks[::max(1, len(ks) // 120)]
                same = 0
                for k in sample:
                    b = src.data(k)
                    streams = {i: src.data(n) for i in range(3)
                               for n in [f'{k[:-len("_wave.res")]}_wavedatasns_{i}.res'] if n in src.entries}
                    a = outcome(audio.convert, b, 'bf1943', streams)
                    saved = (audio.xma_parts, audio.xma2_riff, audio._xas_stream)
                    audio.xma_parts, audio.xma2_riff = audio.xma_parts_reference, audio.xma2_riff_reference
                    audio._xas_stream = lambda f, n, ch, rate, ty: audio.Stream(
                        1, 4, ch, rate, ty, n, audio._xas_blocks_reference(f, n, ch, rate))
                    try:
                        r = outcome(audio.convert, b, 'bf1943', streams)
                    finally:
                        audio.xma_parts, audio.xma2_riff, audio._xas_stream = saved
                    same += a == r
                    g.add('convert:bf1943', len(sample), r, a)
                c.check(same == len(sample), f'audio.convert of {len(sample)} BF1943 Waves (WW2 install\'s sound '
                        f'port): {same} byte-identical with the Rust framing and with the Python framing')
            except audio.AudioError as e:
                print(f'skip the end-to-end conversions ({e})')
        print(f'game streams: {n_xma:,} EA-XMA, {n_xas:,} EA-XAS')
    print(f'audio framing timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'audio xma_parts / xma2_riff / XAS blocks', GAME)
    g.save()


# ------------------------------------------------------------------------------------------------ navsample

def samples(rng: random.Random, cols: int) -> Any:
    counts = np.array([rng.choice([0, 0, 1, 2, 5]) for _ in range(cols)], np.int64)
    start = np.zeros(cols + 1, np.int64)
    np.cumsum(counts, out=start[1:])
    n = int(start[-1])
    nrm = np.array([rng.choice([0.0, 1.0, -1.0, 0.5, 63.5 / 127, rng.uniform(-1.2, 1.2), np.inf]) for _ in range(3 * n)],
                   np.float32).reshape(-1, 3)
    hit = np.array([rng.randrange(1 << 32) for _ in range(3 * n)], np.uint32).reshape(-1, 3) if rng.random() < 0.7 \
        else None
    return navsample.Samples(rng.choice([0.8, 0.28, 0.2]), rng.uniform(-900, 900), rng.uniform(-900, 900), cols, 1,
                             120.0, -20.0, 0.3, 0.28, 1.8, 0.05, start,
                             np.array([rng.uniform(-50, 150) for _ in range(n)], np.float32), nrm,
                             np.array([rng.randrange(256) for _ in range(n)], np.uint8), hit)


def changed_samples(s: Any, rng: random.Random) -> Any:
    import copy
    g = copy.copy(s)
    k = rng.randrange(8)
    if k == 0:
        g.y = g.y.astype(np.float64)
    elif k == 1 and len(g.normal):
        g.normal = g.normal.copy()
        g.normal[0, 0] = np.nan
    elif k == 2:
        g.start = g.start.copy()
        if len(g.start) > 1:
            g.start[-1] += 300
    elif k == 3:
        g.pitch = 1e40
    elif k == 4:
        g.nx = 1 << 40
    elif k == 5:
        g.hit = np.zeros(3, np.uint32)
    elif k == 6:
        g.flags = g.flags.astype(np.int32)
    else:
        g.pitch = 'x'
    return g


def nat_write(s: Any) -> Any:
    """Whether the Rust core writes s (its bytes) or hands it to the reference (None)."""
    a = navsample_offline._write_args(s)
    return None if a is None else N.navkit_navsample_write(*a)


def area_navsample(c: Checks) -> None:
    g = Golden('navsample')
    t = Tally(g)
    rng = random.Random(0xF3D4)
    tmp = tempfile.mkdtemp(prefix='f3_navsample_', dir=SCRATCH)
    path = os.path.join(tmp, 'run.bin')
    g.strip = [path, repr(path)[1:-1]]
    try:
        for i in range(300):
            s = samples(rng, rng.randrange(0, 60))
            t.both(f'built {i} write', 'quick:write', 0, navsample_offline.file_bytes,
                   navsample_offline.file_bytes_reference,
                   nat_write,
                   s)
            try:
                b = navsample_offline.file_bytes_reference(s)
            except Exception:                                    # noqa: BLE001 - refused by the reference too
                continue
            for k, data in enumerate([b, damaged(b, rng), damaged(b, rng)]):
                with open(path, 'wb') as f:
                    f.write(data)
                t.both(f'built {i} load {k}', 'quick:load', 0, navsample.load, navsample.load_reference,
                       lambda p: (lambda d: N.navkit_navsample_v3(d, *struct.unpack_from('<2i', d, 20))
                                  if len(d) >= 56 else None)(open(p, 'rb').read()), path)
            t.both(f'changed {i}', 'quick:changed', 0, navsample_offline.file_bytes,
                   navsample_offline.file_bytes_reference,
                   nat_write,
                   changed_samples(s, rng))
    finally:
        for name in os.listdir(tmp):
            os.remove(os.path.join(tmp, name))
        os.rmdir(tmp)
    if GAME:
        root = opfor_root()
        files = sorted(glob.glob(os.path.join(root, 'work', 'navsample*', '**', '*.bin'), recursive=True)) if root \
            else []
        n_cols = n_surf = n_whole = n_chunks = 0
        skipped_v12: list[str] = []
        chunk_dir = tempfile.mkdtemp(prefix='f3_navsample_game_', dir=SCRATCH)
        chunk_path = os.path.join(chunk_dir, 'chunk.bin')

        def nat_load(p: str) -> Any:
            d = open(p, 'rb').read()
            return N.navkit_navsample_v3(d, *struct.unpack_from('<2i', d, 20)) if len(d) >= 56 else None
        try:
            for p in files:
                group, size = rel(p, root), os.path.getsize(p)
                with open(p, 'rb') as fh:
                    head = fh.read(56)
                if len(head) < 56 or head[:4] != b'ONS1':
                    continue
                version = struct.unpack_from('<I', head, 4)[0]
                # the whole file both ways while the reference's arrays fit the memory cap (the reference holds
                # the file, its records' views and float64 normals: about 3.5x the file; version 1 / 2's loop
                # builds Python lists, about 15x the file: those runs are E3c's reader, read whole up to 40 MB)
                if size <= (120_000_000 if version == 3 else 40_000_000):
                    n_whole += 1
                    t.both(group, group, size, navsample.load, navsample.load_reference,
                           nat_load if version == 3 else None, p, real=version == 3)
                    gc.collect()
                elif version != 3:
                    skipped_v12.append(group)
                if version != 3:
                    continue
                with open(p, 'rb') as fh:                   # streamed: only one column range in memory
                    fh.seek(size - 12)
                    at, tag = struct.unpack('<Q4s', fh.read(12))
                    if tag != b'CNT3':
                        continue                             # an unfinished run: compared whole above
                    nx, nz = struct.unpack_from('<2i', head, 20)
                    cols = nx * nz

                    def counts_sum(lo: int, hi: int, fh: Any = fh, at: int = at) -> tuple[int, bytes]:
                        fh.seek(at + lo)
                        cb = fh.read(hi - lo)
                        return int(np.frombuffer(cb, np.uint8).sum(dtype=np.int64)), cb
                    total = sum(counts_sum(k, min(cols, k + (1 << 24)))[0] for k in range(0, cols, 1 << 24))
                    n_cols += cols
                    n_surf += total
                    # every record through both readers and both writers, in column ranges of at most ~1.5 M
                    # records: each range a file of its own (the header with nx = the range, nz = 1)
                    step = max(1, cols // (total // 1_500_000 + 1))
                    lo = a = 0
                    while lo < cols:
                        hi = min(cols, lo + step)
                        n, cb = counts_sum(lo, hi)
                        fh.seek(56 + 20 * a)
                        with open(chunk_path, 'wb') as out:
                            for piece in (head[:20], struct.pack('<2i', hi - lo, 1), head[28:56], fh.read(20 * n), cb,
                                          struct.pack('<Q', 56 + 20 * n), b'CNT3'):
                                out.write(piece)
                        del cb
                        n_chunks += 1
                        t.both(f'{group} columns {lo}-{hi}', group + ' read', size, navsample.load,
                               navsample.load_reference, nat_load, chunk_path, real=True)
                        part, _ = navsample.load_reference(chunk_path)
                        t.both(f'{group} write {lo}-{hi}', group + ' write', size, navsample_offline.file_bytes,
                               navsample_offline.file_bytes_reference, nat_write, part, real=True)
                        del part
                        lo, a = hi, a + n
                gc.collect()
        finally:
            if os.path.exists(chunk_path):
                os.remove(chunk_path)
            os.rmdir(chunk_dir)
        print(f'sample files: {len(files)} ({n_whole} also read whole), version 3: {n_cols:,} columns, {n_surf:,} '
              f'surfaces in {n_chunks} column ranges; version 1 / 2 files over 40 MB not read whole (E3c reader): {skipped_v12}')
    print(f'navsample timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'navsample load (version 3) / write', GAME)
    g.save()


# ------------------------------------------------------------------------------------------------ hkpack

def area_hkpack(c: Checks) -> None:
    g = Golden('hkpack')
    t = Tally(g)
    rng = random.Random(0xF3E5)
    root = opfor_root()
    nat = lambda d: N.hkpack_parse(d) if type(d) is bytes else None                     # noqa: E731
    seeds: list[bytes] = []
    if root:
        for _p, _size, ents in bundle_entries(os.path.join(root, "game", "**", "mp_005", '*.fbrb'), {'HavokPhysicsData'}):
            seeds += [b for _, _, b in ents[:4]]
            if len(seeds) >= 8:
                break
    for i, b in enumerate(seeds):
        t.both(f'seed {i}', 'quick:seed', 0, hkpack.Packfile, hkpack.Packfile.reference, nat, b, real=True)
    for i in range(800 if seeds else 0):
        t.both(f'damaged {i}', 'quick:damaged', 0, hkpack.Packfile, hkpack.Packfile.reference, nat,
               damaged(rng.choice(seeds), rng))
    t.both('no packfile', 'quick:edge', 0, hkpack.Packfile, hkpack.Packfile.reference, nat, b'nothing here')
    if GAME and root:
        n = 0
        for pat in (os.path.join(root, 'game', '**', '*.fbrb'), os.path.join(root, 'work', 'ww2', '**', '*.fbrb')):
            for p, size, ents in bundle_entries(pat, {'HavokPhysicsData'}):
                for _, name, b in ents:
                    n += 1
                    t.both(f'{rel(p, root)}/{name}', rel(p, root), size, hkpack.Packfile, hkpack.Packfile.reference,
                           nat, b, real=hkpack.MAGIC in b)
        # NavKit's offline sampler reads the level folders' *_Havok.res through hkpack (asset_bodies)
        levels = sorted(glob.glob(os.path.join(root, 'work', 'levels', '*', '**', '*_havok.res'), recursive=True))
        for p in levels:
            b = open(p, 'rb').read()
            n += 1
            t.both(rel(p, root), 'work/levels', len(levels), hkpack.Packfile, hkpack.Packfile.reference, nat, b,
                   real=hkpack.MAGIC in b)
        print(f'packfiles: {n:,}')
    print(f'hkpack timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'hkpack.Packfile', bool(root))
    g.save()


# ------------------------------------------------------------------------------------------------ disasm

def decoded_in_rust(b: bytes, px: bool, _raw: Any, _head: Any) -> Any:
    """The Rust program behind parse_shader's tree (disasm lists it in Rust), or None (the Python lists it)."""
    try:
        return xenos.native_program(xenos.parse_shader(b, px))
    except Exception:                                            # noqa: BLE001 - not decoded: the reference raises
        return None


def area_disasm(c: Checks) -> None:
    suite_paths.use('shader-tools')
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests'))
    import test_xenos_native as tx
    g = Golden('disasm')
    t = Tally(g)
    rng = random.Random(0xF3F6)

    def one(label: str, group: str, stamp: Any, blob: bytes, pixel: bool, real: bool) -> None:
        for raw in (False, True):
            for head in (True, False):
                t.both(f'{label} raw={raw} header={head}', group, stamp,
                       lambda b, px, r, h: xenos.disasm(xenos.parse_shader(b, px), r, h),
                       lambda b, px, r, h: xenos.disasm_reference(xenos.parse_shader_python(b, px), r, h),
                       decoded_in_rust,
                       blob, pixel, raw, head, real=real)
    for i in range(300):
        pixel = rng.random() < 0.5
        blob = tx.build(rng, pixel)
        one(f'built {i}', 'quick:built', 0, blob, pixel, False)
        one(f'built {i} damaged', 'quick:damaged', 0, tx.mutate(blob, rng), pixel, False)
    # a tree whose microcode was read: the Python lists it
    sh = xenos.parse_shader(tx.build(rng, True), True)
    _ = sh.cf
    t.both('read tree', 'quick:edge', 0, xenos.disasm, xenos.disasm_reference, lambda s: xenos.native_program(s), sh)
    if GAME:
        items, skipped = tx.corpus()
        for s in skipped:
            print(f'  skipped: {s}')
        for label, _, blob, pixel in items:
            one(label, label.split(' ')[0].rsplit('/', 1)[0], len(items), blob, pixel, True)
        print(f'programs: {len(items):,} distinct')
    print(f'disasm timing (parse + disasm): Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'xenos.disasm', GAME)
    g.save()


def main() -> int:
    stdout_utf8()
    c = Checks()
    for name, fn in (('aim', area_aim), ('hpd', area_hpd), ('audio', area_audio), ('navsample', area_navsample),
                     ('hkpack', area_hkpack), ('disasm', area_disasm)):
        if wanted(name):
            t0 = time.perf_counter()
            fn(c)
            print(f'-- {name}: {time.perf_counter() - t0:.1f} s', flush=True)
            gc.collect()
    return c.done()


if __name__ == '__main__':
    sys.exit(main())
