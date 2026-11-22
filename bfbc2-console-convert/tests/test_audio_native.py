r"""audio.py's EA-XAS encoder and decoder (the Rust core: rust/bc2-convert audio, through bc2_native) against
results recorded from the NumPy code they were ported from.

The NumPy encoder and decoder (xas_encode_frames_python, xas_decode_frames_python, encode_xas_python) were the
reference until 2026-11-18, when the port had passed the full test tier and they were retired (rust/BENCHMARKS.md,
"Retired Python"). Their results were recorded first, from the NumPy code itself, in audio_golden.json beside this
file: each case's output SHA-1 (or its exception) under "quick", "bf1943" and "xbox". Where the Rust core now answers
otherwise on purpose (an input the NumPy code took and the port refuses with a clear error), the new answer is
under "now", case by case; every other case must give the recorded answer.

  python test_audio_native.py          quick, no game data: the integer shift rule against NumPy's float formula for
                                       every residual to 2^26; seeded PCM of every shape (0, 1, 31 ... 20,000 samples
                                       a channel, 1 - 6 channels; silence, full scale, clipping extremes, tones, noise,
                                       impulses, other integer dtypes, Fortran order) encoded and the frames decoded;
                                       random frames (every coefficient set and shift field) decoded; encode_xas in
                                       DICE's blocks at every rate; encode_xas_many against encode_xas; the inputs
                                       refused (beyond int16, floats, wrong shapes)
  python test_audio_native.py --game   + every Wave of Battlefield 1943 (in memory, and streamed with its waveDataSns
                                       entries) and every in-memory Wave of BFBC2's unpacked Xbox bundles (OPFOR
                                       work\xbox): audio.convert's PC Wave against the recording (EA-XMA decoded in
                                       the Rust core since round PR1b-1c; recorded again then, "now") and, with ffmpeg
                                       there, against convert_reference on ffmpeg's C path (XMA-DEC-1); convert_many
                                       against convert. Skips what is missing. Options:
                                       --every N (every Nth sound of each source; the game tier takes every 10th,
                                       the full tier all), --limit N (sounds per source), --workers N (sounds side by
                                       side)
  python test_audio_native.py --record [--game]
                                       write the "now" answers from the current code where they differ from the
                                       recording: only after a deliberate change, proven on the game's sounds
                                       (tests\test_audio.py) - the recording is the only oracle

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import argparse
import hashlib
import json
import os
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(HERE))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core')
import audio  # noqa: E402
from bfbc2core import fbrb, paths  # noqa: E402

GOLDEN = os.path.join(HERE, 'audio_golden.json')
FAILS: list[str] = []


def check(ok: bool, what: str) -> None:
    if not ok:
        FAILS.append(what)
        print(f'FAIL {what}', flush=True)


def sha1(b: bytes) -> str:
    """The first 16 hex digits of b's SHA-1."""
    return hashlib.sha1(b).hexdigest()[:16]


def outcome(fn: Any, *a: Any) -> list[str]:
    """['ok', SHA-1 of fn's bytes] or ['error', exception type, message]."""
    try:
        r = fn(*a)
    except Exception as e:                                       # noqa: BLE001 - recorded, compared
        return ['error', type(e).__name__, str(e)]
    return ['ok', sha1(r)]


# ------------------------------------------------------------------------------------------------ the recording

def load() -> dict[str, Any]:
    try:
        with open(GOLDEN, encoding='utf-8') as f:
            return dict(json.load(f))
    except FileNotFoundError:
        return {}


def write(golden: dict[str, Any]) -> None:
    """audio_golden.json, one case per line (a change shows as the lines of the cases it changed)."""
    lines = ['{']
    keys = sorted(golden)
    for i, k in enumerate(keys):
        v, end = golden[k], (',' if i < len(keys) - 1 else '')
        if isinstance(v, dict) and v:
            rows = [f'{json.dumps(n)}: {json.dumps(v[n])}' for n in sorted(v)]
            lines += [f'{json.dumps(k)}: {{', *[r + ',' for r in rows[:-1]], *rows[-1:], '}' + end]
        else:
            lines.append(f'{json.dumps(k)}: {json.dumps(v)}{end}')
    lines.append('}')
    with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')


class Recorded:
    """Each case's recorded answer ("now" over the reference's) against the current one: counts, the first
    differences reported; with record, the current answers that differ from the reference's go to "now"."""

    def __init__(self, golden: dict[str, Any], group: str, record: bool):
        self.ref: dict[str, Any] = golden.get(group, {})
        self.now: dict[str, Any] = golden.setdefault('now', {})
        self.group, self.record = group, record
        self.n = self.same = self.differ = self.missing = self.changed = 0

    def __call__(self, name: str, got: list[str], input_sha: str | None = None) -> None:
        key = f'{self.group}/{name}'
        ref = self.ref.get(name)
        if ref is None:
            self.missing += 1
            check(False, f'{key}: not recorded')
            return
        if input_sha is not None:
            if ref[0] != input_sha:
                self.missing += 1
                check(False, f'{key}: the input is not the recorded one (check it, then re-record)')
                return
            ref = ref[1:]
        self.n += 1
        if self.record:
            if got != ref:
                self.now[key] = got
            else:
                self.now.pop(key, None)
            return
        want = self.now.get(key, ref)
        self.changed += key in self.now
        if got == want:
            self.same += 1
        else:
            self.differ += 1
            if self.differ <= 10:
                check(False, f'{key}: {got} != recorded {want}')

    def report(self, what: str) -> str:
        if self.record:
            return f'{what}: {self.n} cases'
        check(self.differ == 0 and self.missing == 0, f'{what}: {self.differ} of {self.n} differ from the recording, '
                                                      f'{self.missing} not recorded')
        return (f'{what}: {self.same} of {self.n} as recorded ({self.changed} of them answered as "now" records, not '
                'as the NumPy code did)')


# ------------------------------------------------------------------------------------------------ quick

def shift_rule() -> None:
    """The port's integer shift (the least m with r <= 7 * 2^m) is NumPy's ceil(log2(max(r, 1) / 7.0)) for every
    residual r below 2^26 (the encoder's are below 2^25): the float step of the retired encoder the port does in
    integers."""
    bad, step = 0, 1 << 22
    for lo in range(0, 1 << 26, step):
        r = np.arange(lo, lo + step, dtype=np.int64)
        m = np.ceil(np.log2(np.maximum(r, 1) / 7.0)).astype(np.int64)
        r4 = np.maximum(r, 1) * 4
        ok = (7 << (m + 2)) >= r4                                  # 7 * 2^m >= r
        ok &= np.where(m > -2, (7 << np.maximum(m + 1, 0)) < r4, True)   # 7 * 2^(m-1) < r (m = -2: 0.875 < 1)
        bad += int(np.count_nonzero(~ok))
    check(bad == 0, f'shift rule: {bad} residuals below 2^26 where NumPy\'s float formula differs')
    print(f'shift rule: NumPy\'s ceil(log2(r / 7)) = the integer rule for all {1 << 26:,} residuals below 2^26')


def signals(rng: np.random.Generator, n: int, ch: int) -> list[tuple[str, np.ndarray]]:
    t = np.arange(n)[:, None] * (1 + np.arange(ch))[None, :]
    out = [
        ('silence', np.zeros((n, ch), np.int16)),
        ('full-scale noise', rng.integers(-32768, 32768, (n, ch)).astype(np.int16)),
        ('clipping extremes', np.where((t // 3) % 2 == 0, 32767, -32768).astype(np.int16)),
        ('+32767', np.full((n, ch), 32767, np.int16)),
        ('-32768', np.full((n, ch), -32768, np.int16)),
        ('tone', (np.sin(t * 0.031) * 20000).astype(np.int16)),
        ('loud tone, clipped', np.clip(np.sin(t * 0.007) * 60000, -32768, 32767).astype(np.int16)),
        ('tone and noise', np.clip(np.sin(t * 0.2) * 9000 + rng.normal(0, 800, (n, ch)), -32768, 32767).astype(np.int16)),
        ('quiet noise', rng.integers(-4, 5, (n, ch)).astype(np.int16)),
        ('square', np.where((t // 50) % 2 == 0, 12000, -12000).astype(np.int16)),
        ('impulses', np.where(rng.random((n, ch)) < 0.01, rng.integers(-32768, 32768, (n, ch)), 0).astype(np.int16)),
        ('random walk', np.clip(np.cumsum(rng.integers(-900, 901, (n, ch)), 0), -32768, 32767).astype(np.int16)),
    ]
    return out


def stream_bytes(st: Any) -> bytes:
    """A Stream as its SPS bytes and its blocks' sample counts and lengths."""
    return audio.write_sps(st) + repr([(n, len(b)) for n, b in st.blocks]).encode()


def quick_cases(codec: Any) -> list[tuple[str, Any, tuple]]:
    """(name, function, arguments) of the quick tier, always the same (seed 1943, the draws in the order of the
    Python-vs-Rust test before 2026-11-18). codec: (encode frames, decode frames, encode_xas)."""
    enc, dec, enc_xas = codec
    rng = np.random.default_rng(1943)
    out: list[tuple[str, Any, tuple]] = []
    for ch in (1, 2, 3, 6):
        for n in [0, 1, 2, 31, 32, 33, 127, 128, 129, 255, 256, 257, 1000, 4096, 4113, 20000]:
            for name, pcm in signals(rng, n, ch):
                out.append((f'encode {name} {n} x {ch}', lambda p: enc(p).tobytes(), (pcm,)))
                out.append((f'decode of the frames of {name} {n} x {ch}', lambda p: dec(enc(p)).tobytes(), (pcm,)))
    # other integer dtypes and Fortran order: within int16, encoded as int16; beyond int16 and floats: refused now
    base = signals(rng, 700, 2)[7][1]
    for dt in (np.int32, np.int64, np.uint8, np.int8, np.uint16):
        x = base.astype(dt) if np.issubdtype(dt, np.signedinteger) else np.abs(base).astype(dt)
        out.append((f'encode dtype {np.dtype(dt).name}', lambda p: enc(p).tobytes(), (x,)))
    out.append(('encode beyond int16', lambda p: enc(p).tobytes(), (base.astype(np.int32) * 3,)))
    out.append(('encode float32', lambda p: enc(p).tobytes(), (base.astype(np.float32),)))
    out.append(('encode Fortran order', lambda p: enc(p).tobytes(), (np.asfortranarray(base),)))
    out.append(('encode 1-D', lambda p: enc(p).tobytes(), (np.zeros(10, np.int16),)))
    out.append(('encode 3-D', lambda p: enc(p).tobytes(), (np.zeros((2, 2, 2), np.int16),)))
    # random frames: every header field (coefficient sets 4 - 15 read through the rest of the table, every shift)
    f = rng.integers(0, 256, (5000, 76), dtype=np.uint8)
    out.append(('decode 5,000 random frames', lambda x: dec(x).tobytes(), (f,)))
    out.append(('decode random frames, 4-D', lambda x: dec(x).tobytes() + repr(dec(x).shape).encode(),
                (f[:4800].reshape(40, 30, 4, 76),)))
    out.append(('decode no frames', lambda x: dec(x).tobytes() + repr(dec(x).shape).encode(), (f[:0],)))
    # encode_xas: DICE's blocks at each rate, a block multiple (empty last block) and not
    for rate in (48000, 44100, 32000, 24000, 22050, 11025):
        per = audio.xas_block_samples(rate)
        for n in (0, 5, per - 1, per, per * 3, per * 3 + 129):
            for ch in (1, 2, 6):
                pcm = signals(rng, n, ch)[7][1]
                out.append((f'encode_xas {rate} {n} x {ch}', lambda p, r: stream_bytes(enc_xas(p, r)), (pcm, rate)))
    return out


def quick(golden: dict[str, Any], record: bool) -> None:
    audio.require()
    shift_rule()
    rec = Recorded(golden, 'quick', record)
    t = time.perf_counter()
    cases = quick_cases((audio.xas_encode_frames, audio.xas_decode_frames, audio.encode_xas))
    for name, fn, args in cases:
        rec(name, outcome(fn, *args))
    print(rec.report(f'{len(cases)} seeded encodes, decodes and streams ({time.perf_counter() - t:.1f} s)'))

    # the batch: encode_xas_many as encode_xas one by one
    rng = np.random.default_rng(7)
    items = [(signals(rng, n, ch)[i][1], rate) for i, n, ch, rate in
             [(1, 1000, 1, 48000), (5, 7777, 2, 44100), (0, 0, 1, 22050), (7, 129, 6, 11025)]]
    many = audio.encode_xas_many(items + [(items[1][0], 24000, 0)])
    one = [audio.encode_xas(p, r) for p, r in items] + [audio.encode_xas(items[1][0], 24000, 0)]
    check(all(a.header() == b.header() and a.blocks == b.blocks for a, b in zip(many, one)), 'encode_xas_many')
    # the refusals say what is wrong
    for bad, says in ((np.zeros((4, 2), np.float64), 'integer'), (np.full((4, 1), 40000, np.int32), 'int16'),
                      (np.zeros(4, np.int16), '(samples, channels)')):
        try:
            audio.xas_encode_frames(bad)
            check(False, f'{bad.dtype} {bad.shape}: refused')
        except ValueError as e:
            check(says in str(e), f'{bad.dtype} {bad.shape}: the error says {says!r} ({e})')
    try:
        audio.xas_decode_frames(np.zeros((3, 75), np.uint8))
        check(False, 'frames of 75 bytes: refused')
    except ValueError as e:
        check('76' in str(e), f'frames of 75 bytes: the error says 76 ({e})')
    print('encode_xas_many as encode_xas one by one; the refused inputs say why')


# ------------------------------------------------------------------------------------------------ game

def both_ways(item: tuple[bytes, str, dict[int, bytes] | None], ff: bool) -> tuple[list[str], bool | None]:
    """audio.convert's outcome (the Rust core: EA-XMA decoded in process, XMA-DEC-1) and, with ffmpeg there, whether
    convert_reference with ffmpeg's C path (-cpuflags 0) gives the same bytes or the same error."""
    got = outcome(audio.convert, *item)
    if not ff:
        return got, None
    return got, outcome(lambda *a: audio.convert_reference(*a, ffmpeg=True), *item) == got


def item_sha(item: tuple[bytes, str, dict[int, bytes] | None]) -> str:
    """The SHA-1 of a sound's input: the Wave and its streamed segments."""
    b, _, streams = item
    h = hashlib.sha1(b)
    for i, s in sorted((streams or {}).items()):
        h.update(f'|{i}|'.encode() + s)
    return h.hexdigest()[:16]


def bf1943_items(every: int, limit: int | None) -> list[tuple[str, tuple[bytes, str, dict[int, bytes] | None]]]:
    try:
        suite_paths.use('bf1943-import')
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP Battlefield 1943: {e}')
        return []
    ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') == 'Wave')[::every][:limit]
    out = []
    for k in ks:
        streams = {i: src.data(n) for i in range(3) for n in [f'{k[:-len("_wave.res")]}_wavedatasns_{i}.res']
                   if k.endswith('_wave.res') and n in src.entries}
        out.append((k, (src.data(k), 'bf1943', streams or None)))
    return out


def xbox_items(every: int, limit: int | None) -> list[tuple[str, tuple[bytes, str, dict[int, bytes] | None]]]:
    """BFBC2's in-memory Xbox Waves (the streamed ones' audio is not in the unpacked files), each name once (the
    first bundle holding it), read one bundle at a time."""
    import glob
    opfor = paths.sibling(paths.OPFOR_DIR, suite=_ROOT)
    where: dict[str, tuple[str, str]] = {}
    for p in sorted(glob.glob(os.path.join(opfor, 'work', 'xbox', '**', '*.fbrb'), recursive=True)):
        try:
            entries = fbrb.entries(p)
        except Exception:                                        # noqa: BLE001 - not a bundle this reader takes
            continue
        for e in entries:
            if e.type == 'Wave' and not e.deleted:
                where.setdefault(e.name.lower(), (p, e.name))
    if not where:
        print('SKIP BFBC2 Xbox: no unpacked Xbox bundles')
        return []
    by: dict[str, list[str]] = {}
    for p, n in where.values():
        by.setdefault(p, []).append(n)
    out: list[tuple[str, tuple[bytes, str, dict[int, bytes] | None]]] = []
    for p, ns in sorted(by.items()):
        for n, b in sorted(fbrb.read(p, ns).items()):
            try:
                w = audio.read_wave(b, 'bfbc2-xbox')
            except audio.AudioError:
                continue
            if all(s is None or len(s[0]) > 8 for s in w.segments):
                out.append((n.lower(), (b, 'bfbc2', None)))
    out.sort()
    return out[::every][:limit]


def game(golden: dict[str, Any], record: bool, every: int, limit: int | None, workers: int) -> None:
    try:
        audio.ffmpeg_path()
        ff = True
    except audio.AudioError as e:
        print(f'no ffmpeg ({e}): the conversions are checked against the recording only')
        ff = False
    for group, label, get in (('bf1943', 'Battlefield 1943', bf1943_items), ('xbox', 'BFBC2 Xbox, in memory', xbox_items)):
        items = get(every, limit)
        if not items:
            continue
        t0 = time.perf_counter()
        rec = Recorded(golden, group, record)
        same = 0
        with ThreadPoolExecutor(workers) as ex:
            for (name, it), (got, eq) in zip(items, ex.map(lambda x: both_ways(x[1], ff), items)):
                rec(name, got, item_sha(it))
                same += bool(eq)
        if ff:
            check(same == len(items), f'{label}: convert = convert_reference with ffmpeg -cpuflags 0 on {same} of '
                  f'{len(items)} Waves (XMA-DEC-1)')
        print(rec.report(f'{label}: {len(items)} Waves converted ({time.perf_counter() - t0:.0f} s)'), flush=True)
        # the batch: convert_many against convert, some of them
        some = [it for _, it in items[:: max(1, len(items) // 40)]]
        many = audio.convert_many(some, workers)
        one = []
        for it in some:
            try:
                one.append(audio.convert(*it))
            except audio.AudioError as e:
                one.append(e)
        check(all(type(a) is type(b) and (a == b if isinstance(a, bytes) else str(a) == str(b))
                  for a, b in zip(many, one)), f'{label}: convert_many differs from convert')
        print(f'{label}: convert_many = convert on {len(some)} Waves')


def peak_gb() -> float | None:
    """This process's peak working set in GB (Windows), else None."""
    if os.name != 'nt':
        return None
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD), ('PeakWorkingSetSize', ctypes.c_size_t),
                    ('WorkingSetSize', ctypes.c_size_t), ('QuotaPeakPagedPoolUsage', ctypes.c_size_t),
                    ('QuotaPagedPoolUsage', ctypes.c_size_t), ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t),
                    ('QuotaNonPagedPoolUsage', ctypes.c_size_t), ('PagefileUsage', ctypes.c_size_t),
                    ('PeakPagefileUsage', ctypes.c_size_t)]
    c = Counters()
    c.cb = ctypes.sizeof(c)
    k32 = ctypes.WinDLL('kernel32')
    k32.GetCurrentProcess.restype = wintypes.HANDLE
    psapi = ctypes.WinDLL('psapi')
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    if not psapi.GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb):
        return None
    return float(c.PeakWorkingSetSize) / 2 ** 30


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--game', action='store_true')
    ap.add_argument('--record', action='store_true')
    ap.add_argument('--every', type=int, default=1)
    ap.add_argument('--limit', type=int, default=None)
    ap.add_argument('--workers', type=int, default=4)
    a = ap.parse_args()
    golden = load()
    quick(golden, a.record)
    if a.game:
        game(golden, a.record, a.every, a.limit, a.workers)
    if a.record and not FAILS:
        golden['now_recorded'] = time.strftime('%Y-%m-%d')
        write(golden)
        print(f'wrote {GOLDEN}')
    peak = peak_gb()
    if peak:
        print(f'peak memory {peak:.2f} GB')
    print('FAILED: ' + '; '.join(FAILS) if FAILS else 'ok')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
