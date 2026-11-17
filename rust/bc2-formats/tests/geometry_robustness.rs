//! Property tests of the terrain and water readers: any bytes, and any damaged copy of a valid file, give a result or
//! an error, never a panic; a height field built from any heights reads them back.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp, missing_docs)]

use bc2_formats::{havok, terrain, water};
use proptest::prelude::*;

fn heightfield(n: u32, values: &[u16]) -> Vec<u8> {
    let mut d = Vec::new();
    for v in [0.0f32, -20.0, 0.0, 512.0, 300.0, 512.0] {
        d.extend(v.to_le_bytes());
    }
    d.extend(n.to_le_bytes());
    d.resize(terrain::HEADER_LEN, 0);
    for v in values {
        d.extend(v.to_le_bytes());
    }
    d
}

fn water_files() -> (Vec<u8>, Vec<u8>) {
    let le = |w: &[u32]| w.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let mut mesh = le(&[1, 3]);
    mesh.extend(
        [0.0f32, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0]
            .iter()
            .flat_map(|f| f.to_le_bytes()),
    );
    mesh.extend(le(&[3, 0, 1, 2]));
    let mut visual = le(&[10, 36, 0, 12, 0, 1, 2, 1]);
    visual.extend(b"Water/A\0\0Systems/Water/WaterOceanFragment\0");
    visual.extend(le(&[1, 0, 1, 0, 3, 0]));
    (mesh, visual)
}

/// Every reader on `data`; the results are dropped (only a panic fails).
fn read_everything(data: &[u8]) {
    if let Ok(hf) = terrain::parse(data) {
        let _ = hf.heights();
        let _ = hf.height(1.0, 2.0);
        let _ = hf.mesh(64);
    }
    let mesh = water::parse_mesh(data);
    if let (Ok(m), Ok(v)) = (mesh, water::parse_visual(data)) {
        let _ = water::triangle_shaders(&m, &v);
    }
    let _ = havok::triangles(data);
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
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn any_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        read_everything(&data);
    }

    #[test]
    fn heights_read_back(values in prop::collection::vec(any::<u16>(), 16..=16)) {
        let hf = terrain::parse(&heightfield(4, &values)).unwrap();
        prop_assert_eq!(&hf.raw, &values);
        for (h, v) in hf.heights().iter().zip(&values) {
            prop_assert_eq!(*h, (-20.0 + f64::from(*v) * (320.0 / 65535.0)) as f32);
        }
        let (verts, tris) = hf.mesh(1);
        prop_assert_eq!((verts.len(), tris.len()), (16, 18));
    }

    #[test]
    fn damaged_files_never_panic(cut in 0usize..400, flips in prop::collection::vec((any::<usize>(), any::<u8>()), 0..6)) {
        let (mesh, visual) = water_files();
        let hf = heightfield(16, &[7; 256]);
        for d in [&mesh, &visual, &hf] {
            let bad = damage(d, cut, &flips);
            read_everything(&bad);
        }
        // a truncated height field is refused
        prop_assert!(terrain::parse(&hf[..cut.min(hf.len() - 1)]).is_err());
    }
}
