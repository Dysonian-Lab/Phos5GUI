use crate::flasher;
use crate::pm3::connection::ManagedPm5Session;
use crate::pm5::bwm::{parse_battery_status, BwmBatteryStatus};
use crate::pm5::cep::{parse_cep_status, CepStatus};
use crate::pm5::fpga::{parse_fpga_status, FpgaStatus};
use crate::pm5::hardware::{parse_antenna_status, parse_factory_info, AntennaStatus, FactoryInfo};
use crate::pm5::rgb_power::{parse_power_status, parse_rgb_status, PowerStatus, RgbStatus};
use crate::pm5::update_pipeline::{self, UpdateCheckResult};
use crate::probe::ng_protocol;
use tauri::{AppHandle, State};

/// Query PM5 BWM battery telemetry (CMD_PM5_BWM_GET_BATTERY, opcode 0x0184).
///
/// Sends the NG command directly over serial, parses the payload with
/// `pm5::bwm::parse_battery_status`, and returns the structured status.
#[tauri::command]
pub async fn get_bwm_battery_status(port: String) -> Result<BwmBatteryStatus, String> {
    let raw = ng_protocol::query_bwm_battery_raw(&port, 115200)
        .await
        .map_err(|e| e.to_string())?;

    parse_battery_status(&raw).ok_or_else(|| {
        format!(
            "Failed to parse BWM battery telemetry: payload too short ({} bytes)",
            raw.len()
        )
    })
}

/// Query PM5 CEP status (CMD_CEP_STATUS, opcode 0x0185).
///
/// Sends the NG command directly over serial, parses the payload with
/// `pm5::cep::parse_cep_status`, and returns the structured status.
#[tauri::command]
pub async fn get_cep_status(port: String) -> Result<CepStatus, String> {
    let raw = ng_protocol::query_cep_status_raw(&port, 115200)
        .await
        .map_err(|e| e.to_string())?;

    parse_cep_status(&raw).ok_or_else(|| {
        format!(
            "Failed to parse CEP status: payload too short ({} bytes)",
            raw.len()
        )
    })
}

/// Query PM5 FPGA management status (CMD_FPGA_STATUS, opcode 0x0501).
///
/// Sends the NG command directly over serial, parses the payload with
/// `pm5::fpga::parse_fpga_status`, and returns the structured status.
#[tauri::command]
pub async fn get_fpga_status(port: String) -> Result<FpgaStatus, String> {
    let raw = ng_protocol::query_fpga_status_raw(&port, 115200)
        .await
        .map_err(|e| e.to_string())?;

    parse_fpga_status(&raw).ok_or_else(|| {
        format!(
            "Failed to parse FPGA status: payload too short ({} bytes)",
            raw.len()
        )
    })
}

/// Query PM5 RGB LED status (CMD_PM5_RGB_GET_STATUS, opcode 0x0182).
#[tauri::command]
pub async fn get_rgb_status(port: String) -> Result<RgbStatus, String> {
    let raw = ng_protocol::query_rgb_status_raw(&port, 115200)
        .await
        .map_err(|e| e.to_string())?;

    parse_rgb_status(&raw).ok_or_else(|| {
        format!(
            "Failed to parse RGB status: payload too short ({} bytes)",
            raw.len()
        )
    })
}

/// Set PM5 RGB LED color/mode/brightness (CMD_PM5_RGB_SET_COLOR, opcode 0x0180).
#[tauri::command]
pub async fn set_rgb_config(
    port: String,
    red: u8,
    green: u8,
    blue: u8,
    mode: u8,
    brightness: u8,
) -> Result<(), String> {
    let payload = [red, green, blue, mode, brightness];
    ng_protocol::send_rgb_command_raw(&port, 115200, &payload)
        .await
        .map_err(|e| e.to_string())
}

/// Query PM5 power rail telemetry (CMD_PM5_POWER_GET_STATUS, opcode 0x0170).
#[tauri::command]
pub async fn get_power_status(port: String) -> Result<PowerStatus, String> {
    let raw = ng_protocol::query_power_status_raw(&port, 115200)
        .await
        .map_err(|e| e.to_string())?;

    parse_power_status(&raw).ok_or_else(|| {
        format!(
            "Failed to parse power status: payload too short ({} bytes)",
            raw.len()
        )
    })
}

/// Enable/disable a PM5 power rail (CMD_PM5_POWER_SET_RAIL, opcode 0x0171).
/// `rail` accepts "v33" or "v50".
#[tauri::command]
pub async fn set_power_rail(port: String, rail: String, enabled: bool) -> Result<(), String> {
    let rail_byte = match rail.as_str() {
        "v33" | "3v3" => 0x00,
        "v50" | "5v0" => 0x01,
        other => {
            return Err(format!(
                "Invalid power rail '{other}': expected 'v33'/'3v3' or 'v50'/'5v0'"
            ))
        }
    };
    let payload = [rail_byte, if enabled { 0x01 } else { 0x00 }];
    ng_protocol::send_power_command_raw(&port, 115200, &payload)
        .await
        .map_err(|e| e.to_string())
}

/// Query PM5 factory production info (CMD_PM5_GET_FACTORY_INFO, opcode 0x0160).
#[tauri::command]
pub async fn get_factory_info(port: String) -> Result<FactoryInfo, String> {
    let raw = ng_protocol::query_factory_info_raw(&port, 115200)
        .await
        .map_err(|e| e.to_string())?;

    parse_factory_info(&raw).ok_or_else(|| {
        format!(
            "Failed to parse factory info: payload too short ({} bytes)",
            raw.len()
        )
    })
}

/// Query PM5 antenna tuning/telemetry status (CMD_PM5_ANTENNA_GET_STATUS, opcode 0x0162).
#[tauri::command]
pub async fn get_antenna_status(port: String) -> Result<AntennaStatus, String> {
    let raw = ng_protocol::query_antenna_status_raw(&port, 115200)
        .await
        .map_err(|e| e.to_string())?;

    parse_antenna_status(&raw).ok_or_else(|| {
        format!(
            "Failed to parse antenna status: payload too short ({} bytes)",
            raw.len()
        )
    })
}

/// Run the complete PM5 provisioning pipeline (bootloader -> MCU flash -> FPGA -> verify).
/// Emits `flash-progress` events with stage/percent/message updates.
#[tauri::command]
pub async fn flash_pm5_complete(
    app: AppHandle,
    port: String,
    variant: String,
) -> Result<String, String> {
    flasher::pm5_provision(&app, &port, &variant)
        .await
        .map_err(|e| e.to_string())
}

/// Check the independent sandbox for upstream PM5 commits ahead of local HEAD.
///
/// Runs `git fetch origin master` inside `.build-env/pm3-src` and compares
/// `HEAD` against `origin/master`, returning the commit diff and changelog.
#[tauri::command]
pub async fn check_pm5_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    update_pipeline::check_updates(&app)
        .await
        .map_err(|e| e.to_string())
}

/// Run the full 1-click upstream update & flash pipeline for the PM5.
///
/// Emits `pm5-update-progress` events at every stage (merge, patch, firmware
/// compile, client compile, hot-swap, flash). Returns the provisioning summary.
#[tauri::command]
pub async fn execute_pm5_update(app: AppHandle, port: String) -> Result<String, String> {
    update_pipeline::execute_update_pipeline(app, port)
        .await
        .map_err(|e| e.to_string())
}

/// Start (or replace) a persistent interactive PM3 session on `port`.
#[tauri::command]
pub async fn pm5_session_start(
    state: State<'_, ManagedPm5Session>,
    port: String,
) -> Result<(), String> {
    state.start(&port).await.map_err(|e| e.to_string())
}

/// Send one command to the active interactive PM3 session and return its output.
#[tauri::command]
pub async fn pm5_session_send(
    state: State<'_, ManagedPm5Session>,
    cmd: String,
) -> Result<String, String> {
    state.send(&cmd).await.map_err(|e| e.to_string())
}

/// Terminate the active interactive PM3 session, if any.
#[tauri::command]
pub async fn pm5_session_stop(state: State<'_, ManagedPm5Session>) -> Result<(), String> {
    state.stop().await;
    Ok(())
}
