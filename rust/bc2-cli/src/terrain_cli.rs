//! `bfbc2 terrain generate`: a new terrain from a seed ([`bc2_terragen`], `spec/transforms/terrain_gen.md`): the main
//! height field shaped to a biome of DICE's, its material map and its water mesh written into a folder, with a
//! `terrain.json` that records the seed, the settings, the generator's version and the terrain's statistics against
//! the biome's DICE medians.
//!
//! ```text
//! bfbc2 terrain generate --biome <b> --size 1024|2048 --seed <n> --out <folder>
//!                        [--name <T>] [--border <m>] [--water <share>] [--pad <x>,<z>[,<r>]] ... [--erosion <k>]
//!                        [--rough <0-1>] [--no-ring] [--water-level <m>] [--donor <level> [--opfor <folder>]]
//! ```
//!
//! `--name`: the terrain's name (default `TG_<BIOME>_<seed>`): the files are `<T>.Heightfield-000.res`,
//! `<T>-MaterialMap.res`, `<T>.Water.Mesh.res` (when ground lies under water) and `<T>.res`, the terrain bundle's
//! names under `Terrains/<T>/` (TERR-NAME-1). `--border`: the border ring's median height minus the centre's in metres
//! (negative: a sea edge). `--water`: the share of ground under water (0 - 1). `--pad`: a site levelled for an
//! objective, world metres (radius 24 by default). `--erosion`: droplets per 1,000 samples (default 125).
//! `--rough`: the escarpment's strength (TGEN-GEN-6; temperate 1, the others 0). `--no-ring`: no sub-terrain ring
//! (TGEN-RING-1; by default `<T>.Heightfield-001` .. `-012` are written). `--water-level`: the water's height in
//! metres (TGEN-VIS-2).
//!
//! `--donor <level>` (round TG2) also writes the visual side from that DICE level of the game copy
//! (`bc2_terragen::visual`, TGEN-BIOME-4): `<T>.Visual.res`, `<T>.MaskTileAtlas.res` and `<T>.Water.res` beside the
//! other terrain resources, and the level bundle's `level/<T>_AmbientOcclusion.res` and `level/Dx9_Single.res`,
//! `Dx10_...`, `Dx11_...` (the donor level's databases with its terrain's entries cloned under `<T>`); the water then
//! lies at the donor's water level unless `--water-level` says otherwise (the donor's rule layers shade by height).
//! The game copy is `--opfor`'s, else `OPFOR_ROOT`'s, else Project OPFOR BFBC2's beside the suite; it is only read.
//!
//! Nothing is written into a game folder. Exit codes: 0 written; 1 refused or failed; 2 usage.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bc2_terragen::{Biome, GENERATOR_VERSION, Pad, Settings, Terrain};
use serde_json::{Value, json};

const USAGE: &str = "usage: bfbc2 terrain generate --biome arid|jungle|snow|temperate|vietnam --size 1024|2048 \
                     --seed <n> --out <folder>\n         [--name <T>] [--border <m>] [--water <share>] \
                     [--pad <x>,<z>[,<radius>]] ... [--erosion <droplets per 1000 samples>]\n         \
                     [--rough <0-1>] [--no-ring] [--water-level <m>] [--donor <level> [--opfor <folder>]]\n\
                     A new terrain from a seed: the main height field and its ring, the material map and the water \
                     mesh;\nwith --donor, its visual side from a DICE level (visual terrain, atlas, visual water, \
                     colour / occlusion map,\nthe shader databases' entries).\n";

fn usage(why: &str) -> ExitCode {
    if !why.is_empty() {
        eprintln!("bfbc2 terrain: {why}");
    }
    eprint!("{USAGE}");
    ExitCode::from(2)
}

fn out(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
}

/// The parsed command line.
struct Args {
    settings: Settings,
    out: PathBuf,
    name: String,
    donor: Option<String>,
    opfor: Option<String>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut biome: Option<Biome> = None;
    let (mut size, mut seed, mut out, mut name) = (None, None, None, None);
    let (mut border, mut water, mut erosion) = (None, None, None);
    let (mut rough, mut level, mut donor, mut opfor, mut ring) = (None, None, None, None, true);
    let mut pads = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or_else(|| format!("{a} needs a value"));
        let num = |v: String| v.parse::<f64>().map_err(|_| format!("{a} {v}: not a number"));
        match a.as_str() {
            "--biome" => biome = Some(val()?.parse()?),
            "--size" => size = Some(val()?.parse::<u32>().map_err(|e| format!("--size: {e}"))?),
            "--seed" => seed = Some(val()?.parse::<u64>().map_err(|e| format!("--seed: {e}"))?),
            "--out" => out = Some(PathBuf::from(val()?)),
            "--name" => name = Some(val()?),
            "--border" => border = Some(num(val()?)?),
            "--water" => water = Some(num(val()?)?),
            "--erosion" => erosion = Some(val()?.parse::<u32>().map_err(|e| format!("--erosion: {e}"))?),
            "--rough" => rough = Some(num(val()?)?),
            "--water-level" => level = Some(num(val()?)?),
            "--donor" => donor = Some(val()?.to_lowercase()),
            "--opfor" => opfor = Some(val()?),
            "--no-ring" => ring = false,
            "--pad" => {
                let text = val()?;
                let parts: Vec<f64> = text
                    .split(',')
                    .map(|part| {
                        part.trim()
                            .parse::<f64>()
                            .map_err(|_| format!("--pad {text}: x,z[,radius]"))
                    })
                    .collect::<Result<_, _>>()?;
                match parts.as_slice() {
                    [x, z] => pads.push(Pad {
                        x: *x,
                        z: *z,
                        radius: 24.0,
                    }),
                    [x, z, r] => pads.push(Pad {
                        x: *x,
                        z: *z,
                        radius: *r,
                    }),
                    _ => return Err(format!("--pad {text}: x,z[,radius]")),
                }
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let biome = biome.ok_or("--biome is required")?;
    let seed = seed.ok_or("--seed is required")?;
    let mut settings = Settings::new(biome, size.ok_or("--size is required")?, seed);
    settings.border_m = border;
    settings.water_share = water;
    settings.erosion = erosion;
    settings.pads = pads;
    settings.rough = rough;
    settings.water_level_m = level;
    settings.ring = ring;
    if rough.is_some_and(|r| !(0.0..=1.0).contains(&r)) {
        return Err("--rough: from 0 to 1".into());
    }
    let name = name.unwrap_or_else(|| format!("TG_{}_{seed}", biome.name().to_uppercase()));
    if name.is_empty() || name.contains(['/', '\\', ':', '.']) {
        return Err(format!("--name {name:?}: a plain name (no path, no dot)"));
    }
    if opfor.is_some() && donor.is_none() {
        return Err("--opfor is for --donor".into());
    }
    Ok(Args {
        settings,
        out: out.ok_or("--out is required")?,
        name,
        donor,
        opfor,
    })
}

fn round(v: f64, k: i32) -> f64 {
    let s = 10f64.powi(k);
    (v * s).round() / s
}

/// `terrain.json`: the seed and settings (enough to generate it again), the generator's version, the files, the
/// height range and water, and the statistics against the biome's DICE medians (keys sorted).
fn record(a: &Args, t: &Terrain, files: &BTreeMap<String, usize>) -> Value {
    let s = t.shape();
    let g = a.settings.biome.targets();
    let pads: Vec<Value> = a
        .settings
        .pads
        .iter()
        .map(|p| json!({"radius": p.radius, "x": p.x, "z": p.z}))
        .collect();
    json!({
        "dice_medians": {
            "beta": g.beta, "edge_minus_centre": g.edge_minus_centre, "hi": g.hi, "range_m": g.range_m,
            "rough64": g.rough64, "slope": g.slope, "wet": g.wet,
        },
        "files": files,
        "generator_version": GENERATOR_VERSION,
        "name": a.name,
        "range_m": t.height.range_m,
        "settings": {
            "biome": a.settings.biome.name(), "border_m": a.settings.border_m, "erosion": a.settings.erosion,
            "pads": pads, "seed": a.settings.seed, "size": a.settings.size, "water_share": a.settings.water_share,
            "rough": a.settings.rough, "ring": a.settings.ring, "water_level_m": a.settings.water_level_m,
            "donor": a.donor,
        },
        "shape": {
            "beta": round(s.beta, 2), "edge_minus_centre": round(s.edge_minus_centre, 1), "hi": round(s.hi, 3),
            "rough64": round(s.rough64, 2), "rough8": round(s.rough8, 2),
            "slope": s.slope.map(|v| round(v, 1)), "steep": s.steep.map(|v| round(v, 3)), "wet": round(s.wet, 3),
        },
        "water_y": t.height.water_y,
    })
}

/// The visual side from the donor (`--donor`), written beside the terrain resources and into `level`.
fn write_visual(a: &Args, t: &Terrain, d: &DonorFiles, files: &mut BTreeMap<String, usize>) -> Result<(), ExitCode> {
    let v = match bc2_terragen::visual::build(t, &a.name, &d.donor()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("bfbc2 terrain: the visual side from {}: {e}", d.level);
            return Err(ExitCode::from(1));
        }
    };
    let level_dir = a.out.join("level");
    if let Err(e) = std::fs::create_dir_all(&level_dir) {
        eprintln!("bfbc2 terrain: {}: {e}", level_dir.display());
        return Err(ExitCode::from(1));
    }
    for (dir, list, prefix) in [(&a.out, &v.terrain, ""), (&level_dir, &v.level, "level/")] {
        for r in list {
            let file = r.name.rsplit('/').next().unwrap_or(&r.name).to_owned();
            if let Err(e) = bc2_gamefs::atomic::write(&dir.join(&file), &[&r.data]) {
                eprintln!("bfbc2 terrain: {file}: {e}");
                return Err(ExitCode::from(1));
            }
            out(&format!(
                "{prefix}{file}: {} bytes ({}, {})",
                r.data.len(),
                r.type_name,
                r.name
            ));
            files.insert(format!("{prefix}{file}"), r.data.len());
        }
    }
    for n in &v.notes {
        out(n);
    }
    Ok(())
}

fn generate(args: &[String]) -> ExitCode {
    let mut a = match parse(args) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let donor = match a
        .donor
        .as_deref()
        .map(|lv| DonorFiles::read(a.opfor.as_deref(), lv))
        .transpose()
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("bfbc2 terrain: --donor: {e}");
            return ExitCode::from(1);
        }
    };
    if let Some(d) = &donor
        && a.settings.water_level_m.is_none()
    {
        a.settings.water_level_m = d.water_level;
    }
    let t = match Terrain::new(&a.settings) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("bfbc2 terrain: {e}");
            return ExitCode::from(1);
        }
    };
    let res = match t.resources(&a.name) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("bfbc2 terrain: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = std::fs::create_dir_all(&a.out) {
        eprintln!("bfbc2 terrain: {}: {e}", a.out.display());
        return ExitCode::from(1);
    }
    let mut files = BTreeMap::new();
    for r in &res {
        let file = r.name.rsplit('/').next().unwrap_or(&r.name).to_owned();
        if let Err(e) = bc2_gamefs::atomic::write(&a.out.join(&file), &[&r.data]) {
            eprintln!("bfbc2 terrain: {file}: {e}");
            return ExitCode::from(1);
        }
        out(&format!("{file}: {} bytes ({})", r.data.len(), r.type_name));
        files.insert(file, r.data.len());
    }
    if let Some(d) = &donor
        && let Err(code) = write_visual(&a, &t, d, &mut files)
    {
        return code;
    }
    let rec = record(&a, &t, &files);
    let text = serde_json::to_string_pretty(&rec).unwrap_or_default() + "\n";
    if let Err(e) = bc2_gamefs::atomic::write(&a.out.join("terrain.json"), &[text.as_bytes()]) {
        eprintln!("bfbc2 terrain: terrain.json: {e}");
        return ExitCode::from(1);
    }
    let s = t.shape();
    out(&format!(
        "{} {} m, seed {}: range {} m, water {}; slope p10/50/90/99 {:.1} / {:.1} / {:.1} / {:.1} deg, HI {:.3}, \
         beta {:.2}, relief 66 m {:.2} m, border - centre {:.1} m, under water {:.3}",
        a.settings.biome,
        a.settings.size,
        a.settings.seed,
        t.height.range_m,
        t.height
            .water_y
            .map_or_else(|| "none".to_owned(), |w| format!("{w:.2} m")),
        s.slope[0],
        s.slope[1],
        s.slope[2],
        s.slope[3],
        s.hi,
        s.beta,
        s.rough64,
        s.edge_minus_centre,
        s.wet
    ));
    out(&format!("written to {}", a.out.display()));
    ExitCode::SUCCESS
}

/// A donor level's files read from the game copy (round TG2, TGEN-BIOME-4).
struct DonorFiles {
    level: String,
    terrain: String,
    visual: Vec<u8>,
    water: Option<Vec<u8>>,
    atlas: Option<Vec<u8>>,
    occlusion: Option<(String, Vec<u8>)>,
    databases: Vec<(u32, Vec<u8>)>,
    water_level: Option<f64>,
}

/// The entries of a bundle that `keep` takes (by name and type), by lower-case name.
fn bundle_entries(path: &Path, keep: &dyn Fn(&str, &str) -> bool) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let h = bc2_formats::fbrb::entries_path(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let names: Vec<String> = h
        .entries
        .iter()
        .filter(|e| !e.is_deleted() && keep(&e.name, &e.type_name))
        .map(|e| e.name.clone())
        .collect();
    let data = bc2_formats::fbrb::read_path_uncached(path, &names).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(names.into_iter().map(|n| n.to_lowercase()).zip(data).collect())
}

impl DonorFiles {
    fn read(opfor: Option<&str>, level: &str) -> Result<Self, String> {
        let here = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let starts: Vec<&Path> = here.iter().map(PathBuf::as_path).collect();
        let root = bc2_gamefs::game::find_opfor(opfor, true, bc2_gamefs::game::Beside::Above(&starts))
            .map_err(|e| e.to_string())?;
        let levels = root.join("game").join("Dist").join("win32").join("levels");
        let bundle = |lv: &str, b: &str| levels.join(lv).join(b);
        let terr = bundle_entries(&bundle(level, "terrain-00.fbrb"), &|n, _| {
            n.to_lowercase().starts_with("terrains/")
        })?;
        let (vt_name, visual) = terr
            .iter()
            .find(|(n, _)| n.ends_with(".visual.res"))
            .map(|(n, d)| (n.clone(), d.clone()))
            .ok_or_else(|| {
                format!("{level}'s terrain bundle has no visual terrain (a mode level: name its base map)")
            })?;
        let low = vt_name.split('/').nth(1).unwrap_or_default().to_owned();
        let one = |suffix: &str| terr.get(&format!("terrains/{low}/{low}{suffix}")).cloned();
        let vt = bc2_formats::visual_terrain::VisualTerrain::decode(&visual).map_err(|e| e.to_string())?;
        // the terrain's name as DICE spells it: its patch shaders' folder
        let terrain = vt
            .shaders
            .first()
            .and_then(|s| s.name.split('/').nth(1))
            .map_or_else(|| low.to_uppercase(), str::to_owned);
        let water_level = one(".water.mesh.res")
            .and_then(|m| bc2_formats::water::parse_mesh(&m).ok())
            .and_then(|m| {
                let mut ys: Vec<f32> = m.vertices.iter().map(|v| v[1]).collect();
                ys.sort_by(f32::total_cmp);
                ys.get(ys.len() / 2).map(|y| f64::from(*y))
            });
        let lvl = bundle_entries(&bundle(level, "level-00.fbrb"), &|_, t| t.ends_with("ShaderDatabase"))?;
        let mut databases = Vec::new();
        for dx in [9u32, 10, 11] {
            let want = format!("dx{dx}_single.res");
            let d = lvl
                .iter()
                .find(|(n, _)| n.ends_with(&want))
                .map(|(_, d)| d.clone())
                .ok_or_else(|| format!("{level} has no Dx{dx} shader database"))?;
            databases.push((dx, d));
        }
        let occ_name = databases.first().and_then(|(_, d)| {
            let o = bc2_formats::shaderdb::open::open(d, Some(9), Some(false), "dx9").ok()?;
            bc2_terragen::visual::occlusion_name(&o.db, &terrain)
        });
        let mut occlusion = None;
        if let Some(n) = &occ_name {
            let want = format!("{}.res", n.to_lowercase());
            for b in [
                bundle(level, "level-00.fbrb"),
                bundle("mp_common", "level-00.fbrb"),
                bundle("sp_common", "level-00.fbrb"),
            ] {
                if !b.is_file() {
                    continue;
                }
                if let Some(d) = bundle_entries(&b, &|e, _| e.to_lowercase() == want)?
                    .into_values()
                    .next()
                {
                    occlusion = Some((n.clone(), d));
                    break;
                }
            }
        }
        Ok(Self {
            level: level.to_owned(),
            water: one(".water.res"),
            atlas: one(".masktileatlas.res"),
            terrain,
            visual,
            occlusion,
            databases,
            water_level,
        })
    }

    fn donor(&self) -> bc2_terragen::visual::Donor<'_> {
        bc2_terragen::visual::Donor {
            terrain: self.terrain.clone(),
            visual: &self.visual,
            water: self.water.as_deref(),
            atlas: self.atlas.as_deref(),
            occlusion: self.occlusion.as_ref().map(|(n, d)| (n.clone(), d.as_slice())),
            databases: self.databases.iter().map(|(dx, d)| (*dx, d.as_slice())).collect(),
        }
    }
}

/// `bfbc2 terrain <command> ...`.
pub fn main(args: &[String]) -> ExitCode {
    match args.split_first() {
        Some((c, rest)) if c == "generate" => generate(rest),
        Some((c, _)) if c == "-h" || c == "--help" => {
            out(USAGE.trim_end());
            ExitCode::SUCCESS
        }
        _ => usage(""),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn arguments() {
        let a = parse(&strs(&[
            "--biome", "Snow", "--size", "1024", "--seed", "9", "--out", "x", "--pad", "10,-20", "--pad", "1,2,30",
            "--water", "0.1",
        ]))
        .unwrap();
        assert_eq!(a.name, "TG_SNOW_9");
        assert_eq!(a.settings.biome, Biome::Snow);
        assert_eq!(
            a.settings.pads[1],
            Pad {
                x: 1.0,
                z: 2.0,
                radius: 30.0
            }
        );
        assert_eq!(a.settings.water_share, Some(0.1));
        assert!(
            parse(&strs(&[
                "--biome", "moon", "--size", "1024", "--seed", "1", "--out", "x"
            ]))
            .is_err()
        );
        assert!(parse(&strs(&["--biome", "arid", "--seed", "1", "--out", "x"])).is_err());
        assert!(
            parse(&strs(&[
                "--biome", "arid", "--size", "1024", "--seed", "1", "--out", "x", "--name", "a/b"
            ]))
            .is_err()
        );
        assert!(parse(&strs(&["--pad", "1"])).is_err());
        let b = parse(&strs(&[
            "--biome",
            "arid",
            "--size",
            "1024",
            "--seed",
            "1",
            "--out",
            "x",
            "--no-ring",
            "--rough",
            "0.5",
            "--water-level",
            "58",
            "--donor",
            "MP_005",
        ]))
        .unwrap();
        assert!(!b.settings.ring);
        assert_eq!((b.settings.rough, b.settings.water_level_m), (Some(0.5), Some(58.0)));
        assert_eq!(b.donor.as_deref(), Some("mp_005"));
        assert!(
            parse(&strs(&[
                "--biome", "arid", "--size", "1024", "--seed", "1", "--out", "x", "--rough", "2"
            ]))
            .is_err()
        );
    }
}
