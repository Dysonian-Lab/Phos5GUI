//! PM5 device identity routines (Plan Section 3.2 & 8.1).
//!
//! Device detection and info extraction from probe responses and factory
//! EEPROM queries.

use serde::Serialize;

use crate::pm5::firmware::Pm5Variant;

/// PM5 device identity information.
#[derive(Debug, Clone, Serialize)]
pub struct Pm5DeviceInfo {
    pub model: String,
    pub variant: Pm5Variant,
    pub serial_number: Option<String>,
    pub batch_id: Option<String>,
    pub production_date: Option<String>,
    pub capabilities_version: u8,
    pub is_pm5: bool,
}

/// Extract device identity from a Layer 1 ProbeResult.
pub fn device_info_from_probe(
    capabilities_version: u8,
    is_pm5: bool,
    variant_hint: Option<&str>,
) -> Pm5DeviceInfo {
    let variant = match variant_hint {
        Some("bwm") => Pm5Variant::Bwm,
        Some("standard") | None => Pm5Variant::Standard,
        _ => Pm5Variant::Standard,
    };

    Pm5DeviceInfo {
        model: "PM5".to_string(),
        variant,
        serial_number: None,
        batch_id: None,
        production_date: None,
        capabilities_version,
        is_pm5,
    }
}

/// Extract device identity from factory EEPROM info payload.
///
/// Parses length-prefixed UTF-8 strings (serial, batch, date) from the raw
/// EEPROM factory info response.
pub fn device_info_from_factory_info(
    raw: &[u8],
    base: Pm5DeviceInfo,
) -> Result<Pm5DeviceInfo, String> {
    if raw.is_empty() {
        return Err("Factory info payload is empty".to_string());
    }

    let _qc_passed = raw[0] != 0;
    let mut offset: usize = 1;

    let read_string = |raw: &[u8], offset: &mut usize| -> Result<Option<String>, String> {
        let len = *raw.get(*offset).ok_or("Truncated length byte")? as usize;
        *offset += 1;
        if *offset + len > raw.len() {
            return Err("Truncated string payload".to_string());
        }
        let s = String::from_utf8(raw[*offset..*offset + len].to_vec())
            .map_err(|e| format!("Invalid UTF-8 in factory info: {e}"))?;
        *offset += len;
        Ok(Some(s))
    };

    let serial = read_string(raw, &mut offset)?;
    let batch = read_string(raw, &mut offset)?;
    let date = read_string(raw, &mut offset)?;

    Ok(Pm5DeviceInfo {
        serial_number: serial,
        batch_id: batch,
        production_date: date,
        ..base
    })
}

/// Determine if a device is PM5 based on capabilities version.
pub fn is_pm5_device(capabilities_version: u8) -> bool {
    // PM5 capabilities versions start at v11 (PM5-specific fields added).
    capabilities_version >= 11
}

/// Determine PM5 variant from capabilities features.
pub fn variant_from_features(compiled_with_bwm: bool) -> Pm5Variant {
    if compiled_with_bwm {
        Pm5Variant::Bwm
    } else {
        Pm5Variant::Standard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_info_from_probe_pm5() {
        let info = device_info_from_probe(13, true, Some("bwm"));
        assert!(info.is_pm5);
        assert_eq!(info.capabilities_version, 13);
        assert!(matches!(info.variant, Pm5Variant::Bwm));
        assert_eq!(info.model, "PM5");
    }

    #[test]
    fn device_info_from_probe_not_pm5() {
        let info = device_info_from_probe(10, false, None);
        assert!(!info.is_pm5);
        assert!(matches!(info.variant, Pm5Variant::Standard));
    }

    #[test]
    fn factory_info_parses_strings() {
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

        let base = device_info_from_probe(13, true, None);
        let info = device_info_from_factory_info(&raw, base).unwrap();
        assert_eq!(info.serial_number.as_deref(), Some("SN12345"));
        assert_eq!(info.batch_id.as_deref(), Some("BATCH-A"));
        assert_eq!(info.production_date.as_deref(), Some("2026-01-15"));
    }

    #[test]
    fn factory_info_empty_returns_err() {
        let base = device_info_from_probe(13, true, None);
        assert!(device_info_from_factory_info(&[], base).is_err());
    }

    #[test]
    fn is_pm5_device_threshold() {
        assert!(is_pm5_device(11));
        assert!(is_pm5_device(13));
        assert!(!is_pm5_device(10));
        assert!(!is_pm5_device(0));
    }

    #[test]
    fn variant_from_features_bwm() {
        assert!(matches!(variant_from_features(true), Pm5Variant::Bwm));
        assert!(matches!(variant_from_features(false), Pm5Variant::Standard));
    }
}
