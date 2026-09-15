# Eidolon Release Process

## Versioning Scheme

Eidolon uses **Semantic Versioning (SemVer)**:
- Major: Breaking changes to trait contracts or event types
- Minor: New device types, adapter implementations
- Patch: Bug fixes, internal refactoring

Current version: `0.0.1` (pre-release)

## Publish Targets

All crates target **crates.io**:

| Crate | Status | Target |
|-------|--------|--------|
| eidolon-core | alpha | crates.io |
| eidolon-desktop | alpha | crates.io |
| eidolon-mobile | alpha | crates.io |
| eidolon-sandbox | alpha | crates.io |

## Release Registry

The authoritative registry is maintained in:
- **Location**: `./release-registry.toml` (this directory)
- **Format**: TOML collection manifest with per-crate metadata
- **Schema**: Conforms to `docs/governance/release_registry_schema.md`

## Publish Process

1. **Run full test suite**: `cargo test --workspace`
2. **Verify device adapters compile**: `cargo build --all-features`
3. **Update versions** in `Cargo.toml` and `release-registry.toml`
4. **Update CHANGELOG.md** with device support changes
5. **Create and push tag**: `git tag v<version> && git push origin <tag>`
6. **Publish crates**: `cargo publish --manifest-path crates/<crate>/Cargo.toml`

## Release Registry Location

- **File**: `release-registry.toml` (repository root)
- **Format**: TOML
- **Contents**: Device automation collection metadata and all workspace crates
- **Update**: When adding device adapters or changing publish targets

## Additional Resources

- **Trait Documentation**: See `crates/eidolon-core/src/` for trait contracts
- **Device Adapter Guide**: See `docs/device-adapters.md`
