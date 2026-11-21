# Releasing

How changes reach `main`, and how a version is released. The version scheme is in `CHANGELOG.md`'s header; the
queue of rounds is `ROADMAP.md`.

## How a round reaches main

1. Work on a branch of its own (a worktree beside the main checkout: `bfbc2-tool-suite-<round>`, branch
   `<kind>-<round>`), never on `main`. Its cargo builds go to `rust-targets\bfbc2-tool-suite-<round>` beside the
   checkouts; after the worktree is removed, `python -m bfbc2core.caches --prune-targets --yes` (from `core\`) deletes
   that build folder (README.md, "Where Rust builds").
2. Before each commit: `python run_tests.py quick`. Before pushing: `python run_tests.py game` on the development
   PC (it needs the game copy, which CI does not have). Since round TS1 the whole-level editor, levelwrite and
   renderer tests run only in the game tier (`#[ignore = "game tier: ..."]`): a change to those crates is proven by
   the game tier, not by the quick tier or CI's `cargo test` (which leaves ignored tests out).
3. Push the branch. The **checks** workflow (`.github/workflows/checks.yml`) runs on every push and pull request:
   the Rust module's build, cargo fmt, clippy with warnings as errors, the workspace's cargo tests, ruff, mypy and
   the quick tier's game-free entries (`python run_tests.py quick --no-game --ci-steps`).
4. Wait for the checks to be green, then merge the branch into `main` (a pull request, or a merge commit from the
   branch). A red check is fixed on the branch, not on `main`.
5. On `main`: the **native** workflow (`.github/workflows/native.yml`) builds and publishes the prebuilt Rust module
   and `bfbc2.exe` (since round RT1) when the Rust sources (or `VERSION`) changed; `python rust\build_native.py
   --download` fetches both. The map editor and `mapexport.exe` are not published (built with `--editor`).
6. Move the round to **Done** in `ROADMAP.md` with its one-line result, and add its user-visible changes to the
   **Unreleased** section of `CHANGELOG.md` (create it at the top when there is none).

A round that retires a tool or folder follows `RETIRED.md`'s checklist (proven equal on recorded data, nothing
pointing at it, the hub and OPFOR updated, then deleted) and adds its row there.

Branch protection (requiring the checks before a merge) is not switched on; that is the repository owner's choice.

## Releasing a version

1. Decide the number (`CHANGELOG.md`'s header): a batch of user-visible features is a minor bump (`0.9.0` ->
   `0.10.0`), fixes only a patch (`0.9.0` -> `0.9.1`).
2. Write it in `VERSION` (one line, `0.MINOR.PATCH`). Nothing else holds the number: `bfbc2core.__version__`,
   `bfbc2.exe --version`, `bfbc2-editor --version`, the editor's Help and the hub's title all read the file.
3. In `CHANGELOG.md`, rename **Unreleased** to `[X.Y.Z] - YYYY-MM-DD` and check that its Added / Changed / Fixed /
   Removed lists say what a user notices.
4. `python rust\build_native.py` (the new `VERSION` is part of the sources' fingerprint, so `bfbc2.exe` is built
   again), then check `bin\bfbc2.exe --version` and `python run_tests.py quick`.
5. Commit (`Version X.Y.Z`) on a branch, push, wait for the checks, merge into `main`.
6. Tag the merge on `main` and push the tag:

   ```
   git tag -a vX.Y.Z -m "Version X.Y.Z"
   git push origin vX.Y.Z
   ```

The versions before 0.9.0 were back-filled from the history and never tagged; `CHANGELOG.md` lists the commit each
one ends at.
