use std::env;

fn main() {
    if env::var("TAURI_CONFIG").is_ok() {
        tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[]),
        ))
        .expect("failed to run tauri-build with custom config");
    } else {
        tauri_build::build();
    }
}
