# bc2-xma: an XMA2 decoder

FFmpeg's `xma2` decoder (`libavcodec/wmaprodec.c`, FFmpeg 9.0) ported to Rust, so the suite decodes the Xbox 360's
EA-XMA sounds in process instead of running ffmpeg once per sound (round PR1b-1c). It follows FFmpeg's C code
operation for operation, from the WAV demuxer's packets to libswresample's 16-bit samples, and gives
`ffmpeg -cpuflags 0`'s samples to the bit on every XMA stream of BFBC2's Xbox copy and Battlefield 1943 (spec rule
XMA-DEC-1 in `spec/formats/audio.md`; the measurements in `rust/BENCHMARKS.md`, "PR1b-1c").

    let d = bc2_xma::decode(packets, channels, rate, byte_rate)?;   // one stream of 1 or 2 channels
    let pcm = bc2_xma::to_s16(&d.planar);                            // interleaved, as ffmpeg's s16le

## Licence

This crate is a translation of FFmpeg's code and tables (`wmaprodec.c`, `wmaprodata.h`, `wma.c`, `vlc.c`,
`libavutil/tx_template.c`, `float_dsp.c`, `sinewin_tablegen.h`, `wavdec.c`, `pcm.c`, `decode.c`), whose authors hold
their copyright (Baptiste Coudurier, Benjamin Larsson, Ulion, Sascha Sommer, Lynne and the other FFmpeg
developers). It is therefore licensed under FFmpeg's licence, the **GNU Lesser General Public License, version 2.1
or (at your option) any later version** (`license = "LGPL-2.1-or-later"` in its `Cargo.toml`; the user's choice,
2026-11-22), not under the suite's MIT licence. The licence's text is `LICENSE` beside this file. The rest of the
suite (MIT, the repository's `LICENSE`) links it as a library into `bc2_native` and `bfbc2.exe`; its source is this
folder.

FFmpeg is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty
of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Lesser General Public License for more details:
<https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>.
