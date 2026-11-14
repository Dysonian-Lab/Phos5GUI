//! PM5 hw version parsing and strict version comparison (Plan Section 3.2 & 8.1).
//!
//! Parses version strings from PM5 hw version output and exposes strict
//! comparison against `capabilities/pm5.json` CAPABILITIES_VERSION.
//!
//! Strict policy: device_ver MUST equal client_ver. Mismatch is a fatal
//! safety condition — the GUI enforces flashing bundled matching firmware.
//! No relaxed patches are applied.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::pm5::capabilities::{CLIENT_CAPABILITIES_VERSION, evaluate_capabilities, VersionMatchStatus};

/// Parsed PM5 hw version output.
#[derive(Debug, Clone, Serialize)]
pub struct Pm5HwVersion {
    pub model: String,
    pub client_version: String,
    pub os_version: String,
    pub capabilities_version: u8,
    pub hardware_variant: String,
}

/// Regex to extract the capabilities version from hw version output.
/// Matches patterns like "Capabilities: v13" or "capabilities_version=13".
static CAPABILITIES_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)capabilities[_\s]*version[:\s=]+v?(\d+)").expect("bad capabilities version regex")
});

/// Regex to extract the PM5 model string.
static MODEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(proxmark5|pm5)").expect("bad PM5 model regex")
});

/// Parse PM5 hw version output into structured info.
///
/// Extracts capabilities version from the output. If no capabilities version
/// is found, returns 0 (unknown).
pub fn parse_pm5_hw_version(output: &str) -> Pm5HwVersion {
    let clean = crate::pm3::output_parser::strip_ansi(output);

    let model = if MODEL_RE.is_match(&clean) {
        "PM5".to_string()
    } else {
        "Unknown".to_string()
    };

    let client_version = extract_version_line(&clean, "client");
    let os_version = extract_version_line(&clean, "os");
    let capabilities_version = CAPABILITIES_VERSION_RE
        .captures(&clean)
        .and_then(|c| c[1].parse::<u8>().ok())
        .unwrap_or(0);

    let hardware_variant = detect_hardware_variant(&clean);

    Pm5HwVersion {
        model,
        client_version,
        os_version,
        capabilities_version,
        hardware_variant,
    }
}

/// Extract a version line for a given section prefix (e.g. "client", "os").
fn extract_version_line(output: &str, section: &str) -> String {
    let re = LazyLock::new(|| {
        Regex::new(&format!(r"(?i){section}\s*:\s*(.+)")).expect("bad version line regex")
    });
    re.captures(output)
        .map(|c| c[1].trim().to_string())
        .unwrap_or_default()
}

/// Detect PM5 hardware variant from hw version output.
fn detect_hardware_variant(output: &str) -> String {
    let lower = output.to_lowercase();
    if lower.contains("bwm") {
        "bwm".to_string()
    } else if lower.contains("standard") {
        "standard".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Compare a probed device capabilities version against the client's expected version.
///
/// Returns `VersionMatchStatus::Matched` only when `device_ver == client_ver`.
/// Any mismatch (device older OR newer) is a non-match requiring firmware flash.
pub fn strict_version_compare(device_ver: u8, client_ver: u8) -> VersionMatchStatus {
    evaluate_capabilities(device_ver, client_ver)
}

/// Validate that the device's capabilities version strictly matches the client's
/// expected version from `capabilities/pm5.json`.
///
/// Returns `Ok(())` when versions match. Returns `Err` with a descriptive
/// message when they do not — the caller must trigger a firmware flash.
pub fn enforce_strict_version_match(device_ver: u8) -> Result<(), String> {
    let client_ver = CLIENT_CAPABILITIES_VERSION;
    match strict_version_compare(device_ver, client_ver) {
        VersionMatchStatus::Matched => Ok(()),
        VersionMatchStatus::DeviceOutdated { device_ver, client_ver } => Err(format!(
            "Strict version mismatch: device capabilities v{} is older than client v{}. Flash bundled matching firmware.",
            device_ver, client_ver
        )),
        VersionMatchStatus::ClientOutdated { device_ver, client_ver } => Err(format!(
            "Strict version mismatch: device capabilities v{} is newer than client v{}. Flash bundled matching firmware or update client.",
            device_ver, client_ver
        )),
        VersionMatchStatus::ProbeFailed { reason } => Err(format!(
            "Version probe failed: {}. Cannot proceed without strict version match.",
            reason
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_returns_ok() {
        assert!(enforce_strict_version_match(13).is_ok());
    }

    #[test]
    fn device_outdated_returns_err() {
        let err = enforce_strict_version_match(11).unwrap_err();
        assert!(err.contains("older"));
        assert!(err.contains("v11"));
        assert!(err.contains("v13"));
    }

    #[test]
    fn device_newer_returns_err() {
        let err = enforce_strict_version_match(14).unwrap_err();
        assert!(err.contains("newer"));
        assert!(err.contains("v14"));
        assert!(err.contains("v13"));
    }

    #[test]
    fn zero_version_is_probe_failed() {
        let err = enforce_strict_version_match(0).unwrap_err();
        assert!(err.contains("probe failed"));
    }

    #[test]
    fn parse_capabilities_version_from_output() {
        let output = "Proxmark5 RFID instrument\nCapabilities version: v13\n";
        let info = parse_pm5_hw_version(output);
        assert_eq!(info.capabilities_version, 13);
    }

    #[test]
    fn parse_no_capabilities_version_returns_zero() {
        let output = "Proxmark5 RFID instrument\n";
        let info = parse_pm5_hw_version(output);
        assert_eq!(info.capabilities_version, 0);
    }

    #[test]
    fn detect_bwm_variant() {
        let output = "PM5 BWM variant";
        let info = parse_pm5_hw_version(output);
        assert_eq!(info.hardware_variant, "bwm");
    }
}
