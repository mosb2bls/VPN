<#
.SYNOPSIS
  Exit check for the Windows checklist (docs/TESTING.md §3): exit country plus whether the
  exit address equals the PC's own direct exit, without printing any address.

.DESCRIPTION
  -Via system : request through the Windows (WinINet) proxy settings, like browsers do.
                Always run this script as a new process (as below): Windows PowerShell caches
                the proxy per process, so a check inside a long-lived window can silently go
                direct after connecting.
  -Via direct : curl.exe, which ignores the Windows proxy (in TUN mode it still goes through
                the tunnel).

  -SaveBaseline (run once while DISCONNECTED): stores a SHA-256 of the direct exit address
  in <Dir>\direct-exit.hash. Later runs print exit=same / exit=DIFFERENT against it. The hash
  file stays on the PC; never commit or paste it.

  -Cn : instead of the exit check, fetch a CN site (https://www.baidu.com) and print its
        HTTP status.

  Output is one line, e.g. "loc=GB exit=DIFFERENT via=system" or "cn=200 via=direct".

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows\exit-check.ps1 -Via direct -SaveBaseline
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows\exit-check.ps1 -Via system
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows\exit-check.ps1 -Via system -Cn
#>
param(
  [ValidateSet('system', 'direct')][string]$Via = 'system',
  [switch]$SaveBaseline,
  [switch]$Cn,
  [string]$Dir = 'C:\edge-test'
)
$ErrorActionPreference = 'Stop'

function Get-Body([string]$url) {
  if ($Via -eq 'system') {
    $r = Invoke-WebRequest -UseBasicParsing -TimeoutSec 10 -Uri $url
    return @{ status = [int]$r.StatusCode; body = [string]$r.Content }
  }
  $tmp = [IO.Path]::GetTempFileName()
  try {
    $code = & curl.exe -s -m 10 -o $tmp -w '%{http_code}' $url
    return @{ status = [int]$code; body = (Get-Content $tmp -Raw) }
  } finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
}

if ($Cn) {
  try { $r = Get-Body 'https://www.baidu.com'; Write-Output "cn=$($r.status) via=$Via" }
  catch { Write-Output "cn=FAILED via=$Via ($($_.Exception.GetType().Name))" }
  exit 0
}

try { $r = Get-Body 'https://1.1.1.1/cdn-cgi/trace' }
catch { Write-Output "loc=FAILED via=$Via ($($_.Exception.GetType().Name))"; exit 0 }
$lines = ([string]$r.body) -split "`n"
$loc = ($lines | Where-Object { $_ -like 'loc=*' } | Select-Object -First 1)
$ipLine = ($lines | Where-Object { $_ -like 'ip=*' } | Select-Object -First 1)
# A failed request (e.g. curl while the kill switch blocks) has no ip= line (round 3, F-6).
if ($r.status -ne 200 -or -not $ipLine) { Write-Output "loc=FAILED via=$Via (http $($r.status))"; exit 0 }
$ip = ([string]$ipLine).Substring(3).Trim()
if (-not $loc) { $loc = 'loc=?' }
$hash = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($ip)))
$file = Join-Path $Dir 'direct-exit.hash'

if ($SaveBaseline) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  Set-Content -Path $file -Value $hash -Encoding ASCII
  Write-Output "$loc baseline=saved via=$Via (run this only while disconnected)"
  exit 0
}
if (-not (Test-Path $file)) { Write-Output "$loc exit=? via=$Via (no baseline: run -SaveBaseline while disconnected)"; exit 0 }
$same = ((Get-Content $file -Raw).Trim() -eq $hash)
Write-Output ("{0} exit={1} via={2}" -f $loc, $(if ($same) { 'same' } else { 'DIFFERENT' }), $Via)
