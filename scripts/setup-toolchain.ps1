# Project-local Windows build tools. Nothing is added to the system PATH.
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$toolDirectory = Join-Path $workspace '.tools'
New-Item -ItemType Directory -Force -Path $toolDirectory | Out-Null

$rustupInstaller = Join-Path $toolDirectory 'rustup-init.exe'
if (-not (Test-Path -LiteralPath $rustupInstaller)) {
    Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $rustupInstaller
}

$savedCargo = $env:CARGO_HOME
$savedRustup = $env:RUSTUP_HOME
try {
    $env:CARGO_HOME = Join-Path $toolDirectory 'cargo'
    $env:RUSTUP_HOME = Join-Path $toolDirectory 'rustup'
    & $rustupInstaller -y --no-modify-path --profile minimal --default-host x86_64-pc-windows-gnu --default-toolchain stable
    if ($LASTEXITCODE -ne 0) { throw 'Rust installation failed.' }
    & (Join-Path $env:CARGO_HOME 'bin\rustup.exe') component add rustfmt clippy
    if ($LASTEXITCODE -ne 0) { throw 'Rust development component installation failed.' }
} finally {
    $env:CARGO_HOME = $savedCargo
    $env:RUSTUP_HOME = $savedRustup
}

$llvmVersion = '20261020'
$llvmFolder = "llvm-mingw-$llvmVersion-ucrt-x86_64"
if (-not (Test-Path -LiteralPath (Join-Path $toolDirectory "$llvmFolder\bin\clang.exe"))) {
    $archive = Join-Path $toolDirectory 'llvm-mingw.zip'
    Invoke-WebRequest -Uri "https://github.com/mstorsjo/llvm-mingw/releases/download/$llvmVersion/$llvmFolder.zip" -OutFile $archive
    Expand-Archive -LiteralPath $archive -DestinationPath $toolDirectory -Force
}
Write-Host 'Build tools are ready. Run: powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Action run -Release'
