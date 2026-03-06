_default:
  just --list -u

# Run all tests
test:
    cargo test

# Build the project
build:
    cargo build

# Check formatting and lints
lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings

# Auto-fix formatting
fmt:
    cargo fmt

# Update insta snapshots
snapshot:
    cargo insta test
    cargo insta review

# Install benchmark dependencies
setup:
    cd bench/babel && npm install

# Run benchmark: Babel vs oxc
bench:
    ./bench/run.sh

# Bump version and create release commit
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    # Update version in Cargo.toml
    sed -i '' 's/^version = ".*"/version = "{{version}}"/' Cargo.toml
    # Regenerate lockfile
    cargo check
    # Stage and commit
    git add Cargo.toml Cargo.lock
    git commit -m "chore: release crates v{{version}}"
