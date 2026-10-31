# tablethat dev tasks — plan + gloss
# Install just: cargo install just
# Activate pre-commit hook: git config core.hooksPath .githooks

[private]
help:
    @just --list --unsorted

# cargo fmt
format:
    cargo fmt

# cargo fmt --check + cargo clippy
check:
    cargo fmt --check
    cargo clippy -- -D warnings

# cargo test
test:
    cargo test

# fmt check, clippy + tests (commit gate)
precommit:
    #!/usr/bin/env sh
    set -e
    just check
    just test

# build + smoke-test both CLIs (used in CI)
smoke:
    cargo build
    ./target/debug/plan --help
    @echo '# hi' | ./target/debug/gloss --width 80

# cargo build (debug)
build:
    cargo build

# cargo build (release)
build-release:
    cargo build --release

# cargo deny + cargo audit
audit:
    cargo deny check
    cargo audit

# install binaries + themes to ~/.config
install:
    cargo install --path .
    @mkdir -p ~/.config/plan/themes ~/.config/gloss/themes
    @cp themes/*.toml ~/.config/plan/themes/
    @cp themes/*.toml ~/.config/gloss/themes/
    @echo "Installed themes to ~/.config/plan/themes/ and ~/.config/gloss/themes/"

# uninstall binaries + copied themes
uninstall:
    cargo uninstall -p tablethat
    @rm -f ~/.config/plan/themes/*.toml ~/.config/gloss/themes/*.toml
    @echo "Removed binaries and themes"

# install cargo-deny + cargo-audit
install-dev:
    cargo install cargo-deny cargo-audit

# install dev tools + pre-commit hook
setup-dev: install-dev
    git config core.hooksPath .githooks
    @echo "Pre-commit hook enabled"
