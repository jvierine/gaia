# GAIA handoff for j and bgu001

## Calibration backfill and admin mute (2026-09-10)

Calibration uploads now transactionally select the new stationary-camera model
while retaining older models. SQLite triggers increment calibration_rebuild for
new/changed calibrations, model selections and camera enable/disable changes.
The user publisher checks this durable queue every 30 seconds and acknowledges
only its captured revision after successful full publication. New requests made
during rendering remain pending. No second publisher automation was added.
A queued run renders the latest 20 minutes, snapshots that manifest, then starts
the full newest-first 24-hour render WHILE uploading the preview. One renderer
uses at most 16 workers under the shared publication lock. Older valid frames
remain available during preview publication; attribution indices stay stable.
The previous serial 1/6/24-hour staging was replaced to avoid network idle time.
At 16:33 UTC PID 761429 was measured at 1406 percent CPU with the 16-core cap.
Paused globe times refresh their catalogue/frame every 30 seconds on BOTH
frontends. IMPORTANT CORRECTION: M is now strictly browser-only on both sites.
The earlier admin implementation that changed enabled/acquisition state was
removed. Only the explicit Cameras-tab pause control changes shared state. Yurga's existing AIDA
fit was explicitly selected to make its earlier stationary-camera frames usable.

## Admin history image sizing (2026-09-10)

FrameBrowser uses the original archived image endpoint rather than the
256-pixel globe texture. Explicit width/height and object-fit: contain make
the full image fit the preview area without cropping or distortion. Verified
in the live Revontuli history dialog using the 608 x 608 Skibotn image: it now
fills the preview height instead of remaining a 256-pixel thumbnail.

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
The publication script selects the invoking user's juha.no account. As bgu001
it explicitly uses his dedicated key, not `j@juha.no`. The shared flock still
prevents simultaneous manual/automatic publishers.

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

Failed asset HTTP responses must be `Cache-Control: no-store`; never cache a
404 as immutable for a year. The viewer retries failed textures with cache
reload, recovering clients that cached the previous missing-file responses.

GAIA JSON/JS/CSS/HTML use Apache compression on both hosts, configured by
`deploy/apache-compression.conf` at `/etc/apache2/conf-available/gaia-compression.conf`.
`deploy/gaia-public-tmpfiles.conf` recreates cache directories on juha.no at boot.
The backend's Publish status now reads `publication-status.json`, written only
after verified delivery. It reports the actual public observation time and
marks delays; the old empty mosaics table no longer determines publication status.

## Verification and follow-up

Check both live routes, including recent texture and attribution URLs, camera
hover/click, Sun up drag, and historical playback. Confirm a full publisher run
succeeds and its manifest references are present on local public storage.
Keep this note updated with subsequent changes. Do not silently reintroduce
SMB into public serving, parallel publishers, or separate renderer code paths.

## Parallel preprocessing (2026-09-10)

Publisher commit 8a44e68 uses a dynamic newest-first timestamp queue with up to
16 Rust worker threads. GAIA_PREPROCESS_WORKERS defaults to 16 and is clamped
to 1..16. Every worker retains its own magnetic-weight cache; cache writes use
unique temporary files and atomic rename. Blending equations, source selection,
GAIA_PUBLISH_ALL, and chronological manifest ordering are unchanged.
The installed j gaia-publish.service sets CPUQuota=1600%, MemoryMax=12G and
GAIA_PREPROCESS_WORKERS=16. The existing archive rebuild was NOT interrupted;
its next publisher invocation loads the new executable. The archive phase was
observed using about 13.7 CPU cores and completed generation with the new pool.

## Verified results (2026-09-10 UTC)

- The full archive rebuild and atomic public delivery finished successfully
  at approximately 16:09 UTC (service Result=success, exit 0). Both hosts serve
  `/gaia/public/archive-manifest.json` with 2,664 timestamps, and the normal
  user publication timer is active again. Open `/gaia/?archive=1` for this record.
- Reliability changes are pushed in `99de622` and `eaef0bf`; worker changes and
  cache concurrency regression coverage are in `8a44e68` and `8c38347`.
- Rust tests: 26 passed, one optional upstream test ignored. Both frontend
  builds succeeded. Public and admin live browser checks showed rendered
  composites, working Sun-up vertical tilt, and zero reported WebGL errors.
- The 16:03 verified live publication had 1,432 composites and no missing files
  among 2,908 referenced assets. The full rebuild generated 2,664 timestamps
  from retained calibrated observations, including sparse older records back
  to 2026-04-13; this is not continuous coverage between those dates.
- Actual bgu001 access was checked: he can read this handoff, write the shared
  publication lock, and use his own key/account to write both the public GUI
  and generated-asset directories on juha.no.
- HTTP checks verified successful manifest delivery with gzip, missing assets
  returning 404 with no-store, and the public admin API returning 403.
- Remaining limitations: uncalibrated raw images are retained but cannot enter
  the globe until calibrated; missing source observations cannot be invented.
  The local serving cache still needs reference-aware garbage collection;
  do not delete assets by age alone. Monitor `/mnt/gaia-public` disk capacity.

## Browser-only mute and delayed live: current deployed contract

- Both frontends use the shared WebGL implementation. Hover or tap a camera,
  then M toggles its attributed region. It does NOT call the enabled API, alter
  SQLite/calibrations, pause crawling, stop rendering, or enqueue a rebuild.
- Preferences are stored per browser/origin in localStorage under
  gaia-muted-cameras. Other viewers are unaffected. A small GPU visibility lookup
  uses the existing dominant-camera source map, so this is not restitching:
  pre-blended overlap contributions are not separated or recomputed locally.
  Attribution image caching is bounded to six maps and two reusable GPU textures.
- On both sites, src/live-time.ts defines LIVE_DELAY_MINUTES=10. Latest selects
  the newest published frame at or before that cutoff, not wall-clock now.
  Admin labels read now minus 10 min; public Latest reads Latest (-10 min).
  Playback retains original observation times; prefetching no longer changes the
  displayed frame timestamp. This delay is a safety margin, not a guarantee that
  every camera has an observation (daylight, outages and real gaps still apply).
- Regression command: node --test tests/live-time.test.mjs (2 tests). The distro
  Node lacks native TypeScript stripping, so the test uses installed TypeScript.
- Live M tests passed on Revontuli and juha.no, including shader mask activation,
  local persistence and show-again. Test cameras were restored. SQLite rebuild
  revision stayed 4/4 and Tromso AI Skibotn remained enabled; no backend mutation.
- The rebuilt Yurga record now appears as a contributor in 931 published frames,
  from September 9 16:42 to September 10 16:41 UTC in the checked snapshot.

## Current j@juha.no transfer and GUI release procedure

- Work in /mnt/data/juha/gaia/code; update this tracked handoff through its symlink
  /mnt/data/juha/gaia/agents.md. Build admin web-dist and separate public-dist
  from the same source revision. Do not run simultaneous builds/uploads.
- As j on Revontuli: npm run build:static, then
  VITE_GAIA_PUBLIC=1 npm run build:static -- --outDir public-dist.
  web-dist is served directly by Revontuli. Upload public-dist to
  j@juha.no:/var/www/html/gaia with rsync -r --size-only --chmod=D2775,F664
  --exclude=index.html; upload index.html to a release-specific .pending name,
  then rename it to index.html remotely AFTER assets arrive. Keep old hashed
  assets; never rsync --delete. Release-specific pending names avoid collisions.
- The sole USER gaia-publish.service pushes generated textures/lens files to
  j@juha.no:/var/www/gaia-public/assets, validates every snapshot reference, then
  atomically replaces manifest.json. /var/www/gaia-public points to local
  /mnt/gaia-public, not the slow SMB archive. No public API forwarding is used.
- A pending calibration rebuild renders a 20-minute preview and then launches
  the full 24-hour /gaia-server --publish process concurrently with preview
  rsync. Exactly one renderer uses GAIA_PREPROCESS_WORKERS=16 and CPUQuota=1600%.
  Upload time and cache hits need not consume 16 cores. Inspect the --publish
  process, not just the long-lived crawler/API process, when checking CPU usage.
- The revised pipeline completed revision 4 at 16:36 UTC; the normal 30-second
  timer remains active. Data remain authoritative under /mnt/data/juha/gaia.

## Daylight acquisition and the active backend unit (2026-09-10)

- Removed GAIA's solar-altitude acquisition gates: Norsk Meteor no longer skips
  above -4 degrees; the legacy darkness_sun_altitude_deg field is ignored and
  Iceland/Greenland records were cleared. Day and night use the SAME conservative
  cadence, quota limits, concurrency, timeout and backoff. Upstreams may still
  supply nothing or stale frames during daylight; never invent observations.
- Historical NMN planning covers full noon-to-noon days, not a solar-filtered
  subset. No new bulk backfill was launched as part of this change.
- Stitching has no hard daylight exclusion. Its existing positive solar-weight
  floor is 0.05; single-contributor normalization cancels that weight. The globe
  terminator only shades Earth, not the projected image layer.
- IMPORTANT: the live API/crawler is j's USER gaia-revontuli.service. Its live.conf
  drop-in enables crawling. Restart using systemctl --user restart
  gaia-revontuli.service after a backend build. The stale SYSTEM unit with the same
  name must stay stopped/disabled; starting it only creates port-conflict retries.

## Historical mute picking correction

At 2026-09-09 21:53 UTC several contributing cameras share station coordinates.
A directly hovered station marker takes priority (8 CSS pixel hit radius); elsewhere image attribution selects the camera. Picking
intersects the 100-km image shell rather than the Earth surface. The matching
source map is loaded before swapping a frame; failed map loads are retryable,
not permanently cached failures. This keeps M tied to the actual displayed
camera region when scrubbing history. Tests cover inverse shell projection.

### Hover and historical timeline keyboard correction

Station overlays retain source IDs in both admin and public views; M toggles exactly the named overlay camera. Timeline range focus no longer swallows M after scrubbing history (text fields still suppress shortcuts). Marker picking precedes image picking so a station dot always names that station.

## Pending backend install: star photometry writer (2026-09-11)

The running `gaia-server` predates commit `378f911` and must be replaced. bgu001
cannot do it: `/mnt/data/juha/gaia-build` is j-owned and the live unit is j's
USER `gaia-revontuli.service`. A release build of `378f911` is staged at
`/mnt/data/juha/gaia/staging/gaia-server-378f911`, sha256
`8f61ad8e95cd3186579f6f00e8f9db2e65df1f364b8ecf1835310cda479e66de`. As j:

    cp /mnt/data/juha/gaia/staging/gaia-server-378f911 /mnt/data/juha/gaia-build/release/gaia-server
    systemctl --user restart gaia-revontuli.service

The `f452a63` build this section first named was installed on 2026-09-11 at
13:01 and is superseded; it has been removed from staging so the older binary
cannot be installed by mistake.

Or rebuild from source in the usual place; the staged binary is only a
convenience. The SYSTEM unit of the same name stays disabled.

The frontend is already live: `web-dist` is `GAIA_STATIC_DIR`, so the Cameras
tab is serving the new quality-weight menu against the old backend. That is safe
but inert. The old settings handler writes `crop_json`/`mask_json` straight from
the request, so the menu reads the stored crop and mask back and sends them with
the weight rather than omitting them; without that a quality-only POST would
erase an operator's crop rectangle and obstruction outlines. Until the restart
the chosen weight is accepted and discarded, and reverts on reload.

After the restart the weight is read by the publisher, and the atlas and
source-map cache keys change (`atlas-v7`, `source-v8-indices`, both now hashing
the per-camera weights), so the first publish re-renders.

### What the 378f911 install turns on

The star photometry pass starts with the process. It is on by default and the
default catalogue path resolves, so from the restart it begins writing
`star_photometry` and `frame_sky` for archived frames in darkness. Measured
cost on a copy of the live archive: a second per full-resolution frame, twelve
frames every five minutes, a few percent of one core. `GAIA_STARPHOT_ENABLED=0`
stops it; every rate limit is an environment variable in
`deploy/gaia-revontuli.service`, and j's user unit needs those lines only to
override the defaults, not to run.

`db::open` now sets a 30 second busy timeout, which affects every process that
opens the database, the publisher included.
