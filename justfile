# Phenotype-org standard justfile

default:
    @just --list

# Watch for changes (cargo-watch)
dev:
    cargo watch -x check -x test

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

# Remove build artifacts
clean:
    cargo clean

# Security audits (cargo-deny + cargo-audit)
audit:
    cargo deny check
    cargo audit

# Find unused dependencies
unused:
    cargo machete

# Full local CI sweep
ci: lint test audit unused

# Generate docs
docs:
    cargo doc --no-deps --workspace
