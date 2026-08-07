#!/usr/bin/env bash
# Lint script for changed files in this repository.
# Uses AIRLOCK_BASE_SHA and AIRLOCK_HEAD_SHA to compute changed files.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Default to HEAD if not in a diff context (e.g., running against working tree)
BASE_SHA="${AIRLOCK_BASE_SHA:-HEAD}"
HEAD_SHA="${AIRLOCK_HEAD_SHA:-HEAD}"

echo "=== Lint setup ==="
echo "Base: $BASE_SHA"
echo "Head: $HEAD_SHA"

# Determine files to lint
if [[ "$BASE_SHA" == "$HEAD_SHA" ]]; then
    echo "No diff context; linting working tree Rust files..."
    CHANGED_RS_FILES=$(git ls-files '*.rs' 2>/dev/null || true)
    CHANGED_TOML_FILES=$(git ls-files '*.toml' 2>/dev/null || true)
else
    CHANGED_RS_FILES=$(git diff --name-only "$BASE_SHA" "$HEAD_SHA" -- '*.rs' 2>/dev/null || true)
    CHANGED_TOML_FILES=$(git diff --name-only "$BASE_SHA" "$HEAD_SHA" -- '*.toml' 2>/dev/null || true)
fi

echo ""
echo "=== Phase 1: rustfmt (auto-fix) ==="
if [[ -n "$CHANGED_RS_FILES" ]]; then
    echo "Formatting: $CHANGED_RS_FILES"
    echo "$CHANGED_RS_FILES" | xargs rustfmt --edition 2021
else
    echo "No .rs files changed — skipping."
fi

echo ""
echo "=== Phase 2: clippy (auto-fix) ==="
# clippy has no auto-fix for workspace, but we run it to check
if [[ -n "$CHANGED_RS_FILES" ]]; then
    echo "Checking: $CHANGED_RS_FILES"
    # Run clippy on the specific files with warnings-as-errors
    echo "$CHANGED_RS_FILES" | xargs cargo clippy --message-format=short 2>&1 || true
else
    echo "No .rs files changed — skipping."
fi

echo ""
echo "=== Phase 3: fmt check ==="
cargo fmt --all -- --check

echo ""
echo "=== Phase 4: clippy check ==="
cargo clippy --all-targets --all-features -- -D warnings

echo ""
echo "=== All checks passed ==="
exit 0
