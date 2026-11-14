# Phos5GUI - Detailed Implementation Plan (Corrected)

**Project:** Phos5GUI - Proxmark5-Specific Desktop GUI  
**Status:** Planning Phase (No code written yet)  
**Based on:** Phosphor-2.0-GUI (Tauri v2 + React 19 + TypeScript + Rust)  
**Target Hardware:** Proxmark5 (PM5) **primary focus** — PM3 support **not promised, not tested**  
**Development Environment:** Standalone PhosSpace Micro-Toolchain at `.build-env\toolchain\`

---

## 1. Executive Summary (Corrected)

Phos5GUI is a **fork** of Phosphor-2.0-GUI refocused on Proxmark5. We **do not remove PM3 code** — we keep the Phosphor architecture intact but:
- **Default/only tested firmware**: PM5 (standard + BWM variants)
- **PM3 code paths remain** but are untested/unsupported (user assumes risk)
- **Version mediation** works for both PM3 and PM5, but PM5 is the priority

**Core Problem:** The `proxmark3.exe` client binary **hard-fails** on `CAPABILITIES_VERSION` mismatch in `client/src/comms.c:942-947`. Tauri cannot query device capabilities through a client that refuses to connect.

**Solution Architecture:**
1. **Direct Rust Serial Probe** — Tauri opens COM port directly via `serialport-rs`, sends raw `CMD_PING` + `CMD_CAPABILITIES` (NG frames) **before** launching `proxmark3.exe`
2. **Error Message Scraping** — Fallback: parse stderr from failed `proxmark3.exe` connection attempt
3. **Strict Version Matching (NO relaxed patch)** — Firmware MUST strictly match the client. `CAPABILITIES_VERSION` mismatch remains a **fatal safety exit**. The GUI enforces flashing bundled matching firmware on mismatch. **No `comms_relaxed_version.patch` is applied.** `comms.c` is **not** patched to relax the version check.
4. **Bootloader-Aware Flashing** — Separate flash flow using `pm3-flash-all` / bootloader mode

---

## 2. Current Architecture Analysis (Unchanged)

### 2.1 Phosphor-2.0-GUI Stack
| Layer | Technology | Purpose |
|-------|------------|---------|
| Frontend | React 19 + TypeScript + Vite | Wizard UI, Advanced Tools tabs |
| Backend | Tauri v2 (Rust) | PM3 process management, command builder, output parser |
| Client Binary | `proxmark3.exe` (C, built in ProxSpace) | USB/CDC communication with device |
| Firmware Bundles | `fullimage.elf` per variant | Flashed to device via `hw flash` |

### 2.2 Version Mismatch Mechanism (Root Cause)
**File:** `D:\proxspace\ProxSpace-master\pm3\proxmark3\include\pm3_cmd.h:296`
```c
#define CAPABILITIES_VERSION 13
```

**Client Check:** `client/src/comms.c:942-947` — **HARD FAIL**
```c
if ((resp.length != sizeof(g_pm3_capabilities)) ||
    (resp.data.asBytes[0] != CAPABILITIES_VERSION)) {
    PrintAndLogEx(ERR, "Capabilities structure version sent by Proxmark3 is not the one expected by this client! (v%u != v%u)");
    return PM3_EDEVNOTSUPP;  // <-- Client exits, no capabilities returned
}
```

**PM5-Specific Capabilities (pm3_cmd.h:269-292):**
```c
bool hw_available_fpga_flash : 1;
bool hw_available_i2c_eeprom : 1;
bool is_pm5 : 1;
bool is_pm5_std_ant : 1;
uint16_t max_cmd_data_size;      // v9
uint16_t em_size;                // v11
bool em_allocated : 1;           // v11
bool compiled_with_bwm : 1;      // v12
bool compiled_with_cep : 1;      // v13
```

---

## 3. Phos5GUI Architecture (Corrected)

### 3.1 Variant Model — PM5 Primary, PM3 Retained (Untested)
| Aspect | Phosphor | Phos5GUI |
|--------|----------|----------|
| Firmware variants shipped | 4 (rdv4, rdv4-bt, generic, generic-256) | **2 PM5 (standard, bwm)** + PM3 variants kept but **hidden/disabled by default** |
| Default device | User selects | **PM5 auto-detected first** |
| CAPABILITIES_VERSION | Per-variant | **Single per release** (PM5) |
| Bootrom handling | Separate | **Unified PM5 image** |

### 3.2 Directory Structure
```
phos5gui/
├── src/                          # React 19 + TypeScript frontend
│   ├── components/               # Reusable UI components
│   ├── pages/                    # Wizard steps, Advanced Tools
│   ├── state/                    # XState v5 machines (Wizard, Connection)
│   ├── hooks/                    # Custom React hooks
│   └── utils/                    # Helpers, constants
├── src-tauri/                    # Rust backend (Tauri v2)
│   ├── src/
│   │   ├── pm5/                  # PM5-specific modules
│   │   │   ├── capabilities.rs   # CAPABILITIES_VERSION handling
│   │   │   ├── commands.rs       # PM5 command builders (BWM, CEP, FPGA, RGB)
│   │   │   ├── firmware.rs       # PM5 firmware management
│   │   │   ├── version.rs        # PM5 version parsing & mediation
│   │   │   └── device.rs         # PM5 device detection & info
│   │   ├── probe/                # NEW: Direct serial capability probe
│   │   │   ├── mod.rs
│   │   │   ├── ng_protocol.rs    # Raw NG frame encoding/decoding
│   │   │   └── capabilities.rs   # Parse capabilities_t from raw bytes
│   │   ├── flasher/              # NEW: Bootloader flash automation (USB-CDC flasher)
│   │   │   ├── mod.rs
│   │   │   ├── bootloader.rs     # Enter/exit bootloader via hardware button or CMD_START_FLASH
│   │   │   └── client_flasher.rs # Invoke proxmark3.exe --flash / pm3-flash-all
│   │   ├── client_patch/         # NEW: Patched client build
│   │   │   ├── mod.rs
│   │   │   └── comms_patch.c     # comms.c with relaxed version check
│   │   ├── connection.rs         # Unified PM5/PM3 connection manager
│   │   ├── output_parser.rs      # PM5 output parsing
│   │   ├── command_builder.rs    # Type-safe command construction
│   │   ├── state.rs              # Backend state machines
│   │   ├── error.rs              # Error types
│   │   ├── logging.rs            # Session logging
│   │   ├── lib.rs                # Tauri entry point
│   │   └── main.rs               # App initialization
│   ├── binaries/
│   │   ├── pm5/                  # Bundled PM5 client + DLLs
│   │   │   ├── proxmark3.exe     # PM5-built client (PATCHED)
│   │   │   └── *.dll             # Runtime dependencies
│   │   └── pm3/                  # Original PM3 clients (kept for compat)
│   ├── firmware/
│   │   ├── pm5/
│   │   │   ├── standard/
│   │   │   │   ├── bootrom.elf
│   │   │   │   └── fullimage.elf
│   │   │   └── bwm/
│   │   │       ├── bootrom.elf
│   │   │       └── fullimage.elf
│   │   └── pm3/                  # Original PM3 firmware (kept)
│   ├── capabilities/
│   │   └── pm5.json              # Expected CAPABILITIES_VERSION + features
│   ├── tauri.conf.json
│   ├── build.rs
│   └── Cargo.toml
├── scripts/                      # Build automation
│   ├── portable-runtime/
│   │   └── setup-micro-toolchain.ps1   # Bootstraps seeded headless engine
│   ├── sandbox/                    # Build scripts for .build-env/pm3-src
│   │   ├── build-sandboxed-client.sh
│   │   ├── build-sandboxed-firmware.sh
│   │   └── apply-patches.sh
│   ├── windows/                    # Seeded PowerShell wrappers
│   │   └── build-sandboxed-client.ps1
│   └── release.sh                  # Creates distributable package
│       ├── build-pm5-client.ps1
│       ├── build-pm5-firmware.ps1
│       └── build-patched-client.ps1
├── package.json
├── vite.config.ts
├── tsconfig.json
└── README.md
```

---

## 4. Client/Firmware Version Mediation — Technical Specification

### 4.1 The Core Problem (Restated)
```
┌─────────────────────────────────────────────────────────────────┐
│  Tauri wants to know device CAPABILITIES_VERSION                │
│                                                                 │
│  proxmark3.exe (bundled) connects →                             │
│    sends CMD_PING → gets response →                             │
│    sends CMD_CAPABILITIES → gets capabilities_t →               │
│    CHECK: capabilities_t.version == CAPABILITIES_VERSION?       │
│      NO  → PrintAndLogEx(ERR, ...) → return PM3_EDEVNOTSUPP     │
│            Client EXITS. Tauri gets NO capabilities data.       │
│      YES → Client continues, Tauri can run commands.            │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Solution: Three-Layer Capability Detection

#### Layer 1: Direct Rust Serial Probe (Primary)
**File:** `src-tauri/src/probe/ng_protocol.rs`

Tauri opens the COM port directly using `serialport-rs`, constructs raw NG frames, and speaks the Proxmark3 protocol **without** the C client binary.

**Critical Windows Constraint:** Serial ports on Windows do NOT support shared access. The probe MUST explicitly drop the `SerialPort` handle and wait 100-200ms before Tauri spawns `proxmark3.exe`, otherwise the child process fails with `ERROR_ACCESS_DENIED` (PM3_EACCESS).

```rust
// Raw NG frame structures (mirror pm3_cmd.h)
#[repr(C, packed)]
struct PacketCommandNGPreamble {
    magic: u32,           // 0x61334d50 ("PM3a")
    length: u16,          // 15 bits length + 1 bit ng flag
    ng: bool,
    cmd: u16,
}

#[repr(C, packed)]
struct PacketCommandNGPostamble {
    crc: u16,             // CRC or 0x3362 (magic)
}

const CMD_PING: u16 = 0x0109;
const CMD_CAPABILITIES: u16 = 0x0112;
const RESPONSENG_PREAMBLE_MAGIC: u32 = 0x62334d50; // "PM3b"

pub struct ProbeResult {
    pub capabilities_version: u8,
    pub capabilities: Option<CapabilitiesStruct>,  // parsed if version matches our parser
    pub max_cmd_data_size: u16,
    pub is_pm5: bool,
    pub method: ProbeMethod,
}

#[derive(Debug, Clone, Copy)]
pub enum ProbeMethod {
    DirectSerial,      // Layer 1 success
    StderrScraping,    // Layer 2 success
    Failed,            // All layers failed
}

pub async fn probe_device_capabilities(port: &str, baudrate: u32) -> Result<ProbeResult> {
    // 1. Open serial port directly
    let mut port = serialport::new(port, baudrate)
        .timeout(Duration::from_millis(2000))
        .open()?;

    // 2. Send CMD_PING (NG frame)
    let ping_payload = [0u8; 32]; // 32 bytes of incrementing pattern
    let ping_frame = build_ng_frame(CMD_PING, &ping_payload);
    port.write_all(&ping_frame)?;

    // 3. Read response, verify PING echo
    let ping_resp = read_ng_response(&mut port)?;
    if !ping_payload.eq(&ping_resp.data[..32]) {
        return Err(Error::ProbeFailed("PING echo mismatch"));
    }

    // 4. Send CMD_CAPABILITIES (no payload)
    let caps_frame = build_ng_frame(CMD_CAPABILITIES, &[]);
    port.write_all(&caps_frame)?;

    // 5. Read capabilities response
    let caps_resp = read_ng_response(&mut port)?;
    
    // 6. Parse capabilities_t from response
    // First byte = version, rest = struct (truncated to our known size)
    let version = caps_resp.data[0];
    
    // 7. Parse known fields (up to our CAPABILITIES_VERSION)
    let caps = parse_capabilities_struct(&caps_resp.data);
    
    // CRITICAL: Explicitly drop the port handle BEFORE returning
    // This releases the COM port lock on Windows
    drop(port);
    
    // CRITICAL: Brief delay to let Windows release the port handle fully
    // Before Tauri spawns proxmark3.exe
    tokio::time::sleep(Duration::from_millis(150)).await;
    
    Ok(ProbeResult {
        capabilities_version: version,
        capabilities: caps,
        max_cmd_data_size: caps.as_ref().map(|c| c.max_cmd_data_size).unwrap_or(512),
        is_pm5: caps.as_ref().map(|c| c.is_pm5).unwrap_or(false),
        method: ProbeMethod::DirectSerial,
    })
}

fn build_ng_frame(cmd: u16, payload: &[u8]) -> Vec<u8> {
    // ... construct NG frame with preamble, payload, CRC/postamble
}

fn read_ng_response(port: &mut dyn SerialPort) -> Result<NgResponse> {
    // ... read preamble, payload, postamble, verify CRC
}
```

**Key Point:** This probe runs **before** any `proxmark3.exe` spawn. It uses only the serial port and the NG protocol spec from `pm3_cmd.h`. The `drop(port)` + `sleep(150ms)` is mandatory on Windows.

#### Layer 2: Error Message Scraping (Fallback)
If Layer 1 fails (permission issues, port locked, etc.), spawn the bundled `proxmark3.exe` in one-shot mode and capture stderr:

```rust
// In connection.rs - modified check_firmware_version
pub async fn check_firmware_version_with_fallback(
    app: &AppHandle,
    port: &str,
) -> Result<FirmwareCheckResult> {
    // Try Layer 1 first
    if let Ok(probe_result) = probe::probe_device_capabilities(port, 9600).await {
        return Ok(convert_probe_result(probe_result));
    }

    // Layer 2: Run bundled client, capture stderr
    let output = run_client_oneshot(port, "hw version").await;
    
    // Parse stderr for: "Capabilities structure version sent by Proxmark3 is not the one expected by this client! (v13 != v11)"
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some((device_ver, client_ver)) = parse_capabilities_mismatch(&stderr) {
        return Ok(FirmwareCheckResult {
            matched: false,
            client_version: client_ver.to_string(),
            device_firmware_version: device_ver.to_string(),
            hardware_variant: detect_variant_from_stderr(&stderr),
            firmware_path_exists: false,
            probe_method: ProbeMethod::StderrScraping,
        });
    }

    // If hw version succeeded (match), parse normally
    let info = parse_detailed_hw_version(&output.stdout);
    // ...
}
```

**Regex for stderr scraping:**
```rust
static CAPABILITIES_MISMATCH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Capabilities structure version sent by Proxmark3 is not the one expected by this client! \(v(\d+) != v(\d+)\)")
        .expect("bad regex")
});
// Group 1 = device version, Group 2 = client version
```

#### Layer 3: Strict Version Matching (No Patched Client)
**Status: DISABLED — Relaxed comms.c patching is NOT used.**

Relaxed capabilities patching (`comms_relaxed_version.patch`) is **disabled** for hardware safety. Firmware MUST strictly match the client. `CAPABILITIES_VERSION` mismatch remains a **fatal safety exit** in `client/src/comms.c`. The GUI does **not** apply any relaxed patch to `comms.c`.

**Mismatch handling:** When `device_ver != client_ver`, the connection state machine transitions to `version_mismatch_prompt`. The user must flash the bundled matching firmware (standard or BWM variant) before the device can be used. There is no "continue in compatibility mode" bypass.

```rust
// client/src/comms.c — UNPATCHED, fatal check remains
if ((resp.length != sizeof(g_pm3_capabilities)) ||
    (resp.data.asBytes[0] != CAPABILITIES_VERSION)) {
    PrintAndLogEx(ERR, "Capabilities structure version sent by Proxmark3 is not the one expected by this client! (v%u != v%u)");
    return PM3_EDEVNOTSUPP;  // <-- Client exits, no capabilities returned
}
```

### 4.3 Revised Mediation Strategy — "Stale Hardware" Decision Engine

**Flow:**
```
[PM5 Connected -> Layer 1 Serial Probe]
                 │
                 ▼
     [Compare Capabilities Version]
                 │
  ┌──────────────┼──────────────────────────────┐
  ▼              ▼                              ▼
[Device == Client]  [Device < Client]           [Device > Client]
Perfect Match       Outdated Hardware Firmware  Outdated Desktop Client
(Green Status)      (e.g., Device v11 vs v13)   (e.g., Device v14 vs v13)
```

#### State A: Outdated Device Firmware (`device_ver < client_ver`)
**Trigger:** Device firmware capabilities version is behind the bundled client.

**Modal Title:** "Outdated Device Firmware Detected"

**Description:** "Your Proxmark5 is running firmware capabilities v{device_ver}, but this client expects v{client_ver}. Some commands and features may be unavailable or unstable."

| Action | Type | Implementation |
|--------|------|----------------|
| **Update PM5 Firmware** | Primary (Recommended) | Launches built-in USB-CDC flash flow using bundled `bootrom.elf` + `fullimage.elf` via `proxmark3.exe --flash --unlock-bootloader` |
| **Continue in Compatibility Mode** | Secondary | Spawns patched client binary. Gates unsupported features: disables BWM panel if `device_ver < 12`, disables CEP panel if `device_ver < 13`. Shows persistent status bar banner: "Running in degraded mode (Device v{device_ver})" |

#### State B: Outdated Desktop Client (`device_ver > client_ver`)
**Trigger:** User updated PM5 firmware via ProxSpace, but runs older Phos5GUI release.

**Modal Title:** "Outdated Desktop Client Detected"

**Description:** "Your Proxmark5 is running newer firmware capabilities (v{device_ver}) than this Phos5GUI release (v{client_ver})."

| Action | Type | Implementation |
|--------|------|----------------|
| **Use Local ProxSpace Client** | Primary | Opens file picker / auto-detects `D:\proxspace\ProxSpace-master\pm3\proxmark3\client\proxmark3.exe` to override bundled client |
| **Rebuild Matching Client** | Secondary (if ProxSpace detected) | Invokes `scripts/windows/build-pm5-client.ps1` to recompile client matching device |
| **Downflash Firmware** | Secondary | Flashes bundled stable firmware pair so hardware matches current GUI version |
| **Continue Anyway** | Tertiary | Proceeds with patched client ignoring trailing extra capability fields |

#### State C: Exact Match (`device_ver == client_ver`)
**Status:** Green indicator in connection bar. Normal operation with bundled patched client.

#### Structured Enum (Rust → Frontend)
```rust
// src-tauri/src/pm5/capabilities.rs
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", content = "detail")]
pub enum VersionMatchStatus {
    Matched,
    DeviceOutdated { device_ver: u8, client_ver: u8 },
    ClientOutdated { device_ver: u8, client_ver: u8 },
    ProbeFailed { reason: String },
}
```

#### Mediation Table (Replaces previous table)

| Scenario | Detection Layer | `VersionMatchStatus` | Primary Action |
|----------|-----------------|---------------------|----------------|
| **Exact match** | Layer 1 or 2 | `Matched` | Normal operation with bundled patched client |
| **Device older by 1–2** (appended fields) | Layer 1 | `DeviceOutdated` | Show State A modal → "Update PM5 Firmware" |
| **Device older by >2** | Layer 1 | `DeviceOutdated` | Show State A modal (stronger warning) |
| **Device newer by 1** | Layer 1 | `ClientOutdated` | Show State B modal → "Use Local Client" |
| **Device newer by >1** | Layer 1 | `ClientOutdated` | Show State B modal → "Rebuild" or "Downflash" |
| **Probe fails entirely** | Layer 2 (stderr) | `ProbeFailed` | Extract versions from error; same logic |
| **All detection fails** | — | `ProbeFailed` | "Cannot detect device version — check connection" |

### 4.4 Capabilities Manifest (`capabilities/pm5.json`)
```json
{
  "capabilities_version": 13,
  "commit_hash": "a1b2c3d4e5f6...",
  "build_date": "2026-11-10T16:00:00Z",
  "variants": {
    "standard": {
      "firmware_path": "firmware/pm5/standard/fullimage.elf",
      "features": {
        "compiled_with_bwm": false,
        "compiled_with_cep": true,
        "hw_available_fpga_flash": true,
        "hw_available_i2c_eeprom": true,
        "is_pm5_std_ant": true
      }
    },
    "bwm": {
      "firmware_path": "firmware/pm5/bwm/fullimage.elf",
      "features": {
        "compiled_with_bwm": true,
        "compiled_with_cep": true,
        "hw_available_fpga_flash": true,
        "hw_available_i2c_eeprom": true,
        "is_pm5_std_ant": true
      }
    }
  },
  "patched_client": true,
  "min_client_version": "2026-11-01",
  "max_client_version": "2026-11-10"
}
```

---

## 5. Flashing Mechanism — PM5/AT32F437 Specific (Corrected)

### 5.1 The Problem (Corrected)
- PM5 uses AT32F437 MCU. **Holding the button does NOT enter Artery DFU ROM (2E3C:DF11).**
- It enters the **Proxmark Bootloader**, which presents as the **same USB-CDC / COM port** serial device.
- Flashing is performed by the **PM3 client flasher itself** — no dfu-util, no WinUSB/Zadig drivers.
- `proxmark3.exe --flash` does **not** check `CAPABILITIES_VERSION` — it's a separate low-level bootloader routine.
- If client refuses to connect (version mismatch), `hw bootloader` cannot be sent via normal client path.
- Device must enter bootloader via **hardware button** (hold during plug-in), then flasher runs over the same COM port.

### 5.2 Flashing Workflow (Automated — USB-CDC Flasher)

```rust
// src-tauri/src/flasher/bootloader.rs

pub enum BootloaderEntryMethod {
    HardwareButton,    // User holds button during plug-in (primary, always works)
    SoftwareCommand,   // Send CMD_START_FLASH via working client (fallback)
    AutoDetect,        // Try software first, fall back to hardware prompt
}

pub async fn enter_bootloader(port: &str, method: BootloaderEntryMethod) -> Result<BootloaderInfo> {
    match method {
        BootloaderEntryMethod::SoftwareCommand => {
            // Use patched client (which connects despite mismatch)
            let output = run_client_oneshot(port, "hw bootloader").await?;
            // Device reboots into bootloader mode, COM port re-enumerates (same VID/PID)
            wait_for_bootloader_port(port).await?;
        }
        BootloaderEntryMethod::HardwareButton => {
            // Emit UI event with exact instructions
            emit_bootloader_instructions(app)?;
            wait_for_bootloader_port(port).await?;
        }
        BootloaderEntryMethod::AutoDetect => {
            if try_software_bootloader(port).await.is_err() {
                enter_bootloader(port, BootloaderEntryMethod::HardwareButton).await?;
            }
        }
    }
    Ok(detect_bootloader_port(port))
}

/// Flash using the standard Proxmark flasher over USB-CDC
/// This is the ONLY supported method for PM5 — no dfu-util, no DFU mode.
pub async fn flash_via_client_flasher(
    port: &str,
    bootrom_path: &Path,
    fullimage_path: &Path,
) -> Result<()> {
    // Use proxmark3.exe --flash (bundled patched client)
    // The flasher protocol does NOT check CAPABILITIES_VERSION
    let output = Command::new("proxmark3.exe")
        .args([
            port,
            "--flash",
            "--unlock-bootloader",
            "--image", bootrom_path.to_str().unwrap(),
            "--image", fullimage_path.to_str().unwrap(),
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Fallback: try pm3-flash-all if available
        return try_pm3_flash_all(port, bootrom_path, fullimage_path).await
            .or(Err(Error::FlashFailed(stderr.to_string())));
    }

    // Wait for device to re-enumerate as PM5 application
    wait_for_pm5_enumeration(port).await?;
    Ok(())
}

async fn try_pm3_flash_all(port: &str, bootrom_path: &Path, fullimage_path: &Path) -> Result<()> {
    let output = Command::new("pm3-flash-all.exe")
        .args([port, bootrom_path.to_str().unwrap(), fullimage_path.to_str().unwrap()])
        .output()
        .await?;
    
    if output.status.success() {
        wait_for_pm5_enumeration(port).await?;
        Ok(())
    } else {
        Err(Error::FlashFailed(String::from_utf8_lossy(&output.stderr).to_string()))
    }
}
```

### 5.3 Bundled Flash Tools
- **`proxmark3.exe`** (patched) — Primary flasher via `--flash --unlock-bootloader --image bootrom.elf --image fullimage.elf`
- **`pm3-flash-all.exe`** — Fallback, built from ProxSpace `tools/pm3-flash-all.c`
- **NO dfu-util, NO Zadig, NO WinUSB drivers** — USB-CDC flasher uses the same COM port

### 5.4 Flash UI Flow (Corrected)
```
User clicks "Flash PM5 Firmware"
       │
       ▼
┌────────────────────────┐
│ Select variant:        │
│ [PM5 Standard] [PM5 BWM]│
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ "Put PM5 in bootloader:│
│ 1. Unplug USB          │
│ 2. Hold PM5 button     │
│ 3. Plug USB (hold 2s)  │
│ 4. Release button      │
│ 5. Click Continue      │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ Flashing bootrom.elf   │
│ [=====] 25%           │
│ (proxmark3.exe --flash)│
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ Flashing fullimage.elf │
│ [==========] 75%      │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ Flash complete!        │
│ Device will reboot.    │
│ Verifying version...   │
└────────────────────────┘
```

**Key Point:** Both `bootrom.elf` and `fullimage.elf` are required. The build scripts must copy BOTH from ProxSpace (`armsrc/obj/bootrom.elf` + `armsrc/obj/fullimage.elf`).

---

## 6. Build Automation (Superseded by Section 14)

**Superseded by Section 14 (PhosSpace Standalone Micro-Toolchain & 1-Click Update Pipeline).** All build execution is performed via the seeded local engine at `.build-env\toolchain\`. See Section 14 for the build script and execution specs.

---

## 7. Execution Model — Persistent Process for Telemetry

### 7.1 Phosphor's Current Model (One-Shot)
```rust
// connection.rs - current
proxmark3.exe <PORT> -c "hf 14a info"  // New process per command
```
**Problem:** Spawning process per command drops USB connection, fails for polling (battery, sniffing).

### 7.2 Phos5GUI Model: Persistent Interactive Process
```rust
// connection.rs - NEW
struct Pm5Session {
    child: Child,           // Long-running proxmark3.exe
    stdin: ChildStdin,      // Write commands
    stdout: ChildStdout,    // Read responses
    stderr: ChildStderr,    // Capture errors
    port_lock: Mutex<()>,   // Serialize access
}

// Start persistent session
async fn start_session(port: &str) -> Result<Pm5Session> {
    let mut child = Command::new("proxmark3.exe")
        .args([port])  // Interactive mode, no -c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for prompt
    wait_for_prompt(&mut child.stdout).await?;
    
    Ok(Pm5Session { child, ... })
}

// Send command to persistent session
async fn send_command(session: &Pm5Session, cmd: &str) -> Result<String> {
    let _lock = session.port_lock.lock().await;
    writeln!(session.stdin, "{}", cmd).await?;
    session.stdin.flush().await?;
    
    // Read until prompt returns
    read_until_prompt(&mut session.stdout).await
}
```

**Use Cases:**
- **Battery polling** (`CMD_PM5_BWM_GET_BATTERY` every 5s) → persistent session
- **FPGA bitstream streaming** → persistent session
- **One-shot commands** (scan, clone) → can use persistent or one-shot

---

## 8. PM5-Specific Feature Set (Unchanged, but Now Buildable)

### 8.1 PM5 Commands Mapping
| Command | NG Opcode | Probe Layer 1 Support | Patched Client Required |
|---------|-----------|----------------------|------------------------|
| `CMD_ANT_CONTROL_WRITE/READ` | 0x0170/0x0171 | ✅ | ✅ |
| `CMD_EEPROM_FACTORY_INFO_READ/WRITE` | 0x0172/0x0173 | ✅ | ✅ |
| `CMD_PM5_QC_TEST_IO/HW` | 0x0176/0x0177 | ✅ | ✅ |
| `CMD_PM5_RGB_SET` | 0x0178 | ✅ | ✅ |
| `CMD_PM5_BWM_*` | 0x0179-0x0184 | ✅ | ✅ |
| `CMD_PM5_POWERSAVE` | 0x0180 | ✅ | ✅ |
| `CMD_CEP_STATUS` | 0x0185 | ✅ | ✅ |
| `CMD_FPGA_BITSTREAM_*` | 0x0501-0x0503 | ✅ | ✅ |

### 8.2 UI Panels (PM5 Priority)
| Panel | Status | Commands |
|-------|--------|----------|
| **Battery & Wireless (BWM)** | Primary | All `CMD_PM5_BWM_*` |
| **Flipper Zero (CEP)** | Primary | `CMD_CEP_STATUS`, CEP handshake |
| **FPGA Management** | Primary | `CMD_FPGA_BITSTREAM_*`, `CMD_PM5_FPGA_SET_PWR_PWM_LOW_COUNT` |
| **Antenna RGB** | Primary | `CMD_PM5_RGB_SET` |
| **Power Management** | Primary | `CMD_PM5_POWERSAVE`, `CMD_PM5_BWM_AUTOOFF` |
| **Device Identity** | Primary | `CMD_EEPROM_FACTORY_INFO_*` |
| **Hardware Diagnostics** | Primary | `CMD_PM5_QC_TEST_*` |
| **Dual-Frequency Antenna** | Primary | `CMD_ANT_CONTROL_*` |

---

## 9. Migration from Phosphor-2.0-GUI (Corrected)

### 9.1 Files to MODIFY (Not Delete)
| File | Change |
|------|--------|
| `src-tauri/src/pm3/connection.rs` | Add `start_persistent_session()`, Layer 1 probe integration |
| `src-tauri/src/pm3/version.rs` | Add PM5 `hw version` parsing; keep PM3 parsing |
| `src-tauri/src/commands/firmware.rs` | Add bootloader flash flow; keep PM3 flash |
| `src-tauri/src/pm3/command_builder.rs` | Add PM5 command builders; keep PM3 |
| `src-tauri/src/pm3/output_parser.rs` | Add PM5 output regexes; keep PM3 |
| `src-tauri/capabilities/default.json` | **Keep** — PM3 variants; add `pm5.json` |
| `src/components/DeviceSelector.tsx` | Default to PM5; keep PM3 options (disabled) |

### 9.2 Files to ADD
```
src-tauri/src/probe/           # Direct serial capability probe
src-tauri/src/flasher/         # Bootloader/DFU flash automation
src-tauri/src/client_patch/    # Patched client build integration
src-tauri/src/pm5/capabilities.rs  # VersionMatchStatus enum + mediation logic
scripts/build-patched-client.sh
scripts/client_patch/comms_relaxed_version.patch

# Frontend: Version Mismatch Mediation UX
src/state/connectionMachine.ts   # XState actor: probing -> evaluating_version -> version_mismatch_prompt -> flashing | spawning_session
src/components/modals/
  ├── VersionMismatchModal.tsx        # Main modal with State A/State B action buttons
  ├── OutdatedDeviceFirmwareModal.tsx # State A specific UI
  ├── OutdatedDesktopClientModal.tsx  # State B specific UI
  └── index.ts                        # Export barrel
src/hooks/useVersionMediation.ts      # Hook: fetches VersionMatchStatus, opens modal, handles actions
```

### 9.3 Files to KEEP As-Is
- Tauri/React/Vite/XState configuration
- Wizard clone flow (LF/HF card cloning unchanged)
- Logging, error handling infrastructure
- Basic UI components

---

## 10. Risk Assessment & Mitigation (Updated)

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **Direct serial probe fails on Windows** (port permissions, exclusive lock) | High | High | Layer 2 fallback (stderr scraping); explicit `drop(port)` + 150ms delay; document "Run as Admin" for probe |
| **Patched client causes instability** (mismatched struct access) | Medium | High | Only relax version check; zero unknown fields; test extensively |
| **USB-CDC flash fails** (bootloader entry, flasher protocol) | Medium | Critical | Hardware button entry (always works); fallback to `pm3-flash-all`; no dfu-util/Zadig dependency |
| **MSYS2 build scripts break** (path conversion, env vars) | Medium | High | Test on clean Windows; CI/CD on GitHub Actions with MSYS2; cap at `make -j4` |
| **Persistent session deadlocks** (stdin/stdout buffering) | Medium | High | Use `tokio::io::AsyncBufRead`; timeouts; session restart on failure |
| **Proxmark3 CAPABILITIES_VERSION changes weekly** | High | Medium | Automated daily ProxSpace sync; patched client absorbs minor changes |

---

## 11. Immediate Next Steps (Corrected)

### Phase 1: Foundation (Week 1-2)
1. **Initialize Phos5GUI repo** at `D:\kilocode\phos5gui`
2. **Copy Phosphor-2.0-GUI** as starting point (keep ALL PM3 code)
3. **Create `probe/` module** — implement `ng_protocol.rs` with raw NG frame encode/decode
4. **Create `flasher/` module** — implement `bootloader.rs` with DFU detection
5. **Create `client_patch/` module** — generate patch file, integrate into build
6. **Write MSYS2 build scripts** (`build-pm5-client.sh`, `build-patched-client.sh`)
7. **Write Windows wrappers** (`.ps1` calling MSYS2 bash)
8. **Verify seeded toolchain** — confirm `.build-env\toolchain\usr\bin\bash.exe` cleanly invokes `make --version` and `arm-none-eabi-gcc --version` (planning/documentation only; do not execute now)

### Phase 2: Core Communication (Week 2-3)
9. **Integrate Layer 1 probe** into `connection.rs` — call before spawning client
10. **Implement Layer 2 fallback** — stderr scraping regex for capabilities mismatch
11. **Build first patched PM5 client** from ProxSpace, verify it connects to mismatched device
12. **Test direct serial probe** on physical PM5 (standard + BWM)

### Phase 3: Version Mediation (Week 3-4)
13. **Implement capability comparison logic** in `src-tauri/src/pm5/capabilities.rs` — returns `VersionMatchStatus` enum
14. **Build `VersionMismatchModal.tsx`** — main modal component rendering State A (Outdated Device) or State B (Outdated Client) action sets
15. **Wire modal actions to Tauri commands**:
    - "Update PM5 Firmware" → `flasher::flash_via_client_flasher()` with bundled `bootrom.elf` + `fullimage.elf`
    - "Use Local ProxSpace Client" → persists override path in config, restarts persistent session
    - "Rebuild Matching Client" → spawns `scripts/windows/build-pm5-client.ps1`, hot-swaps binary
    - "Downflash Firmware" → same flasher flow with current bundled firmware
    - "Continue in Compatibility Mode" / "Continue Anyway" → spawns patched client with feature gating
16. **Implement feature gating** — `UiFeatureSet` derived from `VersionMatchStatus`; disables BWM panel if `device_ver < 12`, CEP if `device_ver < 13`, etc.
17. **Add persistent status banner** — "Running in degraded mode (Device v{device_ver})" when compatibility mode active
18. **Test all mismatch scenarios**: device older by 1/2/3+, device newer by 1/2+, probe failure fallbacks

### Phase 4: PM5 Features (Week 4-6)
19. **Add BWM panel** with persistent session for battery polling
20. **Add CEP panel** for Flipper Zero
21. **Add FPGA Management panel** with bitstream upload
22. **Add remaining PM5 panels** (RGB, Power, Factory Info, QC, Dual Antenna)
23. **Verify PM3 paths still compile** (disabled by default)

### Phase 5: Polish & Release (Week 6-8)
24. **Automated CI/CD** (GitHub Actions: MSYS2 build → Tauri build → artifacts)
25. **Portable ZIP + NSIS installer** with bundled `pm3-flash-all.exe`
26. **Documentation**: PM5 flashing guide, local client override, troubleshooting
27. **Beta testing** with PM5 hardware (standard + BWM)
28. **v1.0.0 release**

---

## 12. Appendix: Key Protocol Specs for Implementation

### 12.1 NG Frame Format (from `pm3_cmd.h`)
```c
// Request (Host -> Device)
struct PacketCommandNGPreamble {
    uint32_t magic;        // 0x61334d50 ("PM3a")
    uint16_t length : 15;  // payload length
    bool ng : 1;           // always 1
    uint16_t cmd;          // command code
};
// payload[length]
struct PacketCommandNGPostamble {
    uint16_t crc;          // CRC16 or 0x3362 (magic)
};

// Response (Device -> Host)
struct PacketResponseNGPreamble {
    uint32_t magic;        // 0x62334d50 ("PM3b")
    uint16_t length : 15;
    bool ng : 1;
    int8_t status;         // PM3_SUCCESS=0, PM3_EDEVNOTSUPP=-2, etc.
    int8_t reason;
    uint16_t cmd;
};
// payload[length]
struct PacketResponseNGPostamble {
    uint16_t crc;
};
```

### 12.2 Critical Command Codes
| Command | Code | Purpose |
|---------|------|---------|
| `CMD_PING` | 0x0109 | Connectivity test, echo payload |
| `CMD_CAPABILITIES` | 0x0112 | Get `capabilities_t` struct |
| `CMD_VERSION` | 0x0107 | Get version strings (bootrom, OS, FPGA) |
| `CMD_HARDWARE_RESET` | 0x0004 | Reboot device |
| `CMD_START_FLASH` | 0x0005 | Enter bootloader (software) |
| `CMD_PM5_BWM_GET_BATTERY` | 0x0184 | Battery telemetry |

### 12.3 PM5 Bootloader Info (USB-CDC, NOT DFU)
- **Interface** — Same USB-CDC COM port as application firmware
- **Entry** — Hold PM5 button during USB plug-in (hardware) OR send `CMD_START_FLASH` via client (software)
- **Flasher** — `proxmark3.exe --flash --unlock-bootloader --image bootrom.elf --image fullimage.elf`
- **Fallback Flasher** — `pm3-flash-all.exe <port> <bootrom.elf> <fullimage.elf>`
- **Files Required** — BOTH `bootrom.elf` AND `fullimage.elf` from `armsrc/obj/`

---
## 14. PhosSpace Standalone Micro-Toolchain & 1-Click Update Pipeline

### 14.1 "PhosSpace" Standalone Micro-Toolchain (Zero-Dependency Engine)

Phos5GUI must be completely autonomous — no requirement that the user install ProxSpace, run `runme64.bat`, or touch a terminal. All build tooling ships as a self-contained micro-toolchain seeded inside the project root. All references and fallbacks pointing to `D:\proxspace` are removed; the host ProxSpace source tree is never read, written, or locked.

#### Headless Build Bundle (~120–150 MB)
A minimal, portable compiler distribution containing strictly what PM5 requires:
- `bash.exe` (MSYS2 runtime shell)
- `make.exe` (GNU Make)
- MinGW-w64 x64 `gcc.exe` (host tools / client compile)
- `arm-none-eabi-gcc.exe` + ARM toolchain libraries (firmware compile)

This bundle is seeded to `.build-env/toolchain/` (~120–150 MB).

#### Bootstrap Script: `scripts/portable-runtime/setup-micro-toolchain.ps1`
An automated PowerShell bootstrap that silently downloads and extracts the headless engine to `.build-env/toolchain/` on first build when local compiling is invoked. No user terminal interaction required.

#### Independent Source Sandbox
`.build-env/pm3-src/` is an independent shallow clone tracking upstream `RfidResearchGroup/proxmark3`. Patches (e.g., `comms_relaxed_version.patch`) are applied programmatically in this sandbox only — never against any host tree.

---

### 14.2 In-GUI "1-Click Rapid Upstream Update & Flash" Pipeline

An automated background state machine tracks rapid upstream PM5 changes (multiple commits daily) and applies them with a single click.

**UI Flow:**

```
[Button: "Check for PM5 Updates"]
             │
             ▼
[Tauri Backend: git fetch origin master against .build-env/pm3-src]
             │
             ├─► No new commits: UI shows "Firmware and client are up to date."
             │
             └─► New commits detected
                         │
                         ▼
             [Modal: "New PM5 Updates Available"]
             * Displays changelog / commit summaries
             * Button: [ 1-Click Update & Flash ]
                         │
                         ▼
[Automated Background Pipeline]
              1. Git fast-forward merge into .build-env/pm3-src
              2. **Strict version match enforced — no comms_relaxed_version.patch applied.** `comms.c` is NOT patched.
              3. Silent headless compile via seeded micro-toolchain:
                - make -j4 PLATFORM=PM5 PLATFORM_EXTRAS=BWM   (firmware)
                - make -j4 PLATFORM=PM5                        (client)
             4. Hot-swap active Tauri client sidecar
             5. Trigger bootloader (auto-command or user prompt)
             6. Execute proxmark3.exe --flash with fresh bootrom.elf + fullimage.elf
                         │
                         ▼
             [UI: "Update Complete! PM5 running latest commit [gHHHHHHH]"]
```

**Backend state machine (Tauri):** `Idle → Fetching → UpdateAvailable → Merging → Patching → CompilingFirmware → CompilingClient → Flashing → Complete | Failed`

---

### 14.3 Seeded Local Toolchain Topology

The standalone build engine is seeded directly inside the project root at `D:\kilocode\phos5gui\.build-env\toolchain\`:

| Component | Path | Tooling |
|-----------|------|---------|
| Root | `D:\kilocode\phos5gui\.build-env\toolchain\` | Seeded micro-toolchain root |
| POSIX & Shell Utilities | `.build-env\toolchain\usr\bin\` | `bash.exe`, `make.exe`, `grep`, `awk`, `sed` |
| Host Compiler (Client) | `.build-env\toolchain\mingw64\bin\` | `gcc.exe` |
| Target Cross-Compiler (Firmware) | `.build-env\toolchain\opt\arm-none-eabi\bin\` | `arm-none-eabi-gcc.exe` |

**Isolation Rule:** The toolchain is completely local, isolated, and self-contained within `.build-env\toolchain\`. All references and fallbacks pointing to `D:\proxspace` are removed. No host MSYS2 installation is required, referenced, or used as a fallback.

---

### 14.4 Build Script & Execution Specs (PowerShell Wrappers)

All compilation commands invoke the seeded engine explicitly. PowerShell wrappers resolve `$localBash` to the seeded toolchain and execute `make` inside `.build-env/pm3-src`:

```powershell
# scripts/windows/build-sandboxed-client.ps1 (seeded toolchain)
$localBash = "D:\kilocode\phos5gui\.build-env\toolchain\usr\bin\bash.exe"
$sandboxRoot = "D:\kilocode\phos5gui\.build-env"
$pm3Src = "$sandboxRoot\pm3-src"

# Inject seeded toolchain MSYS paths so make can locate gcc and arm-none-eabi-gcc
# without relying on host /etc/profile or mount tables.
$envPath = "PATH=/usr/bin:/mingw64/bin:/opt/arm-none-eabi/bin:`$PATH"

# Firmware (BWM variant)
& $localBash -c "$envPath && cd /d/kilocode/phos5gui/.build-env/pm3-src && make -j4 PLATFORM=PM5 PLATFORM_EXTRAS=BWM"

# Client (standard)
& $localBash -c "$envPath && cd /d/kilocode/phos5gui/.build-env/pm3-src && make -j4 PLATFORM=PM5"
```

No path conversion to host MSYS2 is performed; all commands target the seeded local engine.

---

### 14.5 Shell Script Definitions (Execution Specs)

Shell scripts executed via `$localBash` operate exclusively inside `.build-env/pm3-src` using the seeded toolchain:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd /d/kilocode/phos5gui/.build-env/pm3-src

# Firmware build
make clean PLATFORM=PM5 PLATFORM_EXTRAS=BWM
make -j4 PLATFORM=PM5 PLATFORM_EXTRAS=BWM

# Client build
make clean PLATFORM=PM5
make -j4 PLATFORM=PM5
```

All `make` invocations are capped at `-j4` to avoid MSYS2 memory spikes on Windows. The seeded `bash.exe`, `make.exe`, `gcc.exe`, and `arm-none-eabi-gcc.exe` resolve from `.build-env/toolchain/` only.

---

### 14.6 Real-Time Telemetry & Safety Requirements

#### Progress Streaming
Compilation percentage and phase status stream to a clean UI progress bar / expandable log drawer:
- `[1/4] Pulling code`
- `[2/4] Compiling Firmware (-j4)`
- `[3/4] Building Client`
- `[4/4] Flashing Board`

#### Build Failure Safety
If upstream commits code that breaks compilation in `armsrc/`:
- Catch the exit error gracefully.
- Keep the existing working binaries intact (no hot-swap, no flash).
- Display: `Upstream build failed at commit [hash]. Your current working firmware was not modified.`

#### Lockouts
- Prevent starting the flash process if compilation failed.
- Prevent starting the flash process if the serial connection is unstable.

---

### 14.7 Directory & Sandbox Topology (Updated)

```
phos5gui/                                    # Project root (portable, USB-stick ready)
├── .build-env/                              # SANDBOX ROOT — fully autonomous
│   ├── toolchain/                           # Seeded headless micro-toolchain (~120-150 MB)
│   │   ├── usr/bin/bash.exe
│   │   ├── usr/bin/make.exe
│   │   ├── usr/bin/grep.exe
│   │   ├── usr/bin/awk.exe
│   │   ├── usr/bin/sed.exe
│   │   ├── mingw64/bin/gcc.exe
│   │   └── opt/arm-none-eabi/bin/arm-none-eabi-gcc.exe
│   ├── pm3-src/                             # Independent shallow clone (upstream)
│   │   ├── .git/
│   │   ├── armsrc/, client/, include/, common/, tools/, Makefile.platform
│   │   └── (patches applied here only)
│   ├── patches/
│   │   └── comms_relaxed_version.patch
│   └── state.json                           # Sandbox metadata (last sync, commit, config)
│
├── scripts/
│   ├── portable-runtime/
│   │   └── setup-micro-toolchain.ps1        # Bootstrap headless toolchain
│   └── windows/                             # Seeded PowerShell wrappers
│       └── build-sandboxed-client.ps1
│
├── src-tauri/
│   ├── binaries/pm5/                        # RUNTIME ARTIFACTS (hot-swapped here)
│   └── firmware/pm5/                        # RUNTIME ARTIFACTS (hot-swapped here)
│
└── ... (Tauri/React/Vite source unchanged)
```

#### Key Isolation Guarantees
| Path | Host ProxSpace Touched? | Notes |
|------|------------------------|-------|
| `D:\proxspace\ProxSpace-master\...` | ❌ Never | No git worktree, no source reads, no writes |
| `.build-env/pm3-src/` | ❌ | Independent clone; patches applied here only |
| `.build-env/toolchain/` | ❌ | Seeded headless bundle |
| `src-tauri/binaries/pm5/` | ❌ | Hot-swapped runtime artifacts |

---

### 14.8 Decision Log (Section 14)

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-11-11 | PhosSpace standalone micro-toolchain; no host git worktree | Eliminates index lock contention, host MSYS2 dependency, and terminal requirement |
| 2026-11-11 | Seeded local toolchain at `.build-env\toolchain\` | Completely local, isolated, self-contained; no `D:\proxspace` fallback |
| 2026-11-11 | Headless build bundle (~120–150 MB) seeded on demand | Zero-dependency first-run experience; portable |
| 2026-11-11 | Independent shallow clone `.build-env/pm3-src/` | Patches applied in sandbox only; host ProxSpace pristine |
| 2026-11-11 | Explicit `$localBash` invocation in build scripts | All `make` commands target seeded engine, not host MSYS2 |
| 2026-11-11 | 1-Click Rapid Upstream Update & Flash pipeline | 1-click hub for rapid PM5 commits; progress streaming + safety lockouts |
| 2026-11-11 | Build failure safety: keep working binaries, report failed commit | Prevents bricking/downgrade on broken upstream commits |

**End of Section 14**

**End of Corrected Plan Document**