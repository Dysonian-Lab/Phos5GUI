//! PM5 firmware path resolution and verification (Plan Section 3.2 & 8.1).
//!
//! Resolves paths to bundled standard and BWM bootrom.elf + fullimage.elf,
//! validates existence and ELF binary headers.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::AppError;

/// A PM5 firmware variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Pm5Variant {
    Standard,
    Bwm,
}

impl Pm5Variant {
    pub fn as_str(self) -> &'static str {
        match self {
            Pm5Variant::Standard => "standard",
            Pm5Variant::Bwm => "bwm",
        }
    }
}

/// Resolved firmware paths for a variant.
#[derive(Debug, Clone)]
pub struct Pm5FirmwarePaths {
    pub variant: Pm5Variant,
    pub bootrom: PathBuf,
    pub fullimage: PathBuf,
}

/// ELF magic bytes: `\x7fELF`.
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

/// Resolve paths to bundled PM5 firmware for `variant`.
///
/// Searches both the Tauri resource directory (production) and the
/// `src-tauri/firmware/pm5/<variant>/` tree (development).
pub fn resolve_pm5_firmware(variant: Pm5Variant) -> Result<Pm5FirmwarePaths, AppError> {
    let dirs = firmware_dirs(variant);

    for base in &dirs {
        let bootrom = base.join("bootrom.elf");
        let fullimage = base.join("fullimage.elf");
        if bootrom.is_file() && fullimage.is_file() {
            return Ok(Pm5FirmwarePaths {
                variant,
                bootrom,
                fullimage,
            });
        }
    }

    let tried: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
    Err(AppError::CommandFailed(format!(
        "Bundled PM5 {} firmware not found (bootrom.elf + fullimage.elf). Tried: {}",
        variant.as_str(),
        tried.join(", ")
    )))
}

/// Resolve paths for both PM5 firmware variants.
pub fn resolve_all_pm5_firmware() -> Result<Vec<Pm5FirmwarePaths>, AppError> {
    Ok(vec![
        resolve_pm5_firmware(Pm5Variant::Standard)?,
        resolve_pm5_firmware(Pm5Variant::Bwm)?,
    ])
}

/// Build the list of directories to search for a variant's firmware.
fn firmware_dirs(variant: Pm5Variant) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    // Production: Tauri resource directory.
    if let Ok(res_dir) = std::env::var("TAURI_RESOURCE_DIR") {
        let base = PathBuf::from(&res_dir)
            .join("firmware")
            .join("pm5")
            .join(variant.as_str());
        dirs.push(base);
        dirs.push(
            PathBuf::from(&res_dir)
                .join("firmware")
                .join(variant.as_str()),
        );
    }

    // Development: relative to CARGO_MANIFEST_DIR.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dirs.push(
        manifest_dir
            .join("firmware")
            .join("pm5")
            .join(variant.as_str()),
    );
    dirs.push(manifest_dir.join("firmware").join(variant.as_str()));

    // Runtime root (next to executable) for portable installs.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("firmware").join("pm5").join(variant.as_str()));
        }
    }

    dirs
}

/// Verify that an ELF file exists and has a valid ELF magic header.
pub fn verify_elf(path: &Path) -> Result<(), AppError> {
    if !path.is_file() {
        return Err(AppError::CommandFailed(format!(
            "Firmware file not found: {}",
            path.display()
        )));
    }

    let header = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err(AppError::CommandFailed(format!(
                "Failed to read firmware {}: {e}",
                path.display()
            )));
        }
    };

    if header.len() < 4 {
        return Err(AppError::CommandFailed(format!(
            "Firmware file too small to be ELF: {}",
            path.display()
        )));
    }

    if &header[..4] != ELF_MAGIC {
        return Err(AppError::CommandFailed(format!(
            "Invalid ELF magic in {}: expected \\x7fELF, got {:02X?}",
            path.display(),
            &header[..4]
        )));
    }

    Ok(())
}

/// Verify both bootrom and fullimage ELF files for a variant.
pub fn verify_pm5_firmware(paths: &Pm5FirmwarePaths) -> Result<(), AppError> {
    verify_elf(&paths.bootrom)?;
    verify_elf(&paths.fullimage)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elf_magic_is_valid() {
        assert_eq!(ELF_MAGIC, [0x7F, b'E', b'L', b'F']);
    }

    #[test]
    fn variant_as_str() {
        assert_eq!(Pm5Variant::Standard.as_str(), "standard");
        assert_eq!(Pm5Variant::Bwm.as_str(), "bwm");
    }
}
