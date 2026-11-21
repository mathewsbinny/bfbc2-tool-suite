//! Team labels by faction on levels of each content profile, from the game copy's own data (read only): BFBC2
//! levels from NavKit's unpack (`work\levels\<lv>`), the Package-only Vietnam and Pacific War (WW2) levels straight
//! from their `level-00.fbrb` in memory. Skipped (with a note) without Project OPFOR BFBC2.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, missing_docs)]

use std::path::{Path, PathBuf};

use bc2_editor::factions::{Profile, Teams, partition_from_bundle};
use bc2_editor::levels;

fn opfor() -> Option<PathBuf> {
    match levels::find_opfor(None) {
        Ok(r) => Some(r),
        Err(e) => {
            println!("skipped: {e}");
            None
        }
    }
}

/// The level partition: NavKit's unpack (a Package overlay first), else the Package bundle, else the Dist bundle.
fn partition(root: &Path, lv: &str) -> Option<Vec<u8>> {
    let work = root.join("work").join("levels").join(lv);
    for b in ["pkg", "dist"] {
        if let Ok(rd) = std::fs::read_dir(work.join(b).join("levels")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(lv))
                    && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dbx"))
                {
                    return std::fs::read(p).ok();
                }
            }
        }
    }
    for top in [
        root.join("game").join("Package").join("levels"),
        root.join("game").join("Dist").join("win32").join("levels"),
    ] {
        let bundle = top.join(lv).join("level-00.fbrb");
        if bundle.is_file() {
            return partition_from_bundle(&bundle, lv).ok();
        }
    }
    None
}

fn teams(root: &Path, lv: &str) -> Option<Teams> {
    let Some(data) = partition(root, lv) else {
        println!("{lv}: not in this game copy");
        return None;
    };
    let t = Teams::from_partition(&data, Profile::of(lv));
    println!("{lv}: {} / {} - {}", t.team1.label, t.team2.label, t.source);
    Some(t)
}

#[test]
fn factions_on_each_profile() {
    let _gate = crate::gate();
    let Some(root) = opfor() else { return };
    // (level, team 1, team 2, from the data)
    let cases = [
        ("mp_005", "US", "RU", true),
        // the Russians attack on Heavy Metal's rush and Valparaiso's assault: team 1 is RU there
        ("mp_012gr", "RU", "US", true),
        ("ons_mp_005gr", "RU", "US", true),
        // squad deathmatch and the campaign name no faction: T1 / T2, said so
        ("mp_004sdm", "T1", "T2", false),
        ("sp_01", "T1", "T2", false),
        // Vietnam: FactionRUS is the North Vietnamese Army
        ("nam_mp_002cq", "US", "NVA", true),
        ("nam_mp_005r", "US", "NVA", true),
        // Pacific War: FactionRUS is Japan
        ("wake_island_s", "US", "JP", true),
        ("coral_sea", "US", "JP", true),
        ("guadal_canal", "US", "JP", true),
        ("iwo_jima_s", "US", "JP", true),
    ];
    let mut seen = 0;
    for (lv, t1, t2, data) in cases {
        let Some(t) = teams(&root, lv) else { continue };
        seen += 1;
        assert_eq!(
            (t.team1.label.as_str(), t.team2.label.as_str()),
            (t1, t2),
            "{lv}: {}",
            t.source
        );
        assert_eq!(t.from_data, data, "{lv}: {}", t.source);
        if !data {
            assert!(t.source.contains("T1 and T2"), "{lv}: the fallback says so");
        }
    }
    println!("{seen} of {} levels in this game copy", cases.len());
}

/// The reading of `FactionRUS` as the Vietnamese side holds on DICE's own data: Vietnam's level description shows
/// team 2's kits with the `vietnam/vc_*` art and team 1's with `vietnam/us_*`.
#[test]
fn vietnam_team_2_has_the_vietnamese_kits() {
    let _gate = crate::gate();
    let Some(root) = opfor() else { return };
    let Some(data) = partition(&root, "nam_mp_002cq") else {
        println!("nam_mp_002cq: not in this game copy");
        return;
    };
    let tree = bc2_formats::dbx::parse_tree(&data).unwrap();
    let mut kits: Vec<(String, String)> = Vec::new();
    for n in tree.instances() {
        if n.attr("type").is_some_and(|t| t.ends_with(".LevelData"))
            && let Some(k) = n.child("LevelDescription").and_then(|d| d.child("KitImages"))
        {
            for item in k.children() {
                let team = item.child("Team").map_or_else(String::new, |c| c.text().to_owned());
                let tex = item
                    .child("Texture")
                    .and_then(|c| c.attr("ref").map(str::to_owned))
                    .unwrap_or_default();
                kits.push((team, tex));
            }
        }
    }
    assert_eq!(kits.len(), 8, "{kits:?}");
    for (team, tex) in &kits {
        let want = if team == "Team2" {
            "/vietnam/vc_"
        } else {
            "/vietnam/us_"
        };
        assert!(tex.contains(want), "{team}: {tex}");
    }
}
