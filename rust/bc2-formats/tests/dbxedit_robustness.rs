//! Property tests of DbxEdit's commands in process (`dbxedit`): any bytes and any text give a result or a fault,
//! never a panic (a panic would end the Python process that runs them); a file read and written back is the same
//! bytes; the XML of a file imported against it gives the file back.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_formats::dbxedit::{self, EditFile, editor};
use proptest::prelude::*;

/// A small partition built with the `dbx` writer: instances with text, numbers, bools, references and an array.
fn built(names: &[String], numbers: &[u32]) -> Vec<u8> {
    let mut f = DbxFile::default();
    let mut instances = Vec::new();
    for (i, n) in names.iter().enumerate() {
        let guid = format!("{i:08X}-0000-0000-0000-000000000000");
        let mut kids = vec![
            Element::text(&mut f, "field", &[("name", "Name")], n),
            Element::scalar(&mut f, "field", 6, &[("name", "Enabled")], [1, (i % 2) as u8]),
        ];
        let raw: Vec<u8> = numbers.iter().flat_map(|w| w.to_be_bytes()).collect();
        kids.push(Element::numbers(&mut f, "field", &[("name", "Values")], 4, &raw));
        let item = Element::text(&mut f, "item", &[("ref", &guid)], "");
        kids.push(Element::container(&mut f, "array", &[("name", "Objects")], vec![item]));
        instances.push(Element::container(
            &mut f,
            "instance",
            &[("guid", &guid), ("type", "a.B")],
            kids,
        ));
    }
    let p = Element::container(&mut f, "partition", &[("guid", "P")], instances);
    f.roots.push(p);
    dbx::write(&f).unwrap()
}

fn every_command(data: &[u8]) {
    if let Ok(mut f) = EditFile::parse(data.to_vec()) {
        let _ = f.serialize();
        let _ = dbxedit::level_check(&f, Some(&f.clone()), &[]);
        let _ = editor::drop(&mut f, "00000000-0000-0000-0000-000000000000");
        let _ = editor::set_field(&mut f, "00000001-0000-0000-0000-000000000000", "Values", "1.5/2");
        let _ = editor::add_item(&mut f, "00000000-0000-0000-0000-000000000000", "Objects", "x", None);
        let _ = editor::replace_prefix(&mut f, "a", "B", &[]);
        let _ = editor::drop_field(&mut f, "Enabled");
        let _ = f.serialize();
    }
    if let Ok(xml) = dbxedit::convert(data) {
        let reference = EditFile::parse(data.to_vec()).ok();
        let _ = dbxedit::import(
            || Ok(xml.clone()),
            "x.xml",
            reference.as_ref().filter(|r| r.is_binary),
            &[],
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn any_bytes_never_panic(tail in prop::collection::vec(any::<u8>(), 0..400)) {
        let mut data = b"{binary}".to_vec();
        data.extend(tail);
        every_command(&data);
    }

    #[test]
    fn damaged_files_never_panic(
        names in prop::collection::vec("[a-zA-Z0-9 &<>/é]{0,10}", 1..4),
        numbers in prop::collection::vec(any::<u32>(), 0..5),
        cuts in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
    ) {
        let mut data = built(&names, &numbers);
        for (at, b) in cuts {
            let i = at.index(data.len());
            if b % 3 == 0 { data.truncate(i.max(25)); } else { data[i] = b; }
        }
        every_command(&data);
    }

    #[test]
    fn built_files_round_trip_and_import_back(
        names in prop::collection::vec("[a-zA-Z0-9 &<>/é]{0,10}", 1..4),
        numbers in prop::collection::vec(any::<u32>(), 0..5),
    ) {
        let data = built(&names, &numbers);
        let f = EditFile::parse(data.clone()).unwrap();
        prop_assert_eq!(f.serialize().unwrap(), data.clone());
        let xml = dbxedit::convert(&data).unwrap();
        prop_assert_eq!(&xml, &dbx::to_xml(&data).unwrap());
        let (back, report) = dbxedit::import(|| Ok(xml.clone()), "x.xml", Some(&f), &[]).unwrap();
        prop_assert_eq!(back.serialize().unwrap(), data);
        prop_assert_eq!(report.changed + report.added + report.removed, 0);
    }

    #[test]
    fn any_text_imports_or_fails(text in "[<>/a-z=\"' &;#x0-9\\r\\n\\t!\\[\\]?-]{0,120}") {
        let _ = dbxedit::import(|| Ok(text.clone()), "x.xml", None, &[]);
        let _ = dbxedit::parse_xml(&text, "x.xml");
    }
}

#[test]
fn deep_nesting_is_refused() {
    let mut text = String::new();
    for _ in 0..2000 {
        text.push_str("<a>");
    }
    assert!(dbxedit::parse_xml(&text, "x").is_err());
}
