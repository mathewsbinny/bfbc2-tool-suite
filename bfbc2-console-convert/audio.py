r"""audio.py - Xbox 360 sounds (Frostbite "Wave" resources, EA-XMA audio) as BFBC2's PC Wave resources (EA-XAS audio);
BFBC2's own Xbox files and Battlefield 1943's.

    import audio
    w = audio.read_wave(xbox_bytes, 'bfbc2-xbox')        # or 'bfbc2-pc', 'bf1943'
    pcm, rate = audio.decode(xbox_bytes, 'bf1943')       # int16 (samples, channels); EA-XMA through ffmpeg
    pc = audio.convert(xbox_bytes, 'bfbc2')              # BFBC2 PC Wave, EA-XAS, in memory
    pc = audio.convert(bf1943_stub, 'bf1943', streams={1: sns_bytes})   # a streamed sound (voice-over)

Needs numpy, and ffmpeg (decoding only: EA-XMA as XMA2; on PATH, in $FFMPEG, or the WinGet Gyan.FFmpeg package) for
anything that reads EA-XMA. The EA-XAS encoder and decoder are this module's own (numpy).
Proven in tests\test_audio.py on BFBC2's Xbox / PC twins and on Battlefield 1943's sounds.

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


def read_wave(b, source):
    """A Wave resource: source 'bfbc2-xbox', 'bfbc2-pc' or 'bf1943'."""
    if source not in SOURCES:
        raise AudioError(f'source {source!r}: one of {SOURCES}')
    e = '<' if source == 'bfbc2-pc' else '>'
    if source == 'bf1943':
        if len(b) < 40:
            raise AudioError(f'Wave: {len(b)} bytes, shorter than a Battlefield 1943 header')
        rate, w04, ch, total, loop_start, loop_end, fid = struct.unpack_from('>f6I', b, 0)
        o = 28
    else:
        if len(b) < 36:
            raise AudioError(f'Wave: {len(b)} bytes, shorter than a BFBC2 header')
        fid, total, intro, ch, outro, w14 = struct.unpack_from(e + '6I', b, 0)
        o = 24
    segs: list[tuple[bytes, bytes] | None] = []
    for i in range(3):
        if o + 4 > len(b):
            raise AudioError(f'Wave: segment {i} at {o:#x} runs past the end ({len(b)} bytes)')
        n = struct.unpack_from(e + 'I', b, o)[0]
        o += 4
        if n == 0:
            segs.append(None)
            continue
        if o + n + 4 > len(b):
            raise AudioError(f'Wave: segment {i}: {n} bytes at {o:#x} run past the end ({len(b)} bytes)')
        data = bytes(b[o:o + n])
        o += n + (-n) % 4
        m = struct.unpack_from(e + 'I', b, o)[0]
        o += 4
        info = bytes(b[o:o + m])
        if len(info) != m:
            raise AudioError(f'Wave: segment {i}: {m} bytes of stream information run past the end')
        o = min(o + m + (-m) % 4, len(b))
        segs.append((data, info))
    if o != len(b) or segs[1] is None:
        raise AudioError(f'Wave: the segments end at {o:#x}, the file at {len(b):#x}' if o != len(b) else
                         'Wave: no loop segment')
    if source == 'bf1943':
        intro = loop_start
        outro = total - loop_end if segs[2] else 0
        return Wave(source, fid, total, intro, ch, outro, 0, segs, rate=rate, word04=w04, loop_end=loop_end)
    return Wave(source, fid, total, intro, ch, outro, w14, segs)


def write_wave_pc(w):
    """BFBC2's PC Wave resource (little-endian) from a Wave."""
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


def read_sps(s):
    """An EA SPS stream (v1, BFBC2): 0x48 header, 0x44 audio blocks, 0x45 end."""
    o, hdr, blocks = 0, None, []
    while o + 4 <= len(s):
        kind, n = s[o], int.from_bytes(s[o + 1:o + 4], 'big')
        if n < 4 or o + n > len(s):
            raise AudioError(f'SPS: block {kind:#x} of {n} bytes at {o:#x} does not fit ({len(s)} bytes)')
        if kind == 0x48:
            hdr = _ea_header(*struct.unpack_from('>II', s, o + 4))
        elif kind == 0x44:
            blocks.append((struct.unpack_from('>I', s, o + 4)[0], bytes(s[o + 8:o + n])))
        elif kind == 0x45:
            o += n
            break
        else:
            raise AudioError(f'SPS: unknown block kind {kind:#x} at {o:#x}')
        o += n
    if hdr is None or o != len(s):
        raise AudioError('SPS: no header block' if hdr is None else f'SPS: ends at {o:#x} of {len(s):#x}')
    return Stream(blocks=blocks, **hdr)


def _sns_blocks(s, o):
    blocks = []
    while o + 8 <= len(s):
        h = struct.unpack_from('>I', s, o)[0]
        flags, n = h >> 24, h & 0xFFFFFF
        if n < 8 or o + n > len(s):
            raise AudioError(f'SNS: block of {n} bytes at {o:#x} does not fit ({len(s)} bytes)')
        blocks.append((struct.unpack_from('>I', s, o + 4)[0], bytes(s[o + 8:o + n])))
        o += n
        if flags & 0x80:
            break
    return blocks, o


def read_snr(s, sns=None):
    """An EA SNR / SNS stream (v0, Battlefield 1943): in memory the header and its blocks; streamed (8-byte s) the
    header, the blocks in sns."""
    if len(s) < 8:
        raise AudioError(f'SNR: {len(s)} bytes, shorter than the header')
    hdr = _ea_header(*struct.unpack_from('>II', s, 0))
    if len(s) == 8:
        if sns is None:
            raise AudioError('SNR: a streamed sound (header only); pass its waveDataSns entry')
        blocks, o = _sns_blocks(sns, 0)
        if o != len(sns):
            raise AudioError(f'SNS: the blocks end at {o:#x}, the stream at {len(sns):#x}')
    else:
        blocks, o = _sns_blocks(s, 8)
        if o != len(s):
            raise AudioError(f'SNR: the blocks end at {o:#x}, the data at {len(s):#x}')
    return Stream(blocks=blocks, **hdr)


def write_sps(st):
    """An EA SPS stream (v1) of st's header and blocks."""
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


def xma_parts(st):
    """The XMA2 packets of each XMA stream (2 channels each), each block's part padded to whole packets."""
    k = (st.channels + 1) // 2
    out = [bytearray() for _ in range(k)]
    for bi, (_, b) in enumerate(st.blocks):
        o = 0
        for i in range(k):
            if o + 4 > len(b):
                raise AudioError(f'EA-XMA: block {bi} ends before stream {i}')
            n = struct.unpack_from('>I', b, o)[0] >> 2
            if n < 4 or o + n > len(b):
                raise AudioError(f'EA-XMA: block {bi}, stream {i}: {n} bytes at {o} do not fit {len(b)}')
            out[i] += b[o + 4:o + n] + bytes((-(n - 4)) % XMA_PACKET)
            o += n
        if o != len(b) and any(b[o:]):                       # BF1943's last streamed block: zeros after the parts
            raise AudioError(f'EA-XMA: block {bi}: the streams end at {o} of {len(b)}')
    return out


def xma2_riff(packets, channels, rate, samples):
    """A RIFF WAVE of XMA2 packets (XMA2WAVEFORMATEX, one stream)."""
    fmt = struct.pack('<HHIIHHH', 0x166, channels, rate, rate * channels * 2, XMA_PACKET, 16, 34)
    fmt += struct.pack('<HIIIIIIIBBH', 1, 4 if channels == 1 else 3, samples, XMA_PACKET, 0, 0, 0, 0, 0, 4, 1)
    body = b'WAVEfmt ' + struct.pack('<I', len(fmt)) + fmt + b'data' + struct.pack('<I', len(packets)) + packets
    return b'RIFF' + struct.pack('<I', len(body)) + body


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


# ---------------------------------------------------------------- EA-XAS v1 (numpy)

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
    """int16 (..., 128) of XAS frames (..., 76) uint8."""
    shp = f.shape[:-1]
    f = f.reshape(-1, XAS_FRAME)
    h = f[:, :16].copy().view('<u4').astype(np.int64)               # (G0, 4)
    lo, hi = h & 0xFFFF, h >> 16
    s0 = ((lo & 0xFFF0) ^ 0x8000) - 0x8000
    s1 = ((hi & 0xFFF0) ^ 0x8000) - 0x8000
    c0, c1 = COEFS[lo & 15], COEFS[(lo & 15) + 4]
    step = np.left_shift(1, 20 - (hi & 15))
    rows = f[:, 16:].reshape(-1, 15, 4).astype(np.int64)            # (G0, row, group)
    nib = np.empty(rows.shape[:1] + (4, 30), np.int64)
    nib[:, :, 0::2] = (rows >> 4).transpose(0, 2, 1)
    nib[:, :, 1::2] = (rows & 15).transpose(0, 2, 1)
    nib = (nib ^ 8) - 8
    out = np.empty(rows.shape[:1] + (4, 32), np.int64)
    out[:, :, 0], out[:, :, 1] = s0, s1
    p2, p1 = s0, s1
    for t in range(30):
        s = np.clip((nib[:, :, t] * step + p1 * c0 + p2 * c1 + 128) >> 8, -32768, 32767)
        out[:, :, t + 2] = s
        p2, p1 = p1, s
    return out.reshape(shp + (128,)).astype(np.int16)


def xas_encode_frames(pcm):
    """XAS frames (frames, channels, 76) uint8 of int16 pcm (samples, channels), the last frame padded with zeros.
    Per group of 32 samples: the first two stored at 12 bits, the coefficient set and shift that give the least
    squared error (each set tried with the shift its prediction residual needs and the next two finer / coarser)."""
    n, ch = pcm.shape
    F = -(-n // 128)
    x = np.zeros((F * 128, ch), np.int64)
    x[:n] = pcm
    g = x.reshape(F, 4, 32, ch).transpose(0, 3, 1, 2).reshape(-1, 32)   # (frames * ch * 4, 32), frame-major
    q0 = np.clip((g[:, 0] + 8) >> 4 << 4, -32768, 32752)
    q1 = np.clip((g[:, 1] + 8) >> 4 << 4, -32768, 32752)
    best_err = np.full(len(g), np.inf)
    best = (np.zeros(len(g), np.int64), np.zeros(len(g), np.int64), np.zeros((len(g), 30), np.int64))
    target = g[:, 2:] * 256
    for c in range(4):
        c0, c1 = int(COEFS[c]), int(COEFS[c + 4])
        r = np.abs(target - (np.c_[q1, g[:, 2:-1]] * c0 + np.c_[q0, q1, g[:, 2:-2]] * c1)).max(1)
        need = np.ceil(np.log2(np.maximum(r, 1) / 7.0)).astype(np.int64)    # step 2^need covers the residual
        for d in (-1, 0, 1):
            k = np.clip(20 - need - d, 0, 15)
            step = np.left_shift(1, 20 - k).astype(np.int64)
            p2, p1 = q0, q1
            err = np.zeros(len(g))
            nib = np.empty((len(g), 30), np.int64)
            for t in range(30):
                pred = p1 * c0 + p2 * c1
                v = np.clip(np.rint((target[:, t] - pred) / step), -8, 7).astype(np.int64)
                s = np.clip((v * step + pred + 128) >> 8, -32768, 32767)
                err += (s - g[:, t + 2]) ** 2
                nib[:, t] = v
                p2, p1 = p1, s
            better = err < best_err
            best_err = np.where(better, err, best_err)
            best = (np.where(better, c, best[0]), np.where(better, k, best[1]), np.where(better[:, None], nib, best[2]))
    cset, k, nib = best
    head = ((q0 & 0xFFF0) | cset) & 0xFFFF | ((((q1 & 0xFFF0) | k) & 0xFFFF) << 16)
    G = len(g) // 4
    frames = np.empty((G, XAS_FRAME), np.uint8)
    frames[:, :16] = head.astype('<u4').reshape(G, 4).view(np.uint8)
    nb = (nib & 15).reshape(G, 4, 15, 2)                             # (frame, group, row, hi/lo)
    frames[:, 16:] = ((nb[..., 0] << 4) | nb[..., 1]).transpose(0, 2, 1).reshape(G, 60).astype(np.uint8)
    return frames.reshape(F, ch, XAS_FRAME)


def xas_block_samples(rate):
    """DICE's EA-XAS block length (samples) for a sample rate: every block of a PC stream but the last."""
    return 256 * -(-rate // 1280)


def encode_xas(pcm, rate, type_=1):
    """An EA-XAS v1 Stream of int16 pcm (samples, channels), in DICE's blocks."""
    n, ch = pcm.shape
    frames = xas_encode_frames(pcm)
    per = xas_block_samples(rate)
    full = n // per                                                  # an exact multiple: DICE's last block is empty
    f = per // 128
    blocks = [(per, frames[i * f:(i + 1) * f].tobytes()) for i in range(full)]
    blocks.append((n - full * per, frames[full * f:].tobytes()))
    return Stream(1, 4, ch, rate, type_, n, blocks)


def ffmpeg_decode_xas(st):
    """int16 (samples, channels) of an EA-XAS stream through ffmpeg's adpcm_ea_xas (as an EA cdata file)."""
    tag = {1: 0x0400, 2: 0x0404, 4: 0x040C, 6: 0x0414}.get(st.channels)
    if tag is None or st.rate > 0xFFFF:
        raise AudioError(f'ffmpeg ea_cdata: {st.channels} channels at {st.rate} Hz not representable')
    data = struct.pack('>HH', tag, st.rate) + bytes(12) + b''.join(b for _, b in st.blocks)
    return ffmpeg_decode(data, 'ea_cdata', st.channels)[:st.samples]


# ---------------------------------------------------------------- whole resources

def decode(b, source, streams=None):
    """int16 (samples, channels) of a Wave's segments one after another, and the sample rate."""
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
    segments. word14: BFBC2's word 0x14 for a BF1943 source (0, as most BFBC2 sounds)."""
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


def main(argv=None):
    import argparse
    ap = argparse.ArgumentParser(description='An Xbox 360 Wave resource (BFBC2 / Battlefield 1943) as BFBC2 PC.')
    ap.add_argument('source', choices=['bfbc2', 'bf1943'])
    ap.add_argument('input')
    ap.add_argument('output')
    ap.add_argument('--stream', action='append', default=[], metavar='N=FILE',
                    help='a streamed segment\'s waveDataSns_<N>.res (N: 0 intro, 1 loop, 2 outro)')
    a = ap.parse_args(argv)
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
