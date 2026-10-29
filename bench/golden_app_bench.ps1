# Phase 0 checks 1, 3 and 4 (viewport, transfer, editing) on a golden pit: starts the release app
# in bench mode with the surfaces a pit.json names (topography, large_surface, and each seam's roof
# and floor), then waits for it to finish. Keep the window in front while it runs: a hidden window
# throttles its frame rate. Results land in bench/out/app-results.json.
#
#   .\bench\golden_app_bench.ps1 testdata\golden\<pit>\pit.json
param([Parameter(Mandatory = $true)][string]$Pit)

$repo = Split-Path -Parent $PSScriptRoot
$config = Get-Content -Raw -Path $Pit | ConvertFrom-Json
$surfaces = @()
foreach ($key in 'topography', 'large_surface') {
    if ($config.$key) { $surfaces += $config.$key }
}
foreach ($seam in $config.seams) { $surfaces += $seam.roof, $seam.floor }
$missing = $surfaces | Where-Object { -not (Test-Path $_) }
if ($missing) { throw "missing: $($missing -join ', ')" }

$exe = Join-Path $repo 'target\release\zd-app.exe'
Write-Host "Bench on $($surfaces.Count) surfaces; keep the window in front until it closes."
Start-Process -FilePath $exe -ArgumentList '--bench', '--surfaces', "`"$($surfaces -join ',')`"" -Wait
Write-Host "Results: $(Join-Path $repo 'bench\out\app-results.json')"
