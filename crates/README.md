# playcua-bare

> **Merge status:** Phase 2 scaffolding — L4 #70

This crate is a standalone scaffold for the **bare-cua core types** merge into
PlayCua. It currently contains placeholder re-exports that will be replaced by
the actual bare-cua domain types, port traits, and IPC wire types in a future
Phase 3 migration.

## Merge timeline

- **Phase 1 (completed):** PlayCua absorbed the bare-cua native crate as
  `playcua-native`. Binaries were renamed (`bare-cua-cli` → `playcua-cli`,
  `bare-cua-mcp` → `playcua-mcp`), the Python package was renamed (`bare_cua` →
  `playcua`), and the `bare-cua` repository was frozen as a read-only archive.

- **Phase 2 (this task — L4 #70):** The `playcua-bare` crate is scaffolded as a
  standalone package under `PlayCua/crates/playcua-bare/`. It is not yet a
  member of the root workspace, to keep its build/test loop independent during
  the migration.

- **Phase 3 (future):** The actual bare-cua source files (`domain/`, `ports/`,
  `ipc/`, `plugins/`, `app/`) will be moved into this crate. `playcua-native` will
  then depend on `playcua-bare` for domain types, decoupling the domain model
  from the runtime binary.

## File mapping (Phase 3)

See `merge_plan.md` at the PlayCua workspace root for the complete file mapping
table (`bare-cua/src/* → PlayCua/crates/playcua-bare/src/*`).

## Verification

```bash
cargo check --manifest-path crates/playcua-bare/Cargo.toml
```

The stub compiles and passes its smoke tests (placeholder types are `Send + Sync`,
port traits are object-safe). No real code is present yet — this is a
**documentation + scaffolding** task.

## See also

- [merge_plan.md](../../merge_plan.md) — full merge plan for L4 #70
- [DEPRECATED_BARE_CUA.md](../../DEPRECATED_BARE_CUA.md) — Phase 1 deprecation record
