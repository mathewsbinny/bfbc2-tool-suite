//! A field-level comparison of two partitions: instances added and removed (by guid), and for each instance in
//! both, the direct fields whose element differs (tag, kind, attributes, text, value bytes or children, compared as
//! strings and bytes, not through XML). The proofs check that an edited partition differs from its base only where
//! the operations say; E3's apply reports it.

use std::collections::{BTreeMap, BTreeSet};

use bc2_formats::dbx::edit::{EditDbx, NodeId};

/// One difference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    /// An instance only in the second partition.
    Added(String),
    /// An instance only in the first.
    Removed(String),
    /// A field of an instance in both (`Transform`, `Objects`; `#3` for an unnamed child at that position; `*` when
    /// the instance's attributes or its number of fields changed).
    Field(String, String),
    /// The instances in both are in another order, or the partition element's attributes changed.
    Partition,
}

fn same(a: &EditDbx, x: NodeId, b: &EditDbx, y: NodeId) -> bool {
    if a.tag(x) != b.tag(y) || a.kind(x) != b.kind(y) || a.text(x) != b.text(y) || a.raw(x) != b.raw(y) {
        return false;
    }
    if !a.attributes(x).eq(b.attributes(y)) {
        return false;
    }
    match (a.children(x), b.children(y)) {
        (None, None) => true,
        (Some(p), Some(q)) => p.len() == q.len() && p.iter().zip(q).all(|(&c, &d)| same(a, c, b, d)),
        _ => false,
    }
}

fn guid_of(d: &EditDbx, e: NodeId) -> String {
    String::from_utf8_lossy(d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase()
}

/// The differences between partitions `a` and `b`, sorted.
#[must_use]
pub fn diff(a: &EditDbx, b: &EditDbx) -> Vec<Change> {
    let index =
        |d: &EditDbx| -> BTreeMap<String, NodeId> { d.instances().into_iter().map(|e| (guid_of(d, e), e)).collect() };
    let (ia, ib) = (index(a), index(b));
    let mut out = BTreeSet::new();
    for g in ia.keys().filter(|g| !ib.contains_key(*g)) {
        out.insert(Change::Removed(g.clone()));
    }
    for g in ib.keys().filter(|g| !ia.contains_key(*g)) {
        out.insert(Change::Added(g.clone()));
    }
    for (g, &x) in &ia {
        let Some(&y) = ib.get(g) else { continue };
        let (Some(cx), Some(cy)) = (a.children(x), b.children(y)) else {
            if !same(a, x, b, y) {
                out.insert(Change::Field(g.clone(), "*".into()));
            }
            continue;
        };
        if cx.len() != cy.len() || !a.attributes(x).eq(b.attributes(y)) {
            out.insert(Change::Field(g.clone(), "*".into()));
            continue;
        }
        for (k, (&p, &q)) in cx.iter().zip(cy).enumerate() {
            if !same(a, p, b, q) {
                let name = a
                    .attr(p, b"name")
                    .map_or_else(|| format!("#{k}"), |n| String::from_utf8_lossy(n).into_owned());
                out.insert(Change::Field(g.clone(), name));
            }
        }
    }
    let order = |d: &EditDbx, keep: &BTreeMap<String, NodeId>| -> Vec<String> {
        d.instances()
            .into_iter()
            .map(|e| guid_of(d, e))
            .filter(|g| keep.contains_key(g))
            .collect()
    };
    let parts_same = match (a.partition(), b.partition()) {
        (Some(p), Some(q)) => a.attributes(p).eq(b.attributes(q)),
        _ => false,
    };
    if order(a, &ib) != order(b, &ia) || !parts_same {
        out.insert(Change::Partition);
    }
    out.into_iter().collect()
}

/// The `ref`s of an array field of an instance, in order (an empty array, or none: empty).
#[must_use]
pub fn array_refs(d: &EditDbx, guid: &str, field: &str) -> Vec<String> {
    let Some(inst) = d.instance(guid.as_bytes(), <[u8]>::to_ascii_uppercase) else {
        return Vec::new();
    };
    let Some(arr) = d.child(inst, field.as_bytes()) else {
        return Vec::new();
    };
    d.children(arr)
        .unwrap_or_default()
        .iter()
        .filter_map(|&c| d.attr(c, b"ref"))
        .map(|r| String::from_utf8_lossy(r).into_owned())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::assert_is_empty)]
mod tests {
    use super::*;
    use crate::write::LevelWriter;

    #[test]
    fn finds_what_changed() {
        let base = crate::write::tests::level();
        let a = EditDbx::parse(&base).unwrap();
        assert!(diff(&a, &a).is_empty());
        let mut w = LevelWriter::parse(&base).unwrap();
        let g = "aaaaaaaa-0000-0000-0000-000000000001";
        let mut r = w.transform(g).unwrap();
        r[3][1] = 9.0;
        w.set_transform(g, &r).unwrap();
        let b = EditDbx::parse(&w.write().unwrap()).unwrap();
        assert_eq!(
            diff(&a, &b),
            vec![Change::Field(g.to_ascii_uppercase(), "Transform".into())]
        );
        w.delete("AAAAAAAA-0000-0000-0000-000000000002").unwrap();
        let b = EditDbx::parse(&w.write().unwrap()).unwrap();
        let d = diff(&a, &b);
        assert!(d.contains(&Change::Removed("AAAAAAAA-0000-0000-0000-000000000002".into())));
        assert!(d.contains(&Change::Field(
            "AAAAAAAA-0000-0000-0000-000000000000".into(),
            "Objects".into()
        )));
        assert_eq!(array_refs(&b, "AAAAAAAA-0000-0000-0000-000000000000", "Objects"), [g]);
    }
}
