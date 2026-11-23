//! Little-endian reading helpers shared by the format readers: every read is bounds-checked and none can panic.

/// `N` bytes at `at`, or `None` past the end.
pub(crate) fn array<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..)?.first_chunk::<N>().copied()
}

/// The `u16`s of `bytes` (a last odd byte is ignored).
pub(crate) fn u16s(bytes: &[u8]) -> impl Iterator<Item = u16> + '_ {
    bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c))
}

/// The `u32`s of `bytes` (a last partial word is ignored).
pub(crate) fn u32s(bytes: &[u8]) -> impl Iterator<Item = u32> + '_ {
    bytes.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c))
}

/// The `f32`s of `bytes` (a last partial word is ignored).
pub(crate) fn f32s(bytes: &[u8]) -> impl Iterator<Item = f32> + '_ {
    bytes.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c))
}

/// The first `K` values of `values`, zero past their end.
pub(crate) fn first<T: Copy + Default, const K: usize>(values: impl IntoIterator<Item = T>) -> [T; K] {
    let mut out = [T::default(); K];
    for (o, v) in out.iter_mut().zip(values) {
        *o = v;
    }
    out
}

/// A finite float's IEEE half bits, to nearest with ties to even (overflow to infinity, tiny values to subnormals
/// and zero).
pub(crate) fn f16_bits(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xFF) as i32;
    let man = b & 0x7F_FFFF;
    if exp == 0xFF {
        return sign | 0x7C00 | u16::from(man != 0);
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign | 0x7C00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = man | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rest = m & ((1 << shift) - 1);
        let mid = 1 << (shift - 1);
        let up = rest > mid || (rest == mid && half & 1 == 1);
        return sign | (half + u32::from(up)) as u16;
    }
    let half = ((e as u32) << 10) | (man >> 13);
    let rest = man & 0x1FFF;
    let up = rest > 0x1000 || (rest == 0x1000 && half & 1 == 1);
    sign | (half + u32::from(up)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_round_to_nearest_even() {
        assert_eq!(f16_bits(1.0), 0x3C00);
        assert_eq!(f16_bits(-2.0), 0xC000);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(65504.0), 0x7BFF);
        assert_eq!(f16_bits(1e6), 0x7C00);
        assert_eq!(f16_bits(58.3125), 0x534A);
        assert_eq!(f16_bits(1.000_488_3), 0x3C00); // a tie: to even
        assert_eq!(f16_bits(6.0e-8), 0x0001);
        assert_eq!(f16_bits(0.0), 0);
    }

    #[test]
    fn reads() {
        let b = [1u8, 0, 2, 0, 0, 0, 0x80, 0x3f, 9];
        assert_eq!(array::<2>(&b, 7), Some([0x3f, 9]));
        assert_eq!(array::<2>(&b, 8), None);
        assert_eq!(array::<2>(&b, 20), None);
        assert_eq!(u16s(&b).collect::<Vec<_>>(), vec![1, 2, 0, 0x3f80]);
        assert_eq!(u32s(&b).collect::<Vec<_>>(), vec![0x0002_0001, 0x3f80_0000]);
        assert_eq!(f32s(&b[4..]).next(), Some(1.0));
        assert_eq!(first::<u32, 3>([7, 8]), [7, 8, 0]);
    }
}
