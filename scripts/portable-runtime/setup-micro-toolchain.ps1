<#
.SYNOPSIS
    Bootstraps the PhosSpace standalone headless micro-toolchain into
    .build-env\toolchain\ on first build (Section 14.1).

.DESCRIPTION
    Phos5GUI must be completely autonomous: no ProxSpace install, no
    `runme64.bat`, no user terminal. This script silently downloads and
    extracts the headless build bundle (~120-150 MB) containing:

        bash.exe, make.exe              (POSIX shell + GNU Make)
        MinGW-w64 x64 gcc.exe           (host tools / client compile)
        arm-none-eabi-gcc.exe + libs    (ARM cross-compiler for firmware)

    Seeded to .build-env/toolchain/:
        usr/bin/bash.exe
        usr/bin/make.exe
        mingw64/bin/gcc.exe
        opt/arm-none-eabi/bin/arm-none-eabi-gcc.exe

    If the toolchain is already present and complete, this is a no-op (exit 0).
    No interactive prompts are issued. All failures surface as a non-zero exit
    code with a descriptive message so the caller (Tauri build pipeline) can
    report them to the user.

.PARAMETER ToolchainUrl
    URL of the headless toolchain bundle ZIP. Override when a release asset is
    available. Default points at the project releases; replace with the actual
    seeded bundle URL before first use.
.PARAMETER TargetDir
    Destination directory. Defaults to .build-env\toolchain relative to the
    script's parent project root.
.PARAMETER Force
    Re-download and re-extract even if the toolchain already appears complete.

.EXAMPLE
    .\scripts\portable-runtime\setup-micro-toolchain.ps1

.EXAMPLE
    .\scripts\portable-runtime\setup-micro-toolchain.ps1 `
        -ToolchainUrl "https://github.com/Dysonian-Lab/Phos5GUI/releases/download/v1.0.0/phos5-micro-toolchain.zip" `
        -Force
#>
[CmdletBinding()]
param(
    [Parameter()]
    [string]$ToolchainUrl = "https://github.com/Dysonian-Lab/Phos5GUI/releases/latest/download/phos5-micro-toolchain.zip",

    [Parameter()]
    [string]$TargetDir = "",

    [Parameter()]
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# Resolve project root from this script's location:
#   scripts/portable-runtime/setup-micro-toolchain.ps1  ->  <root>
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$projectRoot = Resolve-Path (Join-Path $scriptDir "..\..")

if ([string]::IsNullOrEmpty($TargetDir)) {
    $TargetDir = Join-Path $projectRoot ".build-env\toolchain"
}
$resolved = Resolve-Path -LiteralPath $TargetDir -ErrorAction SilentlyContinue
if (-not $resolved) {
    New-Item -ItemType Directory -Path (Split-Path -Parent $TargetDir) -Force | Out-Null
    $resolved = Join-Path $projectRoot ".build-env\toolchain"
    New-Item -ItemType Directory -Path $resolved -Force | Out-Null
}
# Normalize to a plain filesystem path string.
$TargetDir = if ($resolved -is [System.Management.Automation.PathInfo]) {
    $resolved.ProviderPath
} else {
    [string]$resolved
}

# Required binaries that prove a complete seed (Section 14.1 / 14.3 topology).
$requiredBinaries = New-Object System.Collections.Generic.List[string]
$requiredBinaries.Add((Join-Path $TargetDir "usr\bin\bash.exe"))
$requiredBinaries.Add((Join-Path $TargetDir "usr\bin\make.exe"))
$requiredBinaries.Add((Join-Path $TargetDir "mingw64\bin\gcc.exe"))
$requiredBinaries.Add((Join-Path $TargetDir "opt\arm-none-eabi\bin\arm-none-eabi-gcc.exe"))

function Test-ToolchainComplete {
    param([string]$Dir)
    $req = New-Object System.Collections.Generic.List[string]
    $req.Add((Join-Path $Dir "usr\bin\bash.exe"))
    $req.Add((Join-Path $Dir "usr\bin\make.exe"))
    $req.Add((Join-Path $Dir "mingw64\bin\gcc.exe"))
    $req.Add((Join-Path $Dir "opt\arm-none-eabi\bin\arm-none-eabi-gcc.exe"))
    foreach ($b in $req) {
        if (-not (Test-Path -LiteralPath $b -PathType Leaf)) {
            return $false
        }
    }
    return $true
}

# Already bootstrapped and complete -> no-op.
if (-not $Force -and (Test-ToolchainComplete -Dir $TargetDir)) {
    Write-Host "[setup-micro-toolchain] Toolchain already seeded at $TargetDir" -ForegroundColor Cyan
    exit 0
}

# Download the headless bundle.
    $guid = [guid]::NewGuid()
    $tempZip = Join-Path $env:TEMP "phos5-micro-toolchain-$guid.zip"
    try {
    Write-Host "[setup-micro-toolchain] Downloading headless toolchain bundle from $ToolchainUrl" -ForegroundColor Cyan
    Invoke-WebRequest -Uri $ToolchainUrl -OutFile $tempZip -TimeoutSec 300
} catch {
    Write-Error "Failed to download toolchain bundle from $ToolchainUrl : $($_.Exception.Message)`n`nProvide a valid -ToolchainUrl or pre-seed $TargetDir with the headless bundle."
    exit 1
}

# Extract into a staging dir, then move into place so a partial extract never
# leaves a broken toolchain behind.
    $stagingDir = Join-Path $env:TEMP "phos5-toolchain-stage-$guid"
New-Item -ItemType Directory -Path $stagingDir -Force | Out-Null
try {
    Write-Host "[setup-micro-toolchain] Extracting bundle to $TargetDir" -ForegroundColor Cyan
    Expand-Archive -LiteralPath $tempZip -DestinationPath $stagingDir -Force

    # The ZIP may contain a single top-level folder; flatten one level if so.
    $contents = Get-ChildItem -LiteralPath $stagingDir
    $extractRoot = $stagingDir
    if ($contents.Count -eq 1 -and $contents[0].PSIsContainer) {
        $extractRoot = $contents[0].FullName
    }

    # Clear any previous target (Force mode) then copy the tree in.
    if ($Force -and (Test-Path -LiteralPath $TargetDir -PathType Container)) {
        Remove-Item -LiteralPath $TargetDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $TargetDir) -Force | Out-Null
    if (-not (Test-Path -LiteralPath $TargetDir -PathType Container)) {
        New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
    }
    Copy-Item -LiteralPath (Join-Path $extractRoot "*") -Destination $TargetDir -Recurse -Force
} finally {
    if (Test-Path -LiteralPath $stagingDir -PathType Container) {
        Remove-Item -LiteralPath $stagingDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $tempZip -PathType Leaf) {
        Remove-Item -LiteralPath $tempZip -Force -ErrorAction SilentlyContinue
    }
}

# Verify the seed is complete.
if (-not (Test-ToolchainComplete -Dir $TargetDir)) {
    $missing = $requiredBinaries | Where-Object { -not (Test-Path -LiteralPath $_ -PathType Leaf) }
    Write-Error "Toolchain extraction did not produce the required binaries at $TargetDir.`nMissing: $($missing -join ', ')"
    exit 1
}

Write-Host "[setup-micro-toolchain] Toolchain seeded successfully at $TargetDir" -ForegroundColor Green
exit 0
