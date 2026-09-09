# Revontuli migration

Target: acquisition, original archive, SQLite metadata, calibration, and projection
on revontuli.uit.no. All durable GAIA data belongs in `/mnt/data/juha/gaia`.
The new viewer and lens calibrator are standalone at Revontuli's `/gaia/`
and `/aida/` routes. The old juha.no installation is retained separately.

There is no API forwarding, reverse tunnel, or connection initiated by juha.no
to Revontuli. No cross-host viewer dependencies or publication pipeline.

Install the backend user service unit in `~/.config/systemd/user/`. The backend
defaults to `GAIA_CRAWLER_ENABLED=0` for safe staging. Enable collection only
after stopping the old collector and taking the final consistent SQLite backup
and archive delta. Switch the public viewer to local published assets only after
historical projection and camera mask tests pass. Keep the old archive and
database for rollback; never copy an active SQLite database without its online
backup API. Rewrite absolute archive/calibration/mosaic paths transactionally in
the destination database, not the source. Projection cache hashes include paths;
expect geometry and textures to be regenerated at the new location.

Camera edits, suggestions and AIDA calibration uploads use the local Revontuli
backend. Existing AIDA test-case support uses its Node service; GAIA is Rust.

Set `GAIA_OCR_ENABLED=1` on Revontuli after installing Tesseract and English
language data. OCR is local, capped at two concurrent jobs and eight seconds
per job. Only explicit full-date UTC overlays are accepted; otherwise use
download completion time. Source-provided timestamps take precedence. Keep
OCR disabled on juha.no. No external OCR service.
