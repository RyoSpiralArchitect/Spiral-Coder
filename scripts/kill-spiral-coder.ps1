param(
  [switch]$Force = $true,
  [string]$PathContains = ""
)

$procs = Get-Process -Name spiral-coder -ErrorAction SilentlyContinue
$needle = ""
if ($PathContains) {
  try { $needle = (Resolve-Path $PathContains).Path } catch { $needle = $PathContains }
  $procs = $procs | Where-Object { $_.Path -and ($_.Path -like ("*" + $needle + "*")) }
}
if (-not $procs) {
  if ($needle) { Write-Host "[kill-spiral-coder] no spiral-coder process (filtered)" }
  else { Write-Host "[kill-spiral-coder] no spiral-coder process" }
  exit 0
}

Write-Host ("[kill-spiral-coder] stopping {0} process(es)..." -f $procs.Count)
if ($Force) {
  $procs | Stop-Process -Force
} else {
  $procs | Stop-Process
}
Write-Host "[kill-spiral-coder] done"
