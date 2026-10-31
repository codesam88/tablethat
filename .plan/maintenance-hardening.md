---
status: open
type: chore
priority: high
area: devops
---

## Notes

Repo health check 2026-10-31: `just check` pass, `cargo test` 59 pass, `cargo audit` 2 warnings, `cargo deny check` FAIL, `core.hooksPath` unset.

## Security

- [ ] Migrate off `serde_yaml@0.9` (RUSTSEC-2024-0449 `unsafe-libyaml@0.2.11` unmaintained) to `serde-saphyr` or `serde_yaml_ng`. `cargo tree -i unsafe-libyaml`. Keeps `deny.toml ignore=[]` green.
- [ ] Track `RUSTSEC-2026-0253` `lru@0.18` unsound via `ratatui-core`. Bump `ratatui` when upstream fixes.
- [ ] Validate slug in `create_task` (`src/plan/tasks.rs:834`): reject `/`, `\`, `..`, enforce `^[a-z0-9-]+$`. Prevents `plan add ../foo` escaping `.plan/`.
- [ ] Harden launches: `Command::new("gloss")` PATH hijack (`tasks.rs:864`, `tui_kanban.rs:259`); `Command::new(&editor)` breaks on `EDITOR="code --wait"`. Consider `current_exe` dir lookup + splitting editor args.
- [ ] Stop silent fallbacks: `Config::load unwrap_or_default()` (`lib.rs:172`), theme bad-TOML skip (`theme.rs:43-45`). Surface errors.
- [ ] Set `permissions: contents: read` in `ci.yml` / `audit.yml` (only `release.yml` has it). Add Dependabot for cargo + actions. Add SBOM/provenance/checksums to release.

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

