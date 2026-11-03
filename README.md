# TableThat

A fully vibe-coded TUI task manager using project-local markdown files.

Obligatory **screenshots**: TUI kanban view (`plan b`) and viewing a task
via the included markdown viewer (`gloss`).

<img width="42%" alt="TUI Kanban overview (`plan b`)" src="https://github.com/user-attachments/assets/27c05260-8d38-4763-9bda-d2b1e91a4952" />
<img width="50%" height="627" alt="Markdown viewer (`gloss`)" src="https://github.com/user-attachments/assets/a6f9b889-5167-421d-acaa-09bd1ff20dcf" />

## Requirements

- Rust 1.89+ (`rust-version = "1.89"`), cargo, and `just`.
- Prebuilt glibc binaries need glibc 2.39+ (Ubuntu 24.04+, Debian 13+,
  Fedora 40+). The `.deb`/`.rpm` packages and `-musl` tarballs are fully
  static and run anywhere.

## Install

```bash
just install          # binaries + themes only (not template configs)
```

Or build locally:

```bash
cargo build
cargo plan kanban      # browse this project's tasks
cargo gloss README.md  # view README.md
```

From a release tarball (no Rust needed, no sudo): see
[`packaging/Makefile`](packaging/Makefile) (`make install`).

## 30-second demo

```bash
plan init
plan add my-first-task -p high
plan open my-first-task
plan b                # interactive kanban browser
gloss .plan/my-first-task.md
```

## Cheatsheet (excerpt)

```text
plan                    # kanban view (default)
plan browse             # interactive browser (alias: b)
plan list               # list tasks (alias: l)
plan add <slug>         # create a task (alias: a)
plan open <slug>        # open in $EDITOR (alias: o)
plan delete <slug>      # delete a task (alias: d)
plan init               # scaffold .plan/
plan lint               # validate frontmatter
plan format [PATH]      # format markdown
gloss FILE|DIR          # TUI viewer
cat file.md | gloss     # filter mode
```

Full reference: [`docs/commands.md`](docs/commands.md).

## Documentation

- [`docs/README.md`](docs/README.md) — documentation index.
- [`docs/tasks.md`](docs/tasks.md) — task format, lifecycle, workflow.
- [`docs/config.md`](docs/config.md) — configuration.
- [`docs/themes.md`](docs/themes.md) — themes.
- [`docs/commands.md`](docs/commands.md) — commands.

## AI agents

See [`SKILL.md`](SKILL.md) and [`AGENTS.md`](AGENTS.md).

## License

MIT OR Apache-2.0
