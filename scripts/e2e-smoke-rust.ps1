Param(
  [Alias("Host")]
  [string]$ListenHost = "127.0.0.1",
  [int]$Port = 18091,
  [string]$WorkspaceRoot = ""
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$ws = if ($WorkspaceRoot -and $WorkspaceRoot.Trim()) {
  $WorkspaceRoot.Trim()
} else {
  Join-Path (Resolve-Path ".tmp") "e2e-smoke-rust-work"
}

New-Item -ItemType Directory -Force -Path $ws | Out-Null

# Use an isolated target dir to avoid the common Windows dev failure:
# cargo cannot overwrite `spiral-coder.exe` if another instance is running from the same target dir.
$env:CARGO_TARGET_DIR = (Join-Path $repoRoot ".tmp\\cargo-target-e2e-rust")
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null

& (Join-Path $PSScriptRoot "kill-spiral-coder.ps1") -PathContains $env:CARGO_TARGET_DIR | Out-Null

Write-Host "[e2e-smoke-rust] cargo build" -ForegroundColor Cyan
cargo build -q

$exe = Join-Path $env:CARGO_TARGET_DIR "debug\\spiral-coder.exe"
if (-not (Test-Path $exe)) {
  throw "expected exe missing: $exe"
}

Write-Host "Starting Rust server: http://$ListenHost`:$Port/  workspace=$ws" -ForegroundColor Cyan
$p = Start-Process -FilePath $exe -ArgumentList @(
  "serve",
  "--host", $ListenHost,
  "--port", $Port
) -PassThru -WindowStyle Hidden -WorkingDirectory $ws

try {
  Start-Sleep -Milliseconds 900

  $st = Invoke-RestMethod -Method Get -Uri "http://$ListenHost`:$Port/api/status" -TimeoutSec 5
  if (-not $st.ok) { throw "status.ok is false" }

  $assets = @(
    "/",
    "/assets/styles.css?v=20260301",
    "/assets/app.js?v=20260301",
    "/assets/core/sandbox.js?v=20260301",
    "/assets/core/exec.js?v=20260301",
    "/assets/observer/logic.js?v=20260301"
  )
  foreach ($path in $assets) {
    $r = Invoke-WebRequest -Method Get -Uri ("http://$ListenHost`:$Port" + $path) -UseBasicParsing -TimeoutSec 5
    if ($r.StatusCode -ne 200) {
      throw "GET $path failed: HTTP $($r.StatusCode)"
    }
  }

  $cwd = Join-Path $ws "work"
  $body = @{
    command = "New-Item -ItemType Directory -Force -Path 'demo-repo' | Out-Null; Set-Location 'demo-repo'; git init"
    cwd = $cwd
  } | ConvertTo-Json

  $ex = Invoke-RestMethod -Method Post -Uri "http://$ListenHost`:$Port/api/exec" -ContentType "application/json" -Body $body -TimeoutSec 15
  if ($ex.exit_code -ne 0) {
    throw "exec failed: exit_code=$($ex.exit_code) stderr=$($ex.stderr)"
  }

  $gitDir = Join-Path (Join-Path $cwd "demo-repo") ".git"
  if (-not (Test-Path $gitDir)) {
    throw "expected git dir missing: $gitDir"
  }

  # Transcript sanitization: models often paste prompts + output lines into code fences.
  # The server should execute ONLY the prompt lines and ignore output.
  $cwd2 = Join-Path $ws "work2"
  $cmd2 = @"
PS> New-Item -ItemType Directory -Force -Path 'demo-repo2' | Out-Null
ディレクトリ: C:\fake\output\should\be\ignored
PS> Set-Location 'demo-repo2'
PS> git init
Initialized empty Git repository in C:/fake/output/ignored/.git/
"@.Trim()
  $body2 = @{
    command = $cmd2
    cwd = $cwd2
  } | ConvertTo-Json
  $ex2 = Invoke-RestMethod -Method Post -Uri "http://$ListenHost`:$Port/api/exec" -ContentType "application/json" -Body $body2 -TimeoutSec 20
  if ($ex2.exit_code -ne 0) {
    throw "exec (transcript sanitize) failed: exit_code=$($ex2.exit_code) stderr=$($ex2.stderr)"
  }
  $gitDir2 = Join-Path (Join-Path $cwd2 "demo-repo2") ".git"
  if (-not (Test-Path $gitDir2)) {
    throw "expected git dir missing (transcript sanitize): $gitDir2"
  }

  Write-Host "E2E smoke (Rust) OK: $gitDir" -ForegroundColor Green
  exit 0
} finally {
  try { Stop-Process -Id $p.Id -Force } catch {}
}
