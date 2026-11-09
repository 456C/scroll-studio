# Manual hosted-runner steps for the private publishing demo.
# No tokens. The service archive and the FFmpeg zip are ordinary files.
param(
    [Parameter(Mandatory = $true)][string]$ServiceCheckout,
    [Parameter(Mandatory = $true)][string]$BrowserScript,
    [Parameter(Mandatory = $true)][string]$WorkRoot
)

$ErrorActionPreference = 'Stop'

Write-Host '=== python ==='
if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    throw 'python is not on PATH.'
}
python -c "import sys; print(sys.version); print(sys.executable)"

Write-Host '=== ffmpeg / ffprobe ==='
if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) { throw 'ffmpeg is not on PATH.' }
if (-not (Get-Command ffprobe -ErrorAction SilentlyContinue)) { throw 'ffprobe is not on PATH.' }
ffmpeg -version
ffprobe -version
$decoders = ffmpeg -hide_banner -decoders 2>&1 | Out-String
$hasPng = $decoders -match '(?m)^\s*\S+\s+png\s'
$hasMjpeg = $decoders -match '(?m)^\s*\S+\s+mjpeg\s'
Write-Host "png decoder: $hasPng"
Write-Host "mjpeg decoder: $hasMjpeg"
if (-not $hasPng -or -not $hasMjpeg) { throw 'png or mjpeg decoder is missing.' }

$archive = Join-Path $WorkRoot 'scroll-studio-service-464766d.zip'
$extracted = Join-Path $WorkRoot 'extracted'
if (Test-Path -LiteralPath $extracted) { Remove-Item -LiteralPath $extracted -Recurse -Force }
New-Item -ItemType Directory -Force -Path $extracted | Out-Null
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
Compress-Archive -Path (Join-Path $ServiceCheckout '*') -DestinationPath $archive -CompressionLevel Optimal
Expand-Archive -LiteralPath $archive -DestinationPath $extracted -Force
$root = $extracted
foreach ($needed in @(
    'prototypes\publishing\listener.py',
    'prototypes\publishing\test_contract.py',
    'prototypes\publishing\test_listener.py',
    'src\app\reader_ui.rs',
    'src\export.rs'
)) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $needed) -PathType Leaf)) {
        throw "Extracted archive is missing $needed"
    }
}
Write-Host "Service working directory: $root"

Set-Location $root
Write-Host '=== selected tests ==='
python -m unittest prototypes.publishing.test_contract prototypes.publishing.test_listener.ListenerTests.test_bind_refuses_non_loopback prototypes.publishing.test_listener.ListenerTests.test_publish_preview_reader_and_pixels prototypes.publishing.test_listener.ListenerTests.test_prose_unicode_preview_and_byte_cap prototypes.publishing.test_listener.ListenerTests.test_replay_revoke_expiry_cancel_and_recovery prototypes.publishing.test_listener.ListenerTests.test_browser_csrf_host_origin_and_withdrawal prototypes.publishing.test_listener.ListenerTests.test_operator_report_hold_remove prototypes.publishing.test_listener.ListenerTests.test_body_cap_closes_before_the_payload prototypes.publishing.test_listener.ListenerTests.test_hold_and_remove_do_not_yield_to_the_owner prototypes.publishing.test_listener.ListenerTests.test_sign_in_csrf_expires_once_and_matches_next prototypes.publishing.test_listener.ListenerTests.test_rejected_connections_close prototypes.publishing.test_listener.ListenerTests.test_busy_admission_does_not_follow_a_trickle
if ($LASTEXITCODE -ne 0) { throw "selected tests exited $LASTEXITCODE" }

Write-Host '=== listener ==='
$proc = Start-Process -FilePath python -ArgumentList '-m','prototypes.publishing.listener' -WorkingDirectory $root -PassThru -WindowStyle Hidden
try {
    $ready = $false
    foreach ($attempt in 1..40) {
        Start-Sleep -Milliseconds 250
        try {
            $client = New-Object System.Net.Sockets.TcpClient('127.0.0.1', 19427)
            $stream = $client.GetStream()
            $bytes = [Text.Encoding]::ASCII.GetBytes("GET /ss-publish/browser/operator HTTP/1.1`r`nHost: 127.0.0.1:19427`r`nConnection: close`r`n`r`n")
            $stream.Write($bytes, 0, $bytes.Length)
            $buffer = New-Object byte[] 400
            $read = $stream.Read($buffer, 0, $buffer.Length)
            $text = [Text.Encoding]::ASCII.GetString($buffer, 0, $read)
            Write-Host ($text.Split("`n")[0])
            if ($text -notmatch '401') { throw 'listener status was not 401' }
            if ($text -notmatch 'referrer-policy: same-origin') { throw 'referrer policy was not same-origin' }
            $ready = $true
            break
        } catch {
            if ($attempt -eq 40) { throw }
        }
    }
    if (-not $ready) { throw 'listener did not answer' }
    python $BrowserScript
    if ($LASTEXITCODE -ne 0) { throw "browser check exited $LASTEXITCODE" }
} finally {
    if ($proc -and -not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    Write-Host 'listener stopped'
}

Write-Host "Service archive: $archive"
"SERVICE_ARCHIVE=$archive" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
