---
status: done
type: feature
priority: medium
area: backend
---

## General markdown support for `plan format` and `plan lint`

## Problem / purpose

`plan format [PATH]` already accepts an arbitrary file or directory, and
`plan lint` is documented as "validate task frontmatter", so users
reasonably point both at general (non-`.plan`) markdown trees. Against
the sample corpus at `tests/misc/` (~165 untracked `.md` files from
another project) both commands fail in ways that are useless for general
markdown:

1. `cargo run -q --bin plan -- lint -r tests/misc` → exit 1 with:
   `plan: error: no schema found (searched .plan/.schema.json, config dir, data dir)`.
   Cause: `validate_all` (`src/plan/lint.rs`) returns a hard `Err` when
   `resolve_file` finds no schema. A directory with no `.plan/`
   workspace and no schema cannot be linted at all — not even for
   checks (YAML well-formedness, mapping shape) that need no schema.
   User proposal: warn but continue without schema enforcement.
2. `cargo run -q --bin plan -- format tests/misc --check` → exit 1
   with exactly one failure:
   `tests/misc/openapi-reference.md: cannot normalize — YAML parse error: line 2 column 16: block sequence entries are not allowed in this context`
   (its frontmatter has `language_tabs: - shell: Shell`,
   Slate/Widdershins-style invalid YAML). Cause: `format_file`
   (`src/plan/format.rs:77`) routes any `.md` file starting with `---`
   through task-style `normalize_frontmatter`, and a
   `serde_saphyr::from_str` `Err` fails the whole file. All other ~164
   files format fine, so this single poison file fails the run.

Structural gaps behind the two failures:

- `normalize_frontmatter` applies task-canonical field reordering
  (`status`/`type`/`priority`/`area` first via `CANONICAL_ORDER`) and
  re-emits non-scalar values via `serde_saphyr::to_string` inline. Both
  are correct for task files but lossy/wrong for general markdown:
  reordering unrelated keys (e.g. `title`, `language_tabs`) is
  gratuitous churn, and inline re-emit collapses multi-line YAML
  structures even when parsing succeeds.
- `plan lint` takes no path argument (it always validates the whole
  `.plan` tree via `read_task_files`), while `plan format [PATH]`
  accepts a file or directory (`sorted_md_entries` for dirs,
  `read_task_files` on `.plan` with no path). There is no way to lint
  one file or one foreign directory.
- `parse_frontmatter` (`src/plan/model.rs:73`) splits on the first two
  `---` lines; delimiter detection already succeeds on
  `openapi-reference.md` — the format failure is at YAML-parse, not at
  splitting.

Root-cause update (2026-11-06, verified by probe): the invalid YAML in
`openapi-reference.md` was produced by `plan format` itself. Formatting
a file with valid multi-line frontmatter corrupts it —
`normalize_frontmatter` (`src/plan/format.rs:31`) embeds the multi-line
`serde_saphyr::to_string` re-emit of a list/map after `"{k}: "` on one
line, so continuation lines lose their indent and the output no longer
parses. Probe: `language_tabs:` + indented `- shell: Shell` became
`language_tabs: - shell: Shell` + dedented `- http: HTTP`; a second run
then fails with the exact `block sequence entries` error above. Any
`.md` with list/nested-map frontmatter formatted via an explicit PATH is
affected; `.plan/` task files are safe (flat scalar schema).
Consequence: already-corrupted files cannot be recovered by the new
formatter (Q2 skip leaves them untouched) — restore them from upstream
instead. The Q3 verbatim decision is now load-bearing: it removes the
corrupting re-emit from the general path entirely.

Purpose: make `plan format` and `plan lint` behave predictably on
general markdown (foreign trees, single files, no-schema directories)
while keeping task-workspace behavior byte-identical: `plan lint` on the
real `.plan/` tree passes today and must keep passing; canonical
task-field ordering must keep applying to task files.

## Solution strategy

Two-track approach, one per command:

- **lint**: separate "no schema resolvable" from "schema violations".
  When no schema is found, downgrade from hard `Err` to a warning and
  continue with schema-independent checks only (frontmatter delimiter
  present, YAML parses, frontmatter is a mapping). Schema-required-field
  and enum checks are skipped with the warning explaining what was not
  enforced. When a schema IS found, behavior is unchanged.
- **format**: separate "task frontmatter" from "general frontmatter".
  `format_file` keeps the current task path (canonical reorder +
  `value_to_yaml_str` scalar re-emit) for files inside a task workspace,
  and gains a general-markdown path that never reorders or re-emits
  frontmatter keys: frontmatter is preserved verbatim (byte-identical
  between the `---` delimiters) and only the body goes through
  `format_commonmark`. A file whose frontmatter does not YAML-parse is
  handled per open question Q2 below (default recommendation: skip the
  file with a warning, body untouched).

Rejected alternatives and their decisive drawbacks:

1. **Strip frontmatter from general markdown before formatting.**
   Drawback: data loss by design — format would silently delete
   metadata the tool does not understand. Unacceptable for a formatter.
2. **Fix up invalid YAML heuristically (e.g. special-case Slate-style
   `key: - item` sequences).** Drawback: unbounded dialect-specific
   repair logic inside a formatter; every new corpus adds a new dialect.
   The tool should be tolerant of unparseable frontmatter, not a repair
   tool for it.
3. **Route general markdown through `normalize_frontmatter` unchanged
   (task ordering for all files).** Drawback: reorders and potentially
   collapses frontmatter the schema knows nothing about; produces diff
   churn on every foreign file and can corrupt multi-line structures via
   inline `serde_saphyr::to_string` re-emit. Task-canonical ordering is
   a task-workspace invariant, not a general-markdown one.
4. **Auto-generate a permissive schema when none is found so lint can
   run unchanged.** Drawback: invents enforceable requirements (required
   fields) out of nothing; a lint that always passes its invented schema
   is theater. Skipping schema checks openly with a warning is honest.

Open questions (decision-or-defer — none of these are decided here;
the implementing worker must get answers before starting steps 2–5):

- **Q1 — schema-missing lint scope.** (a) Global relax: `validate_all`
  warns-but-continues whenever no schema resolves, including inside a
  `.plan` workspace? Or relax-only-for-explicit-PATH: hard error stays
  when linting a `.plan` workspace root (schema is guaranteed by `plan
  init` scaffolding, so its absence signals a broken workspace), and
  warn-but-continue applies only when lint is pointed at an explicit
  foreign path? Or keep the hard error everywhere and add a `--no-schema`
  opt-in flag? Recommendation: relax-only-for-explicit-PATH (see
  INTERVIEW in parent report).
- **Q2 — invalid-YAML frontmatter in format.** Skip-file-with-warning
  (file untouched, warning on stderr, exit 0 unless `--check`?) vs
  format-body-only (frontmatter bytes preserved, body still formatted)
  vs keep the hard error — and what `--check` reports and which exit
  code each variant yields. Recommendation: skip-file-with-warning;
  body-only formatting next to unparseable metadata risks splicing a
  formatted body under garbage delimiters.
- **Q3 — verbatim vs round-trip for general frontmatter.** Must
  task-canonical field reordering never touch general markdown
  (frontmatter preserved byte-verbatim, recommended), or is a
  parse-and-reserialize round-trip acceptable when parsing succeeds?
  Recommendation: byte-verbatim; round-trip through `serde_json::Value`
  cannot preserve comments, key order, or block-scalar styles.
- **Q4 — `lint` PATH argument.** Does `lint` gain an optional `[PATH]`
  (file or dir) for symmetry with `format [PATH]`, defaulting to the
  current whole-`.plan`-tree behavior with no arg? Recommendation: yes
  — without it Q1's relax-only-for-explicit-PATH has no explicit path
  to key on, and single-file lint is the natural debugging companion to
  single-file format.
- **Q5 — fate of the `tests/misc` corpus.** It is ~165 untracked files
  — far too heavy to commit as fixtures. Curate 2–4 representative
  files into `tests/fixtures/` (or inline strings in unit tests)?
  Suggested set: one no-frontmatter file (e.g. `admin-cli.md` style),
  one valid frontmatter file, `openapi-reference.md` (invalid Slate
  YAML), and one `---`-body file where `---` is a thematic break, not
  frontmatter (if such exists in the corpus; else synthesize). Remainder
  stays untracked local-only corpus, never committed. Recommendation:
  inline strings for unit tests + at most 2–4 tiny curated fixture
  files.

## Decisions (settled with user 2026-11-06 — all recommendations accepted)

- **Q1 → relax-only-for-explicit-PATH.** Missing schema stays a hard
  error for `.plan` workspace roots; warn-but-continue (parse + mapping
  checks only) applies when lint is pointed at an explicit foreign path.
- **Q2 → skip-with-warning.** Invalid-YAML frontmatter: file left
  byte-untouched, stderr warning; `--check` lists it as skipped, exit 0
  unless other files would reformat.
- **Q3 → byte-verbatim.** General-markdown frontmatter passes through
  untouched; only the body is formatted.
- **Q4 → yes.** `lint` gains optional `[PATH]` mirroring
  `format [PATH]`.
- **Q5 → no objection.** Inline strings + at most 2–4 tiny fixtures
  under `tests/fixtures/general-md/`; corpus stays untracked.

## Implementation steps

Each step is sized for one worker invocation. Steps run in order; each
leaves the tree green (`just check` clean) before the next starts.

### Step 1 — Failing-behavior-first characterization tests (red)

Scope: add tests only, no `src/` behavior change. Files:
`src/plan/lint.rs` (tests module), `src/plan/format.rs` (tests module).
New tests (behavioral invariants per AGENTS.md testing policy — assert
observable behavior, never literals; each must fail on current code):

- lint without schema: `validate_all` on a tempdir with `.md` files but
  no schema returns `Ok` (post-Q1 behavior) or the agreed Q1 variant —
  writes the test to the Q1 answer; initially red because current code
  returns the hard `no schema found` Err.
- lint schema-independent checks still fire without schema: malformed
  frontmatter / non-mapping frontmatter in a schema-less tree still
  produces errors (guards against a vacuous always-Ok implementation).
- format preserves general frontmatter verbatim: a file with valid but
  non-task frontmatter (multi-line list + comment) round-trips its
  frontmatter bytes exactly; body still formatted.
- format never corrupts multi-line frontmatter (regression for the
  2026-11-06 probe): a file with a valid indented list (`language_tabs:`
  + `  - shell: Shell`) formats to output whose frontmatter still
    YAML-parses to the same value — red today because the re-emit
    produces `language_tabs: - shell: Shell` + dedented items, which no
    longer parses (second run fails). This is the highest-priority case:
    current code destroys user data.
- format on Slate-style invalid YAML behaves per the Q2 answer (default:
  `Err`-free skip leaving bytes identical, warning surfaced) — red
  today because `format_file` returns `cannot normalize — YAML parse
  error`. NOTE: files already corrupted by the old re-emit (e.g. the
  `tests/misc/openapi-reference.md` copy) stay skipped, never repaired;
  restore those from upstream.
- fixpoint: formatting the formatted output changes nothing (idempotence
  guard for the new general path).

Done-criteria (exact commands, all from repo root):

```bash
cargo test lint_no_schema
# expect FAIL (red): returns Err(["error: no schema found ..."]) instead of Ok

cargo test general_frontmatter_verbatim
# expect FAIL (red): canonical reorder touches non-task keys

cargo test no_corrupt_multiline_frontmatter
# expect FAIL (red): re-emit dedents list items, output no longer parses

cargo test slate_invalid_yaml
# expect FAIL (red): Err("... cannot normalize — YAML parse error: ...")
```

Leaves-out: any change to `lint.rs`/`format.rs` non-test code, CLI
wiring, docs, fixtures.

### Step 2 — Lint change (schema-optional validation)

Scope: `src/plan/lint.rs` (`validate_all`: split schema resolution from
per-file checks; warn-and-continue without schema; skip required/enum
checks when schemaless), plus `src/bin/plan/main.rs` `Commands::Lint`
arm only if Q4 is answered yes (add optional `path: Option<PathBuf>`,
thread through to `validate_all` or a new entry point). No format code.

Done-criteria:

```bash
cargo test lint_no_schema
# expect ok

cargo test --lib plan::lint
# expect ok, all existing lint tests still pass unmodified

cargo run -q --bin plan -- lint -r tests/misc
# expect exit 0 (or the Q1-agreed code), stderr carries a
# "warning: no schema found ..." line, schema-independent diagnostics
# (if any) listed, no "plan: error: no schema found ..." hard error

cargo run -q --bin plan -- lint -r .
# expect exit 0, real .plan/ tree still validates exactly as before
```

Leaves-out: format behavior, docs, fixtures, any schema-format change.

### Step 3 — Format change (verbatim general frontmatter + invalid-YAML policy)

Scope: `src/plan/format.rs` only (`format_file` dispatch: task path vs
general path; verbatim-frontmatter preservation; Q2 invalid-YAML
branch), plus `src/bin/plan/main.rs` `Commands::Format` arms only
insofar as exit codes/warning plumbing for the Q2 branch require it. No
lint code.

Done-criteria:

```bash
cargo test general_frontmatter_verbatim
cargo test slate_invalid_yaml
cargo test --lib plan::format
# expect all ok, all pre-existing format tests pass unmodified

cargo run -q --bin plan -- format tests/misc --check
# expect per the Q2 answer (default: exit 0-or-agreed code; stderr names
# openapi-reference.md as skipped-with-warning; the other ~164 files
# report only genuine would-format diffs, no "cannot normalize — YAML
# parse error" hard failure)

cargo run -q --bin plan -- format --check
# expect exit 0 on the real .plan/ tree (task canonicalization unchanged)
```

Leaves-out: lint behavior, docs, fixtures.

### Step 4 — Docs update (`docs/commands.md`, `docs/tasks.md` if referenced)

Scope: docs only. `docs/commands.md`: document `lint [PATH]` (if Q4 yes)
or schema-optional lint behavior, the general-markdown format contract
(verbatim frontmatter, Q2 invalid-YAML policy with exit codes, `--check`
reporting), and which behavior applies to task workspaces vs foreign
paths. `docs/tasks.md`: touch only if it describes format/lint — keep
the edit to the minimal correcting delta.

Done-criteria:

```bash
just check
# expect clean (docs-only change: fmt --check + clippy unaffected)

grep -n "no schema" docs/commands.md
# expect the warn-but-continue contract stated with exit codes

grep -n "verbatim" docs/commands.md
# expect the frontmatter-preservation contract stated
```

Leaves-out: code, tests, fixtures.

### Step 5 — Fixture curation (2–4 files, corpus stays untracked)

Scope: create `tests/fixtures/general-md/` (or extend existing
`tests/golden/` layout if the worker finds a fitter home — check
`tests/` first) with 2–4 small curated files per Q5, plus wire them into
an integration test (`tests/cli_e2e.rs` style: run `plan format`/`plan
lint` against the fixture dir, assert behavioral outcomes). Verify
`git status --porcelain tests/misc` still shows the corpus untracked and
no misc file is copied wholesale.

Done-criteria:

```bash
cargo test --test cli_e2e
# expect ok, including the new general-md cases

ls tests/fixtures/general-md | wc -l
# expect 2–4

git status --porcelain | grep -c misc
# expect 0 tracked changes under tests/misc (corpus untouched/untracked)
```

Leaves-out: `src/` behavior changes, docs, gloss, TUI.

## Out-of-scope (explicitly not this plan)

- `gloss` changes of any kind (viewer, filter mode, rendering).
- Schema-format changes: `.plan/.schema.json` shape, `plan init`
  scaffolding, required-field/enum semantics when a schema exists.
- Fixing the invalid Slate/Widdershins frontmatter in
  `openapi-reference.md` upstream or in the corpus — the corpus file
  must stay byte-identical as the regression case.
- TUI work (`plan browse`/kanban interactivity).
- Committing, vendoring, or otherwise absorbing the 165-file
  `tests/misc` corpus into the repo.

## Self-review (coverage, placeholders, consistency)

- Both failure transcripts from the problem statement appear verbatim
  with causes pinned to file/line (`lint.rs validate_all`,
  `format.rs:77` `normalize_frontmatter`): covered, no re-derivation
  beyond the two permitted `read`s (corpus spot-checks already supplied
  as verified facts).
- All five decision points (a)–(e) appear as open questions Q1–Q5
  with recommendations but no decisions: covered; steps 1–3 are
  written against the defaults but explicitly conditional on interview
  answers.
- Steps ordered failing-tests → lint → format → docs → fixtures,
  one worker invocation each, each with scope/files, exact-command
  done-criteria, and leaves-out: covered.
- Out-of-scope names gloss, schema-format, upstream Slate fix, TUI:
  covered, plus corpus non-absorption.
- No TBD/TODO placeholders: checked — none present. (Q1–Q5 are open
  questions by requirement, each with a recommendation, not
  placeholders.)
- Consistency: step 2's done-criteria keep `lint -r .` green (matches
  "real `.plan/` tree currently passes"); step 3's keep `format --check`
  green on `.plan/` (matches "task-canonical ordering must keep applying
  to task files"); AGENTS.md test policy (behavioral, non-vacuous,
  fixpoint) cited in step 1; `just check` before commit noted per step.

## Progress log

- 2026-11-07 — Step 1 done (7 tests, all red for the right reasons;
  `cargo test --lib plan::` 70 passed / 7 new failed; `just check`
  green). Review fixes by parent:
  `general_frontmatter_verbatim_preserved` body made genuinely overlong
  (67→92 cols; short bodies pass through the split-only engine
  untouched, so `assert_ne!` body-changed would have failed post-fix);
  `slate_invalid_yaml_is_skipped` bool-assert rewritten (`assert!`,
  clippy-clean). Step 2 authorized next.
- 2026-11-07 — Step 2 implemented (worker) + parent review. Findings:
  (a) corpus incident — `tests/misc/openapi-reference.md` was
  rewritten at 23:48 (frontmatter lines 3–11 changed) by an unknown
  hand; worker audit shows read-only commands. Parent restored
  first-seen bytes (body verified identical, lines 19+); both original
  failures reproduce. If you edited it intentionally, say so —
  otherwise avoid touching `tests/misc/` during the build. (b) Design
  gap caught in review: schemaless lint FAILs files with no frontmatter
  at all (`admin-cli.md`, `SUMMARY.md`, …) — useless on general
  markdown, where most files have none. Decision: schemaless + file does
  not start with `---` → skip silently (nothing to validate); schema
  mode keeps today's error; `---`-led files keep full structural checks
  in both modes. Fix-up (skip + `lint_no_schema_no_frontmatter_skipped`
  test) sent back to worker before Step 3.
- 2026-11-07 — Fix-up verified by parent: `plan::lint` 6/6 green; full
  `plan::` 74 pass / 4 fail (exactly the Step-1 format reds);
  `lint -r tests/misc` now reports only genuine `---`-led breakage
  (openapi-reference.md YAML error, exit 1 — correct, lint must fail
  broken files); workspace `lint -r .` exit 0. Step 3 authorized next:
  general-verbatim format path + invalid-YAML skip, plus task-path
  hardening of the multi-line re-emit (`{k}:` + block lines, never
  inline) so exotic task values cannot corrupt either.
- 2026-11-07 — Step 3 done + parent-verified: `plan::` 79/79 green,
  full `cargo test` 240 green, `just check` clean. Dual dispatch
  accepted (content-based in `format_file` = defense in depth;
  location-based in main.rs). Verified: corpus `--check` = 1 line
  (openapi skip-warning), exit 0; write-mode skip is byte-identical;
  task list-values emit block style; `.plan` tree canonical. Steps 4
  (docs) + 5 (fixtures) authorized in parallel.
- 2026-11-07 — Steps 4+5 done + parent-verified: docs describe
  `lint [PATH]` / task-vs-general contracts (grep hits + help
  transcripts ok); 4 hand-written fixtures + 3 e2e tests, cli_e2e 22/22
  green; full `cargo test` 243 green; `just check` clean; corpus
  untouched (still untracked). Plan complete.
- 2026-11-07 — Follow-up polish: `--root` help now states workspace
  context (config/schema/.plan discovery); `(overrides ...)` dropped
  from format help flags; `--root` vs `[PATH]` contrast documented in
  `docs/commands.md`. Full suite + `just check` green.
- 2026-11-07 — Step 3 done + parent-verified: `plan::` 79/79 green,
  full `cargo test` 240 green, `just check` clean. Dual dispatch
  accepted (content-based in `format_file` = defense in depth;
  location-based in main.rs). Verified: corpus `--check` = 1 line
  (openapi skip-warning), exit 0; write-mode skip is byte-identical;
  task list-values emit block style; `.plan` tree canonical. Steps 4
  (docs) + 5 (fixtures) authorized in parallel.
