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

use crate::error::RenderError;
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
    /// `Clouds` (empty when the state has none): its `PanoramicCloudLayer`'s ranges, tiling, threshold,
    /// transparency and rotation (round R5d).
    pub clouds: Fields,
    /// The sky dome as the system's settings and setup name it (round R5d): `SkydomeEnable`, the panoramic cloud
    /// blend mode and the static cloud layer's panoramic texture and its alpha.
    pub dome: Dome,
}

/// The sky dome's settings (round R5d): `WeatherSystemSettings.SkydomeEnable` and `PanoramicCloudBlendMode`, and the
/// `PanoramicCloudLayer` textures of the `StaticCloudLayerData` the system's `WeatherSystemSetup.Clouds` names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dome {
    /// `SkydomeEnable` (`None`: the settings do not say).
    pub enabled: Option<bool>,
    /// `PanoramicCloudBlendMode` (`PcbmMultiplyAddAlpha` ...), or empty.
    pub blend_mode: String,
    /// `PanoramicTexture` (`terrains/mp_005/textures/mp05_skydome`), when set.
    pub panoramic: Option<String>,
    /// `PanoramicAlphaTexture`, when set.
    pub panoramic_alpha: Option<String>,
}

/// A field's text (`<field name="X">text<`), the first of that name.
fn text_field<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let tag = format!("<field name=\"{name}\">");
    let at = xml.find(&tag)? + tag.len();
    xml.get(at..)?.split('<').next()
}

/// The sky dome's settings: from the settings partition's XML and the system's setup (its `Setup`, then the setup's
/// `Clouds`).
fn dome(files: &Files, settings_xml: &str, system_xml: &str) -> Dome {
    let by_name = |xml: &str| -> HashMap<String, String> { refs(xml).into_iter().map(|(r, n)| (n, r)).collect() };
    let clouds = by_name(system_xml)
        .get("Setup")
        .and_then(|s| xml_of(files, s))
        .and_then(|x| by_name(&x).get("Clouds").and_then(|c| xml_of(files, c)));
    let tex = |name: &str| clouds.as_deref().and_then(|x| by_name(x).get(name).cloned());
    Dome {
        enabled: text_field(settings_xml, "SkydomeEnable").map(|t| t == "true"),
        blend_mode: text_field(settings_xml, "PanoramicCloudBlendMode")
            .unwrap_or_default()
            .to_owned(),
        panoramic: tex("PanoramicTexture"),
        panoramic_alpha: tex("PanoramicAlphaTexture"),
    }
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
    /// `BloomScale` (r, g, b).
    pub bloom_scale: [f32; 3],
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
            bloom_scale: [0.0; 3],
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

/// The status line's words when a level has no weather the view can read (round GW1): it is drawn under
/// [`neutral`].
pub const NO_WEATHER: &str = "No weather data found for this level; showing a default sky";

/// The game's weather entity: placed through a blueprint whose `Object` is one (Guadalcanal, `MP_005`), or an
/// instance of the level partition itself (`SP_004_B`, Coral Sea).
const ENTITY_TYPE: &str = "Weather.WeatherSystemEntityData";
/// The entity that changes the weather's state (`State`), at once when `StartTransitionOnSpawn`.
const STATE_ENTITY_TYPE: &str = "GameSharedResources.WeatherStateEntityData";

/// A weather entity's references: its `WeatherSystem` and `Settings` (a `WeatherSystemSettings`) partitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRefs {
    /// The `WeatherSystem` partition (`weather/wake_island/weathersystem_wake`).
    pub system: String,
    /// The `Settings` partition (`weather/wake_island/guadal_canal`).
    pub settings: String,
}

/// The instances of a partition's XML of one type (each `<instance ...>` to its end).
fn instances_of<'a>(xml: &'a str, ty: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    xml.split("<instance ").skip(1).filter(move |b| {
        b.split('>')
            .next()
            .is_some_and(|h| h.contains(&format!("type=\"{ty}\"")))
    })
}

/// The weather entities of a partition's XML, in order, with both references set.
#[must_use]
pub fn entities_in(xml: &str) -> Vec<EntityRefs> {
    instances_of(xml, ENTITY_TYPE)
        .filter_map(|b| {
            let by_name: HashMap<String, String> = refs(b).into_iter().map(|(r, n)| (n, r)).collect();
            Some(EntityRefs {
                system: by_name.get("WeatherSystem")?.clone(),
                settings: by_name.get("Settings")?.clone(),
            })
        })
        .collect()
}

/// The state a level's weather may move to as it starts: the first enabled `WeatherStateEntityData` of the level
/// partition with `StartTransitionOnSpawn` names its `State` (Coral Sea: settings `ActiveState` 0, the entity's
/// `State` 4, `coral_sea`). Read for notes only: whether the engine applies it at load is not known yet (no capture),
/// so the view draws the settings' `ActiveState` (RNDR-WTHR-1).
#[must_use]
pub fn spawn_state_in(xml: &str) -> Option<usize> {
    instances_of(xml, STATE_ENTITY_TYPE).find_map(|b| {
        let text = |name: &str| {
            let tag = format!("<field name=\"{name}\">");
            let at = b.find(&tag)? + tag.len();
            b.get(at..).and_then(|r| r.split('<').next())
        };
        (text("Enabled") == Some("true") && text("StartTransitionOnSpawn") == Some("true"))
            .then(|| text("State")?.parse().ok())
            .flatten()
    })
}

/// The references of the weather entity a placed blueprint is (round GW1): its partition holds a
/// `WeatherSystemEntityData`, which names its `WeatherSystem` and `Settings` partitions by path - in any folder and
/// any bundle of the level's stack (Guadalcanal's `weather/wake_island/guadal_canal_e` names
/// `weather/wake_island/weathersystem_wake` and `weather/wake_island/guadal_canal`). `None`: the blueprint is not
/// read yet, or is not a weather entity.
#[must_use]
pub fn entity(files: &Files, blueprint: &str) -> Option<EntityRefs> {
    let bytes = files.read(&format!("{blueprint}.dbx"))?;
    // the type names are plain text in a binary partition: skip every blueprint that is not a weather entity before
    // converting it
    let short = ENTITY_TYPE.rsplit('.').next().unwrap_or(ENTITY_TYPE).as_bytes();
    if !bytes.windows(short.len()).any(|w| w == short) {
        return None;
    }
    entities_in(&dbx::to_xml(&bytes).ok()?).into_iter().next()
}

/// The active weather state of a weather folder (`weather/r_mp005`), its `WeatherSettings*` and `WeatherSystem*`
/// partitions found by name (renderqa's rule; DICE's stock folders follow it, OPFOR's WW2 levels do not: use
/// [`entity`] and [`read_system`]).
///
/// # Errors
/// No `WeatherSettings` / `WeatherSystem` partition, or the state's partitions are missing.
pub fn read(files: &Files, folder: &str) -> Result<Weather, RenderError> {
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
        return Err(RenderError::Data(format!(
            "no WeatherSettings / WeatherSystem partition under {folder}/"
        )));
    };
    read_system(files, &system, &settings, None)
}

/// The active state of a `WeatherSystem` partition: `state` when given (the level's spawn transition,
/// [`spawn_state_in`]), else its `WeatherSystemSettings` partition's `ActiveState` (both by partition path, as a
/// weather entity names them: [`entity`]).
///
/// # Errors
/// A partition is not found (or not read yet), or the state's partitions are missing.
pub fn read_system(files: &Files, system: &str, settings: &str, state: Option<usize>) -> Result<Weather, RenderError> {
    let sx = xml_of(files, settings).ok_or_else(|| RenderError::Data(format!("{settings}: not found")))?;
    let yx = xml_of(files, system).ok_or_else(|| RenderError::Data(format!("{system}: not found")))?;
    let active = state.unwrap_or_else(|| {
        fields(&sx)
            .get("ActiveState")
            .and_then(|v| v.first().copied())
            .unwrap_or(0.0) as usize
    });
    let states: Vec<String> = refs(&yx)
        .into_iter()
        .filter(|(_, n)| n.is_empty())
        .map(|(r, _)| r)
        .collect();
    let state = states
        .get(active)
        .ok_or_else(|| {
            RenderError::Data(format!(
                "{settings}: ActiveState {active} of {system}'s {} states",
                states.len()
            ))
        })?
        .clone();
    let stx = xml_of(files, &state).ok_or_else(|| RenderError::Data(format!("{state}: not found")))?;
    let by_kind: HashMap<String, String> = refs(&stx).into_iter().map(|(r, n)| (n, r)).collect();
    let part = |kind: &str| -> Result<Fields, RenderError> {
        let r = by_kind
            .get(kind)
            .ok_or_else(|| RenderError::Data(format!("{state}: no {kind} partition")))?;
        Ok(fields(
            &xml_of(files, r).ok_or_else(|| RenderError::Data(format!("{r}: not found")))?,
        ))
    };
    Ok(Weather {
        sun: part("Sun")?,
        outdoor_light: part("OutdoorLight")?,
        post_process: part("PostProcess")?,
        fog: part("Fog").unwrap_or_default(),
        sky: part("Sky").unwrap_or_default(),
        clouds: part("Clouds").unwrap_or_default(),
        dome: dome(files, &sx, &yx),
        state,
    })
}

/// A weather system's states (partition paths, in `States` order: the index `ActiveState` and a state entity's
/// `State` name), read through the level's files (round VW2).
///
/// # Errors
/// The system partition is not found.
pub fn system_states(files: &Files, system: &str) -> Result<Vec<String>, RenderError> {
    let yx = xml_of(files, system).ok_or_else(|| RenderError::Data(format!("{system}: not found")))?;
    Ok(refs(&yx)
        .into_iter()
        .filter(|(_, n)| n.is_empty())
        .map(|(r, _)| r)
        .collect())
}

/// A level's weather as data (round VW2): where its weather entity is, the system and settings it names, the
/// system's states, the settings' `ActiveState` and the level's spawn-time switch (RNDR-WTHR-6).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LevelWeather {
    /// The partition holding the weather entity (the level partition, or a placed blueprint).
    pub entity_at: String,
    /// The `WeatherSystem` partition.
    pub system: String,
    /// The `WeatherSystemSettings` partition.
    pub settings: String,
    /// The system's states (partition paths).
    pub states: Vec<String>,
    /// The settings' `ActiveState`.
    pub active: usize,
    /// The state the level's first enabled `WeatherStateEntityData` with `StartTransitionOnSpawn` names.
    pub spawn: Option<usize>,
}

impl LevelWeather {
    /// The state players see as the level starts: the spawn-time switch when it names one of the system's states,
    /// else `ActiveState` (WX0: in 45 of 70 levels with a switch it names another state than `ActiveState`; the
    /// placeholder states some settings name are dark or unlit and never seen).
    #[must_use]
    pub fn starts_in(&self) -> usize {
        self.spawn.filter(|&s| s < self.states.len()).unwrap_or(self.active)
    }

    /// A state's short name (`weather/r_mp02/states/ons_mp_002_night` -> `ons_mp_002_night`).
    #[must_use]
    pub fn name(&self, k: usize) -> String {
        self.states
            .get(k)
            .map_or_else(String::new, |p| p.rsplit('/').next().unwrap_or(p).to_owned())
    }

    /// The level's weather read: its system's states and its settings' `ActiveState`.
    ///
    /// # Errors
    /// The system or the settings partition is not found.
    pub fn read(files: &Files, entity_at: &str, refs: &EntityRefs, spawn: Option<usize>) -> Result<Self, RenderError> {
        let sx =
            xml_of(files, &refs.settings).ok_or_else(|| RenderError::Data(format!("{}: not found", refs.settings)))?;
        let active = fields(&sx)
            .get("ActiveState")
            .and_then(|v| v.first().copied())
            .unwrap_or(0.0) as usize;
        Ok(Self {
            entity_at: entity_at.to_owned(),
            system: refs.system.clone(),
            settings: refs.settings.clone(),
            states: system_states(files, &refs.system)?,
            active,
            spawn,
        })
    }

    /// State `k`'s partitions read.
    ///
    /// # Errors
    /// [`read_system`]'s.
    pub fn state(&self, files: &Files, k: usize) -> Result<Weather, RenderError> {
        read_system(files, &self.system, &self.settings, Some(k))
    }
}

/// What a weather state looks like, worked out from its field values alone (round VW2): a grey ground (albedo 0.3)
/// facing up under the state's key light at [`NOON_DIR`] and its sky (top) colour, exposed as the game's post-process
/// exposes a frame of it (`MiddleGray` over its luminance, clamped to `MinExposure` .. `MaxExposure`) and tonemapped
/// ([`crate::tonemap::map`]); the fog colour likewise. The simplified view tints by it; the game-accurate view's
/// frames are measured against it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// The lit ground in HDR (before exposure).
    pub hdr: [f32; 3],
    /// The exposure.
    pub exposure: f32,
    /// The ground on screen (0..1).
    pub ground: [f32; 3],
    /// The fog / haze colour on screen (0..1).
    pub fog: [f32; 3],
}

/// The albedo [`look`] lights.
pub const LOOK_ALBEDO: f32 = 0.3;

/// A state's [`Look`].
#[must_use]
pub fn look(w: &Weather) -> Look {
    let ol = &w.outdoor_light;
    let len = NOON_DIR.iter().map(|v| v * v).sum::<f32>().sqrt();
    let up = NOON_DIR.get(1).copied().unwrap_or(1.0) / len;
    let key = rgb(ol, "KeyColor", 0.0).map(|c| c * first(ol, "KeyColorIntensity", 1.0));
    let top = rgb(ol, "TopColor", 0.0).map(|c| c * first(ol, "TopColorIntensity", 1.0));
    let hdr: [f32; 3] = std::array::from_fn(|k| {
        LOOK_ALBEDO * (key.get(k).copied().unwrap_or(0.0) * up + top.get(k).copied().unwrap_or(0.0))
    });
    let (_, post) = light(w, NOON_DIR);
    let lum = hdr.iter().zip([0.3, 0.59, 0.11]).map(|(c, w)| c * w).sum::<f32>();
    let exposure =
        (post.middle_gray / (lum + 0.001)).clamp(post.min_exposure, post.max_exposure.max(post.min_exposure));
    let fog = rgb(&w.fog, "Color", 0.4);
    Look {
        hdr,
        exposure,
        ground: crate::tonemap::map(hdr, exposure, &post),
        fog: crate::tonemap::map(fog, exposure, &post),
    }
}

/// The simplified view's tint for a state: its [`Look`]'s ground over [`neutral`]'s (a plain day is about 1; a
/// night about 0.3, blue), each channel within 0.05 .. 1.5.
#[must_use]
pub fn tint(w: &Weather) -> [f32; 3] {
    let a = look(w).ground;
    let b = look(&neutral()).ground;
    std::array::from_fn(|k| {
        let (x, y) = (a.get(k).copied().unwrap_or(0.0), b.get(k).copied().unwrap_or(1.0));
        (x / y.max(1e-3)).clamp(0.05, 1.5)
    })
}

/// A neutral daylight for a level without weather data ([`NO_WEATHER`]): a white key light, a pale blue sky light
/// and a grey-blue fog, with the post-process values of DICE's daytime states (Guadalcanal's: middle grey 0.03,
/// exposure 1.5 to 2.4). Not the game's look of any level.
#[must_use]
pub fn neutral() -> Weather {
    let f = |pairs: &[(&str, &[f32])]| -> Fields { pairs.iter().map(|(k, v)| ((*k).to_owned(), v.to_vec())).collect() };
    Weather {
        state: "(default sky: no weather data)".to_owned(),
        sun: Fields::new(),
        outdoor_light: f(&[
            ("KeyColor", &[1.0, 0.97, 0.92]),
            ("KeyColorIntensity", &[4.0]),
            ("KeySpecularScale", &[1.0]),
            ("TopColor", &[0.4, 0.45, 0.57]),
            ("TopColorIntensity", &[1.0]),
            ("BottomColor", &[0.35, 0.4, 0.48]),
            ("BottomColorIntensity", &[0.5]),
        ]),
        post_process: f(&[
            ("MiddleGray", &[0.03]),
            ("MinExposure", &[1.5]),
            ("MaxExposure", &[2.4]),
            ("Brightness", &[1.0, 1.0, 1.0]),
            ("Contrast", &[1.0, 1.0, 1.0]),
            ("Saturation", &[1.0, 1.0, 1.0]),
            ("BloomScale", &[0.05, 0.05, 0.05]),
        ]),
        fog: f(&[("Color", &[0.58, 0.66, 0.74])]),
        sky: Fields::new(),
        clouds: Fields::new(),
        dome: Dome::default(),
    }
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
        bloom_scale: rgb(pp, "BloomScale", 0.0),
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

    // spec: RNDR-WTHR-5, RNDR-WTHR-6
    #[test]
    fn weather_entities_and_spawn_states_read_from_a_partition() {
        // Coral Sea's level partition, cut down: a state entity that transitions on spawn, then the weather entity
        let xml = "<partition>\n\
            <instance guid=\"1\" type=\"GameSharedResources.WeatherStateEntityData\">\n\
            <field name=\"Enabled\">false</field>\n<field name=\"State\">3</field>\n\
            <field name=\"StartTransitionOnSpawn\">true</field>\n</instance>\n\
            <instance guid=\"2\" type=\"GameSharedResources.WeatherStateEntityData\">\n\
            <field name=\"Enabled\">true</field>\n<field name=\"DefaultState\">0</field>\n\
            <field name=\"State\">4</field>\n<field name=\"StartTransitionOnSpawn\">true</field>\n</instance>\n\
            <instance guid=\"3\" type=\"Weather.WeatherSystemEntityData\">\n\
            <field ref=\"weather/wake_island/weathersystem_wake/e136\" name=\"WeatherSystem\" />\n\
            <field ref=\"weather/wake_island/guadal_canal/8697\" name=\"Settings\" />\n</instance>\n\
            <instance guid=\"4\" type=\"Weather.WeatherSystemEntityData\">\n\
            <field ref=\"null\" name=\"WeatherSystem\" />\n</instance>\n</partition>";
        assert_eq!(
            entities_in(xml),
            vec![EntityRefs {
                system: "weather/wake_island/weathersystem_wake".into(),
                settings: "weather/wake_island/guadal_canal".into(),
            }]
        );
        assert_eq!(spawn_state_in(xml), Some(4));
        assert_eq!(spawn_state_in("<partition />"), None);
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
