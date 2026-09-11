#!/bin/sh
# Called under the shared publication lock. Immutable assets are acknowledged
# once, avoiding tens of thousands of SMB stat calls every live update.
set -eu
audience=$1
manifest_name=$2
snapshot=$3
case "$audience" in public|open) ;; *) exit 1;; esac
target="$(id -un)@juha.no"
state=/mnt/data/juha/gaia/publication-receipts
mkdir -p "$state"
receipt="$state/$audience-assets.txt"
token="$state/$audience-token"
remote_token=$(${RSYNC_RSH:-ssh} "$target" "test ! -f /mnt/shovel/gaia/serving/$audience/receipt-token || cat /mnt/shovel/gaia/serving/$audience/receipt-token")
if test -f "$receipt" && test -f "$token" && test "$(cat "$token")" = "$remote_token"; then
    comm -23 "$snapshot/assets.txt" "$receipt" > "$snapshot/new-assets.txt"
else
    cp "$snapshot/assets.txt" "$snapshot/new-assets.txt"
fi
split -n l/4 "$snapshot/new-assets.txt" "$snapshot/chunk-"
pids=''
for part in "$snapshot"/chunk-*; do
    rsync -r --size-only --files-from="$part" "/mnt/data/juha/gaia/$audience/assets/" "$target:/mnt/shovel/gaia/serving/$audience/assets/" &
    pids="$pids $!"
done
failed=0
for pid in $pids; do wait "$pid" || failed=1; done
test "$failed" = 0
rsync -r "$snapshot/new-assets.txt" "$target:/mnt/shovel/gaia/serving/$audience/assets.pending"
rsync -r --checksum "$snapshot/manifest.json" "$target:/mnt/shovel/gaia/serving/$audience/manifest.pending"
next_token=$(sha256sum "$snapshot/manifest.json" | cut -d' ' -f1)
${RSYNC_RSH:-ssh} "$target" sh -s "$audience" "$manifest_name" "$next_token" <<'REMOTE'
set -eu
cd "/mnt/shovel/gaia/serving/$1"
UV_THREADPOOL_SIZE=16 node -e 'const fs=require("fs");const names=fs.readFileSync("assets.pending","utf8").trim().split("\n").filter(Boolean);let i=0;Promise.all(Array.from({length:16},async()=>{while(i<names.length){const n=names[i++];if(!(await fs.promises.stat("assets/"+n)).size)throw Error("Empty asset "+n)}})).catch(e=>{console.error(e);process.exit(1)})'
mv manifest.pending "$2"
printf '%s\n' "$3" > receipt-token
rm assets.pending
REMOTE
cp "$snapshot/assets.txt" "$receipt.pending"
mv "$receipt.pending" "$receipt"
printf '%s\n' "$next_token" > "$token"
if test "$manifest_name" = manifest.json; then
    cp "$snapshot/manifest.json" "/mnt/data/juha/gaia/$audience/verified-manifest.pending"
    mv "/mnt/data/juha/gaia/$audience/verified-manifest.pending" "/mnt/data/juha/gaia/$audience/verified-manifest.json"
fi
rm "$snapshot/new-assets.txt" "$snapshot"/chunk-*
