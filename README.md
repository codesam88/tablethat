# TableThat

A fully vibe-coded TUI task manager using project-local markdown files.

I am not sure why I had to code another one of these but somehow, the 53 existing
projects didn't cut it. Among all the abandoned, half-working, too much friction projects,
each time I ended up rolling my own:

- [x] Kanban style list of tasks hosted out of project-local markdown files
- [x] Schema-validator and auto-formatter enforce metadata and minimize churn
- [x] Interactive TUI mode to browse, filter, view and edit tasks
- [x] Simple pager to render markdown with basic search/scroll + themes (@`glow` really??)
- [x] All confirmed to work and look good on transparent/dark terminals with tmux/ssh (YMMV!)


Why **table that**? Imagine you are coding away with AI, deep inside some rabbit
hole, when it throws you something funny. You ask back, and a short exploration
later you realize there is a potential veritable problem or architectural issue
that may need your attention. Instead of continuing the rabbit hole and
polluting your context, you *table that*, and tell it to create a task
documenting the issue and initial findings, then let it go back to work. You go
investigate on the side, or let it get back to you next day.


## Quick Steps

### Install

    just install

This will build and install binaries, themes and template configs to the user's
default directories (typically `~/.cargo/bin/` and `~/.config/{plan,gloss}/`).

### Or try the local build

    cargo build
    cargo plan kanban        # browse local project's tasks
    cargo gloss README.md    # view README.md

## The Tools

### plan

Task management with kanban TUI. Tasks live in `.plan/*.md` as markdown files with YAML frontmatter.

    plan                    # kanban view (default)
    plan browse             # interactive browser (alias: b)
    plan kanban             # kanban text view (alias: k)
    plan list               # list tasks (alias: l)
    plan add <slug>         # create a task (alias: a)
    plan open <slug>        # open in $EDITOR (alias: o)
    plan delete <slug>      # delete a task (alias: d)
    plan init               # scaffold .plan/ directory
    plan lint               # validate frontmatter
    plan format [PATH]      # format markdown

Commands can be abbreviated to their first letter (e.g., `plan b` for browse)

### gloss

Markdown viewer in lieu of `less` and `glow`. The default pager for `plan`:

    gloss README.md         # TUI single file viewer
    gloss docs/             # TUI directory browser
    cat file.md | gloss     # filter mode (stdin → stdout)
    :%!gloss                # vim filter

## Task file format

Each task is a markdown file in `.plan/`.

```markdown
---
status: open
type: bug
priority: high
area: backend
---

## Notes

Description and context.
```

**Status:** `idea` · `backlog` · `open` · `in-progress` · `blocked` · `done`
**Type:** `bug` · `feature` · `chore` · `decision` · `perf`
**Priority:** `high` · `medium` · `low`

Validation uses `.plan/.schema.json` (project-local), or falls back to `~/.config/plan/schema.json`.
Typically you may also include a simple .TEMPLATE.md to help AI create tasks to your liking.

## Configuration

Both tools use layered configuration: **defaults < config file < env vars < CLI flags**.

### plan

| Source | Path |
| --- | --- |
| Config file | `plan.toml` (project) / `~/.config/plan/config.toml` (user) |
| Env prefix | `PLAN_` |
| Env vars | `PLAN_ROOT`, `PLAN_EDITOR`, `PLAN_CONFIG`, `PLAN_THEMES_DIR` |

### gloss

| Source | Path |
| --- | --- |
| Config file | `gloss.toml` (project) / `~/.config/gloss/config.toml` (user) |
| Env prefix | `GLOSS_` |
| Env vars | `GLOSS_CONFIG`, `GLOSS_THEMES_DIR` |

### Themes

Theme configs are expected in the user's `${config}/${tool}/themes/` directory.
Cycle themes by pressing `c` in the markdown viewer.

```toml
name = "my-theme"

[theme]
h1_color = "#FF8800"
h2_color = "#FF8800"
h3_color = "#FF8800"
code_color = "cyan"
code_block_color = "yellow"
```

Color values: named (`red`, `green`, `blue`, `cyan`, `magenta`, `yellow`, `gray`, `darkgray`, `white`, `lightred`, ...), ANSI256 decimal (`42`), or hex (`#FF8800`).

Included themes: `default`, `matrix`, `matrix2`, `codesam`, `morning`, `dracula`, `nord`, `molokai`, `solarized`, `warm`, `mono`.

## Use with AI Agents

In `AGENTS.md` or some separate `SKILL.md`, tell it to use the plan tool
for creation, update, discovering issues. Example in this project's
[SKILL.md](SKILL.md).

The system is made for easy browsing and adjustment, but remember that all your
tasks are now turned to code. AI can help you to review, manage and prioritize,
you only need to review and course-correct.

## License

MIT OR Apache-2.0
