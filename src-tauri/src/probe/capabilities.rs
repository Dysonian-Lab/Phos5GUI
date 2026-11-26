use serde::Serialize;

/// Parsed `capabilities_t` fields understood by this release.
///
/// Only the PM5-relevant fields are decoded; anything beyond the known layout
/// is ignored so a newer device firmware can still be probed without failing.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CapabilitiesStruct {
    pub version: u8,
    pub is_pm5: bool,
    pub is_pm5_std_ant: bool,
    pub max_cmd_data_size: u16,
    pub em_size: u16,
    pub compiled_with_bwm: bool,
    pub compiled_with_cep: bool,
    pub hw_available_fpga_flash: bool,
    pub hw_available_i2c_eeprom: bool,
}

impl CapabilitiesStruct {
    /// Attempt to decode `capabilities_t` from a raw capabilities response payload.
    ///
    /// `data[0]` is the capabilities structure version. The remaining fields are
    /// read defensively: if the payload is shorter than expected the missing
    /// fields fall back to safe defaults rather than failing the probe.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        let version = data[0];

        // Read a u16 little-endian at `offset` when present.
        let u16_at = |offset: usize| -> u16 {
            if offset + 2 <= data.len() {
                u16::from_le_bytes([data[offset], data[offset + 1]])
            } else {
                0
            }
        };
        // Read a bool flag at `offset` when present.
        let flag_at = |offset: usize| -> bool { data.get(offset).is_some_and(|b| *b != 0) };

        Some(Self {
            version,
            is_pm5: flag_at(1),
            is_pm5_std_ant: flag_at(2),
            max_cmd_data_size: u16_at(3),
            em_size: u16_at(5),
            compiled_with_bwm: flag_at(7),
            compiled_with_cep: flag_at(8),
            hw_available_fpga_flash: flag_at(9),
            hw_available_i2c_eeprom: flag_at(10),
        })
    }
}

impl Default for CapabilitiesStruct {
    fn default() -> Self {
        Self {
            version: 0,
            is_pm5: false,
            is_pm5_std_ant: false,
            max_cmd_data_size: 512,
            em_size: 0,
            compiled_with_bwm: false,
            compiled_with_cep: false,
            hw_available_fpga_flash: false,
            hw_available_i2c_eeprom: false,
        }
    }
}

/// Parse `capabilities_t` from raw response bytes.
pub fn parse_capabilities_struct(data: &[u8]) -> Option<CapabilitiesStruct> {
    CapabilitiesStruct::from_bytes(data)
}
