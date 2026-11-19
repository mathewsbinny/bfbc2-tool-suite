//! What the level partition says about how its objects are drawn, beyond their transforms (round R5b):
//!
//! - **Shader-set overrides.** A placed object (`Entity.ReferenceObjectData`) may name a `MeshShaderSet`: one of its
//!   mesh's shader sets other than the default (`Barrack_Mesh`'s yellow or green container plates, a fence's yellow
//!   tarp). The mesh set resource lists every set it can be drawn with by the set's own name, `<partition>/<GUID>`
//!   (`Objects/Buildings/Barrack/Barrack_Mesh/397B3802-...`), the very reference the override holds - so the override
//!   picks the set by name ([`crate::material::prepare_set`]).
//! - **Model groups.** Each `*ModelGroupEntityData` / `ForestEntityData` (`Name` `Levels/MP_005/StaticModelGroupEntity`,
//!   its members in the group file `StaticModelGroupEntity_HavokAsset.dbx`) lists `MemberDatas` in the group file's
//!   order: per member its `InstanceCount` items, the `MeshEntityType` (a `RigidMeshEntityData` or
//!   `Overgrowth.TreeMeshEntityData`: the `Mesh` drawn and the member's `MeshShaderSet`) and one `SpawnDatas` record
//!   per instance with that instance's own `MeshShaderSet` (`MP_005`: 31 concrete building blocks, 193 yellow fences).
//!   Item k of the group file is the member whose instances cover k, its instance `k - first`.
//! - **The reflection cube** of `WorldRender.StaticEnvmapEntityData`, and **the terrain** the first
//!   `TerrainEntityData` names (a level's folders can hold other levels' terrains: `ONS_MP_005GR` holds `SP_005`'s).

use std::collections::HashMap;

use bc2_formats::dbx::{self, Node};

use crate::files::Files;

/// How one group item is drawn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Member {
    /// The mesh its `MeshEntityType` draws (a resource name, `path` of the `path/guid` reference).
    pub mesh: Option<String>,
    /// The shader set it is drawn with (`path/guid`, lower case): the instance's own, else the member type's.
    pub shader_set: Option<String>,
}

/// The level partition's drawing facts.
#[derive(Debug, Clone, Default)]
pub struct Placements {
    /// Placed object instance GUID (lower case) -> its shader-set override (`path/guid`, lower case).
    pub objects: HashMap<String, String>,
    /// Group file name (lower case, `staticmodelgroupentity_havokasset.dbx`) -> its items, in the file's order.
    pub groups: HashMap<String, Vec<Member>>,
    /// The `StaticEnvmapEntityData`'s texture.
    pub envmap: Option<String>,
    /// The terrain: the first `TerrainEntityData`'s `TerrainAsset` (`terrains/mp_005/mp_005` -> `mp_005`), as
    /// `level.terrain_name` reads it.
    pub terrain: Option<String>,
    /// The weather folder the partition references (`weather/r_sp04` of `SP_004_B`'s `WeatherSettings` field).
    pub weather: Option<String>,
}

impl Placements {
    /// A group item's facts.
    #[must_use]
    pub fn member(&self, group_file: &str, item: usize) -> Option<&Member> {
        self.groups.get(&group_file.to_lowercase())?.get(item)
    }

    /// Shader-set overrides counted (placed objects, group items).
    #[must_use]
    pub fn override_counts(&self) -> (usize, usize) {
        (
            self.objects.len(),
            self.groups
                .values()
                .flatten()
                .filter(|m| m.shader_set.is_some())
                .count(),
        )
    }
}

/// A reference field's target: `path/guid` (lower case), `None` for `null` or no field.
fn reference(node: Option<Node<'_, '_>>) -> Option<String> {
    let r = node?.attr("ref")?;
    (r != "null" && !r.is_empty()).then(|| r.to_lowercase())
}

/// `path` of `path/guid`.
fn path_of(r: &str) -> Option<String> {
    r.rfind('/').and_then(|i| r.get(..i)).map(str::to_owned)
}

/// Reads the level partition (`levels/<name>.dbx`).
///
/// # Errors
/// The partition is not found or does not read.
pub fn read(files: &Files, level: &str) -> Result<Placements, String> {
    let name = format!("levels/{level}.dbx");
    let data = files.read(&name).ok_or_else(|| format!("{name}: not found"))?;
    let tree = dbx::parse_tree(&data).map_err(|e| format!("{name}: {e}"))?;
    let by_guid: HashMap<String, Node<'_, '_>> = tree
        .instances()
        .filter_map(|i| Some((i.attr("guid")?.to_lowercase(), i)))
        .collect();
    let mut out = Placements::default();
    for inst in tree.instances() {
        if out.weather.is_none() {
            out.weather = inst.children().find_map(|c| {
                let r = c.attr("ref")?.to_lowercase();
                let mut parts = r.split('/');
                match (parts.next(), parts.next(), parts.next()) {
                    (Some("weather"), Some(f), Some(n)) if n.starts_with("weather") => Some(format!("weather/{f}")),
                    _ => None,
                }
            });
        }
        let ty = inst.attr("type").unwrap_or_default();
        let guid = inst.attr("guid").unwrap_or_default().to_lowercase();
        if ty == "Entity.ReferenceObjectData" {
            if let Some(set) = reference(inst.child("MeshShaderSet")) {
                out.objects.insert(guid, set);
            }
        } else if out.terrain.is_none() && ty.ends_with(".TerrainEntityData") {
            out.terrain = reference(inst.child("TerrainAsset"))
                .and_then(|r| path_of(&r))
                .and_then(|p| p.rsplit('/').next().map(str::to_owned));
        } else if ty == "WorldRender.StaticEnvmapEntityData" {
            out.envmap = reference(inst.child("StaticEnvmap")).and_then(|r| path_of(&r));
        } else if ty.ends_with("ModelGroupEntityData") || ty.ends_with(".ForestEntityData") {
            let group = inst.child("Name").map(|n| n.text()).unwrap_or_default();
            let file = format!("{}_havokasset.dbx", group.rsplit('/').next().unwrap_or(group)).to_lowercase();
            let mut items = Vec::new();
            for item in inst.child("MemberDatas").map(|m| m.children()).into_iter().flatten() {
                let Some(member) = item
                    .attr("ref")
                    .and_then(|r| by_guid.get(&r.to_lowercase()))
                else {
                    continue;
                };
                let count = member
                    .child("InstanceCount")
                    .and_then(|c| c.value().ok())
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let mesh_type = member
                    .child("MeshEntityType")
                    .and_then(|m| m.attr("ref"))
                    .and_then(|r| by_guid.get(&r.to_lowercase()));
                let mesh = mesh_type
                    .and_then(|m| reference(m.child("Mesh")))
                    .and_then(|r| path_of(&r));
                let type_set = mesh_type.and_then(|m| reference(m.child("MeshShaderSet")));
                let spawns: Vec<Option<String>> = member
                    .child("SpawnDatas")
                    .map(|s| s.children())
                    .into_iter()
                    .flatten()
                    .map(|s| reference(s.child("MeshShaderSet")))
                    .collect();
                for k in 0..count {
                    items.push(Member {
                        mesh: mesh.clone(),
                        shader_set: spawns.get(k).cloned().flatten().or_else(|| type_set.clone()),
                    });
                }
            }
            out.groups.insert(file, items);
        }
    }
    Ok(out)
}
