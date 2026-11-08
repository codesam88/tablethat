# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] – 2026-11-08

### Added

- `plan format [PATH]` with `--width/-w COLS`, `--reflow`/`--no-reflow`,
  and `--check` dry-run; `[format]` section in config (reflow on by default)
- `plan lint [PATH]`: lint a single file or a foreign directory; without a
  schema it warns and runs structural checks only
- `plan format` on general markdown: foreign frontmatter preserved
  byte-verbatim, unparseable frontmatter skipped with a warning
- Viewer width policy for `gloss`: base width from `-w` or live terminal
  width minus chrome; prose clamped to 40–120 columns, tables to 60–160
- Tables in the viewer wrap cell text instead of truncating (compact tables
  stay compact; overlong words hard-break with orphan protection)
- Unicode display-width measurement in formatter and viewer (CJK/em dash
  wrap and align correctly)
- Unicode table borders in gloss with `--ascii` fallback
- List description terms highlighted in inline-code color bold
- `plan list`: `l:limit` (slug) vs `/`: (fulltext) search split
- Rolling release channel: floating `head` prerelease rebuilt from green
  main; version tags stay stable

### Changed

- Formatter rewritten as conservative splice engine (split-only wrap,
  hardbreak-exempt whitespace strip, AST-ranged table alignment,
  idempotent output)
- Hide-done toggle rebound from `h` to `H` (frees hjkl navigation)
- TUI no longer swallows event/draw errors; transient resize interrupts
  are retried

### Fixed

- Right table border overlapping the scrollbar gap (content width now
  accounts for the left pad)
- Tables one column too wide from shrink integer-division residue, and
  long-token columns overflowing small budgets (soft floors yield to a
  hard floor of 4)
- `plan format` corrupting multi-line YAML frontmatter (inline re-emit
  dedented block sequences); values now block-emit
- arm64 nfpm packaging used the wrong arch name; rolling version now
  derives from the last stable release

## [0.1.1] – 2026-11-01

### Added

- Linux ARM64 releases: tarball plus static `.deb`/`.rpm` alongside amd64
- Static musl tarballs (`-musl` suffix) for pre-glibc-2.39 distributions

### Changed

- MSRV check (1.89) moved from every PR to a release-only gate

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
