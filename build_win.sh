#!/usr/bin/env bash
set -euo pipefail

DIST=target/dist
LAST_BUILT="$DIST/.last_built_version"

VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])")

if [[ -f "$LAST_BUILT" ]] && [[ "$(cat "$LAST_BUILT")" == "$VERSION" ]]; then
    echo "note: version $VERSION was already built — bump the version in Cargo.toml to build again" >&2
    exit 1
fi

cargo build --release --target x86_64-pc-windows-gnu

mkdir -p "$DIST"
cp target/x86_64-pc-windows-gnu/release/sshtool.exe "$DIST/sshtool.exe"
echo "$VERSION" > "$LAST_BUILT"

echo "Built sshtool.exe v${VERSION} → ${DIST}/sshtool.exe"
