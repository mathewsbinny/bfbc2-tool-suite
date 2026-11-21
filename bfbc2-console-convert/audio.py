r"""audio.py - Xbox 360 sounds (Frostbite "Wave" resources, EA-XMA audio) as BFBC2's PC Wave resources (EA-XAS audio);
BFBC2's own Xbox files and Battlefield 1943's.

    import audio
    w = audio.read_wave(xbox_bytes, 'bfbc2-xbox')        # or 'bfbc2-pc', 'bf1943'
    pcm, rate = audio.decode(xbox_bytes, 'bf1943')       # int16 (samples, channels); EA-XMA through ffmpeg
    pc = audio.convert(xbox_bytes, 'bfbc2')              # BFBC2 PC Wave, EA-XAS, in memory
    pc = audio.convert(bf1943_stub, 'bf1943', streams={1: sns_bytes})   # a streamed sound (voice-over)

    pcs = audio.convert_many([(xbox_bytes, 'bfbc2', None), ...])  # many at once (ffmpeg and the encoder side by side)

Needs numpy, and ffmpeg (decoding only: EA-XMA as XMA2; on PATH, in $FFMPEG, or the WinGet Gyan.FFmpeg package) for
anything that reads EA-XMA. The EA-XAS encoder and decoder are this module's own and run in the suite's Rust core
(rust/bc2-convert's audio, through bfbc2core's bc2_native), which they need: without the module (or with one built
from other sources) they raise bfbc2core.native.NativeMissing saying how to get it (README.md, "The Rust core");
BFBC2_PURE_PYTHON does not change that. The NumPy codec that was their reference was retired on 2026-11-18, after the
port had given the same bytes on every sound of BFBC2's Xbox copy and Battlefield 1943 and passed the full test tier;
its results are recorded in tests\audio_golden.json, which tests\test_audio_native.py checks. The block framing
(DICE's blocks) stays here. The Wave resource and the EA stream framing run in the Rust core too (round F0:
bc2_native.wave_read and its kin, the same values and bytes). The readers (read_wave, read_sps, read_snr) run there
only: round RT3 ported their errors (AudioError, and struct.error where a size word lay past the end, in the
reference's words) and NaN rates, then retired read_wave_reference / read_sps_reference / read_snr_reference against
their recorded answers (tests\f0_wave_replay*.json). The writers (write_wave_pc, write_sps) run there when it is built;
where the reference would raise, the Python runs (write_wave_pc_reference, write_sps_reference: kept as the reference
and the fallback for the objects the converters build; BFBC2_PURE_PYTHON=1 or use_native(False) runs them only).
Proven in tests\test_audio.py on BFBC2's Xbox / PC twins and on Battlefield 1943's sounds. The EA-XMA parts, the XMA2 RIFF and the EA-XAS blocks (xma_parts, xma2_riff, _xas_stream's
blocks) run in the Rust core only (round F3: bc2_native.audio_xma_parts, audio_xma2_riff, audio_xas_blocks, with the
reference's errors and messages since round RT2); their Python (xma_parts_reference, xma2_riff_reference,
_xas_blocks_reference) was retired in round RT2 against its recorded answers (tests\f3_golden_audio.json). An
argument only the reference took (a float or None where a count goes, a negative sample count) raises
bfbc2core.native.Refused (a ValueError).

Frostbite Wave resource, BFBC2 (Xbox big-endian, PC little-endian; the words are the same on both, 35,811 twins):
    0x00  u32 flags << 16 | id    flags 1 - 4, 0x13, 0x14, 0x23, 0x24, 0x31; id 0 or a 16-bit value (0xda91 on
                                  voice-over and most streamed effects, 0xfe2d ambiences, 0x4d87 music, 0x3067); not
                                  identified, equal on both platforms in every twin
    0x04  u32 total samples       intro + loop + outro
    0x08  u32 intro samples       0: no intro segment
    0x0C  u32 channels            1, 2, 6
    0x10  u32 outro samples       0: no outro segment
    0x14  u32                     0 (33,869 files), 1 - 7 elsewhere; not identified, equal on both platforms
    then three segments, intro, loop, outro, each:
          u32 size                0: no segment (always present for the loop)
          size bytes              the audio in memory: an EA SPS stream (below), or for a streamed segment the u32
                                  sample count only (the audio is <name>_waveDataSns_<0|1|2>.res, a non-resource
                                  entry of streaming_sounds / streaming_vo_<lang>: 1 = loop, 0 intro, 2 outro)
          u32 m, m bytes          stream information (0 for in-memory audio): EA's stream header and a seek table
                                  for a streamed segment (or one whose head is in memory); its layout differs per
                                  platform (the Xbox's starts 01 10 01 80, the PC's c0 06 10 01 ...), not decoded
    every blob padded to 4 bytes with the bytes ad de ad (the Xbox leaves the last one unpadded in 9 files).
    The twins: every word equal (one file's total differs by DICE's edit); sizes differ (codec), and which segments
    are streamed / preloaded differs in a few looping vehicle sounds (DICE re-split them for the PC).
    The PC's loop segments: 34,038 streamed stubs (most 44-byte files: all voice-over, long sounds), 94 streamed with
    stream information (the 47 six-channel ambiences among them), 1,618 in memory, 61 in memory with stream
    information (the head of a streamed sound). Voice-over is the same size on both platforms because its Wave is
    such a stub: the audio is in streaming_vo_<lang> / streaming_sounds (on the PC EALayer3; the Xbox's streaming
    bundles are not in the unpacked console files).

Battlefield 1943's (big-endian, 2,750 files):
    0x00  f32 sample rate         48000.0 (0x473b8000), 44100.0, 32000.0, 24000.0, 11025.0
    0x04  u32 2
    0x08  u32 channels            1, 2
    0x0C  u32 total samples
    0x10  u32 loop start          = intro samples
    0x14  u32 loop end            = intro + loop samples when there is an outro; = total on 6 looping idles; else 0
    0x18  u32 flags << 16 | id    as BFBC2's word 0x00 (flags 1 - 4, 0x12, 0x22, 0x32; ids 0xda91, 0xfe2d, 0x4d87)
    then the three segments as BFBC2's, the audio as EA SNR / SNS v0 (below): in memory the whole stream; streamed
    (1,536 voice-over files, 27 others) the 8-byte SNR header only, the blocks in streaming_sounds-00.fbrb
    (<name>_waveDataSns_1.res).

EA audio core stream (always big-endian):
    header word   version << 28 | codec << 24 | (channels - 1) << 18 | sample rate (18 bits)
                  version 0 (BF1943) / 1 (BFBC2); codec 2 PCM16BE, 3 EA-XMA, 4 EA-XAS, 5 - 7 EALayer3
    sample word   type << 30 | loop << 29 | samples   type 0 in memory (BF1943), 1 stream / SPS (all of BFBC2);
                  loop 1: a u32 loop start follows (no BFBC2 or BF1943 Wave sets it)
    v1 (SPS, BFBC2): blocks of u8 kind, u24 size (with the 4 bytes): 0x48 the header (the two words), 0x44 audio,
                  0x45 the end (size 4)
    v0 (SNR / SNS, BF1943): the two header words, then blocks of u32 flags << 24 | size: in memory one block, flags 0;
                  streamed, flags 0x80 on the last
    an audio block: u32 samples, then the codec's data.
    Xbox: EA-XMA. The PC: EA-XAS in memory (all 1,846 in-memory streams), EALayer3 v2 (codec 6) streamed.

EA-XMA block data: per XMA stream (2 channels each, the last 1 for an odd count) u32 n << 2 | 0 - 3 (n: the bytes of
    this part, the u32 included), then n - 4 bytes of XMA2 packets (2,048-byte packets, the last cut short). BFBC2's
    blocks hold one packet per stream (~4,096 - 13,000 samples), BF1943's in-memory sounds one block of all packets;
    BF1943's streams end their last block with zero bytes after the parts (1,539 of 1,563).
    Decoded by padding each part to whole packets and wrapping each stream as a RIFF WAVE with an XMA2WAVEFORMATEX
    (format 0x166) for ffmpeg's xma2 decoder; it gives the header's sample count.

EA-XAS v1 (PC) block data: frames of 128 samples per channel, interleaved (frame 0 of each channel, then frame 1 ...),
    76 bytes a channel:
          4 x u32 LE      a group of 32 samples each: bits 4-15 its first sample (bits 0-3: coefficient set 0 - 3),
                          bits 20-31 its second (bits 16-19: shift k)
          15 x 4 bytes    row r, byte g: group g's samples 2 + 2r (high nibble) and 3 + 2r (low nibble)
          sample = clip16((nibble << (20 - k)) + s[-1] * c0 + s[-2] * c1 + 128 >> 8),
          (c0, c1) = (0, 0), (240, 0), (460, -208), (392, -220)
    DICE's layout (all 1,846 PC streams): every block but the last 256 * ceil(rate / 1280) samples (76 frames at
    48 kHz, 70 at 44.1, 38 at 24, 50 at 32, 36 at 22.05, 18 at 11.025 kHz), the last the rest (whole frames; empty,
    0 samples, when the count is a multiple of the block: 3 files), then the end block.
    The encoder: per group the coefficient set and shift with the least squared error, nibbles rounded to nearest
    (the same result as trying all 64 pairs); 41.6 dB SNR on the end credits' music, ~20 - 25 dB on noise-like
    high-frequency effects (the limit of 4-bit ADPCM there).

Converted resources keep the source's words (BF1943's mapped: intro = loop start, outro = total - loop end) and hold
all audio in memory as EA-XAS (a streamed source needs its waveDataSns entry: `streams`). Word 0x14 is 0 for BF1943.
"""
import os
import shutil
import struct
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np

PAD = b'\xad\xde\xad'
XMA_PACKET = 2048
XAS_FRAME = 76
COEFS = np.array([0, 240, 460, 392, 0, 0, -208, -220, 0, 1, 3, 4, 7, 8, 10, 11, 0, -1, -3, -4], np.int64)
CODECS = {2: 'PCM16BE', 3: 'EA-XMA', 4: 'EA-XAS', 5: 'EALayer3 v1', 6: 'EALayer3 v2 PCM', 7: 'EALayer3 v2 Spike'}
SOURCES = ('bfbc2-xbox', 'bfbc2-pc', 'bf1943')


class AudioError(Exception):
    what_to_do = ('Check that the resource is a Wave from BFBC2 (Xbox 360 or PC) or Battlefield 1943 (Xbox 360); a '
                  'streamed sound needs its <name>_waveDataSns_<n>.res from the streaming bundle; decoding EA-XMA '
                  'needs ffmpeg (PATH or $FFMPEG).')


# ---------------------------------------------------------------- the Rust core (the EA-XAS codec)

def require():
    """The Rust core, which encodes and decodes EA-XAS (bfbc2core.native.require): NativeMissing, saying how to get
    it, when it is not built or downloaded or was built from other sources. A tool that converts sounds calls this
    when it starts, so it stops before doing other work."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('EA-XAS audio (bfbc2-console-convert/audio.py)', 'xas_encode_many')


_NATIVE: list[Any] = []


def _native() -> Any:
    """The Rust core with wave_read (the container readers), or None: not built, or BFBC2_PURE_PYTHON set."""
    if not _NATIVE:
        m = None
        try:
            root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
            if root not in sys.path:
                sys.path.insert(0, root)
            import suite_paths
            suite_paths.use('core')
            from bfbc2core import dbx as _dbx
            if os.environ.get(_dbx.PURE_PYTHON_ENV, '') in ('', '0'):
                m = _dbx._import_native()
        except ImportError:
            m = None
        _NATIVE.append(m if m is not None and hasattr(m, 'wave_read') else None)
    return _NATIVE[0]


def use_native(on: bool) -> bool:
    """The Rust container readers on (when built, whatever BFBC2_PURE_PYTHON says) or off for this process; True when
    they run. The EA-XAS codec always runs in the Rust core."""
    _NATIVE.clear()
    if not on:
        _NATIVE.append(None)
        return False
    saved = os.environ.pop('BFBC2_PURE_PYTHON', None)
    try:
        return _native() is not None
    finally:
        if saved is not None:
            os.environ['BFBC2_PURE_PYTHON'] = saved


def _pcm16(pcm):
    """pcm as C-ordered little-endian int16 (samples, channels), the samples the Rust core encodes: a 2-D integer
    array within int16 (other integer types are cast); ValueError otherwise (the retired NumPy encoder widened
    anything to int64 and clipped its own way - no caller passes such samples)."""
    if not isinstance(pcm, np.ndarray) or pcm.ndim != 2:
        raise ValueError(f'EA-XAS encode: pcm must be a (samples, channels) array, not {getattr(pcm, "shape", type(pcm))}')
    if pcm.dtype.kind not in 'iu':
        raise ValueError(f'EA-XAS encode: integer samples within int16 needed, not {pcm.dtype}')
    if pcm.dtype != np.int16 and pcm.size and (pcm.min() < -32768 or pcm.max() > 32767):
        raise ValueError(f'EA-XAS encode: samples beyond int16 ({pcm.min()} .. {pcm.max()})')
    return np.ascontiguousarray(pcm, '<i2')


# ---------------------------------------------------------------- the Frostbite wrapper

class Wave:
    """A Wave resource: the header words and three segments (intro, loop, outro), each None or (data, info)."""

    def __init__(self, source, flags_id, total, intro, channels, outro, word14, segments, rate=None, word04=None,
                 loop_end=None):
        self.source, self.flags_id, self.total, self.intro, self.channels = source, flags_id, total, intro, channels
        self.outro, self.word14, self.segments = outro, word14, segments
        self.rate, self.word04, self.loop_end = rate, word04, loop_end      # BF1943 only

    def words(self):
        return [self.flags_id, self.total, self.intro, self.channels, self.outro, self.word14]


def _reader():
    """The Rust core's container readers (round RT3: their Python, read_wave_reference / read_sps_reference /
    read_snr_reference, was retired): NativeMissing, saying how to get it, without the module."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('Wave resources (bfbc2-console-convert/audio.py)', 'wave_read')


def _bytes_of(b, what):
    """b as bytes (a bytearray or memoryview reads as its bytes, as the reference read it); another type raises
    Refused (the retired reference raised its own TypeError there)."""
    if type(b) is bytes:
        return b
    if isinstance(b, bytes | bytearray | memoryview):
        return bytes(b)
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    raise native.refused(f'{what}: a bytes-like object is needed, not {type(b).__name__}', AudioError)


def _read_answer(got):
    """A reader's answer: its values, or the reference's error raised where the core gives (type name, message)."""
    if len(got) == 2:
        raise _F3_ERRORS[got[0]](got[1])
    return got


def read_wave(b, source):
    """A Wave resource: source 'bfbc2-xbox', 'bfbc2-pc' or 'bf1943'. In the Rust core (round F0; since round RT3 its
    only reader: the reference's AudioError, or struct.error, where it raised)."""
    if source not in SOURCES:
        raise AudioError(f'source {source!r}: one of {SOURCES}')
    got = _read_answer(_reader().wave_read(_bytes_of(b, 'Wave'), str(source)))
    fid, total, intro, ch, outro, w14, segs, rate, w04, loop_end = got
    if source == 'bf1943':
        return Wave(source, fid, total, intro, ch, outro, 0, segs, rate=rate, word04=w04, loop_end=loop_end)
    return Wave(source, fid, total, intro, ch, outro, w14, segs)


def write_wave_pc(w):
    """BFBC2's PC Wave resource (little-endian) from a Wave: in the Rust core when it is built (round F0), else
    write_wave_pc_reference."""
    nat = _native()
    got = nat.wave_write_pc(w) if nat is not None else None
    return got if got is not None else write_wave_pc_reference(w)


def write_wave_pc_reference(w):
    """The Python writer (the reference)."""
    out = [struct.pack('<6I', *w.words())]
    for i, s in enumerate(w.segments):
        if s is None:
            if i == 1:
                raise AudioError('Wave: no loop segment')
            out.append(struct.pack('<I', 0))
            continue
        data, info = s
        out += [struct.pack('<I', len(data)), data, PAD[:(-len(data)) % 4],
                struct.pack('<I', len(info)), info, PAD[:(-len(info)) % 4]]
    return b''.join(out)


# ---------------------------------------------------------------- EA audio core streams

class Stream:
    """An EA stream: version, codec, channels, rate, type, samples, and its audio blocks [(samples, data)]."""

    def __init__(self, version, codec, channels, rate, type_, samples, blocks):
        self.version, self.codec, self.channels, self.rate = version, codec, channels, rate
        self.type, self.samples, self.blocks = type_, samples, blocks

    def header(self):
        return (self.version << 28 | self.codec << 24 | (self.channels - 1) << 18 | self.rate,
                self.type << 30 | self.samples)


def _ea_header(h1, h2):
    if (h2 >> 29) & 1:
        raise AudioError('EA stream: the loop flag is set (no Wave seen has it; its loop start word is not handled)')
    return dict(version=h1 >> 28, codec=(h1 >> 24) & 15, channels=((h1 >> 18) & 0x3F) + 1, rate=h1 & 0x3FFFF,
                type_=h2 >> 30, samples=h2 & 0x1FFFFFFF)


def _stream(got):
    version, codec, channels, rate, type_, samples, blocks = got
    return Stream(version, codec, channels, rate, type_, samples, blocks)


def read_sps(s):
    """An EA SPS stream (v1, BFBC2): 0x48 header, 0x44 audio blocks, 0x45 end. In the Rust core (round F0; its only
    reader since round RT3: the reference's AudioError, or struct.error, where it raised)."""
    return _stream(_read_answer(_reader().sps_read(_bytes_of(s, 'SPS'))))


def read_snr(s, sns=None):
    """An EA SNR / SNS stream (v0, Battlefield 1943): in memory the header and its blocks; streamed (8-byte s) the
    header, the blocks in sns. In the Rust core (round F0; its only reader since round RT3)."""
    s = _bytes_of(s, 'SNR')
    if len(s) == 8 and sns is not None:
        sns = _bytes_of(sns, 'SNS')
    elif len(s) != 8:
        sns = None                      # the reference read sns only for a header-only s
    return _stream(_read_answer(_reader().snr_read(s, sns)))


def write_sps(st):
    """An EA SPS stream (v1) of st's header and blocks: in the Rust core when it is built (round F0), else
    write_sps_reference."""
    nat = _native()
    got = nat.sps_write(st) if nat is not None else None
    return got if got is not None else write_sps_reference(st)


def write_sps_reference(st):
    """The Python writer (the reference)."""
    out = [b'\x48\x00\x00\x0c', struct.pack('>II', *st.header())]
    for n, data in st.blocks:
        out.append(struct.pack('>II', 0x44000000 | (8 + len(data)), n) + data)
    out.append(b'\x45\x00\x00\x04')
    return b''.join(out)


def segment_stream(w, i, streams=None):
    """Segment i of a Wave as a Stream; streams: {segment index: waveDataSns bytes} for streamed segments."""
    data, info = w.segments[i]
    sns = (streams or {}).get(i)
    if w.source == 'bf1943':
        if info and len(data) > 8:
            st = read_snr(data)
            if sum(n for n, _ in st.blocks) != st.samples:
                raise AudioError(f'segment {i}: in memory with stream information, {sum(n for n, _ in st.blocks)} of '
                                 f'{st.samples} samples in memory (the rest is streamed)')
            return st
        return read_snr(data, sns)
    if len(data) == 4:
        if sns is None:
            n = struct.unpack('>I' if w.source == 'bfbc2-xbox' else '<I', data)[0]
            raise AudioError(f'segment {i}: streamed ({n} samples); pass its waveDataSns entry')
        st = read_sps(sns)
    else:
        st = read_sps(data)
    if sum(n for n, _ in st.blocks) != st.samples:
        raise AudioError(f'segment {i}: {sum(n for n, _ in st.blocks)} of {st.samples} samples in memory (the head of '
                         f'a streamed sound; pass its waveDataSns entry)')
    return st


# ---------------------------------------------------------------- EA-XMA (through ffmpeg)

def ffmpeg_path():
    p = os.environ.get('FFMPEG') or shutil.which('ffmpeg')
    if p:
        return p
    base = os.path.join(os.environ.get('LOCALAPPDATA', ''), 'Microsoft', 'WinGet', 'Packages')
    if os.path.isdir(base):
        for d in sorted(os.listdir(base), reverse=True):
            if d.startswith('Gyan.FFmpeg'):
                for root, _, files in os.walk(os.path.join(base, d)):
                    if 'ffmpeg.exe' in files:
                        return os.path.join(root, 'ffmpeg.exe')
    raise AudioError('ffmpeg not found (PATH, $FFMPEG, WinGet Gyan.FFmpeg)')


def ffmpeg_decode(data, fmt, channels):
    """int16 (samples, channels) of a file ffmpeg reads (fmt: its demuxer), through a pipe."""
    r = subprocess.run([ffmpeg_path(), '-v', 'error', '-f', fmt, '-i', 'pipe:0', '-f', 's16le', '-acodec',
                        'pcm_s16le', 'pipe:1'], input=data, capture_output=True)
    if r.returncode or (not r.stdout and data):
        raise AudioError(f'ffmpeg ({fmt}): {r.stderr.decode(errors="replace").strip()[:300]}')
    return np.frombuffer(r.stdout, '<i2').reshape(-1, channels)


def _f3(name):
    """The Rust core's function `name` of round F4 (decode, convert), or None (not built, or the containers' readers
    off: their Python reference runs)."""
    nat = _native()
    return getattr(nat, name, None) if nat is not None else None


_F3_ERRORS = {'AudioError': AudioError, 'struct.error': struct.error, 'ZeroDivisionError': ZeroDivisionError}


def _f3_answer(got, what):
    """A round-F3 function's answer: the reference's error raised where the core gives it as (type name, message)
    (round RT2); an argument the core does not take (None) raises Refused."""
    if got is None:
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        if root not in sys.path:
            sys.path.insert(0, root)
        import suite_paths
        suite_paths.use('core')
        from bfbc2core import native
        raise native.refused(f'{what}: an argument the EA framing does not take (a whole number is needed)', AudioError)
    if type(got) is tuple:
        raise _F3_ERRORS[got[0]](got[1])
    return got


def xma_parts(st):
    """The XMA2 packets of each XMA stream (2 channels each), each block's part padded to whole packets: bytearrays,
    in the Rust core (round F3: bc2_native.audio_xma_parts; a block that ends before a stream, a part that does not
    fit or bytes other than zeros after the parts raise AudioError)."""
    if any(type(b) is not bytes for _, b in st.blocks):
        st = Stream(st.version, st.codec, st.channels, st.rate, st.type, st.samples,
                    [(n, bytes(b)) for n, b in st.blocks])
    return _f3_answer(require().audio_xma_parts(st), 'EA-XMA parts')


def xma2_riff(packets, channels, rate, samples):
    """A RIFF WAVE of XMA2 packets (XMA2WAVEFORMATEX, one stream), in the Rust core (round F3; a value past its
    field's width raises struct.error, as struct.pack does)."""
    return _f3_answer(require().audio_xma2_riff(bytes(packets), channels, rate, samples), 'XMA2 RIFF')


def decode_xma(st, trim=True):
    """int16 (samples, channels) of an EA-XMA stream (ffmpeg's xma2 decoder per XMA stream), cut / padded to the
    header's sample count (trim=False: as ffmpeg gives it)."""
    if st.codec != 3:
        raise AudioError(f'EA stream codec {st.codec} ({CODECS.get(st.codec, "?")}), not EA-XMA')
    cols = []
    for i, p in enumerate(xma_parts(st)):
        ch = min(2, st.channels - 2 * i)
        cols.append(ffmpeg_decode(xma2_riff(bytes(p), ch, st.rate, st.samples), 'wav', ch))
    n = min(len(c) for c in cols)
    pcm = np.hstack([c[:n] for c in cols])
    if not trim:
        return pcm
    if len(pcm) < st.samples:
        pcm = np.vstack([pcm, np.zeros((st.samples - len(pcm), st.channels), np.int16)])
    return pcm[:st.samples]


# ---------------------------------------------------------------- EA-XAS v1 (the Rust core)

def decode_xas(st):
    """int16 (samples, channels) of an EA-XAS v1 stream (this module's decoder; ffmpeg's adpcm_ea_xas alike)."""
    ch = st.channels
    data = b''.join(b for _, b in st.blocks)
    if len(data) % (XAS_FRAME * ch):
        raise AudioError(f'EA-XAS: {len(data)} bytes, not whole {XAS_FRAME * ch}-byte frames')
    f = np.frombuffer(data, np.uint8).reshape(-1, ch, XAS_FRAME)
    pcm = xas_decode_frames(f)                                      # (frames, ch, 128)
    return pcm.transpose(0, 2, 1).reshape(-1, ch)[:st.samples]


def xas_decode_frames(f):
    """int16 (..., 128) of XAS frames (..., 76) uint8, in the Rust core."""
    f = np.asarray(f)
    if f.dtype != np.uint8 or not f.ndim or f.shape[-1] != XAS_FRAME:
        raise ValueError(f'EA-XAS decode: frames must be uint8 (..., {XAS_FRAME}), not {f.dtype} {f.shape}')
    out = require().xas_decode_frames(np.ascontiguousarray(f).tobytes())
    return np.frombuffer(out, '<i2').astype(np.int16, copy=False).reshape(f.shape[:-1] + (128,))


def xas_encode_frames(pcm):
    """XAS frames (frames, channels, 76) uint8 of int16 pcm (samples, channels), the last frame padded with zeros.
    Per group of 32 samples: the first two stored at 12 bits, the coefficient set and shift that give the least
    squared error (each set tried with the shift its prediction residual needs and the next two finer / coarser).
    In the Rust core."""
    n, ch = _pcm16(pcm).shape
    return np.frombuffer(_frames_bytes([pcm])[0], np.uint8).reshape(-(-n // 128), ch, XAS_FRAME)


def _frames_bytes(pcms):
    """The frames' bytes (frame-major) of each pcm, all at once on every core in the Rust core."""
    arrays = [_pcm16(p) for p in pcms]
    out = require().xas_encode_many([(a.tobytes(), a.shape[1]) for a in arrays])
    for r in out:
        if isinstance(r, Exception):
            raise r
    return out


def xas_block_samples(rate):
    """DICE's EA-XAS block length (samples) for a sample rate: every block of a PC stream but the last."""
    return 256 * -(-rate // 1280)


def encode_xas(pcm, rate, type_=1):
    """An EA-XAS v1 Stream of int16 pcm (samples, channels), in DICE's blocks."""
    n, ch = pcm.shape
    return _xas_stream(_frames_bytes([pcm])[0], n, ch, rate, type_)


def encode_xas_many(items):
    """encode_xas of many (pcm, rate) or (pcm, rate, type_), encoded all at once on every core: a list of
    Streams."""
    items = [tuple(it) for it in items]
    shapes = [it[0].shape for it in items]
    data = _frames_bytes([it[0] for it in items])
    return [_xas_stream(d, n, ch, it[1], it[2] if len(it) > 2 else 1) for d, (n, ch), it in zip(data, shapes, items)]


def _xas_stream(frames, n, ch, rate, type_):
    """The Stream of a sound's frames (bytes, frame-major) in DICE's blocks: 256 * ceil(rate / 1280) samples each,
    the last the rest (empty when the count is a multiple of the block). The blocks are cut in the Rust core (round
    F3: bc2_native.audio_xas_blocks; a rate whose blocks hold no samples raises ZeroDivisionError, as the reference's
    division did)."""
    blocks = _f3_answer(require().audio_xas_blocks(bytes(frames), n, ch, rate), 'EA-XAS blocks')
    return Stream(1, 4, ch, rate, type_, n, blocks)


def ffmpeg_decode_xas(st):
    """int16 (samples, channels) of an EA-XAS stream through ffmpeg's adpcm_ea_xas (as an EA cdata file)."""
    tag = {1: 0x0400, 2: 0x0404, 4: 0x040C, 6: 0x0414}.get(st.channels)
    if tag is None or st.rate > 0xFFFF:
        raise AudioError(f'ffmpeg ea_cdata: {st.channels} channels at {st.rate} Hz not representable')
    data = struct.pack('>HH', tag, st.rate) + bytes(12) + b''.join(b for _, b in st.blocks)
    return ffmpeg_decode(data, 'ea_cdata', st.channels)[:st.samples]


# ---------------------------------------------------------------- whole resources

def _stream_list(streams):
    """[streams.get(i) for i in range(3)] for the Rust core (each bytes or None), or None (another kind of
    argument: the Python reads it)."""
    if not streams:
        return [None, None, None]
    if type(streams) is not dict:
        return None
    got = [streams.get(i) for i in range(3)]
    return got if all(v is None or type(v) is bytes for v in got) else None


def _ffmpeg_or_none():
    try:
        return ffmpeg_path()
    except AudioError:
        return None                         # the Rust core hands the sound back: the Python raises


def decode(b, source, streams=None):
    """int16 (samples, channels) of a Wave's segments one after another, and the sample rate. In the Rust core when it
    is built (round F4: bc2_native.audio_decode, ffmpeg run as ffmpeg_decode runs it), else decode_reference; a
    sound it does not take (or an error) is the Python's."""
    fn = _f3('audio_decode')
    sl = _stream_list(streams) if fn is not None and type(b) is bytes and type(source) is str else None
    got = fn(b, source, sl, None) if sl is not None else None
    if got == 'ffmpeg':                     # an EA-XMA sound: ffmpeg looked for only now, as the Python does
        got = fn(b, source, sl, _ffmpeg_or_none())
    if got is not None and got != 'ffmpeg':
        pcm, ch, rate = got
        return np.frombuffer(pcm, '<i2').reshape(-1, ch), rate
    return decode_reference(b, source, streams)


def decode_reference(b, source, streams=None):
    """The Python (the reference)."""
    w = read_wave(b, source)
    out, rate = [], None
    for i in range(3):
        if w.segments[i] is None:
            continue
        st = segment_stream(w, i, streams)
        rate = st.rate
        if st.codec == 3:
            out.append(decode_xma(st))
        elif st.codec == 4:
            out.append(decode_xas(st))
        else:
            raise AudioError(f'segment {i}: codec {st.codec} ({CODECS.get(st.codec, "?")}) is not decoded here')
    return np.vstack(out), rate


def convert(b, source, streams=None, word14=0):
    """An Xbox Wave (source 'bfbc2' / 'bfbc2-xbox' or 'bf1943') as BFBC2's PC Wave: the same words (BF1943's mapped),
    every segment in memory as EA-XAS v1 in DICE's blocks. streams: {segment index: waveDataSns bytes} for streamed
    segments. word14: BFBC2's word 0x14 for a BF1943 source (0, as most BFBC2 sounds). In the Rust core when it is
    built (round F4: bc2_native.audio_convert, without the interpreter's lock: convert_many's threads run side by
    side), else convert_reference; a sound it does not take (or an error) is the Python's."""
    fn = _f3('audio_convert')
    sl = _stream_list(streams) if fn is not None and type(b) is bytes and type(source) is str else None
    got = fn(b, source, sl, word14, None) if sl is not None else None
    if got == 'ffmpeg':                     # ffmpeg looked for once a sound needs it, as the Python does
        got = fn(b, source, sl, word14, _ffmpeg_or_none())
    return got if type(got) is bytes else convert_reference(b, source, streams, word14)


def convert_reference(b, source, streams=None, word14=0):
    """The Python (the reference)."""
    source = 'bfbc2-xbox' if source == 'bfbc2' else source
    if source not in ('bfbc2-xbox', 'bf1943'):
        raise AudioError(f'convert: source {source!r} (bfbc2 or bf1943)')
    w = read_wave(b, source)
    segs: list[tuple[Any, bytes] | None] = []
    for i in range(3):
        if w.segments[i] is None:
            segs.append(None)
            continue
        st = segment_stream(w, i, streams)
        if st.codec != 3:
            raise AudioError(f'segment {i}: codec {st.codec} ({CODECS.get(st.codec, "?")}), not EA-XMA')
        if st.channels != w.channels:
            raise AudioError(f'segment {i}: {st.channels} channels, the Wave says {w.channels}')
        pc = encode_xas(decode_xma(st), st.rate)
        segs.append((write_sps(pc), b''))
    out = Wave('bfbc2-pc', w.flags_id, w.total, w.intro, w.channels, w.outro,
               w.word14 if source == 'bfbc2-xbox' else word14, segs)
    return write_wave_pc(out)


def convert_many(items, workers=None):
    """convert of many (b, source, streams) or (b, source, streams, word14), side by side in threads (ffmpeg runs
    in its own processes, the Rust core's encoder without the interpreter): the PC Waves in order, each the bytes or
    the AudioError it raised (returned, not raised). workers: the threads (default the CPU count, at most 8)."""
    items = [tuple(it) for it in items]

    def one(it):
        try:
            return convert(*it)
        except AudioError as e:
            return e
    if len(items) < 2 or workers == 1:
        return [one(it) for it in items]
    with ThreadPoolExecutor(workers or min(8, os.cpu_count() or 1)) as ex:
        return list(ex.map(one, items))


def main(argv=None):
    import argparse
    ap = argparse.ArgumentParser(description='An Xbox 360 Wave resource (BFBC2 / Battlefield 1943) as BFBC2 PC.')
    ap.add_argument('source', choices=['bfbc2', 'bf1943'])
    ap.add_argument('input')
    ap.add_argument('output')
    ap.add_argument('--stream', action='append', default=[], metavar='N=FILE',
                    help='a streamed segment\'s waveDataSns_<N>.res (N: 0 intro, 1 loop, 2 outro)')
    a = ap.parse_args(argv)
    require()                               # the encoder runs in the Rust core: say so before reading anything
    streams = {}
    for s in a.stream:
        n, _, p = s.partition('=')
        with open(p, 'rb') as fh:
            streams[int(n)] = fh.read()
    with open(a.input, 'rb') as fh:
        data = fh.read()
    try:
        out = convert(data, a.source, streams)
    except AudioError as e:
        print(f'error: {e}\n{AudioError.what_to_do}')
        return 1
    with open(a.output, 'wb') as fh:
        fh.write(out)
    print(f'{a.output}: {len(out)} bytes')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
