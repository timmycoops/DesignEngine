# install-prereqs.ps1 - installs the Windows toolchain for the Zovlyn Pit Design Engine.
#
# Run from an Administrator PowerShell in the repo root:
#   powershell -ExecutionPolicy Bypass -File scripts\install-prereqs.ps1
#
# Safe to re-run: anything already installed is skipped. Uses winget.
# The Visual Studio C++ Build Tools are a large download (several GB) and take the longest.

$ErrorActionPreference = 'Continue'

$script:done = @()
$script:skipped = @()
$script:failed = @()

function Test-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($id)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin'
    $env:Path = "$machine;$user;$cargo"
}

function Test-Cmd($name) {
    return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

function Install-Package($label, $id, [string[]]$extra = @()) {
    Write-Host ''
    Write-Host "Installing $label ($id) ..." -ForegroundColor Cyan
    $wingetArgs = @('install', '--id', $id, '--exact', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements') + $extra
    & winget @wingetArgs
    if ($LASTEXITCODE -eq 0) {
        $script:done += $label
    } else {
        $script:failed += "$label (winget exit code $LASTEXITCODE)"
    }
    Update-SessionPath
}

function Skip($label, $detail) {
    Write-Host ("  [have] {0,-26} {1}" -f $label, $detail) -ForegroundColor Green
    $script:skipped += $label
}

Write-Host ''
Write-Host 'Zovlyn Pit Design Engine - prerequisite installer' -ForegroundColor Cyan

if (-not (Test-Admin)) {
    Write-Host ''
    Write-Host 'Not running as Administrator. Installs will ask for approval one by one,' -ForegroundColor Yellow
    Write-Host 'and the C++ Build Tools may fail. Best: close this, open PowerShell with' -ForegroundColor Yellow
    Write-Host '"Run as administrator", and run the script again.' -ForegroundColor Yellow
    Write-Host ''
}

if (-not (Test-Cmd 'winget')) {
    Write-Host 'winget is not available. Install "App Installer" from the Microsoft Store, then run this again.' -ForegroundColor Red
    exit 1
}

Update-SessionPath

# 1. Visual Studio C++ Build Tools (Tauri, Rust MSVC toolchain, Manifold's C++ build)
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vcPath = $null
$anyVsPath = $null
if (Test-Path $vswhere) {
    $vcPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    $anyVsPath = & $vswhere -latest -products * -property installationPath
}
if ($vcPath) {
    Skip 'C++ Build Tools' $vcPath
} elseif ($anyVsPath) {
    # Visual Studio or Build Tools exist but without the C++ workload: add it in place.
    Write-Host ''
    Write-Host "Adding the C++ workload to the existing install at $anyVsPath ..." -ForegroundColor Cyan
    $setup = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\setup.exe'
    $p = Start-Process -FilePath $setup -Wait -PassThru -ArgumentList @(
        'modify', '--installPath', "`"$anyVsPath`"",
        '--add', 'Microsoft.VisualStudio.Workload.VCTools', '--includeRecommended', '--passive', '--norestart')
    if ($p.ExitCode -eq 0 -or $p.ExitCode -eq 3010) { $script:done += 'C++ Build Tools (workload added)' }
    else { $script:failed += "C++ Build Tools workload (installer exit code $($p.ExitCode))" }
} else {
    Install-Package 'C++ Build Tools' 'Microsoft.VisualStudio.2022.BuildTools' @(
        '--override', '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
}

# 2. Rust (rustup) with the MSVC toolchain as default
if ((Test-Cmd 'rustup') -or (Test-Path (Join-Path $env:USERPROFILE '.cargo\bin\rustup.exe'))) {
    Skip 'Rust (rustup)' ((rustc --version) 2>$null)
} else {
    Install-Package 'Rust (rustup)' 'Rustlang.Rustup'
}
if (Test-Cmd 'rustup') {
    rustup default stable-msvc
    rustup component add clippy rustfmt
}

# 3. CMake (Manifold builds from source on the first compile)
if (Test-Cmd 'cmake') { Skip 'CMake' ((cmake --version | Select-Object -First 1)) }
else { Install-Package 'CMake' 'Kitware.CMake' }

# 4. Node.js LTS (20 or later)
$nodeOk = $false
if (Test-Cmd 'node') {
    $nodeVersion = (node --version)
    if ($nodeVersion -match 'v(\d+)\.') { $nodeOk = [int]$Matches[1] -ge 20 }
}
if ($nodeOk) { Skip 'Node.js' $nodeVersion }
else { Install-Package 'Node.js LTS' 'OpenJS.NodeJS.LTS' }

# 5. pnpm
if (Test-Cmd 'pnpm') { Skip 'pnpm' ((pnpm --version)) }
else { Install-Package 'pnpm' 'pnpm.pnpm' }

# 6. Git for Windows
if (Test-Cmd 'git') { Skip 'Git' ((git --version)) }
else { Install-Package 'Git for Windows' 'Git.Git' }

# 7. GitHub CLI
if (Test-Cmd 'gh') { Skip 'GitHub CLI' ((gh --version | Select-Object -First 1)) }
else { Install-Package 'GitHub CLI' 'GitHub.cli' }

# 8. WebView2 runtime (normally already part of Windows)
$wv2 = $null
foreach ($k in @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')) {
    $v = (Get-ItemProperty -Path $k -Name pv -ErrorAction SilentlyContinue).pv
    if ($v -and $v -ne '0.0.0.0') { $wv2 = $v; break }
}
if ($wv2) { Skip 'WebView2 runtime' $wv2 }
else { Install-Package 'WebView2 runtime' 'Microsoft.EdgeWebView2Runtime' }

# Summary
Write-Host ''
Write-Host '---------------------------------------------' -ForegroundColor Cyan
if ($script:done.Count) { Write-Host ('Installed: ' + ($script:done -join ', ')) -ForegroundColor Green }
if ($script:skipped.Count) { Write-Host ('Already had: ' + ($script:skipped -join ', ')) -ForegroundColor Green }
if ($script:failed.Count) {
    Write-Host ('Failed: ' + ($script:failed -join '; ')) -ForegroundColor Red
    Write-Host 'Re-run the script as Administrator, or install the failed items manually.' -ForegroundColor Yellow
}
Write-Host ''
Write-Host 'Next:' -ForegroundColor Cyan
Write-Host '  1. Close this window and open a NEW PowerShell so the PATH changes apply.'
Write-Host '  2. Sign in to GitHub once:  gh auth login'
Write-Host '  3. Confirm everything:      powershell -ExecutionPolicy Bypass -File scripts\check-prereqs.ps1'
