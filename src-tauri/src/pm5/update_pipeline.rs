//! In-GUI 1-click upstream update & flash pipeline (Plan Section 14.2).
//!
//! Tracks rapid upstream PM5 changes against the independent source sandbox at
//! `.build-env/pm3-src` and, on a single user action, fast-forwards the sandbox,
//! re-applies the relaxed capabilities patch, rebuilds firmware + client with the
//! seeded headless micro-toolchain, hot-swaps the freshly compiled artifacts into
//! the runtime locations, and finally runs the existing 4-stage PM5 flasher.
//!
//! Design constraints (Section 14.6):
//! - All build work runs inside `.build-env` only; no host toolchain tree is read,
//!   written or locked.
//! - The seeded toolchain at `.build-env/toolchain` is required. If it is missing
//!   the bootstrap script is invoked to seed it.
//! - Progress and raw build output stream to the UI via the `pm5-update-progress`
//!   event.
//! - Safety gate: if firmware or client compilation fails, the pipeline aborts
//!   BEFORE hot-swapping or flashing, so the currently working sidecars/firmware
//!   are left untouched and the failed commit is reported.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::error::AppError;
use crate::flasher;

const EVENT_NAME: &str = "pm5-update-progress";

const UPSTREAM_URL: &str = "https://github.com/RfidResearchGroup/proxmark3.git";

const STAGE_PULL: &str = "[1/4] Pulling code";
const STAGE_FIRMWARE: &str = "[2/4] Compiling Firmware (-j4)";
const STAGE_CLIENT: &str = "[3/4] Building Client";
const STAGE_FLASH: &str = "[4/4] Flashing Board";

/// High-level state of the update pipeline (Plan Section 14.2 backend state
/// machine). Serialized as snake_case so the frontend can switch on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    Idle,
    Fetching,
    UpdateAvailable,
    Merging,
    Patching,
    CompilingFirmware,
    CompilingClient,
    HotSwapping,
    Flashing,
    Complete,
    Failed,
}

/// Result of an upstream update check against `.build-env/pm3-src`.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheckResult {
    pub updates_available: bool,
    pub current_commit: String,
    pub latest_commit: String,
    pub commit_logs: Vec<String>,
}

/// Progress payload emitted on `pm5-update-progress`.
///
/// Every status change carries `status`/`stage`/`percent`/`message`. Build output
/// is streamed as additional events where `output` holds a single line and
/// `stream` is `"stdout"` or `"stderr"`.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgress {
    pub status: PipelineStatus,
    pub stage: String,
    pub percent: u8,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_commit: Option<String>,
}

// ---------------------------------------------------------------------------
// Path resolution (must work regardless of the process working directory)
// ---------------------------------------------------------------------------

fn is_project_root(dir: &Path) -> bool {
    dir.join("src-tauri").is_dir() && dir.join("package.json").is_file()
}

fn is_runtime_root(dir: &Path) -> bool {
    dir.join(".build-env").is_dir() || dir.join("_up_").join(".build-env").is_dir()
}

fn normalize_runtime_root(dir: &Path) -> PathBuf {
    if dir.join(".build-env").is_dir() {
        return dir.to_path_buf();
    }
    let up = dir.join("_up_");
    if up.join(".build-env").is_dir() {
        return up;
    }
    dir.to_path_buf()
}

fn find_project_root_from(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if is_project_root(&cur) {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

fn find_runtime_root_from(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if is_runtime_root(&cur) {
            return Some(normalize_runtime_root(&cur));
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Resolve runtime root containing `.build-env/`.
/// Works for portable/USB-stick installs (exe dir arbitrary) and dev layouts; no hardcoded dev paths.
fn resolve_project_root() -> Result<PathBuf, AppError> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(root) = find_runtime_root_from(parent) {
                return Ok(root);
            }
            if let Some(root) = find_project_root_from(parent) {
                return Ok(root);
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(root) = find_runtime_root_from(&cwd) {
            return Ok(root);
        }
        if let Some(root) = find_project_root_from(&cwd) {
            return Ok(root);
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) =
        find_project_root_from(&manifest).or_else(|| find_runtime_root_from(&manifest))
    {
        return Ok(root);
    }
    Err(AppError::CommandFailed(
        "Could not locate runtime root containing .build-env (checked executable directory, current directory, and compile-time paths).".to_string(),
    ))
}
fn sandbox_root(root: &Path) -> PathBuf {
    root.join(".build-env")
}

fn pm3_src(root: &Path) -> PathBuf {
    sandbox_root(root).join("pm3-src")
}

fn toolchain_dir(root: &Path) -> PathBuf {
    sandbox_root(root).join("toolchain")
}

fn patch_file(root: &Path) -> PathBuf {
    let sandbox = sandbox_root(root)
        .join("patches")
        .join("comms_relaxed_version.patch");
    if sandbox.is_file() {
        return sandbox;
    }
    root.join("src-tauri")
        .join("src")
        .join("client_patch")
        .join("comms_relaxed_version.patch")
}

fn bash_exe(root: &Path) -> PathBuf {
    toolchain_dir(root).join("usr").join("bin").join("bash.exe")
}

fn toolchain_complete(root: &Path) -> bool {
    let tc = toolchain_dir(root);
    [
        tc.join("usr").join("bin").join("bash.exe"),
        tc.join("usr").join("bin").join("make.exe"),
        tc.join("mingw64").join("bin").join("gcc.exe"),
        tc.join("opt")
            .join("arm-none-eabi")
            .join("bin")
            .join("arm-none-eabi-gcc.exe"),
    ]
    .iter()
    .all(|p| p.is_file())
}

/// Prefer the toolchain's bundled git, otherwise fall back to whatever `git` is
/// on PATH.
fn resolve_git(root: &Path) -> PathBuf {
    let seeded = toolchain_dir(root).join("usr").join("bin").join("git.exe");
    if seeded.is_file() {
        seeded
    } else {
        PathBuf::from("git")
    }
}

/// Convert a Windows path (`D:\a\b`) into the MSYS form (`/d/a/b`) used by the
/// seeded bash.
fn to_msys_path(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = &s[2..];
        if rest.starts_with('/') {
            return format!("/{drive}{rest}");
        }
        return format!("/{drive}/{rest}");
    }
    s
}

fn short_commit(commit: &str) -> String {
    commit.trim().chars().take(7).collect()
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

fn stage_label(status: PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Idle
        | PipelineStatus::Fetching
        | PipelineStatus::UpdateAvailable
        | PipelineStatus::Merging
        | PipelineStatus::Patching => STAGE_PULL,
        PipelineStatus::CompilingFirmware => STAGE_FIRMWARE,
        PipelineStatus::CompilingClient => STAGE_CLIENT,
        PipelineStatus::HotSwapping
        | PipelineStatus::Flashing
        | PipelineStatus::Complete
        | PipelineStatus::Failed => STAGE_FLASH,
    }
}

fn emit_status(app: &AppHandle, status: PipelineStatus, percent: u8, message: impl Into<String>) {
    let _ = app.emit(
        EVENT_NAME,
        UpdateProgress {
            status,
            stage: stage_label(status).to_string(),
            percent,
            message: message.into(),
            output: None,
            stream: None,
            failed_commit: None,
        },
    );
}

fn emit_failed(app: &AppHandle, commit: &str, message: String) -> AppError {
    let _ = app.emit(
        EVENT_NAME,
        UpdateProgress {
            status: PipelineStatus::Failed,
            stage: stage_label(PipelineStatus::Failed).to_string(),
            percent: 100,
            message: message.clone(),
            output: None,
            stream: None,
            failed_commit: Some(short_commit(commit)),
        },
    );
    AppError::CommandFailed(message)
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

async fn run_git(root: &Path, cwd: &Path, args: &[&str]) -> Result<std::process::Output, AppError> {
    let git = resolve_git(root);
    Command::new(&git)
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| {
            AppError::CommandFailed(format!(
                "Failed to spawn git ({}) for `git {}`: {e}",
                git.display(),
                args.join(" ")
            ))
        })
}

async fn git_output(root: &Path, cwd: &Path, args: &[&str]) -> Result<String, AppError> {
    let out = run_git(root, cwd, args).await?;
    if !out.status.success() {
        return Err(AppError::CommandFailed(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// Update check
// ---------------------------------------------------------------------------

/// Check the independent sandbox for upstream commits ahead of local HEAD.
///
/// Runs `git fetch origin master` inside `.build-env/pm3-src`, compares
/// `HEAD` with `origin/master`, and parses `git log HEAD..origin/master
/// --oneline` for the changelog.
pub async fn check_updates(app: &AppHandle) -> Result<UpdateCheckResult, AppError> {
    let root = resolve_project_root()?;
    let src = pm3_src(&root);

    emit_status(
        app,
        PipelineStatus::Fetching,
        5,
        format!("Fetching upstream master in {}", src.display()),
    );

    if !src.exists() {
        return Err(AppError::CommandFailed(format!(
            "PM3 source sandbox not found at {}. It is created by the update pipeline on first run.",
            src.display()
        )));
    }

    git_output(&root, &src, &["fetch", "origin", "master"]).await?;

    let current_commit = git_output(&root, &src, &["rev-parse", "HEAD"]).await?;
    let latest_commit = git_output(&root, &src, &["rev-parse", "origin/master"]).await?;

    let updates_available = current_commit != latest_commit;

    let commit_logs = if updates_available {
        let raw = git_output(&root, &src, &["log", "HEAD..origin/master", "--oneline"]).await?;
        raw.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    emit_status(
        app,
        if updates_available {
            PipelineStatus::UpdateAvailable
        } else {
            PipelineStatus::Idle
        },
        10,
        if updates_available {
            format!("{} new commit(s) upstream", commit_logs.len())
        } else {
            "Firmware and client are up to date.".to_string()
        },
    );

    Ok(UpdateCheckResult {
        updates_available,
        current_commit,
        latest_commit,
        commit_logs,
    })
}

// ---------------------------------------------------------------------------
// Sandbox bootstrap
// ---------------------------------------------------------------------------

async fn run_toolchain_setup(root: &Path) -> Result<(), AppError> {
    let script = root
        .join("scripts")
        .join("portable-runtime")
        .join("setup-micro-toolchain.ps1");
    if !script.is_file() {
        return Err(AppError::CommandFailed(format!(
            "Micro-toolchain is missing and the bootstrap script was not found at {}.",
            script.display()
        )));
    }

    let output = Command::new("powershell")
        .args(["-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .output()
        .await
        .map_err(|e| AppError::CommandFailed(format!("Failed to spawn powershell: {e}")))?;

    if !output.status.success() {
        return Err(AppError::CommandFailed(format!(
            "Toolchain bootstrap failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

async fn ensure_pm3_src(root: &Path, app: &AppHandle) -> Result<(), AppError> {
    let src = pm3_src(root);
    if src.join(".git").exists() {
        return Ok(());
    }

    let sandbox = sandbox_root(root);
    tokio::fs::create_dir_all(&sandbox).await.map_err(|e| {
        AppError::CommandFailed(format!("Failed to create {}: {e}", sandbox.display()))
    })?;

    emit_status(
        app,
        PipelineStatus::Merging,
        12,
        format!("Cloning upstream proxmark3 into {}", src.display()),
    );

    let out = Command::new(resolve_git(root))
        .args(["clone", "--depth", "1", "--branch", "master", UPSTREAM_URL])
        .arg(&src)
        .current_dir(&sandbox)
        .output()
        .await
        .map_err(|e| AppError::CommandFailed(format!("Failed to spawn git clone: {e}")))?;

    if !out.status.success() {
        return Err(AppError::CommandFailed(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Patch application (idempotent)
// ---------------------------------------------------------------------------

async fn apply_patch(root: &Path) -> Result<(), AppError> {
    let src = pm3_src(root);
    let patch = patch_file(root);
    if !patch.is_file() {
        return Err(AppError::CommandFailed(format!(
            "Relaxed capabilities patch not found at {}",
            patch.display()
        )));
    }
    let patch_arg = patch.to_string_lossy().to_string();

    // If `git apply --reverse --check` succeeds the patch is already applied;
    // skip it so the pipeline is safe to re-run.
    let reverse_check = run_git(
        root,
        &src,
        &["apply", "--reverse", "--check", patch_arg.as_str()],
    )
    .await?;
    if reverse_check.status.success() {
        return Ok(());
    }

    let apply = run_git(root, &src, &["apply", patch_arg.as_str()]).await?;
    if !apply.status.success() {
        return Err(AppError::CommandFailed(format!(
            "Failed to apply {}: {}",
            patch.display(),
            String::from_utf8_lossy(&apply.stderr).trim()
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Streaming build
// ---------------------------------------------------------------------------

/// Run `make -j4 ...` inside the sandbox with the seeded toolchain, streaming
/// every stdout/stderr line to the UI. Returns the last 200 output lines so the
/// caller can include build context in a failure message.
async fn run_make(
    app: &AppHandle,
    root: &Path,
    status: PipelineStatus,
    percent: u8,
    label: &str,
    extras: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let src = pm3_src(root);
    let bash = bash_exe(root);
    if !bash.is_file() {
        return Err(AppError::CommandFailed(format!(
            "Seeded bash not found at {}. Run scripts/portable-runtime/setup-micro-toolchain.ps1.",
            bash.display()
        )));
    }

    let msys = to_msys_path(&src);
    let target = match extras {
        Some(e) => format!("PLATFORM=PM5 PLATFORM_EXTRAS={e}"),
        None => "PLATFORM=PM5 SKIPQT=1 IS_MINGW=1 LUAPLATFORM=mingw SKIPPYTHON=1".to_string(),
    };
    // Client builds run on Windows MinGW: avoid POSIX-only wait APIs by targeting
    // the client explicitly with PLATFORM=PM5 in the seeded mingw64 environment.
    let make_cmd = if extras.is_none() {
        format!("make -j4 client {target}")
    } else {
        format!("make -j4 {target}")
    };
    let script = format!(
        "export PATH=\"/usr/bin:/mingw64/bin:/opt/arm-none-eabi/bin:$PATH\"; \
         cd \"{msys}\" || exit 1; {make_cmd}"
    );

    emit_status(
        app,
        status,
        percent,
        format!("{}: make -j4 {target}", label),
    );

    let mut child = Command::new(&bash)
        .arg("-c")
        .arg(&script)
        .current_dir(&src)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| AppError::CommandFailed(format!("Failed to spawn bash: {e}")))?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, String)>(512);

    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(("stdout".to_string(), line)).await.is_err() {
                    break;
                }
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(("stderr".to_string(), line)).await.is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    // Forward output to the UI while the build runs, keeping a bounded tail.
    let forward_app = app.clone();
    let forwarder = tokio::spawn(async move {
        let mut tail: VecDeque<String> = VecDeque::new();
        while let Some((stream, line)) = rx.recv().await {
            if tail.len() >= 200 {
                tail.pop_front();
            }
            tail.push_back(line.clone());
            let _ = forward_app.emit(
                EVENT_NAME,
                UpdateProgress {
                    status,
                    stage: stage_label(status).to_string(),
                    percent,
                    message: String::new(),
                    output: Some(line),
                    stream: Some(stream),
                    failed_commit: None,
                },
            );
        }
        tail.into_iter().collect::<Vec<_>>()
    });

    let wait_result = child.wait().await;
    let tail = forwarder.await.unwrap_or_default();

    match wait_result {
        Ok(st) if st.success() => Ok(tail),
        Ok(st) => Err(AppError::CommandFailed(format!(
            "`make {target}` failed with exit code {}.\n--- last output ---\n{}",
            st.code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string()),
            tail.join("\n")
        ))),
        Err(e) => Err(AppError::CommandFailed(format!("Build task failed: {e}"))),
    }
}

// ---------------------------------------------------------------------------
// Hot-swap
// ---------------------------------------------------------------------------

fn find_file_recursive(base: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let direct = base.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, name, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), AppError> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::CommandFailed(format!("Failed to create {}: {e}", parent.display()))
        })?;
    }
    std::fs::copy(src, dst).map_err(|e| {
        AppError::CommandFailed(format!(
            "Failed to copy {} -> {}: {e}",
            src.display(),
            dst.display()
        ))
    })?;
    Ok(())
}

fn dedup(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

fn client_destinations(app: &AppHandle, root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("proxmark3.exe"));
            dirs.push(dir.join("binaries").join("proxmark3.exe"));
            dirs.push(dir.join("binaries").join("proxmark3").join("proxmark3.exe"));
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.join("proxmark3.exe"));
        dirs.push(res.join("binaries").join("proxmark3").join("proxmark3.exe"));
    }
    dirs.push(
        root.join("src-tauri")
            .join("binaries")
            .join("proxmark3")
            .join("proxmark3.exe"),
    );
    dirs.push(
        root.join("src-tauri")
            .join("binaries")
            .join("proxmark3.exe"),
    );
    dedup(dirs)
}

fn firmware_destinations(app: &AppHandle, root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.join("firmware").join("pm5").join("bwm"));
        dirs.push(res.join("firmware").join("bwm"));
    }
    dirs.push(
        root.join("src-tauri")
            .join("firmware")
            .join("pm5")
            .join("bwm"),
    );
    dedup(dirs)
}

/// Copy the freshly built client + firmware into the runtime locations.
async fn hot_swap(app: &AppHandle, root: &Path) -> Result<Vec<String>, AppError> {
    let src = pm3_src(root);

    let client_src = find_file_recursive(&src.join("client"), "proxmark3.exe", 2)
        .or_else(|| find_file_recursive(&src.join("client"), "proxmark3", 2))
        .ok_or_else(|| {
            AppError::CommandFailed(
                "Compiled proxmark3 client was not found in the sandbox after the build."
                    .to_string(),
            )
        })?;

    let bootrom = find_file_recursive(&src.join("armsrc"), "bootrom.elf", 3).ok_or_else(|| {
        AppError::CommandFailed(
            "Compiled bootrom.elf was not found in the sandbox after the build.".to_string(),
        )
    })?;
    let fullimage =
        find_file_recursive(&src.join("armsrc"), "fullimage.elf", 3).ok_or_else(|| {
            AppError::CommandFailed(
                "Compiled fullimage.elf was not found in the sandbox after the build.".to_string(),
            )
        })?;

    let mut written: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for dst in client_destinations(app, root) {
        match copy_file(&client_src, &dst) {
            Ok(()) => written.push(dst.display().to_string()),
            Err(e) => errors.push(e.to_string()),
        }
    }

    for dir in firmware_destinations(app, root) {
        for fw in [&bootrom, &fullimage] {
            let name = fw.file_name().map(|n| n.to_os_string()).unwrap_or_default();
            let dst = dir.join(name);
            match copy_file(fw, &dst) {
                Ok(()) => written.push(dst.display().to_string()),
                Err(e) => errors.push(e.to_string()),
            }
        }
    }

    if written.is_empty() {
        return Err(AppError::CommandFailed(format!(
            "Hot-swap failed: no runtime artifact could be written. {}",
            errors.join("; ")
        )));
    }
    Ok(written)
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// Run the full update & flash pipeline. Emits `pm5-update-progress` at every
/// step. On any compile failure the pipeline aborts before touching the active
/// sidecars or firmware.
pub async fn execute_update_pipeline(app: AppHandle, port: String) -> Result<String, AppError> {
    let root = resolve_project_root()?;
    let src = pm3_src(&root);

    // Step 1: ensure the seeded toolchain exists.
    if !toolchain_complete(&root) {
        emit_status(
            &app,
            PipelineStatus::Fetching,
            3,
            "Micro-toolchain missing; bootstrapping .build-env/toolchain ...",
        );
        run_toolchain_setup(&root).await?;
    }

    ensure_pm3_src(&root, &app).await?;

    emit_status(
        &app,
        PipelineStatus::Fetching,
        8,
        "Fetching upstream master ...",
    );
    git_output(&root, &src, &["fetch", "origin", "master"]).await?;
    let failed_commit = git_output(&root, &src, &["rev-parse", "origin/master"]).await?;

    // Step 2: fast-forward merge.
    emit_status(
        &app,
        PipelineStatus::Merging,
        15,
        "Fast-forward merging origin/master ...",
    );
    git_output(&root, &src, &["merge", "--ff-only", "origin/master"]).await?;

    // Step 3: relaxed capabilities patch is DISABLED (strict version match policy).
    // Firmware MUST strictly match the client. CAPABILITIES_VERSION mismatch is a
    // fatal safety exit — mismatches require flashing bundled matching firmware.
    emit_status(
        &app,
        PipelineStatus::Patching,
        20,
        "Strict version match enforced — no relaxed comms.c patch applied.",
    );
    // Patch application intentionally skipped: do not relax version checks.

    // Step 4: compile firmware (BWM). Safety gate: abort before hot-swap on error.
    if let Err(e) = run_make(
        &app,
        &root,
        PipelineStatus::CompilingFirmware,
        25,
        "Firmware (BWM)",
        Some("BWM"),
    )
    .await
    {
        return Err(emit_failed(
            &app,
            &failed_commit,
            format!(
                "Upstream build failed at commit {}. Your current working firmware was not modified. {}",
                short_commit(&failed_commit),
                e
            ),
        ));
    }

    // Step 5: compile client. Same safety gate.
    if let Err(e) = run_make(
        &app,
        &root,
        PipelineStatus::CompilingClient,
        55,
        "Client",
        None,
    )
    .await
    {
        return Err(emit_failed(
            &app,
            &failed_commit,
            format!(
                "Upstream build failed at commit {}. Your current working firmware was not modified. {}",
                short_commit(&failed_commit),
                e
            ),
        ));
    }

    // Step 6 (safety gate) is implicit: both builds succeeded, so it is now safe
    // to replace the running artifacts.

    // Step 7: hot-swap freshly compiled artifacts.
    emit_status(
        &app,
        PipelineStatus::HotSwapping,
        80,
        "Hot-swapping freshly compiled artifacts ...",
    );
    let written = hot_swap(&app, &root).await?;
    emit_status(
        &app,
        PipelineStatus::HotSwapping,
        82,
        format!("Swapped {} artifact(s)", written.len()),
    );

    // Step 8: flash + verify via the existing 4-stage provisioner.
    emit_status(
        &app,
        PipelineStatus::Flashing,
        85,
        "Flashing PM5 (BWM) over USB-CDC ...",
    );
    let result = flasher::pm5_provision(&app, &port, "bwm")
        .await
        .map_err(|e| emit_failed(&app, &failed_commit, format!("Flashing failed: {e}")))?;

    emit_status(
        &app,
        PipelineStatus::Complete,
        100,
        format!("Update complete: {result}"),
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_msys_path_converts_windows_drive() {
        // Arbitrary drive letters must convert dynamically (e.g. D:\, C:\, E:\).
        assert_eq!(
            to_msys_path(Path::new(r"D:\workspace\project\.build-env\pm3-src")),
            "/d/workspace/project/.build-env/pm3-src"
        );
        assert_eq!(
            to_msys_path(Path::new(r"C:\Users\Owner\AppData\Local\Temp")),
            "/c/Users/Owner/AppData/Local/Temp"
        );
        assert_eq!(
            to_msys_path(Path::new(r"E:\build\sandbox")),
            "/e/build/sandbox"
        );
        // Non-drive paths are returned unchanged (already POSIX).
        assert_eq!(
            to_msys_path(Path::new("/d/already/posix")),
            "/d/already/posix"
        );
    }

    #[test]
    fn short_commit_takes_seven_chars() {
        assert_eq!(short_commit("a1b2c3d4e5f6"), "a1b2c3d");
        assert_eq!(short_commit("  deadbeef  "), "deadbee");
        assert_eq!(short_commit(""), "");
    }

    #[test]
    fn stage_labels_map_to_four_user_stages() {
        assert_eq!(stage_label(PipelineStatus::Fetching), STAGE_PULL);
        assert_eq!(stage_label(PipelineStatus::Patching), STAGE_PULL);
        assert_eq!(
            stage_label(PipelineStatus::CompilingFirmware),
            STAGE_FIRMWARE
        );
        assert_eq!(stage_label(PipelineStatus::CompilingClient), STAGE_CLIENT);
        assert_eq!(stage_label(PipelineStatus::Flashing), STAGE_FLASH);
        assert_eq!(stage_label(PipelineStatus::Complete), STAGE_FLASH);
        assert_eq!(stage_label(PipelineStatus::Failed), STAGE_FLASH);
    }

    #[test]
    fn find_file_recursive_locates_nested_artifact() {
        let base = std::env::temp_dir().join(format!("pm5-find-test-{}", std::process::id()));
        let nested = base.join("armsrc").join("obj");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("fullimage.elf"), b"elf").unwrap();

        assert_eq!(
            find_file_recursive(&base, "fullimage.elf", 3),
            Some(nested.join("fullimage.elf"))
        );
        assert_eq!(find_file_recursive(&base, "bootrom.elf", 3), None);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn pipeline_status_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(PipelineStatus::CompilingFirmware).unwrap(),
            serde_json::json!("compiling_firmware")
        );
        assert_eq!(
            serde_json::to_value(PipelineStatus::UpdateAvailable).unwrap(),
            serde_json::json!("update_available")
        );
    }
}
