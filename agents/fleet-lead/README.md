# Fleet lead agent

Coordinates team mailbox and substrate dispatch for multi-agent fleets.

## Config

- `agents/fleet-lead/agent.yaml` — persona, default server, skills
- `catalog/registry.yaml` → `agents.fleet-lead`

## Skills

| Skill | Purpose |
|-------|---------|
| `substrate-dispatch-skill` | `substrate_dispatch`, `substrate_plan`, `substrate_route` |
| `mcp-boundary-guard` | ADR-017 layer selection |
| `github-fork-policy` | `gh repo fork` only |

## Dogfood

See [docs/DOGFOOD.md](../../docs/DOGFOOD.md) for the ADR-018 zero-loop ritual (eco-027).
