#!/usr/bin/env bash
# Publish all workspace crates + the root binary to crates.io in
# dependency order. Run from anywhere; resolves to the repo root via
# this script's own location.
#
# Usage:
#   scripts/publish.sh              # real publish
#   scripts/publish.sh --dry-run    # cargo publish --dry-run for each
#   scripts/publish.sh --no-verify  # skip the post-package build check

set -euo pipefail

DRY_RUN=""
NO_VERIFY=""
for arg in "$@"; do
    case "$arg" in
        --dry-run)   DRY_RUN="--dry-run" ;;
        --no-verify) NO_VERIFY="--no-verify" ;;
        *) echo "unknown flag: $arg" >&2; exit 2 ;;
    esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Dependency-ordered: each crate must publish before any crate that
# depends on it. Root binary `infinity-msfs` last.
crates=(
    infinity-build-core
    infinity-build-sdk
    infinity-build-watch
    infinity-build-create
    infinity-build-js
    infinity-build-rust
    infinity-build-package
    infinity-msfs
)

for crate in "${crates[@]}"; do
    echo
    echo "──── publishing $crate ────"
    cargo publish -p "$crate" $DRY_RUN $NO_VERIFY
done

echo
echo "✓ all crates published"
