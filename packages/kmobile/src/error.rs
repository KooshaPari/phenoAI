use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum KMobileError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device connection error: {0}")]
    DeviceConnectionError(String),

    #[error("Simulator not found: {0}")]
    SimulatorNotFound(String),

    #[error("Simulator start error: {0}")]
    SimulatorStartError(String),

    #[error("Simulator stop error: {0}")]
    SimulatorStopError(String),

    #[error("Simulator reset error: {0}")]
    SimulatorResetError(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Project initialization error: {0}")]
    ProjectInitError(String),

    #[error("Project deploy error: {0}")]
    ProjectDeployError(String),

    #[error("Build error: {0}")]
    BuildError(String),

    #[error("Test execution error: {0}")]
    TestExecutionError(String),

    #[error("Test file not found: {0}")]
    TestFileNotFound(String),

    #[error("App installation error: {0}")]
    AppInstallError(String),

    #[error("Command execution error: {0}")]
    CommandError(String),

    #[error("MCP server error: {0}")]
    McpServerError(String),

    #[error("API server error: {0}")]
    ApiServerError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("File system error: {0}")]
    FileSystemError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Permission error: {0}")]
    PermissionError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl KMobileError {
    #[allow(dead_code)]
    pub fn is_recoverable(&self) -> bool {
        match self {
            KMobileError::ConfigError(_) => false,
            KMobileError::DeviceNotFound(_) => true,
            KMobileError::DeviceConnectionError(_) => true,
            KMobileError::SimulatorNotFound(_) => true,
            KMobileError::SimulatorStartError(_) => true,
            KMobileError::SimulatorStopError(_) => true,
            KMobileError::SimulatorResetError(_) => true,
            KMobileError::ProjectNotFound(_) => false,
            KMobileError::ProjectInitError(_) => false,
            KMobileError::ProjectDeployError(_) => true,
            KMobileError::BuildError(_) => true,
            KMobileError::TestExecutionError(_) => true,
            KMobileError::TestFileNotFound(_) => false,
            KMobileError::AppInstallError(_) => true,
            KMobileError::CommandError(_) => true,
            KMobileError::McpServerError(_) => true,
            KMobileError::ApiServerError(_) => true,
            KMobileError::NetworkError(_) => true,
            KMobileError::FileSystemError(_) => true,
            KMobileError::SerializationError(_) => true,
            KMobileError::AuthenticationError(_) => false,
            KMobileError::PermissionError(_) => false,
            KMobileError::TimeoutError(_) => true,
            KMobileError::InvalidInput(_) => false,
            KMobileError::Unknown(_) => false,
        }
    }

    #[allow(dead_code)]
    pub fn error_code(&self) -> &'static str {
        match self {
            KMobileError::ConfigError(_) => "CONFIG_ERROR",
            KMobileError::DeviceNotFound(_) => "DEVICE_NOT_FOUND",
            KMobileError::DeviceConnectionError(_) => "DEVICE_CONNECTION_ERROR",
            KMobileError::SimulatorNotFound(_) => "SIMULATOR_NOT_FOUND",
            KMobileError::SimulatorStartError(_) => "SIMULATOR_START_ERROR",
            KMobileError::SimulatorStopError(_) => "SIMULATOR_STOP_ERROR",
            KMobileError::SimulatorResetError(_) => "SIMULATOR_RESET_ERROR",
            KMobileError::ProjectNotFound(_) => "PROJECT_NOT_FOUND",
            KMobileError::ProjectInitError(_) => "PROJECT_INIT_ERROR",
            KMobileError::ProjectDeployError(_) => "PROJECT_DEPLOY_ERROR",
            KMobileError::BuildError(_) => "BUILD_ERROR",
            KMobileError::TestExecutionError(_) => "TEST_EXECUTION_ERROR",
            KMobileError::TestFileNotFound(_) => "TEST_FILE_NOT_FOUND",
            KMobileError::AppInstallError(_) => "APP_INSTALL_ERROR",
            KMobileError::CommandError(_) => "COMMAND_ERROR",
            KMobileError::McpServerError(_) => "MCP_SERVER_ERROR",
            KMobileError::ApiServerError(_) => "API_SERVER_ERROR",
            KMobileError::NetworkError(_) => "NETWORK_ERROR",
            KMobileError::FileSystemError(_) => "FILE_SYSTEM_ERROR",
            KMobileError::SerializationError(_) => "SERIALIZATION_ERROR",
            KMobileError::AuthenticationError(_) => "AUTHENTICATION_ERROR",
            KMobileError::PermissionError(_) => "PERMISSION_ERROR",
            KMobileError::TimeoutError(_) => "TIMEOUT_ERROR",
            KMobileError::InvalidInput(_) => "INVALID_INPUT",
            KMobileError::Unknown(_) => "UNKNOWN_ERROR",
        }
    }
}

pub type Result<T> = std::result::Result<T, KMobileError>;

// Convert from common error types
impl From<std::io::Error> for KMobileError {
    fn from(error: std::io::Error) -> Self {
        KMobileError::FileSystemError(error.to_string())
    }
}

impl From<serde_json::Error> for KMobileError {
    fn from(error: serde_json::Error) -> Self {
        KMobileError::SerializationError(error.to_string())
    }
}

impl From<toml::de::Error> for KMobileError {
    fn from(error: toml::de::Error) -> Self {
        KMobileError::SerializationError(error.to_string())
    }
}

impl From<toml::ser::Error> for KMobileError {
    fn from(error: toml::ser::Error) -> Self {
        KMobileError::SerializationError(error.to_string())
    }
}

impl From<reqwest::Error> for KMobileError {
    fn from(error: reqwest::Error) -> Self {
        KMobileError::NetworkError(error.to_string())
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for KMobileError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        KMobileError::Unknown(error.to_string())
    }
}

// Error context helpers
pub trait ErrorContext<T> {
    #[allow(dead_code)]
    fn with_context(self, context: &str) -> Result<T>;
}

impl<T, E> ErrorContext<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn with_context(self, context: &str) -> Result<T> {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(KMobileError::Unknown(format!("{context}: {error}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_recoverable() {
        assert!(!KMobileError::ConfigError("bad".into()).is_recoverable());
        assert!(KMobileError::DeviceNotFound("x".into()).is_recoverable());
        assert!(KMobileError::NetworkError("down".into()).is_recoverable());
        assert!(!KMobileError::InvalidInput("bad".into()).is_recoverable());
        assert!(KMobileError::TimeoutError("slow".into()).is_recoverable());
        assert!(!KMobileError::Unknown("oops".into()).is_recoverable());
    }
}
