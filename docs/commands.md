# Commands

Canonical reference for `plan` and `gloss` subcommands, plus dev recipes.
Linked from `README.md` and `AGENTS.md`.

## `plan`

Global flags: `--config PATH`, `--root PATH` (`-r`), `--show-done`.

`--root` sets the workspace root: where `.plan/` (task files, schema)
is discovered — for every subcommand (default: auto-detect the nearest
`.plan/` ancestor). `[PATH]` only selects what `lint`/`format` operate on, with
foreign semantics (all-`.md` enumeration, adjacent-only schema search). The
two compose: `plan -r <repo> format foreign/file.md` uses the repo's config
while targeting a foreign file. Note `plan format -r <dir>` with no path
still assumes a task tree (`<dir>/.plan`) and errors if absent — pass the
directory as `[PATH]` to format foreign files.

Subcommands (abbreviate to first letter, e.g. `plan b`):

| Command | Alias | Description |
| --- | --- | --- |
| (none) | — | Default view (`kanban` or `list` per config) |
| `kanban` | `k` | Kanban board text view (filters/sort) |
| `browse` | `b` | Interactive kanban browser |
| `list` | `l` | List tasks in a table (filters/sort) |
| `init` | — | Scaffold `.plan/` with schema + template |
| `add <slug>` | `a` | Create a task |
| `open <slug>` | `o` | Open a task in `$EDITOR` |
| `delete <slug>` | `d` | Delete a task |
| `lint [PATH]` | — | Validate frontmatter (schema optional for foreign paths) |
| `format [PATH]` | — | Format markdown (file or dir) |

Filters (kanban/browse/list): `-s/--status`, `-t/--type`, `-p/--priority`,
`-a/--area`, `-q` search. Sort: `-S/--sort FIELD` (repeatable).

`add` options: `-s/--status` (default `open`), `-t/--type` (default
`feature`), `-p/--priority` (default `medium`), `--area` (default `""`).

`lint [PATH]`: validate frontmatter. With no `PATH`, validate every task in
the workspace's `.plan/` tree; a schema is required and its absence is a hard
error: `plan: error: no schema found (searched .plan/.schema.json, config
dir, data dir)` (exit 1). With a `PATH`:
- directory: lint all `.md` files in it;
- file: must be `.md`, lint that file.
Schema is searched only in adjacent locations: `<dir>/.schema.json`,
`<dir>/.plan/.schema.json`, or (for a file) its parent directory. If none is
found, lint prints `warning: no schema found (searched ...)` to stderr and
continues with schema-independent checks only (frontmatter delimiter present,
YAML parses, frontmatter is a mapping); required/enum/unknown-key checks are
skipped. Exit 0 when no `FAIL` diagnostics remain; exit 1 otherwise, with each
failure on stderr as `<path>: FAIL — ...`. Files with no frontmatter are
skipped silently in schemaless mode; in schema mode they are validated as
before.

`format [PATH]`: with a file, format that file; with a directory, format all
`.md` files in it; with no path, normalize every task in the workspace.
- Task path (files under `<root>/.plan/`, or the no-path whole tree): applies
  canonical field ordering (`status`/`type`/`priority`/`area` first) and
  re-emits multi-line values in block style. A split failure (unclosed `---`)
  is a hard error: `<path>: cannot normalize — ...` (exit 1).
- General path (all other explicit paths): frontmatter is preserved
  byte-verbatim (key order, comments, and block-scalar styles untouched) and
  only the body is formatted.
- Invalid, unparseable, or non-mapping frontmatter: skipped with
  `<path>: WARNING — invalid frontmatter, skipped` on stderr; the file is
  left byte-untouched and reports `Ok(false)` (it never counts as
  "would format" under `--check`). In the general path an unclosed `---`
  delimiter is skipped the same way.
`--check` dry-runs: directory runs print `would format <path>` per file and
exit 1 with `N file(s) would be reformatted` if any would change; single-file
runs print `<path>: would be reformatted`. Skipped files do not affect the
exit code. `--width`/`-w COLS` overrides `format.width` (default 72) for this
run. `--reflow`/`--no-reflow` overrides `format.reflow` (default true) for
this run.

## `gloss`

Modes:

- `gloss FILE` — TUI single-file viewer.
- `gloss DIR` — TUI directory browser.
- `gloss` (no arg, tty) — browse `.md` files in cwd.
- `cat file.md | gloss` — filter mode (stdin → stdout).
- `:%!gloss` — vim filter.

Flags: `--config PATH`, `--no-color`, `--theme NAME`, `--width/-w COLS`,
`--no-scrollbar`.

## Dev recipes (justfile)

| Recipe | Description |
| --- | --- |
| `just check` | fmt --check + clippy (required before commit) |
| `just test` | cargo test |
| `just format` | cargo fmt |
| `just precommit` | check + test |
| `just audit` | cargo deny + cargo audit |
| `just install` | cargo install + themes (no template configs) |
| `just smoke` | build + smoke-test both CLIs |
| `just run …` | cargo run passthrough |
| `just build` / `just build-release` | build |
| `just uninstall` | remove binaries + themes |
| `just install-dev` | install cargo-deny + cargo-audit |
| `just setup-dev` | install dev tools + enable pre-commit hook |

Single test invocation:

```bash
cargo test <test_name>
cargo test --test cli_e2e
cargo test --test gloss_render
```

## Cargo aliases

`.cargo/config.toml` defines `cargo plan` and `cargo gloss` as aliases for
`cargo run --bin …`. Use either form:

```bash
cargo plan kanban
cargo run --bin plan -- kanban
```

## Tarball install

Inside a release tarball, `packaging/Makefile` provides `make install`
(binaries + themes + docs) without Rust:

```bash
tar xzf tablethat-<target>.tar.gz && cd tablethat
make install                         # -> ~/.local
make install PREFIX=/usr/local       # system-wide
make uninstall
```

`just install` installs binaries + themes only; it does not install
template configs.
