//! Round SG1: static group members edited and exported.
//!
//! - `every_stock_group_keeps_the_rules`: the ground truth. Every model group of the levels read (BFBC2's Dist and
//!   Package levels, Vietnam, BFBC1, the four WW2 levels) keeps [`members::check_group`]'s rules (its file's members
//!   as its member types count them, leaves unique and in the client tree, cumulative parts and network ids, the
//!   physics' part words and materials); its physics resource reads and writes back byte for byte, and laid out again
//!   for its own layout it is the same bytes.
//! - `an_export_deletes_moves_and_adds_members`: Wake Island (WW2) and Atacama Desert (`MP_005`) exported with one
//!   member deleted, one moved and two added to the static group: the group file and the partition in DICE's layout
//!   (`mapexport check`'s group lines and every other line pass), the deleted member's leaf reused by the first
//!   added one and a new client leaf for the second (the exclusive tree's slots moved up by one), the other groups'
//!   files the same bytes as an export of the same project without the member edits.
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); writes only into a temporary folder (or
//! `BC2_MEMBER_EDIT_STAGE\<level>`, kept for the install round trip); skipped without a game copy.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    missing_docs
)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::files::{FileEdit, Member};
use bc2_levelwrite::members::{self, GroupPhysics};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::{LevelWriter, MapProject, Op, Profile};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// A level's own files (`levels/<lv>/...`, lower case) from its level bundles.
fn files(game: &Game, lv: &str) -> HashMap<String, Vec<u8>> {
    let live = game.live(&game.level_bundles(lv, "level")).unwrap();
    let want: Vec<(&String, &export::Found)> = live
        .iter()
        .filter(|(n, _)| {
            n.starts_with(&format!("levels/{lv}/"))
                && (n.ends_with("_havokasset.dbx")
                    || n.ends_with("_havokasset.res")
                    || n.ends_with("staticculltree_client.dbx"))
                || **n == format!("levels/{lv}.dbx")
        })
        .collect();
    let found: Vec<&export::Found> = want.iter().map(|x| x.1).collect();
    let data = export::read_found(&found).unwrap();
    want.iter().map(|x| x.0.clone()).zip(data).collect()
}

const LEVELS: [&str; 16] = [
    "mp_001",
    "mp_002",
    "mp_005",
    "mp_008",
    "mp_012gr",
    "sp_01",
    "sp_005",
    "sp_009",
    "wake_island_s",
    "guadal_canal",
    "iwo_jima_s",
    "coral_sea",
    "nam_mp_002cq",
    "nam_mp_005cq",
    "bc1_oasis_cq",
    "bc1_harvest_day_cq",
];

#[test]
#[ignore = "game tier: every model group of 16 levels; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn every_stock_group_keeps_the_rules() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (mut groups, mut members_n, mut rules) = (0usize, 0usize, 0usize);
    for lv in LEVELS {
        if game.level_bundles(lv, "level").is_empty() {
            println!("{lv}: not installed");
            continue;
        }
        let f = files(&game, lv);
        let w = LevelWriter::parse(&f[&format!("levels/{lv}.dbx")]).unwrap();
        let tree =
            bc2_formats::dbx::edit::EditDbx::parse(&f[&format!("levels/{lv}/staticculltree_client.dbx")]).unwrap();
        let leaves = bc2_levelwrite::culltree::CullTree::read(&tree).unwrap().leaf_count();
        let mut here = 0;
        for g in members::group_entities(&w) {
            let file = members::file_of(&g.name);
            let Some(gf) = f.get(&format!("levels/{lv}/{file}")) else {
                continue;
            };
            let gw = LevelWriter::parse(gf).unwrap();
            let res = f.get(&format!("levels/{lv}/{}.res", file.trim_end_matches(".dbx")));
            for (ok, what) in members::check_group(&w, &g, &gw, res.map(Vec::as_slice), Some(leaves)) {
                assert!(ok, "{lv}: {what}");
                rules += 1;
            }
            let rs = members::runs(&w, &g).unwrap();
            let counts: Vec<usize> = rs.iter().map(|r| r.count).collect();
            here += counts.iter().sum::<usize>();
            if let Some(res) = res {
                let p = GroupPhysics::read(res).unwrap();
                assert_eq!(&p.write(), res, "{lv} {file}: read and written again");
                let lay = members::layout(&w, &g).unwrap();
                assert_eq!(
                    &p.relaid(&counts, &lay).unwrap().write(),
                    res,
                    "{lv} {file}: laid out again"
                );
            }
            groups += 1;
        }
        println!("{lv}: {here} members, client tree {leaves} leaves");
        members_n += here;
    }
    println!("{groups} groups, {members_n} members, {rules} rules checked");
    assert!(groups > 40, "{groups}");
}

/// The static group of a level: its file, its instance, and (place, member type size) of every member.
struct Group {
    file: String,
    instance: String,
    /// (first place, count) of each member type.
    runs: Vec<(usize, usize)>,
    base: LevelWriter,
}

fn static_group(game: &Game, lv: &str) -> Group {
    let f = files(game, lv);
    let w = LevelWriter::parse(&f[&format!("levels/{lv}.dbx")]).unwrap();
    let g = members::group_entities(&w)
        .into_iter()
        .find(|g| g.name.to_ascii_lowercase().ends_with("/staticmodelgroupentity"))
        .unwrap();
    let file = members::file_of(&g.name);
    let base = LevelWriter::parse(&f[&format!("levels/{lv}/{file}")]).unwrap();
    let inst = base
        .d
        .instances()
        .into_iter()
        .find(|&e| base.d.child(e, b"Assets").is_some())
        .unwrap();
    let instance = String::from_utf8_lossy(base.d.attr(inst, b"guid").unwrap()).to_ascii_uppercase();
    let runs = members::runs(&w, &g)
        .unwrap()
        .iter()
        .map(|r| (r.first, r.count))
        .collect();
    Group {
        file,
        instance,
        runs,
        base,
    }
}

fn spec(level: &str, p: &MapProject, profile: Profile) -> ExportSpec {
    ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: level.into(),
        folder: String::new(),
        title: format!("SG1 {level}"),
        mode: "Conquest".into(),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile,
        identity: None,
        path_data: false,
    }
}

/// The staged level bundle's entries by lower-case name.
fn staged(stage: &Path) -> HashMap<String, Vec<u8>> {
    let (h, data) = bc2_formats::fbrb::extract_path_uncached(stage.join("level-00.fbrb")).unwrap();
    h.entries
        .iter()
        .zip(data)
        .filter(|(e, _)| !e.is_deleted())
        .map(|(e, d)| (e.name.to_ascii_lowercase(), d))
        .collect()
}

#[test]
#[ignore = "game tier: two whole exports of stock levels with member edits; run_tests.py game runs it"]
fn an_export_deletes_moves_and_adds_members() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let keep = std::env::var_os("BC2_MEMBER_EDIT_STAGE").map(PathBuf::from);
    let tmp = std::env::temp_dir().join(format!("bc2_levelwrite_sg1_{}", std::process::id()));
    for (lv, profile, name) in [
        ("wake_island_s", Profile::Ww2, "MAPEDIT_SG1_WAKE"),
        ("mp_005", Profile::Bfbc2, "MAPEDIT_SG1_ATACAMA"),
    ] {
        if game.level_bundles(lv, "level").is_empty() {
            println!("{lv}: not installed");
            continue;
        }
        let t0 = std::time::Instant::now();
        let base = export::level_partition(&game, lv).unwrap();
        let up = lv.to_ascii_uppercase();
        let mut p = MapProject::new(&format!("SG1 {lv}"), lv, &format!("levels/{up}"), &base, profile);
        let c = export::choose_mode(&game, &p, profile, "conquest", None).unwrap();
        p.push(Op::SetMode { to: Box::new(c) }, "target");
        let plain = p.clone();
        let g = static_group(&game, lv);
        // a member type with at least three members: delete its first, move its second, add two like its third
        let &(first, count) = g.runs.iter().find(|r| r.1 >= 3).unwrap();
        let (del, mv, like) = (first, first + 1, first + 2);
        let rows = |k: usize| bc2_levelwrite::files::member_transform(&g.base, &g.instance, k).unwrap();
        let moved = {
            let mut r = rows(mv);
            r[3][0] += 6.0;
            r
        };
        let near = |dx: f32| {
            let mut r = rows(like);
            r[3][2] += dx;
            r
        };
        let asset = members::asset_at(&g.base, &g.instance, del).unwrap();
        p.push(
            Op::Files {
                edits: vec![
                    FileEdit::MemberDelete {
                        file: g.file.clone(),
                        instance: g.instance.clone(),
                        item: Member::Base(del),
                        asset,
                        rows: rows(del),
                    },
                    FileEdit::MemberTransform {
                        file: g.file.clone(),
                        instance: g.instance.clone(),
                        item: Member::Base(mv),
                        from: rows(mv),
                        to: moved,
                    },
                ],
            },
            "SG1: delete one, move one",
        );
        for (n, dx) in [(1, 8.0), (2, 16.0)] {
            p.push(
                Op::Files {
                    edits: vec![FileEdit::MemberAdd {
                        file: g.file.clone(),
                        instance: g.instance.clone(),
                        item: Member::Added { added: n },
                        like: Member::Base(like),
                        rows: near(dx),
                    }],
                },
                "SG1: add one",
            );
        }
        let root = keep.clone().unwrap_or_else(|| tmp.clone());
        let stage = root.join(name.to_ascii_lowercase()).join("stage");
        let plain_stage = tmp.join(format!("{}_plain", name.to_ascii_lowercase())).join("stage");
        // round A2 (XLAY-PATH-1): with member edits the base's path data is not the level's: the path-data step
        // refuses before anything is built, naming them
        let with_path = ExportSpec {
            path_data: true,
            ..spec(name, &p, profile)
        };
        let e = export::build(&game, &p, "test", &with_path, &tmp.join("path_refused")).unwrap_err();
        println!("{name}: {e}");
        assert!(
            e.to_string().contains("XVAL-AI-1") && e.to_string().contains("static group member"),
            "{e}"
        );
        let rec = export::build(&game, &p, "test", &spec(name, &p, profile), &stage).unwrap();
        let built = t0.elapsed().as_secs_f64();
        let r = export::check(&game, &stage).unwrap();
        for (ok, l) in &r.checks {
            if l.starts_with("static group") || l.starts_with("cull") {
                println!("{} {l}", if *ok { "ok    " } else { "FAILED" });
            }
        }
        assert!(
            r.ok(),
            "{lv}: {:#?}",
            r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>()
        );
        // round V1: the export's rules on the member edits - the stage's group lines (XVAL-GRP-1: SG1's layout and the
        // kinds kept) pass, and the project raises no error live
        let grp: Vec<(&str, bool, &str)> = r.lines().filter(|x| x.0 == "XVAL-GRP-1").collect();
        assert!(grp.len() >= 2 && grp.iter().all(|x| x.1), "{lv}: {grp:#?}");
        let found = bc2_levelwrite::validate::project(&game, &p).unwrap();
        assert!(
            found
                .iter()
                .all(|f| f.severity != bc2_levelwrite::rules::Severity::Error),
            "{lv}: {found:#?}"
        );
        println!(
            "{lv}: V1 - {} XVAL-GRP-1 lines pass; validate: {} findings, no error",
            grp.len(),
            found.len()
        );
        // every member of the group's smallest kind deleted: the last one is refused, under XVAL-GRP-1
        if let Some(&(first_of, n)) = g.runs.iter().min_by_key(|r| r.1) {
            let mut last = plain.clone();
            let edits = (first_of..first_of + n)
                .map(|k| FileEdit::MemberDelete {
                    file: g.file.clone(),
                    instance: g.instance.clone(),
                    item: Member::Base(k),
                    asset: members::asset_at(&g.base, &g.instance, k).unwrap(),
                    rows: rows(k),
                })
                .collect();
            last.push(Op::Files { edits }, "SG1: delete every member of a kind");
            let e = export::build(&game, &last, "test", &spec(name, &last, profile), &tmp.join("refused")).unwrap_err();
            assert!(e.0.starts_with("XVAL-GRP-1: "), "{lv}: {}", e.0);
            println!("{lv}: refused: {}", e.0.chars().take(140).collect::<String>());
        }
        for l in rec
            .report
            .iter()
            .filter(|l| l.contains("static group") || l.contains("_havokasset") || l.contains("client tree"))
        {
            println!("{lv}: {l}");
        }
        assert!(
            rec.report.iter().any(|l| l.contains("1 members moved")
                && l.contains("1 deleted")
                && l.contains("2 added (1 on freed leaves, 1 new leaves")),
            "{lv}: {:#?}",
            rec.report
        );
        assert!(
            rec.report.iter().any(|l| l.contains("EnumOffset")),
            "{lv}: the exclusive tree's slots moved"
        );
        // the staged files: the group in DICE's layout
        let s = staged(&stage);
        let low = name.to_ascii_lowercase();
        let w = LevelWriter::parse(&s[&format!("levels/{low}.dbx")]).unwrap();
        let gs = members::group_entities(&w)
            .into_iter()
            .find(|x| members::file_of(&x.name) == g.file)
            .unwrap();
        let rs = members::runs(&w, &gs).unwrap();
        let ty = rs.iter().find(|r| r.first == first).unwrap();
        assert_eq!(ty.count, count + 1, "{lv}: one deleted, two added");
        let total: usize = rs.iter().map(|r| r.count).sum();
        let base_total: usize = g.runs.iter().map(|r| r.1).sum();
        assert_eq!(total, base_total + 1);
        let gf = LevelWriter::parse(&s[&format!("levels/{low}/{}", g.file)]).unwrap();
        let tr = |k: usize| bc2_levelwrite::files::member_transform(&gf, &g.instance_in(&gf), k).unwrap();
        // places: the deleted member's run shifts down by one; the moved one now at `first`; the added ones at the
        // end of its type's run
        assert_eq!(tr(first), moved, "{lv}: the moved member");
        assert_eq!(tr(first + count - 1), near(8.0), "{lv}: the first added member");
        assert_eq!(tr(first + count), near(16.0), "{lv}: the second added member");
        // the deleted member's leaf went to the first added one; the second has the client tree's new last leaf
        let deleted_leaf = members::enum_at(&g.base, &g.instance, del).unwrap();
        assert_eq!(ty.enums[count - 1], deleted_leaf, "{lv}: the freed leaf reused");
        let leaves = {
            let t =
                bc2_formats::dbx::edit::EditDbx::parse(&s[&format!("levels/{low}/staticculltree_client.dbx")]).unwrap();
            bc2_levelwrite::culltree::CullTree::read(&t).unwrap().leaf_count()
        };
        assert_eq!(
            ty.enums[count],
            i64::from(leaves),
            "{lv}: a new leaf at the client tree's end"
        );
        // the physics: one more instance's parts; the packfile and the member types' bytes the base's
        let res = s[&format!("levels/{low}/{}.res", g.file.trim_end_matches(".dbx"))].clone();
        let f = files(&game, lv);
        let base_res = &f[&format!("levels/{lv}/{}.res", g.file.trim_end_matches(".dbx"))];
        let (a, b) = (GroupPhysics::read(&res).unwrap(), GroupPhysics::read(base_res).unwrap());
        assert_eq!(
            (a.types.clone(), a.tail.clone(), a.ends.clone()),
            (b.types.clone(), b.tail.clone(), b.ends.clone())
        );
        assert_eq!(a.part_words.len(), b.part_words.len() + ty.parts as usize);
        // the same project without member edits: the other groups' files the same bytes
        export::build(&game, &plain, "test", &spec(name, &plain, profile), &plain_stage).unwrap();
        let s0 = staged(&plain_stage);
        let mut same = 0;
        #[allow(clippy::iter_over_hash_type, reason = "an assertion per file and a count")]
        for (n, d) in &s0 {
            if n.contains("_havokasset.") && !n.contains(&g.file.trim_end_matches(".dbx").to_string()) {
                assert_eq!(s.get(n), Some(d), "{lv}: {n}");
                same += 1;
            }
        }
        println!("{lv}: built in {built:.1} s; {same} other group files the same bytes as without the edits");
        assert!(same >= 2);
    }
    if keep.is_none() {
        let _ = std::fs::remove_dir_all(&tmp);
    } else {
        let _ = std::fs::remove_dir_all(tmp);
    }
}

impl Group {
    /// The instance's guid as the staged file has it (the export gives the staged partitions new guids only for
    /// the partition itself; the instance keeps its own).
    fn instance_in(&self, f: &LevelWriter) -> String {
        if f.find(&self.instance).is_some() {
            return self.instance.clone();
        }
        let inst =
            f.d.instances()
                .into_iter()
                .find(|&e| f.d.child(e, b"Assets").is_some())
                .unwrap();
        String::from_utf8_lossy(f.d.attr(inst, b"guid").unwrap()).to_ascii_uppercase()
    }
}
