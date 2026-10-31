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

- [ ] Step 0: `cargo update`, verify suite + deny/audit, commit lockfile.
- [ ] Step 1: frontmatter injection — validate values in `create_task`,
  YAML-quote strings in `value_to_yaml_str` (TDD).
- [ ] Step 2: gloss highlight Unicode panic — char-aware offsets (TDD).
- [ ] Step 3: release gates — test/clippy/audit before publish.
- [ ] Step 4: pin action SHAs.
- [ ] Step 5: `list_tasks` returns `Result` (drop lib `exit`).
- [ ] Step 6: slug truncation char boundary (+ helper test).
- [ ] Step 7: MSRV job runs check + smoke, not just test.
- [ ] Step 8: fix intra-doc links (`cargo doc` clean).
- [ ] Step 9: sorted `format <dir>` + surface `read_dir` errors (helper + test).
- [ ] Step 10: `multiple-versions` policy — decide, document, set.

## Acceptance

- `just precommit` green, `cargo deny check` green, `cargo doc` zero
  warnings, e2e suite green. Each step committed separately.
