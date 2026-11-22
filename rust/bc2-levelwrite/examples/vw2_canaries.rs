//! Round VW2's export canaries (`EDITOR_ARCHITECTURE.md` 3.12, "Round VW2"): a project per case made as the editor
//! makes one - its target (era and mode) the first operation, its weather (`Op::SetWeather`) the second - built
//! into `<stages>\<level>\stage` and checked with `mapexport check`. Each case is also built WITHOUT its weather
//! operation (the baseline), and the two stages are compared: every entry of the baseline's bundles is in the
//! weather stage byte for byte, the level partition differs only in the fields the weather sets (the spawn switch's
//! `State`, the weather entity's references or its placement's `ReferencedObject`), and every other entry is one of
//! the weather's new partitions under `weather/<level>/`, each of which is its source partition with only the
//! intended changes (guids, names, `ActiveState`, the appended state, the night's two values). Nothing in the game
//! copy is written.
//!
//! `cargo run --release -p bc2-levelwrite --example vw2_canaries -- <game> <projects> <stages> [case ...]`
#![allow(
    clippy::print_stdout,
    clippy::many_single_char_names,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::panic
)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bc2_formats::dbx::edit::EditDbx;
use bc2_levelwrite::export::{self, ExportSpec, Game, Identity};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::ops::Op;
use bc2_levelwrite::weather::{Borrow, Night, WeatherChoice};
use bc2_levelwrite::{BaseKind, MapProject, Profile};

struct Case {
    id: &'static str,
    base: &'static str,
    profile: Profile,
    level: &'static str,
    title: &'static str,
    weather: fn() -> WeatherChoice,
}

fn own(state: usize, name: &str) -> WeatherChoice {
    WeatherChoice {
        state,
        name: name.into(),
        borrow: None,
        night: None,
        unseen_ok: false,
    }
}

const CASES: [Case; 6] = [
    Case {
        id: "state",
        base: "mp_002",
        profile: Profile::Bfbc2,
        level: "MAPEDIT_VW2_ARICA_DAWN",
        title: "Arica Harbor at dawn",
        weather: || own(6, "ons_mp_002_dawn"),
    },
    Case {
        id: "state_wake",
        base: "wake_island_s",
        profile: Profile::Ww2,
        level: "MAPEDIT_VW2_WAKE_IWO",
        title: "Wake Island under Iwo Jima's sky",
        weather: || own(1, "iwo_jima"),
    },
    Case {
        id: "borrow_append",
        base: "mp_005",
        profile: Profile::Bfbc2,
        level: "MAPEDIT_VW2_ATACAMA_FOG",
        title: "Atacama in fog",
        weather: || WeatherChoice {
            borrow: Some(Borrow {
                donor: "mp_008cq".into(),
                state: "weather/r_mp008_cq/states/mp_008_ons_fog".into(),
                swap: false,
            }),
            ..own(0, "mp_008_ons_fog")
        },
    },
    Case {
        id: "borrow_swap",
        base: "mp_005",
        profile: Profile::Bfbc2,
        level: "MAPEDIT_VW2_ATACAMA_ARICA_SKY",
        title: "Atacama under Arica's dawn",
        weather: || WeatherChoice {
            borrow: Some(Borrow {
                donor: "mp_002".into(),
                state: "weather/r_mp02/states/ons_mp_002_dawn".into(),
                swap: true,
            }),
            ..own(6, "ons_mp_002_dawn")
        },
    },
    Case {
        id: "night",
        base: "mp_005",
        profile: Profile::Bfbc2,
        level: "MAPEDIT_VW2_ATACAMA_NIGHT",
        title: "Atacama by night",
        weather: || WeatherChoice {
            night: Some(Night::default()),
            unseen_ok: true,
            ..own(0, "ons_mp_002_night")
        },
    },
    Case {
        id: "rain",
        base: "mp_005",
        profile: Profile::Bfbc2,
        level: "MAPEDIT_VW2_ATACAMA_RAIN",
        title: "Atacama after rain",
        weather: || WeatherChoice {
            borrow: Some(Borrow {
                donor: "sp_03".into(),
                state: "weather/r_sp03/states/afterrain".into(),
                swap: false,
            }),
            unseen_ok: true,
            ..own(0, "afterrain")
        },
    },
];

fn project(game: &Game, c: &Case) -> MapProject {
    let base = export::level_partition(game, c.base).unwrap();
    let targets = game.targets(&format!("levels/{}/level-00.fbrb", c.base));
    let package = game
        .bundle(&format!("package:levels/{}/level-00.fbrb", c.base))
        .is_file();
    let mut p = MapProject::new(
        c.title,
        c.base,
        &format!("levels/{}", c.base.to_ascii_uppercase()),
        &base,
        Profile::of_stack(c.base, &targets, package),
    );
    p.base.kind = Some(BaseKind::Multiplayer);
    let choice = export::choose_mode(game, &p, c.profile, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(choice) }, "New project: era and mode");
    p
}

fn build(game: &Game, p: &MapProject, c: &Case, projects: &Path, stage: &Path, level: &str) -> export::StageRecord {
    let path = projects.join(format!("{}.mapedit.json", level.to_ascii_lowercase()));
    p.save(&path).unwrap();
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: level.into(),
        folder: String::new(),
        title: c.title.into(),
        mode: "Conquest".into(),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: c.profile,
        identity: Some(Identity {
            description: format!("{} with another weather (round VW2)", c.base.to_ascii_uppercase()),
            picture: String::new(),
            ..Identity::default()
        }),
        path_data: false,
    };
    std::fs::write(
        export::spec_path_of(&path),
        serde_json::to_string_pretty(&spec).unwrap() + "\n",
    )
    .unwrap();
    export::build(game, p, &path.display().to_string(), &spec, stage).unwrap()
}

fn entries(stage: &Path, file: &str) -> BTreeMap<String, Vec<u8>> {
    let (h, data) = bc2_formats::fbrb::extract_path_uncached(stage.join(file)).unwrap();
    h.entries
        .iter()
        .zip(data)
        .map(|(e, d)| (e.name.to_ascii_lowercase(), d))
        .collect()
}

/// A partition's XML with its guids replaced by their order of first appearance, the guid-like tails of references
/// too, and the renamed paths mapped back: what is left differs only where the export meant it to.
fn normal(data: &[u8], back: &HashMap<String, String>) -> String {
    let mut x = bc2_formats::dbx::to_xml(data).unwrap();
    // longest names first, then by name: a name inside another is replaced after it, the same way on every run
    let mut back: Vec<(&String, &String)> = back.iter().collect();
    back.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(b.0)));
    for (new, old) in back {
        x = x.replace(new.as_str(), old);
        x = x.replace(&title(new), &title(old));
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = String::new();
    let b = x.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let is_guid = b.len() >= i + 36
            && b[i..i + 36].iter().enumerate().all(|(k, c)| {
                if [8, 13, 18, 23].contains(&k) {
                    *c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            });
        if is_guid {
            let g = x[i..i + 36].to_ascii_uppercase();
            let n = seen.len();
            let k = *seen.entry(g).or_insert(n);
            let _ = write!(out, "<g{k}>");
            i += 36;
        } else {
            out.push(char::from(b[i]));
            i += 1;
        }
    }
    out
}

fn title(p: &str) -> String {
    p.split('/').map(str::to_ascii_uppercase).collect::<Vec<_>>().join("/")
}

/// The lines of `b` that differ from `a` (`a`'s line, `b`'s line), case aside; a line only `b` has (an item
/// appended) is ("", line).
fn lines_differing(a: &str, b: &str) -> Vec<(String, String)> {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < la.len() || j < lb.len() {
        let (x, y) = (la.get(i).copied().unwrap_or(""), lb.get(j).copied().unwrap_or(""));
        if x.eq_ignore_ascii_case(y) {
            i += 1;
            j += 1;
        } else if lb.get(j + 1).is_some_and(|n| n.eq_ignore_ascii_case(x)) || i >= la.len() {
            out.push((String::new(), y.trim().to_owned()));
            j += 1;
        } else {
            out.push((x.trim().to_owned(), y.trim().to_owned()));
            i += 1;
            j += 1;
        }
    }
    out
}

fn run(game: &Game, c: &Case, projects: &Path, stages: &Path) -> bool {
    println!("==== {} - {} ({})", c.id, c.base, (c.weather)().describe());
    let t = std::time::Instant::now();
    let p0 = project(game, c);
    let base_stage = stages
        .join(format!("{}_baseline", c.level.to_ascii_lowercase()))
        .join("stage");
    let base_level = c.level.to_owned();
    // the baseline: the same project without its weather, the same id, staged in another folder
    let r0 = build(game, &p0, c, projects, &base_stage, &base_level);
    let mut p = p0.clone();
    let w = (c.weather)();
    p.push(
        Op::SetWeather {
            to: Some(Box::new(w.clone())),
        },
        "VW2: weather",
    );
    // a night / rain export not turned on is refused
    if w.unseen() {
        let mut off = p0.clone();
        off.push(
            Op::SetWeather {
                to: Some(Box::new(WeatherChoice {
                    unseen_ok: false,
                    ..w.clone()
                })),
            },
            "VW2: weather, not turned on",
        );
        let path = projects.join("refused.mapedit.json");
        let _ = &path;
        off.save(&path).unwrap();
        let spec = ExportSpec {
            level: format!("{}_R", c.level),
            ..serde_json::from_str(
                &std::fs::read_to_string(export::spec_path_of(
                    &projects.join(format!("{}.mapedit.json", base_level.to_ascii_lowercase())),
                ))
                .unwrap(),
            )
            .unwrap()
        };
        match export::build(game, &off, "refused", &spec, &stages.join("refused")) {
            Err(e) => println!("  refused while not turned on: {e}"),
            Ok(_) => panic!("{}: a night / rain export built without being turned on", c.id),
        }
        let _ = std::fs::remove_dir_all(stages.join("refused"));
    }
    let stage = stages.join(c.level.to_ascii_lowercase()).join("stage");
    let rec = build(game, &p, c, &projects.join("weather"), &stage, c.level);
    for l in rec
        .report
        .iter()
        .filter(|l| l.contains("weather") || l.starts_with("  "))
    {
        println!("  | {l}");
    }
    let wr = rec.weather.clone().expect("the stage records its weather");
    // 1. the bundles against the baseline's
    let mut ok = true;
    let (bl, wl) = (r0.level.to_ascii_lowercase(), rec.level.to_ascii_lowercase());
    for file in ["level-00.fbrb", "terrain-00.fbrb", "ingame-00.fbrb", "loader-00.fbrb"] {
        let (a, b) = (entries(&base_stage, file), entries(&stage, file));
        let rename = |n: &str| n.replace(&bl, &wl);
        let mut same = 0;
        let mut changed = Vec::new();
        for (n, d) in &a {
            match b.get(&rename(n)) {
                Some(x) if x == d => same += 1,
                Some(_) => changed.push(n.clone()),
                None => {
                    println!("  FAILED {file}: {n} missing");
                    ok = false;
                }
            }
        }
        let added: Vec<&String> = b.keys().filter(|n| !a.keys().any(|m| rename(m) == **n)).collect();
        // what differs besides the weather: the names (the id differs by `_B`) - compared below for the level
        let expected_changed = |n: &str| n == format!("levels/{bl}.dbx");
        let unexpected: Vec<&String> = changed.iter().filter(|n| !expected_changed(n)).collect();
        // a swap's sky: the closure copies the donor's panorama and cloud textures (`terrains/<donor>/textures/`)
        let swap = w.borrow.as_ref().is_some_and(|x| x.swap);
        let foreign: Vec<&&String> = added
            .iter()
            .filter(|n| !n.starts_with(&format!("weather/{wl}/")))
            .filter(|n| !(swap && n.starts_with("terrains/") && n.contains("/textures/")))
            .collect();
        if swap {
            println!(
                "  {file}: the closure's copies of the donor's sky textures: {:?}",
                added.iter().filter(|n| n.starts_with("terrains/")).collect::<Vec<_>>()
            );
        }
        println!(
            "  {file}: {same} of the baseline's {} entries byte for byte, {} named by the level id, {} new (weather)",
            a.len(),
            changed.len(),
            added.len()
        );
        if !unexpected.is_empty() || !foreign.is_empty() {
            println!("  FAILED {file}: changed {unexpected:?}, new outside weather/{wl}/: {foreign:?}");
            ok = false;
        }
    }
    // 2. the level partition: only the weather's fields differ from the baseline's
    let (a, b) = (entries(&base_stage, "level-00.fbrb"), entries(&stage, "level-00.fbrb"));
    let la = EditDbx::parse(&a[&format!("levels/{bl}.dbx")]).unwrap();
    let lb = EditDbx::parse(&b[&format!("levels/{wl}.dbx")]).unwrap();
    let d = bc2_levelwrite::diff::diff(&la, &lb);
    let fields: Vec<String> = d
        .iter()
        .map(|x| match x {
            bc2_levelwrite::diff::Change::Field(_, f) => f.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    println!(
        "  level partition: {} fields differ from the baseline's: {fields:?}",
        fields.len()
    );
    let allowed = ["State", "WeatherSystem", "Settings", "ReferencedObject"];
    if fields.iter().any(|f| !allowed.contains(&f.as_str())) {
        println!("  FAILED: the level partition changed outside the weather's fields");
        ok = false;
    }
    // 3. each new partition against its source (the record's): only the intended changes
    let mut lives: HashMap<String, HashMap<String, export::Found>> = HashMap::new();
    for (path, why) in &wr.written {
        if Path::new(path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("res"))
        {
            let data = b.get(path).cloned().unwrap_or_default();
            println!(
                "  {path}: {} bytes, value {:?} ({why})",
                data.len(),
                data.get(..4).map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            );
            ok &= data == 72u32.to_le_bytes();
            continue;
        }
        let Some(data) = b.get(&format!("{path}.dbx")) else {
            continue;
        };
        let Some((src, lv)) = wr.source_of(path) else {
            println!("  FAILED {path}: no source recorded");
            ok = false;
            continue;
        };
        let live = lives.entry(lv.to_owned()).or_insert_with(|| {
            let mut db = game.loaded(lv, "level");
            db.extend(game.startup());
            game.live(&db).unwrap()
        });
        let Some(f) = live.get(&format!("{src}.dbx")) else {
            println!("  FAILED {path}: source {src} not in {lv}'s bundles");
            ok = false;
            continue;
        };
        let orig = export::read_found(&[f]).unwrap().pop().unwrap();
        let back: HashMap<String, String> = wr.sources.iter().map(|(n, s, _)| (n.clone(), s.clone())).collect();
        let diffs = lines_differing(&normal(&orig, &HashMap::new()), &normal(data, &back));
        let named: Vec<String> = diffs
            .iter()
            .map(|(x, y)| {
                let field = |s: &str| {
                    s.split("name=\"")
                        .nth(1)
                        .and_then(|r| r.split('"').next())
                        .unwrap_or(s)
                        .to_owned()
                };
                if x.is_empty() { format!("+{y}") } else { field(x) }
            })
            .collect();
        println!("  {path} <- {src} ({lv}): {} lines differ {named:?}", diffs.len());
        let fine = named.iter().all(|n| {
            [
                "ActiveState",
                "WeatherSystem",
                "Settings",
                "ReflectionEnvmapScale",
                "Trancparency",
                "Name",
            ]
            .contains(&n.as_str())
                || n.starts_with("+<item ref=")
        });
        if !fine {
            println!("  FAILED: {path} changed outside the intended fields: {diffs:?}");
            ok = false;
        }
    }
    // 4. the check
    let r = export::check(game, &stage).unwrap();
    for (pass, what) in &r.checks {
        if !pass || what.starts_with("weather") || what.starts_with("closure") || what.starts_with("references") {
            println!("  {} {what}", if *pass { "ok    " } else { "FAILED" });
        }
    }
    println!(
        "  {} checks, {} in {:.0} s: {}",
        r.checks.len(),
        c.level,
        t.elapsed().as_secs_f64(),
        if r.ok() && ok {
            "every check passed"
        } else {
            "A CHECK FAILED"
        }
    );
    let _ = std::fs::remove_dir_all(base_stage.parent().unwrap());
    r.ok() && ok
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = Game::open(&PathBuf::from(&a[1])).expect("game");
    let projects = PathBuf::from(&a[2]);
    let stages = PathBuf::from(&a[3]);
    std::fs::create_dir_all(projects.join("weather")).unwrap();
    let only: Vec<&str> = a.iter().skip(4).map(String::as_str).collect();
    let mut failed = Vec::new();
    for c in &CASES {
        if !only.is_empty() && !only.contains(&c.id) {
            continue;
        }
        if !run(&game, c, &projects, &stages) {
            failed.push(c.id);
        }
    }
    println!("failed: {failed:?}");
    if !failed.is_empty() {
        std::process::exit(1);
    }
}
