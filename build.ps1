<#
  Builds the whole Nebula Sessions package in one go:
    1. downloads depot_tools and the Chromium source at -Version (default: the version the original ships),
    2. applies chromium\patches\*.patch (native device values: cores, memory, WebGL card, client hints),
    3. builds Chromium (chrome target, release),
    4. builds the session manager (cargo, release),
    5. copies both into dist\Nebula and zips it.

  Needs: Windows 10/11 x64, Visual Studio 2022 with "Desktop development with C++" and the
  Windows 11 SDK incl. "Debugging Tools for Windows", git, Rust (cargo), ~120 GB free disk
  and several hours for the first Chromium build. Re-runs reuse the checkout and only rebuild what changed.

  Examples:
    powershell -ExecutionPolicy Bypass -File build.ps1
    powershell -ExecutionPolicy Bypass -File build.ps1 -SkipChromium   # only the manager, reuse an existing Chromium build
#>
param(
    [string]$Version = "158.0.8127.43",
    [string]$Work = "C:\nebula-chromium",   # keep it short: Chromium paths get long
    [switch]$SkipChromium
)
$ErrorActionPreference = "Stop"
$App = $PSScriptRoot
$Out = Join-Path $Work "src\out\Nebula"

function Step($t) { Write-Host "`n==> $t" -ForegroundColor Cyan }
function Run($exe, [string[]]$a) {
    & $exe @a
    if ($LASTEXITCODE -ne 0) { throw "$exe $($a -join ' ') failed with exit code $LASTEXITCODE" }
}

# Git settings Chromium needs, for this script's processes only (your global git config is untouched).
$env:GIT_CONFIG_COUNT = "2"
$env:GIT_CONFIG_KEY_0 = "core.autocrlf"; $env:GIT_CONFIG_VALUE_0 = "false"
$env:GIT_CONFIG_KEY_1 = "core.longpaths"; $env:GIT_CONFIG_VALUE_1 = "true"

if (-not $SkipChromium) {
    Step "Checking prerequisites"
    foreach ($tool in "git", "python", "cargo") {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is not installed or not on PATH" }
    }
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vs = if (Test-Path $vswhere) { & $vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath }
    if (-not $vs) { throw "Visual Studio 2022 with 'Desktop development with C++' is required" }
    $env:vs2022_install = $vs
    New-Item -ItemType Directory -Force $Work | Out-Null
    $freeGB = [math]::Floor((Get-PSDrive ($Work.Substring(0, 1))).Free / 1GB)
    if (-not (Test-Path (Join-Path $Work "src")) -and $freeGB -lt 120) { throw "Only $freeGB GB free on $($Work.Substring(0,2)); the Chromium checkout and build need about 120 GB" }

    Step "depot_tools"
    $depot = Join-Path $Work "depot_tools"
    if (-not (Test-Path $depot)) { Run git @("clone", "https://chromium.googlesource.com/chromium/tools/depot_tools.git", $depot) }
    $env:PATH = "$depot;$env:PATH"
    $env:DEPOT_TOOLS_WIN_TOOLCHAIN = "0"   # use the local Visual Studio, not Google's internal toolchain

    Step "Chromium source $Version"
    Push-Location $Work
    try {
        $marker = Join-Path $Work ".synced-$Version"
        if (-not (Test-Path $marker)) {
            Set-Content -Encoding ascii .gclient 'solutions = [{"name": "src", "url": "https://chromium.googlesource.com/chromium/src.git", "managed": False, "custom_deps": {}, "custom_vars": {}}]'
            Run gclient @("sync", "--revision", "src@refs/tags/$Version", "--no-history", "--nohooks", "-D", "--force", "--reset")
            Run gclient @("runhooks")
            Set-Content $marker $Version
        }

        Step "Applying patches"
        foreach ($p in Get-ChildItem (Join-Path $App "chromium\patches") -Filter *.patch | Sort-Object Name) {
            & git -C src apply --check $p.FullName 2>$null
            if ($LASTEXITCODE -eq 0) { Run git @("-C", "src", "apply", $p.FullName); Write-Host "applied $($p.Name)" ; continue }
            & git -C src apply --reverse --check $p.FullName 2>$null
            if ($LASTEXITCODE -eq 0) { Write-Host "already applied $($p.Name)"; continue }
            throw "$($p.Name) does not fit Chromium $Version"
        }

        Step "Building Chromium (the first build takes hours)"
        New-Item -ItemType Directory -Force $Out | Out-Null
        # Written as a file because PowerShell mangles quotes passed to native programs.
        Set-Content -Encoding ascii (Join-Path $Out "args.gn") @"
is_debug = false
is_component_build = false
is_official_build = false
symbol_level = 0
blink_symbol_level = 0
v8_symbol_level = 0
enable_nacl = false
chrome_pgo_phase = 0
proprietary_codecs = true
ffmpeg_branding = "Chrome"
treat_warnings_as_errors = false
"@
        Push-Location src
        try {
            Run gn @("gen", "out\Nebula")
            Run autoninja @("-C", "out\Nebula", "chrome")
        } finally { Pop-Location }
    } finally { Pop-Location }
}

Step "Building the session manager"
Push-Location $App
try { Run cargo @("build", "--release") } finally { Pop-Location }

Step "Packaging"
$Dist = Join-Path $App "dist\Nebula"
if (Test-Path $Dist) { Remove-Item -Recurse -Force $Dist }   # only our own previous output
New-Item -ItemType Directory -Force $Dist | Out-Null
Copy-Item (Join-Path $App "target\release\nebula-sessions.exe") $Dist
if (Test-Path (Join-Path $Out "chrome.exe")) {
    $cr = New-Item -ItemType Directory -Force (Join-Path $Dist "chromium")
    Copy-Item (Join-Path $Out "chrome.exe") $cr
    Get-ChildItem $Out -File | Where-Object { $_.Extension -in ".dll", ".pak", ".bin", ".dat", ".json", ".manifest" } | Copy-Item -Destination $cr
    Copy-Item -Recurse (Join-Path $Out "locales") $cr
    Set-Content -Encoding ascii (Join-Path $cr "VERSION") $Version   # the manager reads the version from here
    $zip = Join-Path $App "dist\Nebula-$Version-win64.zip"
} else {
    Write-Warning "No Chromium build found in ${Out}: packaging the manager alone (it will use an installed Chromium/Edge)."
    $zip = Join-Path $App "dist\Nebula-manager-win64.zip"
}
Compress-Archive -Path (Join-Path $Dist "*") -DestinationPath $zip -Force
Write-Host "`nDone: $Dist`n      $zip" -ForegroundColor Green
