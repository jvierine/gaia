# Greenland source verification, 2026-09-10

Narsarsuaq (`dtu-narsarsuaq-allsky`) already exists but remains disabled: the JPEG retrieved on 10 September visibly says 2026-05-24 03:20 UT despite a current HTTP Last-Modified header. Do not treat that header or download time as a fresh observation.

Added THAAO / Thule (`thaao-thule-allsky`) as a **delayed daily preview**, not realtime coverage. Its public page at https://www.thuleatmos-it.it/data/skythule/index.php exposes a dated JPEG in an HTML img element; the current generic HTML crawler collects that preview only (currently 00:00 UTC on the previous day). Poll every six hours and deduplicate normally. The filename supplies UTC; no HTTP/download-time substitution is used. No darkness gate at download time: these are historical observations.

The page also lists a 20-minute sequence in JavaScript. That full sequence is NOT imported by this source; a dedicated sequence importer is future work. Verified a separate sample at 2026-09-09 04:00 UTC, 720x576 pixels. The camera is a sky/cloud imager, not a guaranteed aurora detection. It needs AIDA calibration before projection. Delayed frames must not masquerade as current aurora.

Provider rights and coordinates: https://www.thuleatmos-it.it/dataaccess/allskycamera/index.php lists THAAO_SKYCAM, CC-BY-4.0, ENEA/INGV ownership, 76.5 N, -68.8 E, 225 m, DOI https://doi.org/10.13127/thaao/skycam, and asks users to consult Daniela Meloni before scientific publication. Attribution is retained in the source record.

Daneborg ITACA is documented at https://space.fmi.fi/MIRACLE/ITACA/ but the provider directs users to request data and contact the PI before publication; no automated feed enabled. Greenland GMN status pages were found but no verified current single-exposure aurora feed was established in this check.
