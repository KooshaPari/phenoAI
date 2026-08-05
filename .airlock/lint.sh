#!/usr/bin/env bash
# Airlock lint script — runs formatters then linters on changed files only
# Requires: cargo, rustfmt, clippy

set -euo pipefail

# Compute changed files between base and head SHAs
BASE_SHA="${AIRLOCK_BASE_SHA:-HEAD}"
HEAD_SHA="${AIRLOCK_HEAD_SHA:-HEAD}"

echo "=== Computing changed files: $BASE_SHA..$HEAD_SHA ==="

# Get list of changed files (exclude lockfiles, vendor, target, etc.)
CHANGED_FILES=$(git diff "$BASE_SHA" "$HEAD_SHA" --name-only 2>/dev/null | \
    grep -vE '\.lock$' | \
    grep -vE '^vendor/' | \
    grep -vE '^target/' | \
    grep -vE '\.git/' || true)

if [[ -z "$CHANGED_FILES" ]]; then
    echo "No relevant files changed."
    exit 0
fi

echo "Changed files:"
echo "$CHANGED_FILES"
echo

# Extract Rust source files
RUST_FILES=$(echo "$CHANGED_FILES" | grep -E '\.rs$' || true)

# === FORMATTERS ===
echo "=== Running formatters ==="

if [[ -n "$RUST_FILES" ]]; then
    echo "--- rustfmt (Rust) ---"
    for file in $RUST_FILES; do
        if [[ -f "$file" ]]; then
            cargo fmt -- "$file"
        fi
    done
fi

# === LINTERS ===
echo ""
echo "=== Running linters ==="

if [[ -n "$RUST_FILES" ]]; then
    echo "--- clippy (Rust) ---"
    # Run clippy on the workspace, filtered to changed files
    cargo clippy --workspace --all-targets -- -D warnings 2>&1 || {
        echo "Clippy found issues!"
        exit 1
    }
fi

echo ""
echo "=== All checks passed ==="
exit 0
