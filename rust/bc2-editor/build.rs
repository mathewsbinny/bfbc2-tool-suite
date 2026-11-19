//! Embeds the suite's application icon in bfbc2-editor.exe (round I1: `rust/app_icon.rs`, README.md "The
//! application icon"); the window's own icon is read at start-up (`src/appicon.rs`).
#![allow(clippy::print_stdout)]

include!("../app_icon.rs");

fn main() {
    embed_app_icon(&["bfbc2-editor"]);
}
