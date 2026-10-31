---
status: in-progress
type: chore
priority: medium
area: backend
---

## Notes

Split god files (`tasks.rs` ~1060 code lines, `markdown.rs` ~680, `tui_kanban.rs` ~960)
into lib modules by concern. Bins become parse + dispatch glue (no `#[path]`).
Parent of record: `.plan/maintenance-hardening.md` (god-file item).

## Decisions 2026-10-31

Second opinion: DeepSeek v4.1 Flash (pros/cons per question, verified line
counts, caught that TUI files are also `#[path]`-included and color wrappers
exist in 3 places). Adopted with amendments:

- (a) Rename `tui_kanban.rs->plan/tui.rs`, `tui_preview.rs->gloss/tui.rs`
  as standalone content-free `git mv` first. Agree.
- (b) Extract `markdown_fmt.rs` + add direct `align_tables` tests (currently
  only covered indirectly). Agree.
- (c) Keep `validate_all(root, &mut warnings)` signature through the move;
  typed diagnostics is later work, not an intermediate struct. Agree.
- Parent amendments: re-home verbatim into lib BEFORE splitting (one
  `tablethat_lib::`->`crate::` flip, not two); resolve color-wrapper
  triplication at move time (`color.rs` canonical); new files use
  `src/plan.rs` / `src/gloss.rs` mod roots (modern style, dir already exists).
- Global rule: moves are byte-identical code motion — tests travel with
  code, bodies unchanged. A red test after a move step means a move bug,
  never "fix the test".

## Problem / purpose

`tasks.rs` holds 6 concerns (model, query, lint, display, store, format);
`lib.rs` mixes config/theme/color; bins bypass the library via `#[path]`
includes, so the lib boundary is fictional and `crate::` vs `lib::`
references are inconsistent. Splitting by concern makes ownership obvious
and keeps files reviewable (~100-350 lines).

## Solution strategy

Pure moves first (compiler + 100-test suite prove each step), dedup and
thinning last. Rejected: splitting while still `#[path]`-included (forces
two import flips); big-bang single commit (unreviewable); `LintReport`
intermediate (churn on churn, settled in `9dd6d68`).

## Implementation steps

- [x] Step 1 (parent): pure renames `plan/tui_kanban.rs->plan/tui.rs`,
  `gloss/tui_preview.rs->gloss/tui.rs`; update `#[path]` attrs only.
  Done: build + tests green, `git diff -M` shows renames.
- [x] Step 2 (worker): `lib.rs` split -> `config.rs` (Config, Colors,
  try_load, workspace_root, resolve_file), `theme.rs` gains ThemeConfig,
  new `color.rs` (converters + status/priority). `lib.rs` keeps
  `pub use` re-exports so all paths work; zero edits elsewhere.
  Done: full suite green, no caller changes.
- [x] Step 3 (worker): re-home to lib. Add `src/plan.rs`
  (`pub mod tasks; pub mod tui;`), `src/gloss.rs`
  (`pub mod filter; pub mod tui;`); flip `tablethat_lib::`->`crate::`
  inside moved files; bins drop `#[path]`, use `tablethat_lib::plan|gloss`.
  Delete tui dup table test + tui dup color wrappers (use `crate::color`).
  Done: suite green, no `#[path]` left.
- [x] Step 4a (worker): `plan/tasks.rs` -> `model.rs` (Task, shim,
  parse_frontmatter), `query.rs` (load/filter/cmp/ord), `lint.rs`
  (validate_all + TEST_SCHEMA + its tests). Byte-identical bodies.
- [x] Step 4b (worker): -> `display.rs` (display fns, delete termcolor
  wrappers, use `crate::color`), `store.rs` (paths/resolve/init/CRUD/slug),
  `format.rs` (normalize/format fns). Byte-identical bodies.
- [x] Step 5 (worker): extract `markdown_fmt.rs` (format_commonmark,
  align_tables, strip_trailing_whitespace) + 2-3 direct `align_tables`
  tests; fix `tests/gloss_render.rs` import (1 line).
- [x] Step 6 (worker or parent): thin mains to clap structs + `run()`
  returning `Result<(), String>`, single exit point each.

## Acceptance

- `just precommit` green at every commit; `git log --stat` shows
  rename/move commits separable from behavior commits (none planned).
- No `#[path]` in tree; bins reference only `tablethat_lib::` + std + clap.
- No duplicated color helpers or table test; all tests present and passing.

## Progress (newest first)

- 2026-10-31: Step 6 DONE (worker, reviewed). Both mains are clap structs
  + `run() -> Result<(), String>` with one exit point each (`plan:` /
  `gloss:` prefix). Stderr-evidence check justified the prefix (e2e pins
  codes + stdout only). All steps complete: 107 tests green, no `#[path]`,
  no dup helpers, bins are glue. Left: god-file checkbox in
  maintenance-hardening + final acceptance pass.
- 2026-10-31: Step 5 DONE (worker, reviewed). `markdown_fmt.rs` extracted
  + 3 direct align_tables tests (were indirect-only). 107 green.
  Hardened brief held: no stray .plan/git mutation this run.
- 2026-10-31: Steps 2-4b DONE (workers, parent-reviewed each). lib.rs split
  (config/theme/color, lib.rs now 11 lines); re-home to lib (no `#[path]`
  left, dup test + tui wrappers deleted); tasks.rs split into
  model/query/lint then display/store/format; tasks.rs deleted.
  plan/ is 6 focused modules (122-390 lines). 104 tests green throughout.
  Process note: checkbox edits were twice lost from this file around worker
  runs — future briefs forbid workers from touching git state or .plan/.
- 2026-10-31: Plan written; DeepSeek consult done, verdict adopted with
  parent amendments above. Starting Step 1.
