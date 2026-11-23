//! The generator's random source (TGEN-GEN-1): a counter-based generator, every value a hash of (seed, stream,
//! index), so a value never depends on the order values are drawn in, the platform or the number of threads.

/// `SplitMix64`'s finaliser: a bijective mix of 64 bits.
#[must_use]
pub const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The random words of one stream of one seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream {
    key: u64,
}

impl Stream {
    /// Stream `stream` of `seed` (the generator's steps each take their own stream).
    #[must_use]
    pub const fn new(seed: u64, stream: u64) -> Self {
        Self {
            key: mix(mix(seed ^ 0x9E37_79B9_7F4A_7C15).wrapping_add(stream.wrapping_mul(0xD1B5_4A32_D192_ED03))),
        }
    }

    /// The `index`-th 64-bit word.
    #[must_use]
    pub const fn word(self, index: u64) -> u64 {
        mix(self.key ^ mix(index.wrapping_add(0x9E37_79B9_7F4A_7C15)))
    }

    /// The `index`-th value uniform in (0, 1) (never 0 or 1).
    #[must_use]
    pub const fn unit(self, index: u64) -> f64 {
        ((self.word(index) >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// The `index`-th standard normal value (Box-Muller on two words).
    #[must_use]
    pub fn normal(self, index: u64) -> f64 {
        let u1 = self.unit(2 * index);
        let u2 = self.unit(2 * index + 1);
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_a_function_of_seed_stream_and_index() {
        let a = Stream::new(7, 1);
        assert_eq!(a.word(5), Stream::new(7, 1).word(5));
        assert_ne!(a.word(5), Stream::new(8, 1).word(5));
        assert_ne!(a.word(5), Stream::new(7, 2).word(5));
        let n = 20_000u64;
        let (mut s, mut s2) = (0.0, 0.0);
        for i in 0..n {
            let v = a.normal(i);
            s += v;
            s2 += v * v;
        }
        let mean = s / 20_000.0;
        let var = s2 / 20_000.0 - mean * mean;
        assert!(mean.abs() < 0.03 && (var - 1.0).abs() < 0.05, "{mean} {var}");
        assert!((0..1000).all(|i| (0.0..1.0).contains(&a.unit(i)) && a.unit(i) > 0.0));
    }
}
