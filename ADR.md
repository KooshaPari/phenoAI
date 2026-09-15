# Eidolon — Architecture Decision Records

This index lists the Architecture Decision Records (ADRs) for the Eidolon
project. ADRs document the architectural decisions that shape Eidolon's
trait surface, platform-impl matrix, and consumer-facing API.

## Conventions

ADRs follow the MADR (Markdown Any Decision Record) format established by
`docs/adr/0001-record-architecture-decisions.md`. Each ADR is a numbered
Markdown file under `docs/`. Decisions are immutable once accepted; supersession
is recorded via the `Supersedes` / `Superseded by` fields and an explicit
cross-reference in the body.

## Index

| #     | Title                                                                        | Status   | Date       | File                                                                                            |
|-------|------------------------------------------------------------------------------|----------|------------|-------------------------------------------------------------------------------------------------|
| 0001  | Record Architecture Decisions (MADR template)                                | Accepted | 2026-04-24 | [`docs/adr/0001-record-architecture-decisions.md`](docs/adr/0001-record-architecture-decisions.md) |
| ADR-001 | Trait-Based Core vs. Direct Code Merge                                     | Accepted | 2026-04-24 | [`docs/ADR-001-trait-based-core.md`](docs/ADR-001-trait-based-core.md)                          |
| ADR-002 | VirtualStage Unification of DesktopAutomator / MobileAutomator / SandboxAutomator | Accepted | 2026-06-10 | [`docs/adr/ADR-002-virtual-stage-unification.md`](docs/adr/ADR-002-virtual-stage-unification.md) |

## Active Decisions

- **ADR-001** establishes the trait-based core (vs. direct code merge from
  KDesktopVirt, kmobile, KVirtualStage, PlayCua, bare-cua). It defines the
  three sibling traits `DesktopAutomator`, `MobileAutomator`, and
  `SandboxAutomator`.
- **ADR-002** refines the trait shape by introducing `VirtualStage` as the
  unified consumer-side surface. The three sibling traits from ADR-001 are
  preserved as backward-compat blanket-impl super-traits, with `MobileStage`
  and `SandboxStage` as type-narrowing sub-traits for platform-specific
  behaviour (XCTest, UiAutomator, Docker, etc.).

## Adding a New ADR

1. Pick the next number in the sequence. The legacy `docs/ADR-NNN-*.md` and
   newer `docs/adr/NNNN-*.md` numbering schemes coexist; prefer the newer
   four-digit scheme for new entries.
2. Create the file with the MADR header (Status, Date, Context, Decision,
   Rationale, Consequences, Alternatives, Reference).
3. Add a row to the index table above.
4. If the new ADR supersedes an existing one, add a `Supersedes` field to
   the new ADR and a `Superseded by` field to the old one, and update both
   bodies with explicit cross-references.
5. Commit on a `docs/eidolon-adr-<slug>-<YYYYMMDD>` branch. Do not push
   without review.
