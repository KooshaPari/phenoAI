# CUA satellite archive policy

**Decision (2026-07-20):** Keep `kmobile`, `mobile-cli`, `mobile-mcp`, and `KDesktopVirt` **archived**. Day-to-day Computer/Mobile Use work happens in **Eidolon** (+ **PlayCua** for sandbox).

## Why

- Eidolon is the unified trait-based runtime (`eidolon-desktop` / `eidolon-mobile` / `eidolon-sandbox`).
- Satellites are source material for extraction, not parallel products.
- Unarchiving invites Dependabot/CI noise without shipping user value.

## When to unarchive

Only for a time-boxed extraction PR that:

1. Copies salvageable modules into Eidolon (see [EXTRACTION_PLAN.md](../EXTRACTION_PLAN.md)).
2. Updates callers/tests in Eidolon.
3. Leaves the satellite **archived** again after the copy (or keeps it archived and works from a zip/tag checkout).

## Active vs archived

| Active | Archived satellites |
|--------|---------------------|
| Eidolon | kmobile, mobile-cli, mobile-mcp, KDesktopVirt |
| PlayCua | — |
