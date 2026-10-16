# Build closetag: frontend (bun) -> embedded assets -> release binary.
$ErrorActionPreference = 'Stop'
Set-Location "$PSScriptRoot\.."

Write-Host "==> frontend: bun install + build"
Push-Location frontend
bun install --frozen-lockfile
bun run build
Pop-Location

Write-Host "==> backend: cargo build --release"
# The gnu toolchain needs MinGW binutils (gcc/dlltool/ld) on PATH.
# This machine keeps a portable w64devkit at D:\dev\tools\w64devkit.
$w64 = 'D:\dev\tools\w64devkit\bin'
if ((Test-Path $w64) -and ($env:Path -notlike "*$w64*")) {
  Write-Host "    (adding $w64 to PATH)"
  $env:Path = "$w64;$env:Path"
}
cargo build --release

Write-Host "==> done: target\release\closetag.exe"
