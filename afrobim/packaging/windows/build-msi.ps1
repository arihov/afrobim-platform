# Builds the AfroBIM .msi with the WiX Toolset v3 command line tools (candle and light).
#
# Usage: pwsh build-msi.ps1 -Version 0.1.0 -Binary path\to\afrobim.exe -OutDir out [-NoFonts]
#
# cargo-wix was not used: it wraps the same two tools and adds its own conventions (a wix\ folder
# in the crate, its own variables), while calling candle and light directly keeps every step
# visible in this file. main.wxs is the same either way.
#
# Not run on Windows by the author. The first CI run is the first real test.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Binary,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [switch]$NoFonts
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$packaging = Split-Path -Parent $here

# Find the WiX v3 tools: the WIX environment variable is set by the WiX installer.
$wixBin = $null
if ($env:WIX -and (Test-Path (Join-Path $env:WIX 'bin\candle.exe'))) { $wixBin = Join-Path $env:WIX 'bin' }
if (-not $wixBin) {
    $found = Get-ChildItem "${env:ProgramFiles(x86)}\WiX Toolset v3*\bin\candle.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) { $wixBin = $found.DirectoryName }
}
if (-not $wixBin) { throw 'WiX Toolset v3 (candle.exe) was not found. Install it (choco install wixtoolset) or set WIX.' }
Write-Host "WiX tools: $wixBin"

# MSI versions are numeric a.b.c.d; drop any pre-release suffix such as -beta.1.
$msiVersion = ($Version -split '[-+]')[0]
if ($msiVersion -notmatch '^\d+\.\d+\.\d+$') { throw "Version must look like 1.2.3, got $Version" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("afrobim-msi-" + [guid]::NewGuid())
$stage = Join-Path $work 'stage'
New-Item -ItemType Directory -Force -Path $stage, $OutDir | Out-Null

# Stage with the same script every platform uses (bash ships with Git for Windows on the runner).
$stageSh = (Join-Path $packaging 'stage.sh') -replace '\\', '/'
$binFwd = (Resolve-Path $Binary).Path -replace '\\', '/'
$stageFwd = $stage -replace '\\', '/'
$stageArgs = @($stageSh, '--bin', $binFwd, '--dest', $stageFwd)
if ($NoFonts) { $stageArgs += '--no-fonts' }
$bash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'   # not the WSL bash that may come first on PATH
if (-not (Test-Path $bash)) { $bash = 'bash' }
& $bash @stageArgs
if ($LASTEXITCODE -ne 0) { throw 'stage.sh failed' }
if (-not (Test-Path (Join-Path $stage 'afrobim.exe'))) { throw 'staged tree has no afrobim.exe (is -Binary the .exe?)' }

$defs = @("-dVersion=$msiVersion", "-dStageDir=$stage", "-dIconPath=$(Join-Path $packaging 'icons\afrobim.ico')")
if (Test-Path (Join-Path $stage 'fonts')) { $defs += '-dWithFonts=1' }

$obj = Join-Path $work 'main.wixobj'
& (Join-Path $wixBin 'candle.exe') -nologo -arch x64 -ext WixUIExtension @defs -out $obj (Join-Path $here 'main.wxs')
if ($LASTEXITCODE -ne 0) { throw 'candle failed' }

$msi = Join-Path $OutDir "AfroBIM-$msiVersion-windows-x64.msi"
& (Join-Path $wixBin 'light.exe') -nologo -ext WixUIExtension -cultures:en-us -out $msi $obj
if ($LASTEXITCODE -ne 0) { throw 'light failed' }

Remove-Item -Recurse -Force $work
Write-Host "wrote $msi"
