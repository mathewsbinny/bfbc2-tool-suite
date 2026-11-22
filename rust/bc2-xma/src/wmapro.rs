//! FFmpeg's WMA Pro decoder (`libavcodec/wmaprodec.c`) for one XMA2 stream of one or two channels: its packet,
//! frame and subframe parsing, inverse quantisation, channel transform, IMDCT ([`crate::mdct`]) and windowing, with
//! FFmpeg's state and its handling of damaged data (a lost packet, a frame that does not parse).

use std::sync::OnceLock;

use crate::bits::{FrameBuf, MAX_FRAMESIZE, Reader, Vlc};
use crate::mdct::Imdct;
use crate::tables as t;

const MAX_SUBFRAMES: usize = 32;
const MAX_BANDS: usize = 29;
const BLOCK_MIN_BITS: u32 = 6;
const BLOCK_SIZES: usize = 8; // 2^6 .. 2^13
/// XMA's frame: 512 samples.
pub(crate) const SAMPLES_PER_FRAME: usize = 512;
/// The XMA packet (`block_align`).
pub(crate) const PACKET: usize = 2048;
const OUT_LEN: usize = 8192 + 8192 / 2;

struct Vlcs {
    sf: Vlc,
    sf_rl: Vlc,
    coef: [Vlc; 2],
    vec4: Vlc,
    vec2: Vlc,
    vec1: Vlc,
}

fn pairs(tab: &[[u8; 2]]) -> (Vec<u8>, Vec<i32>) {
    (
        tab.iter().map(|p| p[1]).collect(),
        tab.iter().map(|p| i32::from(p[0])).collect(),
    )
}

fn vlcs() -> &'static Vlcs {
    static V: OnceLock<Vlcs> = OnceLock::new();
    V.get_or_init(|| {
        let (l, s) = pairs(&t::SCALE_TABLE);
        let sf = Vlc::new(&l, &s, -60);
        let (l, s) = pairs(&t::SCALE_RL_TABLE);
        let sf_rl = Vlc::new(&l, &s, 0);
        let c0s: Vec<i32> = t::COEF0_SYMS.iter().map(|&v| i32::from(v)).collect();
        let c0 = Vlc::new(&t::COEF0_LENS, &c0s, 0);
        let (l, s) = pairs(&t::COEF1_TABLE);
        let c1 = Vlc::new(&l, &s, 0);
        let v4s: Vec<i32> = t::VEC4_SYMS.iter().map(|&v| i32::from(v)).collect();
        let vec4 = Vlc::new(&t::VEC4_LENS, &v4s, -1);
        let (l, s) = pairs(&t::VEC2_TABLE);
        let vec2 = Vlc::new(&l, &s, -1);
        let (l, s) = pairs(&t::VEC1_TABLE);
        let vec1 = Vlc::new(&l, &s, 0);
        Vlcs {
            sf,
            sf_rl,
            coef: [c0, c1],
            vec4,
            vec2,
            vec1,
        }
    })
}

/// `ff_sine_window_init(window, n)`: `sinf((i + 0.5) * (M_PI / (2.0 * n)))`, the argument rounded to a float as the
/// call to `sinf` rounds it.
fn sine_window(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = ((i as f64 + 0.5) * (std::f64::consts::PI / (2.0 * n as f64))) as f32;
            x.sin()
        })
        .collect()
}

struct Transforms {
    /// windows[k]: the sine window of 2^(k + 6) entries.
    windows: Vec<Vec<f32>>,
}

fn transforms() -> &'static Transforms {
    static T: OnceLock<Transforms> = OnceLock::new();
    T.get_or_init(|| Transforms {
        windows: (0..BLOCK_SIZES)
            .map(|k| sine_window(1 << (k as u32 + BLOCK_MIN_BITS)))
            .collect(),
    })
}

#[derive(Clone)]
struct Channel {
    prev_block_len: i32,
    transmit_coefs: bool,
    num_subframes: usize,
    subframe_len: [u16; MAX_SUBFRAMES],
    cur_subframe: usize,
    decoded_samples: i32,
    grouped: bool,
    quant_step: i32,
    reuse_sf: bool,
    scale_factor_step: i32,
    max_scale_factor: i32,
    saved_scale_factors: [[i32; MAX_BANDS]; 2],
    scale_factor_idx: usize,
    /// Which of `saved_scale_factors` `scale_factors` points at.
    sf_sel: usize,
    table_idx: usize,
    /// `coeffs`: an offset into `out`.
    coeffs: usize,
    num_vec_coeffs: i32,
    out: Vec<f32>,
}

impl Channel {
    fn new() -> Self {
        Channel {
            prev_block_len: SAMPLES_PER_FRAME as i32,
            transmit_coefs: false,
            num_subframes: 0,
            subframe_len: [0; MAX_SUBFRAMES],
            cur_subframe: 0,
            decoded_samples: 0,
            grouped: false,
            quant_step: 0,
            reuse_sf: false,
            scale_factor_step: 0,
            max_scale_factor: 0,
            saved_scale_factors: [[0; MAX_BANDS]; 2],
            scale_factor_idx: 0,
            sf_sel: 0,
            table_idx: 0,
            coeffs: 0,
            num_vec_coeffs: 0,
            out: vec![0.0; OUT_LEN],
        }
    }
}

#[derive(Clone, Default)]
struct ChGroup {
    num_channels: usize,
    transform: bool,
    transform_band: [bool; MAX_BANDS],
    matrix: [f32; 4],
    channels: [usize; 2],
}

/// What a call of `decode_packet` gave.
pub(crate) struct Call {
    /// A negative value is FFmpeg's error (the rest of the demuxer's packet is dropped).
    pub(crate) ret: i32,
    /// A frame of 512 samples a channel (`got_frame`).
    pub(crate) got: bool,
}

#[allow(clippy::struct_excessive_bools, reason = "FFmpeg's context's flags")]
/// One stream's decoder (`WMAProDecodeCtx` with XMA2's settings: decode flags 0x10d6, 16 bits, 512-sample frames).
pub(crate) struct Stream {
    pub(crate) nb_channels: usize,
    log2_frame_size: u32,
    max_num_subframes: usize,
    subframe_len_bits: u32,
    max_subframe_len_bit: bool,
    min_samples_per_subframe: i32,
    num_sfb: [usize; BLOCK_SIZES],
    sfb_offsets: [[i32; MAX_BANDS]; BLOCK_SIZES],
    sf_offsets: [[[usize; MAX_BANDS]; BLOCK_SIZES]; BLOCK_SIZES],
    imdct: Vec<Option<Imdct>>,
    tmp: Vec<f32>,
    fb: FrameBuf,

    // packet state
    packet_offset: usize,
    num_saved_bits: usize,
    frame_offset: usize,
    pub(crate) packet_loss: bool,
    pub(crate) packet_done: bool,
    pub(crate) eof_done: bool,
    pub(crate) skip_packets: u32,
    /// The current packet (2048 bytes or what is left) and the reader's place in it, between calls.
    pkt_pos: usize,

    // frame state
    gb_bits: usize,
    gb_index: usize,
    skip_frame: bool,
    parsed_all_subframes: bool,
    pub(crate) trim_start: u32,
    pub(crate) trim_end: u32,

    // subframe state
    subframe_len: i32,
    channels_for_cur_subframe: usize,
    channel_indexes: [usize; 2],
    num_bands: usize,
    transmit_num_vec_coeffs: bool,
    table_idx: usize,
    esc_len: u32,
    num_chgroups: usize,
    chgroup: [ChGroup; 2],
    channel: Vec<Channel>,
    /// The frame `decode_frame` gave (each channel's 512 samples).
    pub(crate) frame: Vec<Vec<f32>>,
    /// Errors FFmpeg would log (damaged or unexpected data).
    pub(crate) errors: u32,
}

/// `get_rate`: the rate the scale-factor bands are laid out for.
fn band_rate(rate: u32) -> i32 {
    if rate > 44100 {
        48000
    } else if rate > 32000 {
        44100
    } else if rate > 24000 {
        32000
    } else {
        24000
    }
}

fn av_log2(v: u32) -> u32 {
    if v == 0 { 0 } else { v.ilog2() }
}

impl Stream {
    /// `decode_init` for XMA2 (an `XMA2WAVEFORMATEX` header: 34 bytes of extra data).
    pub(crate) fn new(nb_channels: usize, rate: u32) -> Option<Self> {
        if !(1..=2).contains(&nb_channels) || rate == 0 {
            return None;
        }
        let log2_max_num_subframes = (0x10d6u32 & 0x38) >> 3; // 2
        let max_num_subframes = 1usize << log2_max_num_subframes;
        let num_possible_block_sizes = log2_max_num_subframes as usize + 1;
        let mut s = Stream {
            nb_channels,
            log2_frame_size: av_log2(PACKET as u32) + 4,
            max_num_subframes,
            subframe_len_bits: av_log2(log2_max_num_subframes) + 1,
            max_subframe_len_bit: max_num_subframes == 16 || max_num_subframes == 4,
            min_samples_per_subframe: (SAMPLES_PER_FRAME / max_num_subframes) as i32,
            num_sfb: [0; BLOCK_SIZES],
            sfb_offsets: [[0; MAX_BANDS]; BLOCK_SIZES],
            sf_offsets: [[[0; MAX_BANDS]; BLOCK_SIZES]; BLOCK_SIZES],
            imdct: (0..BLOCK_SIZES).map(|_| None).collect(),
            tmp: vec![0.0; 8192],
            fb: FrameBuf::new(),
            packet_offset: 0,
            num_saved_bits: 0,
            frame_offset: 0,
            packet_loss: true,
            packet_done: false,
            eof_done: false,
            skip_packets: 0,
            pkt_pos: 0,
            gb_bits: 0,
            gb_index: 0,
            skip_frame: true,
            parsed_all_subframes: false,
            trim_start: 0,
            trim_end: 0,
            subframe_len: 0,
            channels_for_cur_subframe: 0,
            channel_indexes: [0; 2],
            num_bands: 0,
            transmit_num_vec_coeffs: false,
            table_idx: 0,
            esc_len: 0,
            num_chgroups: 0,
            chgroup: [ChGroup::default(), ChGroup::default()],
            channel: (0..nb_channels).map(|_| Channel::new()).collect(),
            frame: vec![vec![0.0; SAMPLES_PER_FRAME]; nb_channels],
            errors: 0,
        };
        let brate = band_rate(rate);
        for i in 0..num_possible_block_sizes {
            let subframe_len = (SAMPLES_PER_FRAME >> i) as i32;
            let mut band = 1usize;
            s.sfb_offsets[i][0] = 0;
            let mut x = 0;
            while x < MAX_BANDS - 1 && s.sfb_offsets[i][band - 1] < subframe_len {
                let mut offset = (subframe_len * 2 * i32::from(t::CRITICAL_FREQ[x])) / brate + 2;
                offset &= !3;
                if offset > s.sfb_offsets[i][band - 1] {
                    s.sfb_offsets[i][band] = offset;
                    band += 1;
                }
                if offset >= subframe_len {
                    break;
                }
                x += 1;
            }
            s.sfb_offsets[i][band - 1] = subframe_len;
            s.num_sfb[i] = band - 1;
            if s.num_sfb[i] == 0 {
                return None;
            }
        }
        for i in 0..num_possible_block_sizes {
            for b in 0..s.num_sfb[i] {
                let offset = ((s.sfb_offsets[i][b] + s.sfb_offsets[i][b + 1] - 1) << i) >> 1;
                for x in 0..num_possible_block_sizes {
                    let mut v = 0;
                    while s.sfb_offsets[x][v + 1] << x < offset {
                        v += 1;
                        if v >= MAX_BANDS - 1 {
                            return None;
                        }
                    }
                    s.sf_offsets[i][x][b] = v;
                }
            }
        }
        // the transforms of the sizes a frame of 512 can hold (FFmpeg makes all eight; the others are never used)
        for i in 0..BLOCK_SIZES {
            let len = 1usize << (BLOCK_MIN_BITS as usize + i);
            if len <= SAMPLES_PER_FRAME && len >= s.min_samples_per_subframe as usize {
                let scale = (1.0 / f64::from(1u32 << (BLOCK_MIN_BITS as usize + i - 1)) / f64::from(1u32 << 15)) as f32;
                s.imdct[i] = Some(Imdct::new(len, scale));
            }
        }
        Some(s)
    }

    /// `decode_subframe_length`.
    fn decode_subframe_length(&self, gb: &mut Reader<'_>, offset: i32) -> i32 {
        if offset == SAMPLES_PER_FRAME as i32 - self.min_samples_per_subframe {
            return self.min_samples_per_subframe;
        }
        if (self.gb_bits as i64 - gb.count() as i64) < 1 {
            return -1;
        }
        let mut frame_len_shift = 0;
        if self.max_subframe_len_bit {
            if gb.bit() == 1 {
                frame_len_shift = 1 + gb.bits(self.subframe_len_bits - 1);
            }
        } else {
            frame_len_shift = gb.bits(self.subframe_len_bits);
        }
        let subframe_len = (SAMPLES_PER_FRAME >> frame_len_shift) as i32;
        if subframe_len < self.min_samples_per_subframe || subframe_len > SAMPLES_PER_FRAME as i32 {
            return -1;
        }
        subframe_len
    }

    /// `decode_tilehdr`.
    fn decode_tilehdr(&mut self, gb: &mut Reader<'_>) -> bool {
        let n = self.nb_channels;
        let mut num_samples = [0i32; 2];
        let mut contains = [false; 2];
        let mut channels_for_cur_subframe = n;
        let mut min_channel_len = 0i32;
        for ch in &mut self.channel {
            ch.num_subframes = 0;
        }
        let fixed_channel_layout = self.max_num_subframes == 1 || gb.bit() == 1;
        loop {
            for c in 0..n {
                contains[c] = if num_samples[c] == min_channel_len {
                    if fixed_channel_layout
                        || channels_for_cur_subframe == 1
                        || min_channel_len == SAMPLES_PER_FRAME as i32 - self.min_samples_per_subframe
                    {
                        true
                    } else {
                        gb.bit() == 1
                    }
                } else {
                    false
                };
            }
            let subframe_len = self.decode_subframe_length(gb, min_channel_len);
            if subframe_len <= 0 {
                return false;
            }
            min_channel_len += subframe_len;
            for c in 0..n {
                let chan = &mut self.channel[c];
                if contains[c] {
                    if chan.num_subframes >= MAX_SUBFRAMES {
                        return false;
                    }
                    chan.subframe_len[chan.num_subframes] = subframe_len as u16;
                    num_samples[c] += subframe_len;
                    chan.num_subframes += 1;
                    if num_samples[c] > SAMPLES_PER_FRAME as i32 {
                        return false;
                    }
                } else if num_samples[c] <= min_channel_len {
                    if num_samples[c] < min_channel_len {
                        channels_for_cur_subframe = 0;
                        min_channel_len = num_samples[c];
                    }
                    channels_for_cur_subframe += 1;
                }
            }
            if min_channel_len >= SAMPLES_PER_FRAME as i32 {
                break;
            }
        }
        true
    }

    /// `decode_channel_transform` (one or two channels: no decorrelation matrix, no channel mask).
    fn decode_channel_transform(&mut self, gb: &mut Reader<'_>) -> bool {
        self.num_chgroups = 0;
        if self.nb_channels > 1 {
            let mut remaining = self.channels_for_cur_subframe;
            if gb.bit() == 1 {
                return false; // "Channel transform bit"
            }
            while remaining > 0 && self.num_chgroups < self.channels_for_cur_subframe {
                let g = self.num_chgroups;
                let mut grp = ChGroup {
                    transform_band: self.chgroup[g].transform_band,
                    matrix: self.chgroup[g].matrix,
                    ..ChGroup::default()
                };
                // remaining is at most 2: the group takes the channels left
                grp.num_channels = remaining;
                let mut k = 0;
                for i in 0..self.channels_for_cur_subframe {
                    let ci = self.channel_indexes[i];
                    if !self.channel[ci].grouped {
                        grp.channels[k] = ci;
                        k += 1;
                    }
                    self.channel[ci].grouped = true;
                }
                if grp.num_channels == 2 {
                    if gb.bit() == 1 {
                        if gb.bit() == 1 {
                            return false; // "Unknown channel transform type"
                        }
                    } else {
                        grp.transform = true;
                        grp.matrix = [1.0, -1.0, 1.0, 1.0]; // two channels in the stream
                    }
                }
                if grp.transform {
                    if gb.bit() == 0 {
                        for tb in grp.transform_band.iter_mut().take(self.num_bands) {
                            *tb = gb.bit() == 1;
                        }
                    } else {
                        for tb in grp.transform_band.iter_mut().take(self.num_bands) {
                            *tb = true;
                        }
                    }
                }
                remaining -= grp.num_channels;
                self.chgroup[g] = grp;
                self.num_chgroups += 1;
            }
        }
        true
    }

    /// `decode_coeffs` for channel `c`.
    fn decode_coeffs(&mut self, gb: &mut Reader<'_>, c: usize) {
        let v = vlcs();
        let vlctable = gb.bit() as usize;
        let (run, level): (&[u16], &[f32]) = if vlctable == 1 {
            (&t::COEF1_RUN, &t::COEF1_LEVEL)
        } else {
            (&t::COEF0_RUN, &t::COEF0_LEVEL)
        };
        let subframe_len = self.subframe_len;
        let ci = &mut self.channel[c];
        let base = ci.coeffs;
        let mut rl_mode = false;
        let mut cur_coeff: i32 = 0;
        let mut num_zeros: i32 = 0;
        while (self.transmit_num_vec_coeffs || !rl_mode) && cur_coeff + 3 < ci.num_vec_coeffs {
            let mut vals = [0u32; 4];
            let idx = v.vec4.read(gb);
            if idx < 0 {
                let mut i = 0;
                while i < 4 {
                    let idx = v.vec2.read(gb);
                    if idx < 0 {
                        let mut v0 = v.vec1.read(gb) as u32;
                        if v0 == t::HUFF_VEC1_SIZE as u32 - 1 {
                            v0 = v0.wrapping_add(large_val(gb));
                        }
                        let mut v1 = v.vec1.read(gb) as u32;
                        if v1 == t::HUFF_VEC1_SIZE as u32 - 1 {
                            v1 = v1.wrapping_add(large_val(gb));
                        }
                        vals[i] = (v0 as f32).to_bits();
                        vals[i + 1] = (v1 as f32).to_bits();
                    } else {
                        let idx = idx as u32;
                        vals[i] = (((idx >> 4) & 0xF) as f32).to_bits();
                        vals[i + 1] = ((idx & 0xF) as f32).to_bits();
                    }
                    i += 2;
                }
            } else {
                let idx = idx as u32;
                vals[0] = ((idx >> 12) as f32).to_bits();
                vals[1] = (((idx >> 8) & 0xF) as f32).to_bits();
                vals[2] = (((idx >> 4) & 0xF) as f32).to_bits();
                vals[3] = ((idx & 0xF) as f32).to_bits();
            }
            for &val in &vals {
                let at = base + cur_coeff as usize;
                if val != 0 {
                    let sign = gb.bit().wrapping_sub(1);
                    ci.out[at] = f32::from_bits(val ^ (sign << 31));
                    num_zeros = 0;
                } else {
                    ci.out[at] = 0.0;
                    num_zeros += 1;
                    rl_mode |= num_zeros > subframe_len >> 8;
                }
                cur_coeff += 1;
            }
        }
        if cur_coeff < subframe_len {
            for x in &mut ci.out[base + cur_coeff as usize..base + subframe_len as usize] {
                *x = 0.0;
            }
            // ff_wma_run_level_decode(version 1); its error is not checked by decode_coeffs
            let coef_mask = (subframe_len - 1) as usize;
            let vlc = &v.coef[vlctable];
            let mut offset = cur_coeff;
            while offset < subframe_len {
                let code = vlc.read(gb);
                #[allow(clippy::comparison_chain, reason = "FFmpeg's order")]
                if code > 1 {
                    offset += i32::from(run[code as usize]);
                    let sign = gb.bit().wrapping_sub(1);
                    let bits = level[code as usize].to_bits() ^ (sign & 0x8000_0000);
                    ci.out[base + (offset as usize & coef_mask)] = f32::from_bits(bits);
                } else if code == 1 {
                    break;
                } else {
                    let lvl = large_val(gb) as i32;
                    if gb.bit() == 1 {
                        if gb.bit() == 1 {
                            if gb.bit() == 1 {
                                self.errors += 1; // "broken escape sequence"
                                return;
                            }
                            offset += gb.bits(self.esc_len) as i32 + 4;
                        } else {
                            offset += gb.bits(2) as i32 + 1;
                        }
                    }
                    let sign = gb.bit() as i32 - 1;
                    ci.out[base + (offset as usize & coef_mask)] = ((lvl ^ sign) - sign) as f32;
                }
                offset += 1;
            }
            if offset > subframe_len {
                self.errors += 1; // "overflow in spectral RLE, ignoring"
            }
        }
    }

    /// `decode_scale_factors`.
    fn decode_scale_factors(&mut self, gb: &mut Reader<'_>) -> bool {
        let v = vlcs();
        for i in 0..self.channels_for_cur_subframe {
            let c = self.channel_indexes[i];
            let nb = self.num_bands;
            let table_idx = self.table_idx;
            let ch = &mut self.channel[c];
            ch.sf_sel = usize::from(ch.scale_factor_idx == 0);
            let sel = ch.sf_sel;
            if ch.reuse_sf {
                let sfo = self.sf_offsets[table_idx][ch.table_idx];
                for b in 0..nb {
                    ch.saved_scale_factors[sel][b] = ch.saved_scale_factors[ch.scale_factor_idx][sfo[b]];
                }
            }
            if ch.cur_subframe == 0 || gb.bit() == 1 {
                if ch.reuse_sf {
                    let mut i = 0usize;
                    while i < nb {
                        let idx = v.sf_rl.read(gb);
                        let (skip, val, sign);
                        if idx == 0 {
                            let code = gb.bits(14);
                            val = (code >> 6) as i32;
                            sign = (code & 1) as i32 - 1;
                            skip = ((code & 0x3f) >> 1) as usize;
                        } else if idx == 1 {
                            break;
                        } else {
                            skip = usize::from(t::SCALE_RL_RUN[idx as usize]);
                            val = i32::from(t::SCALE_RL_LEVEL[idx as usize]);
                            sign = gb.bit() as i32 - 1;
                        }
                        i += skip;
                        if i >= nb {
                            self.errors += 1; // "invalid scale factor coding"
                            return false;
                        }
                        ch.saved_scale_factors[sel][i] += (val ^ sign) - sign;
                        i += 1;
                    }
                } else {
                    ch.scale_factor_step = gb.bits(2) as i32 + 1;
                    let mut val = 45 / ch.scale_factor_step;
                    for sf in ch.saved_scale_factors[sel].iter_mut().take(nb) {
                        val += v.sf.read(gb);
                        *sf = val;
                    }
                }
                ch.scale_factor_idx = usize::from(ch.scale_factor_idx == 0);
                ch.table_idx = table_idx;
                ch.reuse_sf = true;
            }
            let sf = &ch.saved_scale_factors[sel];
            ch.max_scale_factor = sf[..nb].iter().copied().max().unwrap_or(sf[0]).max(sf[0]);
        }
        true
    }

    /// `inverse_channel_transform`.
    fn inverse_channel_transform(&mut self) {
        let sfb = self.sfb_offsets[self.table_idx];
        for g in 0..self.num_chgroups {
            let grp = self.chgroup[g].clone();
            if !grp.transform {
                continue;
            }
            let [c0, c1] = grp.channels;
            for b in 0..self.num_bands {
                let (lo, hi) = (sfb[b] as usize, sfb[b + 1].min(self.subframe_len) as usize);
                let (o0, o1) = (self.channel[c0].coeffs, self.channel[c1].coeffs);
                if grp.transform_band[b] {
                    let m = grp.matrix;
                    for y in lo..hi {
                        let d0 = self.channel[c0].out[o0 + y];
                        let d1 = self.channel[c1].out[o1 + y];
                        let mut s0 = 0f32;
                        s0 += d0 * m[0];
                        s0 += d1 * m[1];
                        let mut s1 = 0f32;
                        s1 += d0 * m[2];
                        s1 += d1 * m[3];
                        self.channel[c0].out[o0 + y] = s0;
                        self.channel[c1].out[o1 + y] = s1;
                    }
                } else if self.nb_channels == 2 {
                    let mul = (181.0f64 / 128.0) as f32;
                    for y in lo..hi {
                        self.channel[c0].out[o0 + y] *= mul;
                    }
                    for y in lo..hi {
                        self.channel[c1].out[o1 + y] *= mul;
                    }
                }
            }
        }
    }

    /// `wmapro_window`.
    fn window(&mut self) {
        let tr = transforms();
        for i in 0..self.channels_for_cur_subframe {
            let c = self.channel_indexes[i];
            let ch = &mut self.channel[c];
            let mut winlen = ch.prev_block_len as usize;
            let mut start = ch.coeffs - (winlen >> 1);
            if (self.subframe_len as usize) < winlen {
                start += (winlen - self.subframe_len as usize) >> 1;
                winlen = self.subframe_len as usize;
            }
            let win = &tr.windows[(av_log2(winlen as u32) - BLOCK_MIN_BITS) as usize];
            let len = winlen >> 1;
            // vector_fmul_window(start, start, start + len, win, len), in place
            for k in 0..len {
                let (ii, jj) = (k, 2 * len - 1 - k);
                let s0 = ch.out[start + ii];
                let s1 = ch.out[start + jj];
                let (wi, wj) = (win[ii], win[jj]);
                ch.out[start + ii] = s0 * wj - s1 * wi;
                ch.out[start + jj] = s0 * wi + s1 * wj;
            }
            ch.prev_block_len = self.subframe_len;
        }
    }

    /// `decode_subframe`.
    #[allow(clippy::too_many_lines, reason = "FFmpeg's function, kept whole to compare")]
    fn decode_subframe(&mut self, gb: &mut Reader<'_>) -> bool {
        let spf = SAMPLES_PER_FRAME as i32;
        let mut offset = spf;
        let mut subframe_len = spf;
        let mut total_samples = spf * self.nb_channels as i32;
        let mut transmit_coeffs = false;
        for ch in &mut self.channel {
            ch.grouped = false;
            if offset > ch.decoded_samples {
                offset = ch.decoded_samples;
                subframe_len = i32::from(ch.subframe_len[ch.cur_subframe.min(MAX_SUBFRAMES - 1)]);
            }
        }
        self.channels_for_cur_subframe = 0;
        for i in 0..self.nb_channels {
            let ch = &mut self.channel[i];
            let cur = ch.cur_subframe.min(MAX_SUBFRAMES - 1);
            total_samples -= ch.decoded_samples;
            if offset == ch.decoded_samples && subframe_len == i32::from(ch.subframe_len[cur]) {
                total_samples -= i32::from(ch.subframe_len[cur]);
                ch.decoded_samples += i32::from(ch.subframe_len[cur]);
                self.channel_indexes[self.channels_for_cur_subframe] = i;
                self.channels_for_cur_subframe += 1;
            }
        }
        if total_samples == 0 {
            self.parsed_all_subframes = true;
        }
        if subframe_len <= 0 {
            return false;
        }
        self.table_idx = av_log2((spf / subframe_len) as u32) as usize;
        self.num_bands = self.num_sfb[self.table_idx];
        offset += spf >> 1;
        for i in 0..self.channels_for_cur_subframe {
            let c = self.channel_indexes[i];
            self.channel[c].coeffs = offset as usize;
        }
        self.subframe_len = subframe_len;
        self.esc_len = av_log2((subframe_len - 1) as u32) + 1;

        if gb.bit() == 1 {
            let mut num_fill_bits = gb.bits(2) as i32;
            if num_fill_bits == 0 {
                let len = gb.bits(4);
                num_fill_bits = gb.bitsz(len) as i32 + 1;
            }
            if num_fill_bits >= 0 {
                if gb.count() + num_fill_bits as usize > self.num_saved_bits {
                    self.errors += 1; // "invalid number of fill bits"
                    return false;
                }
                gb.skip_long(i64::from(num_fill_bits));
            }
        }
        if gb.bit() == 1 {
            self.errors += 1; // "Reserved bit"
            return false;
        }
        if !self.decode_channel_transform(gb) {
            self.errors += 1;
            return false;
        }
        for i in 0..self.channels_for_cur_subframe {
            let c = self.channel_indexes[i];
            let tc = gb.bit() == 1;
            self.channel[c].transmit_coefs = tc;
            transmit_coeffs |= tc;
        }
        if transmit_coeffs {
            let mut quant_step = (90 * 16) >> 4;
            self.transmit_num_vec_coeffs = gb.bit() == 1;
            if self.transmit_num_vec_coeffs {
                let num_bits = av_log2(((self.subframe_len + 3) / 4) as u32) + 1;
                for i in 0..self.channels_for_cur_subframe {
                    let c = self.channel_indexes[i];
                    let n = (gb.bits(num_bits) << 2) as i32;
                    if n > self.subframe_len {
                        self.errors += 1; // "num_vec_coeffs is too large"
                        return false;
                    }
                    self.channel[c].num_vec_coeffs = n;
                }
            } else {
                for i in 0..self.channels_for_cur_subframe {
                    let c = self.channel_indexes[i];
                    self.channel[c].num_vec_coeffs = self.subframe_len;
                }
            }
            let mut step = gb.sbits(6);
            quant_step += step;
            if step == -32 || step == 31 {
                let sign = i32::from(step == 31) - 1;
                let mut quant = 0;
                loop {
                    if gb.count() + 5 >= self.num_saved_bits {
                        break;
                    }
                    step = gb.bits(5) as i32;
                    if step != 31 {
                        break;
                    }
                    quant += 31;
                }
                quant_step += ((quant + step) ^ sign) - sign;
            }
            if self.channels_for_cur_subframe == 1 {
                let c = self.channel_indexes[0];
                self.channel[c].quant_step = quant_step;
            } else {
                let modifier_len = gb.bits(3);
                for i in 0..self.channels_for_cur_subframe {
                    let c = self.channel_indexes[i];
                    self.channel[c].quant_step = quant_step;
                    if gb.bit() == 1 {
                        if modifier_len > 0 {
                            self.channel[c].quant_step += gb.bits(modifier_len) as i32 + 1;
                        } else {
                            self.channel[c].quant_step += 1;
                        }
                    }
                }
            }
            if !self.decode_scale_factors(gb) {
                return false;
            }
        }
        for i in 0..self.channels_for_cur_subframe {
            let c = self.channel_indexes[i];
            if self.channel[c].transmit_coefs && gb.count() < self.num_saved_bits {
                self.decode_coeffs(gb, c);
            } else {
                let o = self.channel[c].coeffs;
                for x in &mut self.channel[c].out[o..o + subframe_len as usize] {
                    *x = 0.0;
                }
            }
        }
        if transmit_coeffs {
            let ti = (av_log2(subframe_len as u32) - BLOCK_MIN_BITS) as usize;
            self.inverse_channel_transform();
            let sfb = self.sfb_offsets[self.table_idx];
            for i in 0..self.channels_for_cur_subframe {
                let c = self.channel_indexes[i];
                let ch = &mut self.channel[c];
                let sf = ch.saved_scale_factors[ch.sf_sel];
                for b in 0..self.num_bands {
                    let end = sfb[b + 1].min(self.subframe_len) as usize;
                    let exp = ch.quant_step - (ch.max_scale_factor - sf[b]) * ch.scale_factor_step;
                    let quant = (std::f64::consts::LOG2_10 * (f64::from(exp) / 20.0)).exp2() as f32;
                    let start = sfb[b] as usize;
                    for k in start..end {
                        self.tmp[k] = ch.out[ch.coeffs + k] * quant;
                    }
                }
                let Some(tx) = self.imdct.get_mut(ti).and_then(Option::as_mut) else {
                    return false;
                };
                let o = ch.coeffs;
                tx.run(
                    &mut ch.out[o..o + subframe_len as usize],
                    &self.tmp[..subframe_len as usize],
                );
            }
        }
        self.window();
        for i in 0..self.channels_for_cur_subframe {
            let c = self.channel_indexes[i];
            if self.channel[c].cur_subframe >= self.channel[c].num_subframes {
                self.errors += 1; // "broken subframe"
                return false;
            }
            self.channel[c].cur_subframe += 1;
        }
        true
    }

    /// `decode_frame`: whether more frames follow in the packet; a frame that does not parse sets `packet_loss`.
    fn decode_frame(&mut self, got: &mut bool) -> bool {
        let fb_data = std::mem::take(&mut self.fb.data);
        let mut gb = Reader::new(&fb_data, self.gb_bits);
        gb.skip_long(self.gb_index as i64);
        let more = self.decode_frame_in(&mut gb, got);
        self.gb_index = gb.count();
        self.fb.data = fb_data;
        more
    }

    fn decode_frame_in(&mut self, gb: &mut Reader<'_>, got: &mut bool) -> bool {
        let len = gb.bits(self.log2_frame_size) as usize;
        if !self.decode_tilehdr(gb) {
            self.errors += 1;
            self.packet_loss = true;
            return false;
        }
        if self.nb_channels > 1 && gb.bit() == 1 && gb.bit() == 1 {
            for _ in 0..self.nb_channels * self.nb_channels {
                gb.skip(4);
            }
        }
        let _drc_gain = gb.bits(8);
        if gb.bit() == 1 {
            if gb.bit() == 1 {
                self.trim_start = gb.bits(av_log2(SAMPLES_PER_FRAME as u32 * 2));
            }
            if gb.bit() == 1 {
                self.trim_end = gb.bits(av_log2(SAMPLES_PER_FRAME as u32 * 2));
            }
        } else {
            self.trim_start = 0;
            self.trim_end = 0;
        }
        self.parsed_all_subframes = false;
        for ch in &mut self.channel {
            ch.decoded_samples = 0;
            ch.cur_subframe = 0;
            ch.reuse_sf = false;
        }
        while !self.parsed_all_subframes {
            if !self.decode_subframe(gb) {
                self.packet_loss = true;
                return false;
            }
        }
        for (f, ch) in self.frame.iter_mut().zip(&mut self.channel) {
            f.copy_from_slice(&ch.out[..SAMPLES_PER_FRAME]);
            ch.out
                .copy_within(SAMPLES_PER_FRAME..SAMPLES_PER_FRAME + SAMPLES_PER_FRAME / 2, 0);
        }
        if self.skip_frame {
            self.skip_frame = false;
            *got = false;
        } else {
            *got = true;
        }
        let used = gb.count() as i64 - self.frame_offset as i64;
        if len as i64 != used + 2 {
            self.errors += 1; // "frame would have to skip bits"
            self.packet_loss = true;
            return false;
        }
        gb.skip_long(len as i64 - used - 1);
        gb.bit() == 1
    }

    /// `save_bits(gb, len, append)`.
    fn save_bits(&mut self, pgb: &mut Reader<'_>, len: usize, append: bool) {
        let buflen = if append {
            (self.fb.count() + len + 7) >> 3
        } else {
            self.frame_offset = pgb.count() & 7;
            self.num_saved_bits = self.frame_offset;
            self.fb.reset();
            (self.num_saved_bits + len + 7) >> 3
        };
        if len == 0 || buflen > MAX_FRAMESIZE {
            self.errors += 1; // "Too small input buffer"
            self.packet_loss = true;
            return;
        }
        if len > self.fb.left() {
            self.errors += 1;
            self.packet_loss = true;
            return;
        }
        self.num_saved_bits += len;
        let mut len = len;
        if append {
            let align = (8 - (pgb.count() & 7)).min(len);
            let v = pgb.bits(align as u32);
            self.fb.put(align as u32, v);
            len -= align;
            self.fb.copy_bits(pgb.tail(), len);
        } else {
            let n = self.num_saved_bits;
            self.fb.copy_bits(pgb.tail(), n);
        }
        pgb.skip_long(len as i64);
        self.fb.flush();
        self.gb_bits = self.num_saved_bits;
        self.gb_index = self.frame_offset;
    }

    /// `decode_packet` on what is left of the demuxer's packet (`buf`; empty: the end of the stream).
    pub(crate) fn decode_packet(&mut self, buf: &[u8]) -> Call {
        let mut got = false;
        if buf.is_empty() {
            self.packet_done = false;
            if self.eof_done {
                return Call { ret: 0, got: false };
            }
            for (f, ch) in self.frame.iter_mut().zip(&self.channel) {
                f.fill(0.0);
                f[..SAMPLES_PER_FRAME / 2].copy_from_slice(&ch.out[..SAMPLES_PER_FRAME / 2]);
            }
            self.eof_done = true;
            self.packet_done = true;
            return Call { ret: 0, got: true };
        }
        let buf_size;
        if self.packet_done || self.packet_loss {
            self.packet_done = false;
            buf_size = buf.len().min(PACKET);
            self.pkt_pos = buf_size;
            let mut pgb = Reader::new(buf, buf_size * 8);
            let _num_frames = pgb.bits(6);
            let mut num_bits_prev_frame = pgb.bits(self.log2_frame_size) as usize;
            pgb.skip(3);
            self.skip_packets = pgb.bits(8);
            if num_bits_prev_frame > 0 {
                let remaining = buf_size * 8 - pgb.count();
                if num_bits_prev_frame >= remaining {
                    num_bits_prev_frame = remaining;
                    self.packet_done = true;
                }
                self.save_bits(&mut pgb, num_bits_prev_frame, true);
                if !self.packet_loss {
                    self.decode_frame(&mut got);
                }
            }
            if self.packet_loss {
                self.num_saved_bits = 0;
                self.packet_loss = false;
            }
            return self.packet_end(&mut pgb, buf_size, got);
        }
        // the rest of the current packet: from buf (the packet's unread bytes) to its end
        buf_size = self.pkt_pos.min(buf.len());
        let start_bits = self.packet_offset;
        let mut pgb = Reader::new(buf, buf_size * 8);
        pgb.skip(start_bits);
        let remaining = |g: &Reader<'_>| buf_size as i64 * 8 - g.count() as i64;
        let frame_size;
        if remaining(&pgb) > i64::from(self.log2_frame_size)
            && {
                frame_size = pgb.show(self.log2_frame_size) as usize;
                frame_size != 0
            }
            && frame_size as i64 <= remaining(&pgb)
        {
            self.save_bits(&mut pgb, frame_size, false);
            if !self.packet_loss {
                self.packet_done = !self.decode_frame(&mut got);
            }
        } else {
            self.packet_done = true;
        }
        self.packet_end(&mut pgb, buf_size, got)
    }

    /// The end of `decode_packet`: the overread check, the rest of a finished packet saved, the bytes used.
    fn packet_end(&mut self, pgb: &mut Reader<'_>, buf_size: usize, got: bool) -> Call {
        let remaining = |g: &Reader<'_>| buf_size as i64 * 8 - g.count() as i64;
        if remaining(pgb) < 0 {
            self.errors += 1; // "Overread"
            self.packet_loss = true;
        }
        if self.packet_done && !self.packet_loss && remaining(pgb) > 0 {
            let r = remaining(pgb) as usize;
            self.save_bits(pgb, r, false);
        }
        self.packet_offset = pgb.count() & 7;
        let used = pgb.count() >> 3;
        // the next call's buffer starts `used` bytes on: the packet's end moves back by as much
        self.pkt_pos = buf_size.saturating_sub(used);
        if self.packet_loss {
            return Call { ret: -1, got };
        }
        Call { ret: used as i32, got }
    }
}

/// `ff_wma_get_large_val`.
fn large_val(gb: &mut Reader<'_>) -> u32 {
    let mut n_bits = 8;
    if gb.bit() == 1 {
        n_bits += 8;
        if gb.bit() == 1 {
            n_bits += 8;
            if gb.bit() == 1 {
                n_bits += 7;
            }
        }
    }
    gb.bits_long(n_bits)
}
