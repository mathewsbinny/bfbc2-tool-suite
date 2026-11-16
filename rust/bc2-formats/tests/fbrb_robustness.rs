//! Property tests of the bundle reader: any bytes, and any damaged copy of a bundle, give entries or an error, never
//! a panic; a bundle built from any entries reads back the same entries and bytes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use std::io::Cursor;

use bc2_formats::fbrb::{self, DELETED};
use proptest::prelude::*;

/// Entries for [`fbrb::build`]: ASCII names (some differing only in case), a few types, any data.
fn items() -> impl Strategy<Value = Vec<(String, String, Vec<u8>)>> {
    prop::collection::vec(
        (
            "[A-Za-z0-9/._]{1,16}",
            prop_oneof![
                Just("<non-resource>".to_owned()),
                Just(DELETED.to_owned()),
                Just("Mesh".to_owned())
            ],
            prop::collection::vec(any::<u8>(), 0..300),
        ),
        0..8,
    )
}

fn build(items: &[(String, String, Vec<u8>)], zipped: bool) -> Vec<u8> {
    let refs: Vec<(&str, &str, &[u8])> = items
        .iter()
        .map(|(n, t, d)| (n.as_str(), t.as_str(), d.as_slice()))
        .collect();
    fbrb::build(&refs, zipped)
}

/// Every reading path on `data`; the results are dropped (only a panic fails).
fn read_everything(data: &[u8]) {
    if let Ok(h) = fbrb::read_header(&mut &data[..]) {
        let names: Vec<String> = h.names().into_keys().collect();
        let _ = fbrb::read(&mut Cursor::new(data), &names);
    }
    let _ = fbrb::extract(&mut Cursor::new(data));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn any_bytes_never_panic(tail in prop::collection::vec(any::<u8>(), 0..200), cut in 0u32..300) {
        let mut data = b"FbRB".to_vec();
        data.extend(cut.to_be_bytes());
        data.extend(&tail);
        read_everything(&data);
        read_everything(&tail);
    }

    #[test]
    fn built_bundles_read_back(items in items(), zipped in any::<bool>()) {
        let b = build(&items, zipped);
        let h = fbrb::read_header(&mut b.as_slice()).unwrap();
        prop_assert_eq!(h.entries.len(), items.len());
        prop_assert_eq!(h.zipped, zipped);
        let (_, data) = fbrb::extract(&mut Cursor::new(&b)).unwrap();
        for ((name, typ, d), (e, got)) in items.iter().zip(h.entries.iter().zip(&data)) {
            prop_assert_eq!(&e.name, name);
            prop_assert_eq!(&e.type_name, typ);
            prop_assert_eq!(e.is_deleted(), typ == DELETED && d.is_empty());
            if !e.is_deleted() {
                prop_assert_eq!(got, d);
            }
        }
        // read by name: the later of two entries with the same name (any case) wins
        let live: Vec<&(String, String, Vec<u8>)> =
            items.iter().filter(|(_, t, d)| !(t == DELETED && d.is_empty())).collect();
        for (name, _, _) in &live {
            let want = &live.iter().rev().find(|(n, _, _)| n.eq_ignore_ascii_case(name)).unwrap().2;
            let got = fbrb::read(&mut Cursor::new(&b), &[name.to_ascii_uppercase()]).unwrap();
            prop_assert_eq!(&got[0], want);
        }
    }

    #[test]
    fn damaged_copies_never_panic(
        items in items(),
        zipped in any::<bool>(),
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0u8..3), 1..4),
    ) {
        let mut b = build(&items, zipped);
        for (at, byte, op) in edits {
            if b.is_empty() {
                break;
            }
            let i = at.index(b.len());
            match op {
                0 => b[i] = byte,
                1 => b.truncate(i),
                _ => {
                    b.drain(i..(i + usize::from(byte % 8) + 1).min(b.len()));
                }
            }
        }
        read_everything(&b);
    }
}

proptest! {
    // each case reads every prefix of the bundle: fewer cases
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn every_truncation_is_an_error_or_a_prefix(items in items(), zipped in any::<bool>()) {
        let b = build(&items, zipped);
        let (_, full) = fbrb::extract(&mut Cursor::new(&b)).unwrap();
        for cut in 0..b.len() {
            if let Ok((_, data)) = fbrb::extract(&mut Cursor::new(&b[..cut])) {
                // only bytes past the last entry's data (a zipped payload's checksum, a stored payload's nothing)
                // may be missing
                prop_assert_eq!(&data, &full);
            }
        }
    }
}
