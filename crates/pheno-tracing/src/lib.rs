//! Canonical tracing subscriber for PlayCua (and other pheno-* binaries).
//!
//! Two surfaces:
//!
//! - [`init`] — fire-and-forget global subscriber install (JSON-to-stderr,
//!   `RUST_LOG` env filter, default level `info`). Idempotent.
//! - [`builder`] — fine-grained builder that lets callers override the
//!   `MakeWriter`, attach per-target level overrides, and produce a
//!   subscriber for tests / non-global use cases.
//!
//! Every binary in the fleet calls `pheno_tracing::init()` exactly once
//! at the top of `main` so JSON log lines on stderr are uniform across
//! services and machine-parseable by the orchestrator.

use std::io;
use std::sync::OnceLock;

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::FmtSubscriber;

/// Install the canonical fleet subscriber.
///
/// - Format: JSON (`tracing-subscriber::fmt::format::Json`).
/// - Writer: stderr.
/// - Filter: `RUST_LOG` env var, defaulting to `info` if unset.
///
/// Safe to call multiple times: only the first call takes effect
/// (subsequent calls are no-ops). Returns silently if a subscriber has
/// already been installed.
pub fn init() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        // We deliberately ignore the `Result` from `try_init` —
        // a second `init()` is the documented no-op path, but so
        // is "another subscriber already installed globally". In
        // both cases we want to proceed without panicking.
        let _ = install_global(io::stderr as fn() -> io::Stderr, &[]);
    });
}

/// Build a configurable subscriber builder.
///
/// ```text
/// pheno_tracing::builder()
///     .with_default_directive("playcua_native", "trace")
///     .finish_json_with_writer(buf);
/// ```
///
/// The builder is **not** installed globally — call `.install()` to do
/// that, or feed the finished subscriber into `tracing::subscriber::with_default`.
pub fn builder() -> SubscriberBuilder {
    SubscriberBuilder::default()
}

/// A configurable tracing-subscriber builder.
///
/// Holds zero or more `(target, level)` overrides plus the env-derived
/// base filter. `.finish_json_with_writer` materialises the subscriber
/// (without installing it globally — see [`SubscriberBuilder::install`]).
#[derive(Debug, Clone, Default)]
pub struct SubscriberBuilder {
    /// Per-target `target=level` directive overrides applied on top of
    /// the env-derived filter.
    directives: Vec<(String, String)>,
}

impl SubscriberBuilder {
    /// Add a `target=level` override that takes precedence over the
    /// env-derived filter. Useful for pinning a single module to
    /// `trace` while everything else stays at `info`.
    pub fn with_default_directive(
        mut self,
        target: impl Into<String>,
        level: impl Into<String>,
    ) -> Self {
        self.directives.push((target.into(), level.into()));
        self
    }

    /// Materialise a JSON-formatted subscriber writing to `writer`.
    ///
    /// `W` is any `MakeWriter<'a>` — typically a fn pointer returning
    /// `io::Stderr` for production or a `Vec<u8>`-backed writer for tests.
    pub fn finish_json_with_writer<W>(
        &self,
        writer: W,
    ) -> FmtSubscriber<
        tracing_subscriber::fmt::format::JsonFields,
        tracing_subscriber::fmt::format::Format<tracing_subscriber::fmt::format::Json>,
        EnvFilter,
        W,
    >
    where
        W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
    {
        let filter = build_filter(&self.directives);
        FmtSubscriber::builder()
            .json()
            .with_current_span(false)
            .with_span_list(false)
            .with_env_filter(filter)
            .with_writer(writer)
            .finish()
    }

    /// Install the subscriber globally with stderr as the writer
    /// (sets it as the *default* subscriber for the current thread and
    /// returns `Ok(())` if no other subscriber was already installed).
    /// Equivalent to `tracing::subscriber::set_global_default`.
    pub fn install(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        install_global(io::stderr as fn() -> io::Stderr, &self.directives)
    }
}

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

fn build_filter(directives: &[(String, String)]) -> EnvFilter {
    // Start from `RUST_LOG` if set, else `info`. Then layer any
    // per-target overrides on top — `from_str` returns Err on a malformed
    // directive, but malformed directives here would mean a bug in
    // the call site, so we panic to surface it during dev.
    let mut filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    for (target, level) in directives {
        let directive = format!("{target}={level}");
        filter = filter.add_directive(
            tracing_subscriber::filter::Directive::from_str(&directive)
                .expect("valid tracing directive"),
        );
    }
    filter
}

fn install_global<W>(
    writer: W,
    directives: &[(String, String)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let filter = build_filter(directives);
    tracing_subscriber::fmt()
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .with_env_filter(filter)
        .with_writer(writer)
        .try_init()
}

// `FromStr` isn't in `tracing_subscriber` preludes; re-import here so
// `build_filter` can stay tidy.
use std::str::FromStr;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// Serialize tests that touch `RUST_LOG` / shared subscriber env.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Shared byte buffer + `MakeWriter` impl for tests.
    #[derive(Clone, Default)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for SharedBuf {
        type Writer = SharedBuf;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn builder_emits_json_with_custom_writer() {
        let _env = ENV_LOCK.lock().expect("env lock");
        // Ensure a sibling test's RUST_LOG=error cannot suppress this line.
        std::env::remove_var("RUST_LOG");
        let buf = SharedBuf::default();
        let subscriber = builder().finish_json_with_writer(buf.clone());

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(marker = "test", "hello from pheno-tracing");
        });

        let captured = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(
            captured.contains("\"hello from pheno-tracing\""),
            "expected JSON-escaped message, got: {captured}"
        );
    }

    #[test]
    fn default_directive_overrides_env_filter() {
        let _env = ENV_LOCK.lock().expect("env lock");
        let buf = SharedBuf::default();
        // `error` is the env-level floor; we override `foo::bar` to `info`.
        std::env::set_var("RUST_LOG", "error");
        let subscriber = builder()
            .with_default_directive("foo::bar", "info")
            .finish_json_with_writer(buf.clone());
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "foo::bar", "yes-info");
        });
        std::env::remove_var("RUST_LOG");

        let captured = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(
            captured.contains("yes-info"),
            "expected directive override to let info through, got: {captured}"
        );
    }
}
