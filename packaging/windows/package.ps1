<#
.SYNOPSIS
  Build, sign and package AI Assistant for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    aiassist-<version>-windows-<arch>.msi            per-machine installer (WiX v5)
    aiassist-<version>-windows-<arch>-portable.zip   aiassist.exe + portable.txt + licences

  The binary links the C runtime statically (+crt-static), so no Visual C++ redistributable is
  needed. Signing is delegated to sign.ps1 (skipped with a warning when no secrets are set).

  Needs: Rust (MSVC toolchain + the target), the Windows SDK (rc.exe, signtool.exe) and
  WiX v5: dotnet tool install --global wix --version 5.0.2

.EXAMPLE
  pwsh packaging/windows/package.ps1 -Arch x64
  pwsh packaging/windows/package.ps1 -Arch arm64     # cross-compiled; needs the MSVC ARM64 build tools
#>
param(
  [ValidateSet('x64', 'x86', 'arm64')] [string] $Arch = 'x64',
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $env:AIASSIST_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
# MSI ProductVersion is numeric (major.minor.build); pre-release tags are dropped there.
$MsiVersion = ($Version -split '-')[0]

$Target = switch ($Arch) { 'x64' { 'x86_64-pc-windows-msvc' } 'x86' { 'i686-pc-windows-msvc' } 'arm64' { 'aarch64-pc-windows-msvc' } }
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Write-Output "AI Assistant $Version for Windows $Arch ($Target)"

if (-not $SkipBuild) {
  # Static CRT, scoped to the target so host build scripts and proc-macros are unaffected.
  $flagVar = 'CARGO_TARGET_' + ($Target.ToUpper() -replace '-', '_') + '_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  # Fail the build (rather than warn) if the icon/VERSIONINFO can't be embedded.
  $env:AIASSIST_REQUIRE_WINRES = '1'
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p aiassist --target $Target }
}

$Bin = Join-Path $TargetDir "$Target\release"

# Check the PE header before packaging: Machine must match -Arch, and Subsystem must be 2
# (Windows GUI: no console window opens with the app).
$bytes = [System.IO.File]::ReadAllBytes((Join-Path $Bin 'aiassist.exe'))
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
$machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
$subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C)
$expected = switch ($Arch) { 'x64' { 0x8664 } 'x86' { 0x14C } 'arm64' { 0xAA64 } }
if ($machine -ne $expected) { throw "aiassist.exe is for machine 0x$('{0:X}' -f $machine), expected 0x$('{0:X}' -f $expected) ($Arch)" }
if ($subsystem -ne 2) { throw "aiassist.exe has PE subsystem $subsystem, expected 2 (GUI)" }
Write-Output "ok aiassist.exe: $Arch, GUI subsystem"

$Stage = Join-Path $TargetDir "windows-package\$Arch"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'aiassist.exe') $Stage
foreach ($f in 'LICENSE-MIT', 'LICENSE-APACHE') { Copy-Item (Join-Path $Root $f) $Stage }

& (Join-Path $PSScriptRoot 'sign.ps1') (Join-Path $Stage 'aiassist.exe')

# ---- MSI ---------------------------------------------------------------------------------------
$Msi = Join-Path $Dist "aiassist-$Version-windows-$Arch.msi"
Invoke-Native 'wix build' {
  wix build (Join-Path $PSScriptRoot 'aiassist.wxs') -arch $Arch `
    -d "Version=$MsiVersion" -d "BinDir=$Stage" -d "IconPath=$(Join-Path $Root 'assets\app-icon\aiassist.ico')" `
    -o $Msi
}
# wix writes debug symbols (.wixpdb) next to the MSI; keep them out of the release assets.
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
& (Join-Path $PSScriptRoot 'sign.ps1') $Msi

# ---- portable zip ------------------------------------------------------------------------------
$Portable = Join-Path $TargetDir "windows-package\aiassist-$Version-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage '*') $Portable
Copy-Item (Join-Path $PSScriptRoot 'portable.txt') $Portable
foreach ($f in 'README.md', 'NOTICE') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Portable }
}
$Zip = Join-Path $Dist "aiassist-$Version-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

# Smoke test when this machine can run the build (an ARM64 build on an x64 runner can't).
# A GUI-subsystem exe doesn't make PowerShell wait, so Start-Process -Wait checks its exit code.
$HostArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
if ($Arch -ne 'arm64' -or $HostArch -eq 'arm64') {
  $p = Start-Process (Join-Path $Stage 'aiassist.exe') -ArgumentList '--version' -Wait -PassThru -NoNewWindow
  if ($p.ExitCode -ne 0) { throw "aiassist.exe --version exited $($p.ExitCode)" }
  Write-Output 'ok aiassist.exe --version'
} else {
  Write-Output "skipping aiassist.exe --version: an $Arch build doesn't run on this $HostArch machine"
}
Get-Item $Msi, $Zip | Format-Table Name, Length
