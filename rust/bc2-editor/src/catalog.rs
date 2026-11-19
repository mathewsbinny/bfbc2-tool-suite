//! What the start screen shows of each set of maps and each level, read from the game copy at run time (round G3,
//! the user's "Choose mode" screen): a picture per set, a thumbnail and the game mode per level. No game art is in the
//! repository: it is read from the game copy's bundles and kept, decoded, in Project OPFOR BFBC2's
//! `work\editor\art` ([`cache_dir`]), made again when the bundles change.
//!
//! **The pictures** are the game's own loading screens, `awards/images/loadingscreens/<name>.res` (the `ITexture`s the
//! loading screen shows, in `ondemand_awards-00.fbrb`; `game\Package\async` first, then `game\Dist\win32\async`):
//! - BFBC2: `mp_005` (DICE's, Atacama Desert);
//! - BFBC1: `bc1_oasis` (DICE's, Oasis);
//! - Vietnam: `nam_mp_002` (DICE's, Hill 137 / Vantage Point's loading screen);
//! - WW2 / Pacific War: `wake_island_s` (OPFOR's, converted from Battlefield 1943's Wake Island by its WW2 install).
//!
//! A set whose picture is not there (Pacific War not installed, a damaged bundle) gets the design's plain tile.
//! **The thumbnails** are the level pictures of the map lists, `awards/images/levels/<level>.res` (same bundles);
//! **the game modes** the level descriptions' `GameMode` (`Levels/<LEVEL>_description.dbx` in `startup-00.fbrb`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use bc2_convert::texcheck;
use bc2_formats::{dbx, fbrb};
use serde::{Deserialize, Serialize};

use crate::levels::ContentSet;

/// The cache's record format.
pub const FORMAT: u32 = 3;
/// The record in [`cache_dir`].
pub const RECORD: &str = "art.json";

/// Each set's picture: its loading screen's name (`awards/images/loadingscreens/<name>.res`).
pub const CARD_ART: [(ContentSet, &str); 4] = [
    (ContentSet::Bfbc1, "bc1_oasis"),
    (ContentSet::Bfbc2, "mp_005"),
    (ContentSet::Vietnam, "nam_mp_002"),
    (ContentSet::Ww2, "wake_island_s"),
];

/// The widest picture kept: a card's (drawn up to about 560 points wide in the 2 x 2 layout).
const CARD_WIDTH: u32 = 640;
/// The widest thumbnail kept.
const THUMB_WIDTH: u32 = 128;

/// Where the decoded pictures are kept: `work\editor\art`.
#[must_use]
pub fn cache_dir(opfor: &Path) -> PathBuf {
    opfor.join("work").join("editor").join("art")
}

/// A decoded picture: its size and pixels (RGBA, row by row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Width and height.
    pub size: [usize; 2],
    /// RGBA bytes.
    pub rgba: Vec<u8>,
}

/// What the start screen shows from the game copy.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Each set's picture, where its loading screen is in the game copy.
    pub cards: HashMap<ContentSet, Picture>,
    /// Each level's thumbnail, where it has one.
    pub thumbs: HashMap<String, Picture>,
    /// Each level's game mode in words (`Conquest`, `Rush` ...), where its description names one.
    pub modes: HashMap<String, String>,
    /// Read from the bundles now (false: from the cache).
    pub fresh: bool,
}

/// A bundle's size and time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    path: String,
    size: u64,
    modified_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Record {
    format: u32,
    bundles: Vec<Stamp>,
    /// The levels asked for (a level listed later is read then).
    levels: Vec<String>,
    cards: BTreeMap<String, String>,
    thumbs: BTreeMap<String, String>,
    modes: BTreeMap<String, String>,
}

/// The bundles read, in order: the pictures (`ondemand_awards`) and the descriptions (`startup`), Package first.
fn bundles(opfor: &Path) -> Vec<PathBuf> {
    let game = opfor.join("game");
    let pk = game.join("Package").join("async");
    let dist = game.join("Dist").join("win32").join("async");
    vec![
        pk.join("ondemand_awards-00.fbrb"),
        dist.join("ondemand_awards-00.fbrb"),
        pk.join("startup-00.fbrb"),
        dist.join("startup-00.fbrb"),
    ]
}

fn stamp(p: &Path) -> Option<Stamp> {
    let m = fs::metadata(p).ok()?;
    let ns = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some(Stamp {
        path: p.to_string_lossy().into_owned(),
        size: m.len(),
        modified_ns: u64::try_from(ns).unwrap_or(u64::MAX),
    })
}

/// A game mode's name in words from a `GameMode` reference (`gamemodes/namconquest/<guid>`).
#[must_use]
pub fn mode_word(reference: &str) -> String {
    let name = reference.split('/').nth(1).unwrap_or("").to_lowercase();
    let name = name.strip_prefix("nam").unwrap_or(&name);
    match name {
        "conquest" => "Conquest".into(),
        "goldrush" => "Rush".into(),
        "squadrush" => "Squad Rush".into(),
        "teamdeathmatch" => "Squad Deathmatch".into(),
        "onslaught" => "Onslaught".into(),
        "airsuperiority" => "Air Superiority".into(),
        n if n.starts_with("sing") => "Campaign".into(),
        "" => String::new(),
        n => n.to_owned(),
    }
}

/// A PC `ITexture` decoded at its first level no wider than `max_w` (the smallest when all are wider), or `None`
/// when it is not one the reader decodes.
#[must_use]
pub fn decode_texture(data: &[u8], max_w: u32) -> Option<Picture> {
    // a bundle's `.res` holds the resource header first ("RES\x02", the type name, two sizes: 64 bytes)
    let data = if data.starts_with(b"RES") {
        data.get(64..)?
    } else {
        data
    };
    let p = decode_level(data, max_w)?;
    Some(shrink(p, max_w as usize))
}

/// `p` without the white margin at its right and bottom: the level pictures sit in the top left of a larger texture
/// (a 512 x 256 texture holds a picture about 300 x 170), the rest white.
#[must_use]
pub fn trim_white(p: Picture) -> Picture {
    let [w, h] = p.size;
    let white = |x: usize, y: usize| {
        p.rgba
            .get((y * w + x) * 4..(y * w + x) * 4 + 3)
            .is_none_or(|c| c.iter().all(|&v| v >= 245))
    };
    let used_w = (0..w)
        .rev()
        .find(|&x| (0..h).any(|y| !white(x, y)))
        .map_or(w, |x| x + 1);
    let used_h = (0..h)
        .rev()
        .find(|&y| (0..used_w).any(|x| !white(x, y)))
        .map_or(h, |y| y + 1);
    if (used_w, used_h) == (w, h) || used_w < 8 || used_h < 8 {
        return p;
    }
    let mut rgba = Vec::with_capacity(used_w * used_h * 4);
    for y in 0..used_h {
        rgba.extend_from_slice(p.rgba.get(y * w * 4..(y * w + used_w) * 4).unwrap_or_default());
    }
    Picture {
        size: [used_w, used_h],
        rgba,
    }
}

/// `p` halved (2 x 2 averages) until it is no wider than `max_w`.
fn shrink(mut p: Picture, max_w: usize) -> Picture {
    while p.size[0] > max_w && p.size[0] >= 2 && p.size[1] >= 2 {
        let (w, h) = (p.size[0] / 2, p.size[1] / 2);
        let src =
            |x: usize, y: usize, c: usize| u32::from(p.rgba.get((y * p.size[0] + x) * 4 + c).copied().unwrap_or(0));
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let s = src(2 * x, 2 * y, c)
                        + src(2 * x + 1, 2 * y, c)
                        + src(2 * x, 2 * y + 1, c)
                        + src(2 * x + 1, 2 * y + 1, c);
                    out.push((s / 4) as u8);
                }
            }
        }
        p = Picture {
            size: [w, h],
            rgba: out,
        };
    }
    p
}

fn decode_level(data: &[u8], max_w: u32) -> Option<Picture> {
    let levels = texcheck::levels(data)?;
    let fmt = u32::from_le_bytes(data.get(8..12)?.try_into().ok()?);
    let first: Vec<&texcheck::Level> = levels.iter().filter(|l| l.0 == 0).collect();
    let pick = first.iter().find(|l| l.2 <= max_w).or_else(|| first.last())?;
    let bytes = data.get(pick.4.clone())?;
    let (h, w, px) = texcheck::decode(bytes, i64::from(pick.2), i64::from(pick.3), fmt)?;
    let rgba = px
        .iter()
        .flat_map(|c| {
            [
                c[0].clamp(0, 255) as u8,
                c[1].clamp(0, 255) as u8,
                c[2].clamp(0, 255) as u8,
                255,
            ]
        })
        .collect();
    Some(Picture { size: [w, h], rgba })
}

fn write_png(path: &Path, p: &Picture) -> std::io::Result<()> {
    let f = BufWriter::new(File::create(path)?);
    let mut e = png::Encoder::new(f, p.size[0] as u32, p.size[1] as u32);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(&p.rgba).map_err(std::io::Error::other)
}

fn read_png(path: &Path) -> Option<Picture> {
    let d = png::Decoder::new(std::io::BufReader::new(File::open(path).ok()?));
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some(Picture {
        size: [info.width as usize, info.height as usize],
        rgba: buf,
    })
}

fn from_cache(dir: &Path, r: &Record) -> Option<Catalog> {
    let mut c = Catalog::default();
    for (set, file) in &r.cards {
        let s = ContentSet::ALL.into_iter().find(|x| x.word() == set)?;
        c.cards.insert(s, read_png(&dir.join(file))?);
    }
    for (lv, file) in &r.thumbs {
        c.thumbs.insert(lv.clone(), read_png(&dir.join(file))?);
    }
    c.modes = r.modes.clone().into_iter().collect();
    Some(c)
}

/// One bundle's header and its live entries by lower-case name.
struct Bundle {
    path: PathBuf,
    header: fbrb::Header,
    names: HashMap<String, usize>,
}

fn open(path: &Path) -> Option<Bundle> {
    let header = fbrb::read_header(&mut File::open(path).ok()?).ok()?;
    let names = header
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_deleted())
        .map(|(k, e)| (e.name.to_ascii_lowercase(), k))
        .collect();
    Some(Bundle {
        path: path.to_owned(),
        header,
        names,
    })
}

/// The data of `wanted` names, each from the first of `bundles` that has it, every bundle read once (at once).
fn read_first(bundles: &[Bundle], wanted: &[String]) -> HashMap<String, Vec<u8>> {
    let mut per: Vec<Vec<(String, usize)>> = vec![Vec::new(); bundles.len()];
    for n in wanted {
        if let Some((b, &e)) = bundles
            .iter()
            .enumerate()
            .find_map(|(b, x)| x.names.get(n).map(|e| (b, e)))
            && let Some(list) = per.get_mut(b)
        {
            list.push((n.clone(), e));
        }
    }
    // the payload is streamed once: the entries in their order in it
    for (list, b) in per.iter_mut().zip(bundles) {
        list.sort_by_key(|(_, e)| b.header.entries.get(*e).map_or(0, |x| x.offset));
    }
    std::thread::scope(|s| {
        let handles: Vec<_> = bundles
            .iter()
            .zip(&per)
            .filter(|(_, l)| !l.is_empty())
            .map(|(b, list)| {
                s.spawn(move || {
                    let entries: Vec<&fbrb::Entry> =
                        list.iter().filter_map(|(_, e)| b.header.entries.get(*e)).collect();
                    let data = File::open(&b.path)
                        .ok()
                        .map(std::io::BufReader::new)
                        .and_then(|mut f| fbrb::read_payload(&mut f, &b.header, &entries).ok())
                        .unwrap_or_default();
                    list.iter().map(|(n, _)| n.clone()).zip(data).collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).flatten().collect()
    })
}

/// The start screen's pictures, thumbnails and modes for `levels` (lower-case names): from the cache when it was made
/// from the bundles as they are now and asked for these levels, else read from the bundles and kept in the cache (a
/// cache that cannot be written is only slower next time). Missing bundles or entries give nothing for them.
#[must_use]
pub fn load(opfor: &Path, levels: &[String]) -> Catalog {
    let dir = cache_dir(opfor);
    let paths = bundles(opfor);
    let stamps: Vec<Stamp> = paths.iter().filter_map(|p| stamp(p)).collect();
    let mut asked: Vec<String> = levels.iter().map(|l| l.to_lowercase()).collect();
    asked.sort();
    asked.dedup();
    if let Some(r) = fs::read(dir.join(RECORD))
        .ok()
        .and_then(|d| serde_json::from_slice::<Record>(&d).ok())
        && r.format == FORMAT
        && r.bundles == stamps
        && asked.iter().all(|l| r.levels.contains(l))
        && let Some(c) = from_cache(&dir, &r)
    {
        return c;
    }
    let open_all = |ps: &[PathBuf]| -> Vec<Bundle> { ps.iter().filter_map(|p| open(p)).collect() };
    let awards = open_all(paths.get(..2).unwrap_or_default());
    let startup = open_all(paths.get(2..).unwrap_or_default());
    let card_name = |n: &str| format!("awards/images/loadingscreens/{n}.res");
    let thumb_name = |lv: &str| format!("awards/images/levels/{lv}.res");
    let mut wanted: Vec<String> = CARD_ART.iter().map(|(_, n)| card_name(n)).collect();
    wanted.extend(asked.iter().map(|l| thumb_name(l)));
    let pics = read_first(&awards, &wanted);
    let descs: Vec<String> = asked.iter().map(|l| format!("levels/{l}_description.dbx")).collect();
    let texts = read_first(&startup, &descs);
    let mut c = Catalog {
        fresh: true,
        ..Catalog::default()
    };
    for (set, n) in CARD_ART {
        if let Some(p) = pics.get(&card_name(n)).and_then(|d| decode_texture(d, CARD_WIDTH)) {
            c.cards.insert(set, p);
        }
    }
    for l in &asked {
        // the picture in the texture's top left, its white margin cut, then made thumbnail-sized
        if let Some(p) = pics.get(&thumb_name(l)).and_then(|d| decode_texture(d, 1024)) {
            c.thumbs.insert(l.clone(), shrink(trim_white(p), THUMB_WIDTH as usize));
        }
        let mode = texts
            .get(&format!("levels/{l}_description.dbx"))
            .and_then(|d| dbx::parse_tree(d).ok().and_then(|t| game_mode(&t)))
            .map(|r| mode_word(&r))
            .filter(|m| !m.is_empty());
        if let Some(m) = mode {
            c.modes.insert(l.clone(), m);
        }
    }
    let _ = store(&dir, &c, stamps, asked);
    c
}

fn game_mode(t: &dbx::DbxTree<'_>) -> Option<String> {
    // the description's GameMode field, wherever it is nested (LevelDescription's)
    (0..t.len()).filter_map(|k| t.node(k)).find_map(|n| {
        (n.attr("name") == Some("GameMode"))
            .then(|| n.attr("ref"))
            .flatten()
            .filter(|r| r.contains('/'))
            .map(str::to_owned)
    })
}

fn store(dir: &Path, c: &Catalog, bundles: Vec<Stamp>, levels: Vec<String>) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut keep: HashSet<String> = HashSet::new();
    let mut cards = BTreeMap::new();
    for (set, p) in &c.cards {
        let file = format!(
            "card_{}.png",
            set.badge()
                .to_lowercase()
                .replace(|ch: char| !ch.is_ascii_alphanumeric(), "_")
        );
        write_png(&dir.join(&file), p)?;
        keep.insert(file.clone());
        cards.insert(set.word().to_owned(), file);
    }
    let mut thumbs = BTreeMap::new();
    for (lv, p) in &c.thumbs {
        let file = format!("thumb_{lv}.png");
        write_png(&dir.join(&file), p)?;
        keep.insert(file.clone());
        thumbs.insert(lv.clone(), file);
    }
    // pictures of an earlier record no longer named go
    for e in fs::read_dir(dir)?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let png = Path::new(&n).extension().is_some_and(|x| x.eq_ignore_ascii_case("png"));
        if png && !keep.contains(&n) {
            let _ = fs::remove_file(e.path());
        }
    }
    let r = Record {
        format: FORMAT,
        bundles,
        levels,
        cards,
        thumbs,
        modes: c.modes.clone().into_iter().collect(),
    };
    let tmp = dir.join(format!("{RECORD}.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(&r).map_err(std::io::Error::other)?)?;
    fs::rename(tmp, dir.join(RECORD))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn modes_in_words() {
        assert_eq!(mode_word("gamemodes/namconquest/1-2"), "Conquest");
        assert_eq!(mode_word("gamemodes/goldrush/x"), "Rush");
        assert_eq!(mode_word("gamemodes/namsquadrush/x"), "Squad Rush");
        assert_eq!(mode_word("gamemodes/teamdeathmatch/x"), "Squad Deathmatch");
        assert_eq!(mode_word("gamemodes/singelplayerfirstlevel/x"), "Campaign");
        assert_eq!(mode_word("gamemodes/airsuperiority/x"), "Air Superiority");
        assert_eq!(mode_word("null"), "");
    }

    #[test]
    fn a_texture_decoded() {
        // a DXT1 texture 8 x 4 with one more level 4 x 2: the first level no wider than 4 is the second
        let mut pc = vec![0u8; texcheck::HEADER];
        for (k, v) in [(2, 0u32), (4, 8), (5, 4), (7, 2), (8, 16), (9, 8)] {
            pc[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
        }
        pc.extend([0xFF, 0xFF, 0, 0, 0, 0, 0, 0].repeat(3));
        let p = decode_texture(&pc, 4).unwrap();
        assert_eq!(p.size, [4, 2]);
        assert_eq!(&p.rgba[..4], &[255, 255, 255, 255]);
        assert_eq!(decode_texture(&pc, 8).unwrap().size, [8, 4]);
        assert!(decode_texture(b"short", 8).is_none());
        // a resource header first: the same texture
        let mut res = b"RES\x02ITexture".to_vec();
        res.resize(64, 0);
        res.extend(&pc);
        assert_eq!(decode_texture(&res, 8).unwrap().size, [8, 4]);
    }

    #[test]
    fn white_margins_cut_and_halving() {
        // 16 x 16 white with a 10 x 9 dark picture in the top left
        let mut rgba = vec![255u8; 16 * 16 * 4];
        for y in 0..9 {
            for x in 0..10 {
                rgba[(y * 16 + x) * 4..(y * 16 + x) * 4 + 3].copy_from_slice(&[10, 20, 30]);
            }
        }
        let t = trim_white(Picture { size: [16, 16], rgba });
        assert_eq!(t.size, [10, 9]);
        assert_eq!(&t.rgba[..4], &[10, 20, 30, 255]);
        assert_eq!(shrink(t, 5).size, [5, 4]);
    }
}
