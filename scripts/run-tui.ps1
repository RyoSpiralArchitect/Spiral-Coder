param(
  [string[]]$Args
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

# Build/run the TUI from an isolated target dir so it can coexist with `run-ui.ps1`.
$env:CARGO_TARGET_DIR = (Join-Path $repoRoot ".tmp\\cargo-target-tui")
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null

# Prevent the common Windows dev failure:
# `cargo run` cannot overwrite `spiral-coder.exe` if it's still running from the SAME target dir.
& (Join-Path $PSScriptRoot "kill-spiral-coder.ps1") -PathContains $env:CARGO_TARGET_DIR | Out-Null

Write-Host "[run-tui] cargo run -- tui" -ForegroundColor Cyan
Write-Host ("[run-tui] CARGO_TARGET_DIR={0}" -f $env:CARGO_TARGET_DIR) -ForegroundColor DarkGray
if ($Args -and $Args.Count -gt 0) {
  cargo run -- tui @Args
} else {
  cargo run -- tui
}
