use serde::{Deserialize, Serialize};

/// RGB LED status reported by the PM5 device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RgbStatus {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub mode: u8,
    pub brightness: u8,
}

/// Power rail telemetry reported by the PM5 device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowerStatus {
    pub v33_enabled: bool,
    pub v50_enabled: bool,
    pub v33_voltage_mv: u16,
    pub v50_voltage_mv: u16,
}

/// Decode an RGB status payload.
///
/// Expected raw layout (little-endian, minimum 5 bytes):
/// - byte 0: red (0-255)
/// - byte 1: green (0-255)
/// - byte 2: blue (0-255)
/// - byte 3: mode (0 = Off, 1 = Solid, 2 = Breathing, 3 = Rainbow)
/// - byte 4: brightness (0-100)
///
/// Returns `None` if the payload is smaller than the required size.
pub fn parse_rgb_status(raw_bytes: &[u8]) -> Option<RgbStatus> {
    if raw_bytes.len() < 5 {
        return None;
    }

    Some(RgbStatus {
        red: raw_bytes[0],
        green: raw_bytes[1],
        blue: raw_bytes[2],
        mode: raw_bytes[3],
        brightness: raw_bytes[4],
    })
}

/// Decode a power rail status payload.
///
/// Expected raw layout (little-endian, minimum 6 bytes):
/// - byte 0:  v33_enabled flag (0 = disabled, non-zero = enabled)
/// - byte 1:  v50_enabled flag (0 = disabled, non-zero = enabled)
/// - bytes 2..4: v33_voltage_mv as u16 LE
/// - bytes 4..6: v50_voltage_mv as u16 LE
///
/// Returns `None` if the payload is smaller than the required size.
pub fn parse_power_status(raw_bytes: &[u8]) -> Option<PowerStatus> {
    if raw_bytes.len() < 6 {
        return None;
    }

    let v33_enabled = raw_bytes[0] != 0;
    let v50_enabled = raw_bytes[1] != 0;
    let v33_voltage_mv = u16::from_le_bytes([raw_bytes[2], raw_bytes[3]]);
    let v50_voltage_mv = u16::from_le_bytes([raw_bytes[4], raw_bytes[5]]);

    Some(PowerStatus {
        v33_enabled,
        v50_enabled,
        v33_voltage_mv,
        v50_voltage_mv,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_power_status, parse_rgb_status, PowerStatus, RgbStatus};

    #[test]
    fn test_parse_rgb_status_valid_payload() {
        // red=255, green=128, blue=64, mode=2 (breathing), brightness=75
        let raw = [0xFF, 0x80, 0x40, 0x02, 0x4B];

        let status = parse_rgb_status(&raw).expect("expected valid RGB status");
        assert_eq!(
            status,
            RgbStatus {
                red: 255,
                green: 128,
                blue: 64,
                mode: 2,
                brightness: 75,
            }
        );
    }

    #[test]
    fn test_parse_rgb_status_short_payload_returns_none() {
        assert!(parse_rgb_status(&[]).is_none());
        assert!(parse_rgb_status(&[0xFF, 0x80, 0x40]).is_none());
        assert!(parse_rgb_status(&[0u8; 4]).is_none());
    }

    #[test]
    fn test_parse_power_status_valid_payload() {
        // v33_enabled=1, v50_enabled=0,
        // v33_voltage_mv=3300 (0x0CE4), v50_voltage_mv=5000 (0x1388)
        let raw = [
            0x01, // v33 enabled
            0x00, // v50 disabled
            0xE4, 0x0C, // v33_voltage_mv LE = 3300
            0x88, 0x13, // v50_voltage_mv LE = 5000
        ];

        let status = parse_power_status(&raw).expect("expected valid power status");
        assert_eq!(
            status,
            PowerStatus {
                v33_enabled: true,
                v50_enabled: false,
                v33_voltage_mv: 3300,
                v50_voltage_mv: 5000,
            }
        );
    }

    #[test]
    fn test_parse_power_status_short_payload_returns_none() {
        assert!(parse_power_status(&[]).is_none());
        assert!(parse_power_status(&[0u8; 5]).is_none());
        assert!(parse_power_status(&[0x01, 0x00, 0xE4, 0x0C]).is_none());
    }
}
