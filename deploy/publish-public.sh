#!/bin/sh
set -eu
test "$(id -u)" -ne 0 || { echo 'Run GAIA publication as j, not root' >&2; exit 1; }
export GAIA_ARCHIVE_ROOT=/mnt/data/juha/gaia
export GAIA_DB_PATH=/mnt/data/juha/gaia/gaia.sqlite3
export GAIA_SOURCES=/mnt/data/juha/gaia/code/sources
unset GAIA_PUBLISH_AUDIENCE
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
# Rebuild requests are transactionally recorded by SQLite triggers, including
# uploads outside the HTTP API. A newer request arriving during this run remains
# pending: only the captured revision is acknowledged after verified delivery.
revision=$(sqlite3 -cmd '.timeout 10000' "$GAIA_DB_PATH" 'SELECT requested FROM calibration_rebuild WHERE id=1;')
completed=$(sqlite3 -cmd '.timeout 10000' "$GAIA_DB_PATH" 'SELECT completed FROM calibration_rebuild WHERE id=1;')
stages=1200
if test "$manifest_name" = archive-manifest.json; then stages=86400; fi
if test "$manifest_name" = manifest.json && test "$revision" != "$completed"; then stages='1200 86400'; fi
if test "${GAIA_REBUILD_HISTORY:-0}" = 1; then stages='1200 86400'; fi
snapshot=$(mktemp -d /mnt/data/juha/gaia/.publication.XXXXXX)
render_pid=''
trap 'if test -n "$render_pid"; then wait "$render_pid" || true; fi; rm -f "$snapshot/manifest.json" "$snapshot/assets.txt"; rmdir "$snapshot"' EXIT
for lookback in $stages; do
export GAIA_PUBLISH_LOOKBACK_SECONDS="$lookback"
echo "GAIA calibration rebuild revision=$revision lookback_seconds=$lookback (newest first)"
/mnt/data/juha/gaia-build/release/gaia-server --publish
/mnt/data/juha/gaia-build/release/gaia-overview "/mnt/data/juha/gaia/public/$manifest_name"
node deploy/overview-viewer.cjs "/mnt/data/juha/gaia/public/$manifest_name" "$snapshot/manifest.json"

node deploy/layer-assets.cjs "$snapshot/manifest.json" /mnt/data/juha/gaia/public/assets public > "$snapshot/assets.txt"
# juha.no stores every serving asset under /mnt/shovel/gaia by user request.
# Files are renamed after transfer;
# only a verified snapshot can become the public manifest. Retain old assets.
/bin/sh deploy/transfer-layer-snapshot.sh public "$manifest_name" "$snapshot"
if test "$manifest_name" = manifest.json; then
    node -e 'const fs=require("fs"),m=JSON.parse(fs.readFileSync(process.argv[1]));const p="/mnt/data/juha/gaia/publication-status.json";fs.writeFileSync(p+".pending",JSON.stringify({verified_utc:new Date().toISOString(),latest_observation_utc:m.images.at(-1)?.at,frames:m.images.length}));fs.renameSync(p+".pending",p)' "$snapshot/manifest.json"
fi

GAIA_OPEN_LOOKBACK_SECONDS="$lookback" /bin/sh /mnt/data/juha/gaia/code/deploy/publish-open.sh "$manifest_name"
done
if test "$manifest_name" = manifest.json; then
    case "$revision" in ''|*[!0-9]*) exit 1;; esac
    sqlite3 -cmd '.timeout 10000' "$GAIA_DB_PATH" "UPDATE calibration_rebuild SET completed=MAX(completed,$revision) WHERE id=1;"
fi
