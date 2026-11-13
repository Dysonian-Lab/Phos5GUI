use std::env;

use tauri_build::{try_build, Attributes};

fn main() {
    // The PM5 capabilities manifest lives at `capabilities/pm5.json` (Plan
    // Section 4.4) but is NOT a Tauri ACL capability file. Restrict Tauri's
    // capability discovery to `default.json` so `pm5.json` is not parsed as an
    // ACL capability (which would fail on the missing `identifier` field).
    const CAPABILITIES_PATTERN: &str = "./capabilities/default.json";

    if env::var("TAURI_CONFIG").is_ok() {
        try_build(
            Attributes::new()
                .app_manifest(tauri_build::AppManifest::new().commands(&[]))
                .capabilities_path_pattern(CAPABILITIES_PATTERN),
        )
        .expect("failed to run tauri-build with custom config");
    } else {
        try_build(Attributes::new().capabilities_path_pattern(CAPABILITIES_PATTERN))
            .expect("failed to run tauri-build");
    }
}