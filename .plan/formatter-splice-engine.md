---
status: in-progress
type: feature
priority: medium
area: backend
---

## Splice-engine markdown formatter

Replace the format round-trip with conservative source-splicing:
parse for structure, apply desired edits only, re-emit the rest
byte-identical. Fixes short-line joining, hardbreak restyling,
pipe-in-code-span table loss.

- Research: `docs/briefs/markdown-formatting.md`
- Branch: `formatter/splice-engine` (worktree `/tmp/opencode/fmt-spike`
  holds the earlier dprint/flowmark spike, `spike/dprint-eval`)

Done: shared `comrak_options()` constructor (renderer + formatter).
Requirements v3: configurable prose wrap (default 80, split-only,
containers wrapped with continuation indent, code spans atomic).
D-2 confirmed (keep pad-to-width pipe style).
Implemented (4 worker items, senior-reviewed, all committed):
skeleton + ws-strip, AST table alignment (incl. nested/quote fix),
wrap + wiring + old-code deletion, format_width/format_reflow +
reflow joins (restructured to `[format]` section + `docs/config.md`).
Perf: whole-.plan format ~30ms, fixpoint clean. Open: PR checks,
then merge decision (user).

## Plan

Status: design (requirements + architecture + validation). Implementation
breakdown follows in a second round.

### 1. Requirements

#### Goal

Replace the formatter's parse→re-render round-trip with a conservative
**splice engine**: parse for structure, apply only the desired edits as
source-range replacements, re-emit everything else byte-identical. The
engine wraps prose to a configurable width (80) — splitting overlong
lines only, never joining short ones — while tables, verbatim blocks,
and headings pass through untouched. Fixes the three reproduced bugs
(short-line joining, hardbreak restyling, pipe-in-code-span table
corruption) without adopting an external engine (dprint corrupts the
same tables; flowmark reflows prose and drags a second comrak; mdformat
is Python — see spike notes from `spike/dprint-eval`).

Repo grounding (`.plan/` corpus, Nov 2026): ~980 lines across ~18 files,
mostly short notes; real GFM tables exist (`rendering-tests.md`); long
lines include task-list items with inline code. Wrapping therefore
applies inside enumerations too (with continuation indent), never to
tables or code blocks.

#### Functional requirements (v1 normalization set)

- **FR-1 — Join policy follows `format_reflow`.** With reflow off,
  soft line breaks pass through untouched; short lines are never drawn
  up into the line above. With reflow on (default), each paragraph unit
  is joined into word flow and re-split to the width.
- **FR-2 — Preserve hardbreak flavor.** `two-spaces + newline` and
  `backslash + newline` are both kept as authored, and each ends the
  current wrapping unit: segments wrap (and reflow) independently.
  (Reproduced bug 2.)
- **FR-3 — Align tables without content loss.** Column padding via
  parser-given cell boundaries; pipes inside code spans or escaped pipes
  are never treated as delimiters. Tables are never wrapped.
  (Reproduced bug 3; data loss is the release-blocker of this project.)
- **FR-4 — Wrap overlong prose to a configurable width (default 80).**
  Only source lines exceeding the width are split, at word boundaries;
  nothing is ever joined. Applies to top-level paragraphs *and* prose
  inside enumerations, definition lists, and blockquotes, with
  continuation lines re-indented to the content column (`- ` → two
  spaces, `1. ` → three, `> ` preserved, nesting accumulates).
- **FR-5 — Inline code spans are atomic words.** A code span never
  breaks across lines; a single word (code span or otherwise) longer
  than the width is left overflowing — width is a preference, not a
  cap (Prettier's printWidth principle).
- **FR-6 — Verbatim and headings pass through.** Fenced/indented code
  blocks, ATX/setext headings, HTML blocks, thematic breaks: untouched.
- **FR-7 — Strip trailing whitespace**, exempting hardbreak markers
  (FR-2): the stripper must not delete a meaningful double-space.
- **FR-8 — Everything else byte-identical.** Markers, heading styles,
  blank-line runs, code-span backticks, reference links are not opinions
  v1 holds.
- **FR-9 — Contracts unchanged.** `plan format` CLI, `format_file`
  changed-flag, `normalize_all` fixpoint, and the frontmatter path in
  `src/plan/format.rs` (canonical order, YAML quoting) are untouched;
  width/reflow reach the engine from user configuration (FR-10).
- **FR-10 — Format configuration.** `format_width` (default 80) and
  `format_reflow` (default true) live in `Config` with standard layering
  (defaults < `plan.toml` < `PLAN_FORMAT_WIDTH` / `PLAN_FORMAT_REFLOW`
  env), documented in `plan.toml.example` alongside the other widths.
  No CLI flags in v1 (follow-up note).

#### Non-functional requirements

- **NFR-1 — Fixpoint.** Formatting twice is byte-identical, always.
- **NFR-2 — HTML-equivalence.** Rendered HTML before == rendered HTML
  after, for every input. Content preservation is machine-checked, not
  eyeballed.
- **NFR-3 — No new dependencies.** comrak 0.59 (already depended on,
  `sourcepos` on every node) is the only parser. MSRV 1.89 and static
  musl builds unaffected.
- **NFR-4 — Performance.** Formatting a typical task file stays in the
  low milliseconds; measure the `.plan/` corpus before/after, no
  regression beyond noise. (`plan` starts in ~30 ms; formatting must not
  dominate the pre-commit hook.)
- **NFR-5 — Purity.** The engine stays a pure `&str -> String` function
  (no I/O), so every behavior is unit-testable.

#### Non-goals (v1)

Indented→fenced conversion (dropped deliberately: diff noise, not
normalization). Marker/heading/reference-link/code-span opinions.
Table multiline cells (already deferred elsewhere). Frontmatter changes.
Sentence-per-line (semantic) breaks — wrapping is width-driven only.

#### Open decisions (need answers before implementation)

- **D-2 — Table alignment style:** keep the current pad-to-width pipe
  style, only fixing the mechanism? Confirmed — keep style.

Decided without review: width math uses byte columns (matches the
current `align_tables` behavior; corpus is ASCII). Unicode display width
is a follow-up note, not v1 scope.

### 2. Architecture

#### Current state

`format_commonmark` (`src/markdown_fmt.rs`): build options → comrak
`markdown_to_commonmark` with `render.width = 120` (this setting is the
reflow mechanism behind bug 1) → hand-rolled `align_tables` string
splitter (bug 3's mechanism) → blanket trailing-whitespace strip (would
eat hardbreaks if comrak hadn't pre-converted them — bug 2's churn).
Options constructor is now shared with the renderer (`comrak_options()`).

#### Target design

```
source lines ──► parse (shared opts) ──► walk AST + sourcepos
                        │                          │
                        │                   collect edits
                        │                    (line ranges)
                        ▼                          ▼
                 apply edits (sorted, non-overlapping, asserted)
                        │
                        ▼
                 output lines (untouched ranges verbatim)
```

The CommonMark renderer leaves the format path entirely. Edits are the
only thing the engine emits; everything else is the input.

- **Edit model.** Three edit kinds, each a 1-based line-range
  replacement on the source lines: `StripTrailingWs { line }`
  (FR-7, hardbreak-exempt), `AlignTable { start, end }` (FR-3, cell
  ranges from `TableRow → TableCell` node positions), `WrapText
  { start, end }` (FR-4/FR-5: re-emit the source range's words,
  split-only, prefix-aware, code spans atomic). Application
  asserts ranges are sorted and disjoint; violation is a bug, surfaced
  as a test failure, never silent overlap.
- **Table alignment.** Cell boundaries come from the AST, never from
  scanning for `|`; display widths computed per column, rows rewritten
  within their line spans. Separator-row markers (`:---:`) preserved
  from the parsed alignment.
- **Wrapping.** Candidate source lines come from `Paragraph` (and
  definition-list definition) nodes: only lines over width are
  re-emitted. Words are whitespace-separated tokens with inline code
  spans resolved to atomic units via each span's source range (a span
  is never split even across a soft break it contains). The
  continuation prefix is derived from the container chain
  (blockquote `> `, list marker width, definition indent) and
  re-applied to every emitted line after the first. Hardbreak nodes
  split the wrapping unit (FR-2). A single overlong word overflows
  untouched (FR-5).
- **Module layout.** All inside `src/markdown_fmt.rs`: `parse`,
  `collect_edits`, `apply_edits` as private helpers;
  `format_commonmark` keeps its signature (FR-9) and becomes
  parse-collect-apply. `src/plan/format.rs` and `src/markdown.rs`
  unchanged by this project.
- **Frontmatter boundary.** Unchanged: `plan/format.rs` strips
  frontmatter first and feeds body-only text to the engine.

### 3. Validation strategy

- **TDD, one failing test per FR before implementation:** short
  soft-break lines preserved (never joined); a 600+ char list item
  wraps with continuation indent; hardbreak flavors preserved with
  independent segment wrapping; piped-code-span table aligns
  losslessly; overlong top-level paragraph splits at word boundaries;
  code span crossing the width stays atomic (neighbors break around
  it); single overlong word overflows; nested list / blockquote /
  definition prose wraps with correct prefixes; trailing-ws stripped
  with hardbreak exemption. Reflow tests (run with reflow on): short
  lines join into flow and re-split to width; containers reflow with
  indents; hardbreak segments stay independent; reflow fixpoint. Each
  test also asserts NFR-2 (HTML-equivalence) for its input.
- **Fixpoint corpus test:** a mixed file (all of the above plus headings,
  lists, code blocks, HTML) formats to a fixpoint; extends the existing
  `formatter_is_idempotent` pin rather than replacing it.
- **Corpus sweep (test, not committed files):** run the engine over every
  repo Markdown file (`.plan/`, README, CHANGELOG, docs) asserting
  fixpoint + HTML-equivalence. Catches construct coverage the unit tests
  miss.
- **Regression gates (unchanged):** `just precommit` (fmt/clippy/zizmor +
  full suite + goldens), `plan lint` exit 0. Intended behavior change to
  eyeball in review: indented code blocks are no longer converted to
  fenced (non-goal); the corpus sweep shows exactly which files that
  touches.
- **Performance gate:** time `normalize_all` over the `.plan/` corpus
  before/after; flag anything beyond noise per NFR-4.
- **Definition of done:** all FR tests green, fixpoint + equivalence
  green, gates green, CHANGELOG entry under `[Unreleased]`, branch
  pushed + PR opened (no merge without prompt).
