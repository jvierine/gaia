# Norsk Meteornettverk cameras

GAIA uses the public https://norskmeteornettverk.no/data/ workflow: obtain a session and CSRF token, request one low-resolution still for UTC now minus two minutes, poll the task, and archive the returned JPEG. Generated JPEG links are not permanent latest-image feeds.

`norsk-meteor.json` enables Kristiansand camera 1 as a pilot. The source kind is `norsk_meteor`; the URL query identifies `station=ams123&camera=1`. Copy this entry with a unique ID and accurate producer/location metadata for another camera. Station identifiers and coordinates are listed by `https://norskmeteornettverk.no/data/index.php?action=get_stations`. Cameras are numbered 1 through 7. Do not automatically enable all cameras: some stations have metered or slow connections.

The pilot requests one still every five minutes, only when an approximate solar-altitude gate is below -4 degrees. There are no archive backfills or streams. Pending tasks are cancelled after 60 seconds. Empty results are reported as unavailable, not stored as images. Failed requests retry on the next scheduled interval, not immediately.

The filename supplies UTC time to minute precision (`source_filename` provenance); this is not an exact exposure timestamp. Download time is recorded separately. No external OCR service is used. Data follows the configured GAIA archive root on Revontuli, `/mnt/data/juha/gaia/YYYY-MM-DD`.

The camera initially has no calibration. Inspect it and calibrate through AIDA in the full GAIA cameras GUI before it can contribute to the globe. The upstream image is not cropped or geometrically modified by this adapter.

Credits and copyright remain attached to the source and archived image. Access does not imply a new redistribution license; resolve any additional provider conditions before broadening publication.

Tests: `cargo test norsk_meteor`; optional real single-frame integration test: `cargo test norsk_meteor::live_archive -- --ignored --nocapture`. This requests one historical frame and may fail after upstream archive retention expires.
