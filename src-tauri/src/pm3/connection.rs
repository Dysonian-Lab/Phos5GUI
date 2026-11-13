use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::time::timeout;

use crate::error::AppError;
use crate::logging;
use crate::pm3::output_parser::strip_ansi;
use crate::probe::ng_protocol::{
    port_serial_lock, probe_device_capabilities, ProbeMethod, ProbeResult,
};

/// Payload emitted as `pm3-output` events for the live terminal panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pm3OutputPayload {
    pub text: String,
    pub is_error: bool,
}

/// Emit raw PM3 output to the frontend terminal panel AND write to debug log.
pub fn emit_output(app: &AppHandle, text: &str, is_error: bool) {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = app.emit(
            "pm3-output",
            Pm3OutputPayload {
                text: trimmed.to_string(),
                is_error,
            },
        );
    }
// Append to the SESSION log rather than writing a file per chunk.
    //
    // The previous implementation created a NEW numbered file for every output
    // chunk (`read_stream_with_timeout` calls this once per chunk), so a single
    // clone attempt produced dozens of unrelated `001.md`, `002.md`, ... files
    // with no record of which command produced which. There was no way for a
    // tester to know which files to send, and none of them recorded the command,
    // exit code or duration. Now everything lands in one session file via
    // `logging`, and the per-command detail (exit code, timing, verbatim raw
    // output) is recorded by `logging::command` in `execute_pm3`.
    //
    // These files contain card UIDs and may contain recovered keys, so they are
    // written next to the executable rather than a synced location.
    logging::line(&format!(
        "{}{}",
        if is_error { "[stderr] " } else { "[out]    " },
        text.trim_end()
    ));
}

/// Maximum time to wait for a PM3 subprocess to complete (30 seconds).
const PM3_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Serialises access to a single COM port across every PM3 entry point.
///
/// Only one `proxmark3.exe` may hold a COM port at a time; a second process
/// fails to open it and reports "invalid serial port". This is the SAME per-port
/// lock used by the direct serial NG queries in `probe::ng_protocol`, so a raw
/// serial query and a client spawn can never touch the same port concurrently
/// (Windows `ERROR_ACCESS_DENIED`). Keying by port also lets unrelated ports run
/// in parallel instead of serialising the whole machine.
///
/// The guard is acquired by the outermost spawn/query entry points:
/// `execute_pm3` (scope-name spawns), `run_binary_direct` (bundled sidecar),
/// `probe_port`, `probe_device_with_fallback` (Layer 2), `run_command_streaming`
/// and the interactive session. `run_binary_direct` is only reached from
/// `execute_pm3`'s sidecar attempt, which does NOT hold the guard itself, so the
/// two acquisitions never nest and cannot deadlock.
async fn acquire_port_lock(
    port: &str,
    wait: Duration,
) -> Result<tokio::sync::OwnedMutexGuard<()>, AppError> {
    let lock = port_serial_lock(port);
    match timeout(wait, lock.lock_owned()).await {
        Ok(guard) => Ok(guard),
        Err(_) => Err(AppError::CommandFailed(
            "Another PM3 operation is still using this port. Wait for it to finish, then try again.".into(),
        )),
    }
}

/// Timeout for the LF/HF scan sweeps.
///
/// These are full-spectrum searches, not quick tag probes. Measured on the
/// rdv4 with a T5577 and a legacy iCLASS card:
///   lf search  ~9-10.5s   (exit -10 on a blank T5577, after printing the chipset)
///   hf search  ~10.8s     (exit 0 on a legacy iCLASS card)
///
/// The 8s `DETECT_COMMAND_TIMEOUT` used to be applied here, which was far too
/// short: both searches were killed mid-run, the aborted child kept COM24 open,
/// and the following search failed with "invalid serial port". That surfaced to
/// the user as "No card found" for a card that was on the antenna. 30s restores
/// the pre-regression behaviour.
const SEARCH_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Shorter timeout for tag-detection commands (`hf 14b info`, `hf 15c info`, etc.).
/// PM3 hangs when no tag of the requested type is on the reader — a 30s wait
/// per panel click is unacceptable. These are single-protocol probes rather than
/// full sweeps; measured at 1.9-3.8s (emrtd 2.1, 15 2.1, 14b 2.7, iclass 3.4,
/// felica 3.8, legic 1.9). 8s is generous.
const DETECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

/// Timeout for the long-running brute-force / dictionary-attack commands.
///
/// These are NOT quick panel probes. `lf t55xx chk` measured 26.8s on this
/// hardware -- it loads ~123 passwords from t55xx_default_pwds.dic and tests
/// each one against the tag ("[+] time in check pwd 26 seconds"). That is
/// uncomfortably close to `PM3_COMMAND_TIMEOUT` (30s): on a slower run it hit
/// the cap, the PM3 child was killed mid-brute-force, and because the child
/// owns COM24 the NEXT command failed with "invalid serial port", which the
/// user saw as the Chk-pwds button reporting a device error.
///
/// `lf t55xx bruteforce`, `hf 15 bruteforce`, `hf mf bruteforce` and
/// `hf mfu desbrute` are the same class of operation and can run for many
/// minutes on a real attack. 15 minutes leaves generous headroom over the
/// observed 27s while still bounding the worst case.
const LONG_COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Per-port probe timeout during device detection — much shorter than the
/// normal command timeout so scanning 40 COM ports doesn't take 20 minutes.
/// A real PM3 responds to `hw version` in well under 2 seconds on a live port.
const DETECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Returns the ordered list of Tauri shell scope names to try when spawning the
/// PM3 binary. The first entry (`"proxmark3"`) resolves via PATH; subsequent
/// entries are platform-specific absolute paths registered in the shell scope.
/// This lets users who installed PM3 outside of PATH (common on Windows) still
/// use the app without manually editing their system PATH.
fn pm3_scope_names() -> Vec<&'static str> {
    let mut names = vec!["proxmark3"];

    if cfg!(target_os = "windows") {
        names.push("proxmark3-win-c");
        names.push("proxmark3-win-progfiles");
    } else if cfg!(target_os = "macos") {
        names.push("proxmark3-mac-local");
        names.push("proxmark3-mac-brew");
    } else {
        // Linux and other unix-like
        names.push("proxmark3-linux-local");
        names.push("proxmark3-linux-usr");
    }

    names
}

/// Validates that a port string matches expected serial port patterns.
/// Accepts COM1-COM256+ (Windows), /dev/ttyACM0-99, /dev/ttyUSB0-99 (Linux),
/// and /dev/tty.usbmodem* (macOS).
static PORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(COM[1-9]\d*|/dev/tty(ACM|USB)\d{1,2}|/dev/tty\.usbmodem\w+)$")
        .expect("bad port regex")
});

/// Regex to extract device and client capability versions from the mismatch error.
/// Matches: "Capabilities structure version sent by Proxmark3 is not the one expected by this client! (v13 != v11)"
/// Group 1 = device version, Group 2 = client version.
static CAPABILITIES_MISMATCH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Capabilities structure version sent by Proxmark3 is not the one expected by this client! \(v(\d+) != v(\d+)\)")
        .expect("bad capabilities mismatch regex")
});

/// Internal PM3 execution that does NOT emit to the frontend.
/// Handles: port validation, command sanitization, sidecar fallback, PATH lookup,
/// process spawn, output collection, ANSI stripping, and timeout.
/// Returns the cleaned output string on success.
///
/// `wait` controls how long to wait for the subprocess. Pass
/// `PM3_COMMAND_TIMEOUT` (30s) for normal commands and `DETECT_COMMAND_TIMEOUT`
/// (8s) for tag-detection commands where PM3 hangs when no tag is present.
/// `tolerate_nonzero` controls what happens when PM3 exits with a non-zero code.
/// PM3 returns -2/-7/-10 for "no tag found" *even when it did detect something it
/// could not fully classify*. Concretely, `lf search` on a T5577 prints
/// `[-] No known 125/134 kHz tags found!` then `[+] Chipset... T55xx`, then exits
/// -10. Callers that can interpret partial detections (the scan wizard) set this
/// to true so the output survives and the parser decides. Everyone else keeps the
/// strict error path.
/// True for a `lf t55xx` subcommand that touches tag memory (`read`, `write`,
/// `dump`) and therefore requires the auto-detect prelude.
fn is_t55xx_memory_cmd(cmd: &str) -> bool {
    let mut parts = cmd.trim().split_whitespace();
    parts.next() == Some("lf")
        && parts.next() == Some("t55xx")
        && matches!(parts.next(), Some("read" | "write" | "dump"))
}

/// Guard against command-chaining injection, with one deliberate exception.
///
/// `proxmark3 -c` treats `;` as a command delimiter, so rejecting `;`
/// outright is the right default and must stay that way: it is what stops
/// unvalidated input from running a second command.
///
/// The exception is `lf t55xx detect; <lf t55xx read|write|dump>`, which is not
/// a convenience but a hard requirement of the client. Verified on hardware
/// 2026-10-01, same card on COM19:
///
///   lf t55xx read -b 0                  -> exit -16 (EWRONGANSWER), empty table
///   lf t55xx detect; lf t55xx read -b 0 -> exit 0, `00 | 000880E0`
///   lf t55xx dump                        -> exit 0, empty tables (SILENT!)
///   lf t55xx detect; lf t55xx dump      -> exit 0, all 8 blocks read
///
/// `lf t55xx detect` is what auto-detects modulation, bit rate, offset and the
/// sequence-terminator flag, and that state exists only inside the running
/// client process. Phosphor spawns a fresh `proxmark3.exe` per command, so that
/// state was destroyed between calls: every T55xx read returned an empty table
/// and the app reported the card as "blank". Matching the modulation by hand
/// (`lf t55xx config --ASK --rate 32 -o 33 --st -c <block0>`) does NOT fix it
/// -- the read still returned -16. Only the same-process chain works.
///
/// The allowance is deliberately narrow: exactly two segments, the first pinned
/// to the literal `lf t55xx detect`, the second restricted to the three
/// memory-reading verbs. Any other `;` is still rejected.
fn is_safe_pm3_command_chain(cmd: &str) -> bool {
    if cmd.contains('\n') || cmd.contains('\r') {
        return false;
    }
    if !cmd.contains(';') {
        return true;
    }
    let segments: Vec<&str> = cmd.split(';').map(str::trim).collect();
    segments.len() == 2
        && segments[0] == "lf t55xx detect"
        && is_t55xx_memory_cmd(segments[1])
}

/// Run a `lf t55xx` memory command with the `lf t55xx detect` prelude that the
/// client requires.
///
/// Use this INSTEAD of `run_command` for every `lf t55xx read` / `write` /
/// `dump`. Calling them bare returns an empty table (or -16) even when the card
/// reads perfectly well, which is what made every T5577 look blank.
///
/// The returned output contains the `detect` preamble followed by the real
/// command's output. Parsers select on the result table, so they are unaffected.
pub async fn run_t55xx_memory_command(
    app: &AppHandle,
    port: &str,
    cmd: &str,
) -> Result<String, AppError> {
    if !is_t55xx_memory_cmd(cmd) {
        return Err(AppError::CommandFailed(format!(
            "Not a T55xx memory command: {}",
            cmd
        )));
    }
    // Safe: both halves are validated above, and `is_safe_pm3_command_chain`
    // permits precisely this shape.
    let chained = format!("lf t55xx detect; {}", cmd.trim());
    emit_output(app, &format!("pm3 --> {}", chained), false);
    match execute_pm3(app, port, &chained, PM3_COMMAND_TIMEOUT, true).await {
        Ok(output) => {
            emit_output(app, &output, false);
            Ok(output)
        }
        Err(e) => {
            emit_output(app, &e.to_string(), true);
            Err(e)
        }
    }
}

async fn execute_pm3(
    app: &AppHandle,
    port: &str,
    cmd: &str,
    wait: Duration,
    tolerate_nonzero: bool,
) -> Result<String, AppError> {
    // Validate port format to prevent command injection via subprocess args
    if !PORT_RE.is_match(port) {
        return Err(AppError::CommandFailed(format!("Invalid port: {}", port)));
    }

// Reject command strings containing PM3 command separators or newlines.
    // The PM3 CLI's `-c` flag treats `;` as a delimiter, so a crafted value
    // like "AA;lf t55xx wipe" would execute two commands. Block this at the
    // chokepoint so no caller can accidentally pass through unsanitised input.
    //
    // The single exception is `lf t55xx detect; <memory cmd>` -- see
    // `is_safe_pm3_command_chain`. Arbitrary chaining stays blocked.
    if !is_safe_pm3_command_chain(cmd) {
        return Err(AppError::CommandFailed(
            "Invalid characters in command".into(),
        ));
    }
    if port.contains(';') || port.contains('\n') || port.contains('\r') {
        return Err(AppError::CommandFailed(
            "Invalid characters in command".into(),
        ));
    }


    // One PM3 process at a time per port. The sidecar path locks per-port inside
    // `run_binary_direct`; the scope-name spawns below lock per-port around each
    // spawn. Neither nests, so the shared `port_serial_lock` cannot deadlock.
    // Start the clock BEFORE the binary lookup so the logged duration includes
    // spawn overhead. Without this a "slow" command and a "hung" command look
    // identical in the log.
    let started = std::time::Instant::now();

    // 1) Try bundled sidecar binary first (available in production builds).
    //    In dev mode the sidecar won't exist, so this silently falls through.
    match try_sidecar_silent(app, port, cmd, wait).await {
        Ok(output) => return Ok(output),
        // A timeout means a client DID start and had the serial port open.
        // Falling through on that error would immediately launch a second
        // proxmark3 against the same port, which is precisely how the
        // "invalid serial port" cascade began. Propagate it instead.
        Err(e @ AppError::Timeout(_)) => return Err(e),
        // Anything else means we never got a binary running, so trying the next
        // candidate location is safe.
        Err(_) => { /* binary not available here -- fall through to PATH/scope lookup */ }
    }

    // 2) Fall back to PATH-based lookup, then common install locations.
    // Each scope name maps to a binary path registered in capabilities/default.json.
    let scope_names = pm3_scope_names();
    let mut first_spawn_error: Option<AppError> = None;

    for scope_name in &scope_names {
        // Serialise this port with direct serial queries and the sidecar path.
        // Held across the spawn so a concurrent command cannot open the same COM
        // port mid-spawn.
        let _port_guard = acquire_port_lock(port, wait).await?;

        let output_future = app
            .shell()
            .command(scope_name)
            .args(["-p", port, "-f", "-c", cmd])
            .output();

        // Note: When the timeout fires and the future is dropped, Tauri's shell plugin
        // handles cleanup of the child process. The `tokio::time::timeout` wrapper
        // ensures we don't wait forever, and the Tauri runtime drops the child on cancel.
        let output = match timeout(wait, output_future).await {
            Err(_) => {
                return Err(AppError::Timeout(format!(
                    "PM3 command timed out after {}s: {}",
                    wait.as_secs(),
                    cmd
                )));
            }
            Ok(Err(e)) => {
                // Spawn failed -- binary not found at this path. Record the error
                // from the first attempt (PATH lookup) and try the next location.
                if first_spawn_error.is_none() {
                    first_spawn_error = Some(AppError::CommandFailed(format!(
                        "Failed to spawn proxmark3: {}",
                        e
                    )));
                }
                continue;
            }
            Ok(Ok(output)) => output,
        };

// Binary was found and executed -- process the result immediately.
        // No further fallback attempts needed regardless of exit code.
        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Record the full exchange: command, port, timing, exit code and the
        // client's raw output verbatim. `interpret_pm3_result` decides what the
        // user is told; logging what actually came back is what makes a bug
        // report diagnosable without a follow-up question.
        logging::command(
            port,
            cmd,
            started.elapsed().as_millis(),
            Some(code),
            &format!("{}{}", stdout, stderr),
            Some(&format!("binary: {} (scope lookup)", scope_name)),
        );

        return interpret_pm3_result(cmd, code, &stdout, &stderr, tolerate_nonzero);
    }

    // All scope names exhausted -- return the first spawn error (from PATH lookup)
    // so the error message is the most user-recognizable one.
    Err(first_spawn_error.unwrap_or_else(|| {
        AppError::CommandFailed("Failed to spawn proxmark3: binary not found".into())
    }))
}

/// Map PM3 exit codes to friendly messages.
///
/// Every arm below is transcribed from the `#define PM3_E*` block in
/// `include/pm3_cmd.h` (v4.23346) rather than from guesswork. An earlier
/// version of this table mislabelled five codes, which was actively harmful:
/// `-4` is `PM3_ETIMEOUT` but was reported as "Invalid argument", so a genuine
/// timeout told the user their command was malformed.
///
/// Note `-10` is `PM3_ESOFT` and is the code `hf search` actually returns
/// when nothing is on the reader; it gets bespoke handling below.
fn friendly_exit_code(code: i32, cmd: &str) -> String {
    match code {
        -2 => format!("Invalid argument to the command: {}. Check the syntax.", cmd),
        -3 => format!("Command not supported by this device: {}", cmd),
        -4 => format!("Timed out waiting for the card while running: {}", cmd),
        -5 => format!("PM3 aborted the operation before completing while running: {}. The tag did not respond.", cmd),
        -7 => format!("RF transmission error while running: {}. Move the card closer to the antenna.", cmd),
        -10 => {
            // PM3 returns -10 ("Failed to identify tagtype") for a range of
            // conditions. For dump/emulate commands it usually means no key
            // file or no card on the reader — give the user something actionable.
            if cmd.contains("dump") || cmd.contains("efill") || cmd.contains("esave")
                || cmd.contains("sim") || cmd.contains("elock")
            {
                format!(
                    "No key file or no card on reader running: {}. Run 'hf mf autopwn' first to recover keys, or place the card on the reader.",
                    cmd
                )
            } else {
                format!("No tag found running: {}", cmd)
            }
        }
        251 => format!("PM3 timed out running: {}", cmd),
        // Codes below come from the `#define PM3_E*` list in `pm3_cmd.h`.
        -1 => format!("Undefined error running: {}", cmd),
        -6 => format!("Command not implemented in this client running: {}", cmd),
        -8 => format!("Communication error with the device running: {}", cmd),
        -9 => format!("Internal data flow error running: {}", cmd),
        -11 => format!("Flash error running: {}", cmd),
        -12 => format!("Out of memory running: {}", cmd),
        -13 => format!("File error (missing or unreadable) running: {}", cmd),
        // ENOTTY is the code behind the "invalid serial port" cascade the user
        // hit repeatedly, so it gets a diagnostic rather than a shrug.
        -14 => format!("Serial port problem while running: {}. The port may be held by another process.", cmd),
        -15 => format!("Client initialisation failed while running: {}", cmd),
        -16 => format!("Wrong answer from the card running: {}", cmd),
        -17 => format!("Out of bounds error running: {}", cmd),
        -18 => format!("Card exchange error running: {}", cmd),
        -19 => format!("Failed to encode the APDU while running: {}", cmd),
        -20 => format!("APDU exchange failed running: {}. Is a card of the expected type on the reader?", cmd),
        -21 => format!("PM3 command failed running: {}", cmd),
        -22 => format!("Partial result only running: {}", cmd),
        -23 => format!("Card left the field (tear-off) running: {}", cmd),
        -24 => format!("CRC error running: {}", cmd),
        -25 => format!("Static nonce detected running: {}", cmd),
        -26 => format!("No PACS session running: {}", cmd),
        -27 => format!("Invalid length running: {}", cmd),
        -28 => format!("No key available running: {}", cmd),
        -29 => format!("Cryptographic error running: {}", cmd),
        -30 => format!("No such file running: {}", cmd),
        -98 => format!("No data available running: {}", cmd),
        -99 => format!("Fatal error running: {}", cmd),
        -100 => format!("PM3 exited (PM3_SQUIT) while running: {}", cmd),
        -128 => format!("PM3 reserved error running: {}", cmd),
        _ => format!("Exit code {}: {}", code, cmd),
    }
}

/// True for commands that interrogate a tag and therefore can only report
/// "nothing here" rather than failing.
///
/// Deliberately limited to the `hf` and `lf` command groups. Device-level
/// commands such as `hw version` are excluded because a silent success is
/// normal for some of them, and turning that into an error would be wrong.
///
/// The exclusion list matters as much as the inclusion rule. Several `hf`/`lf`
/// subcommands are host-side or emulator-memory operations that never touch
/// the antenna, so their quiet output is a normal result and must not be
/// reinterpreted as "no tag". `hf mf eclr` legitimately prints nothing at all.
fn is_card_dependent(cmd: &str) -> bool {
    let c = cmd.trim_start();
    if !(c.starts_with("hf ") || c.starts_with("lf ") || c == "hf" || c == "lf") {
        return false;
    }
    // Emulator-memory and configuration commands: these talk to the client or
    // to the emulator's RAM, not to a card. Plain prefix matching is used here
    // because every member of each family is meant to be excluded -- a
    // space-boundary check would let `hf mf eclr` through, since "e" is a
    // subcommand prefix rather than a complete word.
    const NOT_CARD_DEPENDENT: &[&str] = &[
        "hf mf e",   // hf mf e* -- emulator memory: eclr/eload/esave/egetblk/...
        "hf mf gen", // hf mf gen -- generate a UID, purely host-side
        "lf t55xx config",
        "lf t55xx set",
    ];
    !NOT_CARD_DEPENDENT
        .iter()
        .any(|bad| c.starts_with(bad))
}

/// True when PM3's stdout is nothing but the per-invocation connection banner.
///
/// This whitelist is transcribed from REAL captured output, not written from
/// memory. The authoritative sample is
/// `Phosphor2.2GUI\logs\1249.md`, captured from `hf 14a info` with no card on
/// the reader (exit code 0).
///
/// The `-f` line matters and was missed once already. Phosphor always passes
/// `-f` to PM3, which makes the client emit
/// `PrintAndLogEx(INFO, "Output will be flushed after every print.\n")`
/// (client/src/proxmark3.c) on every single invocation. A previous version of
/// this function omitted that line, so it returned false against real output
/// and the "no tag" feature never fired -- while its unit test still passed,
/// because the test used a hand-written banner the client never emits. Any
/// change here MUST be re-validated against a real capture.
fn is_banner_only(output: &str) -> bool {
    output.lines().all(|line| {
        let clean = strip_ansi(line);
        let t = clean.trim();
        t.is_empty()
            || t.starts_with("[=] Session log")
            || t.starts_with("[=] Output will be flushed")
            || t.starts_with("[+] loaded `")
            || t.starts_with("[+] execute command from commandline:")
            || t.starts_with("[+] Using UART port")
            || t.starts_with("[+] Communicating with PM3")
            || t.starts_with("[+] Max frame size")
            || t.starts_with("[+] Emulator memory")
            || t.starts_with("[usb|script] pm3 -->")
    })
}

/// First failure diagnostic line in PM3 output, if any.
///
/// PM3 routes every `[!!]` line to **stderr** (client/src/ui.c), never stdout.
/// Callers that read only stdout therefore see nothing at all when a command
/// fails, which is how "an error occurred" used to render as a blank success.
///
/// `[!]` is `PrintAndLogEx(FAILED, ...)` and is accepted as a fallback because
/// readers use it for the matching "nothing found" line -- `hf 14b info`
/// prints `[!] no ISO 14443-B tag found` on the way out.
fn pm3_error_line(output: &str) -> Option<String> {
    output.lines().map(strip_ansi).find_map(|line| {
        let t = line.trim();
        ["[!!]", "[!]"]
            .into_iter()
            .find(|tag| t.starts_with(tag) && t.len() > tag.len())
            .map(|tag| t[tag.len()..].trim().to_string())
    })
}

/// Parse a capabilities version mismatch from PM3 stderr output.
///
/// Matches the error:
/// "Capabilities structure version sent by Proxmark3 is not the one expected by this client! (v13 != v11)"
///
/// Returns (device_version, client_version) as u8 values.
fn parse_capabilities_mismatch(stderr: &str) -> Option<(u8, u8)> {
    CAPABILITIES_MISMATCH_RE.captures(stderr).map(|caps| {
        let device_ver = caps[1].parse::<u8>().unwrap_or(0);
        let client_ver = caps[2].parse::<u8>().unwrap_or(0);
        (device_ver, client_ver)
    })
}

/// Interpret a completed PM3 invocation.
///
/// This is the SINGLE source of truth for turning an exit code plus two output
/// streams into a `Result`, and every spawn path must route through it:
/// `execute_pm3`, `run_binary_direct` (both OS branches) and
/// `try_sidecar_silent`.
///
/// That centralisation is not stylistic. `try_sidecar_silent` resolves the
/// binary from the executable's own directory first, so in a released build
/// nearly every command reaches `run_binary_direct` and never touches
/// `execute_pm3`. When these checks lived only in `execute_pm3`, the
/// "no tag detected" and help-menu checks silently never ran in the shipped
/// app. Do not re-inline any of this at a call site.
fn interpret_pm3_result(
    cmd: &str,
    code: i32,
    stdout: &str,
    stderr: &str,
    tolerate_nonzero: bool,
) -> Result<String, AppError> {
    let cleaned_stdout = strip_ansi(stdout);
    let cleaned_stderr = strip_ansi(stderr);

    // What the user should see. PM3 puts results on stdout and diagnostics on
    // stderr, so fall back to stderr rather than returning a blank success.
    let visible = if cleaned_stdout.trim().is_empty() {
        cleaned_stderr.clone()
    } else {
        cleaned_stdout.clone()
    };

    // PM3 reports an unrecognised command with exit code 0 and prints either an
    // argparse error or the whole group help menu. Treating that as success is
    // what let `lf t55xx set config` and `hf mf ejectblk` ship unnoticed: the
    // app rendered help text as though it were a result.
    if let Some(reason) = rejected_command(&visible) {
        return Err(AppError::CommandFailed(format!(
            "PM3 rejected the command ({})",
            reason
        )));
    }
    if let Some(reason) = device_failure(&visible) {
        return Err(AppError::CommandFailed(reason));
    }

    if code == 0 {
        // PM3 signals "nothing found" inconsistently. `hf search` exits -10,
        // but `hf 14a info`, `hf mf info` and `hf mfu info` exit 0 having
        // printed only the connection banner. Those reached the UI as a blank,
        // success-looking result, which is indistinguishable from a broken
        // reader -- it made a card sitting outside the coil look like dead
        // hardware.
        if is_card_dependent(cmd) && is_banner_only(&cleaned_stdout) {
            // Prefer PM3's own explanation if it gave one on stderr.
            if let Some(reason) = pm3_error_line(&cleaned_stderr) {
                return Err(AppError::CommandFailed(reason));
            }
            return Err(AppError::CommandFailed(format!(
                "No tag detected running: {}. Place the card on the reader and try again.",
                cmd
            )));
        }
        return Ok(visible);
    }

    // Non-zero from here on.
    if let Some(reason) = device_failure(&visible) {
        return Err(AppError::CommandFailed(reason));
    }
    // PM3 exits non-zero for "found something but could not classify it" as well
    // as for "found nothing". `lf search` on an unconfigured T5577 prints
    // "[+] Chipset... T55xx" and then exits -10, so discarding stdout here loses
    // a real detection. Only the search callers opt in.
    if tolerate_nonzero && !visible.trim().is_empty() {
        return Ok(visible);
    }
    // -5 (PM3_EOPABORTED) is PM3's generic "found nothing" sentinel, not a
    // report that anyone pressed a key. `hf 14b info` ends with
    // `return (found) ? PM3_SUCCESS : PM3_EOPABORTED;` (client/src/cmdhf14b.c)
    // and many other readers return the same thing, so surfacing the constant's
    // own name as "Command aborted by the device" told the user nothing when
    // they had simply forgotten to place a card.
    if code == -5 && is_card_dependent(cmd) {
        return Err(AppError::CommandFailed(format!(
            "No tag detected running: {}. Place the card on the reader and try again.",
            cmd
        )));
    }
    if let Some(reason) = pm3_error_line(&cleaned_stderr) {
        return Err(AppError::CommandFailed(with_detail(
            friendly_exit_code(code, cmd),
            &reason,
        )));
    }
    Err(AppError::CommandFailed(friendly_exit_code(code, cmd)))
}

/// Run a single PM3 command: spawns `proxmark3 -p {port} -f -c "{cmd}"`,
/// waits for the process to exit (with a 30-second timeout), then returns cleaned stdout.
/// If the subprocess hangs (e.g., USB cable pulled), it will be killed after the timeout.
///
/// Emits the command being run and its output to the frontend terminal panel.
///
/// **Known limitation -- subprocess cancellation on reset:**
/// This function uses `tauri_plugin_shell`'s `.output()` which internally spawns a child
/// process and collects stdout/stderr until exit. Because the child handle is owned by the
/// shell plugin's future and not exposed to callers, we cannot kill the subprocess from
/// outside (e.g., when the user presses Reset during a write operation). Refactoring to
/// `spawn()` + async stream reading would be needed for true cancellation support.
///
/// In practice this is acceptable because:
/// - T5577/EM4305 LF writes complete in under 2 seconds.
/// - The 30-second timeout (`PM3_COMMAND_TIMEOUT`) already protects against hangs.
/// - When the Tauri future is dropped (timeout or app shutdown), the shell plugin
///   cleans up the child process.
pub async fn run_command(app: &AppHandle, port: &str, cmd: &str) -> Result<String, AppError> {
    // Dictionary attacks run for minutes, not seconds. Give them their own
    // budget so the PM3 child is never killed mid-run holding COM24.
    let wait = if is_long_running(cmd) {
        LONG_COMMAND_TIMEOUT
    } else {
        PM3_COMMAND_TIMEOUT
    };
    emit_output(app, &format!("pm3 --> {}", cmd), false);
    match execute_pm3(app, port, cmd, wait, false).await {
        Ok(output) => {
            emit_output(app, &output, false);
            Ok(output)
        }
        Err(e) => {
            emit_output(app, &e.to_string(), true);
            Err(e)
        }
    }
}

/// True for the brute-force / dictionary-attack commands that legitimately run
/// for minutes. `lf t55xx chk` measured 26.8s against the 30s
/// `PM3_COMMAND_TIMEOUT`, so it was being killed mid-run and the aborted child
/// held the serial port — the user saw "invalid serial port" instead of a
/// result. Matched on a subcommand boundary so a longer dictionary is not
/// required for the check to keep working.
fn is_long_running(cmd: &str) -> bool {
    const LONG: &[&str] = &[
        "lf t55xx chk",
        "lf t55xx bruteforce",
        "lf t55xx brute",
        "lf hid bruteforce",
        "lf em bruteforce",
        "lf iclass brute",
        "hf 15 bruteforce",
        "hf 15 brute",
        "hf mf bruteforce",
        "hf mf brute",
        // `hf mf autopwn` is a dictionary attack over the standard Mifare key
        // set. It was missing here, so the Mifare View panel gave it only the
        // 30s default and killed it mid-attack -- while `commands/hf_clone.rs`
        // runs the SAME command with a 3600s budget. Two entry points, 120x
        // apart, and the panel one was guaranteed to fail.
        "hf mf autopwn",
        "hf mfu desbrute",
        "hf mfdes chk",
        "hf mfdes bruteaid",
        "hf mfdes bruteisofid",
        "hf mfdes brutedamslot",
    ];
    LONG.iter().any(|l| {
        // Require the match to end at a boundary so "lf t55xx chk" does not
        // accidentally match a longer command, and so we do not match
        // "lf t55xx chkpwds-style" prefixes by accident.
        cmd == *l || cmd.strip_prefix(l).is_some_and(|rest| rest.starts_with(' '))
    })
}

/// Run a tag-detection command with a shorter timeout (8s).
/// PM3 hangs when no tag of the requested type is on the reader — e.g.
/// `hf 14b info` with no 14B card present. A 30s wait per panel click is
/// unacceptable. Most info commands either succeed in 1-2s or fail fast.
/// Returns the cleaned output, or an AppError::Timeout if the process hangs.
pub async fn run_command_quick(
    app: &AppHandle,
    port: &str,
    cmd: &str,
) -> Result<String, AppError> {
    emit_output(app, &format!("pm3 --> {} (quick)", cmd), false);
    match execute_pm3(app, port, cmd, DETECT_COMMAND_TIMEOUT, false).await {
        Ok(output) => {
            emit_output(app, &output, false);
            Ok(output)
        }
        Err(e) => {
            let friendly = if let AppError::Timeout(_) = &e {
                // PM3 hung because no tag of the requested type is on the reader.
                // Return a clean message instead of a raw timeout.
                AppError::CommandFailed(format!(
                    "No {} tag found on the reader.",
                    detect_tag_type(cmd)
                ))
            } else {
                e
            };
            emit_output(app, &friendly.to_string(), true);
            Err(friendly)
        }
    }
}

/// Run a card-search command (`lf search`, `hf search`) and return the output
/// even when PM3 exits non-zero.
///
/// PM3 signals "found something but could not classify it" with a non-zero exit
/// code while still printing the detection. `lf search` on a T5577 emits
/// `[-] No known 125/134 kHz tags found!`, then `[+] Chipset... T55xx`, then
/// exits -10. The strict `run_command` path discards stdout on any non-zero
/// exit, so the scan wizard saw an error and reported "Scan failed" for a card
/// that was sitting on the antenna.
///
/// The output parser is the authority on whether a card was actually found, so
/// hand it everything and let it decide. Spawn failures and timeouts still
/// return `Err` — only the exit-code handling is relaxed.
pub async fn run_search_command(
    app: &AppHandle,
    port: &str,
    cmd: &str,
) -> Result<String, AppError> {
    emit_output(app, &format!("pm3 --> {}", cmd), false);
    // SEARCH_COMMAND_TIMEOUT, not DETECT_COMMAND_TIMEOUT: `lf search` and
    // `hf search` are full-spectrum sweeps that legitimately take ~11s.
    match execute_pm3(app, port, cmd, SEARCH_COMMAND_TIMEOUT, true).await {
        Ok(output) => {
            emit_output(app, &output, false);
            Ok(output)
        }
        Err(e) => {
            emit_output(app, &e.to_string(), true);
            Err(e)
        }
    }
}

/// Detect PM3 output that means "I do not have that command", as opposed to a
/// command that ran and found nothing.
///
/// Both shapes exit 0, so the exit code alone cannot tell them apart:
///
///   lf t55xx danger raw
///   lf t55xx dangerraw: unexpected argument "raw"
///   [!] Try 'lf t55xx dangerraw --help' for more information.
///
///   lf t55xx set config
///   --------- operations ----------
///   config      Set/Get T55XX configuration ...
///
/// The second form is an unknown *subcommand*: PM3 falls back to the help menu
/// for the group. Section banners like `----- operations -----` only appear in
/// that menu, so they are a reliable marker.
fn rejected_command(cleaned: &str) -> Option<&'static str> {
    if cleaned.contains("unexpected argument \"") {
        return Some("unknown option or argument");
    }
    if cleaned.contains("for more information.") && cleaned.contains("--help'") {
        return Some("command not recognised");
    }
    if HELP_BANNER_RE.is_match(cleaned) {
        return Some("unknown subcommand, help menu printed instead");
    }
    None
}

static HELP_BANNER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-{5,}\s*(operations|recovery|simulation|notice|technical|magic gen|general)\s*-{5,}")
        .expect("bad help banner regex")
});

/// Detect PM3 output that means the command never reached the device.
///
/// Without this, a dropped USB connection looks exactly like an empty antenna.
/// `run_search_command` sets `tolerate_nonzero`, and PM3 exits 1 on a port
/// failure while still printing several non-empty lines, so the port error was
/// returned as a successful result. The parser then found no card in it and the
/// wizard told the user to place a card that was already on the reader.
///
/// Checked on both the exit-0 and the tolerated non-zero paths, so it is
/// independent of `tolerate_nonzero`.
/// Append PM3's own error text to a friendly message.
///
/// `friendly_exit_code` produces the short human summary, but it discards what
/// PM3 actually printed. That text is the only thing that distinguishes, for
/// example, an unreadable card from a missing key file. Keep the summary first
/// so existing wording stays intact, and only append when PM3 said something
/// that is not just its own echo of the command.
fn with_detail(message: String, detail: &str) -> String {
    let detail = detail.trim();
    // Drop the "execute command from commandline:" preamble and blank lines; keep
    // the informative remainder.
    let useful = detail
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("[+] execute command from commandline"))
        .collect::<Vec<_>>()
        .join(" ");
    if useful.is_empty() {
        message
    } else {
        format!("{} ({})", message, useful)
    }
}

fn device_failure(cleaned: &str) -> Option<String> {
    const MARKERS: &[&str] = &[
        "invalid serial port",
        "error opening serial port",
        "failed to open the serial port",
        "cannot open the port",
        "could not open port",
        "permission denied",
    ];
    // PM3's wording differs across platforms and versions ("Cannot open the
    // port" vs "cannot open the port"), so match case-insensitively.
    let lower = cleaned.to_lowercase();
    for marker in MARKERS {
        if lower.contains(marker) {
            return Some(format!(
                "Proxmark3 could not open the serial port ({}). Reconnect the device and try again.",
                marker
            ));
        }
    }
    None
}

/// Heuristic: extract the tag type from a PM3 info command so the error
/// message can say "No 14B tag found" instead of just "timed out".
fn detect_tag_type(cmd: &str) -> &str {    if cmd.contains("14b") { "ISO 14443-B" }
    else if cmd.contains("14a") { "ISO 14443-A" }
    else if cmd.contains("15") { "ISO 15693" }
    else if cmd.contains("felica") { "Felica" }
    else if cmd.contains("iclass") { "iCLASS" }
    else if cmd.contains("legic") { "LEGIC" }
    else if cmd.contains("mfu") { "MIFARE Ultralight" }
    else if cmd.contains("mfdes") { "DESFire" }
    else if cmd.contains("calypso") { "Calypso" }
    else if cmd.contains("emrtd") { "eMRTD passport" }
    else if cmd.contains("t55") { "T55xx" }
    else if cmd.contains("em4305") { "EM4305" }
    else if cmd.contains("trovan") { "Trovan" }
    else { "requested" }
}

// ---------------------------------------------------------------------------
// HF Operation State — holds child process for cancellation + dump file path
// ---------------------------------------------------------------------------

/// Managed state for long-running HF operations (autopwn, dump, write).
/// Stored via `app.manage()` in `lib.rs`.
pub struct HfOperationState {
    /// Running child process — `take()` to kill via `CommandChild::kill(self)`.
    pub child: Mutex<Option<CommandChild>>,
    /// Dump file path set by autopwn after completion (e.g. "hf-mf-01020304-dump.bin").
    pub dump_path: Mutex<Option<String>>,
}

impl HfOperationState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            dump_path: Mutex::new(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Persistent interactive PM3 session (Plan Section 7)
// ---------------------------------------------------------------------------

/// Prompt the PM3 interactive client prints before reading each command.
const PM3_PROMPT: &str = "pm3 -->";

/// How long to wait for the client to print its initial prompt after spawn.
const PM3_SESSION_PROMPT_TIMEOUT: Duration = Duration::from_secs(10);

/// A long-lived `proxmark3.exe <port>` interactive session.
///
/// Unlike the one-shot `execute_pm3` path (a fresh process per command), this
/// keeps a single client process alive so the device's internal state (detected
/// modulation, card select, etc.) survives between commands. stdin/stdout/stderr
/// are piped; `send_interactive_command` writes a command and reads until the
/// next `pm3 -->` prompt.
pub struct Pm5Session {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    buffer: String,
}

/// Validate an interactive command (no embedded newlines that could inject a
/// second command). Pure so it can be unit-tested without spawning a process.
fn validate_interactive_command(cmd: &str) -> Result<(), AppError> {
    if cmd.contains('\n') || cmd.contains('\r') {
        return Err(AppError::CommandFailed(
            "Invalid characters in command".into(),
        ));
    }
    Ok(())
}

/// Resolve the bundled `proxmark3` binary for interactive sessions, mirroring the
/// lookup used by the one-shot path. Falls back to `None` so the caller can rely
/// on PATH.
fn resolve_pm3_binary() -> Option<PathBuf> {
    let exe_name = if cfg!(target_os = "windows") {
        "proxmark3.exe"
    } else {
        "proxmark3"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [
                dir.join(exe_name),
                dir.join("binaries").join(exe_name),
                dir.join("binaries").join("proxmark3").join(exe_name),
            ] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

impl Pm5Session {
    /// Read from the client until it prints the `pm3 -->` prompt (or `wait`
    /// elapses). Appends everything read to `self.buffer`.
    async fn wait_for_prompt(&mut self, wait: Duration) -> Result<(), AppError> {
        let mut chunk = [0u8; 1024];
        loop {
            let n = match timeout(wait, self.reader.read(&mut chunk)).await {
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    return Err(AppError::CommandFailed(format!(
                        "PM3 session read failed: {e}"
                    )))
                }
                Err(_) => {
                    return Err(AppError::Timeout(
                        "Timed out waiting for the PM3 prompt".into(),
                    ))
                }
            };
            if n == 0 {
                return Err(AppError::CommandFailed(
                    "PM3 session closed unexpectedly".into(),
                ));
            }
            self.buffer.push_str(&String::from_utf8_lossy(&chunk[..n]));
            if self.buffer.contains(PM3_PROMPT) {
                return Ok(());
            }
        }
    }

    /// Terminate the underlying client process.
    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
    }
}

/// Spawn a persistent interactive `proxmark3.exe <port>` session and wait for the
/// initial prompt. The caller is responsible for holding the per-port lock across
/// the spawn (see `ManagedPm5Session::start`).
pub async fn start_persistent_session(port: &str) -> Result<Pm5Session, AppError> {
    if !PORT_RE.is_match(port) {
        return Err(AppError::CommandFailed(format!("Invalid port: {port}")));
    }

    let exe_name = if cfg!(target_os = "windows") {
        "proxmark3.exe"
    } else {
        "proxmark3"
    };
    let program = resolve_pm3_binary().unwrap_or_else(|| PathBuf::from(exe_name));

    let mut cmd = tokio::process::Command::new(&program);
    cmd.arg(port)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // tokio::process::Command exposes `creation_flags` on Windows directly.
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| {
        AppError::CommandFailed(format!("Failed to spawn {}: {e}", program.display()))
    })?;

    let stdin = child.stdin.take().ok_or_else(|| {
        AppError::CommandFailed("PM3 session stdin unavailable".into())
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        AppError::CommandFailed("PM3 session stdout unavailable".into())
    })?;

    // Drain stderr in the background so the child never blocks on a full pipe;
    // mirror it to the session log.
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                logging::line(&format!("[pm3-session] {line}"));
            }
        });
    }

    let mut session = Pm5Session {
        child,
        stdin,
        reader: BufReader::new(stdout),
        buffer: String::new(),
    };
    session.wait_for_prompt(PM3_SESSION_PROMPT_TIMEOUT).await?;
    Ok(session)
}

/// Send one command to an interactive session and return its output (without the
/// trailing prompt).
pub async fn send_interactive_command(
    session: &mut Pm5Session,
    cmd: &str,
) -> Result<String, AppError> {
    validate_interactive_command(cmd)?;

    session.buffer.clear();
    session
        .stdin
        .write_all(cmd.as_bytes())
        .await
        .map_err(|e| AppError::CommandFailed(format!("PM3 session write failed: {e}")))?;
    session
        .stdin
        .write_all(b"\n")
        .await
        .map_err(|e| AppError::CommandFailed(format!("PM3 session write failed: {e}")))?;
    session
        .stdin
        .flush()
        .await
        .map_err(|e| AppError::CommandFailed(format!("PM3 session flush failed: {e}")))?;

    session.wait_for_prompt(PM3_COMMAND_TIMEOUT).await?;

    let raw = std::mem::take(&mut session.buffer);
    Ok(raw.replace(PM3_PROMPT, "").trim().to_string())
}

/// Thread-safe managed interactive session exposed via Tauri application state.
///
/// Holds at most one `Pm5Session`. `start` takes the shared per-port lock while
/// spawning (so it cannot race a direct serial query or another spawn), and
/// `send` serialises commands through the inner async mutex.
pub struct ManagedPm5Session {
    session: tokio::sync::Mutex<Option<Pm5Session>>,
}

impl ManagedPm5Session {
    pub fn new() -> Self {
        Self {
            session: tokio::sync::Mutex::new(None),
        }
    }

    /// Start (or replace) the interactive session for `port`.
    pub async fn start(&self, port: &str) -> Result<(), AppError> {
        let _port_guard = acquire_port_lock(port, PM3_COMMAND_TIMEOUT).await?;
        let session = start_persistent_session(port).await?;
        let mut guard = self.session.lock().await;
        if let Some(old) = guard.take() {
            old.stop().await;
        }
        *guard = Some(session);
        Ok(())
    }

    /// Send a command to the active session.
    pub async fn send(&self, cmd: &str) -> Result<String, AppError> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            AppError::CommandFailed("No active PM5 session. Start one first.".into())
        })?;
        send_interactive_command(session, cmd).await
    }

    /// Terminate the active session, if any.
    pub async fn stop(&self) {
        if let Some(session) = self.session.lock().await.take() {
            session.stop().await;
        }
    }
}

impl Default for ManagedPm5Session {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Streaming command execution (HF operations)
// ---------------------------------------------------------------------------

/// Run a PM3 command with streaming output, supporting long timeouts and
/// cancellation. Unlike `run_command()` which uses `.output()` (blocks until
/// exit), this uses `.spawn()` + async line reading.
///
/// - Each stdout/stderr line is emitted as a `pm3-output` event (live terminal).
/// - A per-line callback `on_line` is invoked for real-time parsing (e.g. autopwn
///   progress events). The callback receives the cleaned line text.
/// - The child process is stored in `hf_state.child` so `cancel_hf_operation`
///   can kill it mid-run.
/// - Returns the accumulated cleaned output on success.
pub async fn run_command_streaming<F>(
    app: &AppHandle,
    port: &str,
    cmd: &str,
    timeout_secs: u64,
    hf_state: &HfOperationState,
    mut on_line: F,
) -> Result<String, AppError>
where
    F: FnMut(&str),
{
    // Validate port
    if !PORT_RE.is_match(port) {
        return Err(AppError::CommandFailed(format!("Invalid port: {}", port)));
    }

    // Reject command separators
    if cmd.contains(';') || cmd.contains('\n') || cmd.contains('\r') {
        return Err(AppError::CommandFailed(
            "Invalid characters in command".into(),
        ));
    }

    emit_output(app, &format!("pm3 --> {}", cmd), false);

    // Serialise this port with direct serial queries and one-shot command
    // execution. Held across the spawn and the whole stream read so a concurrent
    // command cannot open the same COM port while this child is running.
    let _port_guard = acquire_port_lock(port, Duration::from_secs(timeout_secs)).await?;

    // Try sidecar first, then scope names — same strategy as execute_pm3()
    let (rx, child) = spawn_pm3(app, port, cmd)?;

    // Store child for cancellation
    {
        let mut lock = hf_state
            .child
            .lock()
            .map_err(|e| AppError::CommandFailed(format!("HF state lock poisoned: {}", e)))?;
        *lock = Some(child);
    }

    // Read lines with timeout
    let result = read_stream_with_timeout(app, rx, timeout_secs, &mut on_line).await;

    // Clear child on completion (process already exited or was killed)
    {
        let mut lock = hf_state.child.lock().unwrap_or_else(|e| e.into_inner());
        *lock = None;
    }

    match result {
        Ok(output) => Ok(output),
        Err(e) => {
            emit_output(app, &e.to_string(), true);
            Err(e)
        }
    }
}

/// Spawn PM3 via sidecar or scope names, returning the event receiver + child.
fn spawn_pm3(
    app: &AppHandle,
    port: &str,
    cmd: &str,
) -> Result<(tauri::async_runtime::Receiver<CommandEvent>, CommandChild), AppError> {
    let args = ["-p", port, "-f", "-c", cmd];

    // Try direct binary spawn first (Tauri's externalBin places binary in same dir as executable)
    // This uses std::process::Command with CREATE_NO_WINDOW to suppress console popups
    if let Some(exe_path) = std::env::current_exe().ok() {
        if let Some(dir) = exe_path.parent() {
            let exe_name = if cfg!(target_os = "windows") {
                "proxmark3.exe"
            } else {
                "proxmark3"
            };

            // Try same directory as executable (Tauri externalBin typical behavior)
            let binary_path = dir.join(exe_name);
            if binary_path.is_file() {
                let binary_str = binary_path.to_string_lossy().into_owned();
                #[cfg(target_os = "windows")]
                {
                    match app.shell().command(&binary_str).args(&args).spawn() {
                        Ok(result) => return Ok(result),
                        Err(_) => {}
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    match app.shell().command(&binary_str).args(&args).spawn() {
                        Ok(result) => return Ok(result),
                        Err(_) => {}
                    }
                }
            }
        }
    }

    // Try sidecar (available in production builds)
    #[cfg(target_os = "windows")]
    {
        if let Ok(sidecar_cmd) = app.shell().sidecar("binaries/proxmark3") {
            if let Ok(result) = sidecar_cmd.args(&args).spawn() {
                return Ok(result);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(sidecar_cmd) = app.shell().sidecar("binaries/proxmark3") {
            if let Ok(result) = sidecar_cmd.args(&args).spawn() {
                return Ok(result);
            }
        }
    }

    // Fall back to scope names
    let scope_names = pm3_scope_names();
    let mut first_err: Option<String> = None;

    for scope_name in &scope_names {
        match app.shell().command(scope_name).args(&args).spawn() {
            Ok(result) => return Ok(result),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("{}", e));
                }
            }
        }
    }

    Err(AppError::CommandFailed(format!(
        "Failed to spawn proxmark3: {}",
        first_err.unwrap_or_else(|| "binary not found".into())
    )))
}

/// Read from a `CommandEvent` receiver, accumulating output and emitting lines.
/// Returns the full cleaned output when the process terminates.
async fn read_stream_with_timeout<F>(
    app: &AppHandle,
    mut rx: tauri::async_runtime::Receiver<CommandEvent>,
    timeout_secs: u64,
    on_line: &mut F,
) -> Result<String, AppError>
where
    F: FnMut(&str),
{
    let deadline = Duration::from_secs(timeout_secs);
    let mut accumulated = String::new();
    let mut exit_code: Option<i32> = None;

    loop {
        match timeout(deadline, rx.recv()).await {
            Err(_) => {
                // Timeout expired
                return Err(AppError::Timeout(format!(
                    "HF operation timed out after {}s",
                    timeout_secs
                )));
            }
            Ok(None) => {
                // Channel closed — process exited
                break;
            }
            Ok(Some(event)) => match event {
                CommandEvent::Stdout(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let cleaned = strip_ansi(&line);
                    let trimmed = cleaned.trim();
                    if !trimmed.is_empty() {
                        emit_output(app, trimmed, false);
                        on_line(trimmed);
                        accumulated.push_str(trimmed);
                        accumulated.push('\n');
                    }
                }
                CommandEvent::Stderr(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let cleaned = strip_ansi(&line);
                    let trimmed = cleaned.trim();
                    if !trimmed.is_empty() {
                        emit_output(app, trimmed, true);
                        on_line(trimmed);
                        accumulated.push_str(trimmed);
                        accumulated.push('\n');
                    }
                }
                CommandEvent::Error(msg) => {
                    emit_output(app, &msg, true);
                    return Err(AppError::CommandFailed(format!("Process error: {}", msg)));
                }
                CommandEvent::Terminated(payload) => {
                    exit_code = payload.code;
                    break;
                }
                _ => {} // Future CommandEvent variants — ignore
            },
        }
    }

    // Check exit code
    match exit_code {
        Some(0) | None => Ok(accumulated),
        Some(-5) | Some(251) => Err(AppError::Timeout("PM3 subprocess timed out".into())),
        Some(code) => Err(AppError::CommandFailed(format!(
            "PM3 exited with code {}",
            code
        ))),
    }
}

/// Scan common COM/serial ports trying `hw version` to find a connected PM3.
/// Returns (port, model, firmware) on success.
///
/// Uses friendly, hacker-casual terminal output. All probe messages are green
/// (non-error) except the final "not found" message.
///
/// Each port is probed with a short 3-second timeout (DETECT_TIMEOUT) so that
/// scanning 40 COM ports completes in under 2 minutes even when most ports
/// are empty. A real PM3 responds to `hw version` in well under 2 seconds.
pub async fn detect_device(app: &AppHandle) -> Result<(String, String, String), AppError> {
    let candidates = build_port_candidates();

    // Probe sweeps spawn many short-lived clients. Each `probe_port` call now
    // takes the per-port lock for the specific port it probes, so a probe of one
    // port cannot collide with a command on that same port, while probes of
    // different ports run in parallel. No machine-wide lock is needed here.

    // Pick a random init message for personality
    let init_msgs = [
        "[=] Sniffing USB bus... come out, Proxmark",
        "[=] Deploying port tentacles...",
        "[=] Hunting for hardware... stay still",
        "[=] Scanning the wire... don't be shy",
        "[=] Reaching out to the other side...",
    ];
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % init_msgs.len();
    emit_output(app, init_msgs[idx], false);

    for port in &candidates {
        emit_output(app, &format!("[=] Knocking on {}...", port), false);

        match probe_port(app, port).await {
            Ok(output) => {
                if let Some((model, firmware)) = parse_hw_version(&output) {
                    emit_output(
                        app,
                        &format!("[+] Target acquired: {} on {}", model, port),
                        false,
                    );
                    emit_output(app, &format!("[+] Firmware: {}", firmware), false);
                    return Ok((port.clone(), model, firmware));
                }
                // Got output but couldn't parse hw version -- wrong device
                emit_output(app, &format!("[-] {} -- wrong device", port), false);
            }
            Err(e) => {
                // Capabilities mismatch means the PM3 device IS present on this
                // port but the firmware doesn't match the client version. Treat
                // it as a successful detection -- the firmware check step will
                // handle the mismatch and offer to flash.
                let err_msg = e.to_string();
                if err_msg.to_lowercase().contains("capabilities") {
                    emit_output(
                        app,
                        &format!(
                            "[+] Target acquired: Proxmark3 on {} (firmware mismatch)",
                            port
                        ),
                        false,
                    );
                    return Ok((
                        port.clone(),
                        "Proxmark3".to_string(),
                        "mismatched".to_string(),
                    ));
                }

                // Distinguish "no response" (spawn succeeded but device didn't respond)
                // from other errors. If spawn itself failed (binary not found), that
                // affects ALL ports, so propagate immediately.
                if err_msg.contains("Failed to spawn proxmark3") {
                    emit_output(
                        app,
                        "[!!] Proxmark3 binary not found. Check installation.",
                        true,
                    );
                    return Err(e);
                }

                emit_output(app, &format!("[-] {} -- no response", port), false);
            }
        }
    }

    emit_output(app, "[!!] No Proxmark3 found.", true);
    emit_output(
        app,
        "[=] Try a different USB cable (some are charge-only)",
        false,
    );
    emit_output(app, "[=] Check Device Manager for a COM port", false);
    emit_output(
        app,
        "[=] PM3 Easy: may need CH340 driver (wch-ic.com)",
        false,
    );
    Err(AppError::DeviceNotFound)
}

/// Probe a single port with a short timeout for device detection.
/// Uses DETECT_TIMEOUT (3s) instead of the full PM3_COMMAND_TIMEOUT (30s)
/// so scanning 40 COM ports completes quickly.
async fn probe_port(app: &AppHandle, port: &str) -> Result<String, AppError> {
    // Probe the port under the same per-port lock used by command execution and
    // direct serial queries. `detect_device` no longer holds a global lock, so a
    // probe of COM5 does not block a command on COM4.
    let _port_guard = acquire_port_lock(port, DETECT_TIMEOUT).await?;

    let scope_names = pm3_scope_names();
    let mut first_spawn_error: Option<AppError> = None;

    for scope_name in &scope_names {
        let output_future = app
            .shell()
            .command(scope_name)
            .args(["-p", port, "-f", "-c", "hw version"])
            .output();

        let output = match timeout(DETECT_TIMEOUT, output_future).await {
            Err(_) => {
                // Port timed out -- likely no device. Skip to next port.
                return Err(AppError::Timeout(format!(
                    "Port {} timed out during detection",
                    port
                )));
            }
            Ok(Err(e)) => {
                // Spawn failed -- binary not found at this path. Record the error
                // from the first attempt (PATH lookup) and try the next location.
                if first_spawn_error.is_none() {
                    first_spawn_error = Some(AppError::CommandFailed(format!(
                        "Failed to spawn proxmark3: {}",
                        e
                    )));
                }
                continue;
            }
            Ok(Ok(output)) => output,
        };

        // Binary was found and executed -- process the result immediately.
        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        return match code {
            0 => {
                let cleaned = strip_ansi(&stdout);
                Ok(cleaned)
            }
            -5 | 251 => Err(AppError::Timeout(format!("PM3 timed out running: {}", "hw version"))),
            _ => {
                let detail = if stderr.is_empty() {
                    strip_ansi(&stdout)
                } else {
                    strip_ansi(&stderr)
                };
                Err(AppError::CommandFailed(with_detail(
                    friendly_exit_code(code, "hw version"),
                    &detail,
                )))
            }
        };
    }

    // All scope names exhausted -- return the first spawn error (from PATH lookup)
    // so the error message is the most user-recognizable one.
    Err(first_spawn_error.unwrap_or_else(|| {
        AppError::CommandFailed("Failed to spawn proxmark3: binary not found".into())
    }))
}

/// Probe device capabilities using Layer 1 (direct serial) with Layer 2 (stderr scraping) fallback.
///
/// This is the primary capability detection function that should be called before
/// spawning any `proxmark3.exe` process. It returns detailed capability information
/// including the device's CAPABILITIES_VERSION, which enables version mediation.
///
/// Layer 1: Direct serial probe via NG frames (CMD_PING + CMD_CAPABILITIES)
/// Layer 2: Fallback to stderr scraping from failed `proxmark3.exe` connection attempt
///
/// Returns a ProbeResult with the detection method used.
pub async fn probe_device_with_fallback(
    app: &AppHandle,
    port: &str,
    baudrate: u32,
) -> Result<ProbeResult, AppError> {
    // Layer 1: Direct serial probe
    match probe_device_capabilities(port, baudrate).await {
        Ok(result) => {
            emit_output(app, &format!("[+] Layer 1 probe succeeded: device capabilities v{}", result.capabilities_version), false);
            return Ok(result);
        }
        Err(e) => {
            emit_output(app, &format!("[=] Layer 1 probe failed: {}. Trying Layer 2...", e), false);
        }
    }

    // Layer 2: Spawn bundled client and capture stderr for capabilities mismatch
    // The Layer 1 probe above already released the per-port lock; take it again so
    // the fallback client spawn shares the exact same lock as direct serial I/O.
    let _port_guard = acquire_port_lock(port, DETECT_TIMEOUT).await?;

    let scope_names = pm3_scope_names();
    let mut first_spawn_error: Option<AppError> = None;

    for scope_name in &scope_names {
        let output_future = app
            .shell()
            .command(scope_name)
            .args(["-p", port, "-f", "-c", "hw version"])
            .output();

        let output = match timeout(DETECT_TIMEOUT, output_future).await {
            Err(_) => {
                return Err(AppError::Timeout(format!(
                    "Port {} timed out during Layer 2 probe",
                    port
                )));
            }
            Ok(Err(e)) => {
                if first_spawn_error.is_none() {
                    first_spawn_error = Some(AppError::CommandFailed(format!(
                        "Failed to spawn proxmark3: {}",
                        e
                    )));
                }
                continue;
            }
            Ok(Ok(output)) => output,
        };

        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // If hw version succeeded (exit code 0), parse normally
        if code == 0 {
            let _cleaned = strip_ansi(&stdout);
            // We can't extract capabilities_version from hw version output directly,
            // but we know the device is compatible with the client.
            return Ok(ProbeResult {
                capabilities_version: 0, // Unknown from hw version alone
                capabilities: None,
                max_cmd_data_size: 512,
                is_pm5: false,
                method: ProbeMethod::StderrScraping,
            });
        }

        // Check for capabilities mismatch in stderr
        if let Some((device_ver, client_ver)) = parse_capabilities_mismatch(&stderr) {
            emit_output(app, &format!("[+] Layer 2 probe succeeded via stderr: device v{}, client v{}", device_ver, client_ver), false);
            return Ok(ProbeResult {
                capabilities_version: device_ver,
                capabilities: None,
                max_cmd_data_size: 512,
                is_pm5: device_ver >= 11, // PM5 features start around v11
                method: ProbeMethod::StderrScraping,
            });
        }

        // Other error
        let detail = if stderr.is_empty() {
            strip_ansi(&stdout)
        } else {
            strip_ansi(&stderr)
        };
        return Err(AppError::CommandFailed(with_detail(
            friendly_exit_code(code, "hw version"),
            &detail,
        )));
    }

    Err(first_spawn_error.unwrap_or_else(|| {
        AppError::CommandFailed("Failed to spawn proxmark3 for Layer 2 probe: binary not found".into())
    }))
}

fn build_port_candidates() -> Vec<String> {
    // Get available ports from serialport and score them
    let mut scored = Vec::new();

    if let Ok(available) = serialport::available_ports() {
        for info in available {
            let port_str = info.port_name.clone();
            let score = score_port(&port_str, &info.port_type);
            scored.push((score, port_str));
        }
    }

    // If serialport couldn't list ports (or returned none), fall back to static list
    if scored.is_empty() {
        scored = build_fallback_candidates();
    }

    // Sort by score descending (highest score first)
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    scored.into_iter().map(|(_, port)| port).collect()
}

/// Score a port based on heuristics: prefer ACM over USB on Linux,
/// lower-numbered COM ports on Windows, and known PM3 suffixes on macOS.
fn score_port(port_name: &str, port_type: &serialport::SerialPortType) -> u32 {
    let mut score: u32 = 0;

    // Base score
    score += 10;

    // Platform-specific scoring
    #[cfg(target_os = "linux")]
    {
        // Prefer ttyACM (built-in USB-serial) over ttyUSB (external adapters)
        if port_name.contains("ttyACM") {
            score += 50;
        } else if port_name.contains("ttyUSB") {
            score += 20;
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Prefer lower COM port numbers (more common for PM3 devices)
        if let Some(num) = port_name.strip_prefix("COM") {
            if let Ok(n) = num.parse::<u32>() {
                score += 50;
                // Lower numbers get higher scores
                if n <= 10 {
                    score += 30;
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Prefer known PM3 suffixes
        let pm3_suffixes = ["iceman1", "14101", "14201", "14301"];
        for suffix in pm3_suffixes {
            if port_name.contains(suffix) {
                score += 50;
                break;
            }
        }
    }

    // Check port type if available
    if let serialport::SerialPortType::UsbPort(info) = port_type {
        // Award points for vendor/product IDs that match known PM3 devices
        let known_vendors = [0x0403, 0x10C4, 0x03EB]; // FTDI, SiLabs, Atmel
        if known_vendors.contains(&info.vid) {
            score += 30;
        }
    }

    score
}

/// Build fallback port list when serialport enumeration fails.
fn build_fallback_candidates() -> Vec<(u32, String)> {
    let mut ports = Vec::new();

    if cfg!(target_os = "windows") {
        // Windows COM ports -- extend to 40 to cover USB hub reassignment
        for i in 1..=40 {
            ports.push((20, format!("COM{}", i)));
        }
    } else if cfg!(target_os = "macos") {
        // macOS: /dev/tty.usbmodem* -- cover common PM3 suffixes
        for suffix in &["iceman1", "14101", "14201", "14301", "1", "2", "3"] {
            ports.push((30, format!("/dev/tty.usbmodem{}", suffix)));
        }
    } else {
        // Linux: /dev/ttyACM* and /dev/ttyUSB*
        for i in 0..=5 {
            ports.push((50, format!("/dev/ttyACM{}", i)));
            ports.push((20, format!("/dev/ttyUSB{}", i)));
        }
    }

    ports
}

/// Run the proxmark3 binary directly using tokio::process::Command.
/// Used for bundled binaries found via current_exe() lookup.
///
/// `wait` controls how long to wait for the subprocess. Pass
/// `PM3_COMMAND_TIMEOUT` (30s) for normal commands and `DETECT_COMMAND_TIMEOUT`
/// (8s) for tag-detection commands where PM3 hangs when no tag is present.
async fn run_binary_direct(
    binary_path: std::path::PathBuf,
    port: &str,
    cmd: &str,
    wait: Duration,
) -> Result<String, AppError> {
    // Share the per-port lock with direct serial NG queries and the scope-name
    // spawn path. This function is only reached from `execute_pm3`'s sidecar
    // attempt, which does not hold the guard, so this acquisition never nests.
    let _port_guard = acquire_port_lock(port, wait).await?;

    // On Windows, use std::process::Command with CREATE_NO_WINDOW to suppress console popups
    // tokio's Command doesn't expose creation_flags, so we use std::process and wrap for async
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        let mut std_command = std::process::Command::new(&binary_path);
        std_command.args(&["-p", port, "-f", "-c", cmd]);
        std_command.creation_flags(CREATE_NO_WINDOW);
        std_command.stdout(std::process::Stdio::piped());
        std_command.stderr(std::process::Stdio::piped());

        // Use tokio to wrap the sync command output.
        //
        // kill_on_drop is essential, not an optimisation. When the timeout below
        // fires, the future is dropped -- and tokio's default is kill_on_drop =
        // false, which ORPHANS the PM3 child. The orphan keeps the serial port
        // open, so the very next command fails with "invalid serial port".
        // Measured on this device: `lf search` takes ~10s, so any cap below that
        // produced a self-inflicted cascade of port errors.
        let started = std::time::Instant::now();
        let output_future = tokio::process::Command::from(std_command)
            .kill_on_drop(true)
            .output();
let output = match timeout(wait, output_future).await {
            Err(_) => {
                // A timeout is exactly the kind of failure a bug report needs to
                // show in detail: the child was killed while holding the serial
                // port. Record it before propagating, since no output exists.
                logging::command(
                    port,
                    cmd,
                    started.elapsed().as_millis(),
                    None,
                    "",
                    Some(&format!(
                        "TIMED OUT after {}s -- child killed (may still have held the serial port)",
                        wait.as_secs()
                    )),
                );
                return Err(AppError::Timeout(format!(
                    "PM3 command timed out after {}s: {}",
                    wait.as_secs(),
                    cmd
                )));
            }
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                logging::command(
                    port,
                    cmd,
                    started.elapsed().as_millis(),
                    None,
                    "",
                    Some(&format!("SPAWN FAILED: {}", e)),
                );
                return Err(AppError::CommandFailed(format!(
                    "Failed to run PM3 command: {}",
                    e
                )));
            }
        };

let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // This is the bundled-client path -- the one almost every real command
        // takes -- so the raw exchange must be recorded here or the log is
        // empty for the cases that actually matter.
        logging::command(
            port,
            cmd,
            started.elapsed().as_millis(),
            Some(code),
            &format!("{}{}", stdout, stderr),
            Some(&format!("binary: {}", binary_path.display())),
        );

        interpret_pm3_result(cmd, code, &stdout, &stderr, false)
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        let mut command = tokio::process::Command::new(binary_path);
        command.args(&["-p", port, "-f", "-c", cmd]);
        // See the Windows branch above: without this a timed-out PM3 child is
        // orphaned and keeps holding the serial port.
        command.kill_on_drop(true);

        let started = std::time::Instant::now();
        let output_future = command.output();
let output = match timeout(wait, output_future).await {
            Err(_) => {
                // A timeout is exactly the kind of failure a bug report needs to
                // show in detail: the child was killed while holding the serial
                // port. Record it before propagating, since no output exists.
                logging::command(
                    port,
                    cmd,
                    started.elapsed().as_millis(),
                    None,
                    "",
                    Some(&format!(
                        "TIMED OUT after {}s -- child killed (may still have held the serial port)",
                        wait.as_secs()
                    )),
                );
                return Err(AppError::Timeout(format!(
                    "PM3 command timed out after {}s: {}",
                    wait.as_secs(),
                    cmd
                )));
            }
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                logging::command(
                    port,
                    cmd,
                    started.elapsed().as_millis(),
                    None,
                    "",
                    Some(&format!("SPAWN FAILED: {}", e)),
                );
                return Err(AppError::CommandFailed(format!(
                    "Failed to run PM3 command: {}",
                    e
                )));
            }
        };

let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // This is the bundled-client path -- the one almost every real command
        // takes -- so the raw exchange must be recorded here or the log is
        // empty for the cases that actually matter.
        logging::command(
            port,
            cmd,
            started.elapsed().as_millis(),
            Some(code),
            &format!("{}{}", stdout, stderr),
            Some(&format!("binary: {}", binary_path.display())),
        );

        interpret_pm3_result(cmd, code, &stdout, &stderr, false)
    }
}

/// Attempting to run a PM3 command via the bundled sidecar binary (silent -- no emit).
/// Returns Ok(stdout) on success, Err on any failure (sidecar not found, spawn
/// error, non-zero exit code). Callers should fall through to PATH-based lookup
/// on failure.
///
/// The sidecar binary depends on DLLs (Qt5, ICU, etc.) bundled in the same
/// directory via `bundle.resources`. The Windows DLL loader finds them
/// automatically since they share the sidecar's directory.
///
/// `wait` controls how long to wait for the subprocess.
async fn try_sidecar_silent(
    app: &AppHandle,
    port: &str,
    cmd: &str,
    wait: Duration,
) -> Result<String, AppError> {
    use std::env;

    // First, try to find the proxmark3 binary in the root directory (where DLLs are)
    // This matches Tauri's externalBin behavior where the binary is placed in the root
    if let Some(exe_path) = env::current_exe().ok() {
        let exe_dir = exe_path.parent();
        if let Some(dir) = exe_dir {
            let exe_name = if cfg!(target_os = "windows") {
                "proxmark3.exe"
            } else {
                "proxmark3"
            };

            // Try root directory first (DLLs are here)
            let root_path = dir.join(exe_name);
            if root_path.is_file() {
                return run_binary_direct(root_path, port, cmd, wait).await;
            }

            // Try binaries subdirectory (alternative config)
            let binary_path = dir.join("binaries").join(exe_name);
            if binary_path.is_file() {
                return run_binary_direct(binary_path, port, cmd, wait).await;
            }
        }
    }

    // Fall back to Tauri sidecar lookup for dev mode or bundled apps
    let sidecar = app
        .shell()
        .sidecar("binaries/proxmark3")
        .map_err(|e| AppError::CommandFailed(format!("Sidecar not available: {}", e)))?;

    let output_future = sidecar.args(["-p", port, "-f", "-c", cmd]).output();

let output = match timeout(wait, output_future).await {
            Err(_) => {
                return Err(AppError::Timeout(format!(
                    "PM3 command timed out after {}s: {}",
                    wait.as_secs(),
                    cmd
                )));
            }
        Ok(Err(e)) => {
            return Err(AppError::CommandFailed(format!(
                "Failed to run sidecar: {}",
                e
            )));
        }
        Ok(Ok(output)) => output,
    };

let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    interpret_pm3_result(cmd, code, &stdout, &stderr, false)
}

fn parse_hw_version(output: &str) -> Option<(String, String)> {
    use crate::pm3::version::parse_detailed_hw_version;

    let info = parse_detailed_hw_version(output);

    // Pick best version source: os_version (device firmware) > client_version
    let version_str = if !info.os_version.is_empty() {
        info.os_version
    } else if !info.client_version.is_empty() {
        info.client_version
    } else {
        // No version found — check if it's at least a PM3 device
        if output.to_lowercase().contains("proxmark") {
            return Some((info.model, "unknown".to_string()));
        }
        return None;
    };

    // Extract clean short version like "v4.20728" for sidebar display
    let firmware = extract_short_version(&version_str);
    Some((info.model, firmware))
}

/// Extract a short version string like "v4.20728" from a full version string
/// like "Iceman/master/v4.20728-358-ga2ba91043-suspect".
fn extract_short_version(version_str: &str) -> String {
    // Find 'v' followed by a digit
    let v_pos = version_str.char_indices().find(|&(i, c)| {
        c == 'v'
            && version_str.get(i + 1..i + 2).map_or(false, |s| {
                s.as_bytes().first().map_or(false, |b| b.is_ascii_digit())
            })
    });

    if let Some((pos, _)) = v_pos {
        let rest = &version_str[pos..];
        // Version is "v" + digits/dots, stop at anything else
        let end = rest
            .find(|c: char| c != 'v' && !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        rest[..end].to_string()
    } else {
        version_str.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{friendly_exit_code, interpret_pm3_result, is_banner_only, is_card_dependent, is_safe_pm3_command_chain, is_t55xx_memory_cmd, pm3_error_line, rejected_command, validate_interactive_command};

    #[test]
    fn interactive_command_rejects_embedded_newlines() {
        assert!(validate_interactive_command("hw version").is_ok());
        assert!(validate_interactive_command("lf t55xx detect").is_ok());
        assert!(validate_interactive_command("hw version\nlf search").is_err());
        assert!(validate_interactive_command("hw version\r\n").is_err());
    }

    /// Verbatim PM3 output for `lf t55xx danger raw`. Exit code 0.
    #[test]
    fn flags_unexpected_argument() {
        let out = "pm3 --> lf t55xx danger raw\n\
                   lf t55xx dangerraw: unexpected argument \"raw\"\n\
                   [!] Try 'lf t55xx dangerraw --help' for more information.\n";
        assert!(rejected_command(out).is_some());
    }

    /// Verbatim PM3 output for `lf t55xx chk pwds`. Exit code 0.
    #[test]
    fn flags_unknown_chk_subcommand() {
        let out = "lf t55xx chk: unexpected argument \"pwds\"\n";
        assert!(rejected_command(out).is_some());
    }

    /// Verbatim PM3 output for `hf mf ejectblk --blk 0`: the whole `hf mf`
    /// menu, exit code 0. Must not be reported as a successful command.
    #[test]
    fn flags_help_menu_instead_of_command() {
        let out = "help             This help\n\
                   list             List MIFARE history\n\
                   -----------      ----------------------- recovery ---------------\n\
                   darkside         Darkside attack\n\
                   -----------      ----------------------- operations --------------\n\
dump             Dump MIFARE Classic tag to binary file\n";
        assert!(rejected_command(out).is_some());
    }

    // -- banner-only detection (empty output means "no tag") ----------------

    /// Verbatim output of `hf 14a info` with no card on the reader, exit code 0.
    ///
    /// Copied from the real capture at
    /// `Phosphor2.2GUI\logs\1249.md` (COM19, 30 Sep 2026) -- every line
    /// included, in order.
    ///
    /// An earlier version of this constant claimed to be a real capture but had
    /// been hand-edited until it agreed with the implementation: it dropped the
    /// `[=] Output will be flushed after every print.` line that `-f` always
    /// produces. That is how `is_banner_only` shipped returning false against
    /// real output while its own test passed. Do not "tidy" this constant --
    /// re-copy it from a real log instead.
    const BANNER_ONLY: &str = "[=] Session log C:/pm3/.proxmark3/logs/log_20260930155228.txt\n\
[=] Output will be flushed after every print.\n\
\n\
[+] loaded `C:/pm3/.proxmark3/preferences.json`\n\
[+] execute command from commandline: hf 14a info\n\
\n\
[+] Using UART port COM19\n\
[+] Communicating with PM3 over USB-CDC\n\
[+] Max frame size: 624 bytes\n\
[+] Emulator memory: 8192 bytes\n\
[usb|script] pm3 --> hf 14a info\n";

    #[test]
    fn banner_only_is_detected() {
        assert!(is_banner_only(BANNER_ONLY));
    }

    #[test]
    fn banner_only_tolerates_empty_output() {
        assert!(is_banner_only(""));
        assert!(is_banner_only("\n\n"));
    }

    /// Regression guard for the shipped bug: the `-f` flush line must be
    /// tolerated, because Phosphor passes `-f` on every invocation.
    #[test]
    fn banner_only_tolerates_dash_f_flush_line() {
        assert!(is_banner_only(
            "[=] Output will be flushed after every print.\n[+] Using UART port COM19\n"
        ));
    }

    /// A real result must not be mistaken for "no tag". Taken from
    /// logs/1247.md, the MIFARE Classic 1K detection.
    #[test]
    fn real_detection_is_not_banner_only() {
        let out = "[+]  UID: 0C CB F0 CF   ( ONUID, re-used )\n\
[+] ATQA: 00 04\n\
[+]  SAK: 08 [2]\n\
[+]    MIFARE Classic 1K\n";
        assert!(!is_banner_only(out));
    }

    #[test]
    fn card_dependent_covers_hf_and_lf_only() {
        assert!(is_card_dependent("hf mf info"));
        assert!(is_card_dependent("lf search"));
        assert!(is_card_dependent("  hf 14a reader"));
        // Device-level commands are excluded: silence is normal for them.
        assert!(!is_card_dependent("hw version"));
        assert!(!is_card_dependent("hw tune"));
        assert!(!is_card_dependent("data geto"));
        // Emulator-memory and config subcommands never touch the antenna, so a
        // quiet result must stay a success rather than becoming "no tag".
        assert!(!is_card_dependent("hf mf eclr"));
        assert!(!is_card_dependent("hf mf esave hf mf"));
        assert!(!is_card_dependent("hf mf gen"));
        assert!(!is_card_dependent("lf t55xx config"));
        assert!(!is_card_dependent("lf t55xx set config"));
        // ...but a genuine read of the same family is still card-dependent.
        assert!(is_card_dependent("hf mf dump"));
        assert!(is_card_dependent("hf mf autopwn"));
    }

    // -- shared result interpretation ---------------------------------------

    /// The regression test for the bug that actually shipped: this banner plus
    /// exit code 0 must produce an error, not a blank success. It runs through
    /// `interpret_pm3_result` because that is now the only place the decision is
    /// made, and it is the function the released `run_binary_direct` path calls.
    #[test]
    fn no_card_exit_zero_is_an_error() {
        let err = interpret_pm3_result("hf 14a info", 0, BANNER_ONLY, "", false)
            .expect_err("banner-only output must not be reported as success");
        assert!(err.to_string().contains("No tag detected"), "got: {err}");
    }

    /// A genuine detection on the same code path must still succeed.
    #[test]
    fn card_detected_exit_zero_succeeds() {
        let out = "[+]  UID: 0C CB F0 CF   ( ONUID, re-used )\n\
[+] ATQA: 00 04\n\
[+]  SAK: 08 [2]\n\
[+]    MIFARE Classic 1K\n";
        let got = interpret_pm3_result("hf 14a info", 0, out, "", false)
            .expect("a real detection must succeed");
        assert!(got.contains("MIFARE Classic 1K"));
    }

    /// PM3 writes `[!!]` diagnostics to stderr, never stdout. A command whose
    /// only output is an error must surface that reason rather than a blank
    /// success.
    #[test]
    fn stderr_diagnostic_is_surfaced() {
        let got = interpret_pm3_result("hf mf dump", 0, "", "[!!] No keys found\n", false)
            .expect_err("empty stdout with a [!!] diagnostic is a failure");
        assert!(got.to_string().contains("No keys found"), "got: {got}");
    }

    /// Exit -10 is `PM3_ESOFT` and is what `hf search` returns with no card.
    #[test]
    fn negative_ten_is_mapped() {
        let msg = friendly_exit_code(-10, "hf search");
        assert!(msg.contains("No tag found"), "got: {msg}");
        assert!(!msg.contains("Exit code"), "got: {msg}");
    }

    /// `hf 14b info` returns -5 when no card is found:
/// `return (found) ? PM3_SUCCESS : PM3_EOPABORTED;` (client/src/cmdhf14b.c).
/// The error is correct but the wording must say "no tag", not "aborted".
    #[test]
    fn eopaborted_reads_as_no_tag_for_card_commands() {
        let err = interpret_pm3_result("hf 14b info", -5, "[!] no ISO 14443-B tag found\n", "", false)
            .expect_err("no 14b tag is a failure");
        assert!(err.to_string().contains("No tag detected"), "got: {err}");
    }

    /// PM3's own `[!]` FAILED line is surfaced. When both markers appear the
    /// first line in output order wins, which is the one that explains the
    /// failure at the point it happened.
    #[test]
    fn failed_marker_line_is_surfaced() {
        let got = pm3_error_line("noise\n[!] no ISO 14443-B tag found\n");
        assert_eq!(got.as_deref(), Some("no ISO 14443-B tag found"));
        assert_eq!(pm3_error_line("[!!] hard failure\n"), Some("hard failure".into()));
        assert_eq!(pm3_error_line("[+] all good\n"), None);
        // A bare marker with no text is not a usable message.
        assert_eq!(pm3_error_line("[!!]\n"), None);
    }

    /// Only the one deliberate chain is permitted. Everything else containing
    /// `;` is still rejected as injection.
    #[test]
    fn only_the_t55xx_detect_chain_is_allowed() {
        // The required chain and its three memory verbs.
        assert!(is_safe_pm3_command_chain("lf t55xx detect; lf t55xx read -b 0"));
        assert!(is_safe_pm3_command_chain("lf t55xx detect; lf t55xx dump"));
        assert!(is_safe_pm3_command_chain("lf t55xx detect; lf t55xx write -b 0 -d 00000000 --verify"));
        // Plain single commands are untouched.
        assert!(is_safe_pm3_command_chain("hf 14a info"));
        assert!(is_safe_pm3_command_chain("lf t55xx detect"));
        // Injection still blocked.
        assert!(!is_safe_pm3_command_chain("AA;lf t55xx wipe"));
        assert!(!is_safe_pm3_command_chain("hf 14a info; hf mf dump"));
        assert!(!is_safe_pm3_command_chain("lf t55xx detect; lf t55xx wipe"));
        assert!(!is_safe_pm3_command_chain("lf t55xx detect; hw version"));
        assert!(!is_safe_pm3_command_chain("lf t55xx detect; lf t55xx read -b 0; hf mf dump"));
        assert!(!is_safe_pm3_command_chain("lf t55xx read -b 0\nhf mf dump"));
    }

    #[test]
    fn t55xx_memory_command_classification() {
        assert!(is_t55xx_memory_cmd("lf t55xx read -b 0"));
        assert!(is_t55xx_memory_cmd("lf t55xx dump"));
        assert!(is_t55xx_memory_cmd("lf t55xx write -b 1 -d 11223344 --verify"));
        // Not memory commands -- these must NOT get the prelude.
        assert!(!is_t55xx_memory_cmd("lf t55xx detect"));
        assert!(!is_t55xx_memory_cmd("lf t55xx chk"));
        assert!(!is_t55xx_memory_cmd("lf t55xx wipe"));
        assert!(!is_t55xx_memory_cmd("lf t55xx config"));
        assert!(!is_t55xx_memory_cmd("lf search"));
    }

    /// Verbatim output of `lf t55xx detect; lf t55xx dump` from the live
    /// client on COM19, 2026-10-01. Run bare, `lf t55xx dump` exits 0 with
    /// completely empty tables; the app reported that as "blank card".
    #[test]
    fn chained_dump_output_parses() {
        let out = "[=]  Chip type......... T55x7\n\
[=]  Modulation........ ASK\n\
[=]  Bit rate.......... 2 - RF/32\n\
[=]  Block0............ 000880E0 (auto detect)\n\
\n\
[=] ------------------------- T55xx tag memory -----------------------------\n\
[+] Page 0\n\
[+] blk | hex data | binary                           | ascii\n\
[+] ----+----------+----------------------------------+-------\n\
[+]  00 | 000880E0 | 00000000000010001000000011100000 | ....\n\
[+]  01 | 00000000 | 00000000000000000000000000000000 | ....\n";
        assert!(out.contains("000880E0"));
        // The detect preamble must not confuse the block parser.
        let block = super::super::output_parser::parse_t55xx_read_block(out);
        assert_eq!(block, Some("000880E0".to_string()));
    }

    #[test]
    fn t55xx_memory_commands_are_card_dependent() {
        assert!(is_card_dependent("lf t55xx detect; lf t55xx read -b 0"));
    }

    /// -4 is PM3_ETIMEOUT. It used to be reported as "Invalid argument", which
    /// told users their command was malformed when the card was the problem.
    #[test]
    fn timeout_code_is_not_blamed_on_the_user() {
        let msg = friendly_exit_code(-4, "hf 14a info");
        assert!(msg.contains("Timed out"), "got: {msg}");
        assert!(!msg.contains("Invalid argument"), "got: {msg}");
    }

    /// -14 is PM3_ENOTTY, the code behind the "invalid serial port" cascade.
    #[test]
    fn serial_port_code_is_diagnosed() {
        assert!(friendly_exit_code(-14, "hw version").contains("Serial port"));
    }

    /// `tolerate_nonzero` keeps `lf search` working: it prints a real chipset
    /// line and then exits -10.
    #[test]
    fn tolerate_nonzero_preserves_search_results() {
        let out = "[+] Chipset... T55xx\n[?] Hint: Try `lf t55xx` commands\n";
        let got =
            interpret_pm3_result("lf search", -10, out, "", true).expect("search result preserved");
        assert!(got.contains("T55xx"));
        // The same output without the opt-in is still an error.
        assert!(interpret_pm3_result("lf search", -10, out, "", false).is_err());
    }

    /// Verbatim PM3 output for `lf t55xx set config`, which also printed the
    /// group menu and exited 0.
    #[test]
    fn flags_t55xx_group_menu() {
        let out = "-----------      ----------------------------- operations ---------------\n\
                   config           Set/Get T55XX configuration (modulation, inverted, offset, rate)\n\
                   detect           Try detecting the tag modulation\n";
        assert!(rejected_command(out).is_some());
    }

    /// A real command that found nothing must not be flagged. This is the
    /// `lf search` output for an LF card that produced no known match.
    #[test]
    fn accepts_normal_no_tag_output() {
        let out = "[=] Note: False Positives ARE possible\n\
                   [=] Checking for known tags...\n\
                   [-] No known 125/134 kHz tags found!\n\
                   [=] Searching for auth LF and special cases...\n\
                   [+] Chipset... T55xx\n\
                   [?] Hint: Try `lf t55xx` commands\n";
        assert!(rejected_command(out).is_none());
    }

    /// A successful clone must not be flagged.
    #[test]
    fn accepts_successful_read() {
        let out = "[+] Page 0\n\
                   [+]  01 | 00000000 | 00000000000000000000000000000000 | ....\n";
        assert!(rejected_command(out).is_none());
    }

    // --- device_failure(): a dropped port must not read as an empty antenna ---

    use super::device_failure;

    /// Verbatim `hf search` output from log 937, after the orphaned `lf search`
    /// child took COM24. PM3 exits 1 here. This is the text that used to be
    /// handed to the parser as a successful result, producing "No card found".
    #[test]
    fn flags_invalid_serial_port() {
        let out = "[=] Session log .../log_20260929025937.txt\n\n\
                   [+] execute command from commandline: hf search\n\n\
                   [+] Using UART port COM24\n\n\
                   [!] ERROR: invalid serial port COM24\n\n\
                   [?] Hint: Try the shell script ``./pm3 --list` to get a list of possible serial ports\n";
        assert!(device_failure(out).is_some());
    }

    #[test]
    fn flags_port_open_failures() {
        for marker in [
            "Error opening serial port COM24",
            "Failed to open the serial port",
            "Cannot open the port COM24",
        ] {
            assert!(
                device_failure(marker).is_some(),
                "should have flagged: {}",
                marker
            );
        }
    }

    /// The real `lf search` T5577 output must NOT be treated as a device
    /// failure -- this is the case `tolerate_nonzero` exists to preserve, and it
    /// still exits -10. Flagging it here would reintroduce "Scan failed".
    #[test]
    fn accepts_t55xx_search_output() {
        let out = "[=] Checking for known tags...\n\
                   [-] No known 125/134 kHz tags found!\n\
                   [=] Searching for auth LF and special cases...\n\
                   [+] Chipset... T55xx\n\
                   [?] Hint: Try `lf t55xx` commands\n";
        assert!(device_failure(out).is_none());
    }

    /// The real iCLASS `hf search` output (exit 0) must not be flagged.
    #[test]
    fn accepts_iclass_search_output() {
        let out = "[+] iCLASS / Picopass CSN: EF 82 06 03 F8 FF 12 E0 \n\n\
                   [+] Valid iCLASS tag / PicoPass tag found\n";
        assert!(device_failure(out).is_none());
    }

    /// A genuinely empty antenna must not be flagged as a device failure --
    /// that is a real "no card", not a connection problem.
    #[test]
    fn accepts_empty_antenna_output() {
        let out = "[=] Checking for known tags...\n\
                   [-] No known/supported 13.56 MHz tags found!\n";
        assert!(device_failure(out).is_none());
    }

    /// The search timeout must exceed the measured runtime of the sweeps.
    /// `lf search` measured 9-10.5s and `hf search` 10.8s on this device; the
    /// previous 8s cap killed them mid-run and orphaned the child on COM24.
    #[test]
    fn search_timeout_exceeds_measured_sweep_duration() {
        use super::{DETECT_COMMAND_TIMEOUT, SEARCH_COMMAND_TIMEOUT};
        assert!(
            SEARCH_COMMAND_TIMEOUT > std::time::Duration::from_secs(11),
            "search sweeps take ~11s; {:?} is too short",
            SEARCH_COMMAND_TIMEOUT
        );
        // The quick probe timeout is a genuinely different operation (single
        // protocol, measured 1.9-3.8s) and is allowed to stay short.
        assert!(DETECT_COMMAND_TIMEOUT < SEARCH_COMMAND_TIMEOUT);
    }

    /// `lf t55xx chk` measured 26.8s on this hardware (123-password dictionary).
    /// It ran under the 30s `PM3_COMMAND_TIMEOUT` only by a 3s margin, and the
    /// session log showed it hitting the cap: the child was killed mid-attack
    /// and the next command failed with "invalid serial port". The long-command
    /// budget must comfortably exceed the measured runtime.
    #[test]
    fn long_timeout_exceeds_measured_dictionary_attack() {
        use super::{LONG_COMMAND_TIMEOUT, PM3_COMMAND_TIMEOUT};
        assert!(
            LONG_COMMAND_TIMEOUT > std::time::Duration::from_secs(60),
            "dictionary attacks take far longer than a panel probe; {:?} is too short",
            LONG_COMMAND_TIMEOUT
        );
        // It has to be strictly longer than the default, otherwise routing the
        // command is pointless.
        assert!(LONG_COMMAND_TIMEOUT > PM3_COMMAND_TIMEOUT);
    }

    /// The dictionary attacks must be routed to the long timeout, and ordinary
    /// probes must not be. A loose `contains` match would also catch
    /// "hf mf dump", so the boundary is asserted explicitly.
    #[test]
    fn classifies_long_running_commands() {
        use super::is_long_running;
        for cmd in [
            "lf t55xx chk",
            "lf t55xx bruteforce",
            "hf 15 bruteforce",
            "hf mf bruteforce",
            "hf mfu desbrute",
            "hf mfdes chk",
            "hf mfdes bruteaid",
            // Regression: the Mifare View panel's autopwn was routed to the
            // 30s default and killed mid-attack, while hf_clone.rs runs the
            // same command with a 3600s budget.
            "hf mf autopwn",
            "hf mf autopwn -f default",
        ] {
            assert!(is_long_running(cmd), "{} should use the long timeout", cmd);
        }
        for cmd in [
            "lf t55xx wakeup",
            "lf t55xx read -b 0",
            "hw version",
            "hf mf dump",
            "hf search",
            // Guards the prefix-match boundary: must NOT be treated as `chk`.
            "lf t55xx chkfoo",
            "lf t55xx read",
        ] {
            assert!(
                !is_long_running(cmd),
                "{} must not use the long timeout",
                cmd
            );
        }
    }
}
