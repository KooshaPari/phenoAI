//! IPC layer — JSON-RPC wire types, read/write helpers, and the dispatcher.

pub mod bridge_client;
pub mod bridge_server;
pub mod dispatcher;
pub mod mod_types;

// Re-export wire types at the ipc:: level for convenience.
pub use bridge_client::{BridgeClient, BridgeError, BRIDGE_ENV_LOCK};
pub use bridge_server::handle_request as handle_bridge_request;
pub use mod_types::{read_request, write_response, Request, Response};
