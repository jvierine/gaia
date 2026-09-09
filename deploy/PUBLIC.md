# Static public globe

Revontuli runs `gaia-server --publish`, then pushes prepared files by rsync to
`juha.no:/mnt/shovel/gaia/public`. No public API requests reach Revontuli.
Install `gaia-publish.service` and `.timer` as user units on Revontuli.

The manifest contains a rolling 24-hour sequence sampled every five minutes,
plus the most recent minute. Publication runs five minutes after the preceding
run finishes. Original ingestion retains its existing cadence and archive.
Textures are lossless transparent WebP, 4096×2048 geographic atlases, draped on
the existing WebGL 100 km shell. Existing crop/mask geometry is rasterized;
overlaps use a unit-normalized Laplacian magnetic-axis blend. For each camera,
IGRF-14 is evaluated at the ray's 100 km shell intersection and
`w=exp(-abs(theta_B)/S)`, with the axial angle folded into `[0,pi/2]` and
`S=20 degrees` by default. Magnetic weights are cached by geometry, IGRF year,
and falloff angle. No photometric equalization is introduced by this publisher
yet. Missing data stays transparent; camera observations must be within ten
minutes before frame time.

Build the public frontend with `VITE_GAIA_PUBLIC=1 npm run build:static`.
Deploy that build to `j@juha.no:/var/www/html/gaia/`; `/gaia/public/` is the
separate Apache alias backed by `/mnt/shovel/gaia/public/`. Retain the full admin
frontend in `/home/j/src/gaia/web-dist/` on Revontuli.
`apache-public.conf` serves prepared files and denies the old public API.
The public viewer provides globe interaction, history playback and credits only.

Assets are sent before atomic manifest replacement. Old generated textures
remain for roughly 49 hours to cover open viewers; rsync deletion is restricted
to `public/assets/`. Raw archives, calibrations and databases are never pruned
by this job. Source observations and contributing camera IDs are recorded per
frame; producer credits/copyright are retained in the manifest. Each contributor
also records the exact seasonal calibration ID used. Content-addressed original
AIDA/WISC HDF5 lens files, validity intervals, fit residuals, SHA-256 values,
camera coordinates, and machine-readable usage conventions are published in the
manifest's `lens_models` and `lens_model_documentation` fields.

Rollback: restore the saved juha.no Apache config and prior frontend build,
then restart its retained GAIA service. Do not enable two publisher instances.
