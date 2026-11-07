---
status: done
type: bug
priority: high
area: backend
---

## Error handling: eliminate panics in user-facing code

Purpose: Several code paths use expect() which panics instead of showing
user-friendly errors. Broken pipe (plan list | head) causes panic.
Library code calls process::exit() which couples it to CLI behavior.

Planned fix:

- `open_task()`: use Command::status() match instead of expect() when
  gloss not found
- `init_plan()`: propagate errors with Result instead of expect()
- `write_colored!` macro: use let \_ = or ok() for I/O calls
- `list_tasks()`: return Result instead of calling process::exit()
- `filter::run_filter`: either implement ANSI color output or remove
  no\_color flag (needs exploration — ratatui can't emit ANSI to stdout,
  may need termcolor bridge)
