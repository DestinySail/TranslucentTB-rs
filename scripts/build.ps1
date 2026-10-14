param([switch]$Test)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $toolchain = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
        if (Test-Path (Join-Path $toolchain 'cargo.exe')) { $env:PATH = "$toolchain;$env:PATH" }
    }
    # Build the DLL before the EXE embeds it; do not rely on workspace parallel build order.
    & cargo build --release -p clearbar-tap --locked
    if ($LASTEXITCODE) { throw 'Embedded TAP build failed.' }
    $env:CLEARBAR_TAP_DLL = (Resolve-Path 'target\release\clearbar_tap.dll').Path
    if ($Test) {
        & cargo test --workspace --locked
        if ($LASTEXITCODE) { throw 'Rust tests failed.' }
    }
    & cargo build --release -p clearbar --locked
    if ($LASTEXITCODE) { throw 'Release build failed. Install Rust MSVC and Visual Studio C++ Build Tools / Windows SDK.' }
    # XAML Diagnostics pins TAP DLLs in Explorer. Use a new folder for each build.
    $name = 'Clearbar-0.1.2-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
    $package = Join-Path $repo "dist\$name"
    New-Item -ItemType Directory -Path $package | Out-Null
    Copy-Item -LiteralPath 'target\release\clearbar.exe','README.md','LICENSE.md','NOTICE.md','THIRD-PARTY-NOTICES.md','VALIDATION.md' -Destination $package
    Copy-Item -Path 'scripts\controls\*.cmd' -Destination $package
    Compress-Archive -LiteralPath $package -DestinationPath ($package + '.zip')
    Write-Output "Package: $package"
    Write-Output "Archive: $package.zip"
} finally { Pop-Location }
