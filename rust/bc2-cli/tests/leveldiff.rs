//! `bfbc2 leveldiff` against LevelDiff.exe's answers, recorded before it was retired (round EX1, `rust/BENCHMARKS.md`
//! "EX1"): the same stdout, stderr, JSON file and exit code, byte for byte.
//!
//! - Two small levels made from XML here (LevelDiff's own test levels: a changed field, a placed object re-created
//!   under a new GUID, reference lists with entries added, removed or reordered, instances added and removed): every
//!   switch and the bad input, against LevelDiff.exe's full texts (`tests/data/leveldiff/synth`).
//! - Real level pairs of OPFOR's game copy and the E3d export stages (stock against export, mode variants, a WW2 level
//!   against its rebuild): against the digests of LevelDiff.exe's answers (`tests/data/leveldiff/leveldiff_golden.tsv`;
//!   no game content in the repository). Skipped where the game copy or a stage is not there.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::panic,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const EXE: &str = env!("CARGO_BIN_EXE_bfbc2");

fn data() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("leveldiff")
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_leveldiff_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The log file's path in a message as `<LOG>` (LevelDiff's `ToolLog` file), the folder of the pairs as `<PAIRS>`.
fn normalise(text: &[u8], pairs: &Path) -> Vec<u8> {
    let s = String::from_utf8_lossy(text).into_owned();
    let s = s.replace(&pairs.display().to_string(), "<PAIRS>");
    let re = regex::Regex::new(r"[A-Za-z]:\\[^\r\n()]*?\\leveldiff-\d{4}-\d{2}-\d{2}(\.\d+)?\.log").unwrap();
    re.replace_all(&s, "<LOG>").into_owned().into_bytes()
}

fn sha1(b: &[u8]) -> String {
    sha1_smol::Sha1::from(b).digest().to_string()
}

/// One run: (code, stdout, stderr, the JSON file's bytes).
fn run(dir: &Path, args: &[String], json: &Path) -> (i32, Vec<u8>, Vec<u8>, Option<Vec<u8>>) {
    let _ = std::fs::remove_file(json);
    let o = Command::new(EXE)
        .arg("leveldiff")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        o.status.code().unwrap_or(-1),
        o.stdout,
        o.stderr,
        std::fs::read(json).ok(),
    )
}

#[test]
fn the_test_levels_as_leveldiff_exe_answered() {
    let dir = temp("synth");
    let src = data().join("synth");
    for n in ["a", "b"] {
        std::fs::copy(src.join(format!("{n}.xml")), dir.join(format!("{n}.xml"))).unwrap();
        let o = Command::new(EXE)
            .args(["dbxedit", "import", &format!("{n}.xml"), &format!("{n}.dbx")])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(dir.join(format!("{n}.dbx")).is_file(), "{o:?}");
    }
    let cases: [(&str, &[&str]); 9] = [
        ("self", &["a.dbx", "a.dbx", "--json", "J"]),
        ("plain", &["a.dbx", "b.dbx", "--json", "J"]),
        ("match", &["a.dbx", "b.dbx", "--match-objects", "--json", "J"]),
        ("ignore", &["a.dbx", "b.dbx", "--ignore", "Enumeration", "--json", "J"]),
        ("type", &["a.dbx", "b.dbx", "--type", "SoldierSpawn", "--json", "J"]),
        ("summary", &["a.dbx", "b.dbx", "--summary"]),
        ("all", &["a.dbx", "b.dbx", "--all", "--match-objects"]),
        ("usage", &["a.dbx"]),
        ("xml", &["a.dbx", "a.xml"]),
    ];
    for (name, args) in cases {
        let json = dir.join(format!("{name}.json"));
        let args: Vec<String> = args
            .iter()
            .map(|a| {
                if *a == "J" {
                    format!("{name}.json")
                } else {
                    (*a).to_owned()
                }
            })
            .collect();
        let (code, out, err, j) = run(&dir, &args, &json);
        let want = |ext: &str| std::fs::read(src.join(format!("{name}.{ext}"))).ok();
        let want_code: i32 = String::from_utf8(want("code").unwrap())
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(code, want_code, "{name}: exit code");
        assert_eq!(
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&want("out").unwrap()),
            "{name}: stdout"
        );
        assert_eq!(
            normalise(&err, &dir),
            normalise(&want("err").unwrap(), &dir),
            "{name}: stderr"
        );
        assert_eq!(j, want("json"), "{name}: the JSON file");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// The file a golden `file` line names: (bundle path, entry).
fn source(game: &Path, where_: &str, bundle: &str) -> PathBuf {
    let opfor = game.parent().unwrap();
    match where_ {
        "game" => {
            let (root, rest) = match bundle.split_once(':') {
                Some(("package", r)) => (game.join("Package"), r),
                Some((_, r)) => (game.join("Dist").join("win32"), r),
                None => (game.join("Dist").join("win32"), bundle),
            };
            rest.split('/').fold(root, |p, s| p.join(s))
        }
        "stage" => opfor
            .join("work")
            .join("mapedit")
            .join(bundle)
            .join("stage")
            .join("level-00.fbrb"),
        _ => bundle.split('/').fold(opfor.join("work"), |p, s| p.join(s)),
    }
}

#[test]
fn real_level_pairs_as_leveldiff_exe_answered() {
    let Some(game) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let golden = std::fs::read_to_string(data().join("leveldiff_golden.tsv")).unwrap();
    let pairs = temp("pairs");
    let mut have = std::collections::HashSet::new();
    for l in golden.lines().filter(|l| l.starts_with("file\t")) {
        let f: Vec<&str> = l.split('\t').collect();
        let b = source(&game, f[2], f[3]);
        if !b.is_file() {
            println!("not there: {} ({})", f[1], b.display());
            continue;
        }
        let names = bc2_formats::fbrb::entries_path(&b).unwrap();
        let entry = names
            .entries
            .iter()
            .find(|e| !e.is_deleted() && e.name.eq_ignore_ascii_case(f[4]))
            .map(|e| e.name.clone())
            .unwrap();
        let data = bc2_gamefs::bundles::read_path(&b, &[&entry]).unwrap();
        std::fs::write(pairs.join(f[1]), &data[0]).unwrap();
        have.insert(f[1].to_owned());
    }
    std::fs::write(pairs.join("not_binary.xml"), "<?xml version=\"1.0\"?>\n<partition />\n").unwrap();
    let (mut ran, mut skipped) = (0, 0);
    for l in golden.lines().filter(|l| l.starts_with("case\t")) {
        let f: Vec<&str> = l.split('\t').collect();
        let (name, code) = (f[1], f[2].parse::<i32>().unwrap());
        let args: Vec<String> = f[6..].iter().map(|a| (*a).to_owned()).collect();
        let needs = args.iter().filter(|a| {
            Path::new(a.as_str())
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dbx"))
                && !a.starts_with("no_such")
        });
        if needs.clone().any(|a| !have.contains(a)) {
            skipped += 1;
            continue;
        }
        let json = pairs.join(format!("{name}.result.json"));
        let args: Vec<String> = args
            .into_iter()
            .map(|a| if a == "<json>" { json.display().to_string() } else { a })
            .collect();
        let (c, out, err, j) = run(&pairs, &args, &json);
        assert_eq!(c, code, "{name}: exit code");
        assert_eq!(sha1(&out), f[3], "{name}: stdout");
        assert_eq!(
            sha1(&normalise(&err, &pairs)),
            f[4],
            "{name}: stderr {}",
            String::from_utf8_lossy(&err)
        );
        assert_eq!(
            j.map_or_else(|| "-".to_owned(), |b| sha1(&b)),
            f[5],
            "{name}: the JSON file"
        );
        ran += 1;
    }
    println!("leveldiff: {ran} recorded cases identical to LevelDiff.exe's, {skipped} skipped");
    assert!(ran > 0 || have.is_empty());
    let _ = std::fs::remove_dir_all(&pairs);
}
