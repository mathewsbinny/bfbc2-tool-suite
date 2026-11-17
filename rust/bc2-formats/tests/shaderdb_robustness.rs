//! Property tests of the shader database reader / writer and the cull tree element reader: any bytes, and any
//! damaged copy of a valid file, give a result or an error, never a panic; whatever reads writes back the same bytes
//! (a database) or accounts for every byte of the stream (elements); a database built from any records reads back.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, missing_docs)]

use std::borrow::Cow;

use bc2_formats::culltree;
use bc2_formats::shaderdb::{self, Named, Shader, ShaderDb};
use proptest::prelude::*;

/// A database with `n` programs (one shader pair, parameter block and group each); `be`: Xbox 360 byte order.
fn database(n: usize, be: bool, dx: u32, code: &[u8]) -> Vec<u8> {
    let u32b = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let param: Vec<u8> = [u32b(8).as_slice(), b"PARM"].concat();
    let binding: Vec<u8> = if be {
        vec![0, 1, 7, 7, 7, 7, 0, 0]
    } else {
        vec![1, 0, 7, 7, 7, 7, 0, 0]
    };
    let regs: Vec<u8> = [u32b(1).as_slice(), b"REGS"].concat();
    let tail: Vec<u8> = if be {
        [u32b(1).as_slice(), &[0; 12], &[0; 16]].concat()
    } else if dx == 9 {
        [u32b(1).as_slice(), &[1; 8]].concat()
    } else {
        [u32b(2).as_slice(), b"ab", &u32b(1), &[3; 0x1c], &u32b(1), b"POSITION\0"].concat()
    };
    let guids: Vec<[u8; 16]> = (0..n).map(|i| [i as u8; 16]).collect();
    let names: Vec<Vec<u8>> = (0..n).map(|i| format!("Objects/M{i}/Shaders/S").into_bytes()).collect();
    let prog: Vec<Vec<u8>> = (0..n)
        .map(|i| {
            [
                &guids[i][..],
                &[0; 16],
                &u32b(i as u32),
                &u32b(i as u32),
                &u32b(0),
                &u32b(0),
            ]
            .concat()
        })
        .collect();
    let keys: Vec<Vec<u8>> = (0..n)
        .map(|i| [&guids[i][..], &[0; 16], &[9; 16], &[0; 40]].concat())
        .collect();
    let layout = [9u8; shaderdb::LAYOUT_SIZE];
    let db = ShaderDb {
        big_endian: be,
        version: 120,
        flag: 1,
        params: vec![&param],
        bindings: vec![&binding],
        regs: vec![&regs],
        vs: (0..n)
            .map(|i| Shader {
                guid: &guids[i],
                code,
                tail: &tail,
                ..Shader::default()
            })
            .collect(),
        ps: (0..n)
            .map(|i| Shader {
                guid: &guids[i],
                code,
                ..Shader::default()
            })
            .collect(),
        programs: prog.iter().map(Vec::as_slice).collect(),
        keys: keys.iter().map(Vec::as_slice).collect(),
        layouts: vec![&layout],
        entries: (0..n)
            .map(|i| Named {
                name: Cow::Borrowed(&names[i]),
                guid: &guids[i],
            })
            .collect(),
        fragments: vec![],
        parts: vec![Named {
            name: Cow::Borrowed(b"M_lod0/P/Standard"),
            guid: &layout[..16],
        }],
        groups: (0..n).map(|i| (&guids[i][..], vec![i as u16])).collect(),
    };
    shaderdb::write(&db)
}

/// Every reading of `data`: as Dx9, Dx10 and Xbox 360; whatever reads must write back the same bytes. Then the
/// cull tree element reader from a few starts.
fn read_everything(data: &[u8]) {
    for (dx, be) in [(9, false), (10, false), (9, true)] {
        if let Ok(db) = shaderdb::parse(data, dx, be) {
            assert_eq!(shaderdb::write(&db), data);
            assert_eq!(shaderdb::written_size(&db), data.len());
        }
    }
    for start in [0, 1, 24] {
        if let Ok(r) = culltree::elements(data, start) {
            assert!(r.end <= data.len());
            for e in &r.elements {
                assert!(e.value.end <= data.len() && e.attrs.end <= r.attrs.len());
                if let Some(end) = e.end {
                    assert!(end <= r.elements.len());
                }
            }
        }
    }
}

fn damage(data: &[u8], cut: usize, flips: &[(usize, u8)]) -> Vec<u8> {
    let mut d = data[..cut.min(data.len())].to_vec();
    for &(at, b) in flips {
        if !d.is_empty() {
            let k = at % d.len();
            d[k] = b;
        }
    }
    d
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn any_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        read_everything(&data);
    }

    #[test]
    fn small_counts_never_panic(words in prop::collection::vec(0u32..4, 0..80)) {
        // mostly-zero files with small counts reach every section
        let mut d = vec![120, 0, 0, 0, 0];
        d.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        read_everything(&d);
    }

    #[test]
    fn databases_read_back(n in 0usize..6, be in any::<bool>(), dx in prop::sample::select(vec![9u32, 10]),
                           code in prop::collection::vec(any::<u8>(), 0..40)) {
        let data = database(n, be, dx, &code);
        let db = shaderdb::parse(&data, dx, be).unwrap();
        prop_assert_eq!(db.programs.len(), n);
        prop_assert_eq!(db.groups.len(), n);
        prop_assert_eq!(shaderdb::write(&db), data.clone());
        prop_assert_eq!(shaderdb::is_big_endian(&data), be);
    }

    #[test]
    fn damaged_files_never_panic(n in 1usize..4, be in any::<bool>(), cut in 0usize..2000,
                                 flips in prop::collection::vec((any::<usize>(), any::<u8>()), 0..6)) {
        let data = database(n, be, 10, b"\x01code");
        let bad = damage(&data, cut, &flips);
        read_everything(&bad);
        if cut < data.len() {
            prop_assert!(shaderdb::parse(&data[..cut], 10, be).is_err());
        }
    }
}
