//! `bfbc2 terrain generate`: a new terrain from a seed ([`bc2_terragen`], `spec/transforms/terrain_gen.md`): the main
//! height field shaped to a biome of DICE's, its material map and its water mesh written into a folder, with a
//! `terrain.json` that records the seed, the settings, the generator's version and the terrain's statistics against
//! the biome's DICE medians.
//!
//! ```text
//! bfbc2 terrain generate --biome <b> --size 1024|2048 --seed <n> --out <folder>
//!                        [--name <T>] [--border <m>] [--water <share>] [--pad <x>,<z>[,<r>]] ... [--erosion <k>]
//! ```
//!
//! `--name`: the terrain's name (default `TG_<BIOME>_<seed>`): the files are `<T>.Heightfield-000.res`,
//! `<T>-MaterialMap.res`, `<T>.Water.Mesh.res` (when ground lies under water) and `<T>.res`, the terrain bundle's
//! names under `Terrains/<T>/` (TERR-NAME-1). `--border`: the border ring's median height minus the centre's in metres
//! (negative: a sea edge). `--water`: the share of ground under water (0 - 1). `--pad`: a site levelled for an
//! objective, world metres (radius 24 by default). `--erosion`: droplets per 1,000 samples (default 125).
//!
//! Nothing is written into a game folder. Exit codes: 0 written; 1 refused or failed; 2 usage.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use bc2_terragen::{Biome, GENERATOR_VERSION, Pad, Settings, Terrain};
use serde_json::{Value, json};

const USAGE: &str = "usage: bfbc2 terrain generate --biome arid|jungle|snow|temperate|vietnam --size 1024|2048 \
                     --seed <n> --out <folder>\n         [--name <T>] [--border <m>] [--water <share>] \
                     [--pad <x>,<z>[,<radius>]] ... [--erosion <droplets per 1000 samples>]\n\
                     A new terrain from a seed: the main height field, the material map and the water mesh.\n";

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
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut biome: Option<Biome> = None;
    let (mut size, mut seed, mut out, mut name) = (None, None, None, None);
    let (mut border, mut water, mut erosion) = (None, None, None);
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
    let name = name.unwrap_or_else(|| format!("TG_{}_{seed}", biome.name().to_uppercase()));
    if name.is_empty() || name.contains(['/', '\\', ':', '.']) {
        return Err(format!("--name {name:?}: a plain name (no path, no dot)"));
    }
    Ok(Args {
        settings,
        out: out.ok_or("--out is required")?,
        name,
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
        },
        "shape": {
            "beta": round(s.beta, 2), "edge_minus_centre": round(s.edge_minus_centre, 1), "hi": round(s.hi, 3),
            "rough64": round(s.rough64, 2), "rough8": round(s.rough8, 2),
            "slope": s.slope.map(|v| round(v, 1)), "steep": s.steep.map(|v| round(v, 3)), "wet": round(s.wet, 3),
        },
        "water_y": t.height.water_y,
    })
}

fn generate(args: &[String]) -> ExitCode {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
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
    }
}
