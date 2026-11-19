//! The light and post-process of a level's active weather state (`renderqa.read_weather`, `renderqa.light`): the
//! weather folder's `WeatherSettings` names the active state of its `WeatherSystem`; the state names its `Sun`,
//! `OutdoorLight`, `PostProcess`, `Fog` and `Sky` partitions. Colours times their intensities, as Project OPFOR
//! measured them in the game (`wake_probe.py`).
//!
//! **Not from the data:** the key light's direction. `renderqa`'s fixed high sun ([`NOON_DIR`]) is used: the formula
//! Project OPFOR measured on Wake Island from the sun's theta / phi points below the horizon for `MP_005`'s states, so
//! how the engine turns them into `outdoorLightDir` is still open (`EDITOR_ARCHITECTURE.md` 3.11, the capture asked
//! for).

use std::collections::HashMap;

use bc2_formats::dbx;

use crate::files::Files;

/// Towards the key light (`renderqa.NOON_DIR`): about 53 degrees up.
pub const NOON_DIR: [f32; 3] = [0.35, 0.8, 0.5];

/// The fields of one partition: name -> numbers (`renderqa._xml_fields`).
pub type Fields = HashMap<String, Vec<f32>>;

/// A weather state's partitions read.
#[derive(Debug, Clone, Default)]
pub struct Weather {
    /// The state's partition name.
    pub state: String,
    /// `Sun`.
    pub sun: Fields,
    /// `OutdoorLight`.
    pub outdoor_light: Fields,
    /// `PostProcess`.
    pub post_process: Fields,
    /// `Fog` (empty when the state has none).
    pub fog: Fields,
    /// `Sky` (empty when the state has none).
    pub sky: Fields,
}

/// The post-process settings the tonemap uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Post {
    /// `MiddleGray`.
    pub middle_gray: f32,
    /// `MinExposure`.
    pub min_exposure: f32,
    /// `MaxExposure`.
    pub max_exposure: f32,
    /// `Brightness` (r, g, b).
    pub brightness: [f32; 3],
    /// `Contrast`.
    pub contrast: [f32; 3],
    /// `Saturation`.
    pub saturation: [f32; 3],
}

impl Default for Post {
    fn default() -> Self {
        Self {
            middle_gray: 1.0,
            min_exposure: 1.0,
            max_exposure: 1.0,
            brightness: [1.0; 3],
            contrast: [1.0; 3],
            saturation: [1.0; 3],
        }
    }
}

/// `<field|complex name="X">a/b/c<` -> X: [a, b, c] (fields that are not numbers are left out).
#[must_use]
pub fn fields(xml: &str) -> Fields {
    let mut out = Fields::new();
    for tag in ["<field name=\"", "<complex name=\""] {
        for (i, _) in xml.match_indices(tag) {
            let rest = xml.get(i + tag.len()..).unwrap_or_default();
            let Some(q) = rest.find('"') else { continue };
            let name = rest.get(..q).unwrap_or_default();
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let Some(body) = rest.get(q..).and_then(|r| r.strip_prefix("\">")) else {
                continue;
            };
            let text = body.get(..body.find('<').unwrap_or(body.len())).unwrap_or_default();
            let nums: Result<Vec<f32>, _> = text.split('/').map(str::parse::<f32>).collect();
            if let Ok(v) = nums {
                out.entry(name.to_owned()).or_insert(v);
            }
        }
    }
    out
}

fn xml_of(files: &Files, name: &str) -> Option<String> {
    dbx::to_xml(&files.read(&format!("{name}.dbx"))?).ok()
}

/// The references of `<item ref="path/guid"` and `<field ref="path/guid" name="X"`.
fn refs(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (i, _) in xml.match_indices("ref=\"") {
        let rest = xml.get(i + 5..).unwrap_or_default();
        let Some(end) = rest.find('"') else { continue };
        let r = rest.get(..end).unwrap_or_default();
        let Some(slash) = r.rfind('/') else { continue };
        let after = rest.get(end + 1..).unwrap_or_default();
        let name = after
            .trim_start()
            .strip_prefix("name=\"")
            .and_then(|n| n.get(..n.find('"')?))
            .unwrap_or_default();
        out.push((r.get(..slash).unwrap_or_default().to_owned(), name.to_owned()));
    }
    out
}

/// The active weather state of a weather folder (`weather/r_mp005`).
///
/// # Errors
/// No `WeatherSettings` / `WeatherSystem` partition, or the state's partitions are missing.
pub fn read(files: &Files, folder: &str) -> Result<Weather, String> {
    let folder = folder.trim_matches('/').to_lowercase();
    let mut names: Vec<&str> = files.names().collect();
    names.sort_unstable();
    let find = |kind: &str| {
        names
            .iter()
            .find(|n| {
                n.starts_with(&format!("{folder}/{kind}"))
                    && std::path::Path::new(n)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("dbx"))
            })
            .map(|n| n.trim_end_matches(".dbx").to_owned())
    };
    let (Some(settings), Some(system)) = (find("weathersettings"), find("weathersystem")) else {
        return Err(format!("no WeatherSettings / WeatherSystem partition under {folder}/"));
    };
    let sx = xml_of(files, &settings).ok_or_else(|| format!("{settings}: unreadable"))?;
    let yx = xml_of(files, &system).ok_or_else(|| format!("{system}: unreadable"))?;
    let active = fields(&sx)
        .get("ActiveState")
        .and_then(|v| v.first().copied())
        .unwrap_or(0.0) as usize;
    let states: Vec<String> = refs(&yx)
        .into_iter()
        .filter(|(_, n)| n.is_empty())
        .map(|(r, _)| r)
        .collect();
    let state = states
        .get(active)
        .ok_or_else(|| format!("{folder}: ActiveState {active} of {} states", states.len()))?
        .clone();
    let stx = xml_of(files, &state).ok_or_else(|| format!("{state}: not found"))?;
    let by_kind: HashMap<String, String> = refs(&stx).into_iter().map(|(r, n)| (n, r)).collect();
    let part = |kind: &str| -> Result<Fields, String> {
        let r = by_kind
            .get(kind)
            .ok_or_else(|| format!("{state}: no {kind} partition"))?;
        Ok(fields(&xml_of(files, r).ok_or_else(|| format!("{r}: not found"))?))
    };
    Ok(Weather {
        sun: part("Sun")?,
        outdoor_light: part("OutdoorLight")?,
        post_process: part("PostProcess")?,
        fog: part("Fog").unwrap_or_default(),
        sky: part("Sky").unwrap_or_default(),
        state,
    })
}

/// Engine constants by name, as `f32`s laid out as the shaders read them (a matrix: 16, row by row).
pub type Values = HashMap<String, Vec<f32>>;

fn first(f: &Fields, k: &str, default: f32) -> f32 {
    f.get(k).and_then(|v| v.first().copied()).unwrap_or(default)
}

fn rgb(f: &Fields, k: &str, default: f32) -> [f32; 3] {
    let v = f.get(k);
    let c = |i: usize| v.and_then(|v| v.get(i).copied()).unwrap_or(default);
    [c(0), c(1), c(2)]
}

/// The engine's light constants and the post-process of a weather state (`renderqa.light` with the fixed sun, and
/// `renderqa.FIXED`: no point light, a lit 1 x 1 shadow map, time 0).
#[must_use]
pub fn light(w: &Weather, sun_dir: [f32; 3]) -> (Values, Post) {
    let ol = &w.outdoor_light;
    let len = sun_dir.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    let d = sun_dir.map(|v| v / len);
    let key = rgb(ol, "KeyColor", 0.0).map(|c| c * first(ol, "KeyColorIntensity", 1.0));
    let top = rgb(ol, "TopColor", 0.0).map(|c| c * first(ol, "TopColorIntensity", 1.0));
    let bottom = rgb(ol, "BottomColor", 0.0).map(|c| c * first(ol, "BottomColorIntensity", 1.0));
    let spec = key.map(|c| c * first(ol, "KeySpecularScale", 1.0));
    let mut v = Values::new();
    let mut put = |k: &str, x: Vec<f32>| {
        v.insert(k.to_owned(), x);
    };
    put("outdoorLightShadowmapSizeAndInvSize", vec![1.0; 4]);
    put("pointLightPos0", vec![0.0, -1000.0, 0.0, 0.0]);
    put("pointLightColor0", vec![0.0; 4]);
    put("pointLightRadius0", vec![0.001, 0.0, 0.0, 0.0]);
    put("time", vec![0.0; 4]);
    put("transparentStartAndEndAndClamp", vec![0.0, 2000.0, 1.0, 0.0]);
    put("viewportZMinMax", vec![0.0, 1.0, 0.0, 0.0]);
    put("outdoorLightHemisphereDir", vec![0.0, 1.0, 0.0, 0.0]);
    for k in 0..3 {
        put(&format!("outdoorLightShadowTransform{k}"), vec![0.0; 16]);
    }
    put("outdoorLightDir", vec![d[0], d[1], d[2], 0.0]);
    put("outdoorLightKeyColor", vec![key[0], key[1], key[2], 0.0]);
    put("outdoorLightTopColor", vec![top[0], top[1], top[2], 0.0]);
    put("outdoorLightBottomColor", vec![bottom[0], bottom[1], bottom[2], 0.0]);
    put(
        "outdoorLightKeySpecularColorAndHemisphereVisibility",
        vec![spec[0], spec[1], spec[2], 1.0],
    );
    let pp = &w.post_process;
    let post = Post {
        middle_gray: first(pp, "MiddleGray", 1.0),
        min_exposure: first(pp, "MinExposure", 1.0),
        max_exposure: first(pp, "MaxExposure", 1.0),
        brightness: rgb(pp, "Brightness", 1.0),
        contrast: rgb(pp, "Contrast", 1.0),
        saturation: rgb(pp, "Saturation", 1.0),
    };
    (v, post)
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    #[test]
    fn fields_and_refs_read_as_renderqa_does() {
        let xml = "<instance>\n<field name=\"ActiveState\">2</field>\n<complex name=\"KeyColor\">0.5/1/0.25/-4</complex>\n\
                   <field name=\"Name\">Weather/x</field>\n<field ref=\"weather/a/sun/0123\" name=\"Sun\" />\n\
                   <item ref=\"weather/a/states/day/4567\" />\n</instance>";
        let f = fields(xml);
        assert_eq!(f.get("ActiveState"), Some(&vec![2.0]));
        assert_eq!(f.get("KeyColor"), Some(&vec![0.5, 1.0, 0.25, -4.0]));
        assert!(!f.contains_key("Name"));
        let r = refs(xml);
        assert_eq!(r.first(), Some(&("weather/a/sun".to_owned(), "Sun".to_owned())));
        assert_eq!(r.get(1), Some(&("weather/a/states/day".to_owned(), String::new())));
    }

    #[test]
    fn light_multiplies_colours_by_their_intensities() {
        let mut w = Weather::default();
        w.outdoor_light.insert("KeyColor".into(), vec![1.0, 0.5, 0.25, 0.0]);
        w.outdoor_light.insert("KeyColorIntensity".into(), vec![2.0]);
        w.outdoor_light.insert("KeySpecularScale".into(), vec![0.5]);
        let (v, post) = light(&w, [0.0, 2.0, 0.0]);
        assert_eq!(v.get("outdoorLightKeyColor"), Some(&vec![2.0, 1.0, 0.5, 0.0]));
        assert_eq!(
            v.get("outdoorLightKeySpecularColorAndHemisphereVisibility"),
            Some(&vec![1.0, 0.5, 0.25, 1.0])
        );
        assert_eq!(v.get("outdoorLightDir"), Some(&vec![0.0, 1.0, 0.0, 0.0]));
        assert_eq!(post, Post::default());
    }
}
