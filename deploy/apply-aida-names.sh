#!/bin/sh
# Apply the GAIA refit-proposal support to WISC/AIDA.
#
# AIDA is owned by j, so this needs to run as root or as j. It backs up the
# file, applies the patch, and checks the result parses -- restoring the backup
# if it does not, because a syntax error in app.js takes the whole calibrator
# down and the failure would only show in a browser console.
set -e
APP=/home/j/src/widefield-star-calibrator/js/app.js
PATCH=/mnt/data/juha/gaia/staging/aida-names.patch
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
BACKUP="$APP.before-gaia-names-$STAMP"

if grep -q gaiaStarName "$APP"; then
    echo "already applied; nothing to do"
    exit 0
fi

cp -p "$APP" "$BACKUP"
echo "backed up to $BACKUP"

if ! patch --dry-run -p0 -i "$PATCH" "$APP" >/dev/null 2>&1; then
    echo "the patch does not apply cleanly to this app.js; nothing changed" >&2
    rm -f "$BACKUP"
    exit 1
fi
patch -p0 -i "$PATCH" "$APP"

if command -v node >/dev/null 2>&1; then
    TMP=$(mktemp /tmp/aida-check-XXXXXX.js)
    cp "$APP" "$TMP"
    if node --check "$TMP"; then
        echo "patched app.js parses"
        rm -f "$TMP"
    else
        cp -p "$BACKUP" "$APP"
        rm -f "$TMP"
        echo "patched app.js does NOT parse; restored the backup" >&2
        exit 1
    fi
fi

chown j:j "$APP" 2>/dev/null || true
echo "done. Reload /aida/; proposal stars now carry bright-star names where one is known."
