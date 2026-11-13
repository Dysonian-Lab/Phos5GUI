use serde::{Deserialize, Serialize};

/// Factory production info reported by the PM5 device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FactoryInfo {
    pub serial_number: String,
    pub batch_id: String,
    pub production_date: String,
    pub qc_passed: bool,
}

/// Antenna tuning and telemetry status reported by the PM5 device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AntennaStatus {
    pub lf_frequency_hz: u32,
    pub lf_voltage_mv: u16,
    pub hf_voltage_mv: u16,
    pub tuning_active: bool,
}

/// Decode a factory info payload.
///
/// Expected raw layout (length-prefixed UTF-8 strings):
/// - byte 0:        qc_passed flag (0 = failed, non-zero = passed)
/// - byte 1:        serial_number length (u8)
/// - bytes 2..:     serial_number UTF-8
/// - next byte:     batch_id length (u8)
/// - next bytes:    batch_id UTF-8
/// - next byte:     production_date length (u8)
/// - next bytes:    production_date UTF-8
///
/// Returns `None` if the payload is truncated or any string is not valid UTF-8.
pub fn parse_factory_info(raw_bytes: &[u8]) -> Option<FactoryInfo> {
    if raw_bytes.is_empty() {
        return None;
    }

    let qc_passed = raw_bytes[0] != 0;
    let mut offset: usize = 1;

    // Helper to read a length-prefixed UTF-8 string advancing `offset`.
    let len = *raw_bytes.get(offset)? as usize;
    offset += 1;
    if offset + len > raw_bytes.len() {
        return None;
    }
    let serial_number = String::from_utf8(raw_bytes[offset..offset + len].to_vec()).ok()?;
    offset += len;

    let len = *raw_bytes.get(offset)? as usize;
    offset += 1;
    if offset + len > raw_bytes.len() {
        return None;
    }
    let batch_id = String::from_utf8(raw_bytes[offset..offset + len].to_vec()).ok()?;
    offset += len;

    let len = *raw_bytes.get(offset)? as usize;
    offset += 1;
    if offset + len > raw_bytes.len() {
        return None;
    }
    let production_date = String::from_utf8(raw_bytes[offset..offset + len].to_vec()).ok()?;

    Some(FactoryInfo {
        serial_number,
        batch_id,
        production_date,
        qc_passed,
    })
}

/// Decode an antenna status payload.
///
/// Expected raw layout (little-endian, minimum 9 bytes):
/// - bytes 0..4:  lf_frequency_hz as u32 LE
/// - bytes 4..6:  lf_voltage_mv as u16 LE
/// - bytes 6..8:  hf_voltage_mv as u16 LE
/// - byte 8:      tuning_active flag (0 = inactive, non-zero = active)
///
/// Returns `None` if the payload is smaller than the required size.
pub fn parse_antenna_status(raw_bytes: &[u8]) -> Option<AntennaStatus> {
    if raw_bytes.len() < 9 {
        return None;
    }

    let lf_frequency_hz = u32::from_le_bytes([raw_bytes[0], raw_bytes[1], raw_bytes[2], raw_bytes[3]]);
    let lf_voltage_mv = u16::from_le_bytes([raw_bytes[4], raw_bytes[5]]);
    let hf_voltage_mv = u16::from_le_bytes([raw_bytes[6], raw_bytes[7]]);
    let tuning_active = raw_bytes[8] != 0;

    Some(AntennaStatus {
        lf_frequency_hz,
        lf_voltage_mv,
        hf_voltage_mv,
        tuning_active,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_antenna_status, parse_factory_info, AntennaStatus, FactoryInfo};

    #[test]
    fn test_parse_factory_info_valid_payload() {
        // qc_passed=1, serial="SN12345", batch="BATCH-A", date="2026-01-15"
        let serial = b"SN12345";
        let batch = b"BATCH-A";
        let date = b"2026-01-15";

        let mut raw = Vec::new();
        raw.push(0x01); // qc_passed
        raw.push(serial.len() as u8);
        raw.extend_from_slice(serial);
        raw.push(batch.len() as u8);
        raw.extend_from_slice(batch);
        raw.push(date.len() as u8);
        raw.extend_from_slice(date);

        let info = parse_factory_info(&raw).expect("expected valid factory info");
        assert_eq!(
            info,
            FactoryInfo {
                serial_number: "SN12345".to_string(),
                batch_id: "BATCH-A".to_string(),
                production_date: "2026-01-15".to_string(),
                qc_passed: true,
            }
        );
    }

    #[test]
    fn test_parse_factory_info_qc_failed() {
        // qc_passed=0, empty strings
        let raw = [0x00, 0x00, 0x00, 0x00];

        let info = parse_factory_info(&raw).expect("expected valid factory info");
        assert_eq!(
            info,
            FactoryInfo {
                serial_number: String::new(),
                batch_id: String::new(),
                production_date: String::new(),
                qc_passed: false,
            }
        );
    }

    #[test]
    fn test_parse_factory_info_invalid_utf8() {
        // valid header but invalid UTF-8 in serial
        let mut raw = Vec::new();
        raw.push(0x01);
        raw.push(2);
        raw.extend_from_slice(&[0xFF, 0xFE]);
        raw.push(0);
        raw.push(0);

        assert!(parse_factory_info(&raw).is_none());
    }

    #[test]
    fn test_parse_factory_info_short_payload_returns_none() {
        assert!(parse_factory_info(&[]).is_none());
        assert!(parse_factory_info(&[0x01]).is_none());
        assert!(parse_factory_info(&[0x01, 0x05, b'S', b'N']).is_none());
    }

    #[test]
    fn test_parse_antenna_status_valid_payload() {
        // lf_frequency_hz=135600 (0x00021210), lf_voltage_mv=3300 (0x0CE4),
        // hf_voltage_mv=5000 (0x1388), tuning_active=1
        let raw = [
            0xB0, 0x11, 0x02, 0x00, // lf_frequency_hz LE = 135600
            0xE4, 0x0C, // lf_voltage_mv LE = 3300
            0x88, 0x13, // hf_voltage_mv LE = 5000
            0x01, // tuning_active
        ];

        let status = parse_antenna_status(&raw).expect("expected valid antenna status");
        assert_eq!(
            status,
            AntennaStatus {
                lf_frequency_hz: 135_600,
                lf_voltage_mv: 3300,
                hf_voltage_mv: 5000,
                tuning_active: true,
            }
        );
    }

    #[test]
    fn test_parse_antenna_status_tuning_inactive() {
        let raw = [
            0x00, 0x00, 0x00, 0x00, // lf_frequency_hz = 0
            0x00, 0x00, // lf_voltage_mv = 0
            0x00, 0x00, // hf_voltage_mv = 0
            0x00, // tuning_active = false
        ];

        let status = parse_antenna_status(&raw).expect("expected valid antenna status");
        assert_eq!(
            status,
            AntennaStatus {
                lf_frequency_hz: 0,
                lf_voltage_mv: 0,
                hf_voltage_mv: 0,
                tuning_active: false,
            }
        );
    }

    #[test]
    fn test_parse_antenna_status_short_payload_returns_none() {
        assert!(parse_antenna_status(&[]).is_none());
        assert!(parse_antenna_status(&[0u8; 8]).is_none());
        assert!(parse_antenna_status(&[0u8; 5]).is_none());
    }
}
