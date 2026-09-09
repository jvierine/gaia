#!/bin/sh
set -eu
export GAIA_ARCHIVE_ROOT=/mnt/data/juha/gaia
export GAIA_DB_PATH=/mnt/data/juha/gaia/gaia.sqlite3
export GAIA_SOURCES=/home/j/src/gaia/sources
/mnt/data/juha/gaia-build/release/gaia-server --publish
# The receiver never connects back. Retain one extra day of old assets for
# already-open viewers; cleanup is restricted to generated public WebP files.
find /mnt/data/juha/gaia/public/assets -maxdepth 1 -name '*.webp' -type f -mmin +2940 -delete
ssh -o BatchMode=yes j@juha.no 'mkdir -p /mnt/shovel/gaia/public/assets'
asset_list=$(mktemp)
trap 'rm -f "$asset_list"' EXIT
node -e 'const fs=require("fs"),p=require("path");const m=JSON.parse(fs.readFileSync(process.argv[1]));const urls=[m.geometry_url,m.igrf_url,...m.images.flatMap(x=>[x.texture_url,x.source_map_url]),...m.lens_models.flatMap(x=>x.models.map(y=>y.url))].filter(Boolean);process.stdout.write([...new Set(urls.map(x=>p.basename(x)))].join("\n")+"\n")' /mnt/data/juha/gaia/public/manifest.json > "$asset_list"
rsync -r --size-only --inplace --files-from="$asset_list" /mnt/data/juha/gaia/public/assets/ j@juha.no:/mnt/shovel/gaia/public/assets/
rsync -r --checksum --inplace /mnt/data/juha/gaia/public/manifest.json j@juha.no:/mnt/shovel/gaia/public/manifest.pending
ssh -o BatchMode=yes j@juha.no 'mv /mnt/shovel/gaia/public/manifest.pending /mnt/shovel/gaia/public/manifest.json'
