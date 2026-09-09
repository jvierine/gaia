# Norsk Meteornettverk cameras

GAIA uses the public https://norskmeteornettverk.no/data/ workflow: obtain a session and CSRF token, request one low-resolution still for UTC now minus two minutes, poll the task, and archive the returned JPEG. Generated JPEG links are not permanent latest-image feeds.

`norsk-meteor.json` enables all 98 cameras listed in the provider camera-FOV inventory across 14 stations (verified 2026-09-09), including the network’s Västerås station in Sweden. The source kind is `norsk_meteor`; the URL query identifies `station=ams123&camera=1`. Copy this entry with a unique ID and accurate producer/location metadata for another camera. Station identifiers and coordinates are listed by `https://norskmeteornettverk.no/data/index.php?action=get_stations`. Cameras are numbered 1 through 7. Kristiansand, Ørsta and Løten are marked quota-limited by the provider; their cameras use a fifteen-minute cadence.

Each camera requests one still every five minutes (fifteen minutes at quota-limited stations), only when an approximate solar-altitude gate is below -4 degrees. Requests are staggered across each interval, with at most two network image tasks in flight. There are no archive backfills or streams. Pending tasks are cancelled after 60 seconds. Empty results are reported as unavailable, not stored as images. Failed requests retry on the next scheduled interval, not immediately.

The filename supplies UTC time to minute precision (`source_filename` provenance); this is not an exact exposure timestamp. Download time is recorded separately. No external OCR service is used. Data follows the configured GAIA archive root on Revontuli, `/mnt/data/juha/gaia/YYYY-MM-DD`.

The cameras initially have no calibration. Provider FOV metadata identifies cameras but is not a full lens model. Inspect it and calibrate through AIDA in the full GAIA cameras GUI before it can contribute to the globe. The upstream image is not cropped or geometrically modified by this adapter.

Credits and copyright remain attached to the source and archived image. Access does not imply a new redistribution license; resolve any additional provider conditions before broadening publication.

Tests: `cargo test norsk_meteor`; optional real single-frame integration test: `cargo test norsk_meteor::live_archive -- --ignored --nocapture`. This requests one historical frame and may fail after upstream archive retention expires.

## Historical recovery

Run `gaia-server --backfill-nmn 2026-09-08` with the same `GAIA_SOURCES`, `GAIA_ARCHIVE_ROOT`, and `GAIA_DB_PATH` environment as the full backend. The date identifies the night starting that UTC date: samples cover noon to the following noon, filtered to local darkness. This recovers sampled stills, not continuous video. The regular 5-minute and metered 15-minute cadences apply.

Only one historical image request runs at a time, independently of the maximum two live image tasks. A one-second pause follows each request. Expect hours for a full network night. The backfill uses the ordinary image archive and preserves filename UTC and retrieval timestamps; it never writes images to the code checkout.

The SQLite `meteor_backfill_frames` table records each camera/time slot as pending, running, archived, duplicate, unavailable, or failed, with error details. Rerunning the same date resumes pending/interrupted/failed slots and skips completed ones. No unavailable image is fabricated. Inspect progress with `SELECT state,count(*) FROM meteor_backfill_frames WHERE night='2026-09-08' GROUP BY state;`. Do not run two backfills for the same date simultaneously.

Camera calibration and public globe playback are separate: archiving these images does not automatically calibrate them or add a historical date selector to the public viewer.
