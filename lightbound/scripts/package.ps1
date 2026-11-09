[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$properties = @{}
foreach ($line in Get-Content -LiteralPath (Join-Path $projectRoot 'gradle.properties')) {
    if ($line -match '^([^#=]+)=(.*)$') { $properties[$Matches[1].Trim()] = $Matches[2].Trim() }
}
$version = $properties['version']
if ($version -notmatch '^[0-9A-Za-z][0-9A-Za-z.+_-]*$') { throw 'Invalid project version.' }
$jarName = "uncommon-nights-$version.jar"
$sourceJar = Join-Path $projectRoot "build\libs\$jarName"
if (-not (Test-Path -LiteralPath $sourceJar -PathType Leaf)) {
    throw "Build the mod first: .\gradlew.bat build. Missing $sourceJar"
}

$jarInspection = [IO.Compression.ZipFile]::OpenRead($sourceJar)
try {
    $metadataEntry = $jarInspection.GetEntry('fabric.mod.json')
    if ($null -eq $metadataEntry) { throw 'Built jar is missing fabric.mod.json.' }
    $reader = [IO.StreamReader]::new($metadataEntry.Open())
    try { $metadata = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    if ($metadata.id -ne 'lightbound' -or $metadata.name -ne 'Uncommon Nights' -or $metadata.version -ne $version) {
        throw 'Built jar metadata does not match the project. Rebuild before packaging.'
    }
    foreach ($asset in @(
        $metadata.icon,
        'assets/minecraft/shaders/core/lightmap.fsh',
        'assets/minecraft/shaders/core/terrain.vsh',
        'assets/lightbound/shaders/post/gateway_void.fsh',
        'assets/lightbound/post_effect/gateway_void.json',
        'assets/lightbound/post_effect/occluded.json',
        'assets/lightbound/shaders/post/occluded.fsh',
        'assets/minecraft/shaders/include/sample_lightmap.glsl'
    )) {
        if ($null -eq $jarInspection.GetEntry($asset)) { throw "Built jar is missing $asset." }
    }
    foreach ($mixin in $metadata.mixins) {
        $configName = if ($mixin -is [string]) { $mixin } else { $mixin.config }
        $configEntry = $jarInspection.GetEntry($configName)
        if ($null -eq $configEntry) { throw "Built jar is missing mixin config $configName." }
        $reader = [IO.StreamReader]::new($configEntry.Open())
        try { $config = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
        foreach ($group in @('mixins', 'client', 'server')) {
            if ($config.PSObject.Properties.Name -contains $group) {
                foreach ($class in $config.$group) {
                    $classPath = ($config.package + '.' + $class).Replace('.', '/') + '.class'
                    if ($null -eq $jarInspection.GetEntry($classPath)) { throw "Built jar is missing $classPath." }
                }
            }
        }
    }
} finally { $jarInspection.Dispose() }

$lockedFiles = @(Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'modpack-dependencies.json') | ConvertFrom-Json)
foreach ($file in $lockedFiles) {
    if ($file.path -notmatch '^mods/[A-Za-z0-9._+%-]+\.jar$' -or $file.path.Contains('..')) {
        throw "Unsafe modpack path: $($file.path)"
    }
    if ($file.hashes.sha1 -notmatch '^[a-f0-9]{40}$' -or $file.hashes.sha512 -notmatch '^[a-f0-9]{128}$') {
        throw "Invalid hashes for $($file.path)"
    }
    if ($file.fileSize -le 0) { throw "Invalid file size for $($file.path)" }
    foreach ($download in $file.downloads) {
        $uri = [uri]$download
        if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'cdn.modrinth.com') {
            throw "Unexpected download host: $download"
        }
    }
}
if ($properties['minecraft_version'] -ne '26.3' -or $properties['fabric_api_version'] -ne '0.165.0+26.3') {
    throw 'Dependency lock targets Minecraft 26.3 / Fabric API 0.165.0+26.3; update it before packaging another target.'
}

$files = @($lockedFiles | ForEach-Object {
    [ordered]@{
        path = $_.path
        hashes = $_.hashes
        env = $_.env
        downloads = @($_.downloads)
        fileSize = $_.fileSize
    }
})
$manifest = [ordered]@{
    formatVersion = 1
    game = 'minecraft'
    versionId = $version
    name = $metadata.name
    summary = $metadata.description
    files = $files
    dependencies = [ordered]@{
        minecraft = $properties['minecraft_version']
        'fabric-loader' = $properties['loader_version']
    }
}

$dist = Join-Path $projectRoot 'dist'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
$destinationJar = Join-Path $dist $jarName
Copy-Item -LiteralPath $sourceJar -Destination $destinationJar -Force
$packPath = Join-Path $dist "uncommon-nights-$version.mrpack"
$stream = [IO.File]::Open($packPath, [IO.FileMode]::Create, [IO.FileAccess]::Write)
try {
    $archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
    try {
        $index = $archive.CreateEntry('modrinth.index.json')
        $writer = [IO.StreamWriter]::new($index.Open(), [Text.UTF8Encoding]::new($false))
        try { $writer.Write(($manifest | ConvertTo-Json -Depth 12)) } finally { $writer.Dispose() }
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $destinationJar, "overrides/mods/$jarName") | Out-Null
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, (Join-Path $projectRoot 'README.md'), 'overrides/UNCOMMON-NIGHTS-README.md') | Out-Null
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, (Join-Path $projectRoot 'art/uncommon-nights-cover.png'), 'overrides/art/uncommon-nights-cover.png') | Out-Null
    } finally { $archive.Dispose() }
} finally { $stream.Dispose() }

# Reopen the result so malformed archives never report success.
$inspection = [IO.Compression.ZipFile]::OpenRead($packPath)
try {
    if ($null -eq $inspection.GetEntry('modrinth.index.json') -or
        $null -eq $inspection.GetEntry("overrides/mods/$jarName")) {
        throw 'Pack validation failed: required entry missing.'
    }
} finally { $inspection.Dispose() }

$checksums = foreach ($filePath in @($destinationJar, $packPath)) {
    $hash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($filePath))"
}
[IO.File]::WriteAllLines((Join-Path $dist 'SHA256SUMS.txt'), [string[]]$checksums, [Text.UTF8Encoding]::new($false))
Get-Item -LiteralPath $destinationJar, $packPath | Select-Object Name, Length, FullName
