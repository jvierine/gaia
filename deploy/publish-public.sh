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
rsync -r --size-only --inplace --delete-delay /mnt/data/juha/gaia/public/assets/ j@juha.no:/mnt/shovel/gaia/public/assets/
rsync -r --checksum --inplace /mnt/data/juha/gaia/public/manifest.json j@juha.no:/mnt/shovel/gaia/public/manifest.pending
ssh -o BatchMode=yes j@juha.no 'mv /mnt/shovel/gaia/public/manifest.pending /mnt/shovel/gaia/public/manifest.json'
