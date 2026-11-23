//! The generated terrain's water mesh (WATER-MESH-1, TGEN-REQ-6): one flat body, a quad over the whole square at the
//! water level (the game copy's 53 water meshes are one body each).

/// A `WaterMesh` resource: u32 bodies (1); u32 vertices (4), 4 x (f32 x, y, z); u32 indices (6), 6 x u32 - the
/// quad over the square of `side` metres centred on the origin at height `y`.
#[must_use]
pub fn water_mesh(side: f64, y: f64) -> Vec<u8> {
    let a = (side / 2.0) as f32;
    let y = y as f32;
    let mut out = Vec::with_capacity(4 + 4 + 48 + 4 + 24);
    out.extend(1u32.to_le_bytes());
    out.extend(4u32.to_le_bytes());
    for [x, z] in [[-a, -a], [a, -a], [a, a], [-a, a]] {
        for v in [x, y, z] {
            out.extend(v.to_le_bytes());
        }
    }
    out.extend(6u32.to_le_bytes());
    for i in [0u32, 2, 1, 0, 3, 2] {
        out.extend(i.to_le_bytes());
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn reads_in_the_suites_reader() {
        let m = bc2_formats::water::parse_mesh(&water_mesh(1024.0, 12.5)).unwrap();
        assert_eq!((m.vertices.len(), m.triangles.len()), (4, 2));
        assert!(m.vertices.iter().all(|v| v[1] == 12.5 && v[0].abs() == 512.0));
    }
}
