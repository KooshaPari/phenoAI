use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal as _;
use std::path::Path;
use std::process::Command;
use tracing::debug;

use crate::error::KMobileError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub version: String,
    pub arch: String,
    pub available_tools: HashMap<String, ToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub path: String,
    pub version: String,
    pub available: bool,
}

#[allow(dead_code)]

/// Returns `emoji` when stdout is an interactive TTY and neither `NO_COLOR` nor `CI`
/// is set; otherwise returns the plain ASCII `fallback`.  Use this for every
/// user-visible symbol so that pipes, CI logs, and `--color=never` contexts
/// receive clean ASCII output.
///
/// # Examples
/// ```
/// # use kmobile::utils::sym;
/// let s = sym("\u{2705}", "[ok]");
/// assert!(s == "\u{2705}" || s == "[ok]");
/// ```
#[allow(dead_code)]
pub fn sym(emoji: &'static str, fallback: &'static str) -> &'static str {
    let no_color = std::env::var("NO_COLOR").is_ok();
    let in_ci = std::env::var("CI").is_ok();
    let is_tty = std::io::stdout().is_terminal();
    if !no_color && !in_ci && is_tty {
        emoji
    } else {
        fallback
    }
}

pub async fn detect_system_info() -> Result<SystemInfo> {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let version = get_os_version().await?;

    let mut available_tools = HashMap::new();

    // Check for Android tools
    if let Ok(tool_info) = check_tool_availability("adb").await {
        available_tools.insert("adb".to_string(), tool_info);
    }

    // Check for iOS tools
    if let Ok(tool_info) = check_tool_availability("simctl").await {
        available_tools.insert("simctl".to_string(), tool_info);
    }

    if let Ok(tool_info) = check_tool_availability("xcodebuild").await {
        available_tools.insert("xcodebuild".to_string(), tool_info);
    }

    // Check for other tools
    if let Ok(tool_info) = check_tool_availability("instruments").await {
        available_tools.insert("instruments".to_string(), tool_info);
    }

    if let Ok(tool_info) = check_tool_availability("ios-deploy").await {
        available_tools.insert("ios-deploy".to_string(), tool_info);
    }

    if let Ok(tool_info) = check_tool_availability("flutter").await {
        available_tools.insert("flutter".to_string(), tool_info);
    }

    if let Ok(tool_info) = check_tool_availability("react-native").await {
        available_tools.insert("react-native".to_string(), tool_info);
    }

    Ok(SystemInfo {
        os,
        version,
        arch,
        available_tools,
    })
}

#[allow(dead_code)]
async fn get_os_version() -> Result<String> {
    let output = Command::new("sw_vers").args(["-productVersion"]).output();

    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        _ => {
            // Fallback for non-macOS systems
            Ok("unknown".to_string())
        }
    }
}

#[allow(dead_code)]
async fn check_tool_availability(tool_name: &str) -> Result<ToolInfo> {
    // First, try to find the tool using 'which'
    let which_output = Command::new("which").arg(tool_name).output();

    let path = match which_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            // Try some common locations
            let common_paths = match tool_name {
                "adb" => vec![
                    "/usr/local/bin/adb",
                    "/opt/android-sdk/platform-tools/adb",
                    "$ANDROID_HOME/platform-tools/adb",
                ],
                "simctl" => vec![
                    "/usr/bin/simctl",
                    "/Applications/Xcode.app/Contents/Developer/usr/bin/simctl",
                ],
                "xcodebuild" => vec![
                    "/usr/bin/xcodebuild",
                    "/Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild",
                ],
                _ => vec![],
            };

            let mut found_path = None;
            for path in common_paths {
                if Path::new(path).exists() {
                    found_path = Some(path.to_string());
                    break;
                }
            }

            found_path
                .ok_or_else(|| KMobileError::CommandError(format!("Tool {tool_name} not found")))?
        }
    };

    // Try to get version
    let version = get_tool_version(tool_name, &path)
        .await
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(ToolInfo {
        path,
        version,
        available: true,
    })
}

#[allow(dead_code)]
async fn get_tool_version(tool_name: &str, tool_path: &str) -> Result<String> {
    let version_args = match tool_name {
        "adb" => vec!["version"],
        "simctl" => vec!["help"],
        "xcodebuild" => vec!["-version"],
        "instruments" => vec!["-version"],
        "ios-deploy" => vec!["--version"],
        "flutter" => vec!["--version"],
        "react-native" => vec!["--version"],
        _ => vec!["--version"],
    };

    let output = Command::new(tool_path).args(&version_args).output()?;

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Extract version from output
    let version = match tool_name {
        "adb" => {
            if let Some(line) = output_str.lines().next() {
                line.split_whitespace()
                    .last()
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                "unknown".to_string()
            }
        }
        "xcodebuild" => {
            if let Some(line) = output_str.lines().next() {
                line.replace("Xcode ", "").trim().to_string()
            } else {
                "unknown".to_string()
            }
        }
        _ => {
            // Try to extract version from first line
            if let Some(line) = output_str.lines().next() {
                line.split_whitespace()
                    .last()
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                "unknown".to_string()
            }
        }
    };

    Ok(version)
}

#[allow(dead_code)]
pub fn validate_app_path(app_path: &str) -> Result<()> {
    let path = Path::new(app_path);

    if !path.exists() {
        return Err(
            KMobileError::FileSystemError(format!("App file not found: {app_path}")).into(),
        );
    }

    // Check file extension
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        match extension.to_lowercase().as_str() {
            "apk" | "aab" | "app" | "ipa" => Ok(()),
            _ => Err(KMobileError::InvalidInput(format!(
                "Invalid app file extension: {extension}"
            ))
            .into()),
        }
    } else {
        Err(KMobileError::InvalidInput("App file has no extension".to_string()).into())
    }
}

#[allow(dead_code)]
pub fn generate_device_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("kmobile-{timestamp}")
}

#[allow(dead_code)]
pub fn parse_coordinates(coordinate_str: &str) -> Result<(i32, i32)> {
    let parts: Vec<&str> = coordinate_str.split(',').collect();
    if parts.len() != 2 {
        return Err(KMobileError::InvalidInput(
            "Invalid coordinate format. Expected 'x,y'".to_string(),
        )
        .into());
    }

    let x = parts[0]
        .trim()
        .parse::<i32>()
        .map_err(|_| KMobileError::InvalidInput("Invalid x coordinate".to_string()))?;
    let y = parts[1]
        .trim()
        .parse::<i32>()
        .map_err(|_| KMobileError::InvalidInput("Invalid y coordinate".to_string()))?;

    Ok((x, y))
}

#[allow(dead_code)]
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[allow(dead_code)]
pub fn create_backup_file(file_path: &str) -> Result<String> {
    let path = Path::new(file_path);
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");

    let backup_path = if let Some(parent) = path.parent() {
        parent.join(format!(
            "{}.{}.backup",
            path.file_name().unwrap().to_string_lossy(),
            timestamp
        ))
    } else {
        Path::new(&format!("{file_path}.{timestamp}.backup")).to_path_buf()
    };

    fs::copy(file_path, &backup_path)?;
    Ok(backup_path.to_string_lossy().to_string())
}

#[allow(dead_code)]
pub fn cleanup_temp_files(temp_dir: &str) -> Result<()> {
    let path = Path::new(temp_dir);
    if path.exists() && path.is_dir() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn get_available_port() -> Result<u16> {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    Ok(addr.port())
}

#[allow(dead_code)]
pub fn is_port_available(port: u16) -> bool {
    use std::net::{SocketAddr, TcpListener};

    TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).is_ok()
}

#[allow(dead_code)]
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

#[allow(dead_code)]
pub fn truncate_string(s: &str, max_length: usize) -> String {
    if s.len() <= max_length {
        s.to_string()
    } else {
        format!("{}...", &s[..max_length.saturating_sub(3)])
    }
}

#[allow(dead_code)]
pub fn retry_with_backoff<F, R, E>(
    mut operation: F,
    max_retries: usize,
    initial_delay: std::time::Duration,
) -> Result<R>
where
    F: FnMut() -> std::result::Result<R, E>,
    E: std::fmt::Display,
{
    let mut delay = initial_delay;
    let mut last_error = None;

    for attempt in 0..=max_retries {
        match operation() {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);

                if attempt < max_retries {
                    debug!("Attempt {} failed, retrying in {:?}", attempt + 1, delay);
                    std::thread::sleep(delay);
                    delay *= 2; // Exponential backoff
                }
            }
        }
    }

    Err(KMobileError::Unknown(format!(
        "Operation failed after {} attempts: {}",
        max_retries + 1,
        last_error.unwrap()
    ))
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_coordinates() {
        assert_eq!(parse_coordinates("100,200").unwrap(), (100, 200));
        assert_eq!(parse_coordinates("0,0").unwrap(), (0, 0));
        assert!(parse_coordinates("invalid").is_err());
        assert!(parse_coordinates("100").is_err());
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(std::time::Duration::from_secs(30)), "30s");
        assert_eq!(
            format_duration(std::time::Duration::from_secs(90)),
            "1m 30s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_secs(3661)),
            "1h 1m 1s"
        );
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test:file<name>"), "test_file_name_");
        assert_eq!(sanitize_filename("normal_name.txt"), "normal_name.txt");
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("very long string", 10), "very lo...");
    }
}
