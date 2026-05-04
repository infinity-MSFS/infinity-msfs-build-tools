# Publish all workspace crates + the root binary to crates.io in
# dependency order. Run from anywhere; resolves to the repo root via
# this script's own location.
#
# Usage:
#   ./scripts/publish.ps1            # real publish
#   ./scripts/publish.ps1 -DryRun    # `cargo publish --dry-run` for each
#   ./scripts/publish.ps1 -NoVerify  # skip the post-package build check

param(
    [switch]$DryRun,
    [switch]$NoVerify
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location $repoRoot

# Dependency-ordered: each crate must publish before any crate that
# depends on it. The root binary `infinity-msfs` goes last because it
# pulls in every other workspace crate.
$crates = @(
    "infinity-build-core",
    "infinity-build-sdk",
    "infinity-build-watch",
    "infinity-build-create",
    "infinity-build-js",
    "infinity-build-rust",
    "infinity-build-package",
    "infinity-msfs"
)

function Invoke-Publish {
    param([string]$Crate)

    Write-Host ""
    Write-Host "──── publishing $Crate ────" -ForegroundColor Cyan

    $publishArgs = @("publish", "-p", $Crate)
    if ($DryRun)   { $publishArgs += "--dry-run" }
    if ($NoVerify) { $publishArgs += "--no-verify" }

    & cargo @publishArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo publish failed for $Crate (exit $LASTEXITCODE)"
    }
}

foreach ($crate in $crates) {
    Invoke-Publish -Crate $crate
}

Write-Host ""
Write-Host "✓ all crates published" -ForegroundColor Green
