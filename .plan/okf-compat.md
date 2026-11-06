---
status: backlog
type: feature
priority: medium
area: backend
---

## Notes

Make `.plan/` directories fully OKF-conformant (https://okf.md/spec/,
v0.2) so external OKF tooling works on task bundles and users are never
locked in to `plan`. Research brief: `docs/briefs/okf-compat.md`
(verified against canonical SPEC.md).

Why it fits: OKF conformance is minimal (parseable frontmatter +
non-empty `type`), which `.plan/` already satisfies structurally. A
`.plan/` dir is a valid OKF bundle ("subdirectory within a larger
repository"); slugs are concept IDs.

Recommended direction (from brief — keep field names, no
rename/migration):
- Must: exclude reserved `index.md`/`log.md` from task listing in
  `store.rs` (today only `.TEMPLATE.md` is excluded — conformant OKF
  files would mis-parse as broken tasks); relax `.schema.json`
  (`additionalProperties: false`) + lint unknown-key warnings to allow
  OKF keys (`title`, `description`, `resource`, `tags`, `timestamp`,
  `sources`, `generated`, `verified`, `status`, `stale_after`).
- Should: emit `title`/`description`/`tags`/`generated` on tasks;
  document the `type` and `status` semantic overlaps as a profile note;
  OKF-friendly `plan init` template.
- Explicitly not changing: flat layout (valid subset), slug charset
  (document as boundary).

Risks: `status` collision is silent (OKF consumers filtering
`status: stable` hide every task); no task-notes-over-OKF precedent
exists (`lead`); RPM/`+` and rolling-version notes live in PR #33,
unrelated.
