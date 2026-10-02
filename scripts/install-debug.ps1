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

& $cargoExe build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$srcExe = Join-Path $repoRoot "target\\debug\\spiral-coder.exe"
if (-not (Test-Path $srcExe)) {
  Write-Host "spiral-coder.exe not found at: $srcExe"
  exit 1
}

New-Item -ItemType Directory -Force -Path $cargoBin | Out-Null
$dstExe = Join-Path $cargoBin "spiral-coder.exe"
Copy-Item -Force $srcExe $dstExe

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

Write-Host "Installed (debug): $dstExe"
