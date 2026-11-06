---
status: done
type: chore
priority: medium
area: devops
---

## Add caching to CI checks

### Problem

CI workflows install cargo-audit and cargo-deny from source on every run
(`cargo install cargo-audit cargo-deny` in audit.yml). This is slow and
rebuilds on every trigger. The main CI workflow already uses
`Swatinem/rust-cache` but the audit workflow does not.

### Current state

| Workflow    | Caching    | Install method                  |
| ----------- | ---------- | ------------------------------- |
| `ci.yml`    | rust-cache | taiki-e/install-action for just |
| `audit.yml` | None       | `cargo install` from source     |

### Fix

- Add `Swatinem/rust-cache` to audit.yml
- Switch audit.yml to `taiki-e/install-action` for cargo-audit and
  cargo-deny (pre-built binaries, same as ci.yml uses for just)
- Consider caching `~/.cargo/bin/` across workflows if install times are
  still slow
