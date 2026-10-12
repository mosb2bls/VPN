<#
.SYNOPSIS
  Network-state snapshot / compare for the Windows checklist (docs/TESTING.md §3, W-BASELINE).

.DESCRIPTION
  Captures what EdgeClient may change and must restore: routes, adapters, DNS servers,
  NRPT rules, IPv6 bindings, WinINet proxy settings (registry + DefaultConnectionSettings),
  WinHTTP proxy, firewall rules with our names, the helper service, and (elevated only) the
  number of our WFP kill-switch filters.

  Console output is only section names with "same" / "DIFFERENT" (and counts), never
  addresses, so it can be pasted into a report as is. The JSON files DO contain local
  addresses (gateway, DNS servers): keep them on the PC; never commit or paste them.

  Exit codes: 0 identical (or no -Compare), 3 sections differ, 1 script error.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\windows\net-snapshot.ps1 -Out C:\edge-test\before.json
  powershell -ExecutionPolicy Bypass -File scripts\windows\net-snapshot.ps1 -Out C:\edge-test\after.json -Compare C:\edge-test\before.json
#>
param(
  [Parameter(Mandatory = $true)][string]$Out,
  [string]$Compare
)
$ErrorActionPreference = 'Stop'
$App = 'EdgeClient'                                  # edge_core::APP_NAME
$Service = 'io.github.mosb2bls.edgeclient.helper'    # helper_proto::WIN_SERVICE_NAME

function Get-Sorted($items) {
  @($items | Sort-Object { $_ | ConvertTo-Json -Compress -Depth 4 })
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
  [Security.Principal.WindowsBuiltInRole]::Administrator)

$snap = [ordered]@{}
$snap.adapters = Get-Sorted (Get-NetAdapter | Select-Object Name, InterfaceDescription, @{ n = 'Status'; e = { "$($_.Status)" } })
$snap.routes = Get-Sorted (Get-NetRoute -ErrorAction SilentlyContinue |
    Select-Object DestinationPrefix, NextHop, InterfaceAlias, RouteMetric)
$snap.dnsServers = Get-Sorted (Get-DnsClientServerAddress -ErrorAction SilentlyContinue |
    Select-Object InterfaceAlias, AddressFamily, @{ n = 'Servers'; e = { $_.ServerAddresses -join ',' } })
$snap.nrpt = Get-Sorted (Get-DnsClientNrptPolicy -ErrorAction SilentlyContinue |
    Select-Object @{ n = 'Namespace'; e = { $_.Namespace -join ',' } }, @{ n = 'NameServers'; e = { $_.NameServers -join ',' } })
$snap.ipv6Bindings = Get-Sorted (Get-NetAdapterBinding -ComponentID ms_tcpip6 -ErrorAction SilentlyContinue |
    Select-Object Name, Enabled)

$is = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$snap.internetSettings = [ordered]@{
  ProxyEnable   = $is.ProxyEnable
  ProxyServer   = $is.ProxyServer
  ProxyOverride = $is.ProxyOverride
  AutoConfigURL = $is.AutoConfigURL
  AutoDetect    = $is.AutoDetect
}
# DefaultConnectionSettings: bytes 4..7 are a change counter that Windows bumps on every
# write, so compare a hash of everything else.
$conn = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections' -ErrorAction SilentlyContinue).DefaultConnectionSettings
if ($conn -and $conn.Length -gt 8) {
  $rest = [byte[]]($conn[0..3] + $conn[8..($conn.Length - 1)])
  $snap.defaultConnectionSettings = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($rest))
} else {
  $snap.defaultConnectionSettings = $null
}
$snap.winhttpProxy = ((netsh winhttp show proxy) -join "`n").Trim()

$snap.firewallRules = Get-Sorted (Get-NetFirewallRule -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -like "*$App*" -or $_.DisplayName -like '*sing-box*' -or $_.DisplayName -like 'EdgeTest*' } |
    Select-Object DisplayName, @{ n = 'Enabled'; e = { "$($_.Enabled)" } }, @{ n = 'Direction'; e = { "$($_.Direction)" } }, @{ n = 'Action'; e = { "$($_.Action)" } })
$svc = Get-Service -Name $Service -ErrorAction SilentlyContinue
$snap.helperService = if ($svc) { "$($svc.Status) / $($svc.StartType)" } else { 'not installed' }
$snap.helperFiles = [ordered]@{
  programFiles = Test-Path (Join-Path $env:ProgramFiles "$App\Helper")
  programData  = Test-Path (Join-Path $env:ProgramData "$App\Helper")
}
if ($isAdmin) {
  $tmp = Join-Path $env:TEMP 'edge-wfp-filters.xml'
  netsh wfp show filters file="$tmp" | Out-Null
  $snap.wfpKillSwitchFilters = ([regex]::Matches((Get-Content $tmp -Raw), [regex]::Escape("$App kill switch"))).Count
  Remove-Item $tmp -Force
} else {
  $snap.wfpKillSwitchFilters = 'not checked (run elevated)'
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$snap | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $Out
Write-Output "snapshot written: $Out (contains local addresses - do not share)"
Write-Output ("helperService: {0}; helperFiles: programFiles={1} programData={2}; wfpKillSwitchFilters: {3}" -f
  $snap.helperService, $snap.helperFiles.programFiles, $snap.helperFiles.programData, $snap.wfpKillSwitchFilters)

if ($Compare) {
  $before = Get-Content $Compare -Raw | ConvertFrom-Json
  $after = Get-Content $Out -Raw | ConvertFrom-Json
  $diff = 0
  foreach ($name in $snap.Keys) {
    $a = $before.$name | ConvertTo-Json -Compress -Depth 6
    $b = $after.$name | ConvertTo-Json -Compress -Depth 6
    if ($a -eq $b) {
      Write-Output ("{0,-26} same" -f $name)
    } else {
      $diff++
      Write-Output ("{0,-26} DIFFERENT" -f $name)
    }
  }
  if ($diff -eq 0) { Write-Output 'RESULT: identical' } else { Write-Output "RESULT: $diff section(s) differ" }
  # 0 = identical, 3 = differences (1 = the script itself failed).
  if ($diff -eq 0) { exit 0 } else { exit 3 }
}
