//! The texture glue round OR2 left in Python (round OR4, "OPFOR in Rust" stage 4): Project OPFOR's
//! `wake_install.res_type` (the bundle type of one of our resources) and `stage_textures` (the textures the drawn
//! shader entries name, staged from our builds or copied from a stock bundle).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::closure::{Src, Take};
use super::index::{OurFile, StockIndex, mesh_set_type, primary_type};
use super::{OMap, Result, read};

static LOD_DATA: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"_lod\d+_data$").ok());
static LOD_SOOT: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"_lod\d+_soot$").ok());
static ANIMTREE: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"animtree[^/]*$").ok());
static TEX_SUFFIX: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"_(c|n|d|s|cm|nm|dm|df|sm|a|rgb|nrm)\.res$").ok());

fn hit(re: &LazyLock<Option<Regex>>, s: &str) -> bool {
    re.as_ref().is_some_and(|r| r.is_match(s))
}

/// `low[:-4]` (every name here is ASCII-safe at its end: an extension).
fn stem(low: &str) -> &str {
    let n = low.chars().count();
    let cut = low.char_indices().nth(n.saturating_sub(4)).map_or(0, |(i, _)| i);
    low.get(..cut).unwrap_or_default()
}

/// `res_type(low, ours, stock_types, src_types)`: the bundle type of one of our resources - the stock entry's of the
/// same name (the first bundle holding it), BF1943's (its Xenon textures as PC ones), else by its name, or by its
/// partition's primary instance (a mesh asset's mesh set, a texture, a physics asset, a wave). `None`: no type known.
///
/// # Errors
/// The partition of that name is damaged or cannot be read.
pub fn res_type(
    low: &str,
    ours: &OMap<OurFile>,
    index: Option<&StockIndex>,
    src_types: &HashMap<String, String>,
) -> Result<Option<String>> {
    if let Some(t) = index.and_then(|ix| ix.stock_type(low)) {
        return Ok(Some(t.to_owned()));
    }
    if let Some(t) = src_types.get(low).filter(|t| !t.is_empty()) {
        return Ok(Some(if t == "XenonTexture" {
            "ITexture".to_owned()
        } else {
            t.clone()
        }));
    }
    let stem = stem(low);
    if hit(&LOD_DATA, stem) {
        return Ok(Some("MeshData".to_owned()));
    }
    if hit(&LOD_SOOT, stem) {
        return Ok(Some("SootMesh".to_owned()));
    }
    if hit(&ANIMTREE, stem) {
        return Ok(Some("AnimTreeInfo".to_owned()));
    }
    if let Some((_, path)) = ours.get(&format!("{stem}.dbx")) {
        let (kind, _) = primary_type(&read(path)?)?;
        if let Some(kind) = kind {
            if let Some(t) = mesh_set_type(&kind) {
                return Ok(Some(t.to_owned()));
            }
            if kind.ends_with("TextureAsset") {
                return Ok(Some("ITexture".to_owned()));
            }
            if kind.ends_with("HavokAsset") {
                return Ok(Some("HavokPhysicsData".to_owned()));
            }
            if kind.contains("Wave") {
                return Ok(Some("Wave".to_owned()));
            }
        }
    }
    if low.contains("/textures") || low.starts_with("textures/") || hit(&TEX_SUFFIX, low) {
        return Ok(Some("ITexture".to_owned()));
    }
    if low.starts_with("animations/") {
        return Ok(Some("GrannyAnimation".to_owned()));
    }
    Ok(None)
}

/// What [`stage_textures`] found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Staged {
    /// The textures found nowhere (their names without `.res`), sorted.
    pub missing: Vec<String>,
    /// How many were copied from a stock bundle.
    pub stock: usize,
    /// How many were staged from our builds.
    pub mine: usize,
}

/// `stage_textures(tex, take, have, ours, index, type_of)`: the textures the drawn entries name that the level bundle
/// needs (in name order) - one of our builds' (converted but reached by nothing) staged from there with its
/// [`res_type`], else the first stock bundle's copied; what the level has without them (`have`: the take, the loaded
/// and preloaded bundles) skipped.
///
/// # Errors
/// [`res_type`]'s.
pub fn stage_textures(
    tex: &[String],
    take: &mut OMap<Take>,
    have: &HashSet<String>,
    ours: &OMap<OurFile>,
    index: &StockIndex,
    src_types: &HashMap<String, String>,
) -> Result<Staged> {
    let mut names: Vec<&String> = tex.iter().collect();
    names.sort_unstable();
    names.dedup();
    let mut out = Staged::default();
    for t in names {
        let k = format!("{t}.res");
        if have.contains(&k) {
            continue;
        }
        if let Some((name, path)) = ours.get(&k) {
            let kind = res_type(&k, ours, Some(index), src_types)?;
            take.insert(
                &k,
                Take {
                    name: name.clone(),
                    kind,
                    src: Src::Path(path.clone()),
                },
            );
            out.mine += 1;
            continue;
        }
        if let Some(b) = index.bundles.iter().find(|b| b.has(&k))
            && let Some((name, kind)) = b.get(&k)
        {
            take.insert(
                &k,
                Take {
                    name: name.to_owned(),
                    kind: Some(kind.to_owned()),
                    src: Src::Stock(b.path.clone(), name.to_owned()),
                },
            );
            out.stock += 1;
            continue;
        }
        out.missing.push(t.clone());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ours(items: &[(&str, &str)]) -> OMap<OurFile> {
        let mut o = OMap::new();
        for (k, p) in items {
            o.insert(k, ((*k).to_owned(), (*p).to_owned()));
        }
        o
    }

    #[test]
    fn types_by_name() {
        let o = ours(&[]);
        let none = HashMap::new();
        let t = |low: &str| res_type(low, &o, None, &none).ok().flatten();
        assert_eq!(t("a/b_lod1_data.res").as_deref(), Some("MeshData"));
        assert_eq!(t("a/kitbag_lod0_soot.res").as_deref(), Some("SootMesh"));
        assert_eq!(t("a/soldier_animtree_1p.res").as_deref(), Some("AnimTreeInfo"));
        assert_eq!(t("objects/x/textures/y.res").as_deref(), Some("ITexture"));
        assert_eq!(t("textures/y.res").as_deref(), Some("ITexture"));
        assert_eq!(t("a/rock_nrm.res").as_deref(), Some("ITexture"));
        assert_eq!(t("animations/a/b.res").as_deref(), Some("GrannyAnimation"));
        assert_eq!(t("a/b.res"), None);
        let src = HashMap::from([("a/b.res".to_owned(), "XenonTexture".to_owned())]);
        assert_eq!(
            res_type("a/b.res", &o, None, &src).ok().flatten().as_deref(),
            Some("ITexture")
        );
        let ix = StockIndex::from_bundles(vec![("b1".into(), vec![("A/B.res".into(), "Wave".into())])]);
        assert_eq!(
            res_type("a/b.res", &o, Some(&ix), &src).ok().flatten().as_deref(),
            Some("Wave")
        );
    }

    #[test]
    fn stages_ours_then_stock() {
        let o = ours(&[("t/ours_c.res", "C:\\w\\ours_c.res")]);
        let ix = StockIndex::from_bundles(vec![
            ("b1".into(), vec![("T/Stock_C.res".into(), "ITexture".into())]),
            ("b2".into(), vec![("T/Stock_C.res".into(), "Other".into())]),
        ]);
        let mut take = OMap::new();
        let have = HashSet::from(["t/loaded_c.res".to_owned()]);
        let tex: Vec<String> = ["t/stock_c", "t/ours_c", "t/loaded_c", "t/none_c", "t/ours_c"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let got = stage_textures(&tex, &mut take, &have, &o, &ix, &HashMap::new()).ok();
        assert_eq!(
            got,
            Some(Staged {
                missing: vec!["t/none_c".into()],
                stock: 1,
                mine: 1
            })
        );
        let keys: Vec<&str> = take.keys().collect();
        assert_eq!(keys, ["t/ours_c.res", "t/stock_c.res"]);
        let s = take.get("t/stock_c.res").map(|t| (t.kind.clone(), t.src.clone()));
        assert_eq!(
            s,
            Some((
                Some("ITexture".to_owned()),
                Src::Stock("b1".into(), "T/Stock_C.res".into())
            ))
        );
    }
}
