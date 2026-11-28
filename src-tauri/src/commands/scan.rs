use std::sync::Mutex;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::ShellExt;

use crate::cards::types::{CardType, RecoveryAction};
use crate::pm3::connection::get_dumps_dir;
use crate::error::AppError;
use crate::pm3::{command_builder, connection, output_parser};
use crate::pm3::connection::HfOperationState;
use crate::state::{WizardAction, WizardMachine, WizardState};

#[derive(serde::Serialize, Clone)]
struct ScanProgressPayload {
    step: u8,
    total_steps: u8,
    step_name: String,
    percentage: u8,
}

#[tauri::command]
pub async fn scan_card(
    app: AppHandle,
    machine: State<'_, Mutex<WizardMachine>>,
    hf_state: State<'_, HfOperationState>,
) -> Result<WizardState, AppError> {
    // Get the port from current state, then transition to ScanningCard
    let port = {
        let mut m = machine
            .lock()
            .map_err(|e| AppError::CommandFailed(format!("State lock poisoned: {}", e)))?;
        let port = match &m.current {
            WizardState::DeviceConnected { port, .. } => port.clone(),
            _ => {
                return Err(AppError::InvalidTransition(
                    "Must be in DeviceConnected to scan".to_string(),
                ));
            }
        };
        m.transition(WizardAction::StartScan)?;
        port
    };

    // Bootstrap dumps directory
    let _ = get_dumps_dir();
    hf_state.abort.store(false, Ordering::SeqCst);

    // Abort helper
    let aborted = || hf_state.abort.load(Ordering::SeqCst);

    let mut found: Option<(CardType, crate::cards::types::CardData)> = None;

    // Step 1: hf 14a info (4s)
    app.emit("scan-progress", ScanProgressPayload { step: 1, total_steps: 6, step_name: "hf 14a info".into(), percentage: 23 }).ok();
    if !aborted() {
        match connection::run_command_streaming(&app, &port, "hf 14a info", 4, &hf_state, |_| {}).await {
            Ok(out) => {
                if let Some(r) = output_parser::parse_hf14a_info_output(&out) {
                    found = Some(r);
                }
            }
            Err(_) => {}
        }
    }

    // Step 2: lf search (14s)
    if found.is_none() && !aborted() {
        app.emit("scan-progress", ScanProgressPayload { step: 2, total_steps: 6, step_name: "lf search".into(), percentage: 33 }).ok();
        match connection::run_command_streaming(&app, &port, "lf search", 14, &hf_state, |_| {}).await {
            Ok(out) => {
                if let Some((ct, data)) = output_parser::parse_lf_search(&out) {
                    // A bare T55xx chipset detection is not a valid credential identity
                    if ct != CardType::T55xx {
                        found = Some((ct, data));
                    }
                }
            }
            Err(_) => {}
        }
    }

    // Step 3: hf search (14s)
    if found.is_none() && !aborted() {
        app.emit("scan-progress", ScanProgressPayload { step: 3, total_steps: 6, step_name: "hf search".into(), percentage: 53 }).ok();
        match connection::run_command_streaming(&app, &port, "hf search", 14, &hf_state, |_| {}).await {
            Ok(out) => {
                if let Some((ct, data)) = output_parser::parse_hf_search(&out) {
                    found = Some((ct, data));
                }
            }
            Err(_) => {}
        }
    }

    // Step 4: lf t55xx detect (4s)
    if found.is_none() && !aborted() {
        app.emit("scan-progress", ScanProgressPayload { step: 4, total_steps: 6, step_name: "lf t55xx detect".into(), percentage: 67 }).ok();
        match connection::run_command_streaming(&app, &port, "lf t55xx detect", 4, &hf_state, |_| {}).await {
            Ok(out) => {
                let status = output_parser::parse_t5577_detect(&out);
                if status.detected {
                    found = Some((
                        CardType::T55xx,
                        crate::cards::types::CardData {
                            uid: "T55xx".to_string(),
                            raw: String::new(),
                            decoded: std::collections::HashMap::new(),
                            subtype: Some("Blank (unprogrammed)".to_string()),
                            ..Default::default()
                        },
                    ));
                }
            }
            Err(_) => {}
        }
    }

    // Step 5: lf em 4x05 info (4s)
    if found.is_none() && !aborted() {
        app.emit("scan-progress", ScanProgressPayload { step: 5, total_steps: 6, step_name: "lf em 4x05 info".into(), percentage: 83 }).ok();
        match connection::run_command_streaming(&app, &port, "lf em 4x05 info", 4, &hf_state, |_| {}).await {
            Ok(out) => {
                if output_parser::parse_em4305_info(&out) {
                    found = Some((
                        CardType::EM4305,
                        crate::cards::types::CardData {
                            uid: "EM4305".to_string(),
                            raw: String::new(),
                            decoded: std::collections::HashMap::new(),
                            ..Default::default()
                        },
                    ));
                }
            }
            Err(_) => {}
        }
    }

    // Step 6: hf felica reader (5s)
    if found.is_none() && !aborted() {
        app.emit("scan-progress", ScanProgressPayload { step: 6, total_steps: 6, step_name: "hf felica reader".into(), percentage: 90 }).ok();
        match connection::run_command_streaming(&app, &port, "hf felica reader", 5, &hf_state, |_| {}).await {
            Ok(out) => {
                if let Some(r) = output_parser::parse_felica_reader_output(&out) {
                    found = Some(r);
                }
            }
            Err(_) => {}
        }
    }

    if aborted() {
        let mut m = machine
            .lock()
            .map_err(|e| AppError::CommandFailed(format!("State lock poisoned: {}", e)))?;
        m.transition(WizardAction::ReportError {
            message: "Scan cancelled".to_string(),
            user_message: "Scan cancelled.".to_string(),
            recoverable: true,
            recovery_action: Some(RecoveryAction::Retry),
        })?;
        return Ok(m.current.clone());
    }

    app.emit("scan-progress", ScanProgressPayload { step: 6, total_steps: 6, step_name: "complete".into(), percentage: 100 }).ok();

    match found {
        Some((mut card_type, mut card_data)) => {
            use crate::cards::types::Frequency;
            // T55xx enrichment
            let has_t55xx_block =
                card_type == CardType::T55xx || card_data.decoded.contains_key("t55xx_chipset");
            if has_t55xx_block {
                enrich_t55xx_data(&app, &port, &mut card_data).await;
            }
            // Static LF handling: serialize to dumps/<uid>.json
            if card_type.frequency() == Frequency::LF && card_type != CardType::T55xx {
                let dumps_dir = get_dumps_dir();
                let filename = format!("{}.json", card_data.uid);
                let dump_path = dumps_dir.join(&filename);
                if let Ok(json) = serde_json::to_string_pretty(&card_data) {
                    let _ = std::fs::write(&dump_path, json);
                    if dump_path.exists() {
                        card_data.dump_file = Some(dump_path.to_string_lossy().to_string());
                    }
                }
            }
            // HF enrichment
            if card_type.frequency() == Frequency::HF {
                enrich_hf_data(&app, &port, &card_type, &mut card_data).await;
                // iCLASS subtype
                if card_type == CardType::IClass {
                    if let Ok(out) = connection::run_command(&app, &port, command_builder::build_hf_iclass_info()).await {
                        let ct = output_parser::parse_hficlass_info_output(&out);
                        // store subtype
                        card_data.subtype = Some(ct.display_name().to_string());
                        // update card_type? keep IClass base
                    }
                }
                // Hitag2 / Paxton probe
                if card_type == CardType::Hitag {
                    // two-pass
                    if let Ok(out) = connection::run_command(&app, &port, "lf hitag read --ht2 -k BDF5E846").await {
                        if let Some((ct, d)) = output_parser::parse_hitag_paxton_output(&out, "BDF5E846") {
                            if ct != CardType::Hitag2 {
                                // Paxton
                                return finish_scan(&machine, ct, d);
                            }
                        }
                    }
                    if let Ok(out) = connection::run_command(&app, &port, "lf hitag read --ht2 -k 4D494B52").await {
                        if let Some((ct, d)) = output_parser::parse_hitag_paxton_output(&out, "4D494B52") {
                            if ct != CardType::Hitag2 {
                                return finish_scan(&machine, ct, d);
                            }
                        }
                    }
                    // generic Hitag2
                    card_type = CardType::Hitag2;
                }
                // UL/NTAG auto-dump if unprotected
                if card_type == CardType::MifareUltralight || card_type == CardType::NTAG {
                    if let Ok(out) = connection::run_command(&app, &port, &format!("hf mfu dump -f {}", card_data.uid)).await {
                        let dumps_dir = get_dumps_dir();
                        if let Some(path_str) = output_parser::extract_dump_file_path(&out) {
                            let p = std::path::PathBuf::from(&path_str);
                            let canonical = if p.is_absolute() { p } else { dumps_dir.join(p) };
                            if canonical.exists() {
                                card_data.dump_file = Some(canonical.to_string_lossy().to_string());
                            }
                        } else {
                            let fallback = dumps_dir.join(format!("{}.bin", card_data.uid));
                            if fallback.exists() {
                                card_data.dump_file = Some(fallback.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
            let caps = card_type.capabilities();
            card_data.can_read = caps.can_read;
            card_data.can_write = caps.can_write;
            card_data.can_emulate = caps.can_emulate;
            if card_data.recommended_blank.is_none() {
                card_data.recommended_blank = Some(caps.recommended_blank);
            }
            if !caps.can_write {
                card_data.non_cloneable_reason = caps.write_reason.clone();
            }
            finish_scan(&machine, card_type, card_data)
        }
        None => {
            let mut m = machine
                .lock()
                .map_err(|e| AppError::CommandFailed(format!("State lock poisoned: {}", e)))?;
            m.transition(WizardAction::ReportError {
                message: "No card detected".to_string(),
                user_message: "No card found. Place the card on the reader and try again."
                    .to_string(),
                recoverable: true,
                recovery_action: Some(RecoveryAction::Retry),
            })?;
            Ok(m.current.clone())
        }
    }
}

#[tauri::command]
pub async fn cancel_scan(
    hf_state: State<'_, HfOperationState>,
) -> Result<(), AppError> {
    hf_state.abort.store(true, Ordering::SeqCst);
    // kill in-flight child
    let child = {
        let mut lock = hf_state.child.lock().map_err(|e| {
            AppError::CommandFailed(format!("HF state lock poisoned: {}", e))
        })?;
        lock.take()
    };
    if let Some(c) = child {
        let _ = c.kill();
    }
    // port release delay
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    Ok(())
}

// keep existing helpers below

/// Read a detected T55xx's functional config blocks (0-7) so the card can
/// actually be cloned onto a blank.
async fn enrich_t55xx_data(
    app: &AppHandle,
    port: &str,
    card_data: &mut crate::cards::types::CardData,
) {
    use crate::pm3::command_builder::T55XX_BLOCK_KEY_PREFIX;

    const BLOCKS: std::ops::Range<u8> = 0..8;
    let mut captured = 0u32;

    for block in BLOCKS {
        let cmd = match command_builder::build_t55xx_read(block) {
            Ok(c) => c,
            Err(_) => break,
        };

        let out = match connection::run_t55xx_memory_command(app, port, &cmd).await {
            Ok(o) => o,
            Err(_) => continue,
        };

        if let Some(hex) = output_parser::parse_t55xx_read_block(&out) {
            card_data
                .decoded
                .insert(format!("{}{}", T55XX_BLOCK_KEY_PREFIX, block), hex);
            captured += 1;
        }
    }

    if captured == 0 {
        card_data.decoded.insert(
            "blocks".to_string(),
            "none (blank or unprogrammed chip)".to_string(),
        );
    } else {
        card_data
            .decoded
            .insert("blocks".to_string(), captured.to_string());
    }
}

/// Enrich HF card data with protocol-specific info commands.
async fn enrich_hf_data(
    app: &AppHandle,
    port: &str,
    card_type: &CardType,
    card_data: &mut crate::cards::types::CardData,
) {
    match card_type {
        CardType::MifareClassic1K | CardType::MifareClassic4K => {
            if !card_data.decoded.contains_key("prng") {
                if let Ok(info_output) =
                    connection::run_command(app, port, command_builder::build_hf_14a_info()).await
                {
                    let clean = output_parser::strip_ansi(&info_output);
                    if let Some(caps) =
                        regex::Regex::new(r"(?i)Prng\s+detection[\s.:]+(WEAK|HARD|STATIC)")
                            .ok()
                            .and_then(|re| re.captures(&clean))
                    {
                        card_data
                            .decoded
                            .insert("prng".to_string(), caps[1].to_uppercase());
                    }
                }
            }
            if !card_data.decoded.contains_key("magic") {
                if let Ok(mf_output) =
                    connection::run_command(app, port, command_builder::build_hf_mf_info()).await
                {
                    let clean = output_parser::strip_ansi(&mf_output);
                    if let Some(caps) = regex::Regex::new(r"(?i)(?:Magic|Gen(?:eration)?)\s*(?:capabilities)?[\s.:]*(?::[\s.]*)?(Gen\s*1[ab]?|CUID|USCUID|Gen\s*2|Gen\s*3|APDU|UFUID|GDM|Gen\s*4\s*(?:GTU|GDM)?|[Uu]ltimate)")
                        .ok()
                        .and_then(|re| re.captures(&clean))
                    {
                        card_data
                            .decoded
                            .insert("magic".to_string(), caps[1].to_string());
                    }
                }
            }
        }
        CardType::MifareUltralight | CardType::NTAG => {
            if let Ok(mfu_output) =
                connection::run_command(app, port, command_builder::build_hf_mfu_info()).await
            {
                let clean = output_parser::strip_ansi(&mfu_output);
                if let Some(caps) = regex::Regex::new(r"(?i)NTAG\s*(\d{3})")
                    .ok()
                    .and_then(|re| re.captures(&clean))
                {
                    card_data
                        .decoded
                        .insert("ntag_type".to_string(), format!("NTAG{}", &caps[1]));
                }
                if let Some(caps) =
                    regex::Regex::new(r"(?i)(?:MIFARE\s+)?Ultralight(?:\s+(EV1|C|Nano|AES))?")
                        .ok()
                        .and_then(|re| re.captures(&clean))
                {
                    if let Some(ul_variant) = caps.get(1) {
                        card_data.decoded.insert(
                            "ul_type".to_string(),
                            format!("Ultralight {}", ul_variant.as_str()),
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

/// Common finish: transition FSM to CardFound with detected card info.
fn finish_scan(
    machine: &Mutex<WizardMachine>,
    card_type: CardType,
    card_data: crate::cards::types::CardData,
) -> Result<WizardState, AppError> {
    let frequency = card_type.frequency();
    let cloneable = card_type.is_cloneable();
    let recommended_blank = card_type.recommended_blank();

    let mut m = machine
        .lock()
        .map_err(|e| AppError::CommandFailed(format!("State lock poisoned: {}", e)))?;
    m.transition(WizardAction::CardFound {
        frequency,
        card_type,
        card_data,
        cloneable,
        recommended_blank,
    })?;
    Ok(m.current.clone())
}


#[tauri::command]
pub async fn start_emulation(
    app: AppHandle,
    card_type: CardType,
    card_data: crate::cards::types::CardData,
    port: String,
    hf_state: State<'_, HfOperationState>,
) -> Result<(), AppError> {
    use crate::pm3::command_builder;
    let dumps_dir = get_dumps_dir();

    // Verify dump file existence before launching simulation
    if let Some(ref dump) = card_data.dump_file {
        let dump_path = if std::path::Path::new(dump).is_absolute() {
            std::path::PathBuf::from(dump)
        } else {
            dumps_dir.join(dump)
        };
        if !dump_path.exists() {
            return Err(AppError::CommandFailed(format!("Emulation dump file not found: {:?}", dump_path)));
        }
    }

    let cmd = match card_type {
        CardType::MifareClassic1K | CardType::MifareClassic4K | CardType::MifareClassicMini
        | CardType::MifareClassicPlus2K | CardType::MifareClassicPossible => {
            command_builder::build_mf_sim(None, Some(&card_data.uid))
        }
        CardType::MifareUltralight | CardType::MifareUltralightC | CardType::MifareUltralightEV1
        | CardType::NTAG | CardType::NTAG213 | CardType::NTAG215 | CardType::NTAG216 => {
            format!("hf mfu sim -u {}", card_data.uid)
        }
        CardType::IClass | CardType::IClassElite | CardType::IClassSE => {
            let dump = card_data.dump_file.clone().unwrap_or_default();
            command_builder::build_hf_iclass_sim(&dump)
        }
        CardType::Felica => {
            let dump = card_data.dump_file.clone().unwrap_or_default();
            command_builder::build_hf_felica_sim(&dump)
        }
        _ => {
            return Err(AppError::CommandFailed("Emulation not supported for this card type".to_string()));
        }
    };

    // Spawn PM3 with CWD = dumps_dir
    let args = ["-p", port.as_str(), "-c", cmd.as_str()];
    // Use scope command
    let child = app
        .shell()
        .command("proxmark3")
        .args(&args)
        .current_dir(&dumps_dir)
        .spawn()
        .map_err(|e| AppError::CommandFailed(format!("Failed to spawn emulation: {}", e)))?;
    // store child
    let child = child.1;
    let mut lock = hf_state.child.lock().map_err(|e| AppError::CommandFailed(format!("HF state lock poisoned: {}", e)))?;
    *lock = Some(child);
    Ok(())
}

#[tauri::command]
pub async fn stop_emulation(
    hf_state: State<'_, HfOperationState>,
) -> Result<(), AppError> {
    let child = {
        let mut lock = hf_state.child.lock().map_err(|e| AppError::CommandFailed(format!("HF state lock poisoned: {}", e)))?;
        lock.take()
    };
    if let Some(c) = child {
        let _ = c.kill();
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    Ok(())
}


#[tauri::command]
pub async fn read_lf_memory_card(
    app: AppHandle,
    card_type: CardType,
    card_data: crate::cards::types::CardData,
    port: String,
    hf_state: State<'_, HfOperationState>,
) -> Result<String, AppError> {
    use crate::cards::types::Frequency;
    let uid = card_data.uid.clone();
    let dumps_dir = get_dumps_dir();

    // Static LF bypass: never run a physical read; return serialized dump.
    if card_type.frequency() == Frequency::LF
        && card_type != CardType::T55xx
        && card_type != CardType::EM4305
        && card_type != CardType::Hitag2
        && card_type != CardType::PaxtonNet2
        && card_type != CardType::PaxtonSwitch2
    {
        if let Some(df) = &card_data.dump_file {
            return Ok(df.clone());
        }
        let fallback = dumps_dir.join(format!("{}.json", uid));
        if fallback.exists() {
            return Ok(fallback.to_string_lossy().to_string());
        }
        return Err(AppError::CommandFailed("No serialized dump found".to_string()));
    }

    match card_type {
        CardType::T55xx => {
            let cmd = format!("lf t55xx dump -f {}", uid);
            let _ = connection::run_command(&app, &port, &cmd).await?;
            let dump_file = format!("{}.bin", uid);
            let path = dumps_dir.join(&dump_file);
            if path.exists() {
                Ok(dump_file)
            } else {
                Err(AppError::CommandFailed("T55xx dump file not written".to_string()))
            }
        }
        CardType::EM4305 => {
            let cmd = format!("lf em 4x05 dump -f {}", uid);
            let _ = connection::run_command(&app, &port, &cmd).await?;
            let dump_file = format!("{}.bin", uid);
            let path = dumps_dir.join(&dump_file);
            if path.exists() {
                Ok(dump_file)
            } else {
                Err(AppError::CommandFailed("EM4305 dump file not written".to_string()))
            }
        }
        CardType::Hitag2 | CardType::PaxtonNet2 | CardType::PaxtonSwitch2 => {
            let key = match card_type {
                CardType::PaxtonNet2 => "BDF5E846",
                CardType::PaxtonSwitch2 => "4D494B52",
                _ => "BDF5E846",
            };
            let cmd = format!("lf hitag read --ht2 -k {}", key);
            let out = connection::run_command(&app, &port, &cmd).await?;
            let obj = serde_json::json!({
                "card_type": card_type.display_name(),
                "uid": uid,
                "raw_output": out,
            });
            let dump_file = format!("{}.json", uid);
            let path = dumps_dir.join(&dump_file);
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&obj).unwrap_or_default());
            if path.exists() {
                Ok(dump_file)
            } else {
                Err(AppError::CommandFailed("Hitag dump file not written".to_string()))
            }
        }
        _ => Err(AppError::CommandFailed("Not an LF memory card".to_string())),
    }
}


#[tauri::command]
pub async fn read_iclass_card(
    app: AppHandle,
    port: String,
    card_data: crate::cards::types::CardData,
) -> Result<String, AppError> {
    let dumps_dir = get_dumps_dir();
    let uid = card_data.uid.clone();
    // Automated key trial sequence per plan
    let keys = ["--ki 0", "-k AEA684A6DAB23278", "-k 2020666666668888"];
    for k in keys {
        let cmd = format!("hf iclass dump {}", k);
        if connection::run_command(&app, &port, &cmd).await.is_ok() {
            // verify dump file on disk
            for ext in ["bin", "json", "eml"] {
                let p = dumps_dir.join(format!("{}.{}", uid, ext));
                if p.exists() {
                    return Ok(p.to_string_lossy().to_string());
                }
            }
            return Ok(uid.clone());
        }
    }
    Err(AppError::CommandFailed("KeyRequired".to_string()))
}


