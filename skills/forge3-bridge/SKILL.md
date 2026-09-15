---
name: forge3-bridge
id: forge3-bridge
version: 0.1.0
language: markdown
author: KooshaPari
requires_servers:
  - forge3-bridge
requires_binaries:
  - forge3
  - forge3-ctl
permissions:
  - network-localhost
triggers:
  - forge3
  - forge daemon
  - rpc.discover
  - agent list
  - tool list
  - model list
  - forge3-bridge
  - json-rpc over websocket
description: |
  Bridge MCP clients (Codex, Claude Desktop, Cursor) to the local Forge Agent SDK
  (forge3) JSON-RPC surface. Exposes 15 tools covering info/methods/tools/
  models/agents/commands/extensions + raw RPC pass-through + shell/search/read/
  write/patch/skill_search helpers + a transport-health doctor.
install:
  type: dotfile
  paths:
    - ~/.claude/skills/forge3-bridge/
    - ~/.codex/skills/forge3-bridge/
    - ~/bin/forge3-cli
    - ~/bin/forge3-mcp
    - ~/bin/forge3-ctl
---

# forge3-bridge skill

This skill teaches the host CLI how to invoke the Forge Agent SDK (`forge3`) over MCP.

## When to load

Load this skill when the user asks any of:

- "show forge3 tools / models / agents / extensions"
- "call `rpc.discover` on forge3"
- "run a shell command via forge3"
- "what methods does forge3 expose?"
- "is the forge3 daemon alive?" (`forge3_doctor`)
- "list forge3 commands / slash-ops"

Do NOT load for general shell commands or file edits that don't reference forge3.

## Mental model

```
┌────────────────────┐    JSON-RPC 2.0     ┌──────────────────────┐
│  MCP client        │ ◀─────────────────▶ │  forge3-bridge MCP   │
│  (Codex / Claude / │   stdio (line-      │  server              │
│   Cursor / ...)    │   delimited JSON)   │  (servers/forge3-    │
└────────────────────┘                     │   bridge/)           │
                                           └──────────┬───────────┘
                                                      │ stdio OR ws://127.0.0.1:9753
                                                      ▼
                                           ┌──────────────────────┐
                                           │  forge3 binary       │
                                           │  (Agent SDK)         │
                                           └──────────────────────┘
```

The host CLI invokes this skill → it routes through the MCP server → which calls
forge3 either via stdio (slower, always works) or via the long-lived `forge3 ws`
daemon (faster, recommended).

## How to use it

### Quick health check

Ask the host CLI:

> Use forge3_doctor to verify the bridge and tell me which transport is recommended.

### Enumerate capabilities

> Use forge3_methods to list every JSON-RPC method forge3 accepts.

> Use forge3_tools to list the LLM-facing tools (bash, read_file, edit, ...).

> Use forge3_models to list the models forge3 routes to.

> Use forge3_agents to list headless agent profiles.

> Use forge3_extensions to list loaded providers (forgecode, claude, codex, ...).

### Run a high-level tool

> Use forge3_shell with command="git log --oneline -5" to show the last 5 commits.

> Use forge3_search with pattern="TODO" path="." to find all TODOs.

> Use forge3_read with path="README.md" to read a file.

> Use forge3_patch with path, old_string, new_string to apply a precise edit.

### Raw JSON-RPC pass-through

For methods without a dedicated tool, use forge3_call:

```
forge3_call(method="agent_get_definition", params_json='{"id":"sage"}')
forge3_call(method="model_execute", params_json='{"name":"claude-opus-4-7","prompt":"..."}')
forge3_call(method="conversation", params_json='{"id":"abc","message":"hi"}')
```

## Protocol gotchas (the binary docs don't tell you)

1. **`id` MUST be a string UUID.** Integer ids silently fail on WebSocket — no reply.
2. **`params` for no-arg methods MUST be omitted or `null`.** `{}` returns `expected unit`.
3. **The WS daemon on `:9753` dies if TCP probes close mid-handshake.** Use a real
   WebSocket handshake (the `_run_mcp` test helper in `tests/test_server.py` shows
   the right way). Never `lsof -nP -iTCP:9753` and immediately `kill` — that drops
   the socket mid-handshake.

## Daemon lifecycle

The skill ships with `forge3-ctl`, a tiny Rust daemon manager:

```
forge3-ctl start    # spawn `forge3 ws --addr 127.0.0.1:9753` detached
forge3-ctl status   # check whether the daemon is alive (PID file, no TCP probe)
forge3-ctl stop     # SIGTERM the daemon (idempotent; cleans stale PID file)
forge3-ctl methods  # enumerate the 44 JSON-RPC methods forge3 advertises
forge3-ctl call M   # execute one RPC call (via stdio, no daemon required)
forge3-ctl install  # copy skill + CLI + MCP + ctl into the dotfile locations
```

If the bridge server is alive but no daemon is running, every call falls back to
spawning `forge3 stdio` per request (slower but always works).

## Discoverable JSON-RPC surface (44 methods, 32 extensions, 13 tools, 3 agents)

```
rpc.discover, info, tool_list, tool_execute, agent_list, agent_get_definition,
model_list, model_get, model_execute, extension_list, extension_get,
extension_subscribe, extension_unsubscribe, command_list, command_execute,
conversation, chat, widget, widget_subscribe, widget_unsubscribe,
secret_get, secret_set, secret_delete, event_subscribe, event_unsubscribe,
session_start, session_end, session_info, session_state, permission_grant,
permission_revoke, log_query, log_tail, notification_push, ... (full list via forge3_methods)
```

Extensions include: `forgecode`, `claude`, `codex`, `gemini`, `cursor`, `github`,
`workspace`, `config`, `secrets`, `notifications`, `events`, `permissions`.

Tools include: `bash`, `read_file`, `write_file`, `edit`, `grep_files`, `glob`,
`fetch_url`, `task_create`, `task_list`, `task_update`.

## Reference: stand-alone CLI

Outside of MCP contexts, the same surface is available as `forge3-cli`:

```bash
forge3-cli info
forge3-cli methods
forge3-cli tools
forge3-cli extensions
forge3-cli models
forge3-cli agents
forge3-cli commands
forge3-cli call tool_list
forge3-cli shell 'uname -a'
forge3-cli search 'TODO'
forge3-cli doctor
```

See `servers/forge3-bridge/README.md` for the dotfile install pattern.

## Reference: dotfile install

The recommended layout (after `forge3-ctl install`):

```
~/.claude/skills/forge3-bridge/   ← this SKILL.md + skill.json (dotfile-style copy)
~/.codex/skills/forge3-bridge/    ← same files (Codex reads the same shape)
~/bin/forge3-cli                  ← CLI wrapper
~/bin/forge3-mcp                  ← MCP server entry point (or `python servers/forge3-bridge/forge3_bridge_server.py`)
~/bin/forge3-ctl                  ← daemon manager (Rust)
```

The skill source-of-truth lives in `KooshaPari/PhenoMCPServers/skills/forge3-bridge/`.
`forge3-ctl install` is the symlinker/installer that copies into the dotfile
locations — exactly like `holman/dotfiles` clone + `install.sh` pattern.