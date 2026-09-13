# GAIA handoff for j and bgu001

## Camera registry loading (2026-09-13)

Removed the misleading two-camera fallback rows. Cameras tab now has an indeterminate progress bar throughout the real registry request (server returns one JSON array, no measurable percentage), explicit empty/error states, 45-second timeout and Retry. No swallowed registry errors or fake camera count; thumbnails remain lazy-loaded separately.


Header branding: display GAIA with Global Auroral Image Aggregator beneath it; no Data Center subtitle (user request, 2026-09-12).


## Camera-use permission notice (2026-09-12)

Shared About text on both sites explicitly states: Scientific and commercial use requires permission from the camera operators. GAIA access grants no reuse permission. Existing copyrights, provider terms, Google/Starvisor access controls, and imagery processing remain unchanged.


## Aggregator identity and initial-load transport (2026-09-12)

Deployed source 423a37b / public release b1667e6. Both live titles, About panels, source contacts and image canvases were checked in-browser. Public full-day playback was 15.008 seconds, 120 frames resident, with no browser errors; admin also reached 120 buffered frames and rendered correctly. Anonymous protected manifest remains HTTP 401. For the then-current 45-camera open catalogue: sheets 3,264,127 bytes versus previews 82,330 bytes; meshes 62,370,288 bytes versus compressed 12,259,719 bytes. Every tested compressed mesh round-tripped byte-for-byte. These are asset-size measurements, not a universal wall-clock speed guarantee. Three Rust overview tests pass (station identity, tile/timestamp mapping, preview and lossless geometry).

Visible name is Global Auroral Image Aggregator. Shared ProviderContacts.tsx explains overview-only service, directs original/high-resolution archive requests to provider PIs/operators, and records source-linked confirmed roles. Network contacts are not asserted to own every station. Old per-camera credits and copyrights remain.

Initial latest selection now uses 96px standalone JPEG previews for the newest three overview samples per station, not whole-day contact sheets. Play still loads the complete station-separated sheets and buffers 120 frames before advancing. Rust gaia-overview also publishes losslessly gzip-compressed meshes; the shared browser uses DecompressionStream with original-mesh fallback. No geometry, lens, mask, UV or weighting values change. Existing older archive packages remain compatible; new publication adds the optimized files automatically. Generic recursive asset validation includes the new URLs, with unchanged audience separation and Git-only code deployment.


## Authenticated calendar-day archives (2026-09-12)

Public date picker requires a Google session; ordinary signed-in users receive
open (non-Starvisor) archives, and only allowlisted users receive full archives.
The Rust /gaia/history/ gateway enforces this selection on EVERY manifest/asset
request. It never forwards to Revontuli. Anonymous users remain on the rolling
day, even if a date query is entered manually. Revontuli remains login-free.
GAIA_PUBLISH_DATE=YYYY-MM-DD builds a midnight-to-midnight UTC day as 120 slots;
raw files are unchanged and missing/unprojectable observations remain gaps.
All DB dates are inventoried by deploy/publish-days.cjs, including sparse dates.
Each day is advertised only after both audience packages have been verified.

juha dated storage is /mnt/shovel/gaia/YYYY-DD-MM/{full,open}/ with assets/ and
manifest.json. Public URLs retain ISO YYYY-MM-DD; the gateway converts safely.
/mnt/shovel/gaia/archive-days.json is the verified day list. These folders have
NO public Apache alias. API and identity integration stays local to juha.no.
Revontuli serves /gaia/public/days.json and days/YYYY-MM-DD/manifest.json.
Run publish-days.cjs under .publication.lock, up to 16 cores, never concurrently
with regular publication. Existing raw/minute data and rolling files are retained.
New gaia-server and gaia-overview binaries are required for date publication;
juha's small gateway needs the updated musl binary and history proxy location.
Deploy ALL code/binaries via the Git release branch, not rsync/scp. Only archive
data goes by rsync. Gateway restart invalidates old sessions (sign in again).

Eight dates found initially: 2026-04-13, 2026-09-01, 2026-09-02, and September
8 through 12. The three early dates contain only 2, 5, and 1 stored images;
raw presence does not imply calibrated coverage. Unit tests cover anonymous
rejection, ordinary/allowlisted audience separation, unsafe paths, and source
identity/contact-sheet tile placement. All eight dates completed verified publication at 07:39:56 UTC. Both audiences
are under their YYYY-DD-MM folders; original data were not moved or deleted.
Public Google sign-in and day selection were verified live for September 11:
00:00–23:48 UTC samples, all 120 GPU-ready, 15.000-second loop without buffering.
Revontuli also passed archive playback (15.007 seconds). Anonymous catalogue
and day-manifest endpoints returned 401. Sparse dates explicitly show no
calibrated coverage when none can be projected. The day picker refreshes its
catalogue every minute. publish-days.cjs can be rerun under the lock to refresh
all dates after more data/calibrations arrive; this bulk job is not a new timer.

## Full-day GPU-buffered overview (2026-09-12)

Final deployed source: 5b893ed; public release branch commit 26e227a.
Final live check: public 15.002-second loop; admin 15.003-second loop.
All 120 samples GPU-resident, no per-frame buffering. Public slider End
immediately selected 07:00 UTC with 37 station layers; source observation
time remained separately 06:57 UTC. Timeline now binds to the exact buffered
manifest snapshot, not a separately refreshed catalogue. Unauthorized Starvisor
sheet returned 401 under restricted and 404 under open.

Supersedes per-frame loading: gaia-overview is a Rust packer using up to 16
workers. It generates 120 twelve-minute samples spanning the rolling day,
station-separated 96px JPEG contact sheets, with source IDs and original
observation_at retained. Source masks/mesh/calibrations and all browser weighting
rules remain unchanged. These are overview samples, not every archived minute.
Browser preloads and decodes ALL samples into 64px GPU camera textures before
starting; pause/scrub then reuse the whole resident sequence. 32x is default:
15 seconds per day; slower speeds scale the wall-clock duration. No per-frame
network request or decode is needed after buffering. Anonymous sheets exclude
Starvisor; their catalogue points and provider credits remain. Auth remounts
clear all protected GPU textures on logout. Anonymous full-archive link removed.

publish-public.sh and publish-open.sh pack before transferring compact viewer
manifests. Local verified manifests MUST retain the original minute archive so
normal incremental publishers do not lose history. The old full-JPEG transfer
was stopped to unblock compact publication; gaia-publish.timer must be resumed
after verification. No originals were deleted. Data transfer remains rsync;
code release remains Git push/pull only. Build gaia-overview alongside gaia-server.
Publication timer has been resumed after verified compact transfer. Public live
browser also completed 15.006-second and repeated 15.003-second loops with all
120 samples ready. Home-key scrubbing immediately selected the oldest frame,
32 camera layers, no progress bar. Both viewers passed this check. Public
restricted manifest still returns 401 without a session. Two Rust tests verify
station identity rejection and red/green contact-sheet tile/time placement.
The browser freezes its history catalogue while exploring buffered data so a
live refresh cannot shift indices beneath a retained sequence.

Measured on live Revontuli browser: 120 frames GPU-ready, 15.003 sec loop,
unique camera identities, no WebGL errors. Camera sheets total 3.26 MB open /
5.11 MB full. Geometry downloads are additional. Date-gated archive selection
and migration of dated data into YYYY-DD-MM folders remain outstanding.

## Git-only code delivery and visible buffering (2026-09-12)

User requires ALL code delivery through Git push/pull, including the built
viewer. Never scp or rsync source files or frontend bundles between hosts.
Image assets, projection meshes and JSON observation manifests remain data and
may continue using the verified rsync publication pipeline.
Public static releases use branch codex/public-viewer in jvierine/gaia; build
on Revontuli, commit generated static files there, push, then pull on juha.no
under /mnt/shovel/gaia/viewer-release. Copy assets locally into www first and
replace index.html last. No builds or data caches on juha.no's system disk.
Retain old hashed assets for open browsers. This supersedes all older code-rsync
instructions below and in AGENTS.md.

Shared GaiaGlobeView shows an accessible buffering progress bar for the selected
frame on both sites. Counts are completed camera checks (including unavailable
observations), not invented bytes or a claim that the entire day is cached.
Camera failures are shown separately. Background prefetch cannot move the bar
for another selected time. Cancel playback pauses the shell and invalidates
pending frame selection, so stale completion cannot replace the selected view.
Full-sequence preload, date selection and 24h-in-15s default remain pending.

Deployment verified: source eb394d0, public release 6d0c2bd; admin asset
index-DiFjMt4Y.js, public index-CDeiuH4X.js. Live browser showed admin
1/63 then 54/63 camera checks, public 46/47; cancel stopped admin playback
while retaining 43 rendered camera layers with unique camera IDs. Public
rendered 38 camera layers with unique IDs. Git clone on shovel's CIFS mount
requires sudo (unprivileged Git cannot chmod config.lock); use sudo git -C
/mnt/shovel/gaia/viewer-release pull --ff-only for subsequent releases.
Only Git fetched code across hosts; install copied files locally on shovel.
Previous public index is retained in legacy/index-before-eb394d0.html.
The JPEG history rebuild was still active at this check; its publish timer
remains stopped until the job's EXIT trap resumes it. Do not duplicate jobs.

## Compact JPEG and station-safe GPU resources (2026-09-12)

New browser camera textures are RGB JPEG quality 80, maximum dimension 256.
Fixed crop/mask/edge feather remain in the reusable 100 km mesh and IGRF weights,
not in JPEG alpha; raw lens coordinates and scientific weighting are unchanged.
Texture addresses include source ID, observation timestamp and archive path.
Every newly published/retained layer frame carries source_id; browser rejects a
frame declaring a different station. Temporal smoothing and compositor texture
maps now use source IDs, not numeric slots. In-flight frame loads pin GPU caches;
eviction only happens when none are loading. Invalid GPU textures/buffers are
omitted before binding, never allowed to leave another station's binding active.

Camera accumulation is capped at 1024 pixels on its long side; the Earth,
coastlines, political boundaries, IGRF lines and labels retain full resolution.
Picking uses the identical reduced raster coordinates. Normal publication updates
newest JPEG frames first; GAIA_REBUILD_HISTORY=1 deploy/publish-public.sh then
rebuilds the rolling day without modifying camera/calibration state. Existing PNG
assets remain valid while a verified JPEG history replacement is transferred.

Validation: 20 real-GPU cases passed at 32px and 1536px output, on half-float
and portable paths: weighted overlap, mute, tiny weights, station-keyed texture
lookup and deleted-texture exclusion. Verified 819 published JPEG URLs against
the database source ID, timestamp and archive path (zero mismatches). First
100-file JPEG sample averaged 5524 bytes; a live public JPEG returned image/jpeg,
7075 bytes, 256x222. These are observations, not a claim of constant playback FPS.
At 06:22 UTC the j-user gaia-jpeg-history.service was rebuilding the rolling day
with up to 16 workers after delivering a newest-first preview. It holds the
publication lock; its EXIT trap restarts gaia-publish.timer. Do not start a
second publisher. Verify completion and timer resumption before claiming the
whole public day is JPEG. The small juha Google gateway was rebuilt for JPEG MIME
and restarted; sessions from before that restart require sign-in again.

## Touch station mute (2026-09-11)

Both viewers share a 550 ms single-finger station hold to toggle browser-only
mute, using the same function/storage as M. Touch station hit radius is 14 CSS
pixels; station rendering size is unchanged. Movement beyond 8 pixels, a second
finger, pointer cancellation, or release cancels the hold. A successful hold
does not open the provider link on release. Normal taps still open providers;
pinch/drag remain navigation. Canvas disables iOS touch callout/text selection
and contextmenu. About text and other page controls remain selectable.

## Playback identity and buffering fix (2026-09-11)

Concurrent `loadFrames` calls used to replace the shared sources array. Older
loads then used `sources.indexOf(s)` and got -1, sharing a smoothing texture
between unrelated cameras. Never key camera textures by mutable array identity.
The loader now snapshots its manifest and assigns stable per-source numeric
slots. It also resets temporal smoothing when calibration geometry changes.
Timeline requests have monotonic tickets; only the latest selected minute may
commit. Prefetch retains eight minutes ahead and two behind instead of clearing
everything every 30 seconds. Six camera workers prepare an atomic frame batch.
The public slider pauses playback on interaction, and catalogue refresh preserves
the selected observation time instead of silently shifting its array index.
Both shells offer speeds through 32x. Public 16x/32x skip respectively two/four
timeline steps per tick to keep display cadence bounded; speed is a target and
uncached frames still buffer rather than display a partial mixed batch.
The canvas exposes selectedEpoch and cameraOrderUnique for playback diagnostics.
Concurrent prefetch requests share in-flight geometry/image downloads. Verified
stable IDs against reordered/expanded catalogues, live public slider seek/pause,
and 32x controls on both live shells. Both rendered unique camera slots during
playback, with no new-build console errors at verification. Cold history still
buffers; do not promise video-rate uncached downloads.

## Public credits and account controls (2026-09-11, latest)

For bgu001: About no longer displays lens-model downloads or calibration
implementation instructions; preserve camera/provider links and institutional
acknowledgements. Admin calibration tools are unchanged. Signed-in account
details and Sign out live inside Info, never over the phone globe.

Anonymous manifests MUST retain Starvisor station metadata, coordinates,
provider links and credits. `imagery_restricted: true` identifies those sources.
They MUST NOT have a `projection` in the anonymous manifest, including retained
history. Their lens assets remain excluded. The publisher skips those sources
before image preparation/history merging; transfer validation rejects restricted
projections and lens assets, not public metadata. This supersedes older notes
below saying the anonymous catalogue contains zero Starvisor cameras.
Google authorization remains server-enforced on juha.no only, with the existing
allowlist. Revontuli remains without login. Do not loosen protected asset routes.

Verified rollout: 39 anonymous Starvisor station records, all with coordinates,
zero restricted projections and zero restricted lens models; unauthenticated
restricted manifest returns 401. Five Rust gateway tests and three publication
boundary checks passed. Phone-sized 390x844 browser check confirms Sign out
inside Info and no account banner over the signed-in globe. Both live viewers
render camera layers. Metadata-only rollout helper `refresh-public-stations.cjs`
can add public station records to an already verified open manifest without
copying imagery; normal subsequent publication maintains the same rule.

## CURRENT CONTRACT: browser composition (2026-09-11)

Verification at approximately 17:09 UTC: both running viewers displayed the
new camera layers. Public playback advanced with 111-120 display fps and zero
WebGL errors in this desktop browser (not an iPhone performance claim).
Compared 5,200,572 vertex weights across 91 matching cached meshes against
the legacy magnetic cache: ZERO byte differences. The loopback test page
`node deploy/verify-gpu.cjs` passed six real-GPU cases: weighted overlap, muted
overlap and weights down to 1e-20, on both half-float and portable paths; zero
weights excluded and all returned pixels matched within two channel levels.
Do not expose this numerical test page as part of the public deployment.
The first rule-preserving anonymous preview was published at 17:07 UTC;
full-day layer backfill/transfer was still running at this check. Older full
archive manifests may still be legacy atlases until explicitly converted with
`deploy/publish-public.sh archive`; do not claim their conversion is complete.
Five Google-gateway tests passed. Anonymous restricted requests returned 401,
legacy mixed /gaia/public returned 403, and the open catalogue had zero
Starvisor cameras. Google login on the public page was also verified as Juha.
Temporary browser verification tunnels must be closed after testing.

This section supersedes ALL historical atlas/local-disk descriptions below.
Juha explicitly changed final composition to the BROWSER on BOTH sites.
`backend/publish_layers.rs` now handles `--publish`: individual masked/cropped
100km meshes and maximum-256px textures, retaining original lens coordinates.
It never rasterizes a cross-camera atlas. The legacy atlas function remains
only as an unused numerical reference; no live publisher calls it.
`src/camera-compositor.ts` is the one shared WebGL compositor. `M` skips the
actual camera layer before normalization, exposing overlapping cameras beneath.

ALL old blend rules are retained. Static weights reuse the exact existing
magnetic-weight-v3 cache bytes (or the same full-IGRF/C-infinity equations).
GAIA_MAGNETIC_FALLOFF_DEG, GAIA_ZENITH_TAPER_START_DEG/WIDTH_DEG,
GAIA_MASK_FADE_PX, GAIA_SOLAR_DARK_DEG/LIGHT_DEG/FLOOR are published in the
manifest. Browser solar weights use the SELECTED composition epoch, not the
observation time; multiply by 2^quality_exponent and normalize sum(w*RGB)/sum(w).
Zero horizon weights remain zero; only the mask feather has its old 1e-6 floor.
There was no active photometric equalization or cloud rejection in the old
atlas loop, so this change does not pretend those dormant functions were active.
GPU half-float accumulation uses a cancelling per-pixel scale to avoid tiny
positive weights underflowing. The portable fallback uses the SAME normalized
running mean, not strongest-camera colours; RGBA8 rounding is lower precision.
Raster interpolation is now in the globe view rather than a 4096x2048 atlas,
so do not promise bit-identical output pixels. Unchanged compositions are cached.

Validate time-dependent parity with `node deploy/verify-composition-rules.cjs`.
The initial 27-case comparison against Rust had zero scale difference and
maximum solar elevation difference 1.56e-10 degrees. Existing Rust suite passed
63 tests (one optional ignored); continue running new tests before release.

Manifest schema `composition: browser-layers-v1`: camera.projection.images
contains at, geometry_url, texture_url, vertex_count, calibration_id. Meshes
are little-endian float32 x,y,z,u,v,weight (24 bytes/vertex). Top-level images
are timeline ticks, NOT finished textures. Honour each frame's own geometry.
Public anonymous namespace is /gaia/open/, approved Google sessions use
/gaia/restricted/, and admin uses its own /gaia/public/ without login.
All juha.no payloads are under /mnt/shovel/gaia/{www,runtime,serving,legacy}.
Old root checkouts and database folders and the full local serving cache were
archived and compared before removal. Root had 2.3GB free and /mnt 3.7GB free
after migration; backups remain recoverable in shovel/gaia/legacy.

Only j's USER gaia-publish.timer/service runs. It prepares up to 16 camera jobs,
publishes a live preview then a full day for pending calibration revisions.
Transfers use four disjoint rsync queues and 16-way destination verification.
Immutable-asset receipts avoid rechecking every historical SMB file each cycle.
Receipts are under /mnt/data/juha/gaia/publication-receipts; remote receipt-token
is in each serving audience. After ANY external asset deletion/restore, remove
that audience's receipt-token so the next transfer fully verifies all assets.
Do not delete assets by age. New assets are verified before manifest replacement.
No credentials or the account allowlist belong in Git.


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

## Independent branding and requested Starvisor access policy (2026-09-11)

Removed GAIA's UiT affiliation and logo from the public and admin About views.
Retain accurate producer institutions, acknowledgements, and copyrights.

Google login and Starvisor access control are REQUESTED, NOT YET IMPLEMENTED.
Use exactly the three Google emails in Juha's request, in a server-side
allowlist outside publicly served assets. The leading asterisk was formatting,
not a wildcard. Do not publish personal account lists in the public repository.

Required security boundary: re-render anonymous composites with Starvisor
excluded before blending; protect the full composites, manifests, historical
assets and direct asset URLs server-side. A dominant-source mask cannot remove
all blended contributions. Keep the same Rust stitcher and shared WebGL view.
Google sign-in can use a lightweight local Rust service on juha.no; never proxy
to Revontuli. Proposed OAuth callback: https://juha.no/gaia/auth/callback.
Client credentials are not yet supplied; keep any future secret outside Git.
The serving-cache disk /mnt/gaia-public was full on inspection. Resolve capacity
or implement reference-aware cache retention before publishing two audiences.
No access restriction has been deployed by this branding-only change.

Branding deployment verified in the public live Info panel; admin build is served
directly on Revontuli. Local browser DNS could not resolve revontuli.uit.no during
this check. Authentication is still pending, not protected by this deployment.

## CURRENT: Google access, storage and realtime recovery (2026-09-11)

This section supersedes earlier static-only, local-disk and pending-login notes.
User explicitly chose Google login ONLY on juha.no/gaia, not Revontuli.
MOMAP now lives at pikos.org/omaps. Its existing Google client still authorizes
https://juha.no (verified in Google Cloud console); no Google settings changed.
GAIA reuses that public client ID but has separate random HttpOnly Secure
SameSite=Lax sessions and single-use Google nonce challenges. No client secret
is required. Only the three exact emails specified by Juha are allowed; the
allowlist is outside Git under /mnt/shovel/gaia/runtime/allowlist on juha.no.
The leading asterisk in the request was formatting, not a wildcard.

backend/public_auth.rs is the small local Rust gateway, on 127.0.0.1:18766.
It verifies RS256 signatures with rotating Google certificates, audience, issuer,
expiry, verified email and nonce. POST origin must be exactly https://juha.no.
Sessions expire after eight hours; restart logs everyone out. It never calls
Revontuli. Google identity does not grant camera-admin rights.
Build the gateway for x86_64-unknown-linux-musl on Revontuli using musl-gcc;
juha.no's older glibc cannot run the normal Revontuli build.

URLs: /gaia/auth/* is local identity; /gaia/restricted/* requires a session
and allowlist membership on EVERY file request, including history. These
responses are private,no-store. Legacy /gaia/public/* is denied on juha.no
to prevent direct-URL bypass. /gaia/open/* is anonymous imagery, generated
with GAIA_PUBLISH_AUDIENCE=anonymous and Starvisor sources excluded before
blending, catalogue and lens export. Both render paths use backend/publish.rs.
Revontuli continues using its original full /gaia/public/ without login.
Previously downloaded public copies cannot be recalled from other browsers.

All juha.no payloads now target /mnt/shovel/gaia/: www/, runtime/, serving/public/,
serving/open/, legacy/. Do not use /mnt/gaia-public or /mnt/gaia-open for new data.
The system gaia-public-auth.service requires the shovel mount and reads its
allowlist/binary there. Apache active configuration is a REGULAR FILE at
/etc/apache2/conf-enabled/gaia.conf, not a symlink to conf-available! Update
the actual enabled file, run apachectl configtest, then reload Apache.
Tracked template is deploy/apache-public-auth.conf. System gaia.service stays
disabled (old crawler); the heavy service runs only on Revontuli.

The realtime outage was confirmed in publication logs: rsync receiver failed
with ENOSPC repeatedly. Last successful public delivery had been 08:54 UTC,
latest frame 08:52. Old hashed serving files filled the 4-GB local disk.
The replacement anonymous preview was HTTP/browser verified with a 15:47 UTC
frame. Migration preserves local snapshots in legacy/ before removal; raw
archive originals remain on Revontuli. Root checkouts were tar-archived and
compared before deletion, freeing about 1.1 GB on the system disk.

The existing j USER publish timer is the sole publication automation. Its
script publishes full assets to serving/public, then runs publish-open.sh for
anonymous assets. Both transfer assets first, validate refs, and rename manifest
last. Both use the same publication lock; do not start duplicate publishers.
GUI releases upload assets to /mnt/shovel/gaia/www, then atomically replace
index.html. Build admin and public from the same commit.

Auth tests include absent/forged sessions, unapproved accounts, approved test
sessions, forged Google credentials, exact Origin, and traversal rejection.
No test-only authentication bypass exists in the deployed server.

Routine publication now renders only the newest 20 minutes for each audience,
retaining valid earlier frames in the 24-hour manifest. Pending calibration
revisions still run 20 minutes then the full day, newest-first. Full archive
is explicit. Rendering audiences sequentially keeps the total cap at 16 cores.
This replaces the earlier background full-day stage on EVERY normal cycle.
Google login was verified in the browser as Juha's approved account, including
visible protected imagery; anonymous direct restricted URLs returned 401 and
legacy mixed public URLs returned 403. No Google client changes were needed.

2026-09-13 registry query optimization: added covering images(source_id, downloaded_utc DESC) index for latest-download and 24-hour counts. Apply the tracked index to the live Revontuli database; it changes no images or source records.

## AIDA calibration masking (2026-09-13)

AIDA latest and selected-history imports request ?calibration=true from GAIA image endpoints. Rust paints outside crop and inside enabled obstruction polygons black using the same conservative pixel-coverage rule as globe projection, keeping full original dimensions and coordinates. PNG avoids JPEG bleeding into black pixels. Metadata headers remain unchanged, originals are untouched, and no-store ensures mask edits apply on reopening. Disabled masks are ignored but crop remains active, matching projection. Standalone AIDA files are an existing non-Git installation; the narrowly scoped/idempotent deployment patch is tracked in GAIA at deploy/aida-calibration-mask.cjs and applied on Revontuli to /home/j/src/widefield-star-calibrator. The patch also updates the app.js cache version. Code changes are made on Revontuli and delivered via Git.

Verified live AIDA Kiruna handoff: masked PNG loaded, 482x482 dimensions and observation/site fields preserved, latest and selected-history endpoints both no-store. Two Rust mask tests passed. Backend restarted successfully; AIDA patch ran twice to check idempotence.
