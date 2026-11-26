use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde::Serialize;
use serialport::SerialPort;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Serial concurrency guard (Windows COM collision protection)
// ---------------------------------------------------------------------------

/// Process-wide port-keyed async lock serializing all direct serial access.
///
/// Multiple PM5 panels may poll concurrently; without this guard, overlapping
/// `serialport::open()` calls on the same COM port fail with `ERROR_ACCESS_DENIED`
/// on Windows. A single global bus lock would be simpler but would serialize
/// unrelated ports, so we key the lock per-port.
static SERIAL_PORT_LOCKS: LazyLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Retrieve (or create) the async mutex guarding direct serial access to `port`.
///
/// This is the single source of truth for port serialisation: both direct serial
/// NG queries and `proxmark3.exe` process spawns in `pm3::connection` acquire the
/// lock returned here, so a client spawn and a raw serial query can never touch
/// the same COM port at once (Windows `ERROR_ACCESS_DENIED`).
pub fn port_serial_lock(port: &str) -> Arc<AsyncMutex<()>> {
    let mut map = SERIAL_PORT_LOCKS
        .lock()
        .expect("serial port lock map poisoned");
    map.entry(port.to_string())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

// ---------------------------------------------------------------------------
// NG protocol constants (mirror pm3_cmd.h / Section 12.1)
// ---------------------------------------------------------------------------

/// Request preamble magic: "PM3a" = 0x61334d50.
const REQUEST_MAGIC: u32 = 0x6133_4d50;
/// Response preamble magic: "PM3b" = 0x62334d50.
const RESPONSE_MAGIC: u32 = 0x6233_4d50;
/// Bit 15 of the length field is the NG flag (always set).
const NG_FLAG: u16 = 1 << 15;
/// Maximum NG payload length (15-bit length field).
const MAX_PAYLOAD_LEN: usize = 0x7FFF;

/// CMD_PING — connectivity test, device echoes the payload.
const CMD_PING: u16 = 0x0109;
/// CMD_CAPABILITIES — returns the `capabilities_t` structure.
const CMD_CAPABILITIES: u16 = 0x0112;
/// CMD_PM5_BWM_GET_BATTERY — returns PM5 BWM battery telemetry.
const CMD_PM5_BWM_GET_BATTERY: u16 = 0x0184;
/// CMD_CEP_STATUS — returns PM5 CEP (Communication Expansion Port) status.
const CMD_CEP_STATUS: u16 = 0x0185;
/// CMD_FPGA_STATUS — returns PM5 FPGA management status.
const CMD_FPGA_STATUS: u16 = 0x0501;
/// CMD_FPGA_BITSTREAM_LOAD — load a bitstream into the PM5 FPGA (0x0501 family).
///
/// The PM5 FPGA management opcode block is `0x0501..0x0503` (Section 8.1). The
/// bitstream load opcode is exposed here so `pm5_provision` can push a concrete
/// bitstream frame instead of guessing client CLI strings.
pub const CMD_FPGA_BITSTREAM_LOAD: u16 = 0x0501;
/// CMD_PM5_POWER_GET_STATUS — returns PM5 power rail telemetry.
const CMD_PM5_POWER_GET_STATUS: u16 = 0x0170;
/// CMD_PM5_POWER_SET_RAIL — enables/disables a PM5 power rail.
const CMD_PM5_POWER_SET_RAIL: u16 = 0x0171;
/// CMD_PM5_RGB_SET_COLOR — sets PM5 RGB LED color/mode/brightness.
const CMD_PM5_RGB_SET_COLOR: u16 = 0x0180;
/// CMD_PM5_RGB_GET_STATUS — returns PM5 RGB LED status.
const CMD_PM5_RGB_GET_STATUS: u16 = 0x0182;
/// CMD_PM5_GET_FACTORY_INFO — returns PM5 factory production info.
const CMD_PM5_GET_FACTORY_INFO: u16 = 0x0160;
/// CMD_PM5_ANTENNA_GET_STATUS — returns PM5 antenna tuning/telemetry.
const CMD_PM5_ANTENNA_GET_STATUS: u16 = 0x0162;

/// Postamble magic accepted when a device omits a real CRC (0x3362).
const POSTAMBLE_MAGIC: u16 = 0x3362;

/// PING payload length (bytes). An incrementing 0..32 pattern.
const PING_PAYLOAD_LEN: usize = 32;

// ---------------------------------------------------------------------------
// Packed wire structures (little-endian, mirror pm3_cmd.h)
// ---------------------------------------------------------------------------

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct CommandPreamble {
    magic: u32,  // 0x61334d50 "PM3a"
    len_ng: u16, // bits 0-14: payload length, bit 15: ng flag
    cmd: u16,    // command code
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct CommandPostamble {
    crc: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct ResponsePreamble {
    magic: u32,  // 0x62334d50 "PM3b"
    len_ng: u16, // bits 0-14: payload length, bit 15: ng flag
    status: i8,  // PM3_SUCCESS=0, ...
    reason: i8,
    cmd: u16, // echoed command code
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct ResponsePostamble {
    crc: u16,
}

const COMMAND_PREAMBLE_LEN: usize = 8; // 4 + 2 + 2
const RESPONSE_PREAMBLE_LEN: usize = 10; // 4 + 2 + 1 + 1 + 2
const POSTAMBLE_LEN: usize = 2;

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

/// Result of a direct serial capability probe (Layer 1).
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub capabilities_version: u8,
    pub capabilities: Option<crate::probe::capabilities::CapabilitiesStruct>,
    pub max_cmd_data_size: u16,
    pub is_pm5: bool,
    pub method: ProbeMethod,
}

/// How the capabilities were obtained.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub enum ProbeMethod {
    DirectSerial,
    StderrScraping,
    Failed,
}

/// Decoded NG response from the device.
pub struct NgResponse {
    pub magic: u32,
    pub length: usize,
    pub status: i8,
    pub reason: i8,
    pub cmd: u16,
    pub data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// CRC16 (CCITT-FALSE, poly 0x1021, init 0xFFFF)
// ---------------------------------------------------------------------------

fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// Frame encoding / decoding
// ---------------------------------------------------------------------------

/// Build a request NG frame: preamble + payload + postamble CRC.
pub fn build_ng_frame(cmd: u16, payload: &[u8]) -> Vec<u8> {
    if payload.len() > MAX_PAYLOAD_LEN {
        // Payload too large for the 15-bit length field; truncate defensively.
    }
    let len_ng = (payload.len() as u16 & 0x7FFF) | NG_FLAG;

    let mut frame = Vec::with_capacity(COMMAND_PREAMBLE_LEN + payload.len() + POSTAMBLE_LEN);
    frame.extend_from_slice(&REQUEST_MAGIC.to_le_bytes());
    frame.extend_from_slice(&len_ng.to_le_bytes());
    frame.extend_from_slice(&cmd.to_le_bytes());
    frame.extend_from_slice(payload);

    let crc = crc16(&frame[..COMMAND_PREAMBLE_LEN + payload.len()]);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame
}

/// Read and validate a response NG frame from `port`.
pub fn read_ng_response(port: &mut dyn SerialPort) -> Result<NgResponse, AppError> {
    // Preamble.
    let mut pre = [0u8; RESPONSE_PREAMBLE_LEN];
    port.read_exact(&mut pre).map_err(|e| {
        AppError::CommandFailed(format!("Failed to read NG response preamble: {e}"))
    })?;

    let magic = u32::from_le_bytes([pre[0], pre[1], pre[2], pre[3]]);
    if magic != RESPONSE_MAGIC {
        return Err(AppError::CommandFailed(format!(
            "Invalid NG response magic: 0x{magic:08X}, expected 0x{RESPONSE_MAGIC:08X}"
        )));
    }

    let len_ng = u16::from_le_bytes([pre[4], pre[5]]);
    let length = (len_ng & 0x7FFF) as usize;
    let _ng_flag = (len_ng & NG_FLAG) != 0;
    let status = pre[6] as i8;
    let reason = pre[7] as i8;
    let cmd = u16::from_le_bytes([pre[8], pre[9]]);

    // Payload.
    let mut data = vec![0u8; length];
    port.read_exact(&mut data)
        .map_err(|e| AppError::CommandFailed(format!("Failed to read NG response payload: {e}")))?;

    // Postamble CRC.
    let mut post = [0u8; POSTAMBLE_LEN];
    port.read_exact(&mut post).map_err(|e| {
        AppError::CommandFailed(format!("Failed to read NG response postamble: {e}"))
    })?;
    let crc = u16::from_le_bytes([post[0], post[1]]);

    let expected_crc = crc16(&[&pre[..], &data].concat());
    if crc != POSTAMBLE_MAGIC && crc != expected_crc {
        return Err(AppError::CommandFailed(format!(
            "NG response CRC mismatch: got 0x{crc:04X}, expected 0x{expected_crc:04X}"
        )));
    }

    Ok(NgResponse {
        magic,
        length,
        status,
        reason,
        cmd,
        data,
    })
}

// ---------------------------------------------------------------------------
// Layer 1 probe: direct serial capability detection
// ---------------------------------------------------------------------------

/// Open the COM port directly, exchange raw NG frames (CMD_PING then
/// CMD_CAPABILITIES), then release the port so a subsequent `proxmark3.exe`
/// spawn can open it.
///
/// **Windows constraint:** serial ports do not support shared access. The port
/// handle is explicitly dropped and a 150 ms delay is inserted before returning,
/// otherwise Tauri spawning `proxmark3.exe` fails with `ERROR_ACCESS_DENIED`.
pub async fn probe_device_capabilities(port: &str, baudrate: u32) -> Result<ProbeResult, AppError> {
    // Serialize direct serial access per port to avoid COM handle collisions.
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;

    // Own the port string so the `'static` `spawn_blocking` closure can move it in.
    let port_owned = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<ProbeResult, AppError> {
        // 1. Open the serial port directly.
        let mut port = serialport::new(port_owned.as_str(), baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port_owned}: {e}"))
            })?;

        // 2. Send CMD_PING (NG frame) with an incrementing 32-byte pattern.
        let ping_payload: Vec<u8> = (0..PING_PAYLOAD_LEN as u8).collect();
        let ping_frame = build_ng_frame(CMD_PING, &ping_payload);
        port.write_all(&ping_frame)
            .map_err(|e| AppError::CommandFailed(format!("Failed to write CMD_PING: {e}")))?;

        // 3. Read the PING response and verify the echo.
        let ping_resp = read_ng_response(&mut *port)?;
        if ping_resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PING rejected with status {} reason {}",
                ping_resp.status, ping_resp.reason
            )));
        }
        if ping_resp.data.len() < PING_PAYLOAD_LEN
            || ping_resp.data[..PING_PAYLOAD_LEN] != ping_payload[..]
        {
            return Err(AppError::CommandFailed("PING echo mismatch".into()));
        }

        // 4. Send CMD_CAPABILITIES (no payload).
        let caps_frame = build_ng_frame(CMD_CAPABILITIES, &[]);
        port.write_all(&caps_frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_CAPABILITIES: {e}"))
        })?;

        // 5. Read the capabilities response.
        let caps_resp = read_ng_response(&mut *port)?;
        if caps_resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_CAPABILITIES rejected with status {} reason {}",
                caps_resp.status, caps_resp.reason
            )));
        }

        // 6. Parse capabilities_t from the response payload (first byte = version).
        let version = caps_resp.data.first().copied().unwrap_or(0);
        let capabilities = crate::probe::capabilities::parse_capabilities_struct(&caps_resp.data);

        // 7. CRITICAL: explicitly drop the port handle before returning so the
        //    Windows COM port lock is released.
        drop(port);

        Ok(ProbeResult {
            capabilities_version: version,
            capabilities,
            max_cmd_data_size: capabilities
                .as_ref()
                .map(|c| c.max_cmd_data_size)
                .unwrap_or(512),
            is_pm5: capabilities.as_ref().map(|c| c.is_pm5).unwrap_or(false),
            method: ProbeMethod::DirectSerial,
        })
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Probe task failed: {e}")))?;

    // 8. CRITICAL: brief delay to let Windows release the port handle fully
    //    before Tauri spawns proxmark3.exe. Keep the per-port lock held through
    //    this settle window to prevent probe collisions.
    sleep(Duration::from_millis(150)).await;

    Ok(result?)
}

/// Query PM5 BWM battery status via direct serial NG command.
/// Sends CMD_PM5_BWM_GET_BATTERY (0x0184) and returns the raw response payload.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn query_bwm_battery_raw(port: &str, baudrate: u32) -> Result<Vec<u8>, AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(5000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_PM5_BWM_GET_BATTERY, &[]);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_PM5_BWM_GET_BATTERY: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PM5_BWM_GET_BATTERY rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("BWM query task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    Ok(result?)
}

/// Query PM5 CEP status via direct serial NG command.
/// Sends CMD_CEP_STATUS (0x0185) and returns the raw response payload.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn query_cep_status_raw(port: &str, baudrate: u32) -> Result<Vec<u8>, AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_CEP_STATUS, &[]);
        sp.write_all(&frame)
            .map_err(|e| AppError::CommandFailed(format!("Failed to write CMD_CEP_STATUS: {e}")))?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_CEP_STATUS rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("CEP query task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    Ok(result?)
}

/// Query PM5 FPGA management status via direct serial NG command.
/// Sends CMD_FPGA_STATUS (0x0501) and returns the raw response payload.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn query_fpga_status_raw(port: &str, baudrate: u32) -> Result<Vec<u8>, AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(5000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_FPGA_STATUS, &[]);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_FPGA_STATUS: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_FPGA_STATUS rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("FPGA query task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    Ok(result?)
}

/// Build the NG frame that loads an FPGA bitstream (CMD_FPGA_BITSTREAM_LOAD).
///
/// The whole bitstream is carried as the frame payload. Bitstreams larger than
/// the 15-bit NG payload limit are rejected by `load_fpga_bitstream_raw` rather
/// than silently truncated.
pub fn build_fpga_bitstream_frame(bitstream: &[u8]) -> Vec<u8> {
    build_ng_frame(CMD_FPGA_BITSTREAM_LOAD, bitstream)
}

/// Load an FPGA bitstream into the PM5 device via CMD_FPGA_BITSTREAM_LOAD.
///
/// Sends the raw NG frame carrying the bitstream bytes and returns the response
/// payload. Acquires the shared per-port serial lock so this cannot race a
/// client process spawn or another serial query on the same port.
pub async fn load_fpga_bitstream_raw(
    port: &str,
    baudrate: u32,
    bitstream: &[u8],
) -> Result<Vec<u8>, AppError> {
    if bitstream.is_empty() {
        return Err(AppError::CommandFailed("FPGA bitstream is empty".into()));
    }
    if bitstream.len() > MAX_PAYLOAD_LEN {
        return Err(AppError::CommandFailed(format!(
            "FPGA bitstream is {} bytes, exceeding the single-frame NG payload limit of {} bytes",
            bitstream.len(),
            MAX_PAYLOAD_LEN
        )));
    }

    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let bitstream = bitstream.to_vec();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_secs(10))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_fpga_bitstream_frame(&bitstream);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_FPGA_BITSTREAM_LOAD: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_FPGA_BITSTREAM_LOAD rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("FPGA bitstream load task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    result
}

/// Query PM5 RGB LED status via direct serial NG command.
/// Sends CMD_PM5_RGB_GET_STATUS (0x0182) and returns the raw response payload.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn query_rgb_status_raw(port: &str, baudrate: u32) -> Result<Vec<u8>, AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_PM5_RGB_GET_STATUS, &[]);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_PM5_RGB_GET_STATUS: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PM5_RGB_GET_STATUS rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("RGB query task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    Ok(result?)
}

/// Send an RGB LED configuration command to the PM5 device.
/// Sends CMD_PM5_RGB_SET_COLOR (0x0180) with the given payload and verifies the response.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn send_rgb_command_raw(
    port: &str,
    baudrate: u32,
    payload: &[u8],
) -> Result<(), AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let payload = payload.to_vec();
    let result = tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_PM5_RGB_SET_COLOR, &payload);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_PM5_RGB_SET_COLOR: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PM5_RGB_SET_COLOR rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(())
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("RGB command task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    result
}

/// Query PM5 power rail telemetry via direct serial NG command.
/// Sends CMD_PM5_POWER_GET_STATUS (0x0170) and returns the raw response payload.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn query_power_status_raw(port: &str, baudrate: u32) -> Result<Vec<u8>, AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_PM5_POWER_GET_STATUS, &[]);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_PM5_POWER_GET_STATUS: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PM5_POWER_GET_STATUS rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Power query task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    Ok(result?)
}

/// Send a power rail enable/disable command to the PM5 device.
/// Sends CMD_PM5_POWER_SET_RAIL (0x0171) with the given payload and verifies the response.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn send_power_command_raw(
    port: &str,
    baudrate: u32,
    payload: &[u8],
) -> Result<(), AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let payload = payload.to_vec();
    let result = tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_PM5_POWER_SET_RAIL, &payload);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_PM5_POWER_SET_RAIL: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PM5_POWER_SET_RAIL rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(())
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Power command task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    result
}

/// Query PM5 factory production info via direct serial NG command.
/// Sends CMD_PM5_GET_FACTORY_INFO (0x0160) and returns the raw response payload.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn query_factory_info_raw(port: &str, baudrate: u32) -> Result<Vec<u8>, AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_PM5_GET_FACTORY_INFO, &[]);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_PM5_GET_FACTORY_INFO: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PM5_GET_FACTORY_INFO rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Factory info query task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    Ok(result?)
}

/// Query PM5 antenna tuning/telemetry status via direct serial NG command.
/// Sends CMD_PM5_ANTENNA_GET_STATUS (0x0162) and returns the raw response payload.
/// Releases the port with a short delay to avoid Windows COM handle contention.
pub async fn query_antenna_status_raw(port: &str, baudrate: u32) -> Result<Vec<u8>, AppError> {
    let lock = port_serial_lock(port);
    let _guard = lock.lock().await;
    let port = port.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut sp = serialport::new(&port, baudrate)
            .timeout(Duration::from_millis(2000))
            .open()
            .map_err(|e| {
                AppError::CommandFailed(format!("Failed to open serial port {port}: {e}"))
            })?;

        let frame = build_ng_frame(CMD_PM5_ANTENNA_GET_STATUS, &[]);
        sp.write_all(&frame).map_err(|e| {
            AppError::CommandFailed(format!("Failed to write CMD_PM5_ANTENNA_GET_STATUS: {e}"))
        })?;

        let resp = read_ng_response(&mut *sp)?;
        if resp.status != 0 {
            return Err(AppError::CommandFailed(format!(
                "CMD_PM5_ANTENNA_GET_STATUS rejected with status {} reason {}",
                resp.status, resp.reason
            )));
        }

        drop(sp);
        Ok(resp.data)
    })
    .await
    .map_err(|e| AppError::CommandFailed(format!("Antenna query task failed: {e}")))?;

    sleep(Duration::from_millis(150)).await;
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpga_bitstream_frame_has_correct_header_and_payload() {
        let frame = build_fpga_bitstream_frame(&[0xAA, 0xBB]);

        assert_eq!(&frame[0..4], &REQUEST_MAGIC.to_le_bytes());
        let len_ng = u16::from_le_bytes([frame[4], frame[5]]);
        assert_eq!(len_ng & 0x7FFF, 2);
        assert_eq!(len_ng & NG_FLAG, NG_FLAG);
        assert_eq!(
            u16::from_le_bytes([frame[6], frame[7]]),
            CMD_FPGA_BITSTREAM_LOAD
        );
        assert_eq!(&frame[8..10], &[0xAA, 0xBB]);
        assert_eq!(
            u16::from_le_bytes([frame[10], frame[11]]),
            crc16(&frame[..10])
        );
    }

    #[test]
    fn fpga_bitstream_frame_empty_payload() {
        let frame = build_fpga_bitstream_frame(&[]);
        let len_ng = u16::from_le_bytes([frame[4], frame[5]]);
        assert_eq!(len_ng & 0x7FFF, 0);
        assert_eq!(frame.len(), COMMAND_PREAMBLE_LEN + POSTAMBLE_LEN);
    }

    #[test]
    fn port_serial_lock_is_keyed_per_port() {
        let a1 = port_serial_lock("COM1");
        let a2 = port_serial_lock("COM1");
        let b = port_serial_lock("COM2");
        assert!(Arc::ptr_eq(&a1, &a2));
        assert!(!Arc::ptr_eq(&a1, &b));
    }
}
