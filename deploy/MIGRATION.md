# Revontuli migration

Target: acquisition, original archive, SQLite metadata, calibration, and projection
on revontuli.uit.no. All durable GAIA data belongs in `/mnt/data/juha/gaia`.
The public viewer stays at https://juha.no/gaia/.

There is no API forwarding, reverse tunnel, or connection initiated by juha.no
to Revontuli. Revontuli pushes prepared viewer files to juha.no; the public
viewer serves only local files and locally received submissions.

Install the backend user service unit in `~/.config/systemd/user/`. The backend
defaults to `GAIA_CRAWLER_ENABLED=0` for safe staging. Enable collection only
after stopping the old collector and taking the final consistent SQLite backup
and archive delta. Switch the public viewer to local published assets only after
historical projection and camera mask tests pass. Keep the old archive and
database for rollback; never copy an active SQLite database without its online
backup API. Rewrite absolute archive/calibration/mosaic paths transactionally in
the destination database, not the source. Projection cache hashes include paths;
expect geometry and textures to be regenerated at the new location.

Next phase: publish only 256-pixel textures, geometry, IGRF vectors, metadata
and bounded playback history to juha.no. Use immutable asset names, atomic
manifest publication, and a bounded retention policy. Originals and processing
stay on Revontuli. Camera edits, suggestions and AIDA uploads need an explicit
submission/import workflow: local receipt is not confirmation of backend
application. Do not create two independently writable camera databases or hide
a proxy behind the public service. The submission transport is not yet decided.

OCR remains disabled. Explicit source timestamps are retained; otherwise use
download completion time with `download_time` provenance. No external OCR service.
