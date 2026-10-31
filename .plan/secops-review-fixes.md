---
status: in-progress
type: chore
priority: high
area: backend
---

## Notes

Fix batch from the 2026-10-31 posture review (all claims challenger-held,
parent-verified). Order: deps first, then data integrity, crash, gates,
then ride-alongs.

## Steps

- [x] Step 0: `cargo update`, verify suite + deny/audit, commit lockfile.
- [x] Step 1: frontmatter injection — validate values in `create_task`,
  YAML-quote strings in `value_to_yaml_str` (TDD).
- [x] Step 2: gloss highlight Unicode panic — char-aware offsets (TDD).
- [x] Step 3: release gates — test/clippy/audit before publish.
- [x] Step 4: pin action SHAs.
- [x] Step 5: `list_tasks` returns `Result` (drop lib `exit`).
- [ ] Step 6: slug truncation char boundary (+ helper test).
- [ ] Step 7: MSRV job runs check + smoke, not just test.
- [ ] Step 8: fix intra-doc links (`cargo doc` clean).
- [ ] Step 9: sorted `format <dir>` + surface `read_dir` errors (helper + test).
- [x] Step 10: `multiple-versions` policy — decide, document, set.

## Acceptance

- `just precommit` green, `cargo deny check` green, `cargo doc` zero
  warnings, e2e suite green. Each step committed separately.

## Progress (newest first)

- 2026-10-31: Step 5 DONE (TDD: test RED uncompilable, GREEN first pass).
  `list_tasks -> Result<(), String>`; 4 CLI arms use `?`; lib has zero
  `process::exit` left. Test uses `.plan`-as-file trick for portable
  unreadable-dir failure. 122 green.
- 2026-10-31: Step 4 DONE (config-only). All 18 action refs pinned to SHAs
  (resolved via GitHub API today) with `# vX` comments so Dependabot keeps
  updating them. YAML validated.
- 2026-10-31: Step 3 DONE (config-only). `verify` job (check + test +
  audit, same pinned actions as CI) gates `build`; chain verified via
  YAML parse. Tags can no longer ship red.
- 2026-10-31: Step 2 DONE (TDD: helper RED missing, then 1 wrong test
  expectation corrected — "Hello" matches too). `ci_match_ranges` maps
  lowercase expansions back to whole source chars; non-overlapping like
  `match_indices`; ASCII behavior identical. 4 tests, 121 green.
- 2026-10-31: Step 10 DONE out of order (user ran the experiment live:
  deleting the line surfaces only transitive ratatui-ecosystem dupes as
  warnings). Policy kept + rationale recorded in deny.toml comments.
- 2026-10-31: Step 1 DONE (TDD: 6 contract tests RED, GREEN first pass).
  `model::yaml_quote` (bare words pass through, specials double-quoted,
  empty stays bare); `create_task` rejects `\n`/`\r` values; `Null` arm
  keeps `area: ` stable (pre-existing empty->`null` wart fixed along the
  way). Canonical files byte-stable by construction (probe reverted).
  117 green.
- 2026-10-31: Step 0 DONE. `cargo update` moved lru 0.18.0->0.18.5,
  clearing RUSTSEC-2026-0253 — audit silent, watch closed. 110 green.
