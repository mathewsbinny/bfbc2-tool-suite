//! The data the editor view's cull-tree mode draws ([`crate::workspace::ViewMode`]), read on demand from the prepared
//! level (read only); the navmesh mode's is [`crate::nav`]'s (round GR2):
//!
//! - **Cull tree**: the level's `ExclusiveStaticCullTree` partition (`bfbc2-culltree/README.md`, "Format"): each leaf's
//!   sphere (centre key / 16 m, radius W / 4 m), and each placed object's slot (`Enumeration`): none (0: drawn
//!   see-through and indestructible in game), shared with another object, or its own.

use std::collections::HashMap;
use std::path::PathBuf;

use bc2_formats::dbx::{self, edit::numbers_of};
use bc2_scene::scene::Scene;

/// A line segment in world space.
pub type Segment = [[f32; 3]; 2];

/// A leaf's sphere: centre and radius, metres.
pub type Sphere = ([f32; 3], f32);

/// What the cull-tree mode shows.
#[derive(Debug, Clone, Default)]
pub struct CullView {
    /// The leaves' spheres.
    pub leaves: Vec<Sphere>,
    /// Placed objects (scene indices) with no slot.
    pub unslotted: Vec<usize>,
    /// Placed objects sharing their slot with another.
    pub shared: Vec<usize>,
    /// Placed objects with a slot of their own.
    pub slotted: usize,
    /// Where the tree came from, or why there is none.
    pub source: String,
}

/// The cull tree partition a level's `LevelData` names, found in the level's folders.
fn tree_file(scene: &Scene, bases: &[PathBuf]) -> Option<PathBuf> {
    let (_, data) = scene.file(0)?;
    let tree = dbx::parse_tree(data).ok()?;
    let level = tree
        .instances()
        .find(|i| i.attr("type").is_some_and(|t| t.ends_with(".LevelData")))?;
    let r = level.child("ExclusiveStaticCullTree")?.attr("ref")?;
    let path = r.rsplit_once('/').map_or(r, |(p, _)| p);
    bases.iter().find_map(|b| {
        let mut f = b.clone();
        for part in path.split('/') {
            f.push(part);
        }
        f.set_extension("dbx");
        if f.is_file() {
            return Some(f);
        }
        // the folders keep DICE's case: look case-insensitively in the last folder
        let dir = f.parent()?;
        let want = f.file_name()?.to_string_lossy().to_lowercase();
        std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase() == want)
        })
    })
}

fn int_of(raw: &[u8]) -> Option<i64> {
    let (size, vals) = numbers_of(raw)?;
    let v = vals.first()?;
    match size {
        4 => Some(i64::from(i32::from_be_bytes(<[u8; 4]>::try_from(*v).ok()?))),
        2 => Some(i64::from(u16::from_be_bytes(<[u8; 2]>::try_from(*v).ok()?))),
        _ => None,
    }
}

/// The cull-tree view of a level: the tree's leaves and every placed object's slot.
#[must_use]
pub fn cull_view(scene: &Scene, bases: &[PathBuf]) -> CullView {
    let mut out = CullView::default();
    // the objects' slots, from the level partition
    let mut by_slot: HashMap<i64, Vec<usize>> = HashMap::new();
    let guids: HashMap<String, usize> = scene
        .entities
        .iter()
        .map(|e| (e.guid.to_ascii_uppercase(), e.index))
        .collect();
    if let Some((_, data)) = scene.file(0)
        && let Ok(tree) = dbx::parse_tree(data)
    {
        for i in tree.instances() {
            if !i.attr("type").is_some_and(|t| t.ends_with(".ReferenceObjectData")) {
                continue;
            }
            let Some(&k) = i.attr("guid").and_then(|g| guids.get(&g.to_ascii_uppercase())) else {
                continue;
            };
            let slot = i.child("Enumeration").and_then(|e| int_of(e.raw())).unwrap_or(0);
            by_slot.entry(slot).or_default().push(k);
        }
    }
    for (slot, ks) in by_slot {
        if slot == 0 {
            out.unslotted.extend(ks);
        } else if ks.len() > 1 {
            out.shared.extend(ks);
        } else {
            out.slotted += 1;
        }
    }
    out.unslotted.sort_unstable();
    out.shared.sort_unstable();
    let Some(file) = tree_file(scene, bases) else {
        out.source =
            "The level's static cull tree was not found in its folders; only the objects' slots are shown.".into();
        return out;
    };
    let Ok(data) = std::fs::read(&file) else {
        out.source = format!("{} does not read", file.display());
        return out;
    };
    let Ok(tree) = dbx::parse_tree(&data) else {
        out.source = format!("{} is not a binary partition", file.display());
        return out;
    };
    for inst in tree.instances() {
        let Some(nodes) = inst.child("Nodes") else { continue };
        for n in nodes.children() {
            let get = |f: &str| n.child(f).and_then(|c| int_of(c.raw()));
            let Some(right) = get("Right") else { continue };
            // a leaf: 0x80000000 | its index (negative as int32)
            if right >= 0 {
                continue;
            }
            let key = |f: &str| get(f).map(|v| f32::from(v as u16 as i16));
            if let (Some(x), Some(y), Some(z), Some(w)) = (
                key("SphereKeyX"),
                key("SphereKeyY"),
                key("SphereKeyZ"),
                get("SphereKeyW"),
            ) {
                out.leaves.push(([x / 16.0, y / 16.0, z / 16.0], w as f32 / 4.0));
            }
        }
    }
    out.source = format!(
        "{}: {} leaves; {} objects with a slot of their own, {} sharing one, {} with none",
        file.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        out.leaves.len(),
        out.slotted,
        out.shared.len(),
        out.unslotted.len()
    );
    out
}

/// A sphere as three circles (12 segments each), for the line pass.
#[must_use]
pub fn sphere_lines((c, r): Sphere) -> Vec<Segment> {
    let mut out = Vec::with_capacity(36);
    let steps = 12;
    let at = |a: f32, plane: usize| -> [f32; 3] {
        let (s, co) = a.sin_cos();
        match plane {
            0 => [c[0] + r * co, c[1] + r * s, c[2]],
            1 => [c[0] + r * co, c[1], c[2] + r * s],
            _ => [c[0], c[1] + r * co, c[2] + r * s],
        }
    };
    for plane in 0..3 {
        for k in 0..steps {
            let a = k as f32 / steps as f32 * std::f32::consts::TAU;
            let b = (k + 1) as f32 / steps as f32 * std::f32::consts::TAU;
            out.push([at(a, plane), at(b, plane)]);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn spheres_close() {
        let l = sphere_lines(([1.0, 2.0, 3.0], 2.0));
        assert_eq!(l.len(), 36);
        let first = l.first().unwrap()[0];
        let last = l.get(11).unwrap()[1];
        assert!((first[0] - last[0]).abs() < 1e-4 && (first[1] - last[1]).abs() < 1e-4);
        assert_eq!(int_of(&[1, 2, 0xFF, 0xFE]), Some(65534));
    }
}
