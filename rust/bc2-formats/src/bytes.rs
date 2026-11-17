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

#[cfg(test)]
mod tests {
    use super::*;

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
