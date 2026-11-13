use serde::{Deserialize, Serialize};

/// FPGA status reported by the PM5 device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FpgaStatus {
    pub bitstream_loaded: bool,
    pub fpga_version: String,
    pub clock_speed_hz: u32,
    pub pwm_low_count: u16,
}

/// Decode an FPGA status payload.
///
/// Expected raw layout (little-endian, minimum 7 bytes):
/// - byte 0:      bitstream_loaded flag (0 = not loaded, non-zero = loaded)
/// - bytes 1..5:  clock_speed_hz as u32 LE
/// - bytes 5..7:  pwm_low_count as u16 LE
/// - bytes 7..:   fpga_version as UTF-8 string (remainder of payload)
///
/// Returns `None` if the payload is smaller than the required size or if the
/// version bytes are not valid UTF-8.
pub fn parse_fpga_status(raw_bytes: &[u8]) -> Option<FpgaStatus> {
    if raw_bytes.len() < 7 {
        return None;
    }

    let bitstream_loaded = raw_bytes[0] != 0;
    let clock_speed_hz = u32::from_le_bytes([raw_bytes[1], raw_bytes[2], raw_bytes[3], raw_bytes[4]]);
    let pwm_low_count = u16::from_le_bytes([raw_bytes[5], raw_bytes[6]]);
    let fpga_version = String::from_utf8(raw_bytes[7..].to_vec()).ok()?;

    Some(FpgaStatus {
        bitstream_loaded,
        fpga_version,
        clock_speed_hz,
        pwm_low_count,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_fpga_status, FpgaStatus};

    #[test]
    fn test_parse_fpga_status_valid_payload() {
        // bitstream_loaded=1, clock_speed_hz=100_000_000 (0x05F5E100),
        // pwm_low_count=42 (0x002A), fpga_version="v2.1"
        let mut raw = vec![
            0x01, // bitstream_loaded
            0x00, 0xE1, 0xF5, 0x05, // clock_speed_hz LE
            0x2A, 0x00, // pwm_low_count LE
        ];
        raw.extend_from_slice(b"v2.1");

        let status = parse_fpga_status(&raw).expect("expected valid FPGA status");
        assert_eq!(
            status,
            FpgaStatus {
                bitstream_loaded: true,
                fpga_version: "v2.1".to_string(),
                clock_speed_hz: 100_000_000,
                pwm_low_count: 42,
            }
        );
    }

    #[test]
    fn test_parse_fpga_status_empty_version() {
        // Minimum payload: 7 bytes, empty version string
        let raw = [
            0x00, // bitstream_loaded
            0x00, 0x00, 0x00, 0x00, // clock_speed_hz = 0
            0x00, 0x00, // pwm_low_count = 0
        ];

        let status = parse_fpga_status(&raw).expect("expected valid FPGA status");
        assert_eq!(
            status,
            FpgaStatus {
                bitstream_loaded: false,
                fpga_version: String::new(),
                clock_speed_hz: 0,
                pwm_low_count: 0,
            }
        );
    }

    #[test]
    fn test_parse_fpga_status_invalid_utf8_version() {
        // Valid header but invalid UTF-8 in version bytes
        let raw = [
            0x01,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
            0xFF, 0xFE, // invalid UTF-8
        ];

        assert!(parse_fpga_status(&raw).is_none());
    }

    #[test]
    fn test_parse_fpga_status_short_payload_returns_none() {
        assert!(parse_fpga_status(&[]).is_none());
        assert!(parse_fpga_status(&[0u8; 6]).is_none());
        assert!(parse_fpga_status(&[0x01, 0x00, 0xE1, 0xF5, 0x05]).is_none());
    }
}
