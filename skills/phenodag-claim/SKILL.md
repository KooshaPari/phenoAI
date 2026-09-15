# Phenodag claim discipline

Use before parallel MCP fleet work across repos.

## Pre-flight

1. `phenodag pick --agent <name> --db MCP_FLEET_90.db`
2. `phenodag claim --agent <name> --repo <repo> --task <task-id>`
3. Heartbeat during long runs: `phenodag heartbeat --agent <name>`

## Rules

- One agent per task; release claims when done
- Do not edit the same repo/branch without a claim
- Mark done only for tasks assigned to your agent: `phenodag done --agent <name> --task <id>`
- Side-DAG tasks (`sd-*`) may run in parallel with core lanes after Wave 1 governance

## Presets

| DB | Preset | Shape |
|----|--------|-------|
| MCP_FLEET.db | mcp-fleet-60 | 60 tasks (complete) |
| MCP_FLEET_90.db | mcp-fleet-90 | 150 tasks (depth wave) |

## Anti-patterns

- Duplicate lanes on the same eco-* task
- Marking tasks done without assignment (will fail)
- Skipping dogfood pre-flight per [DOGFOOD.md](../../docs/DOGFOOD.md)
