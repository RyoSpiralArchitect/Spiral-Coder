Param(
  [switch]$AddCargoBinToUserPath = $true
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$cargoBin = Join-Path $env:USERPROFILE ".cargo\\bin"
$cargoExe = Join-Path $cargoBin "cargo.exe"

if (-not (Test-Path $cargoExe)) {
  Write-Host "cargo.exe not found at: $cargoExe"
  Write-Host "Install Rust first (rustup) then re-run this script."
  exit 1
}

& $cargoExe install --path . --force
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Update PATH for the current session too.
if ($env:PATH -notlike "*$cargoBin*") {
  $env:PATH = "$cargoBin;$env:PATH"
}

if ($AddCargoBinToUserPath) {
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if ($null -eq $userPath) { $userPath = "" }
  if ($userPath -notlike "*$cargoBin*") {
    [Environment]::SetEnvironmentVariable("Path", "$cargoBin;$userPath", "User")
    Write-Host "Added to User PATH: $cargoBin"
    Write-Host "Restart your terminal, then run: spiral-coder --version"
  } else {
    Write-Host "User PATH already contains: $cargoBin"
  }
}

Write-Host "Installed: spiral-coder.exe"
