#!/bin/sh
# Add GAIA refit-proposal support to WISC/AIDA.
#
# REFUSES unless app.js is byte-for-byte the version this patch was generated
# and verified against. AIDA is Juha's, it is edited there, and it has to keep
# working both standalone on juha.no/aida and as GAIA's calibrator. A patch that
# applies "close enough" to a file that has moved on is how that gets broken, so
# this would rather do nothing than guess.
set -e
APP=/home/j/src/widefield-star-calibrator/js/app.js
PATCH=/mnt/data/juha/gaia/staging/aida-proposal.patch
EXPECT=cb6481005bc6681eca8f1c04cfd05312

if grep -q applyGaiaProposal "$APP" 2>/dev/null && [ "proposal" = proposal ]; then
    echo "already applied; nothing to do"; exit 0
fi
if grep -q gaiaStarName "$APP" 2>/dev/null && [ "proposal" = names ]; then
    echo "already applied; nothing to do"; exit 0
fi

GOT=$(md5sum "$APP" | cut -d' ' -f1)
if [ "$GOT" != "$EXPECT" ]; then
    echo "REFUSING: app.js is not the version this patch was verified against." >&2
    echo "  expected $EXPECT" >&2
    echo "  found    $GOT" >&2
    echo "AIDA has been edited since. Ask Juha to review deploy/aida-proposal.patch" >&2
    echo "and apply it himself, or regenerate it against the current file." >&2
    exit 1
fi

STAMP=$(date -u +%Y%m%dT%H%M%SZ)
BACKUP="$APP.before-gaia-proposal-$STAMP"
cp -p "$APP" "$BACKUP"
echo "backed up to $BACKUP"
patch -p0 -i "$PATCH" "$APP"
TMP=$(mktemp /tmp/aida-check-XXXXXX.js)
cp "$APP" "$TMP"
if node --check "$TMP"; then
    rm -f "$TMP"; chown j:j "$APP" 2>/dev/null || true
    echo "done; app.js parses"
else
    cp -p "$BACKUP" "$APP"; rm -f "$TMP"
    echo "patched app.js does NOT parse; restored the backup" >&2; exit 1
fi
