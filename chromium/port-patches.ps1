<#
  Checks which chromium\patches\*.patch still apply to a new Chromium tag, without a 120 GB checkout.
  Fetches only the files the patches touch from chromium.googlesource.com into a scratch git repo,
  commits them, then applies the patches in order (git apply --check, then --3way, then --reject
  to list the failing hunks) and prints one line per patch.

    powershell -ExecutionPolicy Bypass -File chromium\port-patches.ps1 -Tag 159.0.8200.10
    ... -Keep   # keep the scratch repo (default: %TEMP%\nebula-port-<Tag>) to fix the .rej files by hand

  Exit code: 0 all patches apply, 1 some do not (or a file is gone from the tag).
#>
param(
    [Parameter(Mandatory)][string]$Tag,
    [string]$Scratch = (Join-Path $env:TEMP "nebula-port-$Tag"),
    [switch]$Keep
)
$ErrorActionPreference = "Stop"
$patches = Get-ChildItem (Join-Path $PSScriptRoot "patches") -Filter *.patch | Sort-Object Name
if (Test-Path $Scratch) { Remove-Item -Recurse -Force $Scratch }
New-Item -ItemType Directory $Scratch | Out-Null
Set-Location $Scratch
git init -q; git config core.autocrlf false; git config user.email n@n; git config user.name nebula

# Files to fetch = every file a patch modifies (files a patch creates do not exist upstream).
$files = foreach ($p in $patches) {
    $cur = $null
    foreach ($l in Get-Content $p.FullName) {
        if ($l -match '^diff --git a/(.+) b/') { $cur = $Matches[1] }
        elseif ($l -match '^new file mode') { $cur = $null }
        elseif ($l -match '^--- a/' -and $cur) { $cur }
    }
}
$files = $files | Sort-Object -Unique
$missing = @()
foreach ($f in $files) {
    $url = "https://chromium.googlesource.com/chromium/src/+/refs/tags/$Tag/${f}?format=TEXT"
    $bytes = $null
    foreach ($try in 1..3) {
        try { $bytes = [Convert]::FromBase64String((Invoke-WebRequest $url -UseBasicParsing).Content); break }
        catch { if ($try -eq 3) { Write-Host "MISSING $f ($($_.Exception.Message))" -ForegroundColor Red; $missing += $f } else { Start-Sleep 2 } }
    }
    if ($bytes) {
        $dst = Join-Path $Scratch $f
        New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
        [IO.File]::WriteAllBytes($dst, $bytes)
    }
}
git add -A; git commit -q -m "chromium $Tag (touched files only)"

$bad = 0
foreach ($p in $patches) {
    git apply --check $p.FullName 2>$null
    if ($LASTEXITCODE -eq 0) { git apply $p.FullName; git add -A; Write-Host "OK      $($p.Name)" -ForegroundColor Green; continue }
    git apply --3way $p.FullName 2>$null   # works only when the old blobs are known; usually it is not
    if ($LASTEXITCODE -eq 0) { git add -A; Write-Host "OK(3way) $($p.Name)" -ForegroundColor Yellow; continue }
    $bad++
    Write-Host "FAIL    $($p.Name)" -ForegroundColor Red
    git apply --reject $p.FullName 2>&1 | Where-Object { $_ -match 'Rejected hunk|error:' } | ForEach-Object { Write-Host "          $_" }
    git add -A   # later patches are still tried on top of whatever applied
}
if ($missing) { Write-Host "Files gone from ${Tag}: $($missing -join ', ')" -ForegroundColor Red }
Write-Host ($(if ($bad -or $missing) { "`n$bad patch(es) need porting. Scratch repo with .rej files: $Scratch" } else { "`nAll $($patches.Count) patches apply to $Tag." }))
if (-not $Keep -and -not $bad -and -not $missing) { Set-Location $env:TEMP; Remove-Item -Recurse -Force $Scratch }
exit [int]([bool]($bad -or $missing))
