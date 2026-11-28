# The task registry

`tasks/registry.toml` is the one source of truth for every task of the project: the suite's rounds, battlefield-opfor's,
xenolith's, Project OPFOR BFBC2's and the retired BF1943 projects', done or not (round TK0, 2026-11-28, seeded from round
TR1's whole-history list and the user's 46 review decisions). The trackers people read are generated from it by
`python tasks\build.py`:

| Generated | What |
|---|---|
| `ROADMAP.md`, between its GENERATED markers | the priority order, in progress, ready to merge, the queues, on hold, Done (one line per round: date, version, evidence), dropped and merged |
| `tasks/board.json` | the task board's data (the board page's generator reads it) |
| battlefield-opfor's `docs/tasks/TASKS.md` | every task of every repository by lane, written into the battlefield-opfor checkout (`--bfo <path>`, `BFBC2_BFO_ROOT`, else the one beside the suite) |

The other repositories cite a task by its code (`PX1`, `XM9-XM11`) and never keep a status of their own: a status is
the registry's `lane`.

## Commands

```
python tasks\build.py                     write the generated files
python tasks\build.py --check             the quick tier's tasks-registry entry: the registry valid, the files current
python tasks\build.py --fmt               the registry in its canonical form again, after a hand edit
python tasks\build.py show <code>         one task with its scope, decisions and links (a code, alias or old code)
python tasks\build.py merge <code>        at a round's merge: done, its CHANGELOG fragment, its version (RELEASING.md)
python tasks\build.py cut [--version V]   CHANGELOG's Unreleased released as a version (RELEASING.md)
```

`--check` fails when a suite file is stale. A battlefield-opfor `TASKS.md` behind the registry is a note there, not a
failure: the suite's tests do not fail on another repository's state. `--strict-bfo` makes it fail, for the merges
that commit both repositories.

## A task

One `[[task]]` table per task, sorted by `id`, with the keys in this order. Empty keys are left out. There are no
comments, because the canonical form drops them: put the words in a field.

| Key | What |
|---|---|
| `id` | stable and never changed or reused: lower case letters, digits and `.+-`. It is the code as it was first written (`kb2-kb4`, `e3b-ed`) |
| `code` | the round code people use. It can be renamed (the old code goes in `renamed_from` or `aliases`) |
| `title` | one line |
| `lane` | `run` (in progress), `merge` (ready to merge), `next`, `bfo` (queued for battlefield-opfor's phase), `later`, `hold`, `done`, `dropped` |
| `repos` | `suite`, `bfo`, `OPFOR`, `xenolith`, `bf1943`, `private`, `all` |
| `category` | one of the `[[category]]` ids at the top of the file |
| `tags` | free words, used for filtering |
| `raised`, `started`, `done` | dates, `YYYY-MM-DD`. A done task has `done` |
| `owner` | the round's branch / worktree, while it runs |
| `parent` | the family or round this task belongs to (`BOTS-KITS` in `STOCKMP-NEXT`'s family) |
| `depends` | ids of the tasks it waits for. The generated pages list the open ones as "After" |
| `merged_into` | for a dropped task merged into others: their ids |
| `aliases`, `renamed_from` | other codes that find this task (`show`). `aliases` are unique; `renamed_from` may repeat another task's code (a reused code) |
| `version` | the suite version that released it: set by `merge` / `cut`, `unreleased` until the version is cut |
| `benchmarks` | the round's `rust/BENCHMARKS.md` (or `BENCHMARKS_ARCHIVE_1.md`) sections, by their heading's name: the registry is the index of which round has which section |
| `kb` | battlefield-opfor knowledge-base pages (`docs/...`) |
| `spec`, `links` | spec rule ids, other references |
| `score` | `{ d, t, h }`: difficulty 1-5, size 1-5 and calibrated agent-hours (the coordinator's scoring of open tasks, 2026-11-28; the board shows them) |
| `needs_slot`, `est_h`, `actual_h`, `tokens`, `worktrees`, `agent_state` | scheduling (HARN1, below) |
| `reason` | why a task was dropped or held |
| `body` | a short paragraph: state and result |
| `scope` | the full scope, where it is longer than a body (the old ROADMAP queue cells) |
| `source` | where TR1 / TK0 found it |
| `decisions` | `{ date, by, source, choice, note }` per ruling: who decided, when and where |

`[[priority]]` tables (`rank`, `title`, `ruled`, `by`, `source`, `tasks`, `lanes`) are the user's priority order: the
generated pages list each group's open tasks in that order, and the queues follow it. Change a group only when the user
changes the order.

The check fails on:

- a registry not in canonical form, or an unknown key;
- a duplicate id, code or alias;
- an unknown lane, repository or category;
- a `depends`, `parent`, `merged_into` or priority entry naming no task;
- a dropped task with neither a reason nor a `merged_into`;
- a done task without a date, or a version on a task that is not done;
- a BENCHMARKS section or CHANGELOG version that does not exist;
- a `kb` page missing in the battlefield-opfor checkout;
- a CHANGELOG fragment left behind by a task that is already done;
- a `python_status.json` retiring round that is no task code;
- a stale generated file.

## How a round uses it

1. **Starting:** set its task to `run`, with `owner` and `started` (a new task: add its block, then `--fmt`). Run
   `python tasks\build.py` and commit the registry with the regenerated files.
2. **User-visible changes:** write them in `tasks/changes/<id>.md`, Keep a Changelog style: an optional lead
   paragraph, then `### Added` / `### Changed` / `### Fixed` / `### Removed` lists. Do not edit `CHANGELOG.md` or
   `VERSION` on the branch: the version is assigned at the merge (`RELEASING.md`).
3. **Evidence:** append the round's section at the end of `rust/BENCHMARKS.md` and name it in the task's
   `benchmarks`.
4. **Ready:** lane `merge`. **At the merge:** `python tasks\build.py merge <code>` (`RELEASING.md`).

## Merge conflicts

The registry is one block per task, sorted by id, so two rounds that change their own tasks, or add tasks with
different ids, merge cleanly. A conflict inside one block is a real disagreement: settle it by hand. The generated
files (the ROADMAP's generated part, `board.json`, battlefield-opfor's `TASKS.md`) are never merged by hand. On a
conflict, take either side, run `python tasks\build.py` on the merged registry and commit the result. `--check` proves
it is right. CHANGELOG fragments are one file per task and never conflict.

## Renamed codes

| Old code | Date | Now | Why |
|---|---|---|---|
| E3b | 2026-11-19 | `E3b-ED` | reused: `E3b` is the Rust core's DbxEdit round of 2026-11-17 (BENCHMARKS "E3b"); this was the map export's profiles round |
| E3c | 2026-11-19 | `E3c-ED` | reused: `E3c` is the Rust core's NavKit hot-paths round of 2026-11-17; this was the export's announcer voice-over |
| E3d | 2026-11-20 | `E3d-ED` | reused: `E3d` is the Rust core's HLSL-compiles round of 2026-11-17; this was "any map, any era, any mode" (BENCHMARKS "E3d (map export)") |
| KB1-KB4 | 2026-11-28 | `KB1` (done) and `KB2-KB4` | KB1 merged; the family's rest keeps the old code as an alias |
| UI4-UI7 | 2026-11-28 | `UI7` | shrunk to UI7 by the user's decision: UI4 is XBM2, UI5 HUB3-HUB4, UI6 HUB1-HUB2 |

Code comments, BENCHMARKS sections and CHANGELOG entries written before 2026-11-28 keep the old codes: they are
history. `show E3b` finds the Rust core's round; the map export's is `show E3b-ED`.
