Param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$Args
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$python = if ($env:SPIRAL_CODER_HF_PYTHON -and $env:SPIRAL_CODER_HF_PYTHON.Trim()) {
  $env:SPIRAL_CODER_HF_PYTHON
} else {
  "python"
}

& $python ".\scripts\spiral_coder_lite_cli.py" @Args
