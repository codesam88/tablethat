---
status: open
type: chore
priority: medium
area: backend
---

## Notes

Style-conformance review 2026-11-12 against the `rust-coding-guide`
skill (Canonical Rust best practices, condensed). Four read-only passes
over `src/` and `tests/`; every finding re-verified against the tree
before recording, and false positives killed (see Rejected).

Purpose: the crate is healthy — no unassigned `let`, no
impl-before-type, no struct field-ordering violations, no glob imports
in production code, 267 tests with none ignored or assertion-free, and
only two `#[allow]`s in the whole tree. That is why the list is short
and mostly behavioural rather than stylistic: three documented config
knobs are parsed and then never used, and the crate has no `Error` type.

Order rationale: theme bugs first (user-visible), then CLI diagnostics,
then the error refactor, then TUI cleanup. The error refactor must
precede TUI cleanup — it shrinks those signatures for free.

## Findings (verified)

### Tier 1: bugs

| #   | Where                                         | Problem                                                                                                                                                                                                         |
| --- | --------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `markdown.rs:249,253`                         | `render_inline` hard-codes `Modifier::BOLD`/`DIM`; `th.bold`/`th.dim` (built from `bold_style`/`emphasis_style` at `markdown.rs:30-54`) are read only by the test at `theme.rs:266`. Theme fields are inert.    |
| 2   | `color.rs:22`                                 | `Color::Rgb` -> `Ansi256(rgb_to_ansi256(..))` quantises hex colours. 35 hex values across `themes/*.toml`; `termcolor::Color::Rgb` exists and does its own fallback. Also `:25` maps `Color::Reset` to `White`. |
| 3   | `config.rs:31`                                | `width` documented as the piped-filter width (`GLOSS_WIDTH`); read only by tests at `:412/:425/:436`. `bin/gloss/main.rs:50` uses terminal width instead.                                                       |
| 4   | `bin/gloss/main.rs:53-61`                     | Unknown `--theme NAME` silently falls back to `themes[0]`; a typo looks like "the theme made no difference".                                                                                                    |
| 5   | `bin/plan/main.rs:448`                        | `default_view: String` + `_ =>` sends typos to the list view, contradicting `try_load`'s "invalid explicit config is an error". Two 11-line arms differ only by a `bool`.                                       |
| 6   | `config.rs:420-434`                           | Four edition-2024 `unsafe { set_var/remove_var }` with no `// SAFETY:`.                                                                                                                                         |
| 7   | `bin/plan/main.rs:317`                        | Message starts `error: `, `main` prefixes `plan: ` -> `plan: error: no schema found`. Only doubled prefix in the dispatcher.                                                                                    |
| 8   | `launch.rs:45`                                | `rsplit('/').next().unwrap_or(program)` — fallback unreachable, and `/`-only split misses `C:\..\Code.exe`.                                                                                                     |
| 9   | `justfile:check`                              | `cargo clippy -- -D warnings` without `--all-targets`: 8 test-code warnings invisible to the commit gate.                                                                                                       |
| 10  | `theme.rs:239-243`                            | `let _ = theme.theme.h1_color;` on `Copy` fields asserts nothing.                                                                                                                                               |
| 11  | `plan/tui.rs:860,875`; `gloss/tui.rs:629,644` | Boxes an `io::Error` then `downcast_ref::<io::Error>()` it back; the `else` arm is unreachable by type.                                                                                                         |
| 12  | `plan/tui.rs:92`                              | Doc says "display width `n`"; body uses `slug.len()` (bytes).                                                                                                                                                   |

### Tier 2: worth a dedicated task

- **No crate error type.** 22 fns return `Result<_, String>` across 9
  files; 19 `Box<dyn std::error::Error>` in the two TUI modules; 16
  `Err(format!(..))`. Every boxed error is provably `io::Error`, so the
  erasure carries no information and `err_chain` (duplicated in both
  mains) adds nothing `Display` doesn't print. Cost today: drift and
  tool-failure are both exit 1, so CI can't distinguish them.
- **`list_tasks` has 9 positional params**, the crate's only
  `#[allow(clippy::too_many_arguments)]`, five of them consecutive
  `Option<&str>`; call sites read
  `load_and_filter_tasks(&entries, None, None, None, None, None, &[])`.
- **`gloss` re-reads the file from disk on every `n`/`N`.**
  `App.raw_lines` is populated (`gloss/tui.rs:244`) and never read;
  `apply_search_highlight` re-reads instead (`:277`). `App.root`
  (`plan/tui.rs:100`) is the crate's other dead field, under
  `#[allow(dead_code)]`.

### Tier 3: ride-alongs

`markdown_fmt.rs:620,672,783,891,896` production `unwrap`s -> `expect`
with the invariant; tuple indexing at `text.rs:31`, `markdown.rs:437`,
`markdown_fmt.rs:123,621,791`; `us`/`ivals`/`nseg` renames
(`markdown_fmt.rs:666` + 4); re-raising `match`es at
`bin/plan/main.rs:243,265` -> `?`; `tablethat_lib::` full paths while
`lib` is aliased at `:3`; `lib.rs:29-31` -> `pub use self::color::*`;
shared helpers duplicated across the two TUI modules (Interrupted-retry,
status line, `search_next`/`prev`); `&mut self` on render fns that never
mutate and gloss's unused `_cfg` param; `selected_field` magic numbers
vs a `FIELD_*` table; `columns[i]` relying on a clamp that lives only in
`rebuild_columns`.

## Steps

### Phase 1: theme cluster

- [ ] Step 1: `render_inline` composes `th.bold`/`th.dim`; keep the base
      modifier through `Strikethrough`. Add a render-level assertion so
      the theme test cannot go inert again (it currently asserts
      storage, not output).
- [ ] Step 2: `ratatui_to_termcolor` uses `termcolor::Color::Rgb`
      directly and handles `Color::Reset`; drop `rgb_to_ansi256` if it
      becomes unused.
- [ ] Step 3: unknown `--theme` is an error listing available themes;
      `themes.first()` over indexing.
- [ ] Step 4: replace the no-op theme field reads with a real property
      (round-trip or render assertion).
- [ ] Step 5: unicode fixture locking the `line[..col-1]` byte-offset
      invariant at `markdown_fmt.rs:545,709`.

### Phase 2: CLI diagnostics

- [ ] Step 6: justfile `check` adds `--all-targets`; fix the 8 warnings.
- [ ] Step 7: drop the doubled `error: ` prefix; `launch.rs` basename
      via `Path::file_name`.
- [ ] Step 8: `config.width` — either wire it (CLI > config > terminal
      width, requiring `Option<usize>`) or delete the field and its docs
      row. Decide before starting.
- [ ] Step 9: `DefaultView` enum with
      `#[serde(rename_all = "lowercase")]`, exhaustive match, one
      `list_tasks` call with a named `kanban` local.
- [ ] Step 10: four `// SAFETY:` comments on the test `set_var` blocks
      (unique `TESTAUTO*_CONFIG` prefix, no concurrent `getenv`).
- [ ] Step 11: match guard on `ErrorKind::Interrupted` replacing the
      box/downcast round trip (4 sites).
- [ ] Step 12: `truncate_slug` doc: byte length, not display width.

### Phase 3: error architecture

- [ ] Step 13: `pub enum Error` + `pub type Result<T>` in `lib.rs`,
      `Internal` newtype for dependency errors; migrate `plan/*` away
      from `String`; migrate the TUI modules off `Box<dyn Error>`;
      delete both `err_chain` copies and the `io::Error::other(String)`
      message-smuggling; add `CliError { Verdict, Failure }` at the
      binary boundary for distinct exit codes; update
      `docs/commands.md`.
- [ ] Step 14: `TaskFilter` struct replacing the 9-param `list_tasks`
      and the 5-`None` `load_and_filter_tasks` calls; drop the
      `too_many_arguments` allow.

### Phase 4: TUI cleanup

- [ ] Step 15: `gloss` `raw_lines` (delete or use as render base, drop
      the per-keystroke `read_to_string`); delete dead `App.root`.
- [ ] Step 16: extract the duplicated poll/retry, status line and search
      helpers; leave browse-vs-preview search semantics separate.
- [ ] Step 17: `&self` on render fns that don't mutate; delete `_cfg`.
- [ ] Step 18: `FIELD_*` consts + table-driven header; clamp-on-read or
      `debug_assert!` for `columns[i]`.

### Phase 5: ride-alongs

- [ ] Step 19: Tier 3 list, done opportunistically as those files are
      next touched.

## Not adopting

Recorded so a later conformance sweep does not "fix" them:

- `plan.rs` + `plan/` instead of `plan/mod.rs` — internally consistent
  across all three modules; converting is pure churn. Adopt for new
  modules only.
- Sweeping derive ordering — 12 sites, inconsistent in different ways;
  adopt going forward.
- Normalising error-message wording — `tests/cli_e2e.rs` asserts on
  message text (26 tests); resolves a style inconsistency by breaking a
  suite.
- `Task`/`Config` public fields -> builders — deserialization DTOs with
  `Default`; the guide prefers public fields there.
- Test-module `unwrap`s -> `expect` (138 sites) — the guide's rationale
  is that `unwrap` preserves the failing location. AGENTS.md's "avoid
  `unwrap`" should gain a "(production code)" caveat instead.
- Hoisting the `MarkdownTheme` literal's fields into `let`s
  (`markdown.rs:45`) — short builder chains; hoisting adds noise.

## Rejected during challenge

- `themes[0]` index panic in `bin/gloss/main.rs` — `theme.rs:151`
  injects a builtin default theme when none load. Safe.
- Mid-character slice panic at `markdown_fmt.rs:545,709` — comrak
  columns are byte offsets. Verified empirically: `plan format` over a
  fixture with em dashes, smart quotes, CJK and combining marks did not
  panic. Remains an untested invariant (Step 5).
- `th.bold`/`th.dim` "reserved for another render path" — no such path
  exists; single grep confirms one read, in a test.
- `assert!` on edit disjointness (`markdown_fmt.rs:125`) — keep as a
  release assert; it guards an internal invariant that has no other
  detector.
- `Color::Reset` mapping in `color.rs` — `termcolor` has no `Reset`
  variant, so falling through to `White` is a real change of rendered
  output, but the fix is a decision (skip emitting colour), not a
  one-liner.

## Acceptance

- `just precommit` green, with `check` running clippy on all targets.
- Each step a separate commit; behaviour changes carry a test that fails
  on the pre-fix code.
- `docs/themes.md`, `docs/config.md` and `docs/commands.md` updated
  wherever a field is wired up, deleted, or an exit code changes.
- No item from "Not adopting" is implemented as a drive-by.

## Progress (newest first)

- 2026-11-12: review captured. Nothing implemented; no source changed.
