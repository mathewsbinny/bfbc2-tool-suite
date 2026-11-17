r"""The Rust EA-XAS encoder and decoder (rust/bc2-convert audio, through the bc2_native module) against audio.py's
NumPy code they were ported from (xas_encode_frames_python, xas_decode_frames_python): the same bytes and samples.

  python test_audio_native.py          quick, no game data: the integer shift rule against NumPy's float formula for
                                       every residual to 2^26; seeded PCM of every shape (0, 1, 31 ... 20,000 samples
                                       a channel, 1 - 6 channels; silence, full scale, clipping extremes, tones, noise,
                                       impulses, other integer dtypes) encoded both ways; random frames (every
                                       coefficient set and shift field) decoded both ways; encode_xas in DICE's blocks
                                       at every rate; encode_xas_many and the dispatcher
  python test_audio_native.py --game   + every Wave of Battlefield 1943 (in memory, and streamed with its waveDataSns
                                       entries) and every in-memory Wave of BFBC2's unpacked Xbox bundles (OPFOR
                                       work\xbox): audio.convert with each backend, the PC Wave byte for byte (EA-XMA
                                       decoded once per sound, by ffmpeg; the reference takes a sound longer than 2^20
                                       samples a channel in pieces of whole frames, the same frames, to bound its
                                       memory: reference_xas); convert_many against convert. Skips what is
                                       missing. Options: --every N (every Nth sound of each source; the game tier
                                       takes every 10th, the full tier all), --limit N (sounds per source), --workers N
                                       (ffmpeg side by side)

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import argparse
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
from bfbc2core import dbx, fbrb, paths  # noqa: E402

NATIVE: Any = audio._with_xas(dbx._import_native())
FAILS: list[str] = []


def check(ok: bool, what: str) -> None:
    if not ok:
        FAILS.append(what)
        print(f'FAIL {what}', flush=True)


# ------------------------------------------------------------------------------------------------ quick

def shift_rule() -> None:
    """The port's integer shift (the least m with r <= 7 * 2^m) is NumPy's ceil(log2(max(r, 1) / 7.0)) for every
    residual r below 2^26 (the encoder's are below 2^25)."""
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


def same_frames(pcm: np.ndarray) -> tuple[bool, bool]:
    """(encoded alike, the frames decoded alike) by both backends."""
    py = audio.xas_encode_frames_python(pcm)
    rs = audio.xas_encode_frames(pcm)
    dec = audio.xas_decode_frames_python(py)
    return py.shape == rs.shape and py.tobytes() == rs.tobytes(), np.array_equal(dec, audio.xas_decode_frames(py))


def quick() -> None:
    assert audio.use_native(True) == 'rust'
    shift_rule()
    rng = np.random.default_rng(1943)
    lengths = [0, 1, 2, 31, 32, 33, 127, 128, 129, 255, 256, 257, 1000, 4096, 4113, 20000]
    n_in, n_samples, enc_bad, dec_bad = 0, 0, [], []
    for ch in (1, 2, 3, 6):
        for n in lengths:
            for name, pcm in signals(rng, n, ch):
                e, d = same_frames(pcm)
                n_in += 1
                n_samples += pcm.size
                if not e:
                    enc_bad.append(f'{name} {n} x {ch}')
                if not d:
                    dec_bad.append(f'{name} {n} x {ch}')
    check(not enc_bad, f'encode: {len(enc_bad)} of {n_in} differ: {enc_bad[:5]}')
    check(not dec_bad, f'decode of the encoded frames: {len(dec_bad)} of {n_in} differ: {dec_bad[:5]}')
    print(f'encode: {n_in} seeded signals ({n_samples:,} samples, 0 - 20,000 a channel, 1 - 6 channels) byte for byte '
          f'the same, their frames decoded alike')

    # other integer dtypes: within int16 the Rust core (the reference widens as it is), beyond it the reference
    base = signals(rng, 700, 2)[7][1]
    for dt in (np.int32, np.int64, np.uint8, np.int8, np.uint16):
        x = base.astype(dt) if np.issubdtype(dt, np.signedinteger) else np.abs(base).astype(dt)
        check(audio.xas_encode_frames(x).tobytes() == audio.xas_encode_frames_python(x).tobytes(), f'dtype {dt}')
    wide = base.astype(np.int32) * 3
    check(audio._pcm16(wide) is None, 'beyond int16: not given to the Rust core')
    check(audio.xas_encode_frames(wide).tobytes() == audio.xas_encode_frames_python(wide).tobytes(), 'beyond int16')
    check(audio._pcm16(base.astype(np.float32)) is None, 'float pcm: not given to the Rust core')
    fortran = np.asfortranarray(base)
    check(audio.xas_encode_frames(fortran).tobytes() == audio.xas_encode_frames_python(fortran).tobytes(), 'F order')
    for bad in (np.zeros(10, np.int16), np.zeros((2, 2, 2), np.int16)):
        errs: list[tuple[type, str] | None] = []
        for fn in (audio.xas_encode_frames, audio.xas_encode_frames_python):
            try:
                fn(bad)
                errs.append(None)
            except Exception as e:  # noqa: BLE001 - the same error both ways
                errs.append((type(e), str(e)))
        check(errs[0] == errs[1] and errs[0] is not None, f'shape {bad.shape}: {errs}')
    print('encode: int8 - int64 / uint8 / uint16 / Fortran order as the reference; beyond int16 and floats left to it')

    # random frames: every header field (coefficient sets 4 - 15 read through the rest of the table, every shift)
    f = rng.integers(0, 256, (5000, 76), dtype=np.uint8)
    check(np.array_equal(audio.xas_decode_frames(f), audio.xas_decode_frames_python(f)), 'decode: random frames')
    f3 = f[:4800].reshape(40, 30, 4, 76)
    check(np.array_equal(audio.xas_decode_frames(f3), audio.xas_decode_frames_python(f3)), 'decode: 4-D frames')
    check(audio.xas_decode_frames(f[:0]).shape == (0, 128), 'decode: no frames')
    print('decode: 5,000 random frames (every coefficient set and shift field) sample for sample the same')

    # encode_xas: DICE's blocks at each rate, a block multiple (empty last block) and not; the batch; decode_xas
    streams = 0
    for rate in (48000, 44100, 32000, 24000, 22050, 11025):
        per = audio.xas_block_samples(rate)
        for n in (0, 5, per - 1, per, per * 3, per * 3 + 129):
            for ch in (1, 2, 6):
                pcm = signals(rng, n, ch)[7][1]
                a, b = audio.encode_xas(pcm, rate), audio.encode_xas_python(pcm, rate)
                check(a.header() == b.header() and a.blocks == b.blocks, f'encode_xas {rate} {n} x {ch}')
                check(audio.write_sps(a) == audio.write_sps(b), f'write_sps {rate} {n} x {ch}')
                check(np.array_equal(audio.decode_xas(a), audio.decode_xas(b)), f'decode_xas {rate} {n} x {ch}')
                streams += 1
    items = [(signals(rng, n, ch)[i][1], rate) for i, n, ch, rate in
             [(1, 1000, 1, 48000), (5, 7777, 2, 44100), (0, 0, 1, 22050), (7, 129, 6, 11025)]]
    many = audio.encode_xas_many(items + [(items[1][0], 24000, 0)])
    one = [audio.encode_xas_python(p, r) for p, r in items] + [audio.encode_xas_python(items[1][0], 24000, 0)]
    check(all(a.header() == b.header() and a.blocks == b.blocks for a, b in zip(many, one)), 'encode_xas_many')
    print(f'encode_xas: {streams} streams in DICE\'s blocks at 6 rates as the reference; encode_xas_many as one by one')
    long = signals(rng, CHUNK + 4321, 2)[7][1]
    x, y = reference_xas(long, 48000), audio.encode_xas_python(long, 48000)
    check(x.header() == y.header() and x.blocks == y.blocks, 'the game test\'s reference by pieces = the reference')
    print('the reference by pieces (the game tier\'s long sounds) = the reference whole')

    # the dispatcher
    check(audio.use_native(False) == 'python' and audio.backend() == 'python', 'use_native(False)')
    pcm = signals(rng, 300, 2)[5][1]
    check(audio.xas_encode_frames(pcm).tobytes() == audio.xas_encode_frames_python(pcm).tobytes(), 'python backend')
    os.environ['BFBC2_PURE_PYTHON'] = '1'
    try:
        check(audio._load_native() is None, 'BFBC2_PURE_PYTHON=1: no Rust core')
        check(audio.use_native(True) == 'rust', 'use_native(True) whatever BFBC2_PURE_PYTHON says')
    finally:
        del os.environ['BFBC2_PURE_PYTHON']
    print('dispatcher: use_native / BFBC2_PURE_PYTHON')


# ------------------------------------------------------------------------------------------------ game

class Decoded:
    """audio.decode_xma remembered per stream: one ffmpeg run per sound for both backends."""

    def __init__(self, real: Any) -> None:
        self.real = real
        self.memo: dict[Any, np.ndarray] = {}

    def __call__(self, st: Any, trim: bool = True) -> np.ndarray:
        key = (st.channels, st.rate, st.samples, trim, tuple(b for _, b in st.blocks))
        if key not in self.memo:
            self.memo[key] = self.real(st, trim)
        return self.memo[key]


def prefetch(item: tuple[bytes, str, dict[int, bytes] | None]) -> Decoded:
    """The sound's EA-XMA segments decoded (ffmpeg; in a worker thread), as convert will ask for them."""
    b, source, streams = item
    dec = Decoded(DECODE_XMA)
    try:
        w = audio.read_wave(b, 'bfbc2-xbox' if source == 'bfbc2' else source)
        for i in range(3):
            if w.segments[i] is not None:
                st = audio.segment_stream(w, i, streams)
                if st.codec == 3 and st.channels == w.channels:
                    dec(st)
    except audio.AudioError:
        pass                                          # convert raises it again, with each backend
    return dec


DECODE_XMA, ENCODE_XAS = audio.decode_xma, audio.encode_xas
CHUNK = 1 << 20          # samples a channel: the reference takes a longer sound a piece at a time (its memory)


def reference_xas(pcm: np.ndarray, rate: int, type_: int = 1) -> Any:
    """audio.encode_xas_python, a long sound's frames made by the reference CHUNK samples (whole frames) at a time:
    every group of 32 samples is encoded on its own (the reference works row by row on them), so the frames are the
    same; its arrays (about 70 bytes a sample) stay below 1 GB."""
    n, ch = pcm.shape
    if n <= CHUNK:
        return audio.encode_xas_python(pcm, rate, type_)
    frames = np.concatenate([audio.xas_encode_frames_python(pcm[i:i + CHUNK]) for i in range(0, n, CHUNK)])
    per = audio.xas_block_samples(rate)
    full, f = n // per, per // 128
    blocks = [(per, frames[i * f:(i + 1) * f].tobytes()) for i in range(full)]
    blocks.append((n - full * per, frames[full * f:].tobytes()))
    return audio.Stream(1, 4, ch, rate, type_, n, blocks)


def both(item: tuple[bytes, str, dict[int, bytes] | None], dec: Decoded) -> tuple[Any, Any, float, float, int]:
    """(Python's result, Rust's, Python encode seconds, Rust encode seconds, samples): audio.convert with each
    backend (main thread only: it swaps audio's decode_xma for the remembered one and times encode_xas)."""
    out, secs = [], []
    for native in (False, True):
        audio.use_native(native)
        spent = [0.0]

        def timed(pcm: np.ndarray, rate: int, type_: int = 1, spent: list[float] = spent, native: bool = native) -> Any:
            t = time.perf_counter()
            st = ENCODE_XAS(pcm, rate, type_) if native else reference_xas(pcm, rate, type_)
            spent[0] += time.perf_counter() - t
            return st
        audio.decode_xma, audio.encode_xas = dec, timed
        try:
            r: Any = audio.convert(*item)
        except audio.AudioError as e:
            r = ('error', str(e))
        finally:
            audio.decode_xma, audio.encode_xas = DECODE_XMA, ENCODE_XAS
        out.append(r)
        secs.append(spent[0])
    audio.use_native(True)
    samples = sum(int(v.size) for k, v in dec.memo.items() if k[3])
    return out[0], out[1], secs[0], secs[1], samples


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


def game(every: int, limit: int | None, workers: int) -> None:
    try:
        audio.ffmpeg_path()
    except audio.AudioError as e:
        print(f'SKIP --game: {e}')
        return
    for label, get in (('Battlefield 1943', bf1943_items), ('BFBC2 Xbox, in memory', xbox_items)):
        items = get(every, limit)
        if not items:
            continue
        t0 = time.perf_counter()
        same, errs, diff, py_s, rs_s, samples = 0, 0, [], 0.0, 0.0, 0
        with ThreadPoolExecutor(workers) as ex:
            for at in range(0, len(items), 4 * workers):          # a few ahead: decoded audio is not all kept
                chunk = items[at:at + 4 * workers]
                for (name, it), dec in zip(chunk, ex.map(prefetch, [it for _, it in chunk])):
                    a, b, tp, tr, n = both(it, dec)
                    py_s, rs_s, samples = py_s + tp, rs_s + tr, samples + n
                    if a == b:
                        same += 1
                        errs += isinstance(a, tuple)
                    else:
                        diff.append(name)
        check(not diff, f'{label}: {len(diff)} of {len(items)} PC Waves differ: {diff[:5]}')
        print(f'{label}: {same} of {len(items)} Waves converted byte for byte the same by both backends ({errs} '
              f'refused alike; {samples:,} samples encoded); encode Python {py_s:.1f} s, Rust {rs_s:.2f} s '
              f'(one at a time), {time.perf_counter() - t0:.0f} s in all', flush=True)
        # the batch: convert_many against convert (Rust), some of them
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
    ap.add_argument('--every', type=int, default=1)
    ap.add_argument('--limit', type=int, default=None)
    ap.add_argument('--workers', type=int, default=4)
    a = ap.parse_args()
    if NATIVE is None:
        print('SKIP: the Rust core (bc2_native) is not built: python rust\\build_native.py')
        return 0
    quick()
    if a.game:
        audio.use_native(True)
        game(a.every, a.limit, a.workers)
    peak = peak_gb()
    if peak:
        print(f'peak memory {peak:.2f} GB')
    print('FAILED: ' + '; '.join(FAILS) if FAILS else 'ok')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
