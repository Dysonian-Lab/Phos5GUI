use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tauri::{AppHandle, Emitter};
use tokio::task::spawn_blocking;

use crate::commands::firmware::FirmwareProgress;
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

fn run_blocking_command(bin: &PathBuf, args: &[String]) -> Result<String, AppError> {
    let bin = bin.clone();
    let args_owned: Vec<String> = args.to_vec();
    let result = std::thread::spawn(move || -> Result<String, AppError> {
        let mut child = Command::new(&bin)
            .args(&args_owned)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to spawn {}: {e}", bin.display()))
            })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let stdout_thread = std::thread::spawn(move || -> String {
            let mut buf = String::new();
            if let Some(mut s) = stdout {
                use std::io::Read;
                let _ = s.read_to_string(&mut buf);
            }
            buf
        });

        let stderr_thread = std::thread::spawn(move || -> String {
            let mut buf = String::new();
            if let Some(mut s) = stderr {
                use std::io::Read;
                let _ = s.read_to_string(&mut buf);
            }
            buf
        });

        let status = child
            .wait()
            .map_err(|e| AppError::CommandFailed(format!("Failed to wait on process: {e}")))?;

        let stdout = stdout_thread.join().unwrap_or_default();
        let stderr = stderr_thread.join().unwrap_or_default();

        if !status.success() {
            let last = stderr
                .trim()
                .lines()
                .last()
                .or_else(|| stdout.trim().lines().last())
                .unwrap_or("unknown error");
            return Err(AppError::CommandFailed(format!(
                "Process failed (exit {}): {}",
                status.code().unwrap_or(-1),
                last
            )));
        }

        Ok(format!("{}{}", stdout, stderr))
    })
    .join()
    .map_err(|e| {
        let msg = if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else {
            "Task panicked".to_string()
        };
        AppError::CommandFailed(format!("Task panicked: {msg}"))
    })??;
    Ok(result)
}

/// Flash using `proxmark3 --flash --unlock-bootloader --image <bootrom> --image <fullimage>`.
///
/// Uses `.spawn()` with piped stdout/stderr and emits progress events to frontend.
pub async fn flash_with_proxmark3(
    app: &AppHandle,
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    let bin = resolve_proxmark3_binary()?;
    let bootrom = bootrom_path.to_string_lossy().into_owned();
    let fullimage = fullimage_path.to_string_lossy().into_owned();
    let args: Vec<String> = vec![
        port.to_string(),
        "--flash".to_string(),
        "--unlock-bootloader".to_string(),
        "--image".to_string(),
        bootrom,
        "--image".to_string(),
        fullimage,
    ];

    let mut child = Command::new(&bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::CommandFailed(format!("Failed to spawn {}: {e}", bin.display())))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::CommandFailed("Failed to capture stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::CommandFailed("Failed to capture stderr".into()))?;

    // Emit initial progress
    let _ = app.emit(
        "firmware-progress",
        FirmwareProgress {
            phase: "connecting".into(),
            percent: 5,
            message: "Connecting to bootloader...".into(),
        },
    );

    // Stream stdout
    let stdout_reader = BufReader::new(stdout);
    for line in stdout_reader.lines() {
        if let Ok(line) = line {
            let _ = app.emit(
                "firmware-progress",
                FirmwareProgress {
                    phase: "writing".into(),
                    percent: 30,
                    message: line.clone(),
                },
            );
            // Parse progress from output if available
            if line.contains("%") {
                if let Some(percent) = extract_percent(&line) {
                    let _ = app.emit(
                        "firmware-progress",
                        FirmwareProgress {
                            phase: "writing".into(),
                            percent,
                            message: line.clone(),
                        },
                    );
                }
            }
        }
    }

    // Stream stderr
    let stderr_reader = BufReader::new(stderr);
    for line in stderr_reader.lines() {
        if let Ok(line) = line {
            let _ = app.emit(
                "firmware-progress",
                FirmwareProgress {
                    phase: "writing".into(),
                    percent: 30,
                    message: line.clone(),
                },
            );
        }
    }

    let status = child
        .wait()
        .map_err(|e| AppError::CommandFailed(format!("Failed to wait on process: {e}")))?;

    if !status.success() {
        let _ = app.emit(
            "firmware-failed",
            FirmwareProgress {
                phase: "error".into(),
                percent: 0,
                message: "Flash process failed".into(),
            },
        );
        return Err(AppError::CommandFailed("Flash process failed".into()));
    }

    let _ = app.emit(
        "firmware-progress",
        FirmwareProgress {
            phase: "done".into(),
            percent: 100,
            message: "Firmware flash complete!".into(),
        },
    );

    let _ = app.emit(
        "firmware-complete",
        FirmwareProgress {
            phase: "done".into(),
            percent: 100,
            message: "Firmware flash complete!".into(),
        },
    );

    Ok(())
}

fn extract_percent(line: &str) -> Option<u8> {
    // Try to extract percentage from patterns like "100%" or " 50 %"
    let re = regex::Regex::new(r"(\d{1,3})\s*%").ok()?;
    re.captures(line)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u8>().ok())
        .filter(|&p| p <= 100)
}

/// Fallback flasher: `pm3-flash-all <port> <bootrom.elf> <fullimage.elf>`.
///
/// Also uses piped stdout/stderr (not `.output()`) so progress streams live.
pub async fn flash_with_pm3_flash_all(
    app: &AppHandle,
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    let bin = resolve_pm3_flash_all_binary()?;
    let bootrom = bootrom_path.to_string_lossy().into_owned();
    let fullimage = fullimage_path.to_string_lossy().into_owned();
    let args: Vec<String> = vec![port.to_string(), bootrom, fullimage];

    let mut child = Command::new(&bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::CommandFailed(format!("Failed to spawn {}: {e}", bin.display())))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::CommandFailed("Failed to capture stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::CommandFailed("Failed to capture stderr".into()))?;

    let _ = app.emit(
        "firmware-progress",
        FirmwareProgress {
            phase: "connecting".into(),
            percent: 5,
            message: "Connecting to bootloader...".into(),
        },
    );

    // Stream stdout
    let stdout_reader = BufReader::new(stdout);
    for line in stdout_reader.lines() {
        if let Ok(line) = line {
            let _ = app.emit(
                "firmware-progress",
                FirmwareProgress {
                    phase: "writing".into(),
                    percent: 30,
                    message: line.clone(),
                },
            );
            if let Some(percent) = extract_percent(&line) {
                let _ = app.emit(
                    "firmware-progress",
                    FirmwareProgress {
                        phase: "writing".into(),
                        percent,
                        message: line.clone(),
                    },
                );
            }
        }
    }

    // Stream stderr
    let stderr_reader = BufReader::new(stderr);
    for line in stderr_reader.lines() {
        if let Ok(line) = line {
            let _ = app.emit(
                "firmware-progress",
                FirmwareProgress {
                    phase: "writing".into(),
                    percent: 30,
                    message: line.clone(),
                },
            );
        }
    }

    let status = child
        .wait()
        .map_err(|e| AppError::CommandFailed(format!("Failed to wait on process: {e}")))?;

    if !status.success() {
        let _ = app.emit(
            "firmware-failed",
            FirmwareProgress {
                phase: "error".into(),
                percent: 0,
                message: "Flash process failed".into(),
            },
        );
        return Err(AppError::CommandFailed("Flash process failed".into()));
    }

    let _ = app.emit(
        "firmware-progress",
        FirmwareProgress {
            phase: "done".into(),
            percent: 100,
            message: "Firmware flash complete!".into(),
        },
    );

    let _ = app.emit(
        "firmware-complete",
        FirmwareProgress {
            phase: "done".into(),
            percent: 100,
            message: "Firmware flash complete!".into(),
        },
    );

    Ok(())
}

/// Enter bootloader via software command `hw bootloader` through the patched
/// client. The patched client connects despite a capabilities-version mismatch.
pub async fn enter_bootloader_software(port: &str) -> Result<(), AppError> {
    let bin = resolve_proxmark3_binary()?;
    let args: Vec<String> = vec![port.to_string(), "hw bootloader".to_string()];
    run_blocking_command(&bin, &args)?;
    Ok(())
}
