# Traceability Matrix

> Requirement → Source → Test → Status for KMobile's top 5 features.

| Requirement | Source | Test | Status |
|---|---|---|---|
| CLI argument parsing & command dispatch | `src/cli.rs` | `crates/kmobile-core/tests/testing_port.rs` | ✅ Implemented |
| Device detection & iOS/Android bridge | `src/device_basic.rs`, `src/device_bridge.rs` | `crates/kmobile-core/tests/device_port.rs` | ✅ Implemented |
| Simulator lifecycle & control | `src/simulator_basic.rs` | `crates/kmobile-core/tests/simulator_port.rs` | ✅ Implemented |
| Project scaffolding & build orchestration | `src/project.rs` | `crates/kmobile-core/tests/project_port.rs` | ✅ Implemented |
| MCP server integration | `src/mcp.rs`, `src/mcp_server.rs` | `crates/kmobile-core/tests/mcp_port.rs` | ✅ Implemented |
