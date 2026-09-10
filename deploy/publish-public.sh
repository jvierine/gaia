#!/bin/sh
set -eu
test "$(id -u)" -ne 0 || { echo 'Run GAIA publication as j, not root' >&2; exit 1; }
export GAIA_ARCHIVE_ROOT=/mnt/data/juha/gaia
export GAIA_DB_PATH=/mnt/data/juha/gaia/gaia.sqlite3
export GAIA_SOURCES=/mnt/data/juha/gaia/code/sources
manifest_name=manifest.json
unset GAIA_PUBLISH_ALL
if test "${1:-live}" = archive; then
    # Refresh the live snapshot before a potentially longer historical rebuild.
    /bin/sh "$0" live
    export GAIA_PUBLISH_ALL=1
    manifest_name=archive-manifest.json
fi
exec 9>/mnt/data/juha/gaia/.publication.lock
flock -n 9 || exit 0
publisher_user=$(id -un)
public_target="$publisher_user@juha.no"
export RSYNC_RSH="ssh -o BatchMode=yes -o ControlMaster=auto -o ControlPersist=120 -o ControlPath=/home/$publisher_user/.ssh/gaia-publish-%C"
if test "$publisher_user" = bgu001; then RSYNC_RSH="$RSYNC_RSH -o IdentitiesOnly=yes -i /home/bgu001/.ssh/gaia_juha_no_ed25519"; fi
remote() { $RSYNC_RSH "$public_target" "$@"; }
/mnt/data/juha/gaia-build/release/gaia-server --publish
snapshot=$(mktemp -d /mnt/data/juha/gaia/.publication.XXXXXX)
trap 'rm -f "$snapshot/manifest.json" "$snapshot/assets.txt"; rmdir "$snapshot"' EXIT
cp "/mnt/data/juha/gaia/public/$manifest_name" "$snapshot/manifest.json"
node -e 'const fs=require("fs"),p=require("path");const m=JSON.parse(fs.readFileSync(process.argv[1]));const urls=[m.geometry_url,m.igrf_url,...m.images.flatMap(x=>[x.texture_url,x.source_map_url]),...m.lens_models.flatMap(x=>x.models.map(y=>y.url))].filter(Boolean);const names=[...new Set(urls.map(x=>p.basename(x)))];for(const name of names){if(!fs.statSync(p.join(process.argv[2],name)).size)throw new Error("Empty asset: "+name)}process.stdout.write(names.join("\n")+"\n")' "$snapshot/manifest.json" /mnt/data/juha/gaia/public/assets > "$snapshot/assets.txt"
# Local disk avoids SMB metadata latency. Files are renamed after transfer;
# only a verified snapshot can become the public manifest. Retain old assets.
rsync -r --size-only --chmod=D2775,F664 --files-from="$snapshot/assets.txt" /mnt/data/juha/gaia/public/assets/ "$public_target:/var/www/gaia-public/assets/"
rsync -r "$snapshot/assets.txt" "$public_target:/var/www/gaia-public/assets.pending"
rsync -r --checksum "$snapshot/manifest.json" "$public_target:/var/www/gaia-public/manifest.pending"
remote sh -s "$manifest_name" <<'REMOTE'
set -eu
cd /var/www/gaia-public
while IFS= read -r asset; do test -s "assets/$asset" || { echo "Missing asset: $asset" >&2; exit 1; }; done < assets.pending
mv manifest.pending "$1"
rm assets.pending
REMOTE
if test "$manifest_name" = manifest.json; then
    node -e 'const fs=require("fs"),m=JSON.parse(fs.readFileSync(process.argv[1]));const p="/mnt/data/juha/gaia/publication-status.json";fs.writeFileSync(p+".pending",JSON.stringify({verified_utc:new Date().toISOString(),latest_observation_utc:m.images.at(-1)?.at,frames:m.images.length}));fs.renameSync(p+".pending",p)' "$snapshot/manifest.json"
fi
