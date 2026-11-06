---
status: done
type: chore
priority: high
area: devops
---

## Release packaging and crates.io readiness

Purpose: Binary crates must commit Cargo.lock for reproducible builds.
Cargo.toml is missing metadata needed for crates.io. License file
doesn't match declared dual license.

Planned fix:

- Remove Cargo.lock from .gitignore, commit it
- Add `rust-version = "1.85"` to Cargo.toml
- Add `repository`, `keywords`, `categories`, `readme` to Cargo.toml
- Either add LICENSE-APACHE or change to `license = "MIT"` only
- Add release CI workflow: build binaries on tag push
  (Linux/macOS/Windows)
