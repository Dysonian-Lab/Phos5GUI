use serde::{Deserialize, Serialize};

/// CEP (Communication Expansion Port) status reported by CMD_CEP_STATUS (0x0185).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CepStatus {
    pub connected: bool,
    pub baudrate: u32,
    pub rx_count: u32,
    pub tx_count: u32,
}

/// Decode a CEP status payload.
///
/// Expected raw layout (little-endian, minimum 13 bytes):
/// - byte 0:      connected flag (0 = disconnected, non-zero = connected)
/// - bytes 1..5:  baudrate as u32 LE
/// - bytes 5..9:  rx_count as u32 LE
/// - bytes 9..13: tx_count as u32 LE
///
/// Returns `None` if the payload is smaller than the required size.
pub fn parse_cep_status(raw_bytes: &[u8]) -> Option<CepStatus> {
    if raw_bytes.len() < 13 {
        return None;
    }

    let connected = raw_bytes[0] != 0;
    let baudrate = u32::from_le_bytes([raw_bytes[1], raw_bytes[2], raw_bytes[3], raw_bytes[4]]);
    let rx_count = u32::from_le_bytes([raw_bytes[5], raw_bytes[6], raw_bytes[7], raw_bytes[8]]);
    let tx_count = u32::from_le_bytes([raw_bytes[9], raw_bytes[10], raw_bytes[11], raw_bytes[12]]);

    Some(CepStatus {
        connected,
        baudrate,
        rx_count,
        tx_count,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_cep_status, CepStatus};

    #[test]
    fn test_parse_cep_status_valid_payload() {
        // connected=1, baudrate=115200 (0x0001C200), rx_count=42, tx_count=7
        let raw = [
            0x01, // connected
            0x00, 0xC2, 0x01, 0x00, // baudrate LE
            0x2A, 0x00, 0x00, 0x00, // rx_count LE
            0x07, 0x00, 0x00, 0x00, // tx_count LE
        ];

        let status = parse_cep_status(&raw).expect("expected valid CEP status");
        assert_eq!(
            status,
            CepStatus {
                connected: true,
                baudrate: 115_200,
                rx_count: 42,
                tx_count: 7,
            }
        );
    }

    #[test]
    fn test_parse_cep_status_short_payload_returns_none() {
        assert!(parse_cep_status(&[]).is_none());
        assert!(parse_cep_status(&[0x01, 0x00, 0xC2, 0x01]).is_none());
        assert!(parse_cep_status(&[0u8; 12]).is_none());
    }
}
