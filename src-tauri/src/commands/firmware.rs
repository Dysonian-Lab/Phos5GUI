use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::ShellExt;

use crate::error::AppError;
use crate::flasher;
use crate::pm3::connection;
use crate::pm3::version::parse_detailed_hw_version;
use crate::pm5::capabilities::{
    evaluate_capabilities, VersionMatchStatus, CLIENT_CAPABILITIES_VERSION,
};
use crate::probe::ng_protocol::ProbeMethod;

// ---------------------------------------------------------------------------
// State — holds the running flash child process (if any) for cancellation
// ---------------------------------------------------------------------------

/// Managed state for the flash subprocess. Stored via `app.manage()`.
pub struct FlashState {
    pub child: Mutex<Option<tauri_plugin_shell::process::CommandChild>>,
}

impl FlashState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }
}

// ---------------------------------------------------------------------------
// DTOs — serialized to frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareCheckResult {
    pub matched: bool,
    pub client_version: String,
    pub device_firmware_version: String,
    pub hardware_variant: String,
    pub firmware_path_exists: bool,
    pub probe_method: ProbeMethod,
    pub capabilities_version: u8,
    pub max_cmd_data_size: u16,
    pub is_pm5: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareProgress {
    pub phase: String,
    pub percent: u8,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const VALID_VARIANTS: &[&str] = &["rdv4", "rdv4-bt", "generic", "generic-256"];

static PORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(COM[1-9]\d*|/dev/tty(ACM|USB)\d{1,2}|/dev/tty\.usbmodem\w+)$")
        .expect("bad port regex")
});

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Probe device capabilities and evaluate version match against the bundled client.
#[tauri::command]
pub async fn evaluate_version_match(
    app: AppHandle,
    port: String,
) -> Result<VersionMatchStatus, String> {
    let probe_result = connection::probe_device_with_fallback(&app, &port, 9600)
        .await
        .map_err(|e| e.to_string())?;

    // When Layer 2 `hw version` succeeded (exit code 0) but capabilities_version
    // couldn't be extracted from stdout (direct serial probe timed out), we must
    // still check for commit mismatch by parsing the hw version output directly.
    // Returning `Matched` here would suppress the DeviceOutdated modal and the
    // firmware update flow even when the device firmware is outdated.
    if probe_result.capabilities_version == 0
        && probe_result.method == ProbeMethod::StderrScraping
    {
        let hw_output = match connection::run_command(&app, &port, "hw version").await {
            Ok(output) => output,
            Err(e) => {
                return Ok(VersionMatchStatus::ProbeFailed {
                    reason: format!("hw version check failed: {}", e),
                });
            }
        };

        // Parse using the PM3 hw version parser to compare client vs device commits.
        let hw_info = crate::pm3::version::parse_detailed_hw_version(&hw_output);
        if hw_info.versions_match {
            return Ok(VersionMatchStatus::Matched);
        }

        // Commits mismatch — device is outdated (State A per Plan Section 4.3).
        return Ok(VersionMatchStatus::DeviceOutdated {
            device_ver: probe_result.capabilities_version,
            client_ver: CLIENT_CAPABILITIES_VERSION,
        });
    }

    Ok(evaluate_capabilities(
        probe_result.capabilities_version,
        CLIENT_CAPABILITIES_VERSION,
    ))
}

/// Flash PM5 firmware (bootrom + fullimage) for the requested variant via USB-CDC flasher.
#[tauri::command]
pub async fn flash_pm5_firmware(
    app: AppHandle,
    port: String,
    variant: String,
) -> Result<(), String> {
    if !PORT_RE.is_match(&port) {
        return Err(format!("Invalid port: {}", port));
    }

    let variant = variant.to_lowercase();
    if variant != "standard" && variant != "bwm" {
        return Err(format!(
            "Invalid PM5 variant '{}'. Expected 'standard' or 'bwm'.",
            variant
        ));
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Failed to resolve resource dir: {}", e))?;

    let candidates = [
        resource_dir.join("firmware").join("pm5").join(&variant),
        resource_dir.join("firmware").join(&variant),
    ];

    let (bootrom_path, fullimage_path) = candidates
        .iter()
        .find_map(|base| {
            let bootrom = base.join("bootrom.elf");
            let fullimage = base.join("fullimage.elf");
            if bootrom.exists() && fullimage.exists() {
                Some((bootrom, fullimage))
            } else {
                None
            }
        })
        .ok_or_else(|| {
            format!(
                "Bundled PM5 firmware not found for variant '{}'. Expected bootrom.elf and fullimage.elf.",
                variant
            )
        })?;

    let bootrom_path: PathBuf = strip_win_prefix(bootrom_path);
    let fullimage_path: PathBuf = strip_win_prefix(fullimage_path);

    flasher::bootloader::flash_via_client_flasher(&port, &bootrom_path, &fullimage_path)
        .await
        .map_err(|e| e.to_string())
}

fn strip_win_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

/// Check firmware version match between bundled client and device OS.
/// Uses Layer 1 direct serial probe (NG frames) with Layer 2 stderr scraping fallback.
/// This runs BEFORE any proxmark3.exe spawn to get capabilities even on mismatch.
#[tauri::command]
pub async fn check_firmware_version(
    app: AppHandle,
    port: String,
) -> Result<FirmwareCheckResult, AppError> {
    // Probe device capabilities using layered approach (Layer 1 + Layer 2 fallback)
    let probe_result = connection::probe_device_with_fallback(&app, &port, 9600).await?;

    // Now try to get detailed hw version info via the client
    let hw_info = match connection::run_command(&app, &port, "hw version").await {
        Ok(output) => {
            let info = parse_detailed_hw_version(&output);
            Some(info)
        }
        Err(e) => {
            // If hw version fails, we still have probe data
            let err_msg = e.to_string();
            if err_msg.to_lowercase().contains("capabilities") {
                None // Mismatch but we have probe data
            } else {
                return Err(e);
            }
        }
    };

    let (matched, client_version, device_firmware_version, hardware_variant) = if let Some(info) = hw_info {
        (info.versions_match, info.client_version, info.os_version, info.hardware_variant)
    } else {
        // No hw version output, use probe data
        (false, "bundled".to_string(), format!("v{}", probe_result.capabilities_version), "pm5".to_string())
    };

    let fw_exists = firmware_file_exists(&app, &hardware_variant);

    Ok(FirmwareCheckResult {
        matched,
        client_version,
        device_firmware_version,
        hardware_variant,
        firmware_path_exists: fw_exists,
        probe_method: probe_result.method,
        capabilities_version: probe_result.capabilities_version,
        max_cmd_data_size: probe_result.max_cmd_data_size,
        is_pm5: probe_result.is_pm5,
    })
}

/// Start flashing firmware to the connected PM3 device.
///
/// Spawns the sidecar binary in flash mode and streams progress to the
/// frontend via Tauri events:
/// - `firmware-progress` — phase/percent updates during flash
/// - `firmware-complete` — flash finished successfully
/// - `firmware-failed` — flash failed with error
///
/// Returns immediately after spawning. Use `cancel_flash` to abort.
#[tauri::command]
pub async fn flash_firmware(
    app: AppHandle,
    port: String,
    hardware_variant: String,
    flash_state: State<'_, FlashState>,
) -> Result<(), AppError> {
    // Reject if a flash is already running
    {
        let lock = flash_state.child.lock().map_err(|e| {
            AppError::CommandFailed(format!("Flash state lock poisoned: {}", e))
        })?;
        if lock.is_some() {
            return Err(AppError::CommandFailed(
                "A firmware flash is already in progress".into(),
            ));
        }
    }

    // Validate port
    if !PORT_RE.is_match(&port) {
        return Err(AppError::CommandFailed(format!("Invalid port: {}", port)));
    }

    // Validate hardware variant (prevent path traversal)
    if !VALID_VARIANTS.contains(&hardware_variant.as_str()) {
        return Err(AppError::CommandFailed(format!(
            "Invalid hardware variant: {}",
            hardware_variant
        )));
    }

    // Resolve firmware path from bundled resources
    let resource_dir = app.path().resource_dir().map_err(|e| {
        AppError::CommandFailed(format!("Failed to resolve resource dir: {}", e))
    })?;
    let fw_path = resource_dir
        .join("firmware")
        .join(&hardware_variant)
        .join("fullimage.elf");

    if !fw_path.exists() {
        return Err(AppError::CommandFailed(format!(
            "Firmware file not found: {}",
            fw_path.display()
        )));
    }

    // Strip Windows extended-length path prefix (\\?\) — PM3 can't parse it.
    // Tauri's resource_dir() returns canonicalized paths with this prefix.
    let fw_path_str = fw_path
        .to_string_lossy()
        .strip_prefix(r"\\?\")
        .unwrap_or(&fw_path.to_string_lossy())
        .to_string();

    // Emit initial progress
    let _ = app.emit(
        "firmware-progress",
        FirmwareProgress {
            phase: "connecting".into(),
            percent: 5,
            message: "Connecting to device...".into(),
        },
    );

    let flash_args = [
        port.as_str(),
        "--flash",
        "--image",
        fw_path_str.as_str(),
        "-w",
    ];

    // Try sidecar first (works in dev mode). In NSIS installs the sidecar
    // binary lives in the root install dir, NOT in binaries/, so the sidecar
    // lookup fails with os error 3. Fall back to scope-based lookup — same
    // strategy as connection::run_command().
    let sidecar_result = match app.shell().sidecar("binaries/proxmark3") {
        Ok(cmd) => cmd.args(&flash_args).output().await.ok(),
        Err(_) => None,
    };

    let output = if let Some(output) = sidecar_result {
        output
    } else {
        // Sidecar not available — try scope names (PATH, common install paths)
        let scope_names: Vec<&str> = if cfg!(target_os = "windows") {
            vec!["proxmark3", "proxmark3-win-c", "proxmark3-win-progfiles"]
        } else if cfg!(target_os = "macos") {
            vec!["proxmark3", "proxmark3-mac-local", "proxmark3-mac-brew"]
        } else {
            vec!["proxmark3", "proxmark3-linux-local", "proxmark3-linux-usr"]
        };

        let _ = app.emit(
            "firmware-progress",
            FirmwareProgress {
                phase: "writing".into(),
                percent: 30,
                message: "Flashing firmware (this may take up to 60 seconds)...".into(),
            },
        );

        let mut last_err = String::from("No PM3 binary found");
        let mut found = None;
        for name in &scope_names {
            match app.shell().command(name).args(&flash_args).output().await {
                Ok(out) => {
                    found = Some(out);
                    break;
                }
                Err(e) => {
                    last_err = format!("{}: {}", name, e);
                }
            }
        }

        match found {
            Some(out) => out,
            None => {
                let _ = app.emit(
                    "firmware-failed",
                    FirmwareProgress {
                        phase: "error".into(),
                        percent: 0,
                        message: format!("PM3 binary not found for flash: {}", last_err),
                    },
                );
                return Ok(());
            }
        }
    };

    // Process flash result
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !stdout.is_empty() {
        connection::emit_output(&app, &stdout, false);
    }
    if !stderr.is_empty() {
        connection::emit_output(&app, &stderr, true);
    }

    let success = output.status.success();
    let event_name = if success {
        "firmware-complete"
    } else {
        "firmware-failed"
    };
    let _ = app.emit(
        event_name,
        FirmwareProgress {
            phase: if success { "done" } else { "error" }.into(),
            percent: if success { 100 } else { 0 },
            message: if success {
                "Firmware flash complete!".into()
            } else if !stderr.is_empty() {
                stderr
                    .lines()
                    .last()
                    .unwrap_or("Flash failed")
                    .to_string()
            } else {
                format!("Flash failed (exit code: {:?})", output.status.code())
            },
        },
    );

    Ok(())
}

/// Cancel an in-progress firmware flash by killing the child process.
#[tauri::command]
pub async fn cancel_flash(flash_state: State<'_, FlashState>) -> Result<(), AppError> {
    let child = {
        let mut lock = flash_state.child.lock().map_err(|e| {
            AppError::CommandFailed(format!("Flash state lock poisoned: {}", e))
        })?;
        lock.take()
    };

    match child {
        Some(child) => {
            child.kill().map_err(|e| {
                AppError::CommandFailed(format!("Failed to kill flash process: {}", e))
            })?;
            Ok(())
        }
        None => Ok(()), // No flash running — no-op
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn firmware_file_exists(app: &AppHandle, variant: &str) -> bool {
    if !VALID_VARIANTS.contains(&variant) {
        return false;
    }
    app.path()
        .resource_dir()
        .map(|dir| {
            dir.join("firmware")
                .join(variant)
                .join("fullimage.elf")
                .exists()
        })
        .unwrap_or(false)
}

