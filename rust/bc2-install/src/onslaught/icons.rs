//! Stage 14 of the console Onslaught port (round OR13): the port of Project OPFOR's `console_icons.py` - the Mark V's
//! own HUD / kill-feed picture (Valparaiso Onslaught, `ons_mp_002`, Name `MRKV`) in the weaponicon movie of the
//! level's `ingame-00` and `loader-00` bundles, and the gate that every level's vehicles have theirs.
//!
//! Why: the vehicle HUD's picture (under the ENTER prompt, in the vehicle) and the kill feed's are frames of the
//! weaponicon movie's WeaponIcon timeline ([`WEAPON_ICON`]) labelled with the `VehicleEntityData`'s Name; a Name
//! without a label shows frame 0, a skull. DICE drew no Mark V picture for PC, so it gets a silhouette in DICE's style
//! (`bc2_convert::pictures::side`, round OR14's polygon fill) drawn from DICE's own PC mesh of the whole boat
//! (`Objects/Objects/Distant_Ships/MarkV/MarkV_Static_Mesh`, mp_005's distant Mark V), put into the atlas's free
//! space with an atlas entry, an image character, its shape and a WeaponIcon frame labelled `MRKV` (`bc2_menu`'s
//! `add_icon_frame` on the patrol boat's frame); the small icon takes the patrol boat's shape by a label alias.
//!
//! The checks: aptcheck passes (the suite's `bfbc2 aptcheck`, run as the command line the caller names: its library
//! is bc2-cli's), every vehicle Name the level uses has a WeaponIcon label, each new frame places one textured quad,
//! and the atlas box reads back the picture. The work folder's files are the retired Python's byte for byte but the
//! preview picture's PNG bytes (`work\console_icons\preview`, a picture for review: its pixels the Python's, its
//! encoding the `png` crate's, not Pillow's zlib stream; rust/BENCHMARKS.md "OR13").

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::LazyLock;

use bc2_formats::aptmodel::Val;
use bc2_menu::graft::{self, GRAFT_LAYOUT, Movie, edit, icon};
use regex::Regex;

use super::port::{StageOut, find, manifest_lines, read_text_utf8, write_manifest};
use super::{OnsError, R, dbx, fbrb, join, joins, read, rmtree, size, write};

/// The patrol boat's Name: the frame the new one is made like.
pub const LIKE: &str = "PBLB";
/// The picture's size (the patrol boat's).
pub const SIZE: (usize, usize) = (78, 36);
/// The weaponicon movie's timelines (every PC / Onslaught copy): the HUD / kill-feed picture, the small vehicle
/// icon, the seat map.
pub const WEAPON_ICON: i64 = 483;
/// The small vehicle icon's timeline.
pub const SMALL: i64 = 34;
/// The seat map's timeline.
pub const SEATS: i64 = 104;
/// The movie and its atlas in a bundle.
pub const MOVIE: &str = "Menu/weaponicon.res";
/// The atlas picture.
pub const ATLAS: &str = "Menu/weaponicon_textures_Win32/atlas0.res";
/// The atlas's entries.
pub const META: &str = "Menu/weaponicon_textures_Win32/atlas_meta.dbx";
/// The bundles the stage writes (`Package\levels\<lv>\<name>.fbrb`).
pub const BUNDLES: [&str; 2] = ["ingame-00", "loader-00"];

/// One picture: (Name, mesh, part endings left out, part endings outlined, seat-map alias).
pub type Icon = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    Option<&'static str>,
);

/// Level -> its pictures (`ICONS`).
pub const ICONS: [(&str, &[Icon]); 1] = [(
    "ons_mp_002",
    &[(
        "MRKV",
        "Objects/Objects/Distant_Ships/MarkV/MarkV_Static_Mesh",
        &[],
        &[],
        None,
    )],
)];
/// The LCRS (`JETS`): no multiplayer level uses it (`SPARE`).
pub const SPARE: [Icon; 1] = [(
    "JETS",
    "Objects/Vehicles/Sea/LCRS/LCRS_Mesh",
    &[],
    &["_Metal"],
    Some(LIKE),
)];

#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
fn re(p: &str) -> Regex {
    Regex::new(p).expect("a constant pattern")
}

static VEHICLE_DATA: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)type="GameSharedResources\.VehicleEntityData"[^>]*>(.*?)</instance>"#));
static NAME: LazyLock<Regex> = LazyLock::new(|| re(r#"<field name="Name">([^<]*)</field>"#));
static VTYPE: LazyLock<Regex> = LazyLock::new(|| re(r#"<field name="VehicleType">([^<]*)</field>"#));
static SOURCE: LazyLock<Regex> = LazyLock::new(|| re(r"^Bundle(Source|Target) (package|game):(\S+)\.fbrb"));
static LEVEL00: LazyLock<Regex> = LazyLock::new(|| re(r"^levels/([^/]+)/level-00$"));
static ATLAS_INFO: LazyLock<Regex> = LazyLock::new(|| re(r#"(?s)type="UI.UITextureAtlasInfo"[^>]*>(.*?)</instance>"#));
static MAXV: LazyLock<Regex> = LazyLock::new(|| re(r#"name="MaxV">([^<]*)<"#));
static ATLAS_INST: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)(\n(\t+)<instance guid="([^"]+)" type="UI.UITextureAtlasInfo"[^>]*>.*?</instance>)"#));

/// `VEHICLE_REF` over lower-case bytes: `objects/vehicles/<...>` before a `/<guid>` (the partition, not the
/// instance).
fn vehicle_refs(data: &[u8]) -> Vec<String> {
    #[allow(clippy::expect_used, reason = "a constant pattern")]
    static R: LazyLock<regex::bytes::Regex> = LazyLock::new(|| {
        regex::bytes::Regex::new(r"objects/vehicles/[a-z0-9_\-/ .]+?(?:/[0-9a-f]{8}-[0-9a-f]{4}-)").expect("pattern")
    });
    // Python's lookahead (?=/<guid start>) as a match that ends with it, the match taken without it; the next search
    // starts after the reference (the lookahead consumed nothing)
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(m) = R.find_at(data, at) {
        let s = m.as_bytes();
        let cut = s.len() - "/xxxxxxxx-xxxx-".len();
        out.push(String::from_utf8_lossy(&s[..cut]).into_owned());
        at = m.start() + cut;
    }
    out
}

fn menu(e: &bc2_menu::error::MenuError) -> OnsError {
    OnsError::Data(e.to_string())
}

/// `vehicle_names(datas)`: the `VehicleEntityData` Names in the partitions' data, but a dummy's
/// (`VehicleHudType_Dummy`: the static F18 flyby has no label in any of DICE's movies).
pub fn vehicle_names(datas: &[Vec<u8>]) -> R<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for data in datas {
        if !data.windows(17).any(|w| w == b"VehicleEntityData") {
            continue;
        }
        let x = bc2_formats::dbx::to_xml(data).map_err(|e| OnsError::Data(e.to_string()))?;
        for c in VEHICLE_DATA.captures_iter(&x) {
            let body = &c[1];
            let n = NAME.captures(body).map(|m| m[1].to_owned());
            let t = VTYPE.captures(body).map(|m| m[1].to_owned());
            if let Some(n) = n.filter(|n| !n.is_empty())
                && t.as_deref() != Some("VehicleHudType_Dummy")
            {
                out.insert(n);
            }
        }
    }
    Ok(out)
}

/// `chains(mft)`: {source bundle (`levels/<lv>/<name>-00`, lower case): [the bundle paths it overlays, in order]}.
pub fn chains(game: &str, mft: &str) -> R<BTreeMap<String, Vec<String>>> {
    let text = super::read_text(mft)?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut src: Option<String> = None;
    for line in bc2_pyrt::py::splitlines(&text) {
        let l = bc2_pyrt::py::strip(line);
        let Some(m) = SOURCE.captures(l) else {
            if l.is_empty() {
                src = None;
            }
            continue;
        };
        if &m[1] == "Source" {
            let s = m[3].to_lowercase();
            out.insert(s.clone(), Vec::new());
            src = Some(s);
        } else if let Some(s) = &src {
            let base = if &m[2] == "package" {
                join(game, "Package")
            } else {
                joins(game, &["Dist", "win32"])
            };
            let p = m[3].split('/').fold(base, |acc, x| join(&acc, x));
            if let Some(v) = out.get_mut(s) {
                v.push(format!("{p}.fbrb"));
            }
        }
    }
    Ok(out)
}

/// The insertion order `chains` gave (Python's dict): the sources in the file's order.
fn chain_order(mft: &str) -> R<Vec<String>> {
    let text = super::read_text(mft)?;
    let mut out: Vec<String> = Vec::new();
    for line in bc2_pyrt::py::splitlines(&text) {
        if let Some(m) = SOURCE.captures(bc2_pyrt::py::strip(line))
            && &m[1] == "Source"
        {
            let s = m[3].to_lowercase();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    Ok(out)
}

/// `live(bundle)`: {lower name: name} of the bundle's live entries (a later one of a name winning).
fn live(bundle: &str) -> R<BTreeMap<String, String>> {
    let h = bc2_gamefs::bundles::entries_path(bundle).map_err(|e| OnsError::Data(format!("{bundle}: {e}")))?;
    Ok(h.entries
        .iter()
        .filter(|e| !e.is_deleted())
        .map(|e| (e.name.to_lowercase(), e.name.clone()))
        .collect())
}

fn read_entries(bundle: &str, names: &[String]) -> R<Vec<Vec<u8>>> {
    bc2_gamefs::bundles::read_path(bundle, names).map_err(|e| OnsError::Data(format!("{bundle}: {e}")))
}

fn is_file(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

/// `level_vehicles(chain, own)`: the vehicle Names a level uses - each vehicle partition a partition of the level's
/// own bundles names, read from the first bundle of the chain that has it.
pub fn level_vehicles(chain: &[String], own: &[String]) -> R<BTreeSet<String>> {
    let mut refs: BTreeSet<String> = BTreeSet::new();
    for b in own.iter().filter(|b| is_file(b)) {
        let ents = live(b)?;
        let dbx: Vec<(String, String)> = ents
            .iter()
            .filter(|(l, _)| l.ends_with(".dbx"))
            .map(|(l, n)| (l.clone(), n.clone()))
            .collect();
        let names: Vec<String> = dbx.iter().map(|(_, n)| n.clone()).collect();
        let datas = read_entries(b, &names)?;
        for ((low, _), data) in dbx.iter().zip(&datas) {
            let mine = &low[..low.len() - 4];
            for r in vehicle_refs(&data.to_ascii_lowercase()) {
                if r != mine {
                    refs.insert(r);
                }
            }
        }
    }
    let mut where_: BTreeMap<String, (String, String)> = BTreeMap::new();
    for b in chain.iter().filter(|b| is_file(b)) {
        let ents = live(b)?;
        for r in &refs {
            if !where_.contains_key(r)
                && let Some(n) = ents.get(&format!("{r}.dbx"))
            {
                where_.insert(r.clone(), (b.clone(), n.clone()));
            }
        }
    }
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (b, n) in where_.into_values() {
        by.entry(b).or_default().push(n);
    }
    let mut datas = Vec::new();
    for (b, ns) in &by {
        datas.extend(read_entries(b, ns)?);
    }
    vehicle_names(&datas)
}

/// `level_movie(chain)`: the first bundle of a level's `ingame-00` overlay order holding the weaponicon movie.
fn level_movie(chain: &[String]) -> R<Option<String>> {
    for b in chain.iter().filter(|b| is_file(b)) {
        if live(b)?.contains_key(&MOVIE.to_lowercase()) {
            return Ok(Some(b.clone()));
        }
    }
    Ok(None)
}

fn bundle_path(game: &str, lv: &str, name: &str) -> String {
    joins(game, &["Package", "levels", lv, &format!("{name}.fbrb")])
}

fn mft_path(game: &str) -> String {
    joins(game, &["Dist", "win32", "package.mft"])
}

fn own_of(chain: &[String], lv: &str) -> Vec<String> {
    let mark = format!("\\{lv}\\");
    chain
        .iter()
        .filter(|b| b.to_lowercase().contains(&mark))
        .cloned()
        .collect()
}

/// `names(lv)`: the vehicle Names the level uses.
pub fn names(game: &str, lv: &str) -> R<BTreeSet<String>> {
    let ch = chains(game, &mft_path(game))?;
    let chain = ch
        .get(&format!("levels/{lv}/level-00"))
        .cloned()
        .unwrap_or_else(|| vec![bundle_path(game, lv, "level-00")]);
    level_vehicles(&chain, &own_of(&chain, lv))
}

/// The WeaponIcon timeline's label frame of a Name (`labelled`).
fn labelled(m: &mut Movie, name: &str, tl: i64) -> R<Option<usize>> {
    let o = m.char(tl).map_err(|e| menu(&e))?;
    match m.label_frame(name, o) {
        Ok(f) => Ok(Some(f)),
        Err(bc2_menu::error::MenuError::Apt(_)) => Ok(None),
        Err(e) => Err(menu(&e)),
    }
}

/// `missing(m, names)`: the Names without a WeaponIcon label (each would show the skull), sorted.
fn missing(m: &mut Movie, names: &BTreeSet<String>) -> R<Vec<String>> {
    let mut out = Vec::new();
    for n in names {
        if labelled(m, n, WEAPON_ICON)?.is_none() {
            out.push(n.clone());
        }
    }
    Ok(out)
}

/// `add_icon(m, name, seat_alias)`: (image character, WeaponIcon frames added, label aliases added).
fn add_icon(m: &mut Movie, name: &str, seat_alias: Option<&str>) -> R<(i64, usize, usize)> {
    let hidden = icon::hide_labels(m, name, WEAPON_ICON, "__opfor_hidden_").map_err(|e| menu(&e))?;
    #[allow(clippy::cast_precision_loss, reason = "a picture's size")]
    let (w, h) = (SIZE.0 as f64, SIZE.1 as f64);
    let r = icon::add_icon_frame_neg(m, LIKE, name, (w, h), (-w, -h));
    icon::restore_labels(m, &hidden).map_err(|e| menu(&e))?;
    let (image, frames) = r.map_err(|e| menu(&e))?;
    let mut aliases = 0;
    if labelled(m, name, SMALL)?.is_none() {
        aliases += edit::add_label_alias(m, LIKE, name).map_err(|e| menu(&e))?;
    }
    if let Some(seat) = seat_alias
        && labelled(m, name, SEATS)?.is_none()
    {
        aliases += edit::add_label_alias(m, seat, name).map_err(|e| menu(&e))?;
    }
    Ok((image, frames, aliases))
}

/// `single_quad(m, name)`: the Name's WeaponIcon frame places one shape of one textured quad.
fn single_quad(m: &mut Movie, name: &str) -> R<bool> {
    let o = m.char(WEAPON_ICON).map_err(|e| menu(&e))?;
    let f = m.label_frame(name, o).map_err(|e| menu(&e))?;
    let mut places: Vec<i64> = Vec::new();
    for it in m.frame_items(f, o).map_err(|e| menu(&e))? {
        if m.tag(it).map_err(|e| menu(&e))? != "place" {
            continue;
        }
        let v = m.vals(it).map_err(|e| menu(&e))?.clone();
        let (Some(Val::Num(flags)), Some(Val::Num(cid))) = (v.get(1), v.get(3)) else {
            continue;
        };
        if flags & 0x02 != 0 && m.char_type(i64::from(*cid)).map_err(|e| menu(&e))? == Some(1) {
            places.push(i64::from(*cid));
        }
    }
    if places.len() != 1 {
        return Ok(false);
    }
    Ok(m.geo_entry(places[0]).map_err(|e| menu(&e))?.is_some())
}

fn u32_at(b: &[u8], at: usize) -> R<u32> {
    b.get(at..at + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| OnsError::Data("struct.error: unpack_from requires a buffer of at least 4 bytes".into()))
}

/// `atlas_place(atlas, meta_x, pic)`: the picture written into the atlas's free space (below every entry, 2 pixels
/// apart): (the atlas, MinU, MaxU, MinV, MaxV).
pub fn atlas_place(atlas: &[u8], meta_x: &str, pic: &Pic) -> R<(Vec<u8>, [f64; 4])> {
    let head: Vec<u32> = (0..8).map(|k| u32_at(atlas, 4 * k)).collect::<R<_>>()?;
    if head[2] != 9 || head[7] != 1 {
        return Err(OnsError::Exit(format!(
            "weaponicon atlas: format {}, {} levels - expected BGRA8, one level",
            head[2], head[7]
        )));
    }
    let (ww, hh) = (head[4] as usize, head[5] as usize);
    let mut used = 0.0f64;
    for c in ATLAS_INFO.captures_iter(meta_x) {
        let v = MAXV
            .captures(&c[1])
            .ok_or_else(|| OnsError::Data("not found: MaxV".into()))?;
        let x: f64 = bc2_pyrt::py::strip(&v[1])
            .parse()
            .map_err(|_| OnsError::Data(format!("ValueError: could not convert string to float: '{}'", &v[1])))?;
        used = used.max(x);
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "np.ceil of a row"
    )]
    let r0 = (used * hh as f64).ceil() as usize + 2;
    let (h, w) = (pic.h, pic.w);
    if r0 + h > hh {
        return Err(OnsError::Exit("weaponicon atlas: no free space".into()));
    }
    let px = atlas
        .get(92..92 + ww * hh * 4)
        .ok_or_else(|| OnsError::Data("ValueError: buffer is smaller than requested size".into()))?;
    let mut px = px.to_vec();
    for y in r0..r0 + h {
        for x in 0..w {
            if px[(y * ww + x) * 4 + 3] != 0 {
                return Err(OnsError::Exit("weaponicon atlas: the free space has pixels".into()));
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let s = &pic.px[(y * w + x) * 4..(y * w + x) * 4 + 4];
            let d = ((r0 + y) * ww + x) * 4;
            px[d..d + 4].copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
    }
    let mut out = atlas[..92].to_vec();
    out.extend(px);
    out.extend(&atlas[92 + ww * hh * 4..]);
    #[allow(clippy::cast_precision_loss, reason = "atlas sizes")]
    let uv = [
        0.0,
        w as f64 / ww as f64,
        r0 as f64 / hh as f64,
        (r0 + h) as f64 / hh as f64,
    ];
    Ok((out, uv))
}

/// `meta_with(src_path, image, uv, work, tag)`: the atlas's entries with a `UITextureAtlasInfo` for the image (an XML
/// edit of the first entry's copy, DbxEdit import `--like` the original); its GUID `opfor_guid('ww2', tag, image)`.
pub fn meta_with(src_path: &str, image: i64, uv: [f64; 4], work: &str, tag: &str) -> R<Vec<u8>> {
    dbx(&["xml", src_path])?;
    let xml = format!("{}.xml", super::stem4(src_path));
    let x = read_text_utf8(&xml)?;
    let m = ATLAS_INST
        .captures(&x)
        .ok_or_else(|| OnsError::Data("not found: the pattern".into()))?;
    let g = bc2_formats::guid::new_guid("opfor-ww2", &[tag, &image.to_string()]);
    let old = m[3].to_owned();
    let mut inst = m[1].replace(&old, &g);
    let vals: Vec<String> = std::iter::once(image.to_string())
        .chain(uv.iter().map(|u| bc2_pyrt::fmt::float_g_prec(*u, 7)))
        .collect();
    for (f, v) in ["TextureFile", "MinU", "MaxU", "MinV", "MaxV"].iter().zip(&vals) {
        let p = re(&format!(r#"(<field name="{f}">)[^<]*(<)"#));
        let n = p.find_iter(&inst).count();
        if n != 1 {
            return Err(OnsError::Exit(format!("atlas_meta: no {f}")));
        }
        inst = p
            .replace(&inst, |c: &regex::Captures<'_>| format!("{}{v}{}", &c[1], &c[2]))
            .into_owned();
    }
    let last = x
        .rfind("</instance>")
        .ok_or_else(|| OnsError::Data("IndexError: list index out of range".into()))?
        + "</instance>".len();
    let mut x = format!("{}{inst}{}", &x[..last], &x[last..]);
    let item_re = re(&format!(
        r#"(\n\t+)<item ref="{}" />"#,
        regex::escape(&old.to_lowercase())
    ));
    let item = item_re
        .captures(&x)
        .ok_or_else(|| OnsError::Data("not found: the pattern".into()))?;
    let (item_end, indent) = (item.get(0).map_or(0, |m| m.end()), item[1].to_owned());
    // x.find('</array>', item.end()): -1 when absent, and x[:-1] + ... + x[-1:] then
    let arr_end = x[item_end..].find("</array>").map_or(x.len() - 1, |p| item_end + p);
    let ind = &indent[..indent.len() - 1];
    x = format!(
        "{}\t<item ref=\"{}\" />{ind}{}",
        &x[..arr_end],
        g.to_lowercase(),
        &x[arr_end..]
    );
    write(&xml, x.as_bytes())?;
    let dst = join(work, "meta_new.dbx");
    let o = bc2_tools::dbxedit::run(
        &[
            "import".to_owned(),
            xml.clone(),
            dst.clone(),
            "--like".to_owned(),
            src_path.to_owned(),
        ],
        None,
    );
    if o.code != 0 || format!("{}{}", o.stdout, o.stderr).contains("guess") {
        return Err(OnsError::Exit(format!("atlas_meta: {}{}", o.stdout, o.stderr)));
    }
    read(&dst)
}

/// A picture: RGBA rows.
pub type Pic = bc2_convert::pictures::Pic;

/// `readback(atlas, meta, image, pic)`: the atlas entry's box holds the picture.
fn readback(atlas: &[u8], meta: &[u8], image: i64, pic: &Pic) -> R<bool> {
    let mx = bc2_formats::dbx::to_xml(meta).map_err(|e| OnsError::Data(e.to_string()))?;
    let p = re(&format!(
        r#"(?s)<field name="TextureFile">{image}</field>(.*?)</instance>"#
    ));
    let f = p
        .captures(&mx)
        .ok_or_else(|| OnsError::Data(format!("not found: TextureFile {image}")))?;
    let mut bx = [0.0f64; 4];
    for (k, key) in ["MinU", "MaxU", "MinV", "MaxV"].iter().enumerate() {
        let v = re(&format!(r#"name="{key}">([^<]*)<"#))
            .captures(&f[1])
            .ok_or_else(|| OnsError::Data(format!("not found: {key}")))?[1]
            .to_owned();
        bx[k] = bc2_pyrt::py::strip(&v)
            .parse()
            .map_err(|_| OnsError::Data(format!("ValueError: could not convert string to float: '{v}'")))?;
    }
    let (ww, hh) = (u32_at(atlas, 16)? as usize, u32_at(atlas, 20)? as usize);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "Python's round of a pixel row"
    )]
    let r = |x: f64, n: usize| -> usize { (x * n as f64).round_ties_even().max(0.0) as usize };
    let (y0, y1, x0, x1) = (
        r(bx[2], hh).min(hh),
        r(bx[3], hh).min(hh),
        r(bx[0], ww).min(ww),
        r(bx[1], ww).min(ww),
    );
    if y1.saturating_sub(y0) != pic.h || x1.saturating_sub(x0) != pic.w {
        return Ok(false);
    }
    for y in 0..pic.h {
        for x in 0..pic.w {
            let a = 92 + ((y0 + y) * ww + x0 + x) * 4;
            let Some(s) = atlas.get(a..a + 4) else { return Ok(false) };
            let q = &pic.px[(y * pic.w + x) * 4..(y * pic.w + x) * 4 + 4];
            if [s[2], s[1], s[0], s[3]] != [q[0], q[1], q[2], q[3]] {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// `picture(mesh, skip, detail)`: the silhouette of a mesh's first level of detail (`type95_icon.silhouette`).
fn picture(ix: &bc2_convert::meshview::Index, mesh: &str, skip: &[&str], detail: &[&str]) -> R<Pic> {
    let m = bc2_convert::meshview::load_mesh(ix, mesh, 0).map_err(OnsError::Data)?;
    let parts: Vec<bc2_convert::pictures::side::MeshPart<'_>> = m
        .parts
        .iter()
        .map(|p| bc2_convert::pictures::side::MeshPart {
            name: &p.name,
            positions: &p.positions,
            tris: &p.indices,
        })
        .collect();
    bc2_convert::pictures::side::silhouette(&parts, SIZE.0, SIZE.1, 8, skip, detail).map_err(OnsError::Data)
}

/// What stage 14 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder: `work\console_icons\` written, the mesh index read.
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The level (lower case).
    pub lv: String,
    /// Per bundle (`ingame-00`, `loader-00`): the file to build from (the ledger's source).
    pub sources: Vec<String>,
    /// `console_common.cli_command('aptcheck')`.
    pub aptcheck: Vec<String>,
    /// `--apply` (the bundles packed) or `--plan`.
    pub write: bool,
}

fn icons_of(lv: &str) -> Option<&'static [Icon]> {
    ICONS.iter().find(|(l, _)| *l == lv).map(|(_, i)| *i)
}

/// `[round(u, 4) for u in uv]`'s repr.
fn box_repr(uv: &[f64; 4]) -> String {
    let v: Vec<String> = uv
        .iter()
        .map(|u| bc2_pyrt::fmt::float_repr(bc2_pyrt::fmt::round_to(*u, 4)))
        .collect();
    format!("[{}]", v.join(", "))
}

/// `build(lv, name, vehicle_names)`: the bundle unpacked into `work\console_icons\<lv>\<name>`, its weaponicon given
/// the level's pictures, checked; the unpacked folder.
#[allow(clippy::too_many_lines, reason = "the Python's build, step by step")]
fn build_one(
    a: &Args,
    k: usize,
    vehicle_names: &BTreeSet<String>,
    ix: &mut Option<bc2_convert::meshview::Index>,
    out: &mut StageOut,
) -> R<String> {
    let lv = a.lv.as_str();
    let name = BUNDLES[k];
    let work = joins(&a.root, &["work", "console_icons"]);
    let src = joins(&work, &[lv, name]);
    rmtree(&src)?;
    let source = a
        .sources
        .get(k)
        .ok_or_else(|| OnsError::Data("a source bundle per bundle".into()))?;
    fbrb(&["unpack", source, &src])?;
    let paths: Vec<String> = [MOVIE, ATLAS, META].iter().map(|r| find(&src, r)).collect::<R<_>>()?;
    let mut m = graft::load(&paths[0]).map_err(|e| menu(&e))?;
    let mut atlas = read(&paths[1])?;
    let mut meta = read(&paths[2])?;
    let mut lines: Vec<String> = Vec::new();
    let mut made: Vec<(&str, i64, Pic)> = Vec::new();
    for (label, mesh, skip, detail, seats) in icons_of(lv).unwrap_or(&[]) {
        if labelled(&mut m, label, WEAPON_ICON)?.is_some() {
            lines.push(format!("{lv} {name}: {label} has its WeaponIcon frame already - left"));
            continue;
        }
        if ix.is_none() {
            *ix = Some(
                bc2_convert::meshview::Index::load(&a.root, &mut |s| eprintln!("{s}"))
                    .map_err(|e| OnsError::Data(e.to_string()))?,
            );
        }
        let Some(index) = ix.as_ref() else {
            unreachable!("loaded above")
        };
        let pic = picture(index, mesh, skip, detail)?;
        let (image, frames, aliases) = add_icon(&mut m, label, *seats)?;
        let tmp = join(&work, &format!("{lv}\\{name}.meta"));
        std::fs::create_dir_all(&tmp).map_err(|e| super::io_err(&tmp, &e))?;
        let meta_src = join(&tmp, "meta.dbx");
        write(&meta_src, &meta)?;
        let meta_x = bc2_formats::dbx::to_xml(&meta).map_err(|e| OnsError::Data(e.to_string()))?;
        let (placed, uv) = atlas_place(&atlas, &meta_x, &pic)?;
        atlas = placed;
        meta = meta_with(
            &meta_src,
            image,
            uv,
            &tmp,
            &format!("onslaught-icon-{}", label.to_lowercase()),
        )?;
        rmtree(&tmp)?;
        lines.push(format!(
            "{lv} {name}: {label} from {}: image {image}, WeaponIcon frame {frames}, {aliases} label aliases, atlas \
             box {}",
            mesh.rsplit('/').next().unwrap_or(mesh),
            box_repr(&uv)
        ));
        let prev_dir = join(&work, "preview");
        std::fs::create_dir_all(&prev_dir).map_err(|e| super::io_err(&prev_dir, &e))?;
        let prev = join(&prev_dir, &format!("{label}.png"));
        bc2_menu::pil::png_write(
            &prev,
            &bc2_menu::pil::Rgba {
                w: pic.w,
                h: pic.h,
                px: pic.px.clone(),
            },
        )
        .map_err(|e| menu(&e))?;
        if frames != 1 {
            // the Python raised before printing the bundle's lines
            return Err(OnsError::Exit(format!(
                "{lv} {name}: {label}: {frames} WeaponIcon frames added (one expected)"
            )));
        }
        made.push((label, image, pic));
    }
    if !made.is_empty() {
        write(&paths[0], &m.to_res(m.big, GRAFT_LAYOUT).map_err(|e| menu(&e))?)?;
        write(&paths[1], &atlas)?;
        write(&paths[2], &meta)?;
    }
    // the checks: aptcheck, every Name labelled, the new frames single pictures, the atlas reads back
    let (prog, rest) = a
        .aptcheck
        .split_first()
        .ok_or_else(|| OnsError::Data("no aptcheck command".into()))?;
    let r = std::process::Command::new(prog)
        .args(rest)
        .args(["check", &paths[0]])
        .output()
        .map_err(|e| OnsError::Io(format!("{prog}: {e}")))?;
    let ok = r.status.success();
    let mut m = graft::load(&paths[0]).map_err(|e| menu(&e))?;
    let lack = missing(&mut m, vehicle_names)?;
    let mut bad = Vec::new();
    let mut wrong = Vec::new();
    for (label, image, pic) in &made {
        if !single_quad(&mut m, label)? {
            bad.push(*label);
        }
        if !readback(&atlas, &meta, *image, pic)? {
            wrong.push(*label);
        }
    }
    let list = |v: &[&str]| {
        let s: Vec<String> = v.iter().map(|x| bc2_pyrt::fmt::str_repr(x)).collect();
        format!("[{}]", s.join(", "))
    };
    let mut line = format!(
        "{lv} {name}: aptcheck {}; {} vehicle Names, {} without a WeaponIcon label",
        if ok { "ok" } else { "FAILED" },
        vehicle_names.len(),
        lack.len()
    );
    if !lack.is_empty() {
        let _ = write!(line, " ({})", lack.join(", "));
    }
    if !bad.is_empty() {
        let _ = write!(line, "; not one picture: {}", list(&bad));
    }
    if !wrong.is_empty() {
        let _ = write!(line, "; atlas read-back differs: {}", list(&wrong));
    }
    lines.push(line);
    if !ok || !lack.is_empty() || !bad.is_empty() || !wrong.is_empty() {
        let stdout = String::from_utf8_lossy(&r.stdout).replace("\r\n", "\n");
        let chars: Vec<char> = stdout.chars().collect();
        let tail: String = chars[chars.len().saturating_sub(400)..].iter().collect();
        lines.push(tail);
        return Err(OnsError::Exit(lines.join("\n")));
    }
    let man = join(&src, "fbrb.manifest");
    let mut text = manifest_lines(&man)?;
    for p in &paths {
        let rel = super::rel(p, &src).replace('\\', "/").to_lowercase();
        let sz = size(p)?;
        for l in &mut text {
            let parts: Vec<&str> = l.split('|').collect();
            if parts.len() == 5 && parts[0].to_lowercase() == rel {
                *l = format!("{}|{}|{}|{sz}|{sz}", parts[0], parts[1], parts[2]);
            }
        }
    }
    write_manifest(&man, &text)?;
    out.printed.extend(lines);
    Ok(src)
}

/// `apply(lv, write)` after the ledger's check: each bundle built (its lines printed), packed when `write`; the
/// writes for the ledger's one commit.
pub fn build(a: &Args, out: &mut StageOut) -> R<()> {
    let lv = a.lv.as_str();
    if icons_of(lv).is_none() {
        out.printed.push(format!("{lv}: no pictures to add"));
        return Ok(());
    }
    let vehicle_names = names(&a.game, lv)?;
    let mut ix = None;
    for (k, name) in BUNDLES.iter().enumerate() {
        let src = build_one(a, k, &vehicle_names, &mut ix, out)?;
        if !a.write {
            continue;
        }
        let built = joins(&a.root, &["work", "console_icons", lv, &format!("{name}.fbrb")]);
        super::pack(&src, &built)?;
        out.writes
            .push((bundle_path(&a.game, lv, name), super::Source::File(built)));
    }
    for (bundle, _) in &out.writes {
        out.after.push(format!(
            "  installed {} (the bundle before it is kept by the install ledger)",
            super::napisal::rel(bundle, &a.root)
        ));
    }
    Ok(())
}

/// `staged_movies()`: {bundle (lower case): movie file} of the movies the next installs put in place - this stage's
/// (`work\console_icons\<lv>\ingame-00`, once `--plan` built it) and the WW2 icon movie (`work\ww2\weapon_icons\ingame`:
/// `ww2_common`'s `ingame-00`).
#[must_use]
pub fn staged_movies(root: &str, game: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (lv, _) in ICONS {
        let f = super::port::under(&joins(root, &["work", "console_icons", lv, "ingame-00"]), MOVIE);
        if is_file(&f) {
            out.insert(bundle_path(game, lv, "ingame-00").to_lowercase(), f);
        }
    }
    let ww2 = super::port::under(&joins(root, &["work", "ww2", "weapon_icons", "ingame"]), MOVIE);
    if is_file(&ww2) {
        out.insert(bundle_path(game, "ww2_common", "ingame-00").to_lowercase(), ww2);
    }
    out
}

/// One level's coverage: (level, the movie read or `None`, its vehicle Names, the Names without a WeaponIcon label).
pub type Coverage = (String, Option<String>, BTreeSet<String>, Vec<String>);

/// `coverage(staged)`: every level `package.mft` registers (but `_common` and `opfor_` levels).
pub fn coverage(game: &str, staged: Option<&BTreeMap<String, String>>) -> R<Vec<Coverage>> {
    let mft = mft_path(game);
    let ch = chains(game, &mft)?;
    let mut srcs = chain_order(&mft)?;
    srcs.sort();
    let mut out = Vec::new();
    for src in srcs {
        let Some(m) = LEVEL00.captures(&src) else { continue };
        let lv = m[1].to_owned();
        if lv.contains("_common") || lv.contains("opfor_") {
            continue;
        }
        let chain = ch.get(&src).cloned().unwrap_or_default();
        let own = own_of(&chain, &lv);
        let movie = level_movie(
            ch.get(&format!("levels/{lv}/ingame-00"))
                .map_or(&[][..], |v| v.as_slice()),
        )?;
        let vn = level_vehicles(&chain, &own)?;
        let Some(movie) = movie else {
            let lack: Vec<String> = vn.iter().cloned().collect();
            out.push((lv, None, vn, lack));
            continue;
        };
        let f = staged.and_then(|s| s.get(&movie.to_lowercase())).cloned();
        let mut mv = match &f {
            Some(f) => graft::load(f),
            None => graft::load(&format!("{movie}:weaponicon")),
        }
        .map_err(|e| menu(&e))?;
        let lack = missing(&mut mv, &vn)?;
        out.push((lv, Some(f.unwrap_or(movie)), vn, lack));
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn references_name_the_partition() {
        let data = b"\x00objects/vehicles/sea/markv/markv_ons/46bb5d9b-0000-4000-8000-000000000000\x00\
objects/vehicles/static/f18/f18/12345678-1234-4000-8000-000000000000\x00\
objects/weapons/handheld/m16/m16/12345678-1234-4000-8000-000000000000\x00";
        assert_eq!(
            vehicle_refs(data),
            [
                "objects/vehicles/sea/markv/markv_ons",
                "objects/vehicles/static/f18/f18"
            ]
        );
        assert_eq!(
            box_repr(&[0.0, 0.152_343_75, 0.871_093_75, 0.906_25]),
            "[0.0, 0.1523, 0.8711, 0.9062]"
        );
    }

    #[test]
    fn chains_in_order() {
        let d = std::env::temp_dir().join(format!("icons-chains-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let mft = d.join("package.mft");
        std::fs::write(
            &mft,
            b"BundleSource game:levels/nam_mp_002cq/ingame-00.fbrb\r\nBundleTarget package:levels/nam_mp_002cq/ingame-00.fbrb\r\n\
BundleTarget game:levels/mp_common/ingame-00.fbrb\r\n\r\nBundleSource game:levels/ons_mp_002/level-00.fbrb\r\n\
BundleTarget package:levels/ons_mp_002/level-00.fbrb\r\n",
        )
        .unwrap();
        let ch = chains(r"C:\g", &mft.to_string_lossy()).unwrap();
        assert_eq!(
            ch["levels/nam_mp_002cq/ingame-00"],
            [
                r"C:\g\Package\levels\nam_mp_002cq\ingame-00.fbrb",
                r"C:\g\Dist\win32\levels\mp_common\ingame-00.fbrb"
            ]
        );
        assert_eq!(chain_order(&mft.to_string_lossy()).unwrap().len(), 2);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
