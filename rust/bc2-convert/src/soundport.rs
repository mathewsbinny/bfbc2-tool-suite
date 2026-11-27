//! Battlefield 1943's sounds in BFBC2 (round OR8, "OPFOR in Rust": Project OPFOR's `wake_level.port_sounds`,
//! `sound_fills`, `not_streaming`, its Python retired): the sound partitions a level, an effect or the material grid
//! reaches that BFBC2 lacks, aligned on BFBC2's sound partitions as templates, their waves converted (EA-XMA ->
//! EA-XAS, [`crate::wavport::convert_checked`]).
//!
//! - The partitions: the roots and every partition they name under `sound/` (or the caller's own prefixes), that
//!   BFBC2 does not have ([`walk`]).
//! - The templates: the donors (BFBC2 sound partitions of the same roles) and the sound partitions they name, a
//!   donor's first 40 followed in turn, then the last donors (types the pool lacks) ([`pool`]).
//! - Each partition through [`crate::dbxport::port`], then the fields BFBC2's sound classes have and BF1943's lack set
//!   as DICE set them when it converted BF1943's sounds ([`FILLS`], [`sound_fills`]); a wave whose audio is in memory
//!   now has its `IsStreaming` false ([`not_streaming`]).
//! - An instance of a type no template has stops the port ([`Error::Exit`]) unless its partition is ported again by
//!   a later step (`later`).

#![allow(
    clippy::implicit_hasher,
    clippy::too_many_lines,
    reason = "the module's sets are std's; the port's steps kept in one function, in the reference's order"
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use bc2_formats::dbx::{self, Body, CONTAINER, DbxFile, Element, NUMBERS, TEXT};
use rayon::prelude::*;
use regex::Regex;

use crate::dbxport::py::lower;
use crate::dbxport::{self, CatView, Fail, Options, SiKey, SiVal};
use crate::effectport::{Error, PortContext, Record, Res, Source, Stock, attr, bc2_partitions, build_templates, child};

/// A reference in a partition's XML (`wake_level.REF`).
static REF: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"(?i)ref="([a-z0-9_/.\- ]+)/[0-9a-f-]{36}""#).ok());
/// A dbxport line of an instance kept in BF1943's schema.
static UNTEMPLATED: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^type (\S+) \(([^)]*)\): no BFBC2 template").ok());
/// Waves converted side by side at a time (only a batch's inputs and outputs held).
pub const WAVE_BATCH: usize = 64;
/// Sound partitions a donor's references add to the pool, followed in turn.
pub const FOLLOW: usize = 40;

fn refs_raw(x: &str) -> Vec<String> {
    REF.as_ref()
        .into_iter()
        .flat_map(|r| r.captures_iter(x))
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_owned()))
        .collect()
}

fn xml(data: &[u8]) -> Res<String> {
    dbx::to_xml(data).map_err(|e| Error::Value(e.to_string()))
}

// ------------------------------------------------------------------------------------------------ the fills

/// A field value the fills set: an integer or a single, big-endian.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    /// `'i'`.
    Int(i32),
    /// `'f'`.
    Float(f32),
}

impl Num {
    fn bytes(self) -> [u8; 4] {
        match self {
            Self::Int(v) => v.to_be_bytes(),
            Self::Float(v) => v.to_be_bytes(),
        }
    }
}

fn skip_duplicates(f: &DbxFile, inst: &Element) -> Res<bool> {
    let Some(c) = child(f, inst, "SkipDuplicateSpawns") else {
        return Ok(false);
    };
    if matches!(c.kind, TEXT | NUMBERS | CONTAINER) {
        return Ok(false);
    }
    Ok(dbx::value(f, c).map_err(|e| Error::Value(e.to_string()))? == "true")
}

/// The fields BFBC2's sound classes have and BF1943's lack, as DICE set them when it converted BF1943's sounds for
/// BFBC2 (Project OPFOR's measure over the 1,081 sound partitions both games ship, `test_ww2_sound_fills`):
/// `SoundTemplate.PreventDuplicateSpawnFrameCount` 15 where BF1943's `SkipDuplicateSpawns` is true, else 3;
/// `PreventDuplicateSpawnDistance` 10; every emitter's `HFDampingOcclusion` 1. `None`: a type without fills.
///
/// # Errors
/// A `SkipDuplicateSpawns` that does not read.
pub fn fills(f: &DbxFile, inst: &Element) -> Res<Option<Vec<(&'static str, Num)>>> {
    Ok(match attr(f, inst, "type").unwrap_or("") {
        "Audio.SoundTemplate" => Some(vec![
            (
                "PreventDuplicateSpawnFrameCount",
                Num::Int(if skip_duplicates(f, inst)? { 15 } else { 3 }),
            ),
            ("PreventDuplicateSpawnDistance", Num::Float(10.0)),
        ]),
        "Audio.SoundSourceConeEmitter"
        | "Audio.SoundSourceFlatEmitter"
        | "Audio.SoundSourceDialogEmitter"
        | "Audio.SoundSourceLfeEmitter" => Some(vec![("HFDampingOcclusion", Num::Float(1.0))]),
        _ => None,
    })
}

/// The types [`fills`] has a rule for.
pub const FILLS: &[&str] = &[
    "Audio.SoundTemplate",
    "Audio.SoundSourceConeEmitter",
    "Audio.SoundSourceFlatEmitter",
    "Audio.SoundSourceDialogEmitter",
    "Audio.SoundSourceLfeEmitter",
];

fn numbers_raw(v: Num) -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    dbx::put_varint(&mut out, 1);
    dbx::put_varint(&mut out, 4);
    out.extend_from_slice(&v.bytes());
    out
}

fn child_mut<'e>(f: &DbxFile, e: &'e mut Element, name: &str) -> Option<&'e mut Element> {
    let Body::Container(kids) = &mut e.body else {
        return None;
    };
    kids.iter_mut().find(|c| attr(f, c, "name") == Some(name))
}

/// One instance's [`fills`] fields not in `had` (its BF1943 version's field names; `None`: no BF1943 version, left as
/// it is) set as DICE did: the number changed.
///
/// # Errors
/// As [`fills`].
pub fn fill_instance(f: &DbxFile, inst: &mut Element, had: Option<&HashSet<String>>) -> Res<usize> {
    let Some(had) = had else { return Ok(0) };
    let Some(rule) = fills(f, inst)? else { return Ok(0) };
    let mut n = 0;
    for (name, v) in rule {
        if had.contains(name) {
            continue;
        }
        let Some(c) = child_mut(f, inst, name) else { continue };
        if c.kind != NUMBERS {
            continue;
        }
        let new = numbers_raw(v);
        if c.raw() != new.as_slice() {
            n += 1;
        }
        c.body = Body::Numbers(new);
    }
    Ok(n)
}

fn instances_mut(f: &mut DbxFile) -> Res<Vec<&mut Element>> {
    let p = f
        .roots
        .first_mut()
        .ok_or_else(|| Error::Value("no partition".to_owned()))?;
    let Body::Container(kids) = &mut p.body else {
        return Err(Error::Value("the partition holds no instances".to_owned()));
    };
    Ok(kids.iter_mut().collect())
}

/// A ported sound partition with [`fills`]' fields set as DICE did on the instances whose BF1943 version (`source`)
/// lacks them: the bytes (the same when nothing changed) and the number of fields changed.
///
/// # Errors
/// A partition that does not read or write.
pub fn sound_fills(data: &[u8], source: &[u8]) -> Res<(Vec<u8>, usize)> {
    let mut f = dbx::parse(data).map_err(|e| Error::Value(e.to_string()))?;
    let s = dbx::parse(source).map_err(|e| Error::Value(e.to_string()))?;
    let mut lacks: HashMap<String, HashSet<String>> = HashMap::new();
    if let Some(p) = s.roots.first() {
        for i in p.children().iter().filter(|e| s.str(e.tag) == "instance") {
            let names = i
                .children()
                .iter()
                .filter_map(|c| attr(&s, c, "name").map(str::to_owned))
                .collect();
            lacks.insert(lower(attr(&s, i, "guid").unwrap_or("")), names);
        }
    }
    let probe = f.clone();
    let mut n = 0;
    let insts = instances_mut(&mut f)?;
    for inst in insts {
        if probe.str(inst.tag) != "instance" {
            continue;
        }
        let g = lower(attr(&probe, inst, "guid").unwrap_or(""));
        n += fill_instance(&probe, inst, lacks.get(&g))?;
    }
    if n == 0 {
        return Ok((data.to_vec(), 0));
    }
    Ok((dbx::write(&f).map_err(|e| Error::Value(e.to_string()))?, n))
}

/// A wave partition with its `WaveAsset`'s `IsStreaming` false: the bytes and whether they changed. The conversion
/// puts the audio in memory (a streamed BF1943 wave's from its `_waveDataSns_` entries) - in DICE's data the flag is
/// true exactly when the audio is streamed.
///
/// # Errors
/// A partition that does not read or write.
pub fn not_streaming(data: &[u8]) -> Res<(Vec<u8>, bool)> {
    let mut f = dbx::parse(data).map_err(|e| Error::Value(e.to_string()))?;
    let probe = f.clone();
    let mut hit = false;
    for inst in instances_mut(&mut f)? {
        if probe.str(inst.tag) != "instance" || attr(&probe, inst, "type") != Some("Audio.WaveAsset") {
            continue;
        }
        let Some(c) = child_mut(&probe, inst, "IsStreaming") else {
            continue;
        };
        if dbx::value(&probe, c).map_err(|e| Error::Value(e.to_string()))? != "true" {
            continue;
        }
        let first = c.raw().first().copied();
        c.body = match (first, &c.body) {
            (Some(b), Body::Scalar(_)) => Body::Scalar([b, 0]),
            (_, Body::Numbers(r)) => Body::Numbers(r.get(..1).unwrap_or_default().iter().copied().chain([0]).collect()),
            (_, b) => b.clone(),
        };
        hit = true;
    }
    if !hit {
        return Ok((data.to_vec(), false));
    }
    Ok((dbx::write(&f).map_err(|e| Error::Value(e.to_string()))?, true))
}

// ------------------------------------------------------------------------------------------------ the port

/// What the sound porter takes besides the game data.
pub struct Spec {
    /// The partitions to port and those they name.
    pub roots: Vec<String>,
    /// Where the files go (an install root).
    pub out: String,
    /// BFBC2 sound partitions to take templates from, the default donors first (`SOUND_DONORS` + `donors`).
    pub donors: Vec<String>,
    /// Templates for the types the pool lacks, taken last (`SOUND_LAST_DONORS`).
    pub last_donors: Vec<String>,
    /// The prefixes a ported partition's references are followed under (`('sound/', RES)`).
    pub follow: Vec<String>,
    /// `stand_ins` (`SOUND_STAND_INS`).
    pub stand_ins: HashMap<SiKey, SiVal>,
    /// Partitions another step ports again on its own templates: `{partition: the step}`.
    pub later: BTreeMap<String, String>,
}

/// The partitions to port, in the order found (BFBC2's own and those BF1943 lacks left out).
///
/// # Errors
/// A partition that does not read.
pub fn walk(src: &dyn Source, bc2: &HashSet<String>, roots: &[String], follow: &[String]) -> Res<Vec<String>> {
    let mut todo: Vec<String> = roots.to_vec();
    let mut want = Vec::new();
    let mut seen = HashSet::new();
    while let Some(p) = todo.pop() {
        if seen.contains(&p) || bc2.contains(&p) || src.entry(&format!("{p}.dbx")).is_none() {
            continue;
        }
        seen.insert(p.clone());
        let x = xml(&src.data(&format!("{p}.dbx"))?)?;
        want.push(p);
        todo.extend(
            refs_raw(&x)
                .into_iter()
                .filter(|r| follow.iter().any(|f| r.starts_with(f.as_str()))),
        );
    }
    Ok(want)
}

/// The template pool: the donors, the sound partitions they name (a donor's first [`FOLLOW`] followed in turn),
/// then the last donors the pool lacks. Only the donors and the partitions BFBC2 has are read.
///
/// # Errors
/// A partition that does not read.
pub fn pool(ix: &dyn Stock, donors: &[String], last: &[String]) -> Res<Vec<String>> {
    let mut pool: Vec<String> = donors.to_vec();
    let mut more: Vec<String> = donors.to_vec();
    while let Some(p) = more.pop() {
        let Some(b) = ix.read(&format!("{p}.dbx"))?.filter(|b| !b.is_empty()) else {
            continue;
        };
        let new: Vec<String> = refs_raw(&xml(&b)?)
            .into_iter()
            .filter(|r| r.starts_with("sound/") && !pool.contains(r))
            .collect();
        pool.extend(new.iter().cloned());
        more.extend(new.into_iter().take(FOLLOW));
    }
    for p in last {
        if !pool.contains(p) {
            pool.push(p.clone());
        }
    }
    Ok(pool)
}

fn write(out: &str, name: &str, data: &[u8]) -> Res<()> {
    let mut path = Path::new(out).to_path_buf();
    path.extend(name.split('/'));
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| Error::Io(format!("{}: {e}", d.display())))?;
    }
    bc2_gamefs::atomic::write_rebuildable(&path, &[data])
        .map_err(|e| Error::Io(format!("{}: {}", path.display(), e.io())))
}

/// A partition ported: its bytes, its untemplated instances' lines, the fields filled.
type Ported = (Vec<u8>, Vec<String>, usize);

/// A wave conversion's input: the Wave resource and its streamed segments.
type WaveJob = (Vec<u8>, [Option<Vec<u8>>; 3]);

fn wave_job(src: &dyn Source, p: &str) -> Res<Option<WaveJob>> {
    let res = format!("{p}.res");
    if src.entry(&res).and_then(|e| e.kind.as_deref()) != Some("Wave") {
        return Ok(None);
    }
    let mut streams: [Option<Vec<u8>>; 3] = [None, None, None];
    if let Some(stem) = p.strip_suffix("_wave") {
        for (i, s) in streams.iter_mut().enumerate() {
            let k = format!("{stem}_wavedatasns_{i}.res");
            if src.entry(&k).is_some() {
                *s = Some(src.data(&k)?);
            }
        }
    }
    Ok(Some((src.data(&res)?, streams)))
}

fn convert(job: &WaveJob) -> Result<Vec<u8>, String> {
    let streams = [job.1[0].as_deref(), job.1[1].as_deref(), job.1[2].as_deref()];
    crate::wavport::convert_checked(&job.0, "bf1943", &streams, 0)
}

/// Ports the BF1943 sound partitions under the roots (and those they name) that BFBC2 lacks into `spec.out`, their
/// waves converted; `log` hears the summary, the first failures and the untemplated instances another step ports
/// again. Returns the partitions ported, in the order found.
///
/// # Errors
/// A partition that does not read or port, a template missing, a file that cannot be written; [`Error::Exit`] for
/// instances of a type no template has (kept in BF1943's schema) outside `spec.later`.
pub fn port_sounds(
    src: &dyn Source,
    ix: &dyn Stock,
    spec: &Spec,
    ctx: &PortContext<'_>,
    log: &mut dyn FnMut(&str) -> Res<()>,
) -> Res<Vec<String>> {
    let bc2 = bc2_partitions(ix);
    let want = walk(src, &bc2, &spec.roots, &spec.follow)?;
    let pool = pool(ix, &spec.donors, &spec.last_donors)?;
    // the pool's later entries are the references the donors make - only those BFBC2 has; the donors must exist
    let mut items = Vec::new();
    for (i, p) in pool.iter().enumerate() {
        let name = format!("{p}.dbx");
        if i < spec.donors.len() || ix.locate(&name).is_some() {
            items.push((p.clone(), ix.read(&name)?));
        }
    }
    let t = build_templates(items, ctx.rules)?;
    let ours: HashSet<&str> = want.iter().map(String::as_str).collect();
    let resolve = |q: &str| -> Result<bool, Fail> {
        let q = lower(q);
        Ok(ours.contains(q.as_str()) || bc2.contains(&q))
    };
    let mut sorted = want.clone();
    sorted.sort_unstable();
    let (mut waves, mut failed, mut untemplated, mut in_memory, mut filled) =
        (0, Vec::new(), Vec::new(), Vec::new(), 0);
    for chunk in sorted.chunks(WAVE_BATCH) {
        // read in the loop's order, converted side by side, then written in order
        let mut sources = Vec::with_capacity(chunk.len());
        let mut jobs = Vec::with_capacity(chunk.len());
        for p in chunk {
            sources.push(src.data(&format!("{p}.dbx"))?);
            jobs.push(wave_job(src, p)?);
        }
        let ported: Vec<Res<Ported>> = chunk
            .par_iter()
            .zip(sources.par_iter())
            .map(|(p, s)| {
                let o = Options {
                    drop_untemplated: false,
                    drop_types: HashSet::new(),
                    prune: false,
                    required: HashMap::new(),
                    catalogue: ctx.catalogue.as_ref().map(|c| CatView {
                        cat: c.cat,
                        els: c.els,
                        defaults: c.defaults,
                    }),
                    ours: None,
                    stand_ins: &spec.stand_ins,
                    donors: Vec::new(),
                    strict: false,
                    schema_error: true,
                    conventions: true,
                    bones: None,
                    name: lower(p),
                    rules: ctx.rules,
                    resolve: Some(&resolve),
                };
                let got = dbxport::port(s, &t, &o)?;
                let lines: Vec<String> = got
                    .records
                    .iter()
                    .filter_map(|r| {
                        let line = Record {
                            path: r.path.clone(),
                            action: r.action.to_owned(),
                            new: r.new.clone(),
                            ..Record::default()
                        }
                        .line();
                        let c = UNTEMPLATED.as_ref()?.captures(&line)?;
                        Some(format!("{p}: {} ({})", c.get(1)?.as_str(), c.get(2)?.as_str()))
                    })
                    .collect();
                let (data, n) = sound_fills(&got.data, s)?;
                Ok((data, lines, n))
            })
            .collect();
        let converted: Vec<Option<Result<Vec<u8>, String>>> =
            jobs.par_iter().map(|j| j.as_ref().map(convert)).collect();
        for (((p, got), job), pc) in chunk.iter().zip(ported).zip(&jobs).zip(converted) {
            let (data, lines, n) = got?;
            filled += n;
            untemplated.extend(lines);
            write(&spec.out, &format!("{p}.dbx"), &data)?; // first: its folders are made in the partition's case
            let (Some(_), Some(pc)) = (job, pc) else { continue };
            match pc {
                Ok(pc) => {
                    let res = format!("{p}.res");
                    let name = src.entry(&res).map_or(res.clone(), |e| e.name.clone());
                    write(&spec.out, &name, &pc)?;
                    waves += 1;
                    let (data, changed) = not_streaming(&data)?; // its audio is in memory now
                    if changed {
                        in_memory.push(p.clone());
                        write(&spec.out, &format!("{p}.dbx"), &data)?;
                    }
                }
                Err(e) => failed.push(format!("{p}: {e}")),
            }
        }
    }
    log(&format!(
        "sounds: {} partitions ported, {waves} waves converted (EA-XMA -> EA-XAS), {} failed; {} streamed in BF1943, \
         IsStreaming false (in memory): {}; {filled} template-filled fields set as DICE converted BF1943's sounds \
         (SOUND_FILLS)",
        want.len(),
        failed.len(),
        in_memory.len(),
        if in_memory.is_empty() {
            "-".to_owned()
        } else {
            in_memory.join(", ")
        }
    ))?;
    for line in failed.iter().take(5) {
        log(&format!("  FAILED {line}"))?;
    }
    let part = |u: &String| u.split_once(':').map_or(u.clone(), |(a, _)| a.to_owned());
    let (again, rest): (Vec<String>, Vec<String>) =
        untemplated.into_iter().partition(|u| spec.later.contains_key(&part(u)));
    if !again.is_empty() {
        let mut by: BTreeMap<String, usize> = BTreeMap::new();
        for u in &again {
            let p = part(u);
            let step = spec.later.get(&p).map_or("", String::as_str);
            *by.entry(format!("{p} (ported again by {step})")).or_insert(0) += 1;
        }
        let body: Vec<String> = by.iter().map(|(k, v)| format!("{k}: {v}")).collect();
        log(&format!("  untemplated, ported again later: {}", body.join("; ")))?;
    }
    if !rest.is_empty() {
        return Err(Error::Exit(format!(
            "sounds: {} instances of a type no donor partition has (kept in BF1943's schema) - add a donor with \
             one:\n  {}",
            rest.len(),
            rest.join("\n  ")
        )));
    }
    Ok(want)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::many_single_char_names)]
mod tests {
    use super::*;

    fn num(f: &mut DbxFile, name: &str, v: Num) -> Element {
        Element::numbers(f, "field", &[("name", name)], 4, &v.bytes())
    }

    fn template(skip: bool, count: i32, dist: f32) -> (DbxFile, Element) {
        let mut f = DbxFile::default();
        let b = Element::scalar(
            &mut f,
            "field",
            6,
            &[("name", "SkipDuplicateSpawns")],
            [1, u8::from(skip)],
        );
        let d = num(&mut f, "PreventDuplicateSpawnDistance", Num::Float(dist));
        let c = num(&mut f, "PreventDuplicateSpawnFrameCount", Num::Int(count));
        let i = Element::container(
            &mut f,
            "instance",
            &[("guid", "A"), ("type", "Audio.SoundTemplate")],
            vec![b, d, c],
        );
        (f, i)
    }

    fn value(f: &DbxFile, i: &Element, name: &str) -> String {
        dbx::value(f, child(f, i, name).unwrap()).unwrap()
    }

    fn had(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }

    // spec: XWW2-SND-1
    #[test]
    fn skip_duplicates_decides_the_frame_count() {
        let (f, mut t) = template(true, 3, 2.0);
        assert_eq!(
            fill_instance(&f, &mut t, Some(&had(&["SkipDuplicateSpawns"]))).unwrap(),
            2
        );
        assert_eq!(value(&f, &t, "PreventDuplicateSpawnFrameCount"), "15");
        assert_eq!(value(&f, &t, "PreventDuplicateSpawnDistance"), "10");
        assert_eq!(
            child(&f, &t, "PreventDuplicateSpawnFrameCount").unwrap().raw()[2..],
            15i32.to_be_bytes()
        );
        let (f, mut t) = template(false, 300, 10.0);
        assert_eq!(
            fill_instance(&f, &mut t, Some(&had(&["SkipDuplicateSpawns"]))).unwrap(),
            1
        );
        assert_eq!(value(&f, &t, "PreventDuplicateSpawnFrameCount"), "3");
    }

    #[test]
    fn fields_bf1943_has_and_instances_without_a_source_are_kept() {
        let (f, mut t) = template(true, 7, 10.0);
        let h = had(&["SkipDuplicateSpawns", "PreventDuplicateSpawnFrameCount"]);
        assert_eq!(fill_instance(&f, &mut t, Some(&h)).unwrap(), 0);
        assert_eq!(value(&f, &t, "PreventDuplicateSpawnFrameCount"), "7");
        assert_eq!(fill_instance(&f, &mut t, None).unwrap(), 0);
    }

    #[test]
    fn emitters_occlusion_damping() {
        for k in ["Cone", "Flat", "Dialog", "Lfe"] {
            let mut f = DbxFile::default();
            let a = num(&mut f, "HFDampingOcclusion", Num::Float(0.8));
            let b = num(&mut f, "HFDampingDistance", Num::Float(0.5));
            let ty = format!("Audio.SoundSource{k}Emitter");
            let mut e = Element::container(&mut f, "instance", &[("guid", "B"), ("type", &ty)], vec![a, b]);
            assert_eq!(
                fill_instance(&f, &mut e, Some(&had(&["HFDampingDistance"]))).unwrap(),
                1,
                "{k}"
            );
            assert_eq!(value(&f, &e, "HFDampingOcclusion"), "1");
            assert_eq!(value(&f, &e, "HFDampingDistance"), "0.5");
        }
    }

    fn partition(f: &mut DbxFile, insts: Vec<Element>) -> Vec<u8> {
        let p = Element::container(f, "partition", &[("guid", "p")], insts);
        f.roots = vec![p];
        dbx::write(f).unwrap()
    }

    // spec: XWW2-SND-2
    #[test]
    fn streamed_waves_in_memory() {
        let mut f = DbxFile::default();
        let s = Element::scalar(&mut f, "field", 6, &[("name", "IsStreaming")], [1, 1]);
        let w = Element::container(
            &mut f,
            "instance",
            &[("guid", "W"), ("type", "Audio.WaveAsset")],
            vec![s],
        );
        let data = partition(&mut f, vec![w]);
        let (out, changed) = not_streaming(&data).unwrap();
        assert!(changed);
        let back = dbx::parse(&out).unwrap();
        let i = &back.roots[0].children()[0];
        assert_eq!(value(&back, i, "IsStreaming"), "false");
        let (again, changed) = not_streaming(&out).unwrap();
        assert!(!changed);
        assert_eq!(again, out);
    }

    #[test]
    fn fills_only_where_bf1943_lacks_the_field() {
        let (mut f, t) = template(true, 3, 2.0);
        let data = partition(&mut f, vec![t]);
        let (out, n) = sound_fills(&data, &data).unwrap(); // BF1943's own has every field
        assert_eq!((n, out == data), (0, true));
        let mut g = DbxFile::default();
        let b = Element::scalar(&mut g, "field", 6, &[("name", "SkipDuplicateSpawns")], [1, 1]);
        let i = Element::container(
            &mut g,
            "instance",
            &[("guid", "a"), ("type", "Audio.SoundTemplate")],
            vec![b],
        );
        let source = partition(&mut g, vec![i]);
        let (out, n) = sound_fills(&data, &source).unwrap();
        assert_eq!(n, 2);
        let back = dbx::parse(&out).unwrap();
        assert_eq!(
            value(&back, &back.roots[0].children()[0], "PreventDuplicateSpawnFrameCount"),
            "15"
        );
    }

    #[test]
    fn references_as_the_reference_finds_them() {
        let x = r#"<field ref="Sound/A/B/0123abcd-0123-abcd-0123-0123456789ab" name="x" />"#;
        assert_eq!(refs_raw(x), ["Sound/A/B"]);
    }
}
