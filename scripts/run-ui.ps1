Param(
  [string]$Host = "127.0.0.1",
  [int]$Port = 18080
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

# Build/run from an isolated target dir so it can coexist with `run-tui.ps1`.
$env:CARGO_TARGET_DIR = (Join-Path $repoRoot ".tmp\\cargo-target-ui")
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null
& (Join-Path $PSScriptRoot "kill-spiral-coder.ps1") -PathContains $env:CARGO_TARGET_DIR | Out-Null

$cargoBin = Join-Path $env:USERPROFILE ".cargo\\bin"
$cargoExe = Join-Path $cargoBin "cargo.exe"
$exe = Join-Path $env:CARGO_TARGET_DIR "debug\\spiral-coder.exe"

if (Test-Path $cargoExe) {
  & $cargoExe build
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if (-not (Test-Path $exe)) {
  Write-Host "spiral-coder.exe not found at: $exe"
  Write-Host "Run: .\\scripts\\install.ps1   (or install Rust and build with cargo)"
  exit 1
}

Write-Host "Spiral-Coder UI: http://$Host`:$Port/"
& $exe serve --host $Host --port $Port
