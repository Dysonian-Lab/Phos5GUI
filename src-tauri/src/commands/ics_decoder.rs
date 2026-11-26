use serialport::SerialPort;
use std::io::{Read, Write};
use std::time::Duration;

#[tauri::command]
pub fn ics_list_ports() -> Result<Vec<String>, String> {
    let ports = serialport::available_ports().map_err(|e| e.to_string())?;
    Ok(ports.into_iter().map(|p| p.port_name).collect())
}

#[tauri::command]
pub fn ics_send_command(
    port: String,
    command: String,
    timeout_ms: Option<u64>,
) -> Result<String, String> {
    let mut p = serialport::new(&port, 115200)
        .timeout(Duration::from_millis(timeout_ms.unwrap_or(2000)))
        .open()
        .map_err(|e| e.to_string())?;
    p.write_all(format!("{}\r\n", command).as_bytes())
        .map_err(|e| e.to_string())?;
    p.flush().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(300));
    let mut buf = vec![0u8; 8192];
    let mut total = 0;
    loop {
        match p.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total >= buf.len() {
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(String::from_utf8_lossy(&buf[..total]).to_string())
}
