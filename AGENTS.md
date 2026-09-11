## Public/admin GAIA synchronization

- Read the current operational handoff at `/mnt/data/juha/gaia/agents.md` (linked to `deploy/HANDOFF.md` in this checkout). Update it after deployments or infrastructure changes so both collaborators remain synchronized.

- Keep the 3D globe and stitching implementation shared between `https://juha.no/gaia/` and `http://revontuli.uit.no/gaia/`. Both shells use `src/GaiaGlobeView.tsx`, `src/globe.ts`, and `src/camera-compositor.ts`. Revontuli prepares individual 256px textures and masked 100km meshes; FINAL normalized magnetic-weighted composition happens in the browser, by Juha's explicit request. No separate admin/public rendering algorithms.
- For every globe, stitching, station-marker, projection, playback, or attribution change, build and deploy both the admin and public frontends from the same Git commit.
- Before pushing, verify both live routes in a browser. A successful local build alone is not acceptance; confirm that the deployed asset hashes changed as expected and that both live canvases render without WebGL or page errors.

## Canonical checkout and GitHub

- Develop directly in the shared Revontuli checkout at `/mnt/data/juha/gaia/code`. Do not develop GAIA from a private home-directory copy.
- The canonical Git remote is `git@github.com:jvierine/gaia.git`. Commit coherent changes and push them to that GitHub repository; do not leave deployed code uncommitted or only on Revontuli.
- This checkout is shared by users `j` and `bgu001`. Preserve the directory's access-control entries and group-writable Git configuration so either collaborator can continue the work. Never copy or share either user's private SSH keys; each user authenticates to GitHub with their own authorized key.
- Before editing, run `git status --short` and preserve unrelated work. After editing, run the relevant tests, `git diff --check`, build both web targets when the viewer is affected, commit, push, and verify the live services.

### GitHub authentication for collaborators

- Do not place a GitHub token, PAT, password, private key, or credential-helper output in this repository, `AGENTS.md`, shell history, service environment, or a shared file. Do not reuse `j`'s private key or account token for `bgu001`.
- The preferred setup is to add Björn's GitHub account, `BjornG-son`, as a collaborator on `jvierine/gaia`, then register `/home/bgu001/.ssh/gaia_jvierine_ed25519.pub` on that GitHub account. This gives Björn attributable commits and pushes without any shared token.
- A write-enabled, repository-scoped GitHub deploy key is the fallback if collaborator access cannot be used. The same `/home/bgu001/.ssh/gaia_jvierine_ed25519.pub` may be registered on `jvierine/gaia`; never upload or inspect the private-key file.
- If a repository-scoped deploy key is used, configure only `bgu001`'s Git settings to select it for this checkout. The shared repository itself retains `origin = git@github.com:jvierine/gaia.git`, so pushes visibly target Juha's repository and `j` continues to use his own GitHub identity.
- Validate access without printing credentials: as `bgu001`, run `git ls-remote origin HEAD`, then push an ordinary reviewed commit. Never use commands that print credential-store contents or private-key material.

### Public GUI deployment as bgu001

- Björn's SSH account on `juha.no` is `bgu001`. His dedicated deployment key stays on Revontuli at `/home/bgu001/.ssh/gaia_juha_no_ed25519`; only its public key is installed on juha.no. This is separate from the GitHub key. Never copy or print the private key.
- From Revontuli as `bgu001`, connect without a password using `ssh -i /home/bgu001/.ssh/gaia_juha_no_ed25519 -o IdentitiesOnly=yes bgu001@juha.no`. The account has group write access to `/var/www/html/gaia` through `gaia-deploy`; deploying the static GUI needs no sudo or service restart.
- Develop and commit in `/mnt/data/juha/gaia/code`, coordinate builds with `j` because this is a shared checkout, and push to `jvierine/gaia`. Both users must preserve group write permissions on deployed files. Do not run simultaneous builds/deployments.
- Build the public bundle in a separate output directory so the live admin build is not overwritten:

```sh
cd /mnt/data/juha/gaia/code
umask 002
VITE_GAIA_PUBLIC=1 npm run build:static -- --outDir public-dist
ssh -i /home/bgu001/.ssh/gaia_juha_no_ed25519 -o IdentitiesOnly=yes bgu001@juha.no 'mkdir -p /home/bgu001/gaia-deploy-backups; cp -a /var/www/html/gaia/index.html /home/bgu001/gaia-deploy-backups/index-$(date -u +%Y%m%dT%H%M%SZ).html'
rsync -r --chmod=D2775,F664 --exclude=index.html -e 'ssh -i /home/bgu001/.ssh/gaia_juha_no_ed25519 -o IdentitiesOnly=yes' public-dist/ bgu001@juha.no:/var/www/html/gaia/
rsync -r --chmod=F664 -e 'ssh -i /home/bgu001/.ssh/gaia_juha_no_ed25519 -o IdentitiesOnly=yes' public-dist/index.html bgu001@juha.no:/var/www/html/gaia/index.pending
ssh -i /home/bgu001/.ssh/gaia_juha_no_ed25519 -o IdentitiesOnly=yes bgu001@juha.no 'mv /var/www/html/gaia/index.pending /var/www/html/gaia/index.html'
```

- Upload assets first and atomically replace the entry HTML last. Retain previous hashed assets for existing viewers and rollback; do not use `rsync --delete`. Backups of the entry HTML live in `/home/bgu001/gaia-deploy-backups` on juha.no.
- For shared-view changes, build the matching admin target with `npm run build:static` (without `VITE_GAIA_PUBLIC`) from the same commit; its `web-dist` directory is served directly on Revontuli. Verify both live pages, asset hashes, station interaction, playback, and WebGL errors.
- On juha.no `/gaia/public/` is DENIED: use anonymous `/gaia/open/` or Google-protected `/gaia/restricted/`. Prepared assets live under `/mnt/shovel/gaia/serving/{open,public}`. `/var/www/gaia-public`, `/var/www/gaia-open` and `/var/www/html/gaia` are compatibility symlinks into shovel. Both collaborators use their own SSH identities. One automated publisher runs as `j` on Revontuli; never enable a duplicate. Transfer immutable assets first and verify them before replacing manifests. If assets are externally deleted or restored, remove the audience receipt-token to force full verification.
- Authentication check: run the SSH command above with `-o BatchMode=yes` and `id`. Deployment permission check: create and remove a temporary file inside `/var/www/html/gaia` before uploading. Keys from another workstation must be authorized separately.

## Codebase map

- `src/GaiaGlobeView.tsx`: shared public/admin 3D view component. Both shells instantiate this file.
- `src/globe.ts`: WebGL globe, station geometry, public stitched-atlas textures, hover/click attribution, playback texture loading, geographic boundaries, and IGRF contours.
- `src/PublicViewer.tsx` and `src/public-viewer.css`: public `juha.no/gaia` shell, playback timeline, information, credits, and lens-model downloads.
- `app/page.tsx` and `app/globals.css`: Revontuli admin shell, camera registry, pause/delete controls, history browser, mask/crop editor, and shared viewer styling.
- `backend/projection.rs`: calibrated per-camera projection assets and the enabled-camera gate.
- `backend/publish.rs`: public 100 km atlas generation, IGRF magnetic-axis Laplacian stitching, attribution maps, manifest, credits, and lens-model publication.
- `backend/crawler.rs`, `backend/db.rs`, and `backend/schema.sql`: gentle acquisition workers, persistent source state, and SQLite schema.
- `sources/`: source catalogues and the consolidated camera-source research notes. Keep producer URLs, copyright, acknowledgement, timestamps, and cadence explicit.
- `deploy/`: Apache and systemd definitions plus public publication/synchronization scripts. Runtime data live under `/mnt/data/juha/gaia`; code lives only in `/mnt/data/juha/gaia/code`.
- `tests/` and Rust module tests: browser/playback checks and numerical tests. Use `/home/j/.cargo/bin/cargo test` on Revontuli when Cargo is not on `PATH`.

## Architecture invariants

- The backend is Rust; the frontend is TypeScript/React/WebGL. The shared browser compositor replaces server-side final atlas generation. Preserve ALL existing weighting rules and settings, including full IGRF, horizon and mask tapers, solar weight at the selected timeline epoch, quality exponent, nonpositive exclusion and sum(weight*RGB)/sum(weight). Do not substitute winner-takes-all image colour. Rust still prepares static per-vertex weights using the existing equations/cache.
- Public `juha.no` is a local-file viewer with a small Rust Google identity gateway. Revontuli pushes individual camera assets; never proxy requests back to Revontuli. Anonymous catalogues retain Starvisor station metadata, points and provider links, but exclude their projections and lens assets BEFORE publication. Restricted camera files require an authorized GAIA session on every request. Both audiences use the same WebGL compositor. Revontuli has no Google login by Juha's explicit choice. Public account details and sign-out belong inside Info; About contains provider credits, not lens-model downloads.
- On juha.no ALL GAIA payloads, runtime files, source backups and serving data belong under `/mnt/shovel/gaia/`; never recreate local `/mnt/gaia-public` storage or root-filesystem data. Small Apache/systemd integration configuration may remain under `/etc`.
- Paused or removed cameras do not contribute projected imagery, station markers, hover targets, public composites, or attribution maps.
- Preserve source observation timestamps. A download-time fallback must remain explicitly distinguishable from a source/instrument timestamp.
- Preserve camera-provider provenance and links. GAIA serves projected low-resolution views and lens models, not replacement copies of providers' original high-resolution imagery.
