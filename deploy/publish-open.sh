#!/bin/sh
# Invoked under publish-public.sh's exclusive lock; no independent automation.
set -eu
export GAIA_PUBLISH_AUDIENCE=anonymous
export GAIA_PUBLISH_LOOKBACK_SECONDS=${GAIA_OPEN_LOOKBACK_SECONDS:-86400}
manifest_name=${1:-manifest.json}
/mnt/data/juha/gaia-build/release/gaia-server --publish
snapshot=$(mktemp -d /mnt/data/juha/gaia/.open-publication.XXXXXX)
trap 'rm -f "$snapshot/manifest.json" "$snapshot/assets.txt"; rmdir "$snapshot"' EXIT
/mnt/data/juha/gaia-build/release/gaia-overview "/mnt/data/juha/gaia/open/$manifest_name"
node deploy/overview-viewer.cjs "/mnt/data/juha/gaia/open/$manifest_name" "$snapshot/manifest.json"
node deploy/layer-assets.cjs "$snapshot/manifest.json" /mnt/data/juha/gaia/open/assets open > "$snapshot/assets.txt"
target="$(id -un)@juha.no"
/bin/sh deploy/transfer-layer-snapshot.sh open "$manifest_name" "$snapshot"
