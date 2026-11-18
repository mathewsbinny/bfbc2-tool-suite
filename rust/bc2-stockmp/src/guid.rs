//! `dbxbin.new_guid`: a deterministic GUID, `UUIDv5` over `NAMESPACE_URL` of the parts joined by `:`, upper case.

use std::fmt::Write as _;

/// Python's `uuid.NAMESPACE_URL` (6ba7b811-9dad-11d1-80b4-00c04fd430c8).
const NAMESPACE_URL: [u8; 16] = [
    0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
];

/// `str(uuid.uuid5(uuid.NAMESPACE_URL, ':'.join([namespace, *parts]))).upper()`.
///
/// ```
/// assert_eq!(bc2_stockmp::guid::new_guid("opfor:stockmp", &["mp_005", "spawner", "t1_00"]).len(), 36);
/// ```
#[must_use]
pub fn new_guid(namespace: &str, parts: &[&str]) -> String {
    let mut name = String::from(namespace);
    for p in parts {
        name.push(':');
        name.push_str(p);
    }
    let mut h = sha1_smol::Sha1::new();
    h.update(&NAMESPACE_URL);
    h.update(name.as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(h.digest().bytes().get(..16).unwrap_or(&[0; 16]));
    b[6] = (b[6] & 0x0F) | 0x50;
    b[8] = (b[8] & 0x3F) | 0x80;
    let mut hex = String::with_capacity(32);
    for x in b {
        let _ = write!(hex, "{x:02X}");
    }
    format!(
        "{}-{}-{}-{}-{}",
        hex.get(0..8).unwrap_or_default(),
        hex.get(8..12).unwrap_or_default(),
        hex.get(12..16).unwrap_or_default(),
        hex.get(16..20).unwrap_or_default(),
        hex.get(20..32).unwrap_or_default()
    )
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    #[test]
    fn python_uuid5() {
        let g = super::new_guid("opfor:stockmp", &["test_01", "spawner", "t1_00"]);
        assert_eq!(g.len(), 36);
        assert_eq!(g.as_bytes()[14], b'5');
        // str(uuid.uuid5(uuid.NAMESPACE_URL, 'python.org')).upper() (Python 3.13)
        assert_eq!(
            super::new_guid("python.org", &[]),
            "7AF94E2B-4DD9-50F0-9C9A-8A48519BDEF0"
        );
    }
}
