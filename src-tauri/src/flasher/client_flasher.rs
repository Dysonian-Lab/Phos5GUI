use std::path::{Path, PathBuf};
use std::process::Command;

use tokio::task::spawn_blocking;

use crate::error::AppError;

fn resolve_proxmark3_binary() -> Result<PathBuf, AppError> {
    let exe_name = if cfg!(target_os = "windows") {
        "proxmark3.exe"
    } else {
        "proxmark3"
    };
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let candidate = dir.join(exe_name);
            if candidate.exists() {
                return Ok(candidate);
            }
            let binaries_candidate = dir.join("binaries").join(exe_name);
            if binaries_candidate.exists() {
                return Ok(binaries_candidate);
            }
        }
    }
    Err(AppError::CommandFailed(format!(
        "Failed to locate {} near application executable",
        exe_name
    )))
}

fn resolve_pm3_flash_all_binary() -> Result<PathBuf, AppError> {
    let exe_name = if cfg!(target_os = "windows") {
        "pm3-flash-all.exe"
    } else {
        "pm3-flash-all"
    };
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let candidate = dir.join(exe_name);
            if candidate.exists() {
                return Ok(candidate);
            }
            let binaries_candidate = dir.join("binaries").join(exe_name);
            if binaries_candidate.exists() {
                return Ok(binaries_candidate);
            }
        }
    }
    Err(AppError::CommandFailed(format!(
        "Failed to locate {} near application executable",
        exe_name
    )))
}

/// Enter bootloader via software command `hw bootloader` through the patched
/// client. The patched client connects despite a capabilities-version mismatch.
pub async fn enter_bootloader_software(port: &str) -> Result<(), AppError> {
    let port = port.to_string();
    let bin = resolve_proxmark3_binary()?;
    spawn_blocking(move || -> Result<(), AppError> {
        let output = Command::new(&bin)
            .args([port.as_str(), "hw bootloader"])
            .output()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to spawn {}: {e}", bin.display()))
            })?;
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

/// Flash using `proxmark3 --flash --unlock-bootloader --image <bootrom> --image <fullimage>`.
pub async fn flash_with_proxmark3(
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    let port = port.to_string();
    let bootrom = bootrom_path.to_string_lossy().into_owned();
    let fullimage = fullimage_path.to_string_lossy().into_owned();
    let bin = resolve_proxmark3_binary()?;
    spawn_blocking(move || -> Result<(), AppError> {
        let output = Command::new(&bin)
            .args([
                port.as_str(),
                "--flash",
                "--unlock-bootloader",
                "--image",
                &bootrom,
                "--image",
                &fullimage,
            ])
            .output()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to spawn {}: {e}", bin.display()))
            })?;
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

/// Fallback flasher: `pm3-flash-all <port> <bootrom.elf> <fullimage.elf>`.
/// Built from upstream pm3-flash-all.c.
pub async fn flash_with_pm3_flash_all(
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    let port = port.to_string();
    let bootrom = bootrom_path.to_string_lossy().into_owned();
    let fullimage = fullimage_path.to_string_lossy().into_owned();
    let bin = resolve_pm3_flash_all_binary()?;
    spawn_blocking(move || -> Result<(), AppError> {
        let output = Command::new(&bin)
            .args([port.as_str(), &bootrom, &fullimage])
            .output()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to spawn {}: {e}", bin.display()))
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
