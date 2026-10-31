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
- [x] Step 3 (4A+B): gloss resolution + editor splitting.
- [x] Step 4 (5A+B): config/theme warnings + `Result` propagation.
- [x] Step 5 (6A+B+C): workflow permissions + dependabot + release attestation.
- [ ] Step 6 (2A): `lru` fixed upstream — monitor via daily `audit.yml` (no action; `cargo audit` currently shows 1 allowed warning).

## Security (labels: [class|severity|effort])

- [x] [supply-chain|critical|effort:S] Migrate off `serde_yaml@0.9` (RUSTSEC-2024-0449 `unsafe-libyaml@0.2.11` unmaintained) to `serde-saphyr` or `serde_yaml_ng`. `cargo tree -i unsafe-libyaml`. Keeps `deny.toml ignore=[]` green.
- [x] [supply-chain|medium|effort:XS] Track `RUSTSEC-2026-0253` `lru@0.18` unsound via `ratatui-core`. Bump `ratatui` when upstream fixes.
- [x] [input-validation|high|effort:S] Validate slug in `create_task` (`src/plan/tasks.rs:834`): reject `/`, `\`, `..`, enforce `^[a-z0-9-]+$`. Prevents `plan add ../foo` escaping `.plan/`.
- [x] [exec|medium|effort:S] Harden launches: `Command::new("gloss")` PATH hijack (`tasks.rs:864`, `tui_kanban.rs:259`); `Command::new(&editor)` breaks on `EDITOR="code --wait"`. Consider `current_exe` dir lookup + splitting editor args.
- [x] [config|medium|effort:S] Stop silent fallbacks: `Config::load unwrap_or_default()` (`lib.rs:172`), theme bad-TOML skip (`theme.rs:43-45`). Surface errors.
- [x] [ci-supply-chain|medium|effort:S] Set `permissions: contents: read` in `ci.yml` / `audit.yml` (only `release.yml` has it). Add Dependabot for cargo + actions. Add SBOM/provenance/checksums to release.

## DevOps

- [x] Enable hook: `git config core.hooksPath .githooks` / `just setup-dev`. Verify in CI docs.
- [x] Fix `ci.yml paths-ignore: .plan/**` ignoring `.plan/.schema.json` schema changes.
- [x] Pin MSRV: `rust-version=1.85` has no CI check. Add `cargo msrv --check` or pin `stable` in `rust-toolchain.toml`.
- [x] Add CI smoke test: `cargo build && plan --help && echo '# hi' | gloss --width 80`.

## Quality

- [x] Add `tests/` e2e: `plan add/list/lint` lifecycle + `gloss` stdin->stdout filter (currently only inline units, `tests/` empty).
- [x] Return `Result` from `create_task/format_file/normalize_all/validate_all` instead of `bool + eprintln!`. Remove prod `expect()` in `tasks.rs:104,748,760,766`.
- [x] Split god files: `tasks.rs:1303`, `markdown.rs:1196`, `tui_kanban.rs:974`. Dedup `status_color` wrapper + table test.

## Acceptance

- `just precommit` + `just audit` green, `cargo deny check` passes, hook active, at least one e2e test added.

## Progress (newest first)

- 2026-10-31: Layout track DONE (plan: `.plan/modular-layout.md`, all steps
  checked). tasks.rs deleted (6 modules, 122-390 lines), lib.rs split
  (11 lines), no `#[path]`, no dup helpers, mains are parse + `run()`.
  107 tests green. Remaining: `lru` watch only (automated via daily audit).
- 2026-10-31: Result refactor DONE by parent (TDD: 5 contract tests RED on bool signatures, then GREEN first pass — 105 total, e2e green through rewritten CLI arms). `create_task/delete_task -> Result<PathBuf,String>`, `open_task/run_filter -> Result<(),String>`, `format_file -> Result<bool /*changed*/,String>`, `normalize_all -> Result<usize,Vec<String>>`, `validate_all(root, &mut warnings) -> Result<(),Vec<String>>`; all 4 named `expect!`s + `markdown.rs` guarded `unwrap` + filter stdin `expect` removed (zero prod expect/unwrap left). Printing moved to CLI arms; `list` output order now errors-then-warnings. Bonus fix: gloss nonzero exit now reports code (was silent).
- 2026-10-31: CLI e2e DONE by parent (TDD: harness compile-RED, then green; non-vacuity proven by mutating `is_valid_slug` call off — reject test FAILS, restore heals). `tests/cli_e2e.rs`, 12 tests, invariant-style (exit codes, file existence, synthesized-value matches, idempotency, width bound) — no output snapshots. Isolation via temp `--root`+cwd, temp HOME/XDG, stripped PLAN_*/GLOSS_*/EDITOR. Excludes TTY modes (open/browse/kanban, gloss TUI). 100 tests total pass. Acceptance item "at least one e2e test" satisfied.
- 2026-10-31: Render-test layer DONE by parent (TDD: harness RED on missing corpus, then fixtures). `tests/gloss_render.rs` + `tests/golden/` (8 corpus docs): table-driven fixed patterns, golden files (blessed via `UPDATE_GOLDEN=1`, each reviewed), properties (no-panic at 4 widths, no content loss, formatter idempotent), theme-mapped-once. Caught 3 wrong expectations pre-commit (trailing-space wrap wart now pinned+noted; link flattening vs span-level rule clarified; fence-info/URL words excluded from loss check). 6 integration tests, 88 total pass. CLI lifecycle e2e (plan add/list/lint, gloss filter) still open.
- 2026-10-31: DevOps batch DONE by parent (config-only). Hook enabled locally (`core.hooksPath=.githooks`) + README Development section; `just smoke` recipe + CI smoke step (verified exit 0); `msrv` CI job (`cargo +1.89 test`, proven locally — 82 pass on 1.89 toolchain, bypasses floating stable via `+toolchain`); `paths-ignore` box was already done in Step 5. All DevOps + Security boxes checked; only Quality + Step 6 watch remain.
- 2026-10-31: Step 5 DONE by parent (worker pool free-limit hit, config-only work). `contents: read` on ci/audit; release top-level narrowed to read, release job `write + id-token + attestations`; `SHA256SUMS.txt` + `attest-build-provenance@v2` (subject: checksums); weekly Dependabot (cargo + actions, limit 5, `chore` prefix); ci `paths-ignore` narrowed `.plan/**` -> `.plan/*.md` so `.schema.json` triggers CI. YAML validated, plan lint pass. Leftover: full SBOM skipped (checksums + attestation cover provenance; revisit if needed).
- 2026-10-31: Step 4 DONE via worker (reviewed by parent). `Config::load` replaced by `try_load -> Result` (explicit `--config` missing/unparseable exits 1 with path; auto-detect warns + skips). `load_themes_with_errors` reports bad TOML, wrapper warns (all 3 callers covered). 7 new tests, 82 total pass, deny green, no old callers remain.
- 2026-10-31: Step 3 DONE via worker (reviewed by parent). New `src/launch.rs` in lib: `split_editor_cmd`/`editor_command` via `shell-words@1.1.1`, `resolve_gloss_program` via `current_exe` sibling + PATH fallback (+`.exe` handling). All 4 sinks rewired, no-shell invariant kept. 9 new tests, 75 total pass, deny green. Leftover: `Config.editor` still unused (decide in Step 4).
- 2026-10-31: Step 2 DONE via worker (reviewed by parent). `is_valid_slug` (`^[a-z0-9]+(?:-[a-z0-9]+)*$`, no leading/trailing `-`) enforced in `create_task` before any fs touch + `strip_prefix` defense-in-depth. RED observed (traversal files created pre-fix, incl. escape proof), 5 new tests, 66 total pass, clippy/fmt clean.
- 2026-10-31: Step 1 DONE via worker (reviewed by parent). `serde_yaml` removed, `serde-saphyr@2` + `preserve_order` + `BSD-3-Clause` license. Null-to-empty shim preserves `area: ` leniency (regression test added). `unsafe-libyaml` gone from tree, `deny` green, audit down to 1 warning (`lru`, accepted per 2A). 61 tests pass. Parent also fixed pre-existing `collapsible_if` clippy lint in `markdown.rs:107` (stable-toolchain drift, unrelated to migration).
- 2026-10-31: Decisions recorded, `serde-saphyr` brief at `docs/briefs/serde-saphyr.md`, RED test `frontmatter_parses_with_pure_rust_backend` added, Step 1 delegated to worker.

