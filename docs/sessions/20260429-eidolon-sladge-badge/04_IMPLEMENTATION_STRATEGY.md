# Implementation Strategy

## Approach

Keep the badge change small and docs-only:

- README receives the sladge badge below the logo.
- Session docs capture why the isolated worktree was required.
- No Rust code, event-bus integration, or branding asset changes.

## Rationale

Eidolon already had unrelated local brand and logo work. A separate worktree
allows the sladge WBS item to be prepared and committed without disturbing that
state.
