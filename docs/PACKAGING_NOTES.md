# Packaging Notes

## Windows Packaging & Resource Mapping Rules

### Rule
`bundle.resources` in `src-tauri/tauri.conf.json` **MUST ALWAYS** use Array/List syntax (`["path/*"]`), **NEVER** Object/Map syntax (`{"path/*": "./dir/"}`).

Example (correct):
```json
"resources": [
  "binaries/proxmark3/*.dll",
  "firmware/pm5/standard/*",
  "firmware/pm5/bwm/*",
  "firmware/pm5/fpga_bitstream.bin",
  "capabilities/pm5.json"
]
```

### Justification
In Tauri v2 on Windows, mapping multiple source files to identical directory keys in an object map triggers `os error 183` (ERROR_ALREADY_EXISTS) during recursive directory creation in `tauri-build`, leaving dangling file handles that cause subsequent runs to fail with `os error 5` (Access Denied). Array syntax preserves source-relative layout under `$RESOURCE` without directory-key collisions.

### ExternalBin Distinction
`proxmark3.exe` is managed exclusively by `bundle.externalBin`. Only sidecar runtime DLLs (`binaries/proxmark3/*.dll`) and static assets (`firmware/**`, `capabilities/**`) belong in `bundle.resources`. Do not duplicate the external sidecar binary in `bundle.resources`.
