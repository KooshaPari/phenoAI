use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeHandle {
    pub id: String,
}

impl BridgeHandle {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
