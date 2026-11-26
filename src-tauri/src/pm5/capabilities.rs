use serde::{Deserialize, Serialize};

/// Result of comparing device firmware version against the client's expected version.
///
/// The frontend uses this to decide whether to block an action, warn the user,
/// or proceed normally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail")]
pub enum VersionMatchStatus {
    /// Device firmware version matches the client's expected version.
    Matched,
    /// Device firmware is older than what the client expects.
    DeviceOutdated { device_ver: u8, client_ver: u8 },
    /// Device firmware is newer than this desktop client.
    ClientOutdated { device_ver: u8, client_ver: u8 },
    /// Version could not be determined (probe failed, timeout, etc.).
    ProbeFailed { reason: String },
}

/// Expected capabilities version of the bundled/patched PM5 client.
pub const CLIENT_CAPABILITIES_VERSION: u8 = 13;

/// A single firmware variant's feature flags (Plan Section 4.4).
#[derive(Debug, Clone, Deserialize)]
pub struct VariantFeatures {
    pub compiled_with_bwm: bool,
    pub compiled_with_cep: bool,
    pub is_pm5_std_ant: bool,
    pub hw_available_fpga_flash: bool,
    pub hw_available_i2c_eeprom: bool,
}

/// One firmware variant entry in the capabilities manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct VariantManifest {
    pub firmware_path: String,
    pub features: VariantFeatures,
}

/// The capabilities manifest schema (Plan Section 4.4).
#[derive(Debug, Clone, Deserialize)]
pub struct CapabilitiesManifest {
    pub capabilities_version: u8,
    pub commit_hash: String,
    pub build_date: String,
    pub variants: std::collections::HashMap<String, VariantManifest>,
    pub patched_client: bool,
    pub min_client_version: String,
    pub max_client_version: String,
}

/// The manifest compiled into the binary from `src-tauri/capabilities/pm5.json`.
pub const PM5_MANIFEST_JSON: &str = include_str!("../../capabilities/pm5.json");

/// Parse the compiled-in capabilities manifest.
pub fn load_manifest() -> Result<CapabilitiesManifest, String> {
    serde_json::from_str(PM5_MANIFEST_JSON)
        .map_err(|e| format!("Invalid capabilities/pm5.json manifest: {e}"))
}

/// The capabilities version declared by the manifest, if it parses.
pub fn manifest_capabilities_version() -> Option<u8> {
    load_manifest().ok().map(|m| m.capabilities_version)
}

/// Validate that the manifest is internally consistent with this client.
///
/// Checks that the declared capabilities version matches
/// [`CLIENT_CAPABILITIES_VERSION`] and that both required variants exist.
pub fn validate_manifest(manifest: &CapabilitiesManifest) -> Result<(), String> {
    if manifest.capabilities_version != CLIENT_CAPABILITIES_VERSION {
        return Err(format!(
            "Manifest capabilities_version {} does not match client {}",
            manifest.capabilities_version, CLIENT_CAPABILITIES_VERSION
        ));
    }
    for variant in ["standard", "bwm"] {
        if !manifest.variants.contains_key(variant) {
            return Err(format!("Manifest is missing the '{variant}' variant"));
        }
    }
    Ok(())
}

/// Evaluate the relationship between a probed device version and the
/// client's expected version.
///
/// `device_ver` is the version reported by the device (e.g. from a
/// `capabilities_t` probe or a version command). `client_ver` is the
/// version the client is built against.
///
/// Returns `ProbeFailed` when `device_ver` is 0 (no probe result) and
/// `Matched` when both are 0 (both unknown — treat as compatible).
pub fn evaluate_capabilities(device_ver: u8, client_ver: u8) -> VersionMatchStatus {
    if device_ver == 0 && client_ver == 0 {
        return VersionMatchStatus::Matched;
    }
    if device_ver == 0 {
        return VersionMatchStatus::ProbeFailed {
            reason: "device version is 0 (probe returned no version)".to_string(),
        };
    }
    if client_ver == 0 {
        return VersionMatchStatus::ProbeFailed {
            reason: "client version is 0 (client built without version)".to_string(),
        };
    }
    if device_ver < client_ver {
        VersionMatchStatus::DeviceOutdated {
            device_ver,
            client_ver,
        }
    } else if device_ver > client_ver {
        VersionMatchStatus::ClientOutdated {
            device_ver,
            client_ver,
        }
    } else {
        VersionMatchStatus::Matched
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_capabilities, load_manifest, manifest_capabilities_version, validate_manifest,
        VersionMatchStatus,
    };
    use serde_json::json;

    #[test]
    fn test_exact_version_match() {
        assert_eq!(evaluate_capabilities(13, 13), VersionMatchStatus::Matched);
    }

    #[test]
    fn test_device_outdated_scenarios() {
        assert_eq!(
            evaluate_capabilities(12, 13),
            VersionMatchStatus::DeviceOutdated {
                device_ver: 12,
                client_ver: 13
            }
        );
        assert_eq!(
            evaluate_capabilities(11, 13),
            VersionMatchStatus::DeviceOutdated {
                device_ver: 11,
                client_ver: 13
            }
        );
        assert_eq!(
            evaluate_capabilities(10, 13),
            VersionMatchStatus::DeviceOutdated {
                device_ver: 10,
                client_ver: 13
            }
        );
    }

    #[test]
    fn test_client_outdated_scenarios() {
        assert_eq!(
            evaluate_capabilities(14, 13),
            VersionMatchStatus::ClientOutdated {
                device_ver: 14,
                client_ver: 13
            }
        );
        assert_eq!(
            evaluate_capabilities(15, 13),
            VersionMatchStatus::ClientOutdated {
                device_ver: 15,
                client_ver: 13
            }
        );
    }

    #[test]
    fn test_serde_tagged_representation() {
        let matched = serde_json::to_value(VersionMatchStatus::Matched).unwrap();
        assert_eq!(matched, json!({ "status": "Matched" }));

        let device_outdated = serde_json::to_value(VersionMatchStatus::DeviceOutdated {
            device_ver: 11,
            client_ver: 13,
        })
        .unwrap();
        assert_eq!(
            device_outdated,
            json!({
                "status": "DeviceOutdated",
                "detail": { "device_ver": 11, "client_ver": 13 }
            })
        );

        let client_outdated = serde_json::to_value(VersionMatchStatus::ClientOutdated {
            device_ver: 14,
            client_ver: 13,
        })
        .unwrap();
        assert_eq!(
            client_outdated,
            json!({
                "status": "ClientOutdated",
                "detail": { "device_ver": 14, "client_ver": 13 }
            })
        );

        let probe_failed = serde_json::to_value(VersionMatchStatus::ProbeFailed {
            reason: "timeout".to_string(),
        })
        .unwrap();
        assert_eq!(
            probe_failed,
            json!({
                "status": "ProbeFailed",
                "detail": { "reason": "timeout" }
            })
        );
    }

    #[test]
    fn manifest_matches_client_version_and_variants() {
        let manifest = load_manifest().expect("pm5.json should parse");
        assert_eq!(manifest.capabilities_version, 13);
        assert_eq!(manifest_capabilities_version(), Some(13));
        validate_manifest(&manifest).expect("manifest should validate against this client");

        let standard = manifest
            .variants
            .get("standard")
            .expect("standard variant present");
        assert!(!standard.features.compiled_with_bwm);
        assert!(standard.features.compiled_with_cep);
        assert!(standard.features.is_pm5_std_ant);

        let bwm = manifest.variants.get("bwm").expect("bwm variant present");
        assert!(bwm.features.compiled_with_bwm);
        assert!(bwm.features.compiled_with_cep);
    }
}
