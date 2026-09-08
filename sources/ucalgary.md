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
`source_stream_update` timestamp basis when no explicit UTC exposure overlay
can be read. This is an upstream update timestamp, not a measured exposure
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

THEMIS appears among the platform's instrument arrays, but the realtime stream
listing did not contain a THEMIS stream at this snapshot. None was fabricated.
