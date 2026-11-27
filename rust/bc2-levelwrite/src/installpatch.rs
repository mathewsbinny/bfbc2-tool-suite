//! The shared bundles an install of exported levels builds from their stages (round OR6): the texts of the overlay
//! and the pictures of the on-demand awards bundle, each from the stages of every exported level the install keeps
//! and without the levels it takes out. `mapexport overlay | awards` and the installer (`bc2_install::mapedit`) call
//! these, so a stage's texts and pictures are read one way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::export::{self, Result};

/// The overlay bundle with the texts of `stages` (each stage's `strings` in every language, its
/// `strings_by_language` in their own) and without the keys of `remove`'s levels (their names, descriptions and
/// flags' spawn names): [`export::patch_overlay_langs`]. Returns the languages written and the texts added.
///
/// # Errors
/// A stage without its record, or as [`export::patch_overlay_langs`].
pub fn overlay(
    source: &Path,
    stages: &[PathBuf],
    remove: &[String],
    work: &Path,
    out: &Path,
) -> Result<(Vec<String>, usize)> {
    let mut strings = Vec::new();
    let mut by_language: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for stage in stages {
        let rec = export::read_record(stage).map_err(|e| export::ExportError(format!("{}: {e}", stage.display())))?;
        strings.extend(rec.strings.iter().cloned());
        for (lang, l) in &rec.strings_by_language {
            by_language.entry(lang.clone()).or_default().extend(l.iter().cloned());
        }
    }
    let mut keys = Vec::new();
    for lv in remove {
        let (n, d) = export::loc_keys(lv);
        keys.extend([n, d]);
        keys.extend(export::flag_loc_keys(lv));
    }
    let langs = export::patch_overlay_langs(source, &strings, &by_language, &keys, work, out)?;
    Ok((langs, strings.len()))
}

/// The on-demand awards bundle with the pictures of `stages` (each stage's `awards`, read from the stage) and
/// without `remove`'s levels' pictures: [`export::patch_awards`]. Returns the pictures added and the entries in the
/// bundle.
///
/// # Errors
/// A stage without its record or a picture, or as [`export::patch_awards`].
pub fn awards(source: &Path, stages: &[PathBuf], remove: &[String], work: &Path, out: &Path) -> Result<(usize, usize)> {
    let mut pictures = Vec::new();
    for stage in stages {
        let rec = export::read_record(stage).map_err(|e| export::ExportError(format!("{}: {e}", stage.display())))?;
        for f in &rec.awards {
            let bytes =
                std::fs::read(stage.join(&f.path)).map_err(|e| export::ExportError(format!("{}: {e}", f.path)))?;
            pictures.push((f.install.clone(), bytes));
        }
    }
    let gone: Vec<String> = remove.iter().flat_map(|lv| export::picture_entries(lv)).collect();
    let n = export::patch_awards(source, &pictures, &gone, work, out)?;
    Ok((pictures.len(), n))
}
