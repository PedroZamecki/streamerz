# Builds Streamerz.exe on Windows the way the Windows CI job does (release, static MSVC runtime).
#
#   powershell -ExecutionPolicy Bypass -File dev\windows-build.ps1 [-Profile release|fast] [-Test] [-Scenarios]
#
# Toolchain, in this order:
#   1. Visual Studio (Build Tools) with "Desktop development with C++": the same as CI.
#   2. cargo-xwin + LLVM (clang-cl, lld-link, llvm-rc) + CMake + Ninja: no Visual Studio needed;
#      Microsoft's CRT and SDK are downloaded once to %LOCALAPPDATA%\cargo-xwin. Same as dev/windows.sh.
param(
    [ValidateSet("release", "fast")] [string]$Profile = "release",
    [switch]$Test,
    [switch]$Scenarios
)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

# Tools installed by winget/MSI land here; a shell opened before they were installed lacks them.
foreach ($p in @("$env:ProgramFiles\CMake\bin", "$env:ProgramFiles\LLVM\bin",
                 "$env:LOCALAPPDATA\Microsoft\WinGet\Links", "$env:USERPROFILE\.local\bin")) {
    if ((Test-Path $p) -and -not ($env:PATH.Split(';') -contains $p)) { $env:PATH = "$p;$env:PATH" }
}

# Same as CI: FLTK with the static MSVC runtime (matches +crt-static in .cargo/config.toml).
cargo fetch --locked
if ($LASTEXITCODE) { throw "cargo fetch failed" }
$registry = if ($env:CARGO_HOME) { "$env:CARGO_HOME\registry\src" } else { "$env:USERPROFILE\.cargo\registry\src" }
foreach ($f in Get-ChildItem "$registry\*\fltk-sys-*\cfltk\fltk\CMake\options.cmake") {
    $text = Get-Content $f.FullName -Raw
    $patched = $text -replace '(option\(FLTK_MSVC_RUNTIME_DLL [^\r\n]*)ON\)', '${1}OFF)'
    if ($patched -ne $text) { Set-Content -Path $f.FullName -Value $patched -NoNewline }
    if ((Get-Content $f.FullName -Raw) -notmatch 'FLTK_MSVC_RUNTIME_DLL [^\r\n]*OFF\)') { throw "could not patch $f" }
}

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = if (Test-Path $vswhere) {
    & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
$target = "x86_64-pc-windows-msvc"
if ($vs) {
    Write-Host "toolchain: MSVC ($vs)"
    $cargo = @("cargo")
    $out = "target"
} elseif (Get-Command cargo-xwin -ErrorAction SilentlyContinue) {
    foreach ($tool in "clang-cl", "lld-link", "llvm-rc", "cmake", "ninja") {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "cargo-xwin needs $tool on PATH" }
    }
    Write-Host "toolchain: cargo-xwin + LLVM (no Visual Studio C++ tools found)"
    $env:XWIN_ACCEPT_LICENSE = "1"
    # FLTK's CMake build with clang-cl: Ninja, not the Visual Studio generator.
    $env:CMAKE_GENERATOR = "Ninja"
    $cargo = @("cargo", "xwin")
    $out = "target\xwin"
} else {
    throw @"
No C/C++ toolchain. Either:
  winget install Microsoft.VisualStudio.2022.BuildTools --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
or install cargo-xwin, LLVM, CMake and Ninja (see the header of this script).
"@
}

$env:STREAMERZ_VERSION = if ($env:STREAMERZ_VERSION) { $env:STREAMERZ_VERSION } else { "dev" }
$run = { param([string[]]$a) & $cargo[0] @($cargo[1..9] + $a | Where-Object { $_ }); if ($LASTEXITCODE) { throw "cargo $($a -join ' ') failed" } }

& $run @("build", "--profile", $Profile, "--locked", "--bin", "Streamerz", "--target", $target, "--target-dir", $out)
$dir = if ($Profile -eq "release") { "release" } else { $Profile }
$exe = Join-Path $out "$target\$dir\Streamerz.exe"
Write-Host "built $exe"

if ($Test) { & $run @("test", "--profile", $Profile, "--locked", "--bin", "Streamerz", "--target", $target, "--target-dir", $out) }
if ($Scenarios) {
    & $exe --scenario tests/scenarios
    if ($LASTEXITCODE) { throw "scenarios failed" }
}
