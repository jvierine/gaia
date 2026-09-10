# GAIA handoff for j and bgu001

Updated 2026-09-10 UTC by Codex for Juha. This tracked file is exposed at
`/mnt/data/juha/gaia/agents.md` on Revontuli. Develop in the shared
`/mnt/data/juha/gaia/code` checkout, preserve each other's work, and push changes
to `jvierine/gaia`. Read the root `AGENTS.md` too.

MANDATORY: `juha.no/gaia` and `revontuli.uit.no/gaia` derive aurora stitching
from the SAME Rust publisher and SAME shared WebGL renderer. Only the Revontuli
UI has camera editing, masks, calibrations, pause/delete and other admin features.
The public build must use `VITE_GAIA_PUBLIC=1`; its API remains denied by Apache.

## Publication repair

The duplicate system/root `gaia-publish.timer` was disabled AND stopped;
its service was stopped. Root generated manifests but failed its SSH upload,
racing the working user publisher and leaving missing public textures.
The script now rejects root and takes an exclusive `.publication.lock`.

The sole automation is j's USER `gaia-publish.service` and `.timer` on Revontuli.
Use `systemctl --user status gaia-publish.service gaia-publish.timer` as j.
The timer waits 30 seconds after each completed run; it cannot overlap itself.
Do not start another publisher timer under root or bgu001.

The publisher snapshots the manifest and asset list, validates source files,
transfers assets using atomic rsync replacement (no --inplace), verifies every
referenced destination file exists and is nonempty, and then atomically replaces
the destination manifest. Old hashed assets are retained for open browsers.
The old age-only deletion was removed: a reused asset can still be referenced.
Reference-aware retention/garbage collection is a future task; monitor disk use.

## juha.no access and storage

`/gaia/public/` now uses `/var/www/gaia-public`, on local disk rather than the
slow SMB `/mnt/shovel` mount. `/var/www/html/gaia` still contains the GUI.
The public path is a symlink to `/mnt/gaia-public` on the separate local 4 GB
disk because juha.no's root filesystem was already 99% full. This is a rebuildable
serving cache: authoritative images and composites stay on Revontuli. If the
temporary local disk is reset, recreate the group-owned directories and run
publication before declaring the viewer healthy.
Both directories deliberately use `gaia-deploy`, with j and bgu001 as members,
2775 directories and 664 files. Bjorn can deploy GUI AND prepared public data as
`bgu001@juha.no`, using his existing Revontuli key
`/home/bgu001/.ssh/gaia_juha_no_ed25519`. He does not need j's account or key.
Preserve these permissions; coordinate any manual public-data repair with j's
automated publisher. Never publish a manifest before its assets are verified.

Apache config: `/etc/apache2/conf-enabled/gaia.conf` on juha.no;
tracked template: `deploy/apache-public.conf`. Old SMB public files remain
untouched for rollback. Revert the Apache alias only to a verified consistent
old manifest and asset set. No raw archives or camera calibrations were deleted.

## Viewer changes

Sun up is a compact checkbox. Vertical mouse/touch drag changes the viewing
tilt while solar azimuth remains locked, including views towards the nightside.
Live mode retains the last successfully loaded composite during publication
delays/fetch failures and shows the actual image time, marking delayed frames.
Historical playback continues to omit observations separated by real gaps.
The same renderer is used by admin and public shells; build/deploy both together.

## Complete retained record and crawler

The stitcher samples every available observation minute, not just every fifth
minute. `deploy/publish-public.sh archive` regenerates all retained calibrated
observation minutes using the same code into `archive-manifest.json`; it never
replaces the live manifest. `/gaia/?archive=1` opens full history on either host.
Raw records without valid calibrations cannot be mapped honestly and remain in
the camera archive. No original observations were deleted.

Crawler workers now exponentially back off individual failed sources (capped
at 15 minutes), resetting on success; healthy sources retain their cadence.
The duplicate publisher, rather than the StarVisor crawler, caused the missing
public files. Publication correctness is checked independently of acquisition.
A successful source download is not publication proof.

## Verification and follow-up

Check both live routes, including recent texture and attribution URLs, camera
hover/click, Sun up drag, and historical playback. Confirm a full publisher run
succeeds and its manifest references are present on local public storage.
Keep this note updated with subsequent changes. Do not silently reintroduce
SMB into public serving, parallel publishers, or separate renderer code paths.
