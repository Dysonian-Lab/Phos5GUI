use std::path::Path;
use std::process::Command;

use tokio::task::spawn_blocking;

use crate::error::AppError;

/// Enter bootloader via software command `hw bootloader` through the patched
/// client. The patched client connects despite a capabilities-version mismatch.
pub async fn enter_bootloader_software(port: &str) -> Result<(), AppError> {
    let port = port.to_string();
    spawn_blocking(move || -> Result<(), AppError> {
        let output = Command::new("proxmark3.exe")
            .args([&port, "hw bootloader"])
            .output()
            .map_err(|e| AppError::CommandFailed(format!("Failed to spawn proxmark3.exe: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::CommandFailed(format!(
                "hw bootloader failed: {stderr}"
            )));
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Bootloader task failed: {e}")))?
}

/// Flash using `proxmark3.exe --flash --unlock-bootloader --image <bootrom> --image <fullimage>`.
pub async fn flash_with_proxmark3(
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    let port = port.to_string();
    let bootrom = bootrom_path.to_string_lossy().into_owned();
    let fullimage = fullimage_path.to_string_lossy().into_owned();
    spawn_blocking(move || -> Result<(), AppError> {
        let output = Command::new("proxmark3.exe")
            .args([
                &port,
                "--flash",
                "--unlock-bootloader",
                "--image",
                &bootrom,
                "--image",
                &fullimage,
            ])
            .output()
            .map_err(|e| AppError::CommandFailed(format!("Failed to spawn proxmark3.exe: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::CommandFailed(format!(
                "proxmark3 --flash failed: {stderr}"
            )));
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Flash task failed: {e}")))?
}

/// Fallback flasher: `pm3-flash-all.exe <port> <bootrom.elf> <fullimage.elf>`.
/// Built from ProxSpace `tools/pm3-flash-all.c`.
pub async fn flash_with_pm3_flash_all(
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    let port = port.to_string();
    let bootrom = bootrom_path.to_string_lossy().into_owned();
    let fullimage = fullimage_path.to_string_lossy().into_owned();
    spawn_blocking(move || -> Result<(), AppError> {
        let output = Command::new("pm3-flash-all.exe")
            .args([&port, &bootrom, &fullimage])
            .output()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to spawn pm3-flash-all.exe: {e}"))
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::CommandFailed(format!(
                "pm3-flash-all failed: {stderr}"
            )));
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Flash-all task failed: {e}")))?
}
