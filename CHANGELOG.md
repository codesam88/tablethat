# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Linux ARM64 releases: tarball plus static `.deb`/`.rpm` alongside amd64

## [0.1.0] – 2026-11-01

### Added

- `gloss` file viewer: optional 1-char scrollbar with viewport thumb and
  search-result markers (`--no-scrollbar` opts out)
- `plan browse`: filter-as-you-type slug search with match highlighting
  (`/` to start, Enter keeps, Esc clears)
- `plan browse`: wrap-around navigation, PgUp/PgDn home/end jumps,
  attention-ordered columns (active work first)
- `plan browse`/`gloss`: Esc-press-twice quit debounce, Ctrl-C cancels
  search like Esc in every search input
- Open editor at the cursor line for vi/emacs-family editors (`+N`)
- CI: MSRV job (1.89), smoke test, release verify gate, weekly zizmor
  workflow lint, Dependabot (cargo + actions)
- Release: SHA256 checksums, build provenance attestation

### Changed

- YAML backend `serde_yaml` (unmaintained) replaced with pure-Rust
  `serde-saphyr`; MSRV is now 1.89
- Task operations (`create`, `delete`, `format`, `lint`, `list`) return
  `Result` instead of `bool`; library code no longer calls
  `process::exit` or panics on user input
- Task frontmatter values are validated (no newlines) and YAML-quoted
  when special; empty values stay bare instead of becoming `null`
- Library split into modules (`config`, `theme`, `color`, `markdown`,
  `plan/*`, `gloss/*`); binaries are parse + dispatch glue
- All GitHub Actions pinned to commit SHAs; release is unix-only

### Fixed

- `RUSTSEC-2026-0253` (`lru`) via dependency update; audit is silent
- Task slug path traversal (`plan add ../evil` rejected)
- Gloss search highlight desync (current match could lose yellow) and
  silent search loss on resize/theme/editor/file changes
- Unicode search highlight and slug truncation panics
- `plan format <dir>` processes files in sorted order and reports
  unreadable directories instead of silently succeeding

### Security

- `cargo deny` (advisories, bans, licenses, sources) and `cargo audit`
  run in CI; `cargo audit` also runs daily
