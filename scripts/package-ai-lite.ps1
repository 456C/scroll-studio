param(
    # Use only after a successful release build of the current AI Lite source.
    [switch]$SkipBuild
)

# Builds the AI Lite addon and writes dist\ai-lite-<version>.zip (the package
# Settings > Addons > Install from file accepts) and its .sha256 beside the app's.

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$addon = Join-Path $workspace 'addons\ai-lite'
$manifest = Get-Content -LiteralPath (Join-Path $addon 'addon.json') -Raw | ConvertFrom-Json
if ($manifest.id -ne 'ai-lite') { throw "Unexpected addon id $($manifest.id)" }
$cargo = Get-Content -LiteralPath (Join-Path $addon 'Cargo.toml') -Raw
if ($cargo -notmatch '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') { throw 'AI Lite version is missing.' }
$version = $Matches[1]
if ($manifest.version -ne $version) {
    throw "addon.json says $($manifest.version) but Cargo.toml says $version"
}

if (-not $SkipBuild) {
    Push-Location $addon
    try {
        # One program that runs without the Visual C++ redistributable, like the app.
        $previous = $env:RUSTFLAGS
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
        cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'AI Lite release build failed.' }
    } finally {
        $env:RUSTFLAGS = $previous
        Pop-Location
    }
}

$program = Join-Path $addon 'target\release\ai-lite.exe'
if (-not (Test-Path -LiteralPath $program -PathType Leaf)) { throw "Missing $program" }
$stage = Join-Path ([IO.Path]::GetTempPath()) "ai-lite-package-$version"
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -LiteralPath (Join-Path $addon 'addon.json') -Destination $stage
Copy-Item -LiteralPath $program -Destination (Join-Path $stage $manifest.entry)
Copy-Item -LiteralPath (Join-Path $addon 'README.md') -Destination $stage

$dist = Join-Path $workspace 'dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$archive = Join-Path $dist "ai-lite-$version.zip"
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive
Remove-Item -LiteralPath $stage -Recurse -Force

# The app installs a .zip with addon.json and the program at its top level.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try { $entries = @($zip.Entries | ForEach-Object { $_.FullName }) } finally { $zip.Dispose() }
foreach ($required in @('addon.json', $manifest.entry)) {
    if ($entries -notcontains $required) { throw "The package is missing $required" }
}

$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$archive.sha256", "$hash  $([IO.Path]::GetFileName($archive))`n", [Text.UTF8Encoding]::new($false))
Write-Host "Packaged $archive ($([math]::Round((Get-Item -LiteralPath $archive).Length / 1MB, 1)) MB), sha256 $hash"
