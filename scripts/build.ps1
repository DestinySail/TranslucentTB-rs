param([switch]$Test)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $toolchain = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
        if (Test-Path (Join-Path $toolchain 'cargo.exe')) { $env:PATH = "$toolchain;$env:PATH" }
    }
    # Ship a self-contained EXE: a static CRT removes the vcruntime140.dll dependency,
    # so a machine without the VC++ Redistributable can still run the download.
    # RUSTFLAGS wins over .cargo/config.toml, so inject it only when no local config
    # exists (local checkouts keep extra -L search paths in that file).
    if (-not $env:RUSTFLAGS -and -not (Test-Path (Join-Path $repo '.cargo\config.toml'))) {
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
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
    $version = (Select-String -Path (Join-Path $repo 'Cargo.toml') -Pattern '^version = "(.+)"' |
        Select-Object -First 1).Matches[0].Groups[1].Value
    $name = "Clearbar-$version-" + (Get-Date -Format 'yyyyMMdd-HHmmss')
    $package = Join-Path $repo "dist\$name"
    New-Item -ItemType Directory -Path $package | Out-Null
    Copy-Item -LiteralPath 'target\release\clearbar.exe','README.md','LICENSE.md','NOTICE.md','THIRD-PARTY-NOTICES.md','VALIDATION.md' -Destination $package
    Copy-Item -Path 'scripts\controls\*.cmd' -Destination $package
    Compress-Archive -LiteralPath $package -DestinationPath ($package + '.zip')
    Write-Output "Package: $package"
    Write-Output "Archive: $package.zip"
} finally { Pop-Location }
