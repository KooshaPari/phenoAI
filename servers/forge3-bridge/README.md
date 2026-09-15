# forge3-bridge MCP server

Bridge to the [Forge Agent SDK](https://github.com/KooshaPari/forge3) (the local `forge3`
binary). Wraps the forge3 JSON-RPC 2.0 surface (over stdio or `ws://127.0.0.1:9753`)
as MCP tools so any MCP-aware client (Codex, Claude Desktop, Cursor, etc.) can call into
the Forge Agent SDK without spawning their own copy.

Framework: [PhenoFastMCP](https://github.com/KooshaPari/PhenoFastMCP) / fastmcp 3.4.2+.

## Tools exposed (15 total)

| Tool                  | What it does                                                              |
| --------------------- | ------------------------------------------------------------------------- |
| `forge3_info`         | `rpc.info` — full extension registry, enable state, version, capabilities |
| `forge3_methods`      | `rpc.discover` — every JSON-RPC method forge3 accepts                     |
| `forge3_tools`        | `tool_list` — LLM-facing tools (read/write/patch/search/shell/...)        |
| `forge3_extensions`   | `info.extensions` — providers, tools, MCP, ...                            |
| `forge3_models`       | `model_list` — every model forge3 routes to                               |
| `forge3_agents`       | `agent_list` — headless agent profiles (forge/muse/sage)                  |
| `forge3_commands`     | `command_list` — slash-style ops (auth, reload, ...)                      |
| `forge3_call`         | raw JSON-RPC passthrough (any method, any params)                         |
| `forge3_shell`        | run shell command via `tool.shell`                                        |
| `forge3_search`       | ripgrep-style code search via `tool.search`                               |
| `forge3_read`         | read a file via `tool.read`                                               |
| `forge3_write`        | write a file via `tool.write`                                             |
| `forge3_patch`        | patch a file via `tool.patch`                                             |
| `forge3_skill_search` | discover skills via `tool.skill_search`                                   |
| `forge3_doctor`       | probe ws + stdio transport health                                         |

## Install

```bash
# 1. Install Python deps
cd servers/forge3-bridge
pip install -r requirements.txt

# 2. Install Rust daemon manager (optional but recommended)
#    This binds forge3-ctl + the skill into ~/.claude/skills/ + ~/.codex/skills/ + ~/bin/
cargo install --path /path/to/forge3-ctl
forge3-ctl install

# 3. Run as MCP server (stdio)
python forge3_bridge_server.py

# Or after pip install -e .
forge3-bridge-mcp
```

## Wire into Claude Desktop / Codex / Cursor

Add to `~/.config/claude-desktop/config.json` (or equivalent MCP config):

```json
{
  "mcpServers": {
    "forge3-bridge": {
      "command": "python",
      "args": [
        "/abs/path/to/PhenoMCPServers/servers/forge3-bridge/forge3_bridge_server.py"
      ],
      "env": {
        "FORGE3_BIN": "/Users/kooshapari/.cargo/bin/forge3",
        "FORGE3_WS": "ws://127.0.0.1:9753"
      }
    }
  }
}
```

## Protocol gotchas (the docs the binary doesn't tell you)

- **`id` MUST be a string UUID.** Integer ids silently fail on WebSocket.
- **`params` for no-arg methods MUST be omitted or `null`.** `{}` returns "expected unit".
- **The WS daemon on `:9753` dies if TCP probes close mid-handshake.** Use a real
  WebSocket handshake for any health check.

## Transport selection

Default = auto: try `ws://127.0.0.1:9753` first, fall back to spawning `forge3 stdio`
per call. Override with `FORGE3_TRANSPORT=ws` or `FORGE3_TRANSPORT=stdio` env var.

## Sources of truth

- `forge3_cli.py` — shared JSON-RPC client used by both the standalone CLI and this server.
- `forge3_bridge_server.py` — `@mcp.tool()` definitions.
- `forge3_mcp.py` — legacy filename compatibility entrypoint; delegates to the canonical server.
- `tests/test_server.py` — pytest smoke for `initialize` + `tools/list` + one `tools/call`.
- Skill frontmatter and dotfile installer: `skills/forge3-bridge/`.
- Rust daemon manager: bundled as `bin/forge3-ctl` (download or `cargo install`).
