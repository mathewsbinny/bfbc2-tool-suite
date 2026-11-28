### Added

- An MIT-only build: `bfbc2.exe` (bc2-cli) and the Rust module (bc2-py) built with `--no-default-features` link no
  `bc2-xma`, the LGPL-2.1-or-later XMA2 decoder; in that build EA-XMA streams do not decode. The checks workflow's
  `mit-only` job proves it with `cargo tree` on every push.

### Changed

- `bc2-xma` is behind an `xma` Cargo feature (bc2-convert's, on by default in bc2-cli and bc2-py). The published
  `bfbc2.exe` and module are built with it, so they combine the suite's MIT code with the LGPL-2.1-or-later decoder;
  README.md ("Licence"), RELEASING.md and the native release notes say so.
