---
name: hermes-tweet-signal
id: hermes-tweet-signal
version: 0.1.0
language: markdown
author: Xquik-dev
permissions:
  - network-public
requires_env:
  - XQUIK_API_KEY
triggers:
  - Hermes Tweet
  - X/Twitter signal brief
  - public tweet research
  - product feedback on X
  - launch reaction on X
description: |
  Build read-only X/Twitter signal briefs through an installed Hermes Tweet
  plugin. Uses endpoint discovery first, read-only routes second, and keeps
  action routes behind explicit user approval.
install:
  type: dotfile
  paths:
    - ~/.claude/skills/hermes-tweet-signal/
    - ~/.codex/skills/hermes-tweet-signal/
---

# Hermes Tweet Signal

Use this skill when a user asks for a concise public X/Twitter signal brief and Hermes Tweet is installed in the runtime. Xquik is an independent third-party service. Not affiliated with X Corp. "Twitter" and "X" are trademarks of X Corp.

## Requirements

- Hermes Tweet installed and enabled.
- `XQUIK_API_KEY` configured where Hermes executes tools.
- `HERMES_TWEET_ENABLE_ACTIONS` unset unless the user explicitly requests an action-capable workflow.

## Workflow

1. Confirm the topic, account, keyword set, and time window.
2. Use `tweet_explore` first to discover supported public read routes.
3. Use `tweet_read` only for public read-only endpoints returned by discovery.
4. Summarize recurring messages, linked accounts, source links, uncertainty, and follow-up searches.
5. Do not call `tweet_action` for a signal brief.

## Output Format

```markdown
## X/Twitter Signal Brief
- Topic:
- Window:
- Routes used:
- Strong signals:
- Accounts or links to inspect:
- Uncertainty:
- Follow-up searches:
```

## Guardrails

- Never ask for credentials in chat or files.
- Do not guess endpoint paths. Use `tweet_explore`.
- Do not call write, DM, follow, profile, monitor, webhook, extraction, or draw endpoints.
- If `XQUIK_API_KEY` is missing, report that endpoint discovery is the only available mode.
- If the user requests posting, replies, likes, or other actions, stop and ask for explicit approval before action-capable work.

## Source

- Hermes Tweet: https://github.com/Xquik-dev/hermes-tweet
- Skill source: https://github.com/KooshaPari/PhenoMCPServers/tree/main/skills/hermes-tweet-signal
