param(
    [string]$Source
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$assetDirectory = Join-Path $workspace 'assets'
$concept = Join-Path $assetDirectory 'scroll-studio-icon-concept-v2.png'

# Keep the selected concept unchanged. Derivatives only resize and encode it.
if ($Source) {
    $sourceFile = (Resolve-Path -LiteralPath $Source).Path
    if (Test-Path -LiteralPath $concept) {
        if ((Get-FileHash -LiteralPath $sourceFile).Hash -ne (Get-FileHash -LiteralPath $concept).Hash) {
            throw 'The saved concept differs from Source; preserve or explicitly replace it before regenerating icons.'
        }
    } else {
        Copy-Item -LiteralPath $sourceFile -Destination $concept
    }
}
if (-not (Test-Path -LiteralPath $concept -PathType Leaf)) {
    throw 'Supply -Source with the approved PNG concept the first time.'
}

Add-Type -AssemblyName System.Drawing
$original = [System.Drawing.Image]::FromFile($concept)
try {
    if ($original.Width -ne $original.Height) { throw 'The approved icon concept must be square.' }
    $sizes = @(16, 24, 32, 48, 64, 128, 256)
    $encodedImages = [System.Collections.Generic.List[byte[]]]::new()
    foreach ($size in $sizes) {
        $bitmap = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $attributes = [System.Drawing.Imaging.ImageAttributes]::new()
        $stream = [System.IO.MemoryStream]::new()
        try {
            $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $graphics.Clear([System.Drawing.Color]::Transparent)
            $attributes.SetWrapMode([System.Drawing.Drawing2D.WrapMode]::TileFlipXY)
            $graphics.DrawImage($original, [System.Drawing.Rectangle]::new(0, 0, $size, $size), 0, 0,
                $original.Width, $original.Height, [System.Drawing.GraphicsUnit]::Pixel, $attributes)
            $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
            $pngBytes = $stream.ToArray()
            $encodedImages.Add($pngBytes)
            if ($size -eq 256) {
                [System.IO.File]::WriteAllBytes((Join-Path $assetDirectory 'scroll-studio-icon.png'), $pngBytes)
            }
        } finally {
            $stream.Dispose()
            $attributes.Dispose()
            $graphics.Dispose()
            $bitmap.Dispose()
        }
    }

    # Windows accepts PNG-compressed ICO images, preserving full alpha at each size.
    $iconStream = [System.IO.MemoryStream]::new()
    $writer = [System.IO.BinaryWriter]::new($iconStream)
    try {
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]$sizes.Count)
        $offset = 6 + 16 * $sizes.Count
        for ($index = 0; $index -lt $sizes.Count; $index++) {
            $dimension = if ($sizes[$index] -eq 256) { 0 } else { $sizes[$index] }
            $writer.Write([byte]$dimension)
            $writer.Write([byte]$dimension)
            $writer.Write([byte]0)
            $writer.Write([byte]0)
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]$encodedImages[$index].Length)
            $writer.Write([uint32]$offset)
            $offset += $encodedImages[$index].Length
        }
        foreach ($png in $encodedImages) { $writer.Write($png) }
        $writer.Flush()
        [System.IO.File]::WriteAllBytes((Join-Path $assetDirectory 'scroll-studio.ico'), $iconStream.ToArray())
    } finally {
        $writer.Dispose()
        $iconStream.Dispose()
    }
} finally {
    $original.Dispose()
}

Get-Item -LiteralPath $concept, (Join-Path $assetDirectory 'scroll-studio-icon.png'), (Join-Path $assetDirectory 'scroll-studio.ico') |
    Select-Object Name, Length
