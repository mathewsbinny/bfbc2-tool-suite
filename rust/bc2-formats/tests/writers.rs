//! Property tests of the writers: the DBX [`dbx::Writer`] (its string and id calls, the string table's order),
//! zlib's deflate (any data comes back, at every level) and bundle packing (any entries come back with their
//! names, flags, size2 and padding; a folder packs as its entries do).
//!
//! Byte parity with the references (Python's writer, Python's zlib, FbrbTool) is proven from Python, where they run:
//! `core/tests/test_dbx_write_native.py`, `core/tests/test_fbrb_pack_native.py`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    missing_docs
)]

use std::fmt::Write as _;
use std::io::{Cursor, Read as _};

use bc2_formats::dbx::{self, CONTAINER, NUMBERS, TEXT};
use bc2_formats::fbrb::{self, DELETED, Layout, PackItem, Padding};
use bc2_formats::zlib;
use proptest::prelude::*;

/// A tree as plain values: (tag, kind, attributes, text, raw, children).
#[derive(Debug, Clone)]
struct Node {
    tag: String,
    kind: u8,
    attrs: Vec<(String, String)>,
    text: String,
    raw: Vec<u8>,
    children: Vec<Node>,
}

fn word() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("name".to_owned()),
        Just("field".to_owned()),
        Just("Levels/MP_005".to_owned()),
        "[a-zé日/_]{1,8}",
    ]
}

fn node() -> impl Strategy<Value = Node> {
    let leaf = (
        "[a-z]{1,6}",
        prop_oneof![Just(TEXT), Just(NUMBERS), 0u8..16],
        prop::collection::vec((word(), word()), 0..4),
        word(),
        prop::collection::vec(any::<u8>(), 0..6),
    )
        .prop_map(|(tag, kind, attrs, text, bytes)| {
            let kind = if kind == CONTAINER { TEXT } else { kind };
            // a value as the format stores it: count and size varints and the numbers, or two bytes
            let raw = if kind == NUMBERS {
                let n = bytes.len() / 2;
                [vec![n as u8, 2], bytes[..2 * n].to_vec()].concat()
            } else {
                [bytes, vec![0, 0]].concat()[..2].to_vec()
            };
            Node {
                kind,
                tag,
                attrs,
                text,
                raw,
                children: Vec::new(),
            }
        });
    leaf.prop_recursive(4, 40, 5, |inner| {
        (
            "[a-z]{1,6}",
            prop::collection::vec((word(), word()), 0..3),
            prop::collection::vec(inner, 0..5),
        )
            .prop_map(|(tag, attrs, children)| Node {
                tag,
                kind: CONTAINER,
                attrs,
                text: String::new(),
                raw: Vec::new(),
                children,
            })
    })
}

/// The tree through the string calls.
fn by_strings(w: &mut dbx::Writer, n: &Node) {
    w.element(&n.tag, n.kind, n.attrs.len()).unwrap();
    for (k, v) in &n.attrs {
        w.attribute(k, v);
    }
    match n.kind {
        CONTAINER => {
            w.open();
            n.children.iter().for_each(|c| by_strings(w, c));
            w.close();
        }
        TEXT => w.text(&n.text),
        _ => w.raw(&n.raw),
    }
}

/// The tree through the id calls, each string's id taken where its string call would have taken it.
fn by_ids(w: &mut dbx::Writer, n: &Node) {
    let t = w.id(&n.tag);
    w.element_id(t, n.kind, n.attrs.len()).unwrap();
    for (k, v) in &n.attrs {
        let (k, v) = (w.id(k), w.id(v));
        w.attribute_ids(k, v);
    }
    match n.kind {
        CONTAINER => {
            w.open();
            n.children.iter().for_each(|c| by_ids(w, c));
            w.close();
        }
        TEXT => {
            let t = w.id(&n.text);
            w.text_id(t);
        }
        _ => w.raw(&n.raw),
    }
}

/// The strings in the order the tree first uses them, the empty string first.
fn first_use(n: &Node, out: &mut Vec<String>) {
    let mut add = |s: &str| {
        if !out.iter().any(|x| x == s) {
            out.push(s.to_owned());
        }
    };
    add(&n.tag);
    for (k, v) in &n.attrs {
        add(k);
        add(v);
    }
    if n.kind == TEXT {
        add(&n.text);
    }
    for c in &n.children {
        first_use(c, out);
    }
}

fn inflate(z: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(z).read_to_end(&mut out).unwrap();
    out
}

/// Data with the structure deflate meets: runs, repeats near and far, noise.
fn data() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        prop_oneof![
            prop::collection::vec(any::<u8>(), 0..64),
            (any::<u8>(), 1usize..600).prop_map(|(b, n)| vec![b; n]),
            (prop::collection::vec(any::<u8>(), 1..20), 1usize..80).prop_map(|(p, n)| p.repeat(n)),
        ],
        0..40,
    )
    .prop_map(|parts| {
        let mut out: Vec<u8> = parts.concat();
        // a copy of the start far back, so matches cross the window's slide
        if out.len() > 100 {
            let head = out[..100].to_vec();
            out.extend(vec![0x55; 40_000]);
            out.extend(head);
        }
        out
    })
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    type_name: String,
    flags: u32,
    size2: u32,
    data: Vec<u8>,
}

fn entries() -> impl Strategy<Value = Vec<Entry>> {
    prop::collection::vec(
        (
            "[A-Za-z0-9_/]{1,20}",
            prop_oneof![
                Just("<non-resource>".to_owned()),
                Just("RigidMeshSet".to_owned()),
                Just(DELETED.to_owned())
            ],
            any::<u32>(),
            any::<u32>(),
            prop::collection::vec(any::<u8>(), 0..300),
        )
            .prop_map(|(name, type_name, flags, size2, data)| {
                let data = if type_name == DELETED { Vec::new() } else { data };
                Entry {
                    name,
                    type_name,
                    flags,
                    size2,
                    data,
                }
            }),
        0..12,
    )
}

fn layout() -> impl Strategy<Value = Layout> {
    (
        any::<bool>(),
        prop_oneof![Just(None), Just(Some(Padding::Align4096)), Just(Some(Padding::Gap64))],
        prop_oneof![Just(1), Just(4), Just(4096), Just(0)],
        any::<bool>(),
    )
        .prop_map(|(zipped, padding, header_align, plen_size2)| Layout {
            zipped,
            padding,
            header_align,
            plen_size2,
            size2_minus64: false,
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn writer_calls_agree(nodes in prop::collection::vec(node(), 1..4)) {
        let (mut a, mut b) = (dbx::Writer::new(), dbx::Writer::new());
        for n in &nodes {
            by_strings(&mut a, n);
            by_ids(&mut b, n);
        }
        let (a, b) = (a.finish().unwrap(), b.finish().unwrap());
        prop_assert_eq!(&a, &b);
        let mut want = vec![String::new()];
        for n in &nodes {
            first_use(n, &mut want);
        }
        let back = dbx::parse(&a).unwrap();
        prop_assert_eq!(&back.strings, &want);
        // and dbx::write of the tree read back gives the same file
        prop_assert_eq!(dbx::write(&back).unwrap(), a);
    }

    #[test]
    fn packed_entries_come_back(es in entries(), lay in layout()) {
        let items: Vec<PackItem<'_>> = es
            .iter()
            .map(|e| PackItem { name: &e.name, type_name: &e.type_name, flags: e.flags, size2: e.size2, data: &e.data })
            .collect();
        let bundle = fbrb::pack(&items, &lay).unwrap();
        let (h, data) = fbrb::extract(&mut Cursor::new(&bundle)).unwrap();
        prop_assert_eq!(h.zipped, lay.zipped);
        prop_assert_eq!(h.entries.len(), es.len());
        let mut end = 0u32;
        let mut live = 0;
        for ((got, d), e) in h.entries.iter().zip(&data).zip(&es) {
            prop_assert_eq!((&got.name, &got.type_name, got.flags, got.size2), (&e.name, &e.type_name, e.flags, e.size2));
            prop_assert_eq!(d, &e.data);
            let marker = got.is_deleted();
            if !marker && !lay.zipped {
                if live > 0 {
                    match lay.padding {
                        Some(Padding::Align4096) => prop_assert!(got.offset % 4096 == 0 && got.offset - end < 4096),
                        Some(Padding::Gap64) => prop_assert_eq!(got.offset, end + 64),
                        None => prop_assert_eq!(got.offset, end),
                    }
                }
                live += 1;
            } else {
                prop_assert_eq!(got.offset, end);
            }
            end = got.offset + got.size;
        }
        if lay.header_align > 1 {
            prop_assert_eq!(h.payload_start % lay.header_align as u64, 0);
        }
        let last = h.entries.iter().rev().find(|e| !e.is_deleted());
        let want = match last {
            Some(l) if lay.plen_size2 => l.offset.wrapping_add(l.size2),
            _ => if lay.zipped { end } else { (bundle.len() as u64 - h.payload_start) as u32 },
        };
        prop_assert_eq!(h.payload_len, Some(want));
    }
}

proptest! {
    // deflate runs unoptimised under `cargo test`: fewer cases, each up to 100 KB (past the window's slide)
    #![proptest_config(ProptestConfig { cases: 40, ..ProptestConfig::default() })]

    #[test]
    fn deflate_round_trips(d in data(), level in 4u8..10) {
        let z = zlib::deflate(&d, level).unwrap();
        prop_assert_eq!(inflate(&z), d.clone());
        prop_assert_eq!(zlib::deflate(&d, level).unwrap(), z);
        let g = zlib::gzip_net(&d);
        let mut out = Vec::new();
        if !d.is_empty() {
            flate2::read::GzDecoder::new(g.as_slice()).read_to_end(&mut out).unwrap();
        }
        prop_assert_eq!(out, d);
    }
}

#[test]
fn a_folder_packs_as_its_entries() {
    let dir = std::env::temp_dir().join(format!("bc2-writers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Levels")).unwrap();
    let es = [
        ("Levels/A.dbx", "<non-resource>", 0x10000u32, b"abc".to_vec()),
        ("Gone", DELETED, 0, Vec::new()),
        (
            "B.res",
            "RigidMeshSet",
            0,
            (0..5000u32).map(|i| (i % 251) as u8).collect(),
        ),
    ];
    let mut manifest = "# fbrb.manifest zipped=0 layout=gap64 hdr=4\n".to_owned();
    for (name, t, flags, d) in &es {
        if *t != DELETED {
            std::fs::write(dir.join(name), d).unwrap();
        }
        writeln!(manifest, "{name}|{t}|{flags:08X}|{}|{}", d.len(), d.len()).unwrap();
    }
    std::fs::write(dir.join("fbrb.manifest"), manifest).unwrap();
    let (bytes, stats) = fbrb::pack_dir(&dir).unwrap();
    let items: Vec<PackItem<'_>> = es
        .iter()
        .map(|(name, t, flags, d)| PackItem {
            name,
            type_name: t,
            flags: *flags,
            size2: d.len() as u32,
            data: d,
        })
        .collect();
    let lay = fbrb::parse_manifest(&std::fs::read_to_string(dir.join("fbrb.manifest")).unwrap())
        .unwrap()
        .layout;
    assert_eq!(bytes, fbrb::pack(&items, &lay).unwrap());
    assert_eq!((stats.entries, stats.payload_len), (3, 3 + 64 + 5000));
    let _ = std::fs::remove_dir_all(&dir);
}
