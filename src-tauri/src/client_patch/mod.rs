//! Patched client build integration.
//!
//! This module documents the relaxed capabilities-version check applied to the
//! bundled `proxmark3.exe` client. The actual C patch lives in
//! `comms_patch.c` and is applied in `.build-env/pm3-src/` via
//! `scripts/sandbox/apply-patches.sh` before building the client.
//!
//! No Rust symbols are exposed here; the module exists to group the patch
//! source alongside the rest of the Phos5GUI backend.
