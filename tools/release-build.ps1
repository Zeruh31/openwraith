# Builds the release installers (MSI and NSIS) of the OpenWraith desktop app
# without the builder's local paths in the executable.
#
# Rust embeds source paths (panic locations, including those of crates in the
# Cargo registry under %USERPROFILE%\.cargo). This script remaps them for this
# build only (`--remap-path-prefix`, through CARGO_ENCODED_RUSTFLAGS), runs
# `npm --prefix app run tauri build`, and then fails if the executable still
# contains the user-profile path.
#
# Usage (PowerShell 7, from anywhere):
#   pwsh tools/release-build.ps1
#
# Changing the flags makes Cargo rebuild everything once. Do not put the
# remapping into a global or repository `.cargo/config.toml`: a repository
# one would hard-code one person's user name, and the flags would also apply
# to every other build.
#
# Building is safe: it never starts the app. Do not run the built app or an
# installer on a machine where the keyboard is connected unless you mean to.

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
$profileDir = $env:USERPROFILE
if ([string]::IsNullOrWhiteSpace($profileDir)) {
    throw "USERPROFILE is not set; cannot remap the user-profile path."
}

# Later remaps take precedence: the Cargo home (if it is elsewhere) last.
$flags = @()
if ($env:RUSTFLAGS) {
    $flags += $env:RUSTFLAGS -split '\s+' | Where-Object { $_ -ne "" }
}
$flags += "--remap-path-prefix=$profileDir=~"
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $profileDir ".cargo" }
$flags += "--remap-path-prefix=$cargoHome=~/.cargo"
$flags += "--remap-path-prefix=$root=openwraith"
$separator = [string][char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join $separator
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue

Write-Host "Building the release installers with remapped paths..."
npm --prefix (Join-Path $root "app") run tauri build
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

# The executable must not contain the user-profile path (ASCII or UTF-16).
$exe = Join-Path $root "target\release\openwraith-app.exe"
if (-not (Test-Path $exe)) {
    throw "The release executable was not built: $exe"
}
$bytes = [System.IO.File]::ReadAllBytes($exe)
$latin = [System.Text.Encoding]::Latin1.GetString($bytes)
$found = @()
foreach ($needle in @($profileDir, $profileDir.Replace('\', '/'))) {
    if ($latin.IndexOf($needle, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
        $found += "ASCII: $needle"
    }
    $wide = [System.Text.Encoding]::Latin1.GetString([System.Text.Encoding]::Unicode.GetBytes($needle))
    if ($latin.IndexOf($wide, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
        $found += "UTF-16: $needle"
    }
}
if ($found.Count -gt 0) {
    Write-Error ("The release executable still contains the user-profile path ({0})." -f ($found -join ", "))
    exit 1
}

$bundle = Join-Path $root "target\release\bundle"
Write-Host "OK: no user-profile path in $exe"
Get-ChildItem -Path (Join-Path $bundle "msi"), (Join-Path $bundle "nsis") -File -ErrorAction SilentlyContinue |
    ForEach-Object { Write-Host ("Installer: {0}" -f $_.FullName) }
