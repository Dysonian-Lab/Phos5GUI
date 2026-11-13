use serde::{Deserialize, Serialize};

/// Battery telemetry reported by PM5 BWM (CMD_PM5_BWM_GET_BATTERY).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BwmBatteryStatus {
    pub voltage_mv: u16,
    pub battery_percent: u8,
    pub is_charging: bool,
}

/// Decode a PM5 BWM battery telemetry frame.
///
/// Expected raw layout (little-endian, minimum 4 bytes):
/// - byte 0..2: `voltage_mv` as u16 LE
/// - byte 2:    `battery_percent` (0..=100, clamped)
/// - byte 3:    status flags (bit 0 = charging)
///
/// Returns `None` if the payload is too short.
pub fn parse_battery_status(raw_bytes: &[u8]) -> Option<BwmBatteryStatus> {
    if raw_bytes.len() < 4 {
        return None;
    }

    let voltage_mv = u16::from_le_bytes([raw_bytes[0], raw_bytes[1]]);
    let battery_percent = raw_bytes[2].min(100);
    let flags = raw_bytes[3];
    let is_charging = (flags & 0x01) != 0;

    Some(BwmBatteryStatus {
        voltage_mv,
        battery_percent,
        is_charging,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_battery_status, BwmBatteryStatus};

    #[test]
    fn test_parse_battery_status_valid_frame() {
        // voltage 4123 mV (0x101B LE), 87%, charging bit set
        let raw = [0x1B, 0x10, 87, 0x01];
        let status = parse_battery_status(&raw).expect("expected valid battery status");
        assert_eq!(
            status,
            BwmBatteryStatus {
                voltage_mv: 4123,
                battery_percent: 87,
                is_charging: true,
            }
        );
    }

    #[test]
    fn test_parse_battery_status_not_charging_and_clamps_percent() {
        let raw = [0xE8, 0x0F, 200, 0x00]; // 4072 mV, percent clamped to 100
        let status = parse_battery_status(&raw).expect("expected valid battery status");
        assert_eq!(status.battery_percent, 100);
        assert!(!status.is_charging);
    }

    #[test]
    fn test_parse_battery_status_too_short() {
        assert!(parse_battery_status(&[0x01, 0x02, 0x03]).is_none());
    }
}
