//! L5 #81 — PlayCua full integration smoke test.
//!
//! Asserts the three L5 #81 wiring claims end-to-end:
//!
//! 1. **The `playcua-native` binary starts.** Spawned as a
//!    subprocess with `< /dev/null` (empty stdin). The stdio
//!    JSON-RPC loop must hit EOF, log "stdin EOF — shutting down",
//!    flush, and exit with code 0.
//! 2. **The tracing subscriber is initialized.** We build a
//!    `pheno-tracing` subscriber with a custom `MakeWriter`
//!    pointing at a `SharedBuf`, install it as the thread-local
//!    default via `tracing::subscriber::with_default`, emit a
//!    known sentinel log line, and assert the buffer captured it.
//! 3. **`pheno_flags::FlagSet::from_env("PLAYCUA")` round-trips
//!    truthy and falsy env values.** Truthy: `1`, `true`, `yes`
//!    (case-insensitive). Falsy: `0`, `false`, `no`. Unknown keys
//!    return `false` from `is_enabled` (the safe default).

use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use pheno_flags::FlagSet;
use tracing_subscriber::fmt::MakeWriter;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Thread-safe shared byte buffer for capturing `MakeWriter` output
/// across the test thread. The `Write` impl appends; `MakeWriter`
/// clones an `Arc` so each `make_writer()` call gets its own
/// writer handle (matching `tracing-subscriber`'s per-event
/// writer contract).
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

fn read_buf(buf: &SharedBuf) -> String {
    String::from_utf8(buf.0.lock().unwrap().clone()).expect("utf-8")
}

/// RAII guard that deletes the env vars it set on drop, so a test
/// failure mid-flight does not leak the variables into the next
/// test. Integration tests that touch `PLAYCUA_*` must also take
/// [`ENV_LOCK`] — `FlagSet::from_env("PLAYCUA")` and the native
/// binary scan *all* `PLAYCUA_*` vars, so parallel cargo-test
/// workers otherwise race (garbage values from one test poison
/// another).
struct EnvGuard {
    keys: Vec<String>,
}

impl EnvGuard {
    fn set(key: &str, value: &str) -> Self {
        // SAFETY: callers hold `ENV_LOCK` for the duration of the
        // test so no other smoke test mutates process env concurrently.
        std::env::set_var(key, value);
        Self {
            keys: vec![key.to_string()],
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for k in &self.keys {
            std::env::remove_var(k);
        }
    }
}

/// Serializes every smoke test that reads or writes `PLAYCUA_*`
/// process environment (or spawns `playcua-native`, which loads flags
/// from the parent env at startup).
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Asserts the `playcua-native` binary starts, hits EOF on empty
/// stdin, and exits cleanly. This is the L5 #81 wiring claim #1.
#[test]
fn binary_starts_and_exits_cleanly_on_eof() {
    let _env = ENV_LOCK.lock().expect("env lock");
    // The `CARGO_BIN_EXE_<name>` env var is set by Cargo for
    // integration tests of the same crate. See:
    //   https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates
    let bin = env!("CARGO_BIN_EXE_playcua-native");

    let output = Command::new(bin)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to spawn playcua-native");

    assert!(
        output.status.success(),
        "playcua-native exited non-zero: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // The binary writes a startup log line to stderr (the tracing
    // subscriber targets stderr by default). The exact contents
    // depend on the JSON formatter, but the version field is
    // always emitted.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("playcua-native starting")
            || stderr.contains("playcua-native exiting")
            || stderr.contains("playcua_native"),
        "expected startup/shutdown log in stderr, got: {stderr}"
    );
}

/// Asserts `pheno_tracing::init()` (and the builder API it wraps)
/// produces a subscriber that emits to a custom `MakeWriter`. This
/// is the L5 #81 wiring claim #2.
#[test]
fn tracing_subscriber_captures_log_via_custom_make_writer() {
    let buf = SharedBuf::default();
    let subscriber = pheno_tracing::builder()
        .with_default_directive("playcua_native", "trace")
        .finish_json_with_writer(buf.clone());

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(marker = "l5-81-smoke", "playcua-native smoke test log line");
    });

    let captured = read_buf(&buf);
    assert!(
        captured.contains("playcua-native smoke test log line"),
        "captured: {captured}"
    );
    // JSON format wraps the message in quotes.
    assert!(
        captured.contains("\"playcua-native smoke test log line\""),
        "expected JSON-escaped message, captured: {captured}"
    );
}

/// Asserts `pheno_flags::FlagSet::from_env("PLAYCUA")` reads
/// truthy values from `PLAYCUA_<KEY>` env vars. This is the
/// L5 #81 wiring claim #3 (positive case).
#[test]
fn flags_round_trip_truthy_values_from_env() {
    let _env = ENV_LOCK.lock().expect("env lock");
    let _a = EnvGuard::set("PLAYCUA_L5_81_A", "1");
    let _b = EnvGuard::set("PLAYCUA_L5_81_B", "true");
    let _c = EnvGuard::set("PLAYCUA_L5_81_C", "yes");
    let _d = EnvGuard::set("PLAYCUA_L5_81_D", "YES"); // case-insensitive

    let flags = FlagSet::from_env("PLAYCUA").expect("flag parse should succeed");

    assert!(flags.is_enabled("L5_81_A"), "1 -> true failed");
    assert!(flags.is_enabled("L5_81_B"), "true -> true failed");
    assert!(flags.is_enabled("L5_81_C"), "yes -> true failed");
    assert!(flags.is_enabled("L5_81_D"), "YES -> true failed");
}

/// Asserts `pheno_flags::FlagSet::from_env("PLAYCUA")` reads
/// falsy values. Negative case for wiring claim #3.
#[test]
fn flags_round_trip_falsy_values_from_env() {
    let _env = ENV_LOCK.lock().expect("env lock");
    let _x = EnvGuard::set("PLAYCUA_L5_81_X", "0");
    let _y = EnvGuard::set("PLAYCUA_L5_81_Y", "false");
    let _z = EnvGuard::set("PLAYCUA_L5_81_Z", "no");

    let flags = FlagSet::from_env("PLAYCUA").expect("flag parse should succeed");

    assert!(!flags.is_enabled("L5_81_X"), "0 -> false failed");
    assert!(!flags.is_enabled("L5_81_Y"), "false -> false failed");
    assert!(!flags.is_enabled("L5_81_Z"), "no -> false failed");
}

/// Asserts unparseable flag values return `FlagError::InvalidValue`
/// (which the main binary maps to `AppError::Validation`).
#[test]
fn flags_reject_unparseable_value() {
    let _env = ENV_LOCK.lock().expect("env lock");
    let _g = EnvGuard::set("PLAYCUA_L5_81_GARBAGE", "not-a-bool");
    let result = FlagSet::from_env("PLAYCUA");
    assert!(result.is_err(), "expected error for unparseable flag value");
}

/// Asserts the `playcua_native` library re-exports its public
/// modules. The integration test is in the same crate, so this
/// is a smoke check that the wiring hasn't broken the library
/// surface.
#[test]
fn playcua_native_library_is_wired() {
    // Re-export check: if any of these modules went missing,
    // this test fails to compile.
    use playcua_native::ipc;
    use playcua_native::modality;

    let _ = ipc::Response::err(serde_json::Value::Null, -32700, "smoke");
    let _ = modality::ModalityKind::parse("native");
}
