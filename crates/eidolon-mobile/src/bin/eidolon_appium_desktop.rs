//! Launch Appium Inspector (when installed) + local Eidolon Appium dashboard.
//!
//! ```text
//! cargo run -p eidolon-mobile --features mobile-appium-desktop \
//!   --bin eidolon-appium-desktop --locked
//! ```

use eidolon_mobile::android::appium_desktop;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    match appium_desktop::run() {
        Ok((bind, inspector)) => {
            eprintln!("eidolon-appium-desktop: dashboard http://{bind}");
            match inspector {
                Some(p) => eprintln!("eidolon-appium-desktop: launched inspector {}", p.display()),
                None => eprintln!(
                    "eidolon-appium-desktop: Inspector not installed — dashboard only \
                     (set EIDOLON_APPIUM_INSPECTOR or install Appium Inspector)"
                ),
            }
            // Keep process alive so the dashboard thread continues.
            loop {
                thread::sleep(Duration::from_secs(3600));
            }
        }
        Err(e) => {
            eprintln!("eidolon-appium-desktop: {e}");
            ExitCode::FAILURE
        }
    }
}
