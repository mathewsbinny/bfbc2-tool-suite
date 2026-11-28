# Releasing

How changes reach `main`, and how a version is released. The version scheme is in `CHANGELOG.md`'s header. The
tasks and their lanes are in the task registry, `tasks/registry.toml` (`tasks/README.md`); `ROADMAP.md` is generated
from it.

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
6. In the merge, before committing it: `python tasks\build.py merge <code>` (below), then `python tasks\build.py
   --check`, and commit the registry, `CHANGELOG.md`, `VERSION` and the regenerated files with the merge. Then
   `python tasks\build.py --bfo <battlefield-opfor checkout>` writes battlefield-opfor's `docs/tasks/TASKS.md`,
   committed there with its next change.

### On the branch: no versions, no CHANGELOG edits

A branch does not edit `VERSION` or `CHANGELOG.md`, and does not number its own version. Until round TK0 each branch
picked "the next version" itself, so two open branches claimed the same number (OR13 and OR16 both wrote 0.38.0 /
0.39.0 against main's 0.37.0). Instead:

- the round's user-visible changes go in `tasks/changes/<task id>.md` (Keep a Changelog lists; `tasks/README.md`);
- its measurements are a section appended at the end of `rust/BENCHMARKS.md`, named in its task's `benchmarks`;
- its task's lane and dates are in `tasks/registry.toml`. `ROADMAP.md`'s generated part is regenerated, never
  edited, and on a merge conflict it is regenerated, not merged by hand (`tasks/README.md`, "Merge conflicts").

### At the merge: the version is assigned (round QW2, audit AU3 AR-9; since TK0 by the registry)

`python tasks\build.py merge <code>` marks the task done with the merge's date and appends its fragment to
**Unreleased**, setting the task's `version` to `unreleased`. If the fragment has a **Removed** list it also releases
the next minor version at once. Removed means: a command line, a subcommand or option, a Python function or module
another project imports, a Rust public API, or a file format or folder a tool reads or writes. The release step is
`cut`: `VERSION` is set, **Unreleased** becomes `[X.Y.0] - <date>` with the repeated Added / Changed / Fixed / Removed
lists folded into one each, and every task released in it gets that version. That way Project OPFOR's pin of the
suite (`opforcore.paths.SUITE_REQUIRES`: at least one version, below the next minor) can tell the suite it was
written against from one that broke it. Then:

1. If Project OPFOR uses what was removed, OPFOR's change that follows moves `SUITE_REQUIRES` to the new range
   (`('0.11.0', '0.12.0')`) in the same sitting, and OPFOR's tests run against the merged suite.
2. Tag the merge on `main` (`git tag -a vX.Y.Z -m "Version X.Y.Z"`; "Releasing a version" step 6). Pushing the tag
   is the repository owner's call, like every push.

A branch begun before TK0 that already wrote its version into `CHANGELOG.md` / `VERSION` keeps it if no other merge
took that number first (otherwise its heading and `VERSION` move to the next free minor), and is merged with
`python tasks\build.py merge <code> --version X.Y.Z`. That sets its version, and that of the Unreleased entries its
heading took in.

A round that only adds or fixes leaves `VERSION` alone; its entries wait in **Unreleased** for the next release
(`python tasks\build.py cut`). A round with nothing a user notices has no fragment.

### Standing rules

- **Spec rules:** each porting round adds the spec rules of what it ports (`spec/`, checked by `spec/trace.py
  --check`). This replaces the queued "spec layers 3-4" task: the layers exist since SP2, and growing them is part of
  every round (the user, 2026-11-28).
- **The game tier** still runs only on the development PC, because it needs the game copy. CI runs the quick tier's
  game-free entries.

A round that retires a tool or folder follows `RETIRED.md`'s checklist (proven equal on recorded data, nothing
pointing at it, the hub and OPFOR updated, then deleted) and adds its row there.

Branch protection (requiring the checks before a merge) is not switched on; that is the repository owner's choice.
`v0.9.0` to `v0.24.0` are tagged and the versions before 0.9.0 stay untagged. 0.25.0 to 0.37.0 were merged without
the tag step (found by round TK0; the task `TAG-BACKFILL`).

## Releasing a version

1. Decide the number (`CHANGELOG.md`'s header): a batch of user-visible features is a minor bump (`0.9.0` ->
   `0.10.0`), fixes only a patch (`0.9.0` -> `0.9.1`).
2. `python tasks\build.py cut --version X.Y.Z`: it writes `VERSION` (one line, `0.MINOR.PATCH`), renames
   **Unreleased** to `[X.Y.Z] - YYYY-MM-DD` with one Added / Changed / Fixed / Removed list each, and sets the version of
   every task released in it. Nothing else holds the number: `bfbc2core.__version__`, `bfbc2.exe --version`,
   `bfbc2-editor --version`, the editor's Help and the hub's title all read the file.
3. Check that the lists say what a user notices (the measurements are BENCHMARKS', the round narrative the commit
   messages').
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
