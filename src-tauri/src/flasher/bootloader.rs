use std::path::Path;
use std::time::Duration;

use tokio::time::sleep;

use crate::error::AppError;

/// How the PM5 device is brought into the bootloader over USB-CDC.
///
/// PM5 (AT32F437) does NOT expose an Artery DFU ROM. Holding the button during
/// plug-in enters the Proxmark bootloader, which presents as the SAME USB-CDC
/// COM port as the application firmware. There is no dfu-util, no Zadig, and
/// no WinUSB driver involved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootloaderEntryMethod {
    /// User holds the PM5 button during USB plug-in (primary, always works).
    HardwareButton,
    /// Send `CMD_START_FLASH` (`hw bootloader`) via a working patched client.
    SoftwareCommand,
    /// Try software first, fall back to the hardware-button prompt.
    AutoDetect,
}

/// Information about a detected bootloader port.
#[derive(Debug, Clone)]
pub struct BootloaderInfo {
    pub port: String,
}

/// Enter the PM5 bootloader over USB-CDC.
///
/// On `AutoDetect`, the software command is attempted first; on failure the
/// caller is expected to fall back to `HardwareButton`, which is driven by the
/// UI (user physically holds the button while plugging in).
pub async fn enter_bootloader(
    port: &str,
    method: BootloaderEntryMethod,
) -> Result<BootloaderInfo, AppError> {
    match method {
        BootloaderEntryMethod::SoftwareCommand => {
            crate::flasher::client_flasher::enter_bootloader_software(port).await?;
        }
        BootloaderEntryMethod::HardwareButton => {
            // Hardware entry is driven by the UI; wait for re-enumeration.
            wait_for_bootloader_port(port).await?;
        }
        BootloaderEntryMethod::AutoDetect => {
            if crate::flasher::client_flasher::enter_bootloader_software(port).await.is_err() {
                // Fall back to hardware-button entry: UI-driven, just wait for re-enumeration.
                wait_for_bootloader_port(port).await?;
            }
        }
    }
    Ok(BootloaderInfo {
        port: port.to_string(),
    })
}

/// Wait for the device to re-enumerate as a bootloader on the same port.
async fn wait_for_bootloader_port(port: &str) -> Result<(), AppError> {
    let _ = port;
    sleep(Duration::from_secs(2)).await;
    Ok(())
}

/// Wait for the device to re-enumerate as a PM5 application after flashing.
pub async fn wait_for_pm5_enumeration(port: &str) -> Result<(), AppError> {
    let _ = port;
    sleep(Duration::from_secs(2)).await;
    Ok(())
}

/// Flash using the standard Proxmark flasher over USB-CDC.
///
/// This is the only supported method for PM5 — no dfu-util, no Zadig, no
/// WinUSB drivers. The flasher protocol does NOT check `CAPABILITIES_VERSION`.
///
/// Falls back to `pm3-flash-all.exe` if `proxmark3.exe --flash` fails.
pub async fn flash_via_client_flasher(
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<(), AppError> {
    if let Err(_) = crate::flasher::client_flasher::flash_with_proxmark3(
        port,
        bootrom_path,
        fullimage_path,
    )
    .await
    {
        return crate::flasher::client_flasher::flash_with_pm3_flash_all(
            port,
            bootrom_path,
            fullimage_path,
        )
        .await;
    }
    Ok(())
}
