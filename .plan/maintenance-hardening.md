---
status: in-progress
type: chore
priority: high
area: devops
---

## Notes

Repo health check 2026-10-31: `just check` pass, `cargo test` 59 pass, `cargo audit` 2 warnings, `cargo deny check` FAIL, `core.hooksPath` unset.

## Decisions 2026-10-31 (user-chosen)

- 1B: `serde-saphyr` pure-Rust (not `yaml_serde` rename). Accept MSRV bump + `Value` refactor.
- 2A: watch `lru`, no code change.
- 3A+B: strict slug `^[a-z0-9-]+$` + canonicalize + `strip_prefix` guard.
- 4A+B: `current_exe` dir lookup for `gloss` + `shell-words` split for `$EDITOR`.
- 5A+B: stderr warn on fallback now, `Config::load -> Result` + hard-fail on explicit `--config` next.
- 6A+B+C: `permissions: contents: read`, Dependabot, SBOM/attestation/checksums.

## Problem / purpose

`cargo deny check` fails on unmaintained `unsafe-libyaml` via deprecated `serde_yaml@0.9`; slug traversal, PATH hijack, silent config fallbacks, and broad CI permissions remain. Fixing in user-chosen order restores green deny/audit and hardens local CLI trust boundary.

## Solution strategy

Chosen: pure-Rust YAML (1B) over drop-in `yaml_serde` rename — decisive drawback of rename is it keeps c2rust-transpiled `libyaml-rs` unsafe. Rejected `serde_yaml_ng` (still `unsafe-libyaml`). Order: 1B, 3, 4, 5, 6, 2-watch. See `docs/briefs/serde-saphyr.md` for tool digest.

## Implementation steps

- [x] Step 1 (1B): `serde_yaml@0.9` -> `serde-saphyr@2`. Scope: `Cargo.toml`, `src/plan/tasks.rs` (`from_str`, `Value`, `to_string`). Done: `cargo deny check` passes, `just precommit` green, frontmatter round-trip tests pass. Leaves out: MSRV CI pin (DevOps item).
- [x] Step 2 (3A+B): slug validation + traversal guard + TDD tests.
- [ ] Step 3 (4A+B): gloss resolution + editor splitting.
- [ ] Step 4 (5A+B): config/theme warnings + `Result` propagation.
- [ ] Step 5 (6A+B+C): workflow permissions + dependabot + release attestation.
- [ ] Step 6 (2A): verify `lru` fix upstream, `cargo update -p ratatui`.

## Security (labels: [class|severity|effort])

- [ ] [supply-chain|critical|effort:S] Migrate off `serde_yaml@0.9` (RUSTSEC-2024-0449 `unsafe-libyaml@0.2.11` unmaintained) to `serde-saphyr` or `serde_yaml_ng`. `cargo tree -i unsafe-libyaml`. Keeps `deny.toml ignore=[]` green.
- [ ] [supply-chain|medium|effort:XS] Track `RUSTSEC-2026-0253` `lru@0.18` unsound via `ratatui-core`. Bump `ratatui` when upstream fixes.
- [ ] [input-validation|high|effort:S] Validate slug in `create_task` (`src/plan/tasks.rs:834`): reject `/`, `\`, `..`, enforce `^[a-z0-9-]+$`. Prevents `plan add ../foo` escaping `.plan/`.
- [ ] [exec|medium|effort:S] Harden launches: `Command::new("gloss")` PATH hijack (`tasks.rs:864`, `tui_kanban.rs:259`); `Command::new(&editor)` breaks on `EDITOR="code --wait"`. Consider `current_exe` dir lookup + splitting editor args.
- [ ] [config|medium|effort:S] Stop silent fallbacks: `Config::load unwrap_or_default()` (`lib.rs:172`), theme bad-TOML skip (`theme.rs:43-45`). Surface errors.
- [ ] [ci-supply-chain|medium|effort:S] Set `permissions: contents: read` in `ci.yml` / `audit.yml` (only `release.yml` has it). Add Dependabot for cargo + actions. Add SBOM/provenance/checksums to release.

## DevOps

- [ ] Enable hook: `git config core.hooksPath .githooks` / `just setup-dev`. Verify in CI docs.
- [ ] Fix `ci.yml paths-ignore: .plan/**` ignoring `.plan/.schema.json` schema changes.
- [ ] Pin MSRV: `rust-version=1.85` has no CI check. Add `cargo msrv --check` or pin `stable` in `rust-toolchain.toml`.
- [ ] Add CI smoke test: `cargo build && plan --help && echo '# hi' | gloss --width 80`.

## Quality

- [ ] Add `tests/` e2e: `plan add/list/lint` lifecycle + `gloss` stdin->stdout filter (currently only inline units, `tests/` empty).
- [ ] Return `Result` from `create_task/format_file/normalize_all/validate_all` instead of `bool + eprintln!`. Remove prod `expect()` in `tasks.rs:104,748,760,766`.
- [ ] Split god files: `tasks.rs:1303`, `markdown.rs:1196`, `tui_kanban.rs:974`. Dedup `status_color` wrapper + table test.

## Acceptance

- `just precommit` + `just audit` green, `cargo deny check` passes, hook active, at least one e2e test added.

## Progress (newest first)

- 2026-10-31: Step 2 DONE via worker (reviewed by parent). `is_valid_slug` (`^[a-z0-9]+(?:-[a-z0-9]+)*$`, no leading/trailing `-`) enforced in `create_task` before any fs touch + `strip_prefix` defense-in-depth. RED observed (traversal files created pre-fix, incl. escape proof), 5 new tests, 66 total pass, clippy/fmt clean.
- 2026-10-31: Step 1 DONE via worker (reviewed by parent). `serde_yaml` removed, `serde-saphyr@2` + `preserve_order` + `BSD-3-Clause` license. Null-to-empty shim preserves `area: ` leniency (regression test added). `unsafe-libyaml` gone from tree, `deny` green, audit down to 1 warning (`lru`, accepted per 2A). 61 tests pass. Parent also fixed pre-existing `collapsible_if` clippy lint in `markdown.rs:107` (stable-toolchain drift, unrelated to migration).
- 2026-10-31: Decisions recorded, `serde-saphyr` brief at `docs/briefs/serde-saphyr.md`, RED test `frontmatter_parses_with_pure_rust_backend` added, Step 1 delegated to worker.

