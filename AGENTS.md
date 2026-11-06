# AGENTS.md

## Project overview

Single Rust crate producing two binaries (`plan`, `gloss`) and one library
(`tablethat_lib`).
- `plan` — task management with kanban TUI. Tasks are `.plan/*.md` files
  with YAML frontmatter.
- `gloss` — markdown viewer (TUI + stdin filter mode).
- Shared lib: config loading (figment), color parsing, theme support.

## Commands

```bash
just check          # fmt --check + clippy (REQUIRED before commit)
just test           # cargo test
just format         # cargo fmt
just precommit      # check + test (runs automatically via .githooks/pre-commit)
just audit          # cargo deny + cargo audit
just install        # cargo install --path . (binaries + themes only)
```

Enable pre-commit hook: `git config core.hooksPath .githooks`

**Run order matters**: `just check` before `just test`. The pre-commit hook
runs both via `just precommit`.

## Git / PR workflow

- **PR-based flow on protected `main`**: do work on a branch, push, open a
  PR, report checks green, then stop.
- **NEVER merge a PR without an explicit user prompt.** Not on green
  checks, not with `--admin`, not ever. Merging is the user's decision.
- Same rule for other irreversible actions: never push tags, retag,
  publish to crates.io, or force-push without an explicit prompt.

## Running a single test

```bash
cargo test <test_name>
cargo test --test cli_e2e
cargo test --test gloss_render
```

Integration tests live in `tests/` (`cli_e2e.rs`, `gloss_render.rs`,
`tests/golden/`). Unit tests are inline in source files.

## Testing policy

Test functional invariants, not policy, implementation, or literals. Every test
should asserts a behavior of the system, not a detail of its current shape: Use
fixpoints (format twice == format once), bounds (`line.len() <= width`),
ordering relations, exit-code + file-existence contracts, content preservation
(`html_eq`, body survives), error-vs-success. Exact matches only for values the
test itself synthesized. Golden input/output reference vectors (`tests/golden/`)
can can be used to pin overall behavior and catch regressions.

No vacuous tests for non-effects or non-existing strings are permitted. A test that trivially passes on removed/damaged code is not a test.

When adjusting tests, consider if the test becomes vacuous or if better
invariants can be found.

## Crate structure

```
src/
  lib.rs
  config.rs
  theme.rs
  color.rs
  markdown.rs
  markdown_fmt.rs
  text.rs
  plan.rs
  gloss.rs
  launch.rs
  bin/plan/main.rs
  bin/gloss/main.rs
  plan/model.rs
  plan/query.rs
  plan/lint.rs
  plan/display.rs
  plan/store.rs
  plan/format.rs
  plan/tui.rs
  gloss/filter.rs
  gloss/tui.rs
```

## Key conventions

- **Rust edition 2024** — MSRV 1.89 (`rust-version = "1.89"`).
- **Clippy**: warnings are denied (`-D warnings`). Avoid `unwrap()` — use
  `expect()` with a message or proper error handling.
- **Pre-commit hook**: `git config core.hooksPath .githooks`.
- **Config layering**: defaults < config file < env vars < CLI flags. Uses
  `figment`. See [`docs/config.md`](docs/config.md).
- **Themes**: loaded from `themes/*.toml`, separate from config. See
  [`docs/themes.md`](docs/themes.md).
- **Task files**: markdown in `.plan/` with YAML frontmatter. See
  [`docs/tasks.md`](docs/tasks.md) (status lifecycle, workflow, slug
  matching, default view).

## Documentation

- [`docs/README.md`](docs/README.md) — documentation index.
- [`docs/tasks.md`](docs/tasks.md) — tasks.
- [`docs/config.md`](docs/config.md) — configuration.
- [`docs/themes.md`](docs/themes.md) — themes.
- [`docs/commands.md`](docs/commands.md) — commands.
