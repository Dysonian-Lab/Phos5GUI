# Phos5GUI - Proxmark5 Desktop GUI

⚠️ **Requirements — read before using this build**

Phos5GUI is purpose-built for the **Proxmark5 (PM5)** hardware platform (standard and BWM variants) running **CAPABILITIES_VERSION 13**.

Client and firmware must be flashed as a matched pair. Stock PM3 firmware, partially flashed devices, or older CAPABILITIES builds will misbehave.

Verify with `hw version`: you must see `PM5` platform flags and `CAPABILITIES_VERSION: 13`.

---

## What It Does

Phos5GUI wraps the Proxmark5 client into a modern visual desktop interface. You plug in your PM5, place a card on the antenna, and the interface handles identification, emulation, research, and cloning.

* **LF (125 kHz)**: Rapid identification, simulation, and cloning.
* **HF (13.56 MHz)**: MIFARE Classic, Ultralight, NTAG, iCLASS, and DESFire support with automated key recovery and memory inspection.
* **PM5 Features**: Native support for Battery & Wireless Module (BWM), onboard FPGA bitstream operations, and RGB antenna controls.

---

## Autonomous Build & Sandbox Architecture (Section 14)

Phos5GUI operates completely self-contained within `.build-env/`:
* Bundles an isolated headless micro-toolchain (`.build-env/toolchain/`) with MinGW and ARM cross-compilers.
* Operates an independent source sandbox (`.build-env/pm3-src/`) with automated patch management (`comms_relaxed_version.patch`).
* Zero host toolchain or hardcoded drive dependencies.

---

## Packaging Notes (Windows / Tauri v2)

⚠️ **Critical Build Rule:** In `src-tauri/tauri.conf.json`, `bundle.resources` must **always** be declared as a JSON array of path patterns, never as an object map.

Mapping individual files to directory targets causes Windows `CreateDirectory` collisions (`os error 183` / `os error 5`) during `tauri-build`. Refer to `docs/PACKAGING_NOTES.md` for complete technical details.

---

## Portable Layout

```text
Phos5GUI_Windows_Portable/
├── phosphor.exe
├── binaries/
│   └── proxmark3/
│       ├── proxmark3.exe
│       ├── libgcc_s_seh-1.dll
│       ├── libstdc++-6.dll
│       ├── libwinpthread-1.dll
│       ├── d3dcompiler_47.dll
│       └── opengl32sw.dll
├── firmware/
│   └── pm5/
│       ├── standard/
│       │   ├── bootrom.elf
│       │   └── fullimage.elf
│       ├── bwm/
│       │   ├── bootrom.elf
│       │   └── fullimage.elf
│       └── fpga_bitstream.bin
└── capabilities/
    └── pm5.json
```

---

## Supported Magic Blanks
* **LF**: T5577, EM4305, T55x7
* **HF**: Gen1a, Gen2 (CUID), Gen3, Gen4 (GTU / GDM / USCUID)