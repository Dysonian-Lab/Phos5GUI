use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;

use crate::error::AppError;
use crate::flasher::client_flasher;
use crate::pm5::bwm::parse_battery_status;
use crate::pm5::fpga::parse_fpga_status;
use crate::probe::ng_protocol;

#[derive(Debug, Clone, Serialize)]
struct FlashProgress {
    stage: String,
    percent: u8,
    message: String,
}

fn emit(app: &AppHandle, stage: &str, percent: u8, message: impl Into<String>) {
    let _ = app.emit(
        "flash-progress",
        FlashProgress {
            stage: stage.to_string(),
            percent,
            message: message.into(),
        },
    );
}

fn strip_win_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

fn resolve_pm5_firmware(app: &AppHandle, variant: &str) -> Result<(PathBuf, PathBuf), AppError> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| AppError::CommandFailed(format!("Failed to resolve resource dir: {}", e)))?;

    let candidates = [
        resource_dir.join("firmware").join("pm5").join(variant),
        resource_dir.join("firmware").join(variant),
    ];

    candidates
        .iter()
        .find_map(|base| {
            let bootrom = base.join("bootrom.elf");
            let fullimage = base.join("fullimage.elf");
            if bootrom.exists() && fullimage.exists() {
                Some((
                    strip_win_prefix(bootrom),
                    strip_win_prefix(fullimage),
                ))
            } else {
                None
            }
        })
        .ok_or_else(|| {
            AppError::CommandFailed(format!(
                "Bundled PM5 firmware not found for variant '{}'. Expected bootrom.elf and fullimage.elf.",
                variant
            ))
        })
}

/// Complete PM5 provisioning pipeline:
/// 1) Bootloader transition
/// 2) Flash bootrom + fullimage (MCU)
/// 3) FPGA bitstream load
/// 4) Verify BWM + FPGA status
pub async fn pm5_provision(app: &AppHandle, port: &str, variant: &str) -> Result<String, AppError> {
    let variant = variant.to_lowercase();
    if variant != "standard" && variant != "bwm" {
        return Err(AppError::CommandFailed(format!(
            "Invalid PM5 variant '{}'. Expected 'standard' or 'bwm'.",
            variant
        )));
    }

    let (bootrom_path, fullimage_path) = resolve_pm5_firmware(app, &variant)?;

    // Stage 1: Bootloader transition
    emit(app, "bootloader", 5, "[1/4] Entering bootloader");
    emit(
        app,
        "bootloader",
        10,
        "Attempting software bootloader entry (CMD_START_FLASH)...",
    );
    match client_flasher::enter_bootloader_software(port).await {
        Ok(_) => {
            emit(
                app,
                "bootloader",
                20,
                "Software bootloader command sent. Waiting for re-enumeration...",
            );
            sleep(Duration::from_secs(3)).await;
        }
        Err(e) => {
            emit(
                app,
                "bootloader",
                20,
                format!(
                    "Software bootloader entry unavailable ({}). Proceeding with flasher over current USB-CDC port; if needed, hold PM5 button while plugging in.",
                    e
                ),
            );
            sleep(Duration::from_secs(1)).await;
        }
    }

    // Stage 2: MCU flash (bootrom + fullimage)
    emit(app, "mcu_flash", 30, "[2/4] Flashing bootrom & fullimage");
    emit(
        app,
        "mcu_flash",
        35,
        format!(
            "Flashing PM5 {}: {} + {}",
            variant,
            bootrom_path.display(),
            fullimage_path.display()
        ),
    );
    flash_mcu(app, port, &bootrom_path, &fullimage_path).await?;
    emit(
        app,
        "mcu_flash",
        55,
        "MCU flash complete. Waiting for application firmware...",
    );
    sleep(Duration::from_secs(4)).await;

    // Stage 3: FPGA bitstream
    emit(app, "fpga", 65, "[3/4] Flashing FPGA bitstream");
    let fpga_msg = load_fpga_bitstream(app, port).await;
    emit(app, "fpga", 75, fpga_msg);
    sleep(Duration::from_secs(5)).await;

    // Stage 4: Verification
    emit(app, "verify", 85, "[4/4] Verifying BWM & FPGA");
    let verification = verify_pm5_state(port).await?;
    emit(app, "verify", 100, verification.clone());

    Ok(verification)
}

async fn flash_mcu(
    app: &AppHandle,
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    emit(
        app,
        "mcu_flash",
        40,
        "Running proxmark3.exe --flash --unlock-bootloader...",
    );
    match client_flasher::flash_with_proxmark3(app, port, bootrom_path, fullimage_path).await {
        Ok(_) => Ok(()),
        Err(e) => {
            emit(
                app,
                "mcu_flash",
                45,
                format!(
                    "proxmark3 flasher failed ({}). Trying pm3-flash-all fallback...",
                    e
                ),
            );
            client_flasher::flash_with_pm3_flash_all(app, port, bootrom_path, fullimage_path).await
        }
    }
}

/// Locate a PM5 FPGA bitstream shipped alongside the firmware.
fn find_fpga_bitstream(app: &AppHandle) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.join("firmware").join("pm5").join("bwm"));
        dirs.push(res.join("firmware").join("pm5"));
        dirs.push(res.join("firmware"));
    }
    let names = ["fpga_bitstream.bin", "fpga.bin", "bitstream.bin"];
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Load the FPGA bitstream via the concrete CMD_FPGA_BITSTREAM_LOAD NG frame
/// (Plan Section 8.1 / 12) instead of guessing client CLI strings.
async fn load_fpga_bitstream(app: &AppHandle, port: &str) -> String {
    match find_fpga_bitstream(app) {
        Some(path) => {
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    return format!(
                        "FPGA bitstream load skipped: failed to read {}: {e}",
                        path.display()
                    );
                }
            };
            emit(
                app,
                "fpga",
                70,
                format!(
                    "Loading FPGA bitstream ({} bytes) via CMD_FPGA_BITSTREAM_LOAD",
                    bytes.len()
                ),
            );
            match ng_protocol::load_fpga_bitstream_raw(port, 115200, &bytes).await {
                Ok(_) => format!(
                    "FPGA bitstream loaded via NG frame ({} bytes).",
                    bytes.len()
                ),
                Err(e) => format!(
                    "FPGA bitstream load failed: {e}. Verification will confirm FPGA state."
                ),
            }
        }
        None => {
            emit(
                app,
                "fpga",
                70,
                "No FPGA bitstream file found in the firmware directory.",
            );
            "FPGA bitstream load skipped: no bitstream file found. Verification will confirm FPGA state."
                .to_string()
        }
    }
}

async fn verify_pm5_state(port: &str) -> Result<String, AppError> {
    // Allow a longer settle time after flash/bitstream operations.
    // PM5 BWM needs time for battery management circuit to initialize.
    sleep(Duration::from_secs(6)).await;

    let mut fpga_ok = false;
    let mut fpga_detail = "unknown".to_string();
    let mut bwm_ok = false;
    let mut bwm_detail = "unknown".to_string();

    // Verify FPGA status (bitstream loaded)
    for attempt in 1..=3 {
        match ng_protocol::query_fpga_status_raw(port, 115200).await {
            Ok(raw) => {
                if let Some(status) = parse_fpga_status(&raw) {
                    fpga_ok = status.bitstream_loaded;
                    fpga_detail = format!(
                        "bitstream_loaded={}, version='{}'",
                        status.bitstream_loaded, status.fpga_version
                    );
                    if fpga_ok {
                        break;
                    }
                } else {
                    fpga_detail = format!("unparseable payload ({} bytes)", raw.len());
                }
            }
            Err(e) => {
                fpga_detail = format!("query failed (attempt {attempt}/3): {e}");
            }
        }
        if attempt < 3 {
            sleep(Duration::from_secs(2)).await;
        }
    }

    // Verify BWM telemetry operational
    for attempt in 1..=3 {
        match ng_protocol::query_bwm_battery_raw(port, 115200).await {
            Ok(raw) => {
                if let Some(status) = parse_battery_status(&raw) {
                    bwm_ok = true;
                    bwm_detail = format!(
                        "{}% / {} mV{}",
                        status.battery_percent,
                        status.voltage_mv,
                        if status.is_charging {
                            " (charging)"
                        } else {
                            ""
                        }
                    );
                    break;
                } else {
                    bwm_detail = format!("unparseable payload ({} bytes)", raw.len());
                }
            }
            Err(e) => {
                bwm_detail = format!("query failed (attempt {attempt}/3): {e}");
            }
        }
        if attempt < 3 {
            sleep(Duration::from_secs(2)).await;
        }
    }

    if fpga_ok && bwm_ok {
        Ok(format!(
            "Provisioning complete. FPGA: LOADED ({fpga_detail}). BWM telemetry: OK ({bwm_detail})."
        ))
    } else {
        Err(AppError::CommandFailed(format!(
            "Provisioning verification failed. FPGA: {} ({}). BWM: {} ({}).",
            if fpga_ok { "OK" } else { "NOT READY" },
            fpga_detail,
            if bwm_ok { "OK" } else { "NOT READY" },
            bwm_detail
        )))
    }
}
