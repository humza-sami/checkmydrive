# Build the engine for one target triple and install it as the Tauri sidecar.
# Usage: scripts/build-sidecar.ps1 [target-triple]   (default: host triple)
param([string]$Triple)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not $Triple) { $Triple = ((rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value) }
$Ext = ""; if ($Triple -like "*windows*") { $Ext = ".exe" }

Push-Location "$Root/engine"
try { cargo build --release --target $Triple; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } } finally { Pop-Location }

$BinDir = "$Root/app/src-tauri/binaries"
New-Item -ItemType Directory -Force $BinDir | Out-Null
Copy-Item "$Root/engine/target/$Triple/release/checkmydrive-engine$Ext" "$BinDir/checkmydrive-engine-$Triple$Ext" -Force
Write-Host "sidecar: $BinDir/checkmydrive-engine-$Triple$Ext"
