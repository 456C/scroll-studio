param(
    # Use only after a successful release build of the current source.
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$readme = Join-Path $workspace 'README.md'
$fontLicenses = @('Karla', 'ComicNeue', 'PatrickHand', 'DeliusUnicase', 'Lato', 'BalsamiqSans') | ForEach-Object {
    Join-Path $workspace "assets\$_-OFL.txt"
}
$fontLicenses += Join-Path $workspace 'assets\KomikaHand-LICENSE.txt'
$letteringResearch = Join-Path $workspace 'docs\font-research.md'
$iconResearch = Join-Path $workspace 'docs\icon-generation.md'
$uiResearch = Join-Path $workspace 'docs\ui-review.md'
$workflowResearch = Join-Path $workspace 'docs\workflow-research.md'
foreach ($requiredFile in (@($readme, $letteringResearch, $iconResearch, $uiResearch, $workflowResearch) + $fontLicenses)) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        throw "A required package file is missing: $requiredFile"
    }
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'build.ps1') -Action build -Release -CargoArgs @('--locked')
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed; no package was created.' }
}

$targetDirectory = Join-Path $workspace 'target'
if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        $targetDirectory = $env:CARGO_TARGET_DIR
    } else {
        $targetDirectory = Join-Path $workspace $env:CARGO_TARGET_DIR
    }
}
if ($env:CARGO_BUILD_TARGET) {
    $targetDirectory = Join-Path $targetDirectory $env:CARGO_BUILD_TARGET
}
$builtExecutable = Join-Path $targetDirectory 'release\scroll-studio.exe'
if (-not (Test-Path -LiteralPath $builtExecutable -PathType Leaf)) {
    throw "Release executable not found: $builtExecutable"
}

# Generate and validate examples from the current source. Never reuse an old
# archive from examples/ or silently omit a sample on a clean checkout.
$samplesDirectory = Join-Path $workspace 'target\sample-package'
& (Join-Path $PSScriptRoot 'samples.ps1') -OutputDirectory $samplesDirectory

$distribution = Join-Path $workspace 'dist'
$manifest = Get-Content -LiteralPath (Join-Path $workspace 'Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') { throw 'Package version is missing.' }
$packageVersion = $Matches[1]
$portableDirectory = Join-Path $distribution "Scroll Studio $packageVersion"
New-Item -ItemType Directory -Force -Path $portableDirectory | Out-Null

$packageSources = @(
    @{ Source = $builtExecutable; Name = 'Scroll Studio.exe' },
    @{ Source = $readme; Name = 'README.md' },
    @{ Source = $letteringResearch; Name = 'font-research.md' },
    @{ Source = $iconResearch; Name = 'icon-generation.md' },
    @{ Source = $uiResearch; Name = 'ui-review.md' },
    @{ Source = $workflowResearch; Name = 'workflow-research.md' },
    @{ Source = (Join-Path $workspace 'docs\painting-performance.md'); Name = 'painting-performance.md' }
)
foreach ($fontLicense in $fontLicenses) {
    $packageSources += @{ Source = $fontLicense; Name = (Split-Path -Leaf $fontLicense) }
}
foreach ($sampleName in @('Lettering playground.scro', 'Bubble presets playground.scro', 'Character studio playground.scro', 'Layout playground.scro', 'After the rain.scro')) {
    $sampleStory = Join-Path $samplesDirectory $sampleName
    if (-not (Test-Path -LiteralPath $sampleStory -PathType Leaf)) { throw "Required sample is missing: $sampleName" }
    $packageSources += @{ Source = $sampleStory; Name = $sampleName }
}
$projectLicense = Join-Path $workspace 'LICENSE'
if (Test-Path -LiteralPath $projectLicense -PathType Leaf) {
    $packageSources += @{ Source = $projectLicense; Name = 'LICENSE' }
}

$packagedFiles = foreach ($item in $packageSources) {
    $destination = Join-Path $portableDirectory $item.Name
    Copy-Item -LiteralPath $item.Source -Destination $destination -Force
    if ($item.Name -eq 'README.md') {
        $portableReadme = [IO.File]::ReadAllText($destination)
        $portableReadme = $portableReadme.Replace('](docs/font-research.md)', '](font-research.md)')
        $portableReadme = $portableReadme.Replace('](docs/icon-generation.md)', '](icon-generation.md)')
        $portableReadme = $portableReadme.Replace('](docs/ui-review.md)', '](ui-review.md)')
        $portableReadme = $portableReadme.Replace('](docs/workflow-research.md)', '](workflow-research.md)')
        $portableReadme = $portableReadme.Replace('](docs/painting-performance.md)', '](painting-performance.md)')
        [IO.File]::WriteAllText($destination, $portableReadme, [Text.UTF8Encoding]::new($false))
    }
    $destination
}

# Package only these explicit files, never stories or other files a user may
# have saved beside a previously unpacked executable.
$archive = Join-Path $distribution "Scroll-Studio-$packageVersion-windows-x64.zip"
Compress-Archive -LiteralPath $packagedFiles -DestinationPath $archive -CompressionLevel Optimal -Force
$archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$archive.sha256", "$archiveHash  $([IO.Path]::GetFileName($archive))`n", [Text.UTF8Encoding]::new($false))
Write-Host "Portable app: $portableDirectory"
Write-Host "Archive:      $archive"
Get-Item -LiteralPath (Join-Path $portableDirectory 'Scroll Studio.exe'), $archive |
    Select-Object FullName, Length
