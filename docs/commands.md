# Commands

Canonical reference for `plan` and `gloss` subcommands, plus dev recipes.
Linked from `README.md` and `AGENTS.md`.

## `plan`

Global flags: `--config PATH`, `--root PATH` (`-r`), `--show-done`.

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
| `lint` | — | Validate task frontmatter |
| `format [PATH]` | — | Format task markdown (file or dir) |

Filters (kanban/browse/list): `-s/--status`, `-t/--type`, `-p/--priority`,
`-a/--area`, `-q` search. Sort: `-S/--sort FIELD` (repeatable).

`add` options: `-s/--status` (default `open`), `-t/--type` (default
`feature`), `-p/--priority` (default `medium`), `--area` (default `""`).

`format [PATH]`: with a file, format that file; with a directory, format all
`.md` files in it; with no path, normalize every task in the workspace.

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
