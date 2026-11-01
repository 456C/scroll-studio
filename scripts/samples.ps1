param([string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $workspace 'target\sample-package'
} elseif (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = Join-Path $workspace $OutputDirectory
}

foreach ($example in @('lettering_gallery', 'bubble_presets', 'character_studio', 'layout_playground', 'demo_story')) {
    & (Join-Path $PSScriptRoot 'build.ps1') -Action run -Release -CargoArgs @(
        '--locked', '--quiet', '--example', $example, '--', $OutputDirectory
    )
    if ($LASTEXITCODE -ne 0) { throw "Sample generation failed: $example" }
}
