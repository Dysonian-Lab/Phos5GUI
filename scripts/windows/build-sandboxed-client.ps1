# scripts/windows/build-sandboxed-client.ps1 (seeded toolchain)
#
# Builds the PM5 client and firmware inside the PhosSpace sandbox via the
# seeded headless micro-toolchain at .build-env/toolchain/.
#
# No path conversion to a host MSYS2 is performed; all commands target the
# seeded local engine.

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$sandboxRoot = Join-Path $projectRoot ".build-env"
$localBash = Join-Path $sandboxRoot "toolchain\usr\bin\bash.exe"
$pm3Src = Join-Path $sandboxRoot "pm3-src"

if (-not (Test-Path $localBash)) {
    throw "Seeded bash not found at: $localBash. Run scripts/portable-runtime/setup-micro-toolchain.ps1 first."
}
if (-not (Test-Path $pm3Src)) {
    throw "PM3 source sandbox not found at: $pm3Src."
}

# Convert Windows paths to MSYS paths for the seeded bash environment.
function Convert-ToMsysPath([string]$path) {
    $p = $path -replace '\\', '/'
    if ($p -match '^([A-Za-z]):/(.*)$') {
        return "/$($matches[1].ToLower())/$($matches[2])"
    }
    return $p
}

$pm3SrcMsys = Convert-ToMsysPath $pm3Src

# Inject seeded toolchain MSYS paths so make can locate gcc and
# arm-none-eabi-gcc without relying on host /etc/profile or mount tables.
$envPath = "PATH=/usr/bin:/mingw64/bin:/opt/arm-none-eabi/bin:`$PATH"

# Firmware (BWM variant)
& $localBash -c "$envPath && cd '$pm3SrcMsys' && make -j4 PLATFORM=PM5 PLATFORM_EXTRAS=BWM"

# Client (standard)
& $localBash -c "$envPath && cd '$pm3SrcMsys' && make -j4 PLATFORM=PM5"
