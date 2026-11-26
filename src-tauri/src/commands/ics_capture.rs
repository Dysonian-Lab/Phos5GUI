use rfd::FileDialog;
use std::fs;
use tauri::command;

#[command]
pub fn save_ics_capture(payload_json: String, default_name: String) -> Result<String, String> {
    let path = FileDialog::new()
        .add_filter("JSON Files", &["json"])
        .set_file_name(&default_name)
        .save_file();

    match path {
        Some(p) => {
            fs::write(&p, payload_json).map_err(|e| format!("Write failed: {}", e))?;
            Ok(p.to_string_lossy().to_string())
        }
        None => Ok("cancelled".to_string()),
    }
}

#[command]
pub fn load_ics_capture() -> Result<String, String> {
    let path = FileDialog::new()
        .add_filter("JSON Files", &["json"])
        .pick_file();

    match path {
        Some(p) => {
            let content = fs::read_to_string(&p).map_err(|e| format!("Read failed: {}", e))?;
            Ok(content)
        }
        None => Ok("cancelled".to_string()),
    }
}

#[command]
pub fn export_raw_file(
    content: String,
    default_name: String,
    extension: String,
) -> Result<String, String> {
    let filter_name = format!("{} Files", extension.to_uppercase());
    let path = FileDialog::new()
        .add_filter(&filter_name, &[&extension])
        .set_file_name(&default_name)
        .save_file();

    match path {
        Some(p) => {
            fs::write(&p, content).map_err(|e| format!("Write failed: {}", e))?;
            Ok(p.to_string_lossy().to_string())
        }
        None => Ok("cancelled".to_string()),
    }
}
