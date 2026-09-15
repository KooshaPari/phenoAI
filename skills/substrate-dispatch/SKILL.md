---
id: substrate-dispatch-skill
title: Substrate dispatch skill
version: 0.1.0
requires_servers:
  - substrate
triggers:
  - substrate dispatch
  - fleet plan
  - route task
permissions:
  - network
---

# Substrate dispatch skill

Use `substrate_dispatch`, `substrate_plan`, and `substrate_route` MCP tools from the
`substrate` server. Requires `SUBSTRATE_HTTP_URL` pointing at a running
substrate `driver-http` instance.
