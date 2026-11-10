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

$archive = Join-Path $WorkRoot 'scroll-studio-service-66582c2.zip'
$extracted = Join-Path $WorkRoot 'extracted'
if (Test-Path -LiteralPath $extracted) { Remove-Item -LiteralPath $extracted -Recurse -Force }
New-Item -ItemType Directory -Force -Path $extracted | Out-Null
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
Compress-Archive -Path (Join-Path $ServiceCheckout '*') -DestinationPath $archive -CompressionLevel Optimal
Expand-Archive -LiteralPath $archive -DestinationPath $extracted -Force
$root = $extracted
foreach ($needed in @(
    'prototypes\publishing\listener.py',
    'prototypes\publishing\demo_site.py',
    'prototypes\publishing\demo_assets\panel-fold.jpg',
    'prototypes\publishing\demo_assets\panel-hull.jpg',
    'prototypes\publishing\demo_assets\panel-sill.jpg',
    'prototypes\publishing\demo_assets\panel-puddle.jpg',
    'prototypes\publishing\test_contract.py',
    'prototypes\publishing\test_listener.py',
    'src\app\reader_ui.rs',
    'src\export.rs'
)) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $needed) -PathType Leaf)) {
        throw "Extracted archive is missing $needed"
    }
}
$assetHashes = @{
    'prototypes\publishing\demo_assets\panel-fold.jpg' = '166b246d70a3b6d55601db68d6c0da042f6aa651921b03e13b7e2fb23fe7c7bd'
    'prototypes\publishing\demo_assets\panel-hull.jpg' = 'b9b8e277b8191a10d47050e8b66a93b32ce0daf1a15bcae7003590314ddaba70'
    'prototypes\publishing\demo_assets\panel-puddle.jpg' = 'e9ca13b83505f0d3b26cabc45db3ac369dd192b3ab87a9b954705296a298bcb5'
    'prototypes\publishing\demo_assets\panel-sill.jpg' = '5ab59c0b261e86d81dd628dde809af15ababbada64d1da41980103b1b1d260a3'
}
foreach ($relative in $assetHashes.Keys) {
    $hashed = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $root $relative)).Hash.ToLowerInvariant()
    if ($hashed -ne $assetHashes[$relative]) { throw "demo asset changed: $relative $hashed" }
}
Write-Host 'demo_site.py and four panel jpgs intact'
Write-Host "Service working directory: $root"
$archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
Write-Host "service archive sha256: $archiveHash"

Set-Location $root
Write-Host '=== selected tests ==='
python -m unittest prototypes.publishing.test_contract prototypes.publishing.test_listener.ListenerTests.test_bind_refuses_non_loopback prototypes.publishing.test_listener.ListenerTests.test_publish_preview_reader_and_pixels prototypes.publishing.test_listener.ListenerTests.test_prose_unicode_preview_and_byte_cap prototypes.publishing.test_listener.ListenerTests.test_replay_revoke_expiry_cancel_and_recovery prototypes.publishing.test_listener.ListenerTests.test_browser_csrf_host_origin_and_withdrawal prototypes.publishing.test_listener.ListenerTests.test_operator_report_hold_remove prototypes.publishing.test_listener.ListenerTests.test_body_cap_closes_before_the_payload prototypes.publishing.test_listener.ListenerTests.test_hold_and_remove_do_not_yield_to_the_owner prototypes.publishing.test_listener.ListenerTests.test_sign_in_csrf_expires_once_and_matches_next prototypes.publishing.test_listener.ListenerTests.test_rejected_connections_close prototypes.publishing.test_listener.ListenerTests.test_busy_admission_does_not_follow_a_trickle prototypes.publishing.test_listener.ListenerTests.test_demo_catalogue_is_separate_from_creator_state
if ($LASTEXITCODE -ne 0) { throw "selected tests exited $LASTEXITCODE" }

Write-Host '=== listener ==='
$proc = Start-Process -FilePath python -ArgumentList '-m','prototypes.publishing.listener' -WorkingDirectory $root -PassThru -WindowStyle Hidden
Write-Host "process: listener pid $($proc.Id) working directory $root"
function Read-Loopback([string]$path, [int]$limit) {
    $client = New-Object System.Net.Sockets.TcpClient('127.0.0.1', 19427)
    try {
        $stream = $client.GetStream()
        $payload = [Text.Encoding]::ASCII.GetBytes("GET $path HTTP/1.1`r`nHost: 127.0.0.1:19427`r`nConnection: close`r`n`r`n")
        $stream.Write($payload, 0, $payload.Length)
        $buffer = New-Object byte[] $limit
        $read = 0
        while ($read -lt $buffer.Length) {
            $got = $stream.Read($buffer, $read, $buffer.Length - $read)
            if ($got -le 0) { break }
            $read += $got
        }
        return [Text.Encoding]::ASCII.GetString($buffer, 0, $read)
    } finally {
        $client.Close()
    }
}
try {
    $ready = $false
    foreach ($attempt in 1..40) {
        Start-Sleep -Milliseconds 250
        try {
            $landing = Read-Loopback '/' 12000
            $status = ($landing -split "`n")[0].Trim()
            if ($status -notmatch '^HTTP/1\.[01] 200') { throw "landing status $status" }
            if ($landing -notmatch 'Enter the synthetic demo') { throw 'landing had no enter link' }
            if ($landing -notmatch 'Read the sample comic and prose') { throw 'landing did not show the sample heading' }
            if ($landing -match 'panel-fold.jpg') { throw 'landing already addressed a story picture' }
            if ($landing -match 'A hull does not need a coastline') { throw 'landing already showed the story' }
            Write-Host "http: GET / $status"
            foreach ($panel in @('panel-fold.jpg', 'panel-hull.jpg', 'panel-sill.jpg', 'panel-puddle.jpg')) {
                $assetPath = "/ss-publish/demo/assets/$panel"
                $asset = Read-Loopback $assetPath 2048
                $assetStatus = ($asset -split "`n")[0].Trim()
                if ($assetStatus -notmatch '^HTTP/1\.[01] 200') { throw "asset status $panel $assetStatus" }
                if ($asset -notmatch '(?i)content-type: image/jpeg') { throw "asset type $panel" }
                Write-Host "http: GET $assetPath $assetStatus image/jpeg"
            }
            $series = Read-Loopback '/ss-publish/browser/series' 800
            $seriesStatus = ($series -split "`n")[0].Trim()
            if ($seriesStatus -notmatch '^HTTP/1\.[01] 401') { throw "series status $seriesStatus" }
            Write-Host "http: GET /ss-publish/browser/series $seriesStatus"
            $operator = Read-Loopback '/ss-publish/browser/operator' 800
            $operatorStatus = ($operator -split "`n")[0].Trim()
            if ($operatorStatus -notmatch '^HTTP/1\.[01] 401') { throw "operator status $operatorStatus" }
            if ($operator -notmatch 'referrer-policy: same-origin') { throw 'referrer policy was not same-origin' }
            Write-Host "http: GET /ss-publish/browser/operator $operatorStatus"
            $ready = $true
            break
        } catch {
            if ($attempt -eq 40) { throw $_ }
        }
    }
    if (-not $ready) { throw 'listener did not answer' }
    $shots = Join-Path $WorkRoot 'shots'
    New-Item -ItemType Directory -Force -Path $shots | Out-Null
    $env:DEMO_SHOT_DIR = $shots
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
