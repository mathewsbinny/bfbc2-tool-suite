r"""Round F0: the Wave resource and the EA stream framing of audio.py in the Rust core (bc2_native.wave_read,
wave_write_pc, sps_read, snr_read, sps_write) against their Python, the reference that stays until the port passes a
full test tier (read_wave_reference, write_wave_pc_reference, read_sps_reference, read_snr_reference,
write_sps_reference).

Compared: every Wave's words, segments and BF1943 fields (types and float bits), every stream's header values and
blocks, the bytes written; for damaged inputs and changed objects the same result or the same error.

  python test_wave_native.py           quick: Waves and streams built here (every source, segments with and without
                                       stream information, odd sizes, SPS and SNR / SNS framings), 3,000 damaged
                                       copies, 600 changed objects written both ways; 40 BF1943 Waves when found
  python test_wave_native.py --game    + every Wave of Battlefield 1943 (with its waveDataSns streams), of BFBC2's
                                       unpacked Xbox bundles (OPFOR work\xbox) and of the game copy (PC): read and
                                       written both ways with every segment's stream, 3 damaged copies of each; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.

The readers' references (read_wave_reference, read_sps_reference, read_snr_reference) were retired in round RT3:
their answers were recorded first (--record, and --game --record, while they were there: f0_wave_replay*.json beside
this test) and are replayed by tests/replay.py; the port must give every recorded answer, the same values or the same
error and message (it answers every input now: "left to the reference" counts nothing). The writers' references
(write_wave_pc_reference, write_sps_reference) stay and are compared live.
"""
import collections
import copy
import glob
import os
import random
import struct
import sys
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core')
import audio  # noqa: E402
from bfbc2core import fbrb, native, paths  # noqa: E402
from f0_parity import Checks, outcome, stdout_utf8  # noqa: E402
from replay import Replay  # noqa: E402

N = native.require('audio.py containers', 'wave_read')
# round RT2: the references' answers recorded (--record) and replayed once they are retired
R = Replay('f0_wave', HERE, [(audio, 'read_wave_reference'), (audio, 'write_wave_pc_reference'),
                             (audio, 'read_sps_reference'), (audio, 'read_snr_reference'),
                             (audio, 'write_sps_reference')])
# the readers' replays (round RT3 retired them)
READ_WAVE_REF: Any = vars(audio)['read_wave_reference']
READ_SPS_REF: Any = vars(audio)['read_sps_reference']
READ_SNR_REF: Any = vars(audio)['read_snr_reference']


class Tally:
    """Counts of one comparison; fallback_ok: cases the reference answered and the Rust core left to it (none is
    expected of the files and streams the game holds or the reference writes: they are counted and checked there)."""

    def __init__(self) -> None:
        self.same = self.ndiff = self.native = self.fallback = self.fallback_ok = 0
        self.differ: list[str] = []

    def add(self, ok: bool, label: str, answered: bool | None = None, ref_ok: bool = False) -> None:
        if answered is not None:
            self.native += answered
            self.fallback += not answered
            self.fallback_ok += (not answered) and ref_ok
        if ok:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 10:
                self.differ.append(label)

    def text(self) -> str:
        return (f'{self.same} identical ({self.native} answered in Rust, {self.fallback} left to the reference, '
                f'{self.fallback_ok} of them answered by it)')


def both(t: Tally, label: str, fn: Any, ref: Any, nat: Any, *args: Any) -> Any:
    """fn (dispatching) against ref (the Python, or its recorded answers) on args; nat(*args) says whether Rust
    answered. fn's result when it answered."""
    a, r = outcome(fn, *args), outcome(ref, *args)
    t.add(a == r, f'{label}: {str(a)[:90]} / {str(r)[:90]}', nat(*args) is not None, r[0] == 'ok')
    return fn(*args) if a[0] == 'ok' else None


def wave_all(t: Tally, label: str, b: bytes, source: str, streams: dict[int, bytes] | None = None) -> None:
    """A Wave read both ways, written both ways, and each segment's stream read and written both ways."""
    w = both(t, label, audio.read_wave, READ_WAVE_REF, lambda b, s: N.wave_read(b, s), b, source)
    if w is None:
        return
    both(t, label + ' write', audio.write_wave_pc, audio.write_wave_pc_reference, N.wave_write_pc, w)
    for i, s in enumerate(w.segments):
        if s is None:
            continue
        data = s[0]
        sns = (streams or {}).get(i)
        if source == 'bf1943':
            st = both(t, f'{label} snr {i}', audio.read_snr, READ_SNR_REF,
                      lambda d, x: N.snr_read(d, x), data, sns)
        else:
            st = both(t, f'{label} sps {i}', audio.read_sps, READ_SPS_REF, N.sps_read, sns if sns is not None and len(data) == 4 else data)
        if st is not None:
            both(t, f'{label} sps write {i}', audio.write_sps, audio.write_sps_reference, N.sps_write, st)


# ------------------------------------------------------------------------------------------------ built inputs

def sps(rng: random.Random) -> bytes:
    st = audio.Stream(1, rng.choice([3, 4, 6]), rng.randint(1, 6), rng.choice([48000, 44100, 22050]), 1, 0,
                      [(rng.randrange(5000), bytes(rng.randrange(256) for _ in range(rng.randrange(40)))) for _ in range(rng.randrange(4))])
    st.samples = sum(n for n, _ in st.blocks) & 0x1FFFFFFF
    return audio.write_sps(st)


def snr(rng: random.Random, streamed: bool) -> tuple[bytes, bytes | None]:
    h = struct.pack('>II', 0 << 28 | 3 << 24 | (rng.randint(1, 2) - 1) << 18 | 48000, rng.randrange(1 << 29))
    blocks = b''
    k = rng.randrange(1, 4)
    for j in range(k):
        data = bytes(rng.randrange(256) for _ in range(rng.randrange(30)))
        flags = 0x80 if (streamed and j == k - 1) else 0
        blocks += struct.pack('>II', flags << 24 | (8 + len(data)), rng.randrange(9000)) + data
    return (h, blocks) if streamed else (h + blocks, None)


def wave(rng: random.Random) -> tuple[bytes, str, dict[int, bytes] | None]:
    source = rng.choice(audio.SOURCES)
    e = '<' if source == 'bfbc2-pc' else '>'
    streams: dict[int, bytes] = {}
    out = b''
    if source == 'bf1943':
        out += struct.pack('>f6I', rng.choice([48000.0, 44100.0, 11025.0, -0.0, float('inf')]), 2, rng.randint(1, 2),
                           rng.randrange(1 << 20), rng.randrange(1000), rng.randrange(1 << 20), rng.randrange(1 << 32))
    else:
        out += struct.pack(e + '6I', *(rng.randrange(1 << 32) for _ in range(6)))
    for i in range(3):
        if i != 1 and rng.random() < 0.5:
            out += struct.pack(e + 'I', 0)
            continue
        if source == 'bf1943':
            streamed = rng.random() < 0.3
            data, sns = snr(rng, streamed)
            if sns is not None:
                streams[i] = sns
        elif rng.random() < 0.2:
            data = struct.pack(e + 'I', rng.randrange(1 << 20))
            streams[i] = sps(rng)
        else:
            data = sps(rng)
        info = bytes(rng.randrange(256) for _ in range(rng.choice([0, 0, 3, 8, 13])))
        out += struct.pack(e + 'I', len(data)) + data + audio.PAD[:(-len(data)) % 4]
        out += struct.pack(e + 'I', len(info)) + info + (audio.PAD[:(-len(info)) % 4] if rng.random() < 0.9 or i < 2 else b'')
    if len(out) < 40:
        out += b''
    return out, source, streams or None


def damaged(b: bytes, rng: random.Random) -> bytes:
    x = bytearray(b)
    k = rng.randrange(3)
    if k in (1, 2) and x:
        for _ in range(rng.randint(1, 3)):
            x[rng.randrange(len(x)) if rng.random() < 0.5 else rng.randrange(min(len(x), 48))] = rng.randrange(256)
    if k in (0, 2) and x:
        del x[rng.randrange(len(x)):]
    return bytes(x)


BAD = [-1, 1 << 32, 1.5, None, True, 'x', (1 << 32) - 1, 0]


def change_wave(w: Any, rng: random.Random) -> Any:
    g = copy.deepcopy(w)
    k = rng.randrange(6)
    if k == 0:
        setattr(g, rng.choice(['flags_id', 'total', 'intro', 'channels', 'outro', 'word14']), rng.choice(BAD))
    elif k == 1:
        segs = list(g.segments)
        segs[rng.randrange(3)] = rng.choice([None, (b'abc', b''), [b'x', b'yz'], (bytearray(b'q'), b''), (b'a',), 'ab'])
        g.segments = segs if rng.random() < 0.7 else tuple(segs)
    elif k == 2:
        g.segments = g.segments[:2] if rng.random() < 0.5 else g.segments + [None]
    elif k == 3:
        g.segments = collections.deque(g.segments)       # a sequence the port does not read: the Python
    else:
        segs = list(g.segments)
        i = rng.choice([j for j, s in enumerate(segs) if s is not None])
        segs[i] = (segs[i][0] + bytes(rng.randrange(5)), segs[i][1] + bytes(rng.randrange(5)))
        g.segments = segs
    return g


def change_stream(st: Any, rng: random.Random) -> Any:
    g = copy.deepcopy(st)
    k = rng.randrange(4)
    if k == 0:
        setattr(g, rng.choice(['version', 'codec', 'channels', 'rate', 'type', 'samples']),
                rng.choice(BAD + [0, 64, 1 << 18, 1 << 29]))
    elif k == 1 and g.blocks:
        i = rng.randrange(len(g.blocks))
        g.blocks[i] = rng.choice([(rng.choice(BAD), b'ab'), (5, bytearray(b'ab')), [6, b'cd'], (7,), (8, b'x' * 9)])
    elif k == 2:
        g.blocks = tuple(g.blocks)
    else:
        g.blocks = g.blocks + [(1, b'end')]
    return g


# ------------------------------------------------------------------------------------------------ the game's Waves

def bf1943_waves(limit: int | None) -> list[tuple[str, bytes, str, dict[int, bytes] | None]]:
    try:
        suite_paths.use('bf1943-import')
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                       # noqa: BLE001
        print(f'skip Battlefield 1943 ({e})')
        return []
    ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') == 'Wave')[:limit]
    out = []
    for k in ks:
        streams = {i: src.data(n) for i in range(3) for n in [f'{k[:-len("_wave.res")]}_wavedatasns_{i}.res']
                   if k.endswith('_wave.res') and n in src.entries}
        out.append((k, src.data(k), 'bf1943', streams or None))
    return out


def bundle_waves(pattern: str, source: str) -> list[tuple[str, bytes, str, dict[int, bytes] | None]]:
    """Every Wave entry of the bundles matching pattern, each name once."""
    where: dict[str, tuple[str, str]] = {}
    for p in sorted(glob.glob(pattern, recursive=True)):
        try:
            entries = fbrb.entries(p)
        except Exception:                                        # noqa: BLE001 - not a bundle this reader takes
            continue
        for e in entries:
            if e.type == 'Wave' and not e.deleted:
                where.setdefault(e.name.lower(), (p, e.name))
    by: dict[str, list[str]] = {}
    for p, n in where.values():
        by.setdefault(p, []).append(n)
    out: list[tuple[str, bytes, str, dict[int, bytes] | None]] = []
    for p, ns in sorted(by.items()):
        for n, b in sorted(fbrb.read(p, ns).items()):
            out.append((n.lower(), b, source, None))
    return out


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    if hasattr(audio, 'use_native'):
        c.check(audio.use_native(True), 'the Rust core runs audio.py\'s containers')
    rng = random.Random(0xF03)

    built = [wave(rng) for _ in range(300)]
    t = Tally()
    for i, (b, source, streams) in enumerate(built):
        wave_all(t, f'built {i} ({source})', b, source, streams)
    c.check(not t.ndiff and not t.fallback_ok, f'built Waves and streams both ways: {t.text()}', '; '.join(t.differ))

    td = Tally()
    for i in range(3000):
        b, source, streams = rng.choice(built)
        w = audio.read_wave(b, source)
        r = rng.random()
        if r < 0.5:
            both(td, f'damaged {i}', audio.read_wave, READ_WAVE_REF, lambda b, s: N.wave_read(b, s),
                 damaged(b, rng), source)
        elif r < 0.6:
            both(td, f'damaged {i} as another source', audio.read_wave, READ_WAVE_REF,
                 lambda b, s: N.wave_read(b, s), b, rng.choice(audio.SOURCES))
        else:
            seg = w.segments[1][0]
            sns = (streams or {}).get(1)
            if source == 'bf1943':
                both(td, f'damaged snr {i}', audio.read_snr, READ_SNR_REF, lambda d, x: N.snr_read(d, x),
                     damaged(seg, rng), None if sns is None else damaged(sns, rng))
            else:
                both(td, f'damaged sps {i}', audio.read_sps, READ_SPS_REF, N.sps_read,
                     damaged(sns if sns is not None and len(seg) == 4 else seg, rng))
    c.check(not td.ndiff, f'damaged Waves and streams: {td.text()}', '; '.join(td.differ))

    tc = Tally()
    waves = [audio.read_wave(b, s) for b, s, _ in built]
    streams_ = [audio.read_sps(s[0]) for w in waves if w.source != 'bf1943' for s in w.segments
                if s is not None and len(s[0]) > 8]
    for i in range(300):
        both(tc, f'changed wave {i}', audio.write_wave_pc, audio.write_wave_pc_reference, N.wave_write_pc,
             change_wave(rng.choice(waves), rng))
        both(tc, f'changed stream {i}', audio.write_sps, audio.write_sps_reference, N.sps_write,
             change_stream(rng.choice(streams_), rng))
    c.check(not tc.ndiff, f'changed Waves and streams written: {tc.text()}', '; '.join(tc.differ))

    items = bf1943_waves(None if game else 40)
    if game:
        opfor = paths.sibling(paths.OPFOR_DIR, suite=SUITE)
        items += bundle_waves(os.path.join(opfor, 'work', 'xbox', '**', '*.fbrb'), 'bfbc2-xbox')
        items += bundle_waves(os.path.join(opfor, 'game', '**', '*.fbrb'), 'bfbc2-pc')
    if items:
        tg, tgd = Tally(), Tally()
        tn = tr = 0.0
        kinds: dict[str, int] = {}
        for name, b, source, streams in items:
            kinds[source] = kinds.get(source, 0) + 1
            t0 = time.perf_counter()
            try:
                w = audio.read_wave(b, source)
                audio.write_wave_pc(w)
            except audio.AudioError:
                pass
            t1 = time.perf_counter()
            if R.live(audio, 'read_wave_reference'):
                try:
                    w = READ_WAVE_REF(b, source)
                    audio.write_wave_pc_reference(w)
                except audio.AudioError:
                    pass
            t2 = time.perf_counter()
            tn += t1 - t0
            tr += t2 - t1
            wave_all(tg, name, b, source, streams)
            frng = random.Random(len(b))
            for k in range(3 if game else 1):
                both(tgd, f'{name} damaged {k}', audio.read_wave, READ_WAVE_REF,
                     lambda b, s: N.wave_read(b, s), damaged(b, frng), source)
        what = ', '.join(f'{v:,} {k}' for k, v in sorted(kinds.items()))
        c.check(not tg.ndiff and not tg.fallback_ok, f'game Waves ({what}) and their streams read and written both ways: {tg.text()}',
                '; '.join(tg.differ))
        c.check(not tgd.ndiff, f'damaged game Waves: {tgd.text()}', '; '.join(tgd.differ))
        print(f'timing (read_wave + write_wave_pc): Python {tr:.2f} s, Rust {tn:.2f} s')
    c.check(R.finish(), 'the references: recorded, or replayed as recorded')
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
