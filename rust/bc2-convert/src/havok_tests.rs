#![expect(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]

use super::*;

const UINT16: u8 = 6;
const UINT32: u8 = 8;
const REAL: u8 = 11;
const VECTOR4: u8 = 12;

fn member(name: &str, cls: Option<&str>, ty: u8, subtype: u8, offset: u32) -> Member {
    Member {
        name: Some(name.to_owned()),
        cls: cls.map(str::to_owned),
        ty,
        subtype,
        carray: 0,
        offset,
    }
}

fn klass(name: &str, parent: Option<&str>, size: u32, members: Vec<Member>) -> Klass {
    Klass {
        name: name.to_owned(),
        parent: parent.map(str::to_owned),
        size,
        members,
    }
}

fn pad16(v: &mut Vec<u8>, with: u8) {
    while !v.len().is_multiple_of(16) {
        v.push(with);
    }
}

/// A big-endian packfile: class names, an empty types section, the data with its objects (offset, class) and
/// local fixups (from, to).
fn packfile(classes: &[&str], content: &[u8], local: &[(u32, u32)], objects: &[(u32, &str)]) -> Vec<u8> {
    let mut cn = Vec::new();
    let mut at = HashMap::new();
    for c in classes {
        cn.extend(0x1234_5678u32.to_be_bytes());
        cn.push(9);
        at.insert(*c, cn.len() as u32);
        cn.extend(c.as_bytes());
        cn.push(0);
    }
    pad16(&mut cn, 0xFF);
    let mut data = content.to_vec();
    pad16(&mut data, 0);
    let loc = data.len() as u32;
    for (s, t) in local {
        data.extend(s.to_be_bytes());
        data.extend(t.to_be_bytes());
    }
    pad16(&mut data, 0xFF);
    let glob = data.len() as u32;
    let virt = glob;
    for (o, c) in objects {
        data.extend(o.to_be_bytes());
        data.extend(0u32.to_be_bytes());
        data.extend(at[c].to_be_bytes());
    }
    pad16(&mut data, 0xFF);
    let end = data.len() as u32;
    let mut b = Vec::new();
    b.extend(MAGIC);
    b.extend(0u32.to_be_bytes());
    b.extend(5u32.to_be_bytes());
    b.extend([4, 0, 0, 1]);
    for v in [3u32, 2, 0, 0, 0] {
        b.extend(v.to_be_bytes());
    }
    b.extend(b"Havok-5.5.0-r1\0\xff");
    b.extend([0u8; 8]);
    let cn_start = 64 + 48 * 3;
    let data_start = cn_start + cn.len() as u32;
    let n = cn.len() as u32;
    for (tag, rel) in [
        (&b"__classnames__"[..], [cn_start, n, n, n, n, n, n]),
        (&b"__types__"[..], [data_start, 0, 0, 0, 0, 0, 0]),
        (&b"__data__"[..], [data_start, loc, glob, virt, end, end, end]),
    ] {
        let mut t = tag.to_vec();
        t.resize(19, 0);
        t.push(0xFF);
        b.extend(t);
        for v in rel {
            b.extend(v.to_be_bytes());
        }
    }
    b.extend(cn);
    b.extend(data);
    b
}

fn classes() -> Classes {
    Classes::new(&[
        klass("Base", None, 8, vec![member("x", None, UINT32, 0, 0)]),
        klass(
            "A",
            Some("Base"),
            64,
            vec![
                member("v", None, VECTOR4, 0, 16),
                member("p", Some("B"), POINTER, STRUCT, 32),
                member("q", Some("B"), POINTER, STRUCT, 36),
                member("arr", None, ARRAY, UINT16, 40),
            ],
        ),
        klass("B", None, 16, vec![member("r", None, REAL, 0, 0)]),
        klass(
            "C",
            None,
            8,
            vec![member("w", None, UINT32, 0, 0), member("h", None, UINT16, 0, 2)],
        ),
    ])
}

fn data_start(b: &[u8]) -> usize {
    u32::from_be_bytes(b[64 + 96 + 20..64 + 96 + 24].try_into().unwrap()) as usize
}

#[test]
fn objects_members_pointers_and_arrays() {
    let mut content: Vec<u8> = (0..112u8).collect();
    content[44..48].copy_from_slice(&3u32.to_be_bytes()); // arr's count
    // p and q point at the same B (64); arr's three u16 at 96
    let b = packfile(&["A", "B"], &content, &[(32, 64), (36, 64), (40, 96)], &[(0, "A")]);
    let out = convert_packfile(&b, 0, &classes()).unwrap();
    assert_eq!(out.len(), b.len());
    assert_eq!(out[17], 1);
    let d = data_start(&b);
    let rev = |a: usize, w: usize| -> Vec<u8> { b[d + a..d + a + w].iter().rev().copied().collect() };
    assert_eq!(out[d..d + 4], rev(0, 4)[..]); // Base.x, the parent's member
    for k in 0..4 {
        assert_eq!(out[d + 16 + 4 * k..d + 20 + 4 * k], rev(16 + 4 * k, 4)[..]); // A.v by 4-byte value
    }
    assert_eq!(out[d + 64..d + 68], rev(64, 4)[..]); // B.r, reached twice, swapped once
    for k in 0..3 {
        assert_eq!(out[d + 96 + 2 * k..d + 98 + 2 * k], rev(96 + 2 * k, 2)[..]); // the array's u16
    }
    assert_eq!(out[d + 4..d + 16], b[d + 4..d + 16]); // no member: kept
    assert_eq!(out[d + 98 + 4..d + 112], b[d + 98 + 4..d + 112]);
}

#[test]
fn a_byte_converted_as_two_widths_is_refused() {
    let b = packfile(&["C"], &[0u8; 16], &[], &[(0, "C")]);
    let e = convert_packfile(&b, 0, &classes()).unwrap_err();
    assert_eq!(
        e,
        HavokError::Refused("byte 0xe2 converted as a 4-byte and a 2-byte value".to_owned())
    );
}

#[test]
fn refusals_and_the_reference() {
    let k = classes();
    let b = packfile(&["Nope"], &[0u8; 16], &[], &[(0, "Nope")]);
    assert_eq!(
        convert_packfile(&b, 0, &k),
        Err(HavokError::Refused(
            "class Nope is not among the game's Havok classes".to_owned()
        ))
    );
    let mut b = packfile(&["A", "B"], &[0u8; 112], &[], &[(0, "A")]);
    b[18] = 1;
    assert_eq!(
        convert_packfile(&b, 0, &k),
        Err(HavokError::Refused(
            "layout rules 04000101, not the Xbox 360's 04 00 00 01".to_owned()
        ))
    );
    // cut short: the reference's own walk decides
    let b = packfile(&["A", "B"], &[0u8; 112], &[], &[(0, "A")]);
    assert_eq!(convert_packfile(&b[..b.len() - 40], 0, &k), Err(HavokError::Reference));
    // an array without its fixup
    let mut content = vec![0u8; 112];
    content[44..48].copy_from_slice(&2u32.to_be_bytes());
    let b = packfile(&["A", "B"], &content, &[], &[(0, "A")]);
    assert_eq!(
        convert_packfile(&b, 0, &k),
        Err(HavokError::Refused("A.arr: an array of 2 without a fixup".to_owned()))
    );
}
