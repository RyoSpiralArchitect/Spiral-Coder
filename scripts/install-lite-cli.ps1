Param(
  [ValidateSet("User", "Repo")]
  [string]$Scope = "User",
  [switch]$Force,
  [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ($Scope -eq "User") {
  $targetDir = Join-Path $env:USERPROFILE ".local\bin"
} else {
  $targetDir = Join-Path $repoRoot "bin"
}

$cmdPath = Join-Path $targetDir "spiral-coder-lite.cmd"
$ps1Path = Join-Path $targetDir "spiral-coder-lite.ps1"

if (-not $DryRun) {
  New-Item -ItemType Directory -Force $targetDir | Out-Null
}

if (((Test-Path $cmdPath) -or (Test-Path $ps1Path)) -and -not $Force -and -not $DryRun) {
  throw "spiral-coder-lite command already exists in '$targetDir'. Re-run with -Force to overwrite."
}

$cmdTemplate = @'
@echo off
setlocal
set "PY=%SPIRAL_CODER_HF_PYTHON%"
if "%PY%"=="" set "PY=python"
"%PY%" "__REPO__\scripts\spiral_coder_lite_cli.py" %*
set "EC=%ERRORLEVEL%"
endlocal & exit /b %EC%
'@

$ps1Template = @'
$ErrorActionPreference = "Stop"
$repo = "__REPO__"
$python = if ($env:SPIRAL_CODER_HF_PYTHON -and $env:SPIRAL_CODER_HF_PYTHON.Trim()) { $env:SPIRAL_CODER_HF_PYTHON } else { "python" }
& $python "$repo\scripts\spiral_coder_lite_cli.py" @args
exit $LASTEXITCODE
'@

$cmdContent = $cmdTemplate.Replace("__REPO__", $repoRoot)
$ps1Content = $ps1Template.Replace("__REPO__", $repoRoot)

if ($DryRun) {
  Write-Host "Dry-run:"
  Write-Host "  targetDir: $targetDir"
  Write-Host "  would write: $cmdPath"
  Write-Host "  would write: $ps1Path"
  exit 0
}

Set-Content -Path $cmdPath -Value $cmdContent -Encoding ASCII
Set-Content -Path $ps1Path -Value $ps1Content -Encoding ASCII

$pathParts = ($env:Path -split ';' | Where-Object { $_ -and $_.Trim() }) | ForEach-Object { $_.Trim().ToLowerInvariant() }
$onPath = $pathParts -contains $targetDir.ToLowerInvariant()

Write-Host "Installed:"
Write-Host "  $cmdPath"
Write-Host "  $ps1Path"
if (-not $onPath) {
  Write-Host ""
  Write-Host "Add this directory to PATH for direct command use:"
  Write-Host "  $targetDir"
}
Write-Host ""
Write-Host "Try:"
Write-Host "  spiral-coder-lite list-providers"
