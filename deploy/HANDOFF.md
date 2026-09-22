# GAIA handoff for j and bgu001

## Event catalogue navigation (2026-09-22)

Deployed source `7aca336` on Revontuli. The normal `/gaia/` header now has an
Events tab immediately after Contribute. It opens an extensible event catalogue;
the first entry links to the isolated November 2025 G4 study at
`/gaia/events/20251111/`. This is a shell-only navigation change: the catalogue
does not query or modify the realtime database, crawler, projection or
publication paths. All 73 frontend tests passed and the static build completed.
The live admin bundle is `main-DHhhQJe9.js` with stylesheet
`main-fIHicVJd.css`; browser verification showed the Events tab, the G4 card and
its canonical link with no console errors.

The ordinary `npm run build:static` initially stopped because the retained
`web-dist/assets-before-recovery` directory is owned by `bgu001` with an ACL
mask that denies `j` write access. It was left untouched. The same checked-out
commit was built into ignored `work/events-build`, then its hashed assets were
installed locally on Revontuli with `index.html` replaced last. The unrelated
modified Apache files and backup files in the shared checkout were preserved.

## Isolated November 2025 event study (2026-09-22)

Deployed GAIA source `77e3867` at `/gaia/events/20251111/`. This is a separate
static event shell: it reuses the shared globe's Earth, boundaries, magnetic
contours, solar illumination and navigation, but starts the globe in base-only
mode. Base-only mode does not request `/gaia/api/sources`, camera projections,
the realtime database, the crawler, or the publication pipeline. Normal
`/gaia/` keeps the default realtime mode and its existing API/data paths.

The event package under `/mnt/data/juha/gaia/public/events/20251111/` is 185 MB:
1,230 compact located/timed previews (1,206 still images and 24 timelapses), an
attributed manifest, and five-minute adaptive playback bins. Exact per-record
UTC, coordinates, provenance, precision and rights notes stay in the manifest.
All 24 timelapses have bounded 720p H.264 research previews with HTTP range
playback; their source links and rights notes remain visible beside the player.
The 7.5 GB research originals were not published or modified. Six downloaded
records without a recoverable time and 24 without numeric coordinates are
reported as excluded rather than placed approximately. Preparation is an
explicit offline command (`tools/prepare-event-study.mjs`), not a service,
timer or realtime image-processing stage.

AIDA source `08db051` accepts the event viewer's same-origin, prefix-validated
event-image handoff, loads its UTC/location fields, and leaves the ordinary
GAIA source/image calibration upload path unchanged. Event fits are downloaded
as HDF5; they are not inserted into the fixed-camera calibration table.

Validation: all 73 GAIA frontend logic tests and all 111 AIDA tests passed;
both GAIA static entry points built. Live HTTP checks returned 200 for the
normal viewer, source API, event route, 1.46 MB event manifest, representative
thumbnail/preview, and AIDA. The deployed admin bundle is
`main-DunTn8Zr.js`; the normal manifest/API services were not restarted.
The required matching public build was delivered through release branch
`codex/public-viewer` at `6140d6c` and installed on juha.no with bundle
`main-DHBdHhys.js`; the previous index is retained as
`legacy/index-before-6140d6c.html`. Live HTTPS checks returned 200 for the
public index, JS, CSS and 6.87 MB open manifest. Event data remain Revontuli-only.

## For j: two ownership settings on web-dist (2026-09-16)

Neither is urgent and nothing is broken -- the directory ACL grants j `rwx`,
so serving and future installs work as they are. Both are here because they
will otherwise have to be redone by hand after every frontend install.

**1. `web-dist/assets` lost its setgid bit.** It is `drwxr-xr-x+` and was
`drwxrwsr-x+`. Without it, files created there no longer inherit the
`gaia-dev` group, which is exactly the inheritance that would stop this
recurring:

    sudo chmod g+ws /mnt/data/juha/gaia/code/web-dist/assets

**2. `bgu001` is not in `gaia-dev`** (its groups are `bgu001 sudo lxd`), so it
cannot `chgrp` what it extracts and Björn has had to fix the group by hand
after an install:

    sudo usermod -aG gaia-dev bgu001    # takes effect on a fresh login

Worth knowing even with both applied: `tar` sets modes from the archive and so
defeats setgid on extraction anyway. Either extract with
`tar --no-same-permissions`, or follow an install with

    chmod 664 web-dist/index.html web-dist/assets/*
    chgrp gaia-dev web-dist/index.html web-dist/assets/*

One straggler, if the directory is ever made uniform:
`web-dist/assets/index-BYJbR12g.js` is still `0644`. It is the superseded
bundle from `fb60ef1`, no longer referenced by `index.html`, and harmless.


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

## Pending install: movable keogram windows and the lens drift check (2026-09-16)

Supersedes the entry below and includes it.

**Keograms.** A selected window carries its own rows, so selections survive
moving between nights and several feed one scatter. Either end drags either
way. Windows retire when the pair changes.

**Lens drift.** New `GET /api/sources/{id}/calibration/drift` returns
tonight's richest frame with each star's projected and fitted position; the
Calibration tab draws them purple and amber on the frame. New
`POST /api/sources/{id}/calibration/refit` refits the eight optical
parameters and writes a real AIDA/WISC HDF5 into
`<archive>/calibrations/<source>/`, adding it to the list **unselected**.

Two things to watch. The refit shells out to `python3` with `h5py` and
`numpy` -- both are present on Revontuli now, and the endpoint fails cleanly
with the python error if they ever are not. And `/original` is the full-size
frame, so the drift panel downloads one archived image per open; that is the
same image the frame browser already serves, but it is not a thumbnail.

Both halves. As bgu001, from the repository root:

    tar xzf /mnt/data/juha/gaia/staging/web-dist-a25b5bf.tar.gz
    chmod 664 web-dist/index.html web-dist/assets/*

then the binary:

    sudo install -o j -g j -m 755 /mnt/data/juha/gaia/staging/gaia-server-a25b5bf /mnt/data/juha/gaia-build/release/gaia-server.new
    sudo mv /mnt/data/juha/gaia-build/release/gaia-server.new /mnt/data/juha/gaia-build/release/gaia-server
    sudo kill -9 $(pgrep -u j -f '/mnt/data/juha/gaia-build/release/gaia-server')

sha256: binary `64e424ee6f534d11857393efa84f5060dee2a667cea4647a9bea1870bfc48ac1`,
bundle `c9531949cd31ba278bf80c7b175f2dc286d4e6ec30a83f9346d43847e4203985`.

Back out with `gaia-server-807546a` and `web-dist-a455d39.tar.gz`. No schema
change. Let any publish in flight finish first.

## Pending install: cloud weight, flat cells (2026-09-16)

Supersedes the entry below. Two changes on top of it: a frame needs at least
three usable stars before it publishes a field at all, and the Hann taper
across each cell is off, leaving the flat cell value with the smooth
transition across the boundaries.

The star floor came from the first live publish: fourteen fields in forty-one
were one value across the whole image -- a single star scaling an entire
all-sky camera -- and eight of those dimmed the camera, the worst to 4.7 per
cent. `GAIA_CLOUD_HANN=1` restores the taper without a rebuild, and both
manifests now publish which field actually built the mosaic.

Binary only; the frontend is unchanged since `a455d39` and needs no
re-extraction.

    sudo install -o j -g j -m 755 /mnt/data/juha/gaia/staging/gaia-server-807546a /mnt/data/juha/gaia-build/release/gaia-server.new
    sudo mv /mnt/data/juha/gaia-build/release/gaia-server.new /mnt/data/juha/gaia-build/release/gaia-server
    sudo kill -9 $(pgrep -u j -f '/mnt/data/juha/gaia-build/release/gaia-server')

Let any publish in flight finish first, or it is interrupted mid-run.

## Pending install: the cloud weight in the composite (2026-09-16)

A weight from the star fading now multiplies both composites -- the server-side
atlas in `publish.rs` and the browser layers via a small per-frame greyscale
field published beside each texture (`cloud_url` in the manifest) and sampled
in the fragment shader.

**PRELIMINARY.** Saturated stars are excluded, because bright aurora over a
clear sky and moonlit thin cloud clip a star's peak identically and mean
opposite things. That biases towards clear. `GAIA_CLOUD_WEIGHT=0` in the
service environment turns it off without a rebuild -- use that if the mosaic
looks wrong rather than rolling back.

Watch for two things after install. The publisher now does one extra grouped
query per camera and one small PNG per frame, so a first full publish is a
little slower; and layers whose frame had no usable stars carry no `cloud_url`
and must composite exactly as before, which is the case to check first.

Both halves together -- the shader reads a field the old publisher never wrote,
and the old shader ignores one the new publisher does. As bgu001, from the
repository root:

    tar xzf /mnt/data/juha/gaia/staging/web-dist-a455d39.tar.gz
    chmod 664 web-dist/index.html web-dist/assets/*

then the binary:

    sudo install -o j -g j -m 755 /mnt/data/juha/gaia/staging/gaia-server-a455d39 /mnt/data/juha/gaia-build/release/gaia-server.new
    sudo mv /mnt/data/juha/gaia-build/release/gaia-server.new /mnt/data/juha/gaia-build/release/gaia-server
    sudo kill -9 $(pgrep -u j -f '/mnt/data/juha/gaia-build/release/gaia-server')

Back out with `gaia-server-2625ee0` and `web-dist-2625ee0.tar.gz`. No schema
change.

## Pending install: paired keograms (2026-09-15)

Two new endpoints and a new sub-tab. `/api/sources/{id}/keogram-pairs` lists
the cameras a given one can be compared against -- enabled, located,
calibrated, and within 746 km, which is twice the ground reach at the taper
knee. `/api/sources/{id}/keogram?partner=...` samples the equidistant cut in
both cameras on a common time grid and returns the two profiles per row.

Cost, since it is the thing to watch: one texture decode per camera per row.
Rows are capped at 300 and a wide window is thinned rather than truncated, so
a 24 h request loses cadence instead of its tail. Textures come from
`projection-cache`, shared with the composite, so a window that has been
published is cheap and one that has not pays a full-resolution decode per
frame the first time only. If the endpoint is ever slow, that is why.

Both halves have to go in together -- the panel calls endpoints the old binary
does not serve. As bgu001, from the repository root:

    tar xzf /mnt/data/juha/gaia/staging/web-dist-fb60ef1.tar.gz

then the binary, by whichever route is available (the release directory is
owned by j; `kill -9` is what makes `Restart=on-failure` fire, a plain TERM
would leave the service down):

    sudo install -o j -g j -m 755 /mnt/data/juha/gaia/staging/gaia-server-fb60ef1 /mnt/data/juha/gaia-build/release/gaia-server.new
    sudo mv /mnt/data/juha/gaia-build/release/gaia-server.new /mnt/data/juha/gaia-build/release/gaia-server
    sudo kill -9 $(pgrep -u j -f '/mnt/data/juha/gaia-build/release/gaia-server')

sha256: binary `fa9405f76e1b25601e0644552b2eab374cf1c0d2fea14342631d796111e7e833`,
bundle `bba348e19727cbf60d47e7fb419fba1b27a313af3dd66d6658bb2e21c821b86c`.
Back out with `gaia-server-3d7fae9` and `web-dist-9bd0b5a.tar.gz`. No schema
change either way.

## Pending backend install: the star series read the wrong column (2026-09-15)

`/api/sources/{id}/stars/series` listed `p.optical_depth` as its 22nd column
and read it from the 27th, which is `s.sun_elevation_deg`. So every sample
came back carrying the sun's elevation as its optical depth -- a smooth -15
to -18 through the evening, which reads as a plausible depth and is why the
fault survived. The five moon and sun fields were each shifted by one for the
same reason. The frame endpoint, which is what the panel actually draws, was
never affected, so nothing wrong was displayed; the API was the liar.

The columns are addressed by name now, and the query sits in
`star_series_samples` so a test can call it. The test plants a different
value in every column, which is the only kind that catches a shift of one.

Staged as `gaia-server-3d7fae9`, sha256
`e0e3bea00158d218198d284b9bc2d8f73f88557bfe34138d18fc8068b256b0db`. Install
as j:

    cp /mnt/data/juha/gaia/staging/gaia-server-3d7fae9 /mnt/data/juha/gaia-build/release/gaia-server
    systemctl --user restart gaia-revontuli.service

To go back, the previous binary is `gaia-server-099c8dc` in the same
directory. No schema change, so a rollback needs nothing else.

## Pending backend install: the season-long distribution (2026-09-15)

The running `gaia-server` was installed 2026-09-15 18:28 at commit `c7c9fef`,
so it has the stalled-fit fixes, frame stepping and the extinction endpoint.
What it lacks is the endpoint the histograms read. bgu001
cannot do it: `/mnt/data/juha/gaia-build` is j-owned and the live unit is j's
USER `gaia-revontuli.service`. A release build of `a36b511` is staged at
`/mnt/data/juha/gaia/staging/gaia-server-a36b511`, sha256
`69c004aab1e45602f203642a7339bc2f499496c183f49a0b06bc79985e479413`. As j:

    cp /mnt/data/juha/gaia/staging/gaia-server-a36b511 /mnt/data/juha/gaia-build/release/gaia-server
    systemctl --user restart gaia-revontuli.service

`gaia-server-b8d2d86` stays in staging on purpose: it is the binary currently
installed, and the way back.

Earlier staged builds are removed as they are superseded, so the staging
directory only ever holds the one to install.

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

AIDA masked-image handoff is now committed in its own jvierine/widefield-star-calibrator repository as well. Its Git checkout on Revontuli is /mnt/data/juha/gaia/aida-code; js/app.js and index.html were byte-compared against the running AIDA installation. Keep future AIDA code changes in that Git repository.

## 2026-09-13 AIDA explicit masks (supersedes black calibration copies)
- AIDA main 1d8f131 deployed on Revontuli from /mnt/data/juha/gaia/aida-code.
- Latest and historical GAIA handoffs load untouched original pixels and GET /api/sources/{id}/settings separately. Crop and enabled normalized polygons feed AIDA star-detection and centroid mask predicates, with padding.
- Physical all-sky annulus inference remains unchanged and sees original pixels, never synthetic black obstruction boundaries. Cropping never shifts/resizes pixels or lens coordinates.
- The old deploy/aida-calibration-mask.cjs patch is retired and fails explicitly to prevent reintroducing the bug. The optional backend calibration=true image endpoint remains for compatibility but AIDA no longer uses it.
- Scoped AIDA tests: 29 passed, 1 skipped, including existing all-sky annulus tests and new crop/multipolygon/edge-padding/disabled-mask regression.

## 2026-09-13 Camera preprocessing visibility and AIDA updates (for bgu001)
- AIDA Git main d8a1dbe fixes GAIA observation metadata being overwritten by original-image EXIF; b45519e adds Mac trackpad rotation to header, on-image help and help.html: Command+Shift+normal click-drag left/right. AIDA runs from /home/j/src/widefield-star-calibrator, installed from /mnt/data/juha/gaia/aida-code. Push/pull code through Git; no scp/rsync code.
- GAIA now records per-camera atomic progress JSON under /mnt/data/juha/gaia/processing-status (hashed source IDs). Only the full-audience rolling publisher writes these, not anonymous or date/archive jobs. Counts refer to the current newest-first pass; the existing calibration-triggered 20-minute pass is followed by a 24-hour pass.
- /api/sources includes processing state/counts/error. Cameras tab refreshes every 15 seconds and on window focus, with progress bars, queued/processing/publishing/ready/no-images/error/stalled states. Ready means Revontuli manifest published, not proof of delivery to juha.no. This is admin-only status; public viewer retains its existing access controls.
- Calibration upload immediately wakes the existing user gaia-publish.service with systemctl --user start --no-block. SQLite rebuild triggers and the timer remain the durable queue; existing flock and 16-worker cap are unchanged. A running publisher completes its captured revision; later changes remain pending for the timer.
- A snapshot older than the camera settings/calibration is reported queued; missing snapshots are queued, not fabricated ready. Ten minutes without worker progress is reported stalled. Original downloads, lens coordinates and projection rules are unchanged.
- Iceland Aðaldalshraun calibration saved 08:34 UTC; pre-existing trigger started at 08:35. Verified live 08:37 manifest contains 274 projected Iceland frames with calibration dca4884f-d762-4dd4-a7cc-d6616871f2b6. Lack of status in the old UI did not mean lack of processing.
- Development/builds are exclusively on Revontuli. This handoff is shared via /mnt/data/juha/gaia/agents.md -> code/deploy/HANDOFF.md; read it and git status/log before edits so simultaneous j/bgu001 work is preserved.

Deployment verification: acdca48 pushed to GAIA main; Rust release and admin frontend built, gaia-revontuli restarted, existing publisher ran successfully. New atomic-status/recalibration regression passed. Live API returned 172 cameras and Iceland ready (6/6 frames in the latest incremental pass); direct Revontuli browser Cameras tab visibly shows "Projected images ready · 6 frames". Counts are current pass, not full retained archive. No calibration was changed for this test.

## 2026-09-13 STARVISOR camera-specific credits
Scraped all 40 homepage station pages (39 registered GAIA sources plus separate Irbit NIR). src/starvisor-credits.json preserves each cam-info attribution, checked timestamp, station URL and contributor profile links. Shared ProviderContacts renders these on admin and public info/credits pages regardless of image authorization. These are installation credits, NOT invented PI or ownership claims. sources/starvisor.json acknowledgements updated for all 39 registered sources; source identity, calibration, ingestion settings and copyright statements unchanged. Refresh with node deploy/scrape-starvisor-credits.mjs from the repo: sequential polite requests, homepage discovery, no image downloads. Review generated diff before deploying; missing credit is explicit, not fabricated. STARVISOR Google allowlist/image restrictions unchanged.

## 2026-09-13 Remove public Starvisor login notice
Removed the top-level Starvisor imagery requires an authorized Google account sentence. Google sign-in remains; access still requires membership in the existing three-person allowlist, not merely Google login. No authentication, authorization, audience selection or restricted-file checks changed.

## Header sign-in placement
Google sign-in now renders beside Info & credits via SignInContext, using the official Google button at 144x40 pixels, matching Info. Header wraps on phones. Removed the separate account bar and height observer. Signed-in account/sign-out remain inside Info; allowlist and backend auth unchanged.

## Public playback dock parity
juha.no now uses the compact Revontuli-style overlay dock: circular play, minus/plus speed controls, Sun up, timestamp and timeline ticks. Public signed-in date selection and latest action remain, and auth rules are unchanged. Speeds now match Revontuli (0.25x through 32x). Both frontends built from the same source; compositor and buffering unchanged.

Player parity correction: BOTH shells now import src/PlaybackToolbar.tsx and src/playback-toolbar.css. No separate public/admin toolbar markup or styling. Shared component owns speed steps, control labels, time display, Sun up and slider events. Shells only adapt their existing timeline state and supply access-controlled archive choices. Shared globe/compositor/buffering remains unchanged. Do not fork this component by deployment.

Globe gesture hint removed from shared GaiaGlobeView. Instructions now live in shared Info/About ServiceDescription. Shared playback dock moved down to a 14px safe-area-aware bottom inset instead of reserving 58px for instructions. Keep mobile viewing free of persistent instructional text.

STARVISOR credits rechecked from all homepage station pages. Added a direct camera-by-camera credits jump link near the top of Info on both shells; section displays total camera count. Original station and contributor links retained. Live public browser verified individual attributions, including Provideniya and Popovo.

## Camera hover credits
Shared globe station-marker and projected-image hover now use src/starvisor-credits.json by exact source_id, the same data as Info. Displays the original installation credit (not a falsely inferred owner/PI title). Non-Starvisor cameras retain their producer label. Applies equally to anonymous station points and authorized/history images. Tooltip wraps and clamps to viewport; no imagery access changes.

## Camera frame history UTC date selection
Admin camera history now has a UTC date picker plus Last 24 hours reset. GET /api/sources/{id}/frames?date=YYYY-MM-DD selects that whole UTC day (inclusive midnight, exclusive following midnight); default retains rolling 24h. Strict date validation returns 400. Loading/empty/error states explicit, aborted stale requests cannot replace selected-date frames. Original selected-image AIDA links retained. This is admin raw-image browsing only, not a public archive access change.

About opening simplified at user request: removed the STARVISOR jump link and Globe controls paragraph from shared ServiceDescription. Main service description now opens About. Camera-specific credit section remains intact; no persistent globe hint restored.

Removed the requested manual-review/automatic-change sentence from both About suggestions and the suggestion form. Submission behavior is unchanged.

Removed the redundant globe-injected Choose archive day / Live view link and its cleanup handler from shared globe.ts. The existing playback date selector remains unchanged. Applies to both viewers; no archive permissions or playback behavior changed.

### What the 66cb4d3 install added, now in place

The clear-sky reference. The photometry cycle now also fits one extinction
coefficient per camera, night and colour channel to the upper envelope of the
star brightnesses, storing it in `extinction_nights` with a zero point per star
in `star_zero_points`. Both tables are created on open. It is bounded work, 24
fits a cycle by default, and every limit is a `GAIA_STARPHOT_FIT_*` variable in
the unit file. On the archive it accepts 151 fits and refuses 745; the accepted
ones average 0.179 mag per air mass against a literature clear sky near 0.20.

`GET /api/sources/{id}/stars/frame`, which the Cameras-tab photometry panel uses
to open the frame behind a scrubbed instant. NOTE: `web-dist` is served straight
from the repo, so that panel is already live in front of the older backend. The
request 404s there and the panel simply stays on its aggregate scatter, which is
inert rather than harmful, but scrubbing does nothing until this build is in.

### What the d1adbb8 install added, now in place

`GET /api/sources/{id}/stars/frame` gains a `field` member: the camera's own
horizon projected into its image, so the star photometry tab can cut the Voronoi
tessellation to the sky the lens actually sees rather than to the frame corners,
which on a fisheye are ground and housing. Kiruna returns a circle of radius
1368 px in a 2832 px frame. A rectilinear lens throws its horizon to infinity
and returns nothing, and the client falls back to the frame, which for that
camera is correct. The outline is cached per calibration and frame size because
reading the lens model shells out to h5dump.

Frontend only and already live: the boundary lines are twice as wide and the
frame view can be magnified. Until this binary is installed the tessellation
still runs to the frame corners on fisheye cameras.

## Commit attribution: human contributors only

- Do not add AI assistants, models, bots, or their vendors as Git authors, committers, or co-authors. This includes Claude, Anthropic, Codex, OpenAI, and similar tools. Do not add AI `Co-Authored-By` trailers or generated-by signatures to commit messages.
- Preserve the actual human author and any genuine human co-authors. Use the contributing human’s configured Git identity; never impersonate another collaborator.
- Inspect the final commit message before pushing, including trailers inserted automatically by tools. Disable automatic AI attribution in your tool settings. These rules apply to every GAIA branch and release commit.

2026-09-14: At Juha’s explicit request, main history is being rewritten to remove Claude/Anthropic co-author trailers only. Human identities, timestamps and file trees are preserved. The public-viewer release branch has no such trailers and is unchanged. Other checkouts must fetch and realign with rewritten origin/main before further pushes; preserve uncommitted work and do not merge the old history back. A private pre-rewrite Git bundle and old-to-new commit map are retained under /mnt/data/juha/gaia for recovery and interpretation of older deployment revision references.

### What the b8d2d86 install adds over the running build

`GET /api/sources/{id}/stars/nights` lists the observing nights a camera has,
local solar noon to noon, with frame and detection counts. The three star
endpoints take an explicit `from` and `to` beside the old `hours` and report
which window they answered with, so the photometry panel can show one night, or
a run of consecutive nights, instead of a trailing number of hours.

`stars/frame` marks each star `saturated` when its fitted background plus
amplitude reaches the top of the range. Bright aurora lifts the background until
stars have no headroom, and a clipped peak reports a flux that is an
underestimate, which must not be read as cloud. The panel draws those as crosses
rather than discs. On Kiruna's night of 14 September, 54 of 140 identified stars
were saturated at 00:47 with the background at 172, and one at 01:59 with it at
151.

Until this is installed the panel falls back to the trailing window and no star
is ever marked saturated.

## Returning to the known-good state (2026-09-15)

Tag `known-good-2026-09-15` marks the combination that was deployed and
working on 15 September. Everything needed to get back to it is on disk, so a
rollback needs no network and no rebuild:

    git checkout known-good-2026-09-15
    tar xzf /mnt/data/juha/gaia/staging/web-dist-9bd0b5a.tar.gz    # from the repo root
    # as j:
    cp /mnt/data/juha/gaia/staging/gaia-server-b8d2d86 /mnt/data/juha/gaia-build/release/gaia-server
    systemctl --user restart gaia-revontuli.service

`web-dist` is gitignored and served straight out of the repository, so the
frontend lives in that archive rather than in the commit. The backend binary in
staging is byte-identical to the one installed at 15:37 that day, sha256
`2846ab2bea063131bb75c08978f430e06b46df8cb3e28a837ea37568a1570c33`.

### Reading the live database without breaking it

Twice now the live API has been taken down by a read of `gaia.sqlite3` from
another account. In WAL mode a reader needs `-shm`, and the archive directory is
world-writable, so a read-only connection from `bgu001` creates `-shm` and `-wal`
owned by `bgu001`; the `j` server then cannot write them and every request
returns 500. Recovery is to delete those two files, which `j` or root must do,
after checking `-wal` is 0 bytes so nothing committed is lost.

`?immutable=1` avoids this, because it never touches the WAL at all, but it is
only safe on a file nothing is writing: used on the live database it yields a
copy that fails `PRAGMA quick_check`. There is no safe way to copy the live
database from another account. Take copies as `j`, or from a stopped service.

### What the c7a26cb install added, now in place

The three fixes in `df27305`, which matter most. The pending-frame query took
two minutes sixteen on the live archive and ran every cycle; it now takes
milliseconds. `run_cycle` no longer returns before the clear-sky fit when no
frame is pending. Refused fits are recorded in a new `extinction_attempts`
table and cameras are walked least-recently-tried first, so the pass can no
longer spend its whole budget re-deciding the same few camera-nights: measured
on a copy of the archive, 18 cameras in 75 seconds against ten in a day.

Frame stepping, a `step=next|prev` parameter on `stars/frame`, for the Prev and
Next buttons beside the zoom controls.

Until this is installed the clear-sky reference stays stalled at the fits it
had, and those two buttons return the nearest frame instead of the neighbour.

### What the c7c9fef install added, now in place

`GET /api/extinction`, read-only. It summarises the clear-sky reference across
the archive, or one camera with `?source=`: fits, cameras fitted, newest fit,
attempts, cameras attempted, how many were refused, the coefficient by colour
channel, and the most recent fits in full.

It exists so the pass can be checked without opening the database. Reading
`gaia.sqlite3` from an account other than `j` has taken the API down twice; this
removes the reason to do it.

    curl -s localhost:18765/api/extinction | python3 -m json.tool

A healthy pass refuses most camera-nights, so a large `attempts_refused` beside
a small `fits` is the expected shape, not a fault.

### What the a36b511 install adds over the running build

`GET /api/sources/{id}/stars/distribution?star=…` returns every detected
measurement of one star, in all four channels, as bare columns: flux, amplitude
and background, three numbers per measurement rather than the twenty-five a
sample row carries. The default window is the whole archive.

The star photometry histograms read it. They cover the whole archive rather than
the window on screen because a star at this latitude barely changes elevation,
so its air mass hardly varies night to night and its clear-sky level — what a
fading is measured against — is far better determined over months than hours.
The window still governs the time series, the frame view and the night picker.

Until this is installed the histogram tab has nothing to draw.

## 2026-09-17 playback/publication recovery
Observed full-day publication repeatedly exceeding its 3600-second timeout, leaving hourly holes despite available raw images. Rolling preparation now selects the same 120 historical epochs used by overview, retaining minute-resolution newest 20 minutes; raw acquisition/history unchanged. Incremental runs catch up from the preceding verified publication instead of only newest 20 minutes. Cloud grids are generated only on cache miss and their key includes actual fading samples, image dimensions and model settings. Shared browser compositor invalidates when cloud texture changes. Preserve 16-worker cap and all existing weights/access rules.

## Mandatory image-processing performance gate

- Do not implement or enable a new image-processing stage in the production pipeline before profiling its computational cost with a bounded prototype on representative production data. Do not assume that caching, parallelism, or a fast single-frame demonstration makes the full workload affordable.
- Processing must be WAY faster than real time: use at least 10x real-time throughput (processing elapsed time <= 10% of the observation interval represented), across the full active camera fleet, as the minimum acceptance target. More headroom is preferred. Live updates must also finish within the publication cadence; a fast 24-hour benchmark does not excuse delayed live frames.
- Profile cold-cache and warm-cache runs, the newest-frame incremental workload, a full 24-hour/calibration rebuild, and overlapping acquisition/publication under realistic server load. Measure wall time, CPU time, peak memory, database/I/O cost, camera/frame counts, concurrency and cache hit rate. Respect the existing aggregate 16-worker limit.
- Before integrating/deploying, record reproducible benchmark commands, input coverage, measured results and remaining headroom in deploy/HANDOFF.md (the shared agents.md). If the gate fails, optimize or redesign first; do not deploy it enabled and hope it catches up.
- New analysis must never starve acquisition, block timely live publication, repeatedly restart an unfinished backfill, or create playback gaps. Expensive optional work needs bounded queues, resumable progress and a safe fallback. Preserve scientific weighting, timestamps and camera identity while optimizing.

Recovery profiling: the live SQLite planner selected SCAN star_photometry for cloudweight::stars_for_frame despite the existing (source_id, observation_utc) index. A representative Popovo frame query took 631 ms unhinted versus 7 ms with INDEXED BY star_photometry_frame (same result); force that index for this per-frame query. This is a measured additional bottleneck, not merely the grid-cache issue. Initial rolling selection regression and 46 frontend tests passed; end-to-end recovery timing still being verified.

Live image projection/publication MUST NOT wait for cloud analysis. Consume an already available cloud field only for the matching camera, observation and calibration; otherwise use cloud factor 1 with the existing magnetic-zenith, horizon/mask, solar and quality weights. No star-photometry scan or cloud-grid computation belongs on the realtime path. Explicit GAIA_PREPARE_CLOUD_WEIGHT=1 is reserved for controlled offline preparation, never the normal publisher service. Do not enable it in production before the profiling gate is met.

Recovery measured: first corrected latest+24h full/open publish completed in 69.358 seconds (321.055 CPU seconds, 523.5 MB peak); subsequent incremental full/open publication completed in 6.825 seconds (20.623 CPU seconds). Public overview now has zero empty samples across all 120 epochs, 22-49 cameras per sample; Iceland restored to 114 samples. Original daily acquisition count was 98,239. Added async cloud-ready handoff in background star photometry and strict daylight guard; parallel fitting shares an aggregate 16-core systemd slice with publisher. Final parallel/ready-product tests and rollout pending.

Ready cloud benchmark (release, live DB read-only, output temporary under data root): cold 6 ms, warm 4 ms, with actual usable stars and a generated grid. Parallel star fitting on a real Ryazan image/catalogue (512 catalogue stars, 164 channel measurements) measured 409 ms at 1 worker versus 89 ms at 16; resulting measurements were byte-for-byte identical via Debug comparison. Reproduce with GAIA_PROFILE_DB=/mnt/data/juha/gaia/gaia.sqlite3 TMPDIR=/mnt/data/juha/gaia cargo test --release --bin gaia-server benchmark_ready_field -- --ignored --nocapture, and benchmark_parallel_frame similarly. All 129 non-ignored backend tests and 46 frontend tests passed before final daylight/ready-cache regression additions.

Final recovery deployment: main 75b1c2a installed on Revontuli; frontend release 7319e90 installed on juha.no and matching admin assets served on Revontuli. Final backend suite: 130 passed, 3 opt-in benchmarks ignored; 46 frontend tests passed. Both runtime services report Slice=gaia-compute.slice and that slice reports CPUQuotaPerSecUSec=16s. API health ok after restart, latest full/open publication 8.184 seconds. Background pass produced three cloud-ready descriptors; subsequent full manifest consumed all three, while anonymous manifest retained access filtering. No cloud analysis blocks live publication. Existing unrelated Apache edits/backups and docs/web-dist were left untouched.

## Browser brightness normalization
Shared GaiaGlobeView now exposes a 44px top-right gear and a native modal settings dialog on both shells. Normalize camera brightness is opt-in, persisted only in browser localStorage (gaia-normalize-images). A 32x32 sample of each decoded camera texture uses positive-weight mesh UV coverage to exclude masked/cropped regions; 95th-percentile maximum-channel brightness targets 180/255 with gain limited to 1..16. Black/empty images remain unchanged. One RGB gain preserves channel ratios, with hue-preserving highlight limiting, applied before all existing cloud/magnetic/solar-weighted blending; gains smooth over 150ms during playback. Original files, calibration, masks, acquisition and server weights unchanged. Samples are measured once when decoded, not each draw. Benchmark: 10,000 histogram calculations on 32x32 RGBA samples took 92ms on Revontuli; 53 frontend tests passed. This is a display aid, not photometric calibration; UI warns it may amplify noise.

## 2026-09-18 SpaceWeatherLive directory ingestion
Audited all 54 directory entries; sources/spaceweatherlive.json adds 25 configurations, of which 14 recent decodable feeds are enabled for one-minute acquisition and 11 are deliberately disabled (stale, unavailable, or operator/panorama review). Existing sources and three restricted Starvisor mappings are retained. The 23 unresolved video/provider entries are tracked individually in docs/spaceweatherlive-audit.json; do not claim them as live acquisitions. See docs/spaceweatherlive-import.md for operator evidence, exact coordinate policy and repeatable importer. New feeds require AIDA calibration; unknown station coordinates are null, not fabricated. Data stays in the existing Revontuli archive; normal publication/sync supplies juha.no after calibration. No new processing stages or separate crawler processes introduced. Sources are loaded at backend startup; restart the existing gaia-revontuli service once after configuration deployment. Browser normalization remains opt-in; a CSS-only dialog-centering follow-up accompanies this deployment.
Deployment verification: main 0f2438a pushed; public release e0527f3 installed through Git on juha.no, matching admin bundle installed on Revontuli. All 64 frontend/source tests passed. All 14 enabled new sources reported live with at least one stored image after the existing crawler startup staggering. Camera registry visibly shows the new sources, provider links, missing-location notices and AIDA links. Both live viewers load with no captured page errors; the settings dialog is now centered. Separate pre-existing KLUN projection error observed: cannot read calibration image_width; browser normalization does not repair invalid calibration metadata.

## 2026-09-18 Author affiliations and research purpose
Shared ServiceDescription states the goal of providing an overview of auroral activity around the world to support space physics research. Shared AuthorAffiliations identifies Juha Vierinen and Björn Gustavsson individually as Professor of Space Physics, University of Tromsø, in both public Info and admin About. This is author affiliation, not a change to camera ownership, permissions or the independent nature of GAIA.

## 2026-09-18 Fast camera-history and selected-image lookup
Deployed backend commit 20ab51b on Revontuli. Camera day queries previously scanned a whole source and sorted julianday(observation_utc); the live query plan now uses covering idx_images_source_history(source_id,julianday(observation_utc),id,observation_utc,width,height), preserving mixed UTC timestamp formats and exact exclusive day-end semantics. Frame listings run on spawn_blocking. Frame history, original/latest images, mask settings and calibration-copy settings use read-only connections: never run schema migration/INSERT OR IGNORE on these read paths. Startup retains migrations. No original images, metadata or observation timestamps changed; no image reprocessing required. Public juha.no is unaffected (static-file viewer; no history API forwarding).
Validation on the 571734-image database: pre-fix Sep 9 Kiruna listing took 12.736 seconds for 473 rows; one request timed out at 25 seconds even after indexing, before removing schema writes. After deployment the identical list was 98 ms first request then 5-10 ms; Sep 17 was 4-6 ms; a 179022-byte selected original was 4-5 ms. 24 concurrent mixed history/image/health requests all HTTP 200, 30-125 ms including client startup. Reproduce with timed HTTP GET /api/sources/tromsoe-ai-kiruna/frames?date=2026-09-09 and /api/images/c584fdb4-ac82-424b-964f-d4387ecd3ed4/original against localhost:18765. Full backend suite 132 passed, 3 benchmark tests ignored, including covering-index/no-sort, mixed timestamp/day/camera boundaries and read-during-writer-lock tests. Live browser opened Iceland history and decoded two consecutive selected full-size 3864x2192 images correctly. Existing unrelated Apache edits/backups and docs/web-dist remain untouched.

## 2026-09-18 Brekke location clarification
Juha identifies swl-yr-brekke as located in Ortneset village, Vestland, Norway. Registry and importer label updated to Ortneset (Brekke), Vestland, Norway. Exact camera coordinates remain null rather than substituting a village centroid; operator identification and calibration remain pending. Existing disabled state is unchanged. Live database name/config metadata updated without restarting acquisition.

## 2026-09-18 Catalogue startup contention follow-up
Deployed aa2b8a7: sources, history, status and credits GET handlers use read-only SQLite connections and spawn_blocking for synchronous database/filesystem work. Previous history fix missed these startup routes, allowing schema writes to stall catalogue loading. New regression holds BEGIN IMMEDIATE while all four startup handlers complete within two seconds. Full suite 133 passed, 3 ignored. Live browser catalogue resolved and Cameras showed 196 sources, with no captured page errors. Fifteen concurrent mixed startup requests all returned HTTP 200: sources 103-285 ms, history 270-333 ms, status 233-311 ms, credits 29-43 ms, health 24-27 ms. No frontend or public-viewer code changed; juha.no remains a static-file viewer without forwarding to this API.
