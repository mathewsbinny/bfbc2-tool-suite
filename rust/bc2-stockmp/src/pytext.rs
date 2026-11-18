//! Python's `str.upper` / `str.lower` on a string's bytes (`dbxbin` decodes them with `surrogateescape`: the bytes
//! that are not UTF-8 stand for surrogates, which neither changes), and Python's `str()` of the few values the port
//! formats.

/// `s.upper()` of the string whose bytes are `s`.
#[must_use]
pub fn upper(s: &[u8]) -> Vec<u8> {
    if s.is_ascii() {
        return s.to_ascii_uppercase();
    }
    let mut out = Vec::with_capacity(s.len());
    for chunk in s.utf8_chunks() {
        out.extend_from_slice(chunk.valid().to_uppercase().as_bytes());
        out.extend_from_slice(chunk.invalid());
    }
    out
}

/// `s.lower()` of the string whose bytes are `s`.
#[must_use]
pub fn lower(s: &[u8]) -> Vec<u8> {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let mut out = Vec::with_capacity(s.len());
    for chunk in s.utf8_chunks() {
        out.extend_from_slice(chunk.valid().to_lowercase().as_bytes());
        out.extend_from_slice(chunk.invalid());
    }
    out
}

/// `str.upper()` of a `str`.
#[must_use]
pub fn upper_str(s: &str) -> String {
    s.to_uppercase()
}

/// `str.lower()` of a `str`.
#[must_use]
pub fn lower_str(s: &str) -> String {
    s.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases() {
        assert_eq!(upper(b"ab-12"), b"AB-12");
        assert_eq!(lower(b"AB-12"), b"ab-12");
        assert_eq!(upper("stra\u{df}e".as_bytes()), "STRASSE".as_bytes());
        assert_eq!(upper(b"a\x80b"), b"A\x80B");
        assert_eq!(lower_str("ABC"), "abc");
        assert_eq!(upper_str("abc"), "ABC");
    }
}
