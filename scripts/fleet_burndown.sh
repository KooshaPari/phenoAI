#!/usr/bin/env bash
# Weekly MCP fleet burndown — wraps phenodag burndown over MCP_FLEET_90.db
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DB="${PHENODAG_DB:-$ROOT/../phenodag/MCP_FLEET_90.db}"
PHENODAG="${PHENODAG_BIN:-phenodag}"

if [[ ! -f "$DB" ]]; then
  echo "missing fleet db: $DB (set PHENODAG_DB)" >&2
  exit 1
fi

echo "=== MCP fleet burndown ==="
echo "db: $DB"
"$PHENODAG" status --db "$DB"
echo
"$PHENODAG" burndown --db "$DB"
