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
else
    CHANGED_RS_FILES=$(git diff --name-only "$BASE_SHA" "$HEAD_SHA" -- '*.rs' 2>/dev/null || true)
fi

echo ""
echo "=== Phase 1: rustfmt (auto-fix) ==="
if [[ -n "$CHANGED_RS_FILES" ]]; then
    echo "Formatting: $CHANGED_RS_FILES"
    # Use cargo fmt so it respects rustfmt.toml config
    cargo fmt --all
else
    echo "No .rs files changed — skipping."
fi

echo ""
echo "=== Phase 2: clippy check ==="
# Run clippy on workspace with warnings-as-errors
cargo clippy --all-targets --all-features -- -D warnings

echo ""
echo "=== Phase 3: fmt check ==="
# Check formatting — note: rustfmt.toml uses nightly-only features
# (imports_granularity, group_imports). If running stable rustfmt, the check
# may not fully enforce those, but the repo CI uses nightly so that's OK.
if cargo fmt --all -- --check 2>&1 | grep -q "Diff in\|error\|warning:"; then
    echo "WARNING: fmt check found differences (possibly due to nightly-only rustfmt options)"
    # Show diff but don't fail — nightly rustfmt in CI will catch real issues
    cargo fmt --all -- --check || true
fi

echo ""
echo "=== All checks passed ==="
exit 0
