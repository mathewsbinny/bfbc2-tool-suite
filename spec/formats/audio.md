# Audio: Wave resources, EA stream framing, EA-XAS

Sounds are `Wave` resources (the largest resource type by count: 216,393 entries in the game copy). A Wave holds a
small header and up to three segments (intro, loop, outro); each segment is either audio in memory, framed as an EA
audio core stream, or a stub whose audio is a separate streamed entry. The PC's in-memory audio is EA-XAS v1 (4-bit
ADPCM), its streamed audio EALayer3; the Xbox 360's audio is EA-XMA (XMA2 packets).

The suite reads Waves from BFBC2 (PC and Xbox 360) and Battlefield 1943, and writes BFBC2 PC Waves with EA-XAS audio
(the Xbox 360 -> PC and BF1943 -> BFBC2 conversions): `bfbc2-console-convert/audio.py`, `bc2_formats::wave` (the
resource and stream framing), `bc2_formats::audioframe` (XMA parts, the RIFF wrapper, DICE's XAS blocks) and
`bc2_convert::audio` (the EA-XAS codec). The Wave layout was decoded by the suite from the twins the two platforms
share.

## The Wave resource

### WAVE-HDR-1: BFBC2 Wave header

Status: **verified** · Variants: PC, Xbox 360

Six 32-bit words (PC little-endian, Xbox 360 big-endian), then the segments (WAVE-SEG-1) from byte 24:

| Offset | Field | Values |
|---|---|---|
| 0x00 | flags << 16 \| id | flags 1 - 4, 0x13, 0x14, 0x23, 0x24, 0x31; id 0 or a 16-bit value (not identified) |
| 0x04 | total samples | intro + loop + outro |
| 0x08 | intro samples | 0: no intro segment |
| 0x0C | channels | 1, 2, 6 |
| 0x10 | outro samples | 0: no outro segment |
| 0x14 | unknown | 0 in most files, 1 - 7 elsewhere |

The words are equal on both platforms for the same sound; only the audio and its sizes differ (the codec).

[data: 35,811 Xbox 360 / PC twins: every word equal (one file's total differs by an edit of DICE's); word 0x14 is 0 in 33,869 files (audio.py's survey)]
[test: rust/bc2-formats/src/wave.rs::wave_round_trip] [test: bfbc2-console-convert/tests/test_wave_native.py::main] [code: rust/bc2-formats/src/wave.rs:97-105] [code: rust/bc2-formats/src/wave.rs:153-164]
[doc: bfbc2-console-convert/audio.py:25-34]

### WAVE-HDR-2: Battlefield 1943 Wave header

Status: **verified** · Variants: BF1943

Big-endian, seven words, then the segments from byte 28:

| Offset | Field | Values |
|---|---|---|
| 0x00 | f32 sample rate | 48000, 44100, 32000, 24000, 11025 |
| 0x04 | 2 | |
| 0x08 | channels | 1, 2 |
| 0x0C | total samples | |
| 0x10 | loop start | = intro samples |
| 0x14 | loop end | intro + loop when there is an outro; total on 6 looping idles; else 0 |
| 0x18 | flags << 16 \| id | as BFBC2's word 0x00 (flags 1 - 4, 0x12, 0x22, 0x32) |

Mapped to BFBC2's words: intro = loop start, outro = total - loop end (when an outro segment exists), word 0x14 = 0.
A NaN rate is refused.

[data: 2,750 BF1943 Wave files read to their exact end]
[test: rust/bc2-formats/src/wave.rs::wave_round_trip] [test: bfbc2-console-convert/tests/test_wave_native.py::main] [code: rust/bc2-formats/src/wave.rs:85-96] [code: rust/bc2-formats/src/wave.rs:134-151]
[doc: bfbc2-console-convert/audio.py:51-61]

### WAVE-SEG-1: Three segments, each a data blob and a stream-information blob

Status: **verified** · Variants: PC, Xbox 360, BF1943

After the header come three segments, intro, loop, outro, each:

| Field | Meaning |
|---|---|
| u32 size | 0: the segment is absent (nothing else follows for it) |
| size bytes | the audio data (WAVE-SEG-2) |
| padding | to a multiple of 4, with the bytes `AD DE AD` (as many as needed) |
| u32 m | the stream information's size (0 for plain in-memory audio) |
| m bytes | stream information (WAVE-SEG-3) |
| padding | to a multiple of 4, `AD DE AD` |

The loop segment is always present. The file ends exactly after the outro segment (or the slot where it would be).
The Xbox 360 leaves the last blob unpadded in a few files.

[data: every Wave of BFBC2's Xbox 360 copy, the PC game copy's twins and BF1943's 2,750 parse to their exact end; 9 Xbox 360 files without the last blob's padding]
[test: rust/bc2-formats/src/wave.rs::wave_round_trip] [code: rust/bc2-formats/src/wave.rs:107-132] [code: rust/bc2-formats/src/wave.rs:12]
[doc: bfbc2-console-convert/audio.py:35-44]

### WAVE-SEG-2: In-memory audio or a streamed stub

Status: **observed** · Variants: PC, Xbox 360, BF1943

A segment's data is either an EA audio core stream (WAVE-STR-1) holding the audio, or, for a streamed segment, a stub:
BFBC2 stores the u32 sample count only, BF1943 the 8-byte SNR header only. The streamed audio is a separate
non-resource entry `<name>_waveDataSns_<n>.res` (n: 0 intro, 1 loop, 2 outro) in `streaming_sounds` or
`streaming_vo_<lang>`.

On the PC: 34,038 loop segments are streamed stubs (most of them 44-byte files: all voice-over and long sounds), 94
streamed with stream information (the 47 six-channel ambiences among them), 1,618 in memory, 61 in memory with stream
information (the head of a streamed sound). Which segments are streamed differs between platforms in a few looping
vehicle sounds.

[data: PC loop segments 34,038 streamed / 94 streamed with information / 1,618 in memory / 61 in memory with information; BF1943 streamed: 1,536 voice-over and 27 other files (audio.py's surveys)]
[doc: bfbc2-console-convert/audio.py:36-50]

### WAVE-SEG-3: Stream information is not decoded

Status: **observed** · Variants: PC, Xbox 360

A streamed segment's (or a preloaded head's) stream information holds EA's stream header and a seek table. Its layout
differs per platform (the Xbox 360's starts `01 10 01 80`, the PC's `C0 06 10 01`) and is not decoded; the suite
copies it as it is and writes 0 for the audio it converts to in-memory audio.

[data: the observed first bytes on both platforms (audio.py)]
[doc: bfbc2-console-convert/audio.py:40-41]

### WAVE-WR-1: Writing a PC Wave

Status: **verified** · Variants: PC

The suite writes the six BFBC2 words little-endian, then each present segment as WAVE-SEG-1 with `AD DE AD` padding
after every blob; a resource without a loop segment is refused. A converted Wave keeps its source's words (BF1943's
mapped as WAVE-HDR-2) and holds all its audio in memory as EA-XAS framed as SPS (a streamed source needs its
`waveDataSns` entry).

[test: rust/bc2-formats/src/wave.rs::wave_round_trip] [test: bfbc2-console-convert/tests/test_audio_native.py::quick] [code: rust/bc2-formats/src/wave.rs:168-205]
[doc: bfbc2-console-convert/audio.py:96-97]

## The EA audio core stream

### WAVE-STR-1: The two header words

Status: **verified** · Variants: PC, Xbox 360, BF1943

Always big-endian, whatever the platform:

| Word | Bits | Field |
|---|---|---|
| header | 31 - 28 | version: 0 (BF1943), 1 (BFBC2) |
| | 27 - 24 | codec: 2 PCM16BE, 3 EA-XMA, 4 EA-XAS, 5 EALayer3 v1, 6 EALayer3 v2 PCM, 7 EALayer3 v2 Spike |
| | 23 - 18 | channels - 1 |
| | 17 - 0 | sample rate (Hz) |
| samples | 31 - 30 | type: 0 in memory (BF1943), 1 stream / SPS (all of BFBC2) |
| | 29 | loop flag: when 1 a u32 loop start follows (no Wave of BFBC2 or BF1943 sets it; the suite refuses it) |
| | 28 - 0 | sample count |

The PC's in-memory streams are EA-XAS (codec 4, all 1,846 of them), its streamed audio EALayer3 v2 (codec 6); the
Xbox 360's audio is EA-XMA (codec 3).

[test: rust/bc2-formats/src/wave.rs::sps_round_trip] [test: rust/bc2-formats/src/wave.rs::snr] [code: rust/bc2-formats/src/wave.rs:210-223]
[doc: bfbc2-console-convert/audio.py:63-74]

### WAVE-STR-2: SPS framing (version 1, BFBC2)

Status: **verified** · Variants: PC, Xbox 360

A version-1 stream is a sequence of blocks, each `u8 kind, u24 size` (big-endian; the size includes these 4 bytes,
at least 4):

| Kind | Block |
|---|---|
| 0x48 | the header: the two words of WAVE-STR-1 (size 12) |
| 0x44 | audio: u32 sample count, then the codec's data (WAVE-STR-4) |
| 0x45 | the end (size 4) |

The header block comes first; the stream ends exactly after the end block. Any other kind is refused. The suite
writes `48 00 00 0C`, the two words, one `0x44` block per audio block, then `45 00 00 04`.

[test: rust/bc2-formats/src/wave.rs::sps_round_trip] [code: rust/bc2-formats/src/wave.rs:239-271] [code: rust/bc2-formats/src/wave.rs:323-337]
[doc: bfbc2-console-convert/audio.py:68-69]

### WAVE-STR-3: SNR / SNS framing (version 0, BF1943)

Status: **verified** · Variants: BF1943

A version-0 stream is the two header words (the SNR header), then blocks of `u32 flags << 24 | size` (big-endian;
size includes the word, at least 8), each an audio block (u32 sample count, then data). In memory: one block,
flags 0. Streamed: the Wave holds the 8-byte SNR header only and the blocks are the SNS entry, the last block's flags
0x80.

[test: rust/bc2-formats/src/wave.rs::snr] [code: rust/bc2-formats/src/wave.rs:276-321]
[doc: bfbc2-console-convert/audio.py:70-72]

### WAVE-STR-4: EA-XMA block data

Status: **verified** · Variants: Xbox 360, BF1943

An EA-XMA audio block holds one part per XMA stream (two channels a stream, the last stream one channel for an odd
count): `u32 n << 2 | 0-3` (big-endian; n the part's bytes including the word), then n - 4 bytes of XMA2 packets
(2,048 bytes a packet, the last one cut short). BFBC2's blocks hold one packet per stream (about 4,096 - 13,000
samples); BF1943's in-memory sounds one block of all packets; BF1943's streams end their last block with zero bytes
after the parts (1,539 of 1,563). Any non-zero byte after the parts is refused. To decode, each stream's parts are
padded to whole packets and wrapped as a RIFF WAVE with an `XMA2WAVEFORMATEX` (format tag 0x166, 2,048-byte packets)
for ffmpeg's xma2 decoder.

[data: BF1943 streams: 1,539 of 1,563 end their last block with zero bytes]
[test: rust/bc2-formats/src/audioframe.rs::parts_padded_per_stream] [test: rust/bc2-formats/src/audioframe.rs::riff_layout] [code: rust/bc2-formats/src/audioframe.rs:50-73] [code: rust/bc2-formats/src/audioframe.rs:81-115]
[doc: bfbc2-console-convert/audio.py:75-81]

## EA-XAS v1

### XAS-FRM-1: Frames: 128 samples of one channel in 76 bytes, frame-major

Status: **verified** · Variants: PC

EA-XAS audio data is a sequence of 76-byte frames, each 128 samples of one channel. Frames are interleaved by
channel: frame 0 of every channel, then frame 1 of every channel, and so on. A frame's 128 samples are four groups of
32 (group g: samples 32g .. 32g + 31).

[test: rust/bc2-convert/src/audio_tests.rs::layout_of_frames] [code: rust/bc2-convert/src/audio.rs:51-53] [code: rust/bc2-convert/src/audio.rs:124-128]
[doc: bfbc2-console-convert/audio.py:82-83]

### XAS-FRM-2: Frame layout

Status: **verified** · Variants: PC

| Bytes | Content |
|---|---|
| 0 - 15 | 4 u32 little-endian, one per group g: bits 0 - 3 the coefficient set (0 - 3), bits 4 - 15 the group's first sample (the top 12 bits of a 16-bit sample), bits 16 - 19 the shift k, bits 20 - 31 the second sample (top 12 bits) |
| 16 - 75 | 15 rows of 4 bytes: row r, byte g holds group g's samples 2 + 2r (high nibble) and 3 + 2r (low nibble) |

[test: rust/bc2-convert/src/audio_tests.rs::layout_of_frames] [test: rust/bc2-convert/src/audio_tests.rs::frames_decode_as_the_reference] [code: rust/bc2-convert/src/audio.rs:276-303]
[doc: bfbc2-console-convert/audio.py:84-86]

### XAS-DEC-1: Decoding a group

Status: **verified** · Variants: PC

Samples 0 and 1 of a group are the header's two 12-bit values as signed 16-bit samples (the low 4 bits zero). Each
following sample n (2 - 31) is

`s[n] = clip16((d << (20 - k)) + s[n-1] * c0 + s[n-2] * c1 + 128 >> 8)`

with d the sample's nibble as a signed 4-bit value (-8 .. 7), k the group's shift, and (c0, c1) the coefficient set:
0 = (0, 0), 1 = (240, 0), 2 = (460, -208), 3 = (392, -220). clip16 clamps to -32768 .. 32767.

[test: rust/bc2-convert/src/audio_tests.rs::frames_decode_as_the_reference] [test: rust/bc2-convert/src/audio_tests.rs::decode_inverts_closely] [code: rust/bc2-convert/src/audio.rs:283-301] [code: rust/bc2-convert/src/audio.rs:56-58]
[doc: bfbc2-console-convert/audio.py:86-88]

### XAS-ENC-1: The suite's encoder: least squared error per group

Status: **verified** · Variants: PC

The suite encodes each group on its own: the first two samples rounded to 12 bits, then the coefficient set and the
shift with the least squared error over the group, each nibble rounded to nearest (half to even), choosing among each
set's needed shift (the least m with residual <= 7 * 2^m) and its finer / coarser neighbours, the first in order on a
tie. This is the same result as trying all 64 (set, shift) pairs. It is the suite's choice, not DICE's encoder; the
game decodes any valid frame.

[data: 41.6 dB SNR on the end credits' music, about 20 - 25 dB on noise-like high-frequency effects (audio.py); every sound of BFBC2's Xbox copy (1,680 Waves) and BF1943 (2,750) gives the retired NumPy reference's bytes (BENCHMARKS E3a, F-round goldens)]
[test: rust/bc2-convert/src/audio_tests.rs::groups_as_the_reference_on_extremes] [test: rust/bc2-convert/src/audio_tests.rs::need_is_the_float_formula] [test: rust/bc2-convert/src/audio_tests.rs::rint_is_half_to_even] [test: bfbc2-console-convert/tests/test_audio_native.py::shift_rule] [code: rust/bc2-convert/src/audio.rs:157-195]
[doc: bfbc2-console-convert/audio.py:92-95]

### XAS-BLK-1: DICE's block length

Status: **verified** · Variants: PC

In DICE's PC streams every audio block but the last holds 256 * ceil(rate / 1280) samples (76 frames a channel at 48
kHz, 70 at 44.1, 50 at 32, 38 at 24, 36 at 22.05, 18 at 11.025 kHz); the last block holds the rest in whole frames, and
is empty (0 samples) when the count is a multiple of the block length; then the end block. The suite's writer cuts
blocks the same way.

[data: all 1,846 PC in-memory EA-XAS streams follow this layout; 3 end with an empty block]
[test: rust/bc2-formats/src/audioframe.rs::blocks_of_dice] [test: bfbc2-console-convert/tests/test_audio.py::dice_layout] [code: rust/bc2-formats/src/audioframe.rs:119-155]
[doc: bfbc2-console-convert/audio.py:89-91]

### XAS-BLK-2: A sound's frames cover whole frames; the sample word holds the true count

Status: **implemented** · Variants: PC

The suite encodes a sound of n samples per channel as ceil(n / 128) frames per channel, samples past the end
encoded as 0 (silence); DICE's blocks (XAS-BLK-1) are then cut from those frames, each block's u32 sample count the
samples it holds per channel, the last block the rest. The stream's sample word (WAVE-STR-1) holds the true total,
so the padding samples of the last frame are not played.

[code: rust/bc2-convert/src/audio.rs:85-94] [code: rust/bc2-convert/src/audio.rs:140] [code: rust/bc2-formats/src/audioframe.rs:129-155]
[inferred: that the game stops at the sample count rather than the frame end is assumed from DICE's own last blocks, which are cut the same way]

## Open questions

- The Wave header's flags and id (word 0x00) and word 0x14: what the engine uses them for.
- The stream-information blob (per platform), and how the game seeks in streamed sounds.
- EALayer3 (codecs 5 - 7): the suite neither reads nor writes it; streamed PC sounds stay as DICE's.
- Whether the game accepts EA-XAS with other block lengths than DICE's (the suite always writes DICE's).
- The Xbox 360's streaming bundles (not in the unpacked console files): their framing is assumed to be BFBC2 PC's SPS.
- The Wave resource's relation to its DBX sound graph (which `Wave` a sound asset plays, voice-over events):
  layer 3, sound.
