# AGENTS.md

## Project overview

Single Rust crate producing two binaries (`plan`, `gloss`) and one library (`tablethat_lib`).
- `plan` — task management with kanban TUI. Tasks are `.plan/*.md` files with YAML frontmatter.
- `gloss` — markdown viewer (TUI + stdin filter mode).
- `src/lib.rs` — shared config loading (figment), color parsing, theme support.

## Commands

```bash
just check          # fmt --check + clippy (REQUIRED before commit)
just test           # cargo test
just format         # cargo fmt
just precommit      # check + test (runs automatically via .githooks/pre-commit)
just audit          # cargo deny + cargo audit
just install        # cargo install --path .
```

Enable pre-commit hook: `git config core.hooksPath .githooks`

**Run order matters**: `just check` before `just test`. The pre-commit hook runs both via `just precommit`.

## Running a single test

```bash
cargo test <test_name>
```

No integration test harness exists yet (`tests/` is empty). Tests are inline in source files.

## Crate structure

```
src/
  lib.rs              — Config, ThemeConfig, ColorsConfig, parse_color, workspace_root
  markdown.rs         — comrak markdown rendering + formatting
  theme.rs            — theme loading from TOML files
  bin/plan/main.rs    — plan CLI (clap derive)
  bin/gloss/main.rs   — gloss CLI (clap derive)
  plan/tasks.rs       — task CRUD, validation, listing
  plan/tui_kanban.rs  — ratatui kanban TUI
  gloss/filter.rs     — stdin→stdout markdown filter
  gloss/tui_preview.rs — ratatui file/directory viewer
```

Binary mains include sibling modules via `#[path = "../../plan/tasks.rs"]` — not the standard module tree. This is intentional.

## Key conventions

- **Rust edition 2024** — requires Rust 1.85+. Check `rustup show` if builds fail on edition.
- **Clippy**: warnings are denied (`-D warnings`). Avoid `unwrap()` — use `expect()` with a message or proper error handling.
- **Config layering**: defaults < config file < env vars < CLI flags. Uses `figment` crate. Prefix: `PLAN_` / `GLOSS_`.
- **Themes are separate from config** — loaded from `themes/*.toml`, not from plan.toml. Config only provides `themes_dir` (where to find them) and `colors` (status/priority for kanban).
- **Theme files**: `themes/*.toml`. Color values: named strings, ANSI256 decimal, or hex (`#FF8800`). Fields: `h1_color`, `h2_color`, `h3_color`, `code_color`, `code_block_color`, `bold_style`, `emphasis_style`.
- **Task files**: markdown in `.plan/` with YAML frontmatter, see **Task Management** below

## Task Management (.plan)

Every task lives as a markdown file in `.plan/<slug>.md` with YAML frontmatter. The filename (without `.md`) is the unique key — no numeric ID needed.

### Status lifecycle

| status        | meaning                   | when to use                                   |
| ------------- | ------------------------- | --------------------------------------------- |
| `idea`        | aspirational, not settled | explore later, not ready to start             |
| `backlog`     | accepted, deferred        | consider when stepping back to plan next work |
| `open`        | ready                     | actionable, waiting to be picked up           |
| `in-progress` | active                    | currently being worked on                     |
| `blocked`     | stuck                     | note the blocker in the body                  |
| `done`        | complete                  | finished                                      |

### Task Management Workflow

- **Discover work:** `plan` or `plan list` lists all tasks; `plan kanban` (alias `k`) groups by status column. Filter with `-s open`, `-a backend`, `-t bug`, etc.
- **Step back / plan:** `plan k` to see the full board. Pull items from `backlog` or `idea` when choosing what to work on next.
- **Sort:** `plan list -S area -S priority` for compound sort.
- **Create:** `plan add <slug>` (alias `a`). Override defaults with `-s`, `-t`, `-p`, `--area`.
- **Open:** `plan open <slug>` (alias `o`) opens in `$EDITOR`. Supports fuzzy slug matching — if multiple tasks match, a TUI selector appears.
- **Update:** update 'status' to reflect status. append progress notes at the bottom (newest first). Do not rewrite history.
- **Complete:** set `status: done` when finished. Do not delete the file.
- **Delete:** `plan delete <slug>` (alias `d`) removes the file. Supports fuzzy slug matching.
- **Validate:** `plan lint` to check; `plan format` to auto-fix. `plan format PATH` formats a specific file or directory. The precommit hook runs `format` automatically.

### Slug matching

`open`, `delete`, and `add` accept a slug-ish query that matches `.plan/*.md` files by:
1. Exact slug match
2. Prefix match
3. Substring match (case-insensitive)

If multiple tasks match, a TUI selector appears (Enter to select, q/Ctrl-C to abort).

### Default view

The default view (`plan` with no subcommand) is configurable via `plan.toml`:
```toml
default_view = "list"  # or "kanban" (default)
```

For the interactive kanban browser, use `plan browse` or `plan b`.
