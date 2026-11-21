//! Round DF1's proofs on stand-in game folders (no game copy needed):
//!
//! - **AU4-BUG-10**: a `bundleManifest` that is there but does not read fails `Game::open` (it was read as empty); a
//!   level with voice-over blocks in `package.mft` on a game copy without `bundleManifest` is refused, never exported
//!   with `package.mft` entries that have no `bundleManifest` lines (the checksum crash).
//! - **AU4-CQ-10**: a bundle that fails after its unpack leaves no scratch folder behind.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::{Path, PathBuf};

use bc2_formats::fbrb::{self, Layout, PackItem};
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::profile::Announcer;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_levelwrite_df1_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A stand-in game folder whose `package.mft` gives level `mp_x` an English voice-over block.
fn game_folder(dir: &Path) -> PathBuf {
    let g = dir.join("game");
    let win32 = g.join("Dist").join("win32");
    std::fs::create_dir_all(&win32).unwrap();
    std::fs::write(
        win32.join("package.mft"),
        "BundleSource game:levels/mp_x/vo/en-00.fbrb\nBundleTarget game:levels/mp_x/vo/en\n",
    )
    .unwrap();
    g
}

#[test]
fn a_missing_or_unreadable_bundle_manifest_is_never_read_as_empty() {
    let _gate = crate::gate();
    let dir = temp("bug10");
    let g = game_folder(&dir);
    // no bundleManifest: the game opens, but the level's voice-over cannot be registered
    let game = Game::open(&g).unwrap();
    assert!(!game.has_manifest());
    assert_eq!(game.vo_blocks("mp_x").len(), 1);
    let e = Announcer::of(&game, "mp_x").expect_err("refused").to_string();
    assert!(e.contains("bundleManifest"), "{e}");
    // a bundleManifest that does not read (here a folder): Game::open fails, naming it
    let bm = g.join("Dist").join("win32").join("bundleManifest");
    std::fs::create_dir_all(&bm).unwrap();
    let e = Game::open(&g).expect_err("refused").to_string();
    assert!(e.contains("bundleManifest"), "{e}");
    // with one: read, and the block is named
    std::fs::remove_dir_all(&bm).unwrap();
    std::fs::write(&bm, "levels/mp_x/vo/en\t0\n").unwrap();
    let game = Game::open(&g).unwrap();
    assert!(game.has_manifest() && game.in_manifest("levels/mp_x/vo/en"));
    let a = Announcer::of(&game, "mp_x").unwrap();
    assert_eq!(a.manifest, ["vo/en"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bundle_failing_after_its_unpack_leaves_no_scratch_folder() {
    let _gate = crate::gate();
    let dir = temp("cq10");
    // a bundle that unpacks but has no level list: patch_startup fails after its unpack
    let items = [PackItem {
        name: "levels/other.dbx",
        type_name: "dbx",
        flags: 0,
        size2: 3,
        data: b"abc",
    }];
    let source = dir.join("startup-00.fbrb");
    std::fs::write(&source, fbrb::pack(&items, &Layout::default()).unwrap()).unwrap();
    let work = dir.join("work");
    let r = export::patch_startup(&source, &[], &[], &work, &dir.join("out.fbrb"));
    assert!(r.is_err(), "no level list");
    assert!(!work.exists(), "the scratch folder was removed on the error path");
    let _ = std::fs::remove_dir_all(&dir);
}
