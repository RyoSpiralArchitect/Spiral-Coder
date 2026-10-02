Param(
  [Alias("Host")]
  [string]$ListenHost = "127.0.0.1",
  [int]$Port = 18080,
  [string]$ApiKey = "",
  [string]$WorkspaceRoot = ""
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

# Avoid inheriting broken local proxy settings (for example 127.0.0.1:9).
$env:HTTP_PROXY = ""
$env:HTTPS_PROXY = ""
$env:ALL_PROXY = ""
$env:http_proxy = ""
$env:https_proxy = ""
$env:all_proxy = ""
$env:GIT_HTTP_PROXY = ""
$env:GIT_HTTPS_PROXY = ""

if ($ApiKey -and $ApiKey.Trim()) {
  $env:SPIRAL_CODER_API_KEY = $ApiKey.Trim()
}

$ws = if ($WorkspaceRoot -and $WorkspaceRoot.Trim()) {
  $WorkspaceRoot.Trim()
} elseif ($env:SPIRAL_CODER_WORKSPACE_ROOT -and $env:SPIRAL_CODER_WORKSPACE_ROOT.Trim()) {
  $env:SPIRAL_CODER_WORKSPACE_ROOT.Trim()
} else {
  (Join-Path $HOME "spiral-coder-work")
}

New-Item -ItemType Directory -Force -Path $ws | Out-Null
$env:SPIRAL_CODER_WORKSPACE_ROOT = $ws

$python = if ($env:SPIRAL_CODER_HF_PYTHON -and $env:SPIRAL_CODER_HF_PYTHON.Trim()) {
  $env:SPIRAL_CODER_HF_PYTHON
} else {
  "python"
}

Write-Host "Spiral-Coder Lite UI: http://$ListenHost`:$Port/"
Write-Host "Workspace root: $ws"
& $python ".\scripts\serve_lite.py" --host $ListenHost --port $Port --workspace $ws
