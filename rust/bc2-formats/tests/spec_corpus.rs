//! The specification's corpus rules (`spec/formats/*.md`, `spec/model/types.md`, `spec/model/references.md`) checked
//! on DICE's bundles in Project OPFOR BFBC2's game copy (run by `run_tests.py game`; ignored by a plain `cargo test`):
//!
//!   cargo test --release -p bc2-formats --test spec_corpus -- --ignored --nocapture
//!
//! The corpus is DICE's: every `.fbrb` under the game copy dated before 2013 (OPFOR's installs and the bundles it
//! changed are newer), read once in one pass that every test shares. The level corpus is every `.dbx` copy in those
//! bundles under `levels/` (Dist and Package). Each test names the rules it proves (`// spec:`); the counts are those
//! of 2026-11-22, the rules' `[data: ...]` lines. The game copy is `OPFOR_ROOT`'s `game`, else
//! `project-opfor-bfbc2\game` beside the suite; a test passes with a note when it is missing.
#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::similar_names,
    clippy::iter_over_hash_type,
    // a corpus walk: short names for header words, flags per texture, an index walk over the tree
    clippy::many_single_char_names,
    clippy::struct_excessive_bools,
    clippy::format_collect,
    clippy::needless_range_loop,
    clippy::collapsible_if,
    missing_docs
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bc2_formats::{bigf, dbx, fbrb, meshdata};
use rayon::prelude::*;

/// The gzip member of no data as DICE's packer writes it (FBRB-DEL-2).
const EMPTY_MEMBER: [u8; 20] = [
    0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0x0B, 0x03, 0x00, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// 2013-01-01 00:00 UTC: DICE's last bundles are from 2012, OPFOR's from 2026.
const DICE_BEFORE: u64 = 1_356_998_400;

fn game() -> Option<PathBuf> {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    let g = root.join("game");
    g.join("Dist").join("win32").join("levels").is_dir().then_some(g)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fbrb")) {
            out.push(p);
        }
    }
}

fn dice(p: &Path) -> bool {
    p.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|d| d.as_secs() < DICE_BEFORE)
}

/// Counts by key, summed over bundles.
#[derive(Default)]
struct Tally(BTreeMap<String, u64>);

impl Tally {
    fn add(&mut self, key: impl Into<String>, n: u64) {
        *self.0.entry(key.into()).or_default() += n;
    }
    fn inc(&mut self, key: impl Into<String>) {
        self.add(key, 1);
    }
    fn merge(&mut self, o: Self) {
        for (k, v) in o.0 {
            self.add(k, v);
        }
    }
    fn get(&self, key: &str) -> u64 {
        self.0.get(key).copied().unwrap_or(0)
    }
    /// Every key under `prefix` (the prefix cut off) with its count.
    fn under(&self, prefix: &str) -> BTreeMap<String, u64> {
        self.0
            .iter()
            .filter_map(|(k, v)| k.strip_prefix(prefix).map(|s| (s.to_owned(), *v)))
            .collect()
    }
}

/// A texture's header facts (TEX-*), for the distinct-name counts.
#[derive(Clone)]
struct Tex {
    wrapped: bool,
    wrap_lengths_ok: bool,
    awards: bool,
    format: u32,
    flags: u32,
    levels_ok: bool,
    length_ok: bool,
}

/// A mesh set LOD against its MeshData (MESH-HDR-2, MSET-LOD-2, MSET-LOD-4).
#[derive(Clone)]
struct SetLod {
    kind: &'static str,
    h0: Option<u32>,
    size_ok: Option<bool>,
    shaders_ok: Option<bool>,
}

#[derive(Clone)]
struct Mesh {
    h0: u32,
    wide0_even: bool,
    parts: u64,
    fb_ok: u64,
}

/// One partition of the level corpus: its name, partition GUID and instance GUIDs (INST-GUID-3).
struct Part {
    name: String,
    guid: u128,
    instances: Vec<u128>,
}

#[derive(Default)]
struct Bundle {
    t: Tally,
    textures: Vec<(String, Tex)>,
    meshes: Vec<(String, Mesh)>,
    set_lods: Vec<(String, Vec<SetLod>)>,
    set_parts: Vec<(String, u64, u64)>,
    parts: Vec<Part>,
}

struct Corpus {
    t: Tally,
    /// distinct-name counts (the first copy of each lower-case name in sorted bundle order)
    d: Tally,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

fn be32(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(d[at..at + 4].try_into().unwrap())
}

fn guid128(s: &str) -> Option<u128> {
    let h: String = s.chars().filter(|c| *c != '-').collect();
    (h.len() == 32).then(|| u128::from_str_radix(&h, 16).ok()).flatten()
}

/// `8-4-4-4-12` hexadecimal digits with hyphens.
fn is_guid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn upper(s: &str) -> bool {
    !s.bytes().any(|c| c.is_ascii_lowercase())
}

fn lower(s: &str) -> bool {
    !s.bytes().any(|c| c.is_ascii_uppercase())
}

/// djb2 with XOR over the lower-case last `|` segment (MSET-PART-1).
fn part_hash(name: &[u8]) -> u32 {
    let seg = name.rsplit(|c| *c == b'|').next().unwrap_or(name);
    seg.iter()
        .fold(5381u32, |h, c| h.wrapping_mul(33) ^ u32::from(c.to_ascii_lowercase()))
}

/// The expected byte length of a PC texture level (TEX-FMT-2), `None` for a format the PC does not use.
fn level_bytes(format: u32, w: u32, h: u32) -> Option<u64> {
    let (w, h) = (u64::from(w.max(1)), u64::from(h.max(1)));
    let blocks = w.div_ceil(4) * h.div_ceil(4);
    Some(match format {
        0 | 18 => blocks * 8,
        1 | 2 | 19 | 20 => blocks * 16,
        9 => w * h * 4,
        13 => w * h * 8,
        10 => w.div_ceil(4) * 4 * h,
        _ => return None,
    })
}

fn texture(d: &[u8]) -> Option<Tex> {
    let (wrapped, body) = if d.starts_with(b"RES") {
        (true, d.get(64..)?)
    } else {
        (false, d)
    };
    let wrap_lengths_ok = !wrapped || (le32(d, 0x38) as usize == body.len() && le32(d, 0x3C) as usize == body.len());
    if body.len() < 92 {
        return None;
    }
    let (kind, format, flags, w, h, m) = (
        le32(body, 4),
        le32(body, 8),
        le32(body, 12),
        le32(body, 16),
        le32(body, 20),
        le32(body, 28),
    );
    let mut levels_ok = (1..=15).contains(&m);
    let mut sum = 0u64;
    for i in 0..15u32 {
        let s = u64::from(le32(body, 32 + 4 * i as usize));
        if i < m {
            levels_ok &= level_bytes(format, w >> i, h >> i) == Some(s);
            sum += s;
        } else {
            levels_ok &= s == 0;
        }
    }
    let faces = if kind == 1 { 6 } else { 1 };
    Some(Tex {
        wrapped,
        wrap_lengths_ok,
        awards: false,
        format,
        flags,
        levels_ok,
        length_ok: body.len() as u64 == 92 + faces * sum,
    })
}

/// DBX-HDR / STR / ELEM / NUM on one file, from its bytes (the raw walk also sees the container byte, TYPE-ELEM-5).
fn dbx_file(d: &[u8], t: &mut Tally) {
    t.inc("dbx:files");
    let (body, area, count) = (be32(d, 8), be32(d, 16), be32(d, 20));
    t.inc(if body == 24 + area {
        "dbx:hdr3 ok"
    } else {
        "dbx:hdr3 bad"
    });
    t.inc(if be32(d, 12) == 0 {
        "dbx:hdr4 zero"
    } else {
        "dbx:hdr4 other"
    });
    let base = 24 + 4 * count as usize;
    let offs: Vec<usize> = (0..count as usize).map(|i| be32(d, 24 + 4 * i) as usize).collect();
    let mut strings: Vec<&[u8]> = Vec::with_capacity(offs.len());
    let mut packed = true;
    let mut next = 0usize;
    for &o in &offs {
        let s = &d[base + o..];
        let len = s.iter().position(|c| *c == 0).unwrap_or(s.len());
        strings.push(&s[..len]);
        packed &= o == next;
        next = o + len + 1;
    }
    packed &= area as usize == 4 * count as usize + next;
    t.inc(if packed {
        "dbx:str3 packed"
    } else {
        "dbx:str3 not packed"
    });
    t.inc(if strings.first().is_some_and(|s| s.is_empty()) {
        "dbx:str2 empty"
    } else {
        "dbx:str2 other"
    });
    if strings.iter().collect::<HashSet<_>>().len() != strings.len() {
        t.inc("dbx:str3 a string twice");
    }
    // the raw walk: kinds, nesting, container bytes, number sizes, string use
    let mut used = vec![false; strings.len()];
    let mut first_use: Vec<usize> = Vec::new();
    let mut note = |id: u64, used: &mut Vec<bool>| {
        if let Some(u) = used.get_mut(id as usize) {
            if !*u {
                *u = true;
                first_use.push(id as usize);
            }
        }
    };
    let mut p = body as usize;
    let mut open = 0usize;
    let mut deepest = 0usize;
    let mut tags: Vec<u64> = Vec::new();
    while p < d.len() {
        let (tag, q) = bc2_formats::varint::read_u64(d, p).unwrap();
        p = q;
        if tag == 0 {
            open -= 1;
            tags.pop();
            continue;
        }
        note(tag, &mut used);
        let head = d[p];
        p += 1;
        let kind = head >> 4;
        t.inc(format!("dbx:kind {kind:X}"));
        for _ in 0..(head & 0xF) {
            let (k, q) = bc2_formats::varint::read_u64(d, p).unwrap();
            let (v, q) = bc2_formats::varint::read_u64(d, q).unwrap();
            note(k, &mut used);
            note(v, &mut used);
            p = q;
        }
        let tag_name = String::from_utf8_lossy(strings[tag as usize]).into_owned();
        match kind {
            0xA => {
                t.inc(format!("dbx:container byte {}", d[p]));
                p += 1;
                open += 1;
                deepest = deepest.max(open);
                tags.push(tag);
            }
            2 => {
                let (v, q) = bc2_formats::varint::read_u64(d, p).unwrap();
                note(v, &mut used);
                p = q;
            }
            7 => {
                let (n, q) = bc2_formats::varint::read_u64(d, p).unwrap();
                let (size, q) = bc2_formats::varint::read_u64(d, q).unwrap();
                if n > 0 {
                    t.inc(format!("dbx:num size {size} on {tag_name}"));
                }
                p = q + (n * size) as usize;
            }
            _ => p += 2,
        }
    }
    t.inc(format!("dbx:deepest {deepest}"));
    if used.iter().skip(1).any(|u| !*u) {
        t.inc("dbx:str3 an unused string");
    }
    if first_use.windows(2).all(|w| w[0] < w[1]) {
        t.inc("dbx:str3 first-use order");
    }
}

/// The model rules (TYPE-*, INST-*, REF-FORM-2) on one level-corpus partition, through the suite's reader.
fn model_file(name: &str, d: &[u8], t: &mut Tally, parts: &mut Vec<Part>) {
    let tree = dbx::parse_tree(d).expect("a DICE partition parses");
    let tag_name = |i: usize| tree.node(i).unwrap().tag();
    let mut parent: Vec<Option<usize>> = vec![None; tree.len()];
    for i in 0..tree.len() {
        for c in tree.node(i).unwrap().children() {
            parent[c.index()] = Some(i);
        }
    }
    for i in 0..tree.len() {
        let n = tree.node(i).unwrap();
        let tag = n.tag();
        t.inc(format!("type:tag {tag}"));
        t.inc(format!("type:tagkind {tag}:{:X}", n.kind()));
        if let Some(pi) = parent[i] {
            t.inc(format!("type:pair {}>{tag}", tag_name(pi)));
        }
        let attrs: Vec<&str> = n.attributes().map(|(k, _)| k).collect();
        t.inc(format!(
            "type:attrs {tag}:{}",
            if attrs.is_empty() {
                "(none)".to_owned()
            } else {
                attrs.join(",")
            }
        ));
        if let Some(r) = n.attr("ref") {
            let form = if r == "null" {
                "null"
            } else if is_guid(r) {
                if lower(r) { "local lower" } else { "local other" }
            } else if let Some((path, g)) = r.rsplit_once('/') {
                if is_guid(g) && lower(r) && !path.is_empty() {
                    "external lower"
                } else {
                    "external other"
                }
            } else {
                "other"
            };
            t.inc(format!("ref:form {form}"));
            if tag == "item" {
                t.inc(if n.kind() == 2 && n.text().is_empty() {
                    "type:item ref empty text"
                } else {
                    "type:item ref other"
                });
            }
        }
        match (tag, n.kind()) {
            ("item", 2) if n.attr("ref").is_none() => t.inc("type:item string"),
            ("item", k) if k != 2 => t.inc("type:item not text"),
            ("array", 2) => t.inc(if n.text().is_empty() {
                "type:array text empty"
            } else {
                "type:array text nonempty"
            }),
            ("array", 0xA) => t.inc(if n.children().next().is_some() {
                "type:array container children"
            } else {
                "type:array container empty"
            }),
            ("array", 7) => t.inc("type:array numbers"),
            ("instance", 2) => t.inc(if n.text().is_empty() {
                "type:instance text empty"
            } else {
                "type:instance text nonempty"
            }),
            ("field", 2) if n.attr("ref").is_none() => t.inc(if n.text().is_empty() {
                "type:text empty"
            } else {
                "type:text nonempty"
            }),
            _ => {}
        }
        if tag == "complex" && n.attributes().len() == 0 {
            t.inc(if parent[i].is_some_and(|p| tag_name(p) == "array") {
                "type:unnamed complex in array"
            } else {
                "type:unnamed complex elsewhere"
            });
        }
        let k = n.kind();
        if k != 0xA && k != 2 && k != 7 {
            let raw = n.raw();
            t.inc(format!("type:scalar {k:X} {}", hex(raw)));
        }
        if k == 7 {
            let raw = n.raw();
            let (cnt, q) = bc2_formats::varint::read_u64(raw, 0).unwrap();
            let (size, _) = bc2_formats::varint::read_u64(raw, q).unwrap();
            t.inc(format!("type:num size {size}"));
            if cnt == 0 {
                t.inc(format!("type:num empty size {size}"));
            } else if cnt == 1 && size != 4 {
                t.inc(format!("type:num 1x{size} on {}", n.attr("name").unwrap_or("")));
            }
            if tag == "complex" {
                t.inc(format!("type:complex num {cnt}x{size}"));
            }
        }
    }
    // partitions and instances
    let Some(p) = tree.partition() else { return };
    t.inc("inst:partitions");
    let pattrs: Vec<(&str, &str)> = p.attributes().collect();
    t.inc(format!("type:exportMode {}", p.attr("exportMode").unwrap_or("(none)")));
    let pg = p.attr("guid").unwrap_or("");
    t.inc(if is_guid(pg) && upper(pg) {
        "inst:partition guid upper"
    } else {
        "inst:partition guid other"
    });
    let prim = p.attr("primaryInstance").unwrap_or("");
    t.inc(if is_guid(prim) && upper(prim) {
        "inst:primary upper"
    } else {
        "inst:primary other"
    });
    let _ = pattrs;
    let stem = name.rsplit('/').next().unwrap_or(name);
    let stem = stem
        .strip_suffix(".dbx")
        .or_else(|| stem.strip_suffix(".DBX"))
        .unwrap_or(stem);
    let mut seen = HashSet::new();
    let mut guids = Vec::new();
    let mut dup = false;
    let mut found = None;
    let (mut ids, mut id_on_primary) = (0, 0);
    for (j, i) in tree.instances().enumerate() {
        let g = i.attr("guid").unwrap_or("");
        t.inc(if is_guid(g) && upper(g) {
            "inst:guid upper"
        } else {
            "inst:guid other"
        });
        dup |= !seen.insert(g.to_ascii_lowercase());
        guids.extend(guid128(g));
        let is_primary = g.eq_ignore_ascii_case(prim);
        if is_primary && found.is_none() {
            found = Some(j);
        }
        if let Some(id) = i.attr("id") {
            ids += 1;
            if is_primary {
                id_on_primary += 1;
            }
            t.inc(if id == stem {
                "inst:id stem"
            } else if id.eq_ignore_ascii_case(stem) {
                "inst:id stem ignoring case"
            } else {
                "inst:id other"
            });
        }
        let ty = i.attr("type").unwrap_or("");
        t.inc(if ty.matches('.').count() == 1 {
            "type:type one dot"
        } else {
            "type:type other"
        });
        if let Some((ns, _)) = ty.split_once('.') {
            t.inc(format!("type:ns {ns}"));
        }
        if ty.ends_with(".CompositeHavokAsset") {
            let assets = i.child("Assets").map_or(0, |a| a.children().count() as u64);
            let tr = i.child("Transforms").map(|x| {
                let (c, _) = bc2_formats::varint::read_u64(x.raw(), 0).unwrap_or((0, 0));
                c
            });
            t.inc(if tr == Some(16 * assets) {
                "type:cpx3 ok"
            } else {
                "type:cpx3 bad"
            });
        }
    }
    if dup {
        t.inc("inst:partitions with a duplicate guid");
    }
    match found {
        Some(0) => t.inc("inst:primary first"),
        Some(_) => t.inc("inst:primary later"),
        None => t.inc(if ids == 0 {
            "inst:primary unmatched, no id"
        } else {
            "inst:primary unmatched, id"
        }),
    }
    t.inc(match (ids, id_on_primary) {
        (1, 1) => "inst:id once on the primary",
        (0, _) => "inst:id none",
        _ => "inst:id elsewhere",
    });
    parts.push(Part {
        name: name.to_ascii_lowercase(),
        guid: guid128(pg).unwrap_or(0),
        instances: guids,
    });
}

/// Entry data: zipped bundles whole; stored bundles (the large sound bundles) read where each entry is, only the
/// first `head` bytes of entries that are not `.dbx`.
fn entry_data(path: &Path, h: &fbrb::Header, head: usize) -> Vec<Vec<u8>> {
    if h.zipped {
        return fbrb::extract_path_uncached(path).expect("a DICE bundle extracts").1;
    }
    let mut f = File::open(path).unwrap();
    h.entries
        .iter()
        .map(|e| {
            if e.is_deleted() {
                return Vec::new();
            }
            let whole = e.name.to_ascii_lowercase().ends_with(".dbx")
                || matches!(e.type_name.as_str(), "ITexture" | "SwfMovie");
            let n = if whole {
                e.size as usize
            } else {
                head.min(e.size as usize)
            };
            let mut b = vec![0u8; n];
            f.seek(SeekFrom::Start(h.payload_start + u64::from(e.offset))).unwrap();
            f.read_exact(&mut b).unwrap();
            b
        })
        .collect()
}

fn bundle(path: &Path, rel: &str) -> Bundle {
    let mut b = Bundle::default();
    let t = &mut b.t;
    let file = std::fs::read(path).unwrap();
    let h = fbrb::read_header(&mut file.as_slice()).unwrap();
    t.inc("fbrb:bundles");
    t.inc(format!("fbrb:version {}", h.version));
    t.inc(if h.zipped {
        "fbrb:zipped".to_owned()
    } else {
        format!("fbrb:stored {rel}")
    });
    // the header member, then zeros up to the payload (FBRB-HDR-3, 4)
    let cut = h.payload_start as usize;
    let packed = &file[8..cut];
    let mut dec = flate2::bufread::GzDecoder::new(packed);
    std::io::copy(&mut dec, &mut std::io::sink()).unwrap();
    let rest = dec.into_inner();
    t.inc(match (rest.is_empty(), rest.iter().all(|c| *c == 0)) {
        (true, _) => "fbrb:hdr3 nothing after the member".to_owned(),
        (false, true) => "fbrb:hdr3 zeros after the member".to_owned(),
        (false, false) => "fbrb:hdr3 other bytes after the member".to_owned(),
    });
    t.inc(format!("fbrb:hdr4 header member {}", hex(&packed[..10])));
    let live: Vec<&fbrb::Entry> = h.entries.iter().filter(|e| !e.is_deleted()).collect();
    if h.zipped && h.payload_len.unwrap_or(0) > 0 {
        t.inc(format!("fbrb:hdr4 payload member {}", hex(&file[cut..cut + 10])));
    }
    // entries (FBRB-ENT-2, 3, 4, 7; FBRB-DEL-2; FBRB-LAY-1)
    let mut lower_names = HashSet::new();
    for e in &h.entries {
        if e.is_deleted() {
            t.inc(format!("fbrb:flags marker {:X}", e.flags));
            continue;
        }
        t.inc("fbrb:live");
        t.inc(format!("fbrb:flags live {:X}", e.flags));
        if e.name.contains('\\') || e.name.starts_with('/') || e.name.is_empty() {
            t.inc("fbrb:name bad");
        }
        lower_names.insert(e.name.to_ascii_lowercase());
        let ext = e.name.rsplit_once('.').map_or("", |(_, x)| x).to_ascii_lowercase();
        t.inc(format!("res:type {}|{ext}", e.type_name));
    }
    if lower_names.len() != live.len() {
        t.inc("fbrb:bundles with a case-insensitive duplicate");
    }
    if live.windows(2).all(|w| w[0].offset <= w[1].offset) {
        t.inc("fbrb:offsets ascending");
    }
    let contiguous =
        live.first().is_some_and(|e| e.offset == 0) && live.windows(2).all(|w| w[1].offset == w[0].offset + w[0].size);
    if live.is_empty() {
        t.inc(if h.entries.is_empty() {
            "fbrb:no entries"
        } else {
            "fbrb:markers only"
        });
        // DICE's packer writes an empty gzip member after the header (FbrbTool writes none)
        if h.zipped && h.payload_len == Some(0) && file[cut..] == EMPTY_MEMBER {
            t.inc("fbrb:no live entry, zipped, payload 0, an empty gzip member");
        }
    } else if h.zipped {
        t.inc("fbrb:zipped with live entries");
        let end = live.last().map(|e| e.offset + e.size);
        if contiguous && end == h.payload_len {
            t.inc("fbrb:zipped contiguous to the payload length");
        }
    } else {
        t.inc("fbrb:stored with live entries");
        if contiguous {
            t.inc(format!("fbrb:stored contiguous {rel}"));
        }
    }
    // entry data
    let data = entry_data(path, &h, 156);
    let names: HashMap<String, usize> = h
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_deleted())
        .map(|(i, e)| (e.name.to_ascii_lowercase(), i))
        .collect();
    let bundle_name = rel.rsplit('/').next().unwrap_or(rel).to_owned();
    let in_levels = rel.contains("/levels/");
    for (e, d) in h.entries.iter().zip(&data) {
        if e.is_deleted() {
            continue;
        }
        let low = e.name.to_ascii_lowercase();
        let ty = e.type_name.as_str();
        if (low.ends_with(".res") || low.ends_with(".bin")) && d.starts_with(b"RES") {
            let family = bundle_name.split('-').next().unwrap_or("");
            t.inc(format!("res:wrapped {ty} in {family}"));
        }
        if ty == "<non-resource>" && !low.ends_with(".dbx") {
            let ext = low.rsplit_once('.').map_or("", |(_, x)| x);
            let top = low.split('/').next().unwrap_or("");
            let family = bundle_name.split('-').next().unwrap_or("");
            t.inc(format!("res:nonres {family}|{top}|{ext}"));
        }
        match ty {
            "Terrain" => t.inc(format!(
                "terr:terrain entry size {} res {}",
                e.size,
                low.ends_with(".res")
            )),
            "TerrainHeightfield" | "TerrainMaterialMap" | "VisualTerrain" | "WaterMesh" | "VisualWater"
                if e.size > 0 =>
            {
                t.inc(format!(
                    "terr:{ty} in {}",
                    if bundle_name == "terrain-00.fbrb" {
                        "terrain-00"
                    } else {
                        "another bundle"
                    }
                ));
                let leaf = low.rsplit('/').next().unwrap_or("");
                let folder_ok = low.starts_with("terrains/");
                let suffix_ok = match ty {
                    "TerrainHeightfield" => leaf.contains(".heightfield-") && leaf.ends_with(".res"),
                    "VisualTerrain" => leaf.ends_with(".visual.res"),
                    "WaterMesh" => leaf.ends_with(".water.mesh.res"),
                    "VisualWater" => leaf.ends_with(".water.res"),
                    _ => leaf.ends_with(".res"),
                };
                t.inc(format!("terr:{ty} name {}", folder_ok && suffix_ok));
            }
            "HavokPhysicsData" => {
                let form = if low.ends_with("_havok_havok.res") {
                    "_Havok_Havok"
                } else if low.ends_with("_havokasset.res") {
                    "_HavokAsset"
                } else if low.ends_with("_havok.res") {
                    "_Havok"
                } else {
                    "other"
                };
                t.inc(format!("havok:name {form} in levels {in_levels}"));
            }
            "SwfMovie" => {
                // BIGF-DAT-1: after the index, 4-byte aligned, in index order, 0 - 3 bytes apart, the last at the end
                let ok = bigf::parse(d).is_some_and(|m| {
                    let index_end = be32(d, 12) as usize;
                    let mut at = index_end;
                    m.iter().all(|(_, r)| {
                        let fits = r.start >= at && r.start - at < 4 && r.start % 4 == 0;
                        at = r.end;
                        fits
                    }) && at == d.len()
                });
                t.inc(format!("bigf:members as described {ok}"));
            }
            "ITexture" => {
                if let Some(mut x) = texture(d) {
                    x.awards = low.starts_with("awards/");
                    b.textures.push((low.clone(), x));
                } else {
                    t.inc("tex:unreadable");
                }
            }
            "MeshData" => match meshdata::parse_mesh(d, false) {
                Ok(m) => {
                    let fb_ok = m
                        .parts
                        .iter()
                        .filter(|p| usize::from(p.bytes[3]) == p.bones.len())
                        .count() as u64;
                    b.meshes.push((
                        low.clone(),
                        Mesh {
                            h0: m.h0,
                            wide0_even: m.sizes[2] == 0 && m.sizes[1] % 2 == 0,
                            parts: m.parts.len() as u64,
                            fb_ok,
                        },
                    ));
                }
                Err(_) => t.inc("mesh:unreadable"),
            },
            _ => {}
        }
        if let Some(kind) = meshdata::Kind::of(ty) {
            let Ok(set) = meshdata::parse_set(d, kind, false, None) else {
                t.inc("mset:unreadable");
                continue;
            };
            let stem = low.strip_suffix(".res").unwrap_or(&low);
            let mut lods = Vec::new();
            let (mut total, mut ok) = (0u64, 0u64);
            for (i, lod) in set.lods.iter().enumerate() {
                let md = names.get(&format!("{stem}_lod{i}_data.res")).map(|&j| &data[j]);
                let parsed = md.and_then(|m| meshdata::parse_mesh(m, false).ok());
                lods.push(SetLod {
                    kind: match kind {
                        meshdata::Kind::Rigid => "rigid",
                        meshdata::Kind::Skinned => "skinned",
                        meshdata::Kind::Composite => "composite",
                        meshdata::Kind::Tree => "tree",
                    },
                    h0: parsed.as_ref().map(|m| m.h0),
                    size_ok: parsed.as_ref().map(|m| lod.size == m.sizes[0] + m.sizes[1] + 12),
                    shaders_ok: parsed.as_ref().map(|m| usize::from(lod.shaders) == m.parts.len()),
                });
                if let Some((parts, _)) = &lod.parts {
                    for (hash, name, _, _) in parts {
                        total += 1;
                        ok += u64::from(*hash == part_hash(name));
                    }
                }
            }
            b.set_lods.push((low.clone(), lods));
            if matches!(kind, meshdata::Kind::Composite | meshdata::Kind::Tree) {
                b.set_parts.push((low.clone(), total, ok));
            }
        }
        if low.ends_with(".dbx") && !d.is_empty() {
            dbx_file(d, t);
            if in_levels {
                model_file(&e.name, d, t, &mut b.parts);
            }
        }
    }
    b
}

fn corpus() -> Option<&'static Corpus> {
    static C: OnceLock<Option<Corpus>> = OnceLock::new();
    C.get_or_init(|| {
        let game = game()?;
        let mut files = Vec::new();
        walk(&game, &mut files);
        files.retain(|p| dice(p));
        files.sort();
        let rel: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(&game)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_ascii_lowercase()
            })
            .collect();
        let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build().unwrap();
        let bundles: Vec<Bundle> = pool.install(|| files.par_iter().zip(&rel).map(|(p, r)| bundle(p, r)).collect());
        let mut t = Tally::default();
        let mut d = Tally::default();
        let (mut tex, mut mesh, mut sets, mut setparts) =
            (HashSet::new(), HashSet::new(), HashSet::new(), HashSet::new());
        let mut inst_names: HashMap<u128, (u32, String)> = HashMap::new();
        let mut part_guids: HashMap<u128, (u32, String)> = HashMap::new();
        let mut part_names = HashSet::new();
        for b in bundles {
            t.merge(b.t);
            for (n, x) in b.textures {
                if tex.insert(n) {
                    d.inc(format!(
                        "tex:{} awards {}",
                        if x.wrapped { "wrapped" } else { "unwrapped" },
                        x.awards
                    ));
                    if x.wrapped {
                        d.inc(format!("tex:wrap lengths ok {}", x.wrap_lengths_ok));
                    } else {
                        d.inc(format!("tex:format {}", x.format));
                        d.inc(format!("tex:flags {}", x.flags));
                    }
                    d.inc(format!("tex:levels ok {}", x.levels_ok));
                    d.inc(format!("tex:length ok {}", x.length_ok));
                }
            }
            for (n, m) in b.meshes {
                if mesh.insert(n) {
                    d.inc("mesh:distinct");
                    d.inc(format!("mesh:h0 {}", m.h0));
                    d.inc(format!("mesh:wide 0 and I even {}", m.wide0_even));
                    d.add("mesh:parts", m.parts);
                    d.add("mesh:fb = bones", m.fb_ok);
                }
            }
            for (n, lods) in b.set_lods {
                if sets.insert(n) {
                    for l in lods {
                        d.inc(format!("mset:lods {}", l.kind));
                        if let Some(h0) = l.h0 {
                            d.inc(format!("mset:h0 {} {h0}", l.kind));
                        }
                        if let Some(ok) = l.size_ok {
                            d.inc(format!("mset:size ok {} {ok}", l.kind));
                        }
                        if let Some(ok) = l.shaders_ok {
                            d.inc(format!("mset:shaders = parts {ok}"));
                        }
                    }
                }
            }
            for (n, total, ok) in b.set_parts {
                if setparts.insert(n) {
                    d.add("mset:part hashes", total);
                    d.add("mset:part hashes ok", ok);
                }
            }
            for p in b.parts {
                if !part_names.insert(p.name.clone()) {
                    continue;
                }
                d.inc("inst:distinct partitions");
                let id = part_names.len() as u32;
                for g in p.instances {
                    let e = inst_names.entry(g).or_insert((id, p.name.clone()));
                    if e.0 != id && e.0 != u32::MAX {
                        e.0 = u32::MAX;
                    }
                }
                let e = part_guids.entry(p.guid).or_insert((id, p.name.clone()));
                if e.0 != id && e.0 != u32::MAX {
                    e.0 = u32::MAX;
                }
            }
        }
        d.add("inst:distinct instance guids", inst_names.len() as u64);
        d.add(
            "inst:instance guids in more than one name",
            inst_names.values().filter(|v| v.0 == u32::MAX).count() as u64,
        );
        d.add("inst:distinct partition guids", part_guids.len() as u64);
        d.add(
            "inst:partition guids in more than one name",
            part_guids.values().filter(|v| v.0 == u32::MAX).count() as u64,
        );
        for (k, v) in &t.0 {
            println!("{k} = {v}");
        }
        for (k, v) in &d.0 {
            println!("distinct {k} = {v}");
        }
        Some(Corpus { t, d })
    })
    .as_ref()
}

fn map(pairs: &[(&str, u64)]) -> BTreeMap<String, u64> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
}

macro_rules! need_corpus {
    () => {{
        let Some(c) = corpus() else {
            println!("skipped: no game copy (OPFOR_ROOT or beside the suite)");
            return;
        };
        c
    }};
}

// spec: FBRB-HDR-3, FBRB-HDR-4, FBRB-HDR-6, FBRB-HDR-10
#[test]
#[ignore = "game tier: needs the game copy"]
fn fbrb_headers() {
    let c = need_corpus!();
    let t = &c.t;
    assert_eq!(t.get("fbrb:bundles"), 1021);
    assert_eq!(t.under("fbrb:version "), map(&[("2", 1021)]));
    assert_eq!(t.get("fbrb:zipped"), 1012);
    // stored: the async on-demand bundles, the streaming sound bundles and the build overlays
    let stored = t.under("fbrb:stored ");
    let stored: Vec<&str> = stored
        .keys()
        .filter(|k| !k.starts_with("contiguous ") && !k.starts_with("with "))
        .map(String::as_str)
        .collect();
    assert_eq!(stored.len(), 9, "{stored:?}");
    for b in &stored {
        let leaf = b.rsplit('/').next().unwrap();
        assert!(
            leaf.starts_with("ondemand_") || leaf.starts_with("streaming_") || leaf.starts_with("build_overlay"),
            "{b}"
        );
    }
    assert_eq!(t.get("fbrb:hdr3 other bytes after the member"), 0);
    assert_eq!(
        t.get("fbrb:hdr3 nothing after the member") + t.get("fbrb:hdr3 zeros after the member"),
        1021
    );
    assert_eq!(
        t.under("fbrb:hdr4 header member "),
        map(&[("1F8B080000000000000B", 1021)])
    );
    assert_eq!(
        t.under("fbrb:hdr4 payload member ").keys().collect::<Vec<_>>(),
        ["1F8B080000000000000B"]
    );
}

// spec: FBRB-ENT-2, FBRB-ENT-3, FBRB-ENT-4, FBRB-ENT-7, FBRB-DEL-2, FBRB-LAY-1, RES-TYPE-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn fbrb_entries() {
    let c = need_corpus!();
    let t = &c.t;
    assert_eq!(t.get("fbrb:name bad"), 0);
    assert_eq!(t.get("fbrb:bundles with a case-insensitive duplicate"), 0);
    assert_eq!(t.under("fbrb:flags live ").keys().collect::<Vec<_>>(), ["10000"]);
    assert_eq!(t.under("fbrb:flags marker ").keys().collect::<Vec<_>>(), ["0"]);
    assert_eq!(t.get("fbrb:offsets ascending"), 1021);
    assert_eq!(
        t.get("fbrb:zipped contiguous to the payload length"),
        t.get("fbrb:zipped with live entries")
    );
    assert_eq!((t.get("fbrb:markers only"), t.get("fbrb:no entries")), (104, 1));
    assert_eq!(
        t.get("fbrb:no live entry, zipped, payload 0, an empty gzip member"),
        105
    );
    // the resource types and their extensions (RES-TYPE-1)
    let types = t.under("res:type ");
    let names: std::collections::BTreeSet<&str> = types.keys().map(|k| k.split('|').next().unwrap()).collect();
    assert_eq!(names.len(), 37, "{names:?}");
    for k in types.keys() {
        let (ty, ext) = k.split_once('|').unwrap();
        if ty == "<non-resource>" {
            assert!(["dbx", "res", "bin", "dbmanifest"].contains(&ext), "{k}");
        } else {
            assert_eq!(ext, "res", "{k}");
        }
    }
}

// spec: RES-NAME-2, RES-WRAP-2
#[test]
#[ignore = "game tier: needs the game copy"]
fn res_entries() {
    let c = need_corpus!();
    let t = &c.t;
    let wrapped = t.under("res:wrapped ");
    assert!(
        wrapped
            .keys()
            .all(|k| k == "ITexture in ondemand_awards" || k == "Wave in ondemand_sounds"),
        "{wrapped:?}"
    );
    let nonres = t.under("res:nonres ");
    for k in nonres.keys() {
        let ok = matches!(
            k.as_str(),
            "overlay|localization|bin" | "build_overlay|database.dbmanifest|dbmanifest"
        ) || (k.starts_with("streaming_") && k.ends_with("|sound|res"));
        assert!(ok, "{k}");
    }
}

// spec: BIGF-DAT-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn bigf_members() {
    let c = need_corpus!();
    let t = &c.t;
    assert_eq!(t.under("bigf:members as described "), map(&[("true", 876)]));
}

// spec: TERR-NAME-1, TERR-NAME-2, HAVOK-RES-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn resource_names() {
    let c = need_corpus!();
    let t = &c.t;
    let terr = t.under("terr:");
    for k in terr.keys() {
        assert!(!k.ends_with(" in another bundle") && !k.ends_with(" name false"), "{k}");
        if let Some(rest) = k.strip_prefix("terrain entry size ") {
            assert_eq!(rest, "0 res true", "{k}");
        }
    }
    let havok = t.under("havok:name ");
    assert!(
        havok
            .keys()
            .all(|k| k.ends_with(" in levels true") && !k.starts_with("other")),
        "{havok:?}"
    );
}

// spec: DBX-HDR-3, DBX-HDR-4, DBX-STR-2, DBX-STR-3
#[test]
#[ignore = "game tier: needs the game copy"]
fn dbx_headers_and_strings() {
    let c = need_corpus!();
    let t = &c.t;
    let files = t.get("dbx:files");
    assert!(files > 100_000, "{files}");
    assert_eq!(
        (t.get("dbx:hdr3 ok"), t.get("dbx:hdr4 zero"), t.get("dbx:str2 empty")),
        (files, files, files)
    );
    assert_eq!(t.get("dbx:str3 packed"), files);
    // DICE's tables hold strings twice but no unused string, and never in first-use order
    assert_eq!(t.get("dbx:str3 a string twice"), 34_819);
    assert_eq!(
        (t.get("dbx:str3 an unused string"), t.get("dbx:str3 first-use order")),
        (0, 0)
    );
}

// spec: DBX-ELEM-6, DBX-ELEM-7, DBX-NUM-2, TYPE-ELEM-5
#[test]
#[ignore = "game tier: needs the game copy"]
fn dbx_elements() {
    let c = need_corpus!();
    let t = &c.t;
    assert_eq!(t.under("dbx:kind ").keys().collect::<Vec<_>>(), ["2", "6", "7", "A"]);
    let deepest = t.under("dbx:deepest ");
    assert!(deepest.keys().all(|k| k.parse::<u32>().unwrap() <= 9), "{deepest:?}");
    assert_eq!(t.under("dbx:container byte ").keys().collect::<Vec<_>>(), ["0"]);
    for k in t.under("dbx:num size ").keys() {
        let (size, tag) = k.split_once(" on ").unwrap();
        assert!(
            matches!((size, tag), ("4", "field" | "complex" | "array") | ("2" | "8", "field")),
            "{k}"
        );
    }
}

// spec: TYPE-ELEM-1, TYPE-ELEM-2, TYPE-ELEM-4, TYPE-ELEM-6, TYPE-ATTR-1, TYPE-ATTR-3, TYPE-ATTR-4
#[test]
#[ignore = "game tier: needs the game copy"]
fn type_elements() {
    let c = need_corpus!();
    let t = &c.t;
    let parts = t.get("inst:partitions");
    assert!(parts > 50_000, "{parts}");
    assert_eq!(
        t.under("type:tag ").keys().collect::<Vec<_>>(),
        ["array", "complex", "field", "instance", "item", "partition"]
    );
    let pairs: Vec<String> = t.under("type:pair ").into_keys().collect();
    let allowed = [
        "partition>instance",
        "instance>field",
        "instance>complex",
        "instance>array",
        "complex>field",
        "complex>complex",
        "complex>array",
        "array>item",
        "array>complex",
    ];
    assert!(pairs.iter().all(|p| allowed.contains(&p.as_str())), "{pairs:?}");
    let kinds: Vec<String> = t.under("type:tagkind ").into_keys().collect();
    let allowed = [
        "partition:A",
        "instance:A",
        "instance:2",
        "field:7",
        "field:2",
        "field:6",
        "complex:A",
        "complex:7",
        "array:A",
        "array:2",
        "array:7",
        "item:2",
    ];
    assert!(kinds.iter().all(|k| allowed.contains(&k.as_str())), "{kinds:?}");
    assert_eq!(t.get("type:instance text nonempty"), 0);
    assert_eq!(t.get("type:instance text empty"), t.get("type:tagkind instance:2"));
    let attrs: Vec<String> = t.under("type:attrs ").into_keys().collect();
    let allowed = [
        "partition:guid,primaryInstance,exportMode",
        "instance:guid,type",
        "instance:guid,type,id",
        "field:name",
        "field:ref,name",
        "complex:name",
        "complex:(none)",
        "array:name",
        "item:ref",
        "item:(none)",
    ];
    assert!(attrs.iter().all(|a| allowed.contains(&a.as_str())), "{attrs:?}");
    assert_eq!(t.get("type:type other"), 0);
    assert_eq!(t.under("type:ns ").len(), 14);
    assert_eq!(t.under("type:exportMode "), map(&[("All", parts)]));
}

// spec: TYPE-VAL-2, TYPE-VAL-3, TYPE-NUM-4, TYPE-CPLX-3, TYPE-ARR-1, TYPE-ARR-2, TYPE-ARR-3, TYPE-ARR-4
#[test]
#[ignore = "game tier: needs the game copy"]
fn type_values() {
    let c = need_corpus!();
    let t = &c.t;
    for k in t.under("type:scalar ").keys() {
        assert!(k.starts_with("6 01"), "{k}");
    }
    assert!(t.get("type:text empty") > 0 && t.get("type:text nonempty") > 0);
    assert_eq!(t.under("type:num size ").keys().collect::<Vec<_>>(), ["2", "4", "8"]);
    for k in t.under("type:num 1x2 on ").keys() {
        assert!(
            [
                "SphereKeyX",
                "SphereKeyY",
                "SphereKeyZ",
                "SphereKeyW",
                "SourceId",
                "TargetId"
            ]
            .contains(&k.as_str()),
            "{k}"
        );
    }
    assert_eq!(t.get("type:cpx3 bad"), 0);
    assert!(t.get("type:cpx3 ok") > 0);
    assert_eq!((t.get("type:item ref other"), t.get("type:item not text")), (0, 0));
    assert_eq!(
        t.get("type:item ref empty text") + t.get("type:item string"),
        t.get("type:tag item")
    );
    assert_eq!(
        (t.get("type:array text nonempty"), t.get("type:array container empty")),
        (0, 0)
    );
    assert!(t.get("type:array numbers") > 0);
    assert_eq!(t.get("type:unnamed complex elsewhere"), 0);
    assert_eq!(t.get("type:unnamed complex in array"), t.get("type:pair array>complex"));
}

// spec: INST-GUID-1, INST-GUID-2, INST-GUID-3, INST-PRIM-1, INST-PRIM-2, INST-PRIM-3, REF-FORM-2
#[test]
#[ignore = "game tier: needs the game copy"]
fn instances_and_references() {
    let c = need_corpus!();
    let (t, d) = (&c.t, &c.d);
    assert_eq!(
        t.get("inst:guid other") + t.get("inst:partition guid other") + t.get("inst:primary other"),
        0
    );
    assert_eq!(t.get("inst:partitions with a duplicate guid"), 0);
    assert!(d.get("inst:instance guids in more than one name") > 0);
    assert!(d.get("inst:partition guids in more than one name") > 0);
    let parts = t.get("inst:partitions");
    assert_eq!(
        t.get("inst:primary first") + t.get("inst:primary later") + t.get("inst:primary unmatched, no id"),
        parts
    );
    assert_eq!(t.get("inst:primary unmatched, id"), 0);
    assert_eq!(t.get("inst:id elsewhere") + t.get("inst:id other"), 0);
    assert_eq!(
        t.get("inst:id once on the primary"),
        parts - t.get("inst:primary unmatched, no id")
    );
    let forms = t.under("ref:form ");
    assert_eq!(
        forms.keys().collect::<Vec<_>>(),
        ["external lower", "local lower", "null"],
        "{forms:?}"
    );
}

// spec: TEX-HDR-2, TEX-FMT-2, TEX-FLAG-1, TEX-WRAP-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn textures() {
    let c = need_corpus!();
    let (t, d) = (&c.t, &c.d);
    assert_eq!(t.get("tex:unreadable"), 0);
    assert_eq!((d.get("tex:levels ok false"), d.get("tex:length ok false")), (0, 0));
    assert_eq!(
        (d.get("tex:wrapped awards false"), d.get("tex:unwrapped awards true")),
        (0, 0)
    );
    assert_eq!(d.get("tex:wrap lengths ok false"), 0);
    assert!(
        d.under("tex:flags ")
            .keys()
            .all(|k| ["0", "1", "2", "3"].contains(&k.as_str()))
    );
}

// spec: MESH-HDR-2, MESH-HDR-4, MESH-PART-2, MSET-LOD-2, MSET-LOD-4, MSET-PART-1
#[test]
#[ignore = "game tier: needs the game copy"]
fn meshes() {
    let c = need_corpus!();
    let (t, d) = (&c.t, &c.d);
    assert_eq!((t.get("mesh:unreadable"), t.get("mset:unreadable")), (0, 0));
    assert_eq!(d.under("mesh:h0 ").keys().collect::<Vec<_>>(), ["0", "1", "2", "4"]);
    let h0 = d.under("mset:h0 ");
    assert!(
        h0.keys()
            .all(|k| matches!(k.as_str(), "rigid 0" | "skinned 1" | "composite 2" | "tree 4")),
        "{h0:?}"
    );
    assert_eq!(d.get("mesh:wide 0 and I even false"), 0);
    assert_eq!(d.get("mesh:fb = bones"), d.get("mesh:parts"));
    assert_eq!(d.get("mset:shaders = parts false"), 0);
    assert!(d.under("mset:size ok ").keys().all(|k| k.ends_with(" true")));
    assert_eq!(d.get("mset:part hashes ok"), d.get("mset:part hashes"));
    assert!(d.get("mset:part hashes") > 6000);
}
