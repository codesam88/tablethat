---
status: in-progress
type: feature
priority: medium
area: frontend
---

## Notes

Tune-up slice of `.plan/key-bindings.md` (browse + gloss viewer).
Decisions 2026-10-31 (user): hjkl untouched; Esc#1 clears filters,
Esc#2 arms, Esc#3 quits; browse search is filter-as-you-type WITH
highlight (replaces nothing; `f` unchanged); browse column order is
attention-ranked, list keeps config order; editor flags vi+emacs style
only (`+N`); then stop asking.

## Steps

- [x] A. Browse PgUp/PgDn: jump to last/first task; wrap only from ends.
- [x] B. Wrap-around Up/Down at grid edges.
- [x] C. Esc debounce: arm + `Press Esc again` hint; quit on 2nd Esc <2s.
- [x] D. Gloss PgUp/PgDn scroll by viewport height, not fixed 20.
- [x] E. Browse `/` filter-as-you-type over slugs + match highlight;
  Enter keeps filter, Esc clears. (Title-only `browse_title`; Up/Down
  confirm + move; footer search bar overrides status bar, no count
  before first keystroke.)
- [x] F. Editor opens at cursor line: `+N` for vi/emacs-family.
- [x] G. Browse columns attention-ordered (in-progress first, done last).
- [x] H. Check off done items in `key-bindings.md`.

## Deferred (not this batch)

Reversed list default, add-task dialog, configurable keys, multi-line
table cells, search history, subtler borders.

## Acceptance

- `just precommit` green; new unit tests for each pure helper;
  e2e untouched (TTY excluded). Each item committed separately.

## Progress (newest first)

- 2026-10-31: Search follow-ups DONE: title-only `browse_title` filter
  (separate from body-matching `filter_search`), Up/Down confirm filter
  and move via extracted `handle_search_key`, footer search bar replaces
  status bar (`/` pre-keystroke, `/q (n)` after). 5 search tests.
