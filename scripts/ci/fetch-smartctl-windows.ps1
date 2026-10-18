# Fetch the official smartmontools Windows installer and extract the 64-bit smartctl.exe.
# Usage: fetch-smartctl-windows.ps1 -Out <path\smartctl.exe> [-Version 7.5]
param(
  [Parameter(Mandatory = $true)][string]$Out,
  [string]$Version = $(if ($env:SMARTMONTOOLS_VERSION) { $env:SMARTMONTOOLS_VERSION } else { '7.5' })
)
$ErrorActionPreference = 'Stop'

$tag = 'RELEASE_' + $Version.Replace('.', '_')
$name = "smartmontools-$Version.win32-setup.exe"
if ($Version -ne '7.5') { $name = "smartmontools-$Version-1.win32-setup.exe" }
$url = "https://github.com/smartmontools/smartmontools/releases/download/$tag/$name"

$work = Join-Path ([IO.Path]::GetTempPath()) ("smartctl-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
$installer = Join-Path $work 'setup.exe'
Write-Host "Downloading $url"
Invoke-WebRequest -Uri $url -OutFile $installer

$sevenZip = (Get-Command 7z -ErrorAction SilentlyContinue).Source
if (-not $sevenZip) { $sevenZip = 'C:\Program Files\7-Zip\7z.exe' }
$extract = Join-Path $work 'x'
& $sevenZip x $installer "-o$extract" -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw "7z extraction failed" }

function Get-PeMachine([string]$path) {
  $b = [IO.File]::ReadAllBytes($path)
  $off = [BitConverter]::ToInt32($b, 0x3C)
  return [BitConverter]::ToUInt16($b, $off + 4)
}

# The installer layout is not guaranteed (bin / bin64), so pick by PE machine type: x64 = 0x8664.
$candidates = @(Get-ChildItem -Path $extract -Recurse -Filter smartctl.exe)
if ($candidates.Count -eq 0) { throw "smartctl.exe not found in installer" }
$candidates | ForEach-Object { Write-Host ("candidate: {0} (machine 0x{1:X4})" -f $_.FullName, (Get-PeMachine $_.FullName)) }
$exe = $candidates | Where-Object { (Get-PeMachine $_.FullName) -eq 0x8664 } | Select-Object -First 1
if (-not $exe) {
  Write-Warning "No x64 smartctl.exe in installer; falling back to the first candidate (32-bit builds run fine under WOW64)."
  $exe = $candidates | Select-Object -First 1
}
Write-Host "Using $($exe.FullName)"

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
Copy-Item $exe.FullName $Out -Force
Write-Host ("Output PE machine: 0x{0:X4}" -f (Get-PeMachine $Out))
Remove-Item -Recurse -Force $work
