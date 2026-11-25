# Signs the files it is given, using whichever method the environment offers. Pluggable on purpose:
# the certificate route is still an owner decision (docs/plan/2026-11-25-afrobim-launch-and-product-roadmap.md, 10.1).
#
# Usage: pwsh sign.ps1 -Files a.exe,b.msi
#
# Method 1, custom command (cloud HSM, Azure Artifact Signing, SSL.com eSigner and so on):
#   WINDOWS_SIGN_COMMAND   a command line where the token {file} stands for the file to sign, for example
#                          sign code trusted-signing {file} --trusted-signing-endpoint ... (Azure) or any vendor CLI.
# Method 2, a PFX certificate (only where the key may be exported, which public CAs no longer allow
# for new certificates; useful for an internal test certificate):
#   WINDOWS_CERT_PFX_BASE64, WINDOWS_CERT_PASSWORD
# With neither set the script does nothing and exits 0, so unsigned builds still work.
param([Parameter(Mandatory = $true)][string[]]$Files)
$ErrorActionPreference = 'Stop'
$ts = if ($env:WINDOWS_TIMESTAMP_URL) { $env:WINDOWS_TIMESTAMP_URL } else { 'http://timestamp.digicert.com' }

if ($env:WINDOWS_SIGN_COMMAND) {
    foreach ($f in $Files) {
        $cmd = $env:WINDOWS_SIGN_COMMAND.Replace('{file}', "`"$((Resolve-Path $f).Path)`"")
        Write-Host "signing $f with the custom command"
        Invoke-Expression $cmd
        if ($LASTEXITCODE -ne 0) { throw "custom sign command failed for $f" }
    }
    exit 0
}

if ($env:WINDOWS_CERT_PFX_BASE64 -and $env:WINDOWS_CERT_PASSWORD) {
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $signtool) { throw 'signtool.exe not found on this runner' }
    $pfx = Join-Path $env:RUNNER_TEMP 'codesign.pfx'
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERT_PFX_BASE64))
    try {
        foreach ($f in $Files) {
            Write-Host "signing $f with the PFX certificate"
            & $signtool.FullName sign /fd SHA256 /f $pfx /p $env:WINDOWS_CERT_PASSWORD /tr $ts /td SHA256 $f
            if ($LASTEXITCODE -ne 0) { throw "signtool failed for $f" }
            & $signtool.FullName verify /pa $f
        }
    } finally { Remove-Item -Force $pfx -ErrorAction SilentlyContinue }
    exit 0
}

Write-Host 'No Windows signing method configured: files left unsigned.'
