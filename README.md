# GAIA — Global Auroral Image Aggregator

GAIA is a UiT data center for near-real-time, attributed auroral imagery. It acquires public camera frames at a polite cadence, preserves the producer's copyright and requested acknowledgement, calibrates wide-field lenses through [AIDA/WISC](https://juha.no/aida/), and projects usable pixels onto a 100 km emission shell. Overlaps are blended with preference for the view nearest magnetic zenith, and each camera's contribution is tapered smoothly to zero between 75° and 85° zenith angle so horizon-grazing pixels drop out of the composite. Crop and obstruction outlines are feathered over 10 working-grid pixels; because each output pixel is normalized by its own summed weight, that feather only shows where images overlap. A whole image is additionally weighted by the solar elevation at its own station, from full weight below -12° down to 0.05 once the sun reaches the horizon, so twilight skies yield to genuinely dark neighbours.

Authors: Juha Vierinen and Björn Gustavsson, UiT The Arctic University of Norway.

The public viewer is at <https://juha.no/gaia/>.

## Architecture

- `backend/`: one Rust service for crawling, ingestion, SQLite metadata, calibration intake, source health, suggestions, projection, quality masks and mosaicking.
- `src/globe.ts`: custom TypeScript/WebGL globe. It draws political boundaries and coastlines, 30° full IGRF-14 magnetic dip-latitude contours, the magnetic and geographic equators, current Sun position and terminator. The Rust service evaluates all degree/order 1–13 coefficients and caches one grid per calendar year.
- `app/`: viewer, camera registry, data-flow status, contributor workflow and acknowledgements.
- `sources/core.json`: human-editable camera-source catalogue.
- `/mnt/shovel/gaia/YYYY-MM-DD/<source>/<image-id>/`: immutable original image and provenance metadata. SQLite lives on the server-local filesystem at `/home/j/var/gaia/gaia.sqlite3` because CIFS file locking is unsuitable for SQLite WAL; AIDA calibration HDF5 files live under `/mnt/shovel/gaia/calibrations/`.

The Rust service SHA-256 deduplicates downloads. It checks only a bounded number of newest archive entries and one frame from snapshot URLs per poll. Historical bulk acquisition is deliberately not enabled by default.

`sources/starvisor.json` contains the current STARVISOR Night Sky Patrol registry, including Popovo. Regenerate it from a saved STARVISOR index page with `node tools/import-starvisor.mjs starvisor.html`; review its producer and copyright fields before committing changes.

## Adding an image source

Add an object to `sources/core.json` and submit a pull request. Every source must name its producer and retain copyright:

```json
{
  "id": "example-camera",
  "name": "Example all-sky camera",
  "kind": "snapshot_url",
  "url": "https://example.org/allsky.jpg",
  "timestamp_mode": "download_time",
  "interval_seconds": 60,
  "producer": {
    "name": "Camera team or person",
    "institution": "Institution",
    "website": "https://example.org/camera",
    "acknowledgement": "Images courtesy of the Example camera team.",
    "copyright": "Copyright Example Institution",
    "license": null
  },
  "latitude_deg": 69.0,
  "longitude_deg": 20.0,
  "altitude_m": 100,
  "enabled": true
}
```

Use `timestamp_mode: "archive"` only when the observation time is trustworthy. Archive sources also provide `timestamp_regex` with a named `timestamp` capture and `timestamp_format`. **OCR is currently disabled:** no OCR subprocess or external OCR service is used. Snapshot images use download completion time (`download_time`), or a configured upstream update header (`source_stream_update`, not an exposure time). Explicit archive filenames retain `source_filename` timing. Download completion time is always retained separately; unchanged images keep their existing observation time. Historical OCR-derived timestamps remain unchanged and labeled with their original basis. A future OCR implementation must run on our server and preserve this provenance.

Enabled sources poll every 60 seconds on independent, staggered schedules, with four concurrent fetches maximum. The timeline requests archived images using `/api/sources/{id}/projection?at=<RFC3339 UTC>`. Each camera contributes its latest frame at or before that time, at most ten minutes old; missing history is left blank. Playback starts at the first available archived minute, skips empty periods, and waits for frame downloads before advancing to the next available minute. The globe and solar terminator use the same selected UTC. Raw image coordinates and lens parameters are unchanged by crop/mask operations. Each camera's obstruction mask can be switched off individually from the crop and mask editor; the outlines stay on record so it can be switched back on without redrawing them.

Supported source kinds are `snapshot_url`, `html_index`, `json_feed`, and `push`. Optional `image_link_regex` limits links found on an HTML page. URL templates accept `{YYYY}`, `{MM}`, `{DD}`, and `{HH}` in UTC.

## Calibration

The Cameras tab links to AIDA with `gaia=1`, the camera `source_id`, and a return URL. AIDA v0.3.62 automatically loads the latest archived frame and adds **Send calibration to GAIA**, which uploads the native calibration HDF5 to `/gaia/api/calibrations`. Fixed cameras can reuse a seasonally valid calibration. Mobile-phone images should be calibrated per image.

Camera locations may be entered numerically or adjusted by dragging a marker. GAIA stores these as catalogue overrides, leaving the submitted crawler JSON unchanged.

## Build and test

```bash
cargo test
npm install
npm run build:static
```

The production backend is a Rust binary. Vite is used only to compile browser TypeScript and CSS into static files; it is not a production service. The WebGL globe is custom GLSL and does not use Three.js.

## Data and acknowledgements

GAIA retains the original source URL, producer/institution, observation-time basis, retrieval time, SHA-256 checksum, copyright and acknowledgement with every image. Derived mosaics must expose all contributing producers. Inclusion in GAIA does not transfer ownership or supersede a producer's terms.

Initial source acknowledgements include IRF/Kiruna Atmospheric and Geophysical Observatory, Tromsø Geophysical Observatory and UNIS/Kjell Henriksen Observatory, the Polish Polar Station Hornsund and IGF PAS, Sodankylä Geophysical Observatory/University of Oulu, and the Tromsø AI project with its originating camera operators.
Production also installs `deploy/no-ocr.conf` at `/etc/systemd/system/gaia.service.d/no-ocr.conf`. This blocks the legacy OCR executables inside the service's mount namespace, including during recovery with an older binary. No host-wide executable is removed or modified.
