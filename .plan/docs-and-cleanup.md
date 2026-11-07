---
status: done
type: chore
priority: medium
area: backend
---

## Documentation and code cleanup

Purpose: Several docs are stale after recent refactors. Dead code
accumulates. Cross-platform portability has gaps.

Planned fix:

- AGENTS.md line 61: change `.plan/tasks/<slug>.md` to `.plan/<slug>.md`
- README: add codesam2 to theme list (13 themes, not 12)
- Remove `parse_color()` from lib.rs (dead code, never called)
- Remove `open_preview()` from tui\_kanban.rs (dead code, replaced by
  OpenGloss)
- Deduplicate `status_color`/`priority_color` — currently in both
  tasks.rs and tui\_kanban.rs, move to lib.rs
- Replace `sensible-editor` with `vi` in editor fallback (Debian-only)
- Fix non-Unix `is_tty()` — currently always returns true, should detect
  piped stdin
