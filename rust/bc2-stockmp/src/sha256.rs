//! SHA-256 (FIPS 180-4), as `hashlib.sha256(...).hexdigest()` gives it: the hashes `pipeline.sha` writes into
//! `build_report.json` and the install markers (round O3).

#![allow(
    clippy::indexing_slicing,
    reason = "fixed-size blocks and schedules, indexed within their lengths"
)]

use std::fmt::Write as _;
use std::io::{self, Read};

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// An incremental SHA-256.
#[derive(Clone)]
pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    len: usize,
    total: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self {
            h: [
                0x6a09_e667,
                0xbb67_ae85,
                0x3c6e_f372,
                0xa54f_f53a,
                0x510e_527f,
                0x9b05_688c,
                0x1f83_d9ab,
                0x5be0_cd19,
            ],
            buf: [0; 64],
            len: 0,
            total: 0,
        }
    }
}

impl Sha256 {
    fn block(&mut self, b: &[u8; 64]) {
        #[cfg(target_arch = "x86_64")]
        if ni::available() {
            ni::compress(&mut self.h, b);
            return;
        }
        self.soft_block(b);
    }

    /// The compression function in portable Rust (FIPS 180-4, 6.2.2).
    #[allow(clippy::many_single_char_names, reason = "the standard's working variables")]
    fn soft_block(&mut self, b: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, c) in b.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*c);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b2, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b2) ^ (a & c) ^ (b2 & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b2;
            b2 = a;
            a = t1.wrapping_add(t2);
        }
        for (x, v) in self.h.iter_mut().zip([a, b2, c, d, e, f, g, h]) {
            *x = x.wrapping_add(v);
        }
    }

    /// More bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if self.len > 0 {
            let n = (64 - self.len).min(data.len());
            self.buf[self.len..self.len + n].copy_from_slice(&data[..n]);
            self.len += n;
            data = &data[n..];
            if self.len == 64 {
                let b = self.buf;
                self.block(&b);
                self.len = 0;
            }
        }
        let (chunks, rest) = data.as_chunks::<64>();
        for c in chunks {
            self.block(c);
        }
        self.buf[..rest.len()].copy_from_slice(rest);
        self.len = rest.len();
    }

    /// The digest as lower-case hex (`hexdigest()`).
    #[must_use]
    pub fn hexdigest(mut self) -> String {
        let bits = self.total.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        let fill = (119 - self.len) % 64;
        pad.extend(std::iter::repeat_n(0u8, fill));
        pad.extend_from_slice(&bits.to_be_bytes());
        let total = self.total;
        self.update(&pad);
        self.total = total;
        self.h.iter().fold(String::with_capacity(64), |mut s, v| {
            let _ = write!(s, "{v:08x}");
            s
        })
    }
}

/// `hashlib.sha256(data).hexdigest()`.
#[must_use]
pub fn hex(data: &[u8]) -> String {
    let mut s = Sha256::default();
    s.update(data);
    s.hexdigest()
}

/// `pipeline.sha(path)`: the file's SHA-256, read a MiB at a time.
///
/// # Errors
/// The file cannot be opened or read.
pub fn file(path: &str) -> io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut s = Sha256::default();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        s.update(buf.get(..n).unwrap_or_default());
    }
    Ok(s.hexdigest())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digests() {
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let long = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&long),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        let mut s = Sha256::default();
        for c in long.chunks(997) {
            s.update(c);
        }
        assert_eq!(
            s.hexdigest(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(hex(&[0u8; 55]), hex([0u8; 55].as_slice()));
    }
}

/// The compression function on the x86 SHA extensions (SHA-NI), as Python's OpenSSL runs it: about 2 GB/s, where
/// the portable one gives a tenth of that (a built bundle of 30 MB is hashed several times by an install).
#[cfg(target_arch = "x86_64")]
#[allow(
    unsafe_code,
    reason = "the SHA-NI intrinsics: unaligned loads and stores of the state and the block"
)]
mod ni {
    use std::arch::x86_64::{
        __m128i, _mm_add_epi32, _mm_alignr_epi8, _mm_blend_epi16, _mm_loadu_si128, _mm_set_epi32, _mm_set_epi64x,
        _mm_sha256msg1_epu32, _mm_sha256msg2_epu32, _mm_sha256rnds2_epu32, _mm_shuffle_epi8, _mm_shuffle_epi32,
        _mm_storeu_si128,
    };

    use super::K;

    /// Whether the CPU has the SHA extensions (and the SSE levels the code uses).
    pub fn available() -> bool {
        std::arch::is_x86_feature_detected!("sha")
            && std::arch::is_x86_feature_detected!("sse4.1")
            && std::arch::is_x86_feature_detected!("ssse3")
    }

    /// One block into the state; only called when [`available`].
    pub fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        // SAFETY: available() checked the CPU features the function is compiled for.
        unsafe { compress_ni(state, block) }
    }

    #[allow(clippy::many_single_char_names, reason = "the standard's round inputs")]
    #[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
    fn schedule(v0: __m128i, v1: __m128i, v2: __m128i, v3: __m128i) -> __m128i {
        let t1 = _mm_sha256msg1_epu32(v0, v1);
        let t2 = _mm_alignr_epi8(v3, v2, 4);
        let t3 = _mm_add_epi32(t1, t2);
        _mm_sha256msg2_epu32(t3, v3)
    }

    /// Four rounds: the message words `w` plus the constants K[4i..4i+4].
    #[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
    fn rounds4(abef: &mut __m128i, cdgh: &mut __m128i, w: __m128i, i: usize) {
        let k = |j: usize| K.get(4 * i + j).copied().unwrap_or(0) as i32;
        let kv = _mm_set_epi32(k(3), k(2), k(1), k(0));
        let t1 = _mm_add_epi32(w, kv);
        *cdgh = _mm_sha256rnds2_epu32(*cdgh, *abef, t1);
        let t2 = _mm_shuffle_epi32(t1, 0x0E);
        *abef = _mm_sha256rnds2_epu32(*abef, *cdgh, t2);
    }

    #[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
    #[allow(
        clippy::cast_ptr_alignment,
        reason = "the loads and stores are the unaligned ones (loadu / storeu)"
    )]
    unsafe fn compress_ni(state: &mut [u32; 8], block: &[u8; 64]) {
        let mask = _mm_set_epi64x(0x0C0D_0E0F_0809_0A0B, 0x0405_0607_0001_0203);
        let sp = state.as_mut_ptr().cast::<__m128i>();
        let bp = block.as_ptr().cast::<__m128i>();
        // SAFETY: the state is 32 bytes and the block 64: two and four unaligned 16-byte loads and stores within them.
        let (dcba, efgh, mut w) = unsafe {
            (
                _mm_loadu_si128(sp),
                _mm_loadu_si128(sp.add(1)),
                [
                    _mm_shuffle_epi8(_mm_loadu_si128(bp), mask),
                    _mm_shuffle_epi8(_mm_loadu_si128(bp.add(1)), mask),
                    _mm_shuffle_epi8(_mm_loadu_si128(bp.add(2)), mask),
                    _mm_shuffle_epi8(_mm_loadu_si128(bp.add(3)), mask),
                ],
            )
        };
        let cdab = _mm_shuffle_epi32(dcba, 0xB1);
        let efgh = _mm_shuffle_epi32(efgh, 0x1B);
        let mut abef = _mm_alignr_epi8(cdab, efgh, 8);
        let mut cdgh = _mm_blend_epi16(efgh, cdab, 0xF0);
        let (abef0, cdgh0) = (abef, cdgh);
        for (i, wi) in w.iter().enumerate() {
            rounds4(&mut abef, &mut cdgh, *wi, i);
        }
        for i in 4..16 {
            // w[i % 4] = schedule(w[i-4], w[i-3], w[i-2], w[i-1])
            let n = schedule(w[i % 4], w[(i + 1) % 4], w[(i + 2) % 4], w[(i + 3) % 4]);
            w[i % 4] = n;
            rounds4(&mut abef, &mut cdgh, n, i);
        }
        abef = _mm_add_epi32(abef, abef0);
        cdgh = _mm_add_epi32(cdgh, cdgh0);
        let feba = _mm_shuffle_epi32(abef, 0x1B);
        let dchg = _mm_shuffle_epi32(cdgh, 0xB1);
        let dcba = _mm_blend_epi16(feba, dchg, 0xF0);
        let hgef = _mm_alignr_epi8(dchg, feba, 8);
        // SAFETY: as the loads above.
        unsafe {
            _mm_storeu_si128(sp, dcba);
            _mm_storeu_si128(sp.add(1), hgef);
        }
    }
}

#[cfg(test)]
mod ni_tests {
    use super::*;

    /// The SHA-NI compression and the portable one give the same states on varied blocks.
    #[test]
    fn ni_as_portable() {
        let mut x: u32 = 0x1234_5678;
        for _ in 0..200 {
            let mut b = [0u8; 64];
            for v in &mut b {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *v = x as u8;
            }
            let mut a = Sha256::default();
            let mut s = Sha256::default();
            a.block(&b);
            s.soft_block(&b);
            assert_eq!(a.h, s.h);
        }
    }
}
