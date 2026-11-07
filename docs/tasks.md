# Task management (`.plan`)

Canonical reference for task files, lifecycle, workflow, and slug matching.
Linked from `README.md`, `AGENTS.md`, and `SKILL.md` — do not restate
these rules elsewhere.

## Task files

Every task lives as a markdown file in `.plan/<slug>.md` with YAML
frontmatter. The filename (without `.md`) is the unique key — no numeric
ID.

### Frontmatter example

```markdown
---
status: open
type: feature
priority: medium
area: backend
---

## Notes

Description and context.
```

Required fields: `status`, `type`, `priority`. Optional: `area`.
Validation uses `.plan/.schema.json` (project-local), falling back to
`~/.config/plan/schema.json`. `additionalProperties: false`.

Allowed values:

- **Status:** `idea` · `backlog` · `open` · `in-progress` · `blocked` · `done`
- **Type:** `bug` · `feature` · `chore` · `decision` · `perf`
- **Priority:** `high` · `medium` · `low`

## Status lifecycle

| status        | meaning                   | when to use                                   |
| ------------- | ------------------------- | --------------------------------------------- |
| `idea`        | aspirational, not settled | explore later, not ready to start             |
| `backlog`     | accepted, deferred        | consider when stepping back to plan next work |
| `open`        | ready                     | actionable, waiting to be picked up           |
| `in-progress` | active                    | currently being worked on                     |
| `blocked`     | stuck                     | note the blocker in the body                  |
| `done`        | complete                  | finished                                      |

## Workflow

- **Discover work:** `plan` or `plan list` lists all tasks;
  `plan kanban` (alias `k`) groups by status column. Filter with
  `-s open`, `-a backend`, `-t bug`, `-p high`, `-q query`.
- **Step back / plan:** `plan k` to see the full board. Pull items from
  `backlog` or `idea` when choosing what to work on next.
- **Sort:** `plan list -S area -S priority` for compound sort.
- **Create:** `plan add <slug>` (alias `a`). Override defaults with
  `-s`, `-t`, `-p`, `--area`. Or copy `.plan/.TEMPLATE.md` to
  `.plan/<slug>.md`.
- **Open:** `plan open <slug>` (alias `o`) opens in `$EDITOR`. Supports
  fuzzy slug matching.
- **Update:** set `status` to reflect state. Append progress notes at the
  bottom (newest first). Do not rewrite history.
- **Complete:** set `status: done` when finished. Do not delete the file.
- **Delete:** `plan delete <slug>` (alias `d`) removes the file. Supports
  fuzzy slug matching.
- **Validate:** `plan lint [PATH]` to check; `plan format [PATH]` to auto-fix
  (file or directory; default `.plan/*.md`). The pre-commit hook runs
  `format` automatically.
- **Init:** `plan init` scaffolds `.plan/` with schema and template.

## Slug matching

`open`, `delete`, and `add` accept a slug-ish query that matches
`.plan/*.md` files by:

1. Exact slug match
2. Prefix match
3. Substring match (case-insensitive)

If multiple tasks match, a TUI selector appears (Enter to select,
q/Ctrl-C to abort).

## Schema / template notes

- `.plan/.schema.json` is project-local; `plan init` copies it from
  config/data dirs or writes an embedded default.
- `.plan/.TEMPLATE.md` is a starter template; copy it when creating tasks.
- `plan format PATH` accepts a file or directory (see `src/plan/format.rs`);
  with no path it normalizes every task in the workspace.

## Agent notes

- Prefer high priority + `open`/`in-progress` items when discovering work.
- Create tasks by copying `.plan/.TEMPLATE.md` to `.plan/<slug>.md` and
  follow its contained instructions.
- Set `status: blocked` and note the blocker in the body.
- Run `plan lint [PATH]` to check and `plan format` to auto-format.
- Append progress notes newest first; do not rewrite history.
