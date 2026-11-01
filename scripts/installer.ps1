param(
    # Package the already verified release executable; otherwise build it first.
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$nsisVersion = '3.13'
# Published checksum: official NSIS SourceForge download page for nsis-3.13.zip.
$nsisSha256 = 'ba63dffc4410ee89193e1cb5a41989991bd77c61068da17e3156d136b7b0b3d8'
$toolsDirectory = Join-Path $workspace '.tools'
$compiler = Join-Path $toolsDirectory "nsis-$nsisVersion\makensis.exe"
$archive = Join-Path $toolsDirectory "nsis-$nsisVersion.zip"
New-Item -ItemType Directory -Force -Path $toolsDirectory | Out-Null

if (-not (Test-Path -LiteralPath $archive) -or
    (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $nsisSha256) {
    $downloaded = $false
    foreach ($mirror in @('netix', 'psychz', 'downloads')) {
        $hostName = if ($mirror -eq 'downloads') { 'downloads.sourceforge.net' } else { "$mirror.dl.sourceforge.net" }
        try {
            Invoke-WebRequest -Uri "https://$hostName/project/nsis/NSIS%203/$nsisVersion/nsis-$nsisVersion.zip" -OutFile $archive -TimeoutSec 60
            if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -eq $nsisSha256) {
                $downloaded = $true
                break
            }
        } catch {
            Write-Verbose "NSIS mirror $mirror failed: $_"
        }
    }
    if (-not $downloaded) { throw 'Could not download the checksum-verified NSIS compiler.' }
}
# Extract only the pinned, verified compiler archive; no system install or PATH edits.
Expand-Archive -LiteralPath $archive -DestinationPath $toolsDirectory -Force
if (-not (Test-Path -LiteralPath $compiler)) { throw 'NSIS compiler is missing.' }

& (Join-Path $PSScriptRoot 'package.ps1') -SkipBuild:$SkipBuild
$manifest = Get-Content -LiteralPath (Join-Path $workspace 'Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') { throw 'Package version is missing.' }
$packageVersion = $Matches[1]
$packageDirectory = Join-Path $workspace "dist\Scroll Studio $packageVersion"
$outputFile = Join-Path $workspace "dist\Scroll-Studio-$packageVersion-Setup-x64.exe"
$packageFiles = @('Scroll Studio.exe','README.md','LICENSE','font-research.md','icon-generation.md','ui-review.md','workflow-research.md',
    'Karla-OFL.txt','ComicNeue-OFL.txt','PatrickHand-OFL.txt','DeliusUnicase-OFL.txt','Lato-OFL.txt','BalsamiqSans-OFL.txt','KomikaHand-LICENSE.txt',
    'Lettering playground.scro','Bubble presets playground.scro','Character studio playground.scro','Layout playground.scro','After the rain.scro')
$totalBytes = 0
foreach ($file in $packageFiles) {
    $totalBytes += (Get-Item -LiteralPath (Join-Path $packageDirectory $file) -ErrorAction Stop).Length
}
$installedKiB = [int][Math]::Ceiling($totalBytes / 1024) + 150
$arguments = @('/NOCONFIG','/V3','/WX',"/DAPP_VERSION=$packageVersion", "/DPACKAGE_DIR=$packageDirectory",
    "/DOUTPUT_FILE=$outputFile", "/DAPP_ICON=$(Join-Path $workspace 'assets\scroll-studio.ico')", "/DINSTALL_KIB=$installedKiB",
    (Join-Path $workspace 'installer\scroll-studio.nsi'))
& $compiler @arguments
if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed with exit code $LASTEXITCODE." }
$hash = (Get-FileHash -LiteralPath $outputFile -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$outputFile.sha256", "$hash  $([IO.Path]::GetFileName($outputFile))`n", [Text.UTF8Encoding]::new($false))
Get-Item -LiteralPath $outputFile | Select-Object FullName, Length
Write-Host "SHA256: $hash"
