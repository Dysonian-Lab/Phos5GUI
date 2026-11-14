//! PM5 command builders for all NG opcodes defined in Plan Section 8.1.
//!
//! These builders construct the raw payload bytes for each PM5 command opcode.
//! The NG frame encoding (preamble + payload + CRC) is handled by
//! [`crate::probe::ng_protocol::build_ng_frame`].
//!
//! PM5 command opcode map (Section 8.1):
//!   0x0170/0x0171 — Ant Control Write/Read
//!   0x0172/0x0173 — EEPROM Factory Info Read/Write
//!   0x0176/0x0177 — QC Test IO/HW
//!   0x0178          — RGB Set
//!   0x0179–0x0184   — BWM Commands
//!   0x0180          — Powersave
//!   0x0185          — CEP Status
//!   0x0501–0x0503   — FPGA Bitstream

use serde::Serialize;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Opcode constants (Section 8.1)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub const CMD_ANT_CONTROL_WRITE: u16 = 0x0170;
pub const CMD_ANT_CONTROL_READ: u16 = 0x0171;
pub const CMD_EEPROM_FACTORY_INFO_READ: u16 = 0x0172;
pub const CMD_EEPROM_FACTORY_INFO_WRITE: u16 = 0x0173;
pub const CMD_PM5_QC_TEST_IO: u16 = 0x0176;
pub const CMD_PM5_QC_TEST_HW: u16 = 0x0177;
pub const CMD_PM5_RGB_SET: u16 = 0x0178;
pub const CMD_PM5_BWM_BASE: u16 = 0x0179;
pub const CMD_PM5_BWM_GET_BATTERY: u16 = 0x0184;
pub const CMD_PM5_POWERSAVE: u16 = 0x0180;
pub const CMD_CEP_STATUS: u16 = 0x0185;
pub const CMD_FPGA_BITSTREAM_BASE: u16 = 0x0501;
pub const CMD_FPGA_BITSTREAM_LOAD: u16 = 0x0501;
pub const CMD_FPGA_BITSTREAM_UNLOAD: u16 = 0x0502;
pub const CMD_FPGA_BITSTREAM_STATUS: u16 = 0x0503;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Ant Control (0x0170 / 0x0171)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Ant Control command sub-operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AntControlOp {
    Enable = 0x01,
    Disable = 0x00,
    Tune = 0x02,
}

/// Build payload for CMD_ANT_CONTROL_WRITE (0x0170).
/// Payload: [op: u8]
pub fn build_ant_control_write(op: AntControlOp) -> Vec<u8> {
    vec![op as u8]
}

/// Build payload for CMD_ANT_CONTROL_READ (0x0171).
/// Payload: empty (device returns antenna status).
pub fn build_ant_control_read() -> Vec<u8> {
    Vec::new()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// EEPROM Factory Info (0x0172 / 0x0173)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Build payload for CMD_EEPROM_FACTORY_INFO_READ (0x0172).
/// Payload: [offset: u16 LE][length: u16 LE]
pub fn build_eeprom_factory_info_read(offset: u16, length: u16) -> Vec<u8> {
    let mut payload = Vec::with_capacity(4);
    payload.extend_from_slice(&offset.to_le_bytes());
    payload.extend_from_slice(&length.to_le_bytes());
    payload
}

/// Build payload for CMD_EEPROM_FACTORY_INFO_WRITE (0x0173).
/// Payload: [offset: u16 LE][data: bytes...]
pub fn build_eeprom_factory_info_write(offset: u16, data: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(2 + data.len());
    payload.extend_from_slice(&offset.to_le_bytes());
    payload.extend_from_slice(data);
    payload
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// QC Test IO / HW (0x0176 / 0x0177)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// QC Test sub-operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum QcTestOp {
    LfAntenna = 0x01,
    HfAntenna = 0x02,
    FullHw = 0x03,
}

/// Build payload for CMD_PM5_QC_TEST_IO (0x0176).
/// Payload: [op: u8]
pub fn build_qc_test_io(op: QcTestOp) -> Vec<u8> {
    vec![op as u8]
}

/// Build payload for CMD_PM5_QC_TEST_HW (0x0177).
/// Payload: empty (device runs full hardware self-test).
pub fn build_qc_test_hw() -> Vec<u8> {
    Vec::new()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// RGB Set (0x0178)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// RGB LED mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RgbMode {
    Off = 0x00,
    Solid = 0x01,
    Breathing = 0x02,
    Rainbow = 0x03,
}

/// Build payload for CMD_PM5_RGB_SET (0x0178).
/// Payload: [red: u8][green: u8][blue: u8][mode: u8][brightness: u8]
pub fn build_rgb_set(red: u8, green: u8, blue: u8, mode: RgbMode, brightness: u8) -> Vec<u8> {
    vec![red, green, blue, mode as u8, brightness]
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// BWM Commands (0x0179 – 0x0184)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// BWM command sub-opcodes within the 0x0179–0x0184 block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BwmCmd {
    GetBattery = 0x01,
    GetTemp = 0x02,
    GetVoltage = 0x03,
    SetAutoOff = 0x04,
    GetAutoOff = 0x05,
}

/// Build payload for BWM command (0x0179–0x0184).
/// Payload: [subcmd: u8][args: bytes...]
pub fn build_bwm_cmd(subcmd: BwmCmd, args: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(1 + args.len());
    payload.push(subcmd as u8);
    payload.extend_from_slice(args);
    payload
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Powersave (0x0180)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Powersave state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PowersaveState {
    Disabled = 0x00,
    Enabled = 0x01,
}

/// Build payload for CMD_PM5_POWERSAVE (0x0180).
/// Payload: [state: u8]
pub fn build_powersave(state: PowersaveState) -> Vec<u8> {
    vec![state as u8]
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CEP Status (0x0185)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// CEP command operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CepOp {
    GetStatus = 0x00,
    Handshake = 0x01,
    Disconnect = 0x02,
}

/// Build payload for CMD_CEP_STATUS (0x0185).
/// Payload: [op: u8]
pub fn build_cep_status(op: CepOp) -> Vec<u8> {
    vec![op as u8]
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// FPGA Bitstream (0x0501 – 0x0503)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Build payload for CMD_FPGA_BITSTREAM_LOAD (0x0501).
/// Payload: the raw bitstream bytes (entire bitstream as frame payload).
pub fn build_fpga_bitstream_load(bitstream: &[u8]) -> Vec<u8> {
    bitstream.to_vec()
}

/// Build payload for CMD_FPGA_BITSTREAM_UNLOAD (0x0502).
/// Payload: empty.
pub fn build_fpga_bitstream_unload() -> Vec<u8> {
    Vec::new()
}

/// Build payload for CMD_FPGA_BITSTREAM_STATUS (0x0503).
/// Payload: empty.
pub fn build_fpga_bitstream_status() -> Vec<u8> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ant_control_write_encodes_op() {
        let payload = build_ant_control_write(AntControlOp::Enable);
        assert_eq!(payload, vec![0x01]);
        let payload = build_ant_control_write(AntControlOp::Disable);
        assert_eq!(payload, vec![0x00]);
    }

    #[test]
    fn ant_control_read_is_empty() {
        assert!(build_ant_control_read().is_empty());
    }

    #[test]
    fn eeprom_factory_info_read_encodes_offset_and_length() {
        let payload = build_eeprom_factory_info_read(0x0010, 0x0020);
        assert_eq!(payload, vec![0x10, 0x00, 0x20, 0x00]);
    }

    #[test]
    fn eeprom_factory_info_write_encodes_offset_and_data() {
        let payload = build_eeprom_factory_info_write(0x0005, &[0xAA, 0xBB, 0xCC]);
        assert_eq!(payload, vec![0x05, 0x00, 0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn qc_test_io_encodes_op() {
        let payload = build_qc_test_io(QcTestOp::FullHw);
        assert_eq!(payload, vec![0x03]);
    }

    #[test]
    fn qc_test_hw_is_empty() {
        assert!(build_qc_test_hw().is_empty());
    }

    #[test]
    fn rgb_set_encodes_all_fields() {
        let payload = build_rgb_set(0xFF, 0x80, 0x40, RgbMode::Breathing, 0x75);
        assert_eq!(payload, vec![0xFF, 0x80, 0x40, 0x02, 0x75]);
    }

    #[test]
    fn bwm_cmd_encodes_subcmd_and_args() {
        let payload = build_bwm_cmd(BwmCmd::GetBattery, &[]);
        assert_eq!(payload, vec![0x01]);
        let payload = build_bwm_cmd(BwmCmd::SetAutoOff, &[0x01]);
        assert_eq!(payload, vec![0x04, 0x01]);
    }

    #[test]
    fn powersave_encodes_state() {
        let payload = build_powersave(PowersaveState::Enabled);
        assert_eq!(payload, vec![0x01]);
        let payload = build_powersave(PowersaveState::Disabled);
        assert_eq!(payload, vec![0x00]);
    }

    #[test]
    fn cep_status_encodes_op() {
        let payload = build_cep_status(CepOp::Handshake);
        assert_eq!(payload, vec![0x01]);
    }

    #[test]
    fn fpga_bitstream_load_encodes_bytes() {
        let payload = build_fpga_bitstream_load(&[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn fpga_bitstream_unload_and_status_are_empty() {
        assert!(build_fpga_bitstream_unload().is_empty());
        assert!(build_fpga_bitstream_status().is_empty());
    }
}
