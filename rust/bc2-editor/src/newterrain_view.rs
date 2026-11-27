//! Round TG4: a Create Level project opens on its new terrain (spec `transforms/terrain_edit.md` TEDIT-NEW-4): the
//! terrain the export builds ([`bc2_levelwrite::newterrain::view`]: the generator's or the height map's, its visual
//! side from the base, the base's tiled layers painted) put in place of the base's in the scene, under the base's
//! file names, so E5's sculpt, material paint, visual paint, shade and ring strokes are made on the new ground and the
//! export lays them on the new terrain's resources.

use std::path::Path;
use std::sync::Arc;

use bc2_levelwrite::MapProject;
use bc2_levelwrite::newterrain::View;
use bc2_scene::scene::{Scene, TerrainExtra, Water};

use crate::error::EditorError;

/// The scene's terrain made the view's: the main height field and its mesh (at `stride`), the material map, the
/// visual terrain, mask atlas, ring and colour / occlusion map, the water; what changed in words.
///
/// # Errors
/// The view's main height field does not read.
pub fn apply(scene: &mut Scene, view: &View, stride: usize) -> Result<Vec<String>, EditorError> {
    let get = |end: &str| view.files.get(&format!("{}{end}", view.stem));
    let main = get(".heightfield-000.res")
        .ok_or_else(|| EditorError::Level("the new terrain has no main height field".into()))?;
    let hf = bc2_formats::terrain::parse(main).map_err(|e| EditorError::Level(format!("the new terrain: {e}")))?;
    let mesh = hf.mesh(stride);
    let n = hf.size;
    scene.heightfield = Some(Arc::new(hf));
    scene.terrain = Some(Arc::new(mesh));
    scene.material_map = get("-materialmap.res")
        .and_then(|d| bc2_formats::terrain_codec::MaterialMap::decode(d).ok())
        .map(Arc::new);
    scene.terrain_extra = Some(Arc::new(TerrainExtra {
        visual: get(".visual.res").cloned(),
        atlas: get(".masktileatlas.res").cloned(),
        ring: (1..=12)
            .map_while(|k| get(&format!(".heightfield-{k:03}.res")).cloned())
            .collect(),
        colour: view.colour.clone(),
    }));
    scene.water = get(".water.mesh.res")
        .and_then(|d| bc2_formats::water::parse_mesh(d).ok())
        .map(|mesh| Water { mesh, shaders: None });
    let mut notes = vec![format!(
        "Create Level: the new terrain ({n} x {n} samples) is shown and sculpted in place of {}'s",
        view.stem
    )];
    notes.extend(view.notes.iter().cloned());
    Ok(notes)
}

/// The new terrain of the project at `path` (when it makes one) applied to a loaded scene of its base, read from the
/// game copy under `root` (OPFOR's folder); `Ok(None)` for a project without a new terrain.
///
/// # Errors
/// The project or the base's terrain does not read, or the terrain does not build.
pub fn apply_project(scene: &mut Scene, root: &Path, path: &Path) -> Result<Option<Vec<String>>, EditorError> {
    let p = MapProject::load(path).map_err(|e| EditorError::Level(e.to_string()))?;
    // a version-13 project's strokes were made on the base's terrain (TEDIT-NEW-4): it keeps showing the base's
    let Some(t) = p.new_terrain().filter(|t| !t.strokes_on_base) else {
        return Ok(None);
    };
    let game = bc2_levelwrite::export::Game::open(&root.join("game")).map_err(|e| EditorError::Level(e.to_string()))?;
    let view = bc2_levelwrite::newterrain::view(&game, &p.base.level, t, path)
        .map_err(|e| EditorError::Level(format!("the project's new terrain: {e}")))?;
    apply(scene, &view, 2).map(Some)
}
