# UCalgary realtime cameras

Registry snapshot: 2026-09-08. All 34 JPEG image streams listed by
https://api.phys.ucalgary.ca/api/v1/rt are included in `ucalgary.json`.
Arrays: TREx RGB (6), TREx NIR (5), TREx Blueline (5), REGO (2),
SMILE ASI (15), SMILE Auto (1).

Station coordinates come from the provider's observatory API,
`/api/v1/data_distribution/observatories?instrument_array=...`.
Rothney is not listed there; its observatory position is published at
https://cam01.sci.ucalgary.ca/RAOSkyWatch/ (50.868039 N, 114.291142 W).
Do not infer station altitude or lens calibration from these coordinates.

Streams are polled once per minute with `countdown_clock` left disabled.
The `x-rt-stream-last-updated-utc` response header is retained as the
`source_stream_update` timestamp basis. OCR is disabled; missing or invalid
headers fall back to download completion time. This is an upstream update timestamp, not a measured exposure
time. Original downloads and download completion times are retained.
Old overnight frames remain old; they are not presented as fresh observations.

Provider references:
- https://data.phys.ucalgary.ca/data/realtime.html
- https://api.phys.ucalgary.ca/docs
- https://data.phys.ucalgary.ca/data/how_to_cite.html

Dataset citations supplied by the provider API are included in every source's
acknowledgement. SMILE citations were marked TBD by the provider at import;
do not invent a DOI or license. Copyright remains with the originating teams.
The Cameras page links uncalibrated images to AIDA; adding a stream does not
make it eligible for scientific projection until calibration is supplied.

## Per-pixel calibration

UCalgary publishes seasonal IDL skymaps containing `FULL_AZIMUTH` and
`FULL_ELEVATION` for each image pixel. GAIA converts those authoritative
direction grids into ordinary AIDA/WISC HDF5 lens files with:

```bash
conda run -n base python tools/import-ucalgary-skymaps.py \
  --archive-root "$GAIA_ARCHIVE_ROOT" --db "$GAIA_DB_PATH"
```

The converter discovers only the newest dated skymap for each fixed camera,
downloads sequentially with an identifying user agent and a delay between
requests, and retains the exact upstream skymap URL in the HDF5 metadata. It
fits each supported AIDA radial model and keeps the best. Validation uses a
spatially interleaved set of pixels that was excluded from fitting; a lens is
rejected if its angular RMS exceeds 0.35 degrees or its 95th-percentile error
exceeds 0.75 degrees. Only directions at or above 5 degrees elevation enter
that test. The calibration's dated directory becomes `valid_from_utc`, and
GAIA selects the season valid at each image time rather than blindly using the
newest lens for historical frames.

Official calibration description and trees:

- https://data.phys.ucalgary.ca/sort_by_project/other/documentation/skymap_file_description.pdf
- https://data.phys.ucalgary.ca/sort_by_project/SMILE/asi/l0/skymaps/
- https://data.phys.ucalgary.ca/sort_by_project/TREx/RGB/skymaps/
- https://data.phys.ucalgary.ca/sort_by_project/TREx/NIR/skymaps/
- https://data.phys.ucalgary.ca/sort_by_project/TREx/blueline/skymaps/
- https://data.phys.ucalgary.ca/sort_by_project/GO-Canada/REGO/skymap/

THEMIS appears among the platform's instrument arrays, but the realtime stream
listing did not contain a THEMIS stream at this snapshot. None was fabricated.
