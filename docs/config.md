# Configuration

Themes are defined separately in `themes/*.toml`; see [themes.md](themes.md).

`plan` and `gloss` share a layered configuration model:
**defaults < config file < environment variables < CLI flags**.
CLI flags always win; environment variables override the file; the file
overrides built-in defaults.

## Config file locations

For `plan` (app name `plan`, prefix `PLAN_`) and `gloss` (app name `gloss`,
prefix `GLOSS_`), `Config::try_load` resolves the file in this order:

1. Explicit `--config PATH` (error if missing/invalid — never silent).
2. `$PLAN_CONFIG` / `$GLOSS_CONFIG` if it points at a file.
3. Project-local `plan.toml` / `gloss.toml` in the current directory.
4. Platform config dir: `…/plan/config.toml` / `…/gloss/config.toml`.

A malformed auto-detected file is skipped with a stderr warning and falls
back to defaults.

## Environment variables and nested keys

`Env::prefixed(...)` is used with `.split("__")`, so a double underscore
in an env var name becomes a `.` path separator into a nested struct.
Single underscores stay within a key name. Example:
`PLAN_FORMAT__WIDTH=40` sets `format.width`; `PLAN_TUI_WIDTH=100` sets
`tui_width`.

Every config key is reachable this way, so env vars exist for all rows in
the Options table (e.g. `PLAN_DEFAULT_VIEW`, `GLOSS_WIDTH`).

## Options

| Config key | Env var | CLI flag | Default | Description |
|---|---|---|---|---|
| `root` | `PLAN_ROOT` / `GLOSS_ROOT` | `--root` (plan, global) | auto-detect | Repo root containing `.plan` |
| `editor` | `PLAN_EDITOR` / `GLOSS_EDITOR` | — | `$EDITOR` or `vi` | Editor for `plan open` |
| `themes_dir` | `PLAN_THEMES_DIR` / `GLOSS_THEMES_DIR` | — | `./themes/` | Theme lookup directory |
| `default_view` | `PLAN_DEFAULT_VIEW` / `GLOSS_DEFAULT_VIEW` | — | `kanban` | Bare `plan` view: `list` or `kanban` |
| `default_sort` | `PLAN_DEFAULT_SORT` / `GLOSS_DEFAULT_SORT` | — | `["priority","slug"]` | Default sort keys |
| `hide_done` | `PLAN_HIDE_DONE` / `GLOSS_HIDE_DONE` | `--show-done` (plan, global, inverts) | `true` | Hide `done` tasks unless shown |
| `kanban_order` | `PLAN_KANBAN_ORDER` / `GLOSS_KANBAN_ORDER` | — | idea,backlog,open,in-progress,blocked,done | Kanban column order |
| `tui_width` | `PLAN_TUI_WIDTH` / `GLOSS_TUI_WIDTH` | — | `80` | Render width for TUI views |
| `pager_width` | `PLAN_PAGER_WIDTH` / `GLOSS_PAGER_WIDTH` | — | `100` | Render width for pager/`open` |
| `width` | `PLAN_WIDTH` / `GLOSS_WIDTH` | `--width`/`-w` (gloss) | `80` | Render width for piped filter output |
| `format.width` | `PLAN_FORMAT__WIDTH` / `GLOSS_FORMAT__WIDTH` | `--width`/`-w` (`plan format`) | `72` | Wrap width for `plan format` |
| `format.reflow` | `PLAN_FORMAT__REFLOW` / `GLOSS_FORMAT__REFLOW` | `--reflow`/`--no-reflow` (`plan format`) | `true` | Join short lines into flow before re-splitting |
| `colors.status.*` | `PLAN_COLORS__STATUS__*` / `GLOSS_COLORS__STATUS__*` | — | see themes | Per-status kanban colors |
| `colors.priority.*` | `PLAN_COLORS__PRIORITY__*` / `GLOSS_COLORS__PRIORITY__*` | — | see themes | Per-priority kanban colors |
| — | — | `--config` (both, global) | auto-detect | Path to config file |
| — | — | `--no-color` (gloss) | `false` | Disable color in filter mode |
| — | — | `--theme` (gloss) | first theme | Theme name override |
| — | — | `--no-scrollbar` (gloss) | `false` | Hide scrollbar in TUI |
| — | — | `--sort`/`-S` (plan) | config default | Sort keys (repeatable) |
| — | — | filters `-s/-t/-p/-a/-q` (plan) | none | Task filters |
| — | — | `plan format [PATH]` | `.plan/*.md` | File/dir to format |

Themes are defined in `themes/*.toml`, not in `plan.toml`; see `themes_dir`.
