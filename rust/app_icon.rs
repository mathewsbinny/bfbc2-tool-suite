// The suite's application icon as a Windows resource (round I1), shared by the build scripts of the binaries
// (bc2-cli: bfbc2.exe, bc2-editor: bfbc2-editor.exe, bc2-levelwrite: mapexport.exe) through `include!`.
//
// The icon is found as the editor (bc2-editor's appicon.rs) and the hub (bfbc2core.appicon) find it:
//   1. BFBC2_APP_ICON, a path to an .ico
//   2. design\icons\local\app.ico   (a personal override; gitignored, never committed)
//   3. design\icons\app.ico         (the committed icon: the author's own M67 render, design\icons\README.md)
//   4. none: the binaries build without an icon
// Cargo runs the script again when BFBC2_APP_ICON changes or anything under design\icons changes (a folder given
// to rerun-if-changed is scanned whole), so a release build picks up a new, changed or removed icon.

use std::path::{Path, PathBuf};

/// The environment variable naming the icon file.
const APP_ICON_ENV: &str = "BFBC2_APP_ICON";

/// The icon to embed, in lookup order, from the suite root and the environment variable's value.
fn find_app_icon(root: &Path, env: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if let Some(v) = env.filter(|v| !v.is_empty()) {
        let p = PathBuf::from(v);
        if p.is_file() {
            return Some(p);
        }
        println!("cargo:warning={APP_ICON_ENV}={} is not a file; looking in design\\icons", p.display());
    }
    let icons = root.join("design").join("icons");
    [icons.join("local").join("app.ico"), icons.join("app.ico")]
        .into_iter()
        .find(|p| p.is_file())
}

/// Embeds the suite's icon in the binaries `bins` of the crate whose build script calls this (as resource 1, the
/// icon Explorer, the taskbar and shortcuts show). Does nothing for a non-Windows target or when there is no icon;
/// a resource compiler that is missing or fails is a warning and the binaries build without an icon.
fn embed_app_icon(bins: &[&str]) {
    println!("cargo:rerun-if-env-changed={APP_ICON_ENV}");
    let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from) else {
        return;
    };
    // rust\<crate> -> the suite root
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&manifest)
        .to_path_buf();
    println!("cargo:rerun-if-changed={}", root.join("design").join("icons").display());
    let env = std::env::var_os(APP_ICON_ENV);
    if let Some(p) = env.as_deref().filter(|v| !v.is_empty()).map(Path::new)
        && p.is_file()
    {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(icon) = find_app_icon(&root, env.as_deref()) else {
        return;
    };
    let Some(out) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    // rc.exe reads the path as a string: forward slashes need no escaping
    let path = icon.display().to_string().replace('\\', "/");
    let rc = out.join("app_icon.rc");
    if let Err(e) = std::fs::write(&rc, format!("1 ICON \"{path}\"\n")) {
        println!("cargo:warning=the application icon was not embedded: {e}");
        return;
    }
    match embed_resource::compile_for(&rc, bins, embed_resource::NONE) {
        embed_resource::CompilationResult::Ok | embed_resource::CompilationResult::NotWindows => {}
        r => println!("cargo:warning=the application icon was not embedded: {r}"),
    }
}
