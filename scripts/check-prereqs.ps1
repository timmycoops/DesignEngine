# check-prereqs.ps1 - checks the Windows toolchain for the Zovlyn Pit Design Engine.
# Run from the repo root:  powershell -ExecutionPolicy Bypass -File scripts\check-prereqs.ps1
# Read-only: it installs nothing. It prints what is missing and the command to install it.

$ErrorActionPreference = 'SilentlyContinue'
$results = @()

function Add-Result($name, $ok, $detail, $fix) {
    $script:results += [pscustomobject]@{ Item = $name; OK = $ok; Detail = $detail; Fix = $fix }
}

function Get-Version($cmd, $argList) {
    $exe = Get-Command $cmd -ErrorAction SilentlyContinue
    if (-not $exe) { return $null }
    $out = & $cmd @argList 2>&1 | Select-Object -First 1
    return "$out".Trim()
}

# Rust
$rustc = Get-Version 'rustc' @('--version')
Add-Result 'Rust (rustc)' ([bool]$rustc) $rustc 'winget install Rustlang.Rustup   (then restart the terminal)'

$host_triple = $null
if (Get-Command rustc -ErrorAction SilentlyContinue) {
    $hostLine = rustc -vV 2>$null | Select-String '^host:' | Select-Object -First 1
    if ($hostLine) { $host_triple = $hostLine.Line.Replace('host:', '').Trim() }
}
$msvc = $host_triple -like '*msvc*'
Add-Result 'Rust MSVC toolchain' $msvc $host_triple 'rustup default stable-msvc'

# Visual Studio Build Tools with the C++ workload
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vcPath = $null
if (Test-Path $vswhere) {
    $vcPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
Add-Result 'C++ Build Tools' ([bool]$vcPath) $vcPath 'Install Visual Studio Build Tools and tick "Desktop development with C++"'

# CMake (Manifold builds from source)
$cmake = Get-Version 'cmake' @('--version')
Add-Result 'CMake' ([bool]$cmake) $cmake 'winget install Kitware.CMake'

# Node.js LTS (20+)
$node = Get-Version 'node' @('--version')
$nodeOk = $false
if ($node -match 'v(\d+)\.') { $nodeOk = [int]$Matches[1] -ge 20 }
Add-Result 'Node.js 20+' $nodeOk $node 'winget install OpenJS.NodeJS.LTS'

# pnpm
$pnpm = Get-Version 'pnpm' @('--version')
Add-Result 'pnpm' ([bool]$pnpm) $pnpm 'corepack enable   (run in an Administrator terminal if it fails)'

# Git
$git = Get-Version 'git' @('--version')
Add-Result 'Git for Windows' ([bool]$git) $git 'winget install Git.Git'

# GitHub CLI + auth
$gh = Get-Version 'gh' @('--version')
Add-Result 'GitHub CLI' ([bool]$gh) $gh 'winget install GitHub.cli'
if ($gh) {
    gh auth status 2>$null | Out-Null
    $ghAuth = ($LASTEXITCODE -eq 0)
    Add-Result 'GitHub CLI signed in' $ghAuth '' 'gh auth login'
}

# Claude Code
$claude = Get-Version 'claude' @('--version')
Add-Result 'Claude Code' ([bool]$claude) $claude 'irm https://claude.ai/install.ps1 | iex'

# WebView2 runtime
$wv2Keys = @(
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
    'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
)
$wv2 = $null
foreach ($k in $wv2Keys) {
    $v = (Get-ItemProperty -Path $k -Name pv -ErrorAction SilentlyContinue).pv
    if ($v -and $v -ne '0.0.0.0') { $wv2 = $v; break }
}
Add-Result 'WebView2 runtime' ([bool]$wv2) $wv2 'Normally preinstalled; otherwise install the Evergreen WebView2 Runtime'

# Repo location: avoid OneDrive (Rust build folders are large)
$here = (Get-Location).Path
$inOneDrive = $here -match 'OneDrive'
Add-Result 'Repo outside OneDrive' (-not $inOneDrive) $here 'Move the repo to a local folder such as D:\Dev\zovlyn-pitdesign'

# Report
Write-Host ''
Write-Host 'Zovlyn Pit Design Engine - prerequisite check' -ForegroundColor Cyan
Write-Host ''
foreach ($r in $results) {
    if ($r.OK) {
        Write-Host ('  [ok]   {0,-24} {1}' -f $r.Item, $r.Detail) -ForegroundColor Green
    } else {
        Write-Host ('  [miss] {0,-24} fix: {1}' -f $r.Item, $r.Fix) -ForegroundColor Yellow
    }
}
$missing = @($results | Where-Object { -not $_.OK }).Count
Write-Host ''
if ($missing -eq 0) {
    Write-Host 'All set. Start Claude Code with:  claude' -ForegroundColor Green
} else {
    Write-Host ("$missing item(s) to fix. Install them, open a new terminal, and run this again.") -ForegroundColor Yellow
}
