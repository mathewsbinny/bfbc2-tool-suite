//! Round OR5: Project OPFOR's menu build steps, which had no Python twin in the suite (`build_menus.bat` ran them):
//!
//!     bfbc2 aptpatch <in.res> <file.patch> <out.res>
//!         OPFOR's `AptTool.exe patch` (src\tools\apt\AptTool.cs, retired): the patch applied, the archive rebuilt
//!     bfbc2 menuart onslaught <unpacked mainmenu folder> [--xbox DIR] [--refresh] [--cache FILE] [--opfor DIR]
//!         OPFOR's `onslaught_art.py` (retired): the Offline Matches panel's ONSLAUGHT mode picture
//!
//! Exit codes: 0 done, 1 an error (its message on stderr), 2 a usage error.

use std::path::PathBuf;
use std::process::ExitCode;

use bc2_menu::{aptpatch, modeart};

const USAGE_PATCH: &str = "usage: bfbc2 aptpatch <in.res> <file.patch> <out.res>\n";
const USAGE_ART: &str = "usage: bfbc2 menuart onslaught <unpacked mainmenu folder> [--xbox DIR] [--refresh] \
                         [--cache FILE] [--opfor DIR]\n";

/// The environment variable naming the folder searched for the user's Xbox 360 files (OPFOR's `OPFOR_XBOX`).
const XBOX_VAR: &str = "OPFOR_XBOX";

#[allow(clippy::print_stdout, clippy::print_stderr)]
fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::from(1)
}

/// `bfbc2 aptpatch`.
#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn aptpatch_main(args: &[String]) -> ExitCode {
    let [res, patch, out] = args else {
        eprint!("{USAGE_PATCH}");
        return ExitCode::from(2);
    };
    let data = match std::fs::read(res) {
        Ok(d) => d,
        Err(e) => return fail(&format!("{res}: {e}")),
    };
    match aptpatch::patch(&data, res, patch) {
        Ok(p) => {
            if let Err(e) = bc2_gamefs::atomic::write(std::path::Path::new(out), &[&p.res]) {
                return fail(&format!("{out}: {e}"));
            }
            println!(
                "applied {} patches, appended {} strings -> {out}",
                p.applied, p.appended
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    }
}

/// `bfbc2 menuart onslaught`.
#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn menuart_main(args: &[String]) -> ExitCode {
    let mut a: Vec<String> = args.to_vec();
    let take = |a: &mut Vec<String>, flag: &str| -> Result<Option<String>, ()> {
        match a.iter().position(|x| x == flag) {
            None => Ok(None),
            Some(i) if i + 1 < a.len() => {
                let v = a.remove(i + 1);
                a.remove(i);
                Ok(Some(v))
            }
            Some(_) => Err(()),
        }
    };
    let (Ok(xbox), Ok(cache), Ok(opfor)) = (take(&mut a, "--xbox"), take(&mut a, "--cache"), take(&mut a, "--opfor"))
    else {
        eprint!("{USAGE_ART}");
        return ExitCode::from(2);
    };
    let refresh = a.iter().position(|x| x == "--refresh").map(|i| a.remove(i)).is_some();
    let [what, unpacked] = a.as_slice() else {
        eprint!("{USAGE_ART}");
        return ExitCode::from(2);
    };
    if what != "onslaught" {
        eprint!("{USAGE_ART}");
        return ExitCode::from(2);
    }
    let cache = match cache {
        Some(c) => PathBuf::from(c),
        None => match bc2_menu::common::find_opfor(opfor.as_deref()) {
            Ok(o) => PathBuf::from(o).join("work").join("import").join("onslaught_mode.png"),
            Err(e) => return fail(&e.to_string()),
        },
    };
    let xbox = xbox
        .or_else(|| std::env::var(XBOX_VAR).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| modeart::XBOX_DEFAULT.to_owned());
    let o = modeart::Options {
        unpacked: PathBuf::from(unpacked),
        xbox: PathBuf::from(xbox),
        refresh,
        cache,
    };
    let mut log = |s: &str| println!("  {s}");
    match modeart::run(&o, &mut log) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}
