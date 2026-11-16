//! Property tests: any bytes give a tree or an error, never a panic; any tree survives a write and read back; the
//! XML agrees with the tree's values.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    missing_docs
)]

use bc2_formats::dbx::{self, Body, CONTAINER, DbxFile, Element, NUMBERS, TEXT};
use proptest::prelude::*;

/// A tree description independent of any string table: built into a [`DbxFile`] by [`build`].
#[derive(Debug, Clone)]
enum Node {
    Container(String, Vec<(String, String)>, Vec<Node>),
    Text(String, Vec<(String, String)>, String),
    Numbers(String, Vec<(String, String)>, u8, Vec<u8>),
    Scalar(String, u8, Vec<(String, String)>, [u8; 2]),
}

/// A tag or attribute name (the XML does not escape them; the game's are plain words).
fn ident() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("field".to_owned()),
        Just("name".to_owned()),
        Just("Id".to_owned()),
        Just("Transform".to_owned()),
        "[a-zA-Z][a-zA-Z0-9]{0,10}",
    ]
}

/// An attribute value or text: anything, escaped in the XML.
fn name() -> impl Strategy<Value = String> {
    prop_oneof![Just("Id".to_owned()), "[a-zA-Z0-9 &<>\"'/.é日]{0,12}"]
}

fn attrs() -> impl Strategy<Value = Vec<(String, String)>> {
    prop::collection::vec((ident(), name()), 0..4)
}

fn leaf() -> impl Strategy<Value = Node> {
    prop_oneof![
        (ident(), attrs(), name()).prop_map(|(t, a, s)| Node::Text(t, a, s)),
        (
            ident(),
            attrs(),
            prop_oneof![Just(2u8), Just(4), Just(8), Just(3)],
            0usize..6
        )
            .prop_flat_map(|(t, a, size, n)| {
                prop::collection::vec(any::<u8>(), n * usize::from(size))
                    .prop_map(move |v| Node::Numbers(t.clone(), a.clone(), size, v))
            }),
        (
            ident(),
            (0u8..16).prop_filter("other kinds", |k| ![CONTAINER, TEXT, NUMBERS].contains(k)),
            attrs(),
            any::<[u8; 2]>()
        )
            .prop_map(|(t, k, a, r)| Node::Scalar(t, k, a, r)),
    ]
}

fn tree() -> impl Strategy<Value = Vec<Node>> {
    let node = leaf().prop_recursive(4, 48, 6, |inner| {
        (ident(), attrs(), prop::collection::vec(inner, 0..6)).prop_map(|(t, a, c)| Node::Container(t, a, c))
    });
    prop::collection::vec(node, 1..4)
}

fn build(f: &mut DbxFile, n: &Node) -> Element {
    let pairs = |a: &[(String, String)]| a.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>();
    match n {
        Node::Container(t, a, c) => {
            let kids = c.iter().map(|k| build(f, k)).collect();
            let a = pairs(a);
            let a: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            Element::container(f, t, &a, kids)
        }
        Node::Text(t, a, s) => {
            let a = pairs(a);
            let a: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            Element::text(f, t, &a, s)
        }
        Node::Numbers(t, a, size, v) => {
            let a = pairs(a);
            let a: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            Element::numbers(f, t, &a, usize::from(*size), v)
        }
        Node::Scalar(t, k, a, r) => {
            let a = pairs(a);
            let a: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            Element::scalar(f, t, *k, &a, *r)
        }
    }
}

/// The tree with every string resolved, to compare trees of different string tables.
fn resolved(f: &DbxFile, e: &Element) -> String {
    let attrs: Vec<String> = e
        .attributes
        .iter()
        .map(|(k, v)| format!("{}={}", f.str(*k), f.str(*v)))
        .collect();
    let body = match &e.body {
        Body::Container(c) => c.iter().map(|k| resolved(f, k)).collect::<Vec<_>>().join(","),
        Body::Text(t) => format!("'{}'", f.str(*t)),
        other => format!("{:?}", other_raw(other)),
    };
    format!("{}#{}[{}]{{{}}}", f.str(e.tag), e.kind, attrs.join(" "), body)
}

fn other_raw(b: &Body) -> Vec<u8> {
    match b {
        Body::Numbers(r) => r.clone(),
        Body::Scalar(r) => r.to_vec(),
        _ => Vec::new(),
    }
}

/// Every leaf's value, in document order, as `dbx::value` gives it.
fn values(f: &DbxFile, e: &Element, out: &mut Vec<String>) {
    match &e.body {
        Body::Container(c) => c.iter().for_each(|k| values(f, k, out)),
        _ => out.push(dbx::value(f, e).unwrap()),
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn any_bytes_never_panic(body in prop::collection::vec(any::<u8>(), 0..256), strings in 0u32..6) {
        let mut data = dbx::MAGIC.to_vec();
        for w in [0u32, 0, 0, strings] {
            data.extend(w.to_be_bytes());
        }
        data.extend(&body);
        let _ = dbx::parse(&data);
        let _ = dbx::to_xml(&data);
        let _ = dbx::parse(&body);
        let _ = dbx::to_xml(&body);
    }

    #[test]
    fn write_then_read_keeps_the_tree(nodes in tree()) {
        let mut f = DbxFile::default();
        f.roots = nodes.iter().map(|n| build(&mut f, n)).collect::<Vec<_>>();
        let bytes = dbx::write(&f).unwrap();
        let back = dbx::parse(&bytes).unwrap();
        let a: Vec<String> = f.roots.iter().map(|e| resolved(&f, e)).collect();
        let b: Vec<String> = back.roots.iter().map(|e| resolved(&back, e)).collect();
        prop_assert_eq!(a, b);
        prop_assert_eq!(dbx::write(&back).unwrap(), bytes);
    }

    #[test]
    fn xml_shows_the_values(nodes in tree()) {
        let mut f = DbxFile::default();
        f.roots = nodes.iter().map(|n| build(&mut f, n)).collect::<Vec<_>>();
        let bytes = dbx::write(&f).unwrap();
        let xml = dbx::to_xml(&bytes).unwrap();
        let mut want = Vec::new();
        for r in &f.roots {
            values(&f, r, &mut want);
        }
        // each leaf line is <tag ...>value</tag> or <tag ... /> (empty text); in document order
        let mut got = Vec::new();
        for line in xml.lines().skip(1) {
            let line = line.trim_start_matches('\t');
            if line.ends_with(" />") {
                got.push(String::new());
            } else if let Some(close) = line.rfind("</") {
                // attribute values escape '>', so the first one ends the start tag
                let start_end = line.find('>').map_or(0, |i| i + 1);
                if close >= start_end {
                    got.push(unescape(&line[start_end..close]));
                }
            }
        }
        prop_assert_eq!(got, want);
    }

    #[test]
    fn damaged_copies_never_panic(nodes in tree(), cut in any::<prop::sample::Index>(), at in any::<prop::sample::Index>(), byte in any::<u8>()) {
        let mut f = DbxFile::default();
        f.roots = nodes.iter().map(|n| build(&mut f, n)).collect::<Vec<_>>();
        let bytes = dbx::write(&f).unwrap();
        let short = &bytes[..cut.index(bytes.len())];
        let _ = dbx::parse(short);
        let _ = dbx::to_xml(short);
        let mut changed = bytes.clone();
        changed[at.index(bytes.len())] = byte;
        if let Ok(t) = dbx::parse(&changed) {
            // whatever parses and writes (no empty tag) reads back as the same tree
            if let Ok(again) = dbx::write(&t) {
                let back = dbx::parse(&again).unwrap();
                let a: Vec<String> = t.roots.iter().map(|e| resolved(&t, e)).collect();
                let b: Vec<String> = back.roots.iter().map(|e| resolved(&back, e)).collect();
                prop_assert_eq!(a, b);
            }
        }
        let _ = dbx::to_xml(&changed);
    }
}

#[test]
fn one_byte_numbers_show_two_bytes_in_the_xml() {
    // as the reference: the XML reads each value's first two bytes (into the next value), dbx::value its own byte
    let mut f = DbxFile::default();
    let e = Element::numbers(&mut f, "field", &[], 1, &[1, 2, 3]);
    assert_eq!(dbx::value(&f, &e).unwrap(), "1/2/3");
    f.roots.push(e);
    let xml = dbx::to_xml(&dbx::write(&f).unwrap()).unwrap();
    assert!(xml.contains("<field>258/515/3</field>"), "{xml}");
}

#[test]
fn every_truncation_of_a_file_is_an_error_or_a_tree() {
    let mut f = DbxFile::default();
    let kids = vec![
        Element::text(&mut f, "field", &[("name", "A")], "x"),
        Element::numbers(&mut f, "field", &[("name", "B")], 4, &[0x3F, 0x80, 0, 0, 0x40, 0, 0, 0]),
        Element::scalar(&mut f, "field", 6, &[("name", "C")], [0, 1]),
    ];
    let inst = Element::container(&mut f, "instance", &[("guid", "g")], kids);
    f.roots.push(inst);
    let bytes = dbx::write(&f).unwrap();
    for n in 0..bytes.len() {
        // a cut inside the body leaves an unclosed container; one at the body's start an empty tree
        if let Ok(t) = dbx::parse(&bytes[..n]) {
            assert!(t.roots.is_empty(), "a cut at {n} read as a tree");
        }
        let _ = dbx::to_xml(&bytes[..n]);
    }
    assert!(dbx::parse(&bytes).is_ok());
}
