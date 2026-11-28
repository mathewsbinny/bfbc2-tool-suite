# CHANGELOG fragments

One file per round, `<task id>.md`: its user-visible changes (an optional lead paragraph, then `### Added` / `### Changed` /
`### Fixed` / `### Removed`). `python tasks\build.py merge <code>` moves it into `CHANGELOG.md` at the merge and deletes
it; a `### Removed` list releases the next minor version at once (`RELEASING.md`). See `../README.md`.
