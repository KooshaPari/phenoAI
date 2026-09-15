# Phenotype-org standard justfile

default:
    @just --list

# Build workspace
build:
    cargo build --workspace

# Run tests
test:
    cargo test --workspace

# Lint (clippy + fmt --check)
lint:
    cargo clippy --workspace -- -D warnings
    cargo fmt --check

# Format code
fmt:
    cargo fmt

# Security audits (cargo-deny + cargo-audit)
audit:
    cargo deny check
    cargo audit

# Find unused dependencies
unused:
    cargo machete

# Full local CI sweep
ci: lint test audit unused

# Hermetic Wayland / USER ns smoke gates (macOS-safe default test run)
smoke-hermetic:
    export PATH="/bin:/usr/bin:/opt/homebrew/bin:{{env('HOME')}}/.cargo/bin:$$PATH" && \
    cargo test -p eidolon-desktop --locked wayland_smoke_fails_loud_off_linux_or_without_wayland && \
    cargo test -p eidolon-desktop --features desktop-linux --locked wayland_host_gate_matches_target

# USER ns live smokes via privileged Docker (macOS or Linux hosts)
smoke-user-ns-docker:
    export PATH="/bin:/usr/bin:/opt/homebrew/bin:{{env('HOME')}}/.cargo/bin:$$PATH" && \
    ./scripts/linux-user-ns-smoke-docker.sh

# Generate docs
docs:
    cargo doc --no-deps --workspace
