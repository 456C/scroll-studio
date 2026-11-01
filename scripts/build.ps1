param(
    [ValidateSet('build', 'run', 'test', 'check', 'fmt', 'clippy')]
    [string]$Action = 'run',
    [switch]$Release,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$savedEnvironment = @{}
$variables = @('PATH', 'CARGO_HOME', 'RUSTUP_HOME', 'CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER', 'CC', 'CXX', 'AR', 'RC', 'RUSTFLAGS')
foreach ($name in $variables) {
    $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}

Push-Location $workspace
try {
    $localCargo = Join-Path $workspace '.tools\cargo\bin\cargo.exe'
    if (Test-Path -LiteralPath $localCargo) {
        $env:CARGO_HOME = Join-Path $workspace '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $workspace '.tools\rustup'
        $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
        $compiler = Get-ChildItem -LiteralPath (Join-Path $workspace '.tools') -Directory -Filter 'llvm-mingw-*-ucrt-x86_64' |
            Sort-Object Name -Descending | Select-Object -First 1
        if ($compiler) {
            $compilerBin = Join-Path $compiler.FullName 'bin'
            $env:PATH = $compilerBin + ';' + $env:PATH
            $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $compilerBin 'x86_64-w64-mingw32-gcc.exe'
            $env:CC = Join-Path $compilerBin 'x86_64-w64-mingw32-clang.exe'
            $env:CXX = Join-Path $compilerBin 'x86_64-w64-mingw32-clang++.exe'
            $env:AR = Join-Path $compilerBin 'llvm-ar.exe'
            $env:RC = Join-Path $compilerBin 'x86_64-w64-mingw32-windres.exe'
            # Use Rust's bundled MinGW runtime archives with the portable LLVM linker.
            $env:RUSTFLAGS = (($env:RUSTFLAGS + ' -C link-self-contained=yes').Trim())
        }
        $cargo = $localCargo
    } else {
        $cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
        if (-not $cargoCommand) {
            throw 'Rust is unavailable. Run powershell -ExecutionPolicy Bypass -File scripts/setup-toolchain.ps1 first, or install Rust from https://rustup.rs.'
        }
        $cargo = $cargoCommand.Source
    }

    $arguments = @($Action)
    if ($Release -and $Action -notin @('fmt')) { $arguments += '--release' }
    if ($CargoArgs) { $arguments += $CargoArgs }
    & $cargo @arguments
    $result = $LASTEXITCODE
} finally {
    foreach ($name in $variables) {
        [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process')
    }
    Pop-Location
}
exit $result
