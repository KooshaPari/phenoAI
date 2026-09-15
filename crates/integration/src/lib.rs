use pheno_config::Config;
use pheno_errors::AppError;

/// The canonical PlayCua application harness.
///
/// Wires together the three pheno-* foundation crates:
/// - `pheno-config` for typed configuration loading
/// - `pheno-tracing` for structured logging initialization
/// - `pheno-errors` for canonical error handling
#[derive(Debug, Clone)]
pub struct PlayCuaApp {
    /// The loaded runtime configuration.
    pub config: Config,
}

impl PlayCuaApp {
    /// Creates a new `PlayCuaApp` by loading configuration from environment
    /// variables with the `PLAYCUA` prefix and initializing the global
    /// tracing subscriber.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Domain` when config loading fails, or
    /// `AppError::Storage` for I/O-related failures.
    pub fn new() -> Result<Self, AppError> {
        let config =
            pheno_config::load_from_env("PLAYCUA").map_err(|e| AppError::domain(e.to_string()))?;

        pheno_tracing::init();

        tracing::info!(
            url = %config.url,
            port = config.port,
            log_level = %config.log_level,
            db_path = %config.db_path,
            "PlayCuaApp initialized"
        );

        Ok(Self { config })
    }

    /// Creates a `PlayCuaApp` from an explicitly provided config.
    ///
    /// Tracing is still initialized via `pheno-tracing::init()`.
    pub fn with_config(config: Config) -> Self {
        pheno_tracing::init();

        tracing::info!(
            url = %config.url,
            port = config.port,
            log_level = %config.log_level,
            db_path = %config.db_path,
            "PlayCuaApp initialized with explicit config"
        );

        Self { config }
    }

    /// Placeholder run method — a no-op that returns `Ok(())`.
    ///
    /// Future iterations will wire the actual PlayCua event loop here.
    pub fn run(&self) -> Result<(), AppError> {
        tracing::info!("PlayCuaApp::run() called (no-op placeholder)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pheno_config::ConfigBuilder;

    /// Helper: build a canonical Config suitable for unit tests
    /// without touching the environment.
    fn sample_config() -> Config {
        ConfigBuilder::new()
            .url("http://localhost:9090")
            .db_path("/var/lib/playcua.db")
            .port(9090)
            .log_level("debug")
            .feature_flag("alpha")
            .build()
            .expect("config should build")
    }

    #[test]
    fn app_loads_config() {
        // Set env vars so pheno-config can materialise a Config.
        std::env::set_var("PLAYCUA_URL", "http://localhost:8080");
        std::env::set_var("PLAYCUA_PORT", "8080");
        std::env::set_var("PLAYCUA_LOG_LEVEL", "info");
        std::env::set_var("PLAYCUA_DB_PATH", "/tmp/playcua.db");
        std::env::set_var("PLAYCUA_FEATURE_FLAGS", "beta,gamma");

        let app = PlayCuaApp::new().expect("app should load config from env");
        assert_eq!(app.config.url, "http://localhost:8080");
        assert_eq!(app.config.port, 8080);
        assert_eq!(app.config.log_level, "info");
        assert_eq!(app.config.db_path, "/tmp/playcua.db");
        assert_eq!(
            app.config.feature_flags,
            vec!["beta".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn app_initializes_tracing() {
        // Tracing is global and idempotent — with_config must not panic
        // and the resulting app must round-trip the config it was given.
        let config = sample_config();

        let app = PlayCuaApp::with_config(config.clone());
        assert_eq!(app.config, config);
    }

    #[test]
    fn app_run_returns_ok() {
        let config = sample_config();
        let app = PlayCuaApp::with_config(config);
        assert!(app.run().is_ok());
    }
}
