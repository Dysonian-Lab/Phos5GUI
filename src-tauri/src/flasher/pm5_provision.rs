use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;

use crate::error::AppError;
use crate::flasher::client_flasher;
use crate::pm3::connection;
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
    let resource_dir = app.path().resource_dir().map_err(|e| {
        AppError::CommandFailed(format!("Failed to resolve resource dir: {}", e))
    })?;

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
            emit(app, "bootloader", 20, "Software bootloader command sent. Waiting for re-enumeration...");
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
    emit(
        app,
        "mcu_flash",
        30,
        "[2/4] Flashing bootrom & fullimage",
    );
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
    emit(app, "mcu_flash", 55, "MCU flash complete. Waiting for application firmware...");
    sleep(Duration::from_secs(4)).await;

    // Stage 3: FPGA bitstream
    emit(
        app,
        "fpga",
        65,
        "[3/4] Flashing FPGA bitstream",
    );
    let fpga_msg = load_fpga_bitstream(app, port).await;
    emit(app, "fpga", 75, fpga_msg);
    sleep(Duration::from_secs(2)).await;

    // Stage 4: Verification
    emit(
        app,
        "verify",
        85,
        "[4/4] Verifying BWM & FPGA",
    );
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
    emit(app, "mcu_flash", 40, "Running proxmark3.exe --flash --unlock-bootloader...");
    match client_flasher::flash_with_proxmark3(port, bootrom_path, fullimage_path).await {
        Ok(_) => Ok(()),
        Err(e) => {
            emit(
                app,
                "mcu_flash",
                45,
                format!("proxmark3 flasher failed ({}). Trying pm3-flash-all fallback...", e),
            );
            client_flasher::flash_with_pm3_flash_all(port, bootrom_path, fullimage_path).await
        }
    }
}

async fn load_fpga_bitstream(app: &AppHandle, port: &str) -> String {
    // PM5 FPGA bitstream load command varies by client build; try common forms.
    let candidates = [
        "hw fpga load",
        "fpga load",
        "hw fpga flash",
    ];

    let mut last_err: Option<String> = None;
    for cmd in candidates {
        emit(app, "fpga", 70, format!("Attempting FPGA bitstream load: `{}`", cmd));
        match connection::run_command(app, port, cmd).await {
            Ok(out) => {
                let lower = out.to_lowercase();
                if lower.contains("error") || lower.contains("failed") || lower.contains("[!!]") {
                    last_err = Some(format!("`{}` reported error output", cmd));
                    continue;
                }
                return format!("FPGA bitstream load command executed: `{}`", cmd);
            }
            Err(e) => {
                last_err = Some(format!("`{}` failed: {}", cmd, e));
            }
        }
    }

    format!(
        "FPGA bitstream load step completed with warnings ({}). Verification will confirm FPGA state.",
        last_err.unwrap_or_else(|| "no compatible FPGA load command found".to_string())
    )
}

async fn verify_pm5_state(port: &str) -> Result<String, AppError> {
    // Allow a short settle time after flash/bitstream operations.
    sleep(Duration::from_secs(2)).await;

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
                        if status.is_charging { " (charging)" } else { "" }
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
