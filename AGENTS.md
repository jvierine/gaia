## Public/admin GAIA synchronization

- Keep the 3D globe and stitching implementation shared between `https://juha.no/gaia/` and `http://revontuli.uit.no/gaia/`. Both shells must use `src/GaiaGlobeView.tsx`, `src/globe.ts`, and the same published magnetic-weighted composite; do not add a separate admin-only or public-only stitching/rendering path.
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

- The backend is Rust; the frontend is TypeScript/React/WebGL. Do not introduce a second stitching implementation in JavaScript.
- Public `juha.no` is a static local-file viewer. Revontuli creates and pushes its manifest and generated assets; the public browser must not proxy hidden API requests back to Revontuli.
- Paused or removed cameras do not contribute projected imagery, station markers, hover targets, public composites, or attribution maps.
- Preserve source observation timestamps. A download-time fallback must remain explicitly distinguishable from a source/instrument timestamp.
- Preserve camera-provider provenance and links. GAIA serves projected low-resolution views and lens models, not replacement copies of providers' original high-resolution imagery.
