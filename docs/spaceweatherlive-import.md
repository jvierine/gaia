# SpaceWeatherLive source import

Directory: https://www.spaceweatherlive.com/en/auroral-activity/webcams.html

The full 54-entry directory was audited on 2026-09-18. See
[the machine-readable audit](spaceweatherlive-audit.json) for every entry,
provider URL, existing GAIA match, probe result and outstanding adapter work.

25 new source configurations are in `sources/spaceweatherlive.json`.
14 returned decodable, recent still images and are enabled at a 60-second polling interval.
11 are disabled: 4 stale upstream snapshots, 2 unreachable/empty feeds, and 5
operator/location or panoramic-projection reviews. Existing IRF, NetNürds and
three restricted Starvisor entries are reused, not duplicated. The two Brekke
entries share one identical URL and therefore one configuration.
The remaining 23 directory entries need video-stream adapters or further
provider-endpoint research. They are NOT claimed as working image downloads.

New feeds are uncalibrated. Acquisition does not mean they can already be
projected onto the globe. Verify exact station coordinates (left null when
unknown), crop/mask and lens calibration in AIDA before projection. Do not copy
a calibration from a different instrument merely because it is at the same site.

Snapshot timestamps use the upstream Last-Modified stream-update time when
available, explicitly labelled as such; otherwise download-time fallback.
No source exposure time is invented. Unchanged images are deduplicated by the
existing crawler. Davis uses the current image discovered from its provider
page, not a hard-coded dated filename. Its image timestamp is a stream-update
time, not an assumed conversion of the displayed DAVT clock.

## Attribution evidence and corrections

- Hankasalmi and Nyrölä: Jyväskylän Sirius ry; Metsähovi: Aalto University.
  Exact coordinates and operator credits:
  https://rwc-finland.fmi.fi/index.php/all-sky-camera-images/
- Abisko: Fabian Wimmer Photography:
  https://fabianwimmer.com/abisko-allsky-webcam-live-northern-lights-weather/
- Porjus: Arctic Colors / Nature of Jokkmokk project:
  https://uk.jokkmokk.jp/
- AuroraMAX: University of Calgary, Canadian Space Agency, Astronomy North and
  City of Yellowknife:
  https://www.asc-csa.gc.ca/eng/multimedia/search/video/17779
- Linuxkidd: Michael J. Kidd; the provider's
  https://allsky.linuxkidd.com/config.js gives Animas, NM
  (32.117087, -108.923644), not the directory's obsolete Mayhill label.
- Davis: Australian Antarctic Division / Australian Antarctic Program:
  https://www.antarctica.gov.au/antarctic-operations/webcams/davis/
- Ettelsberg: Ettelsberg-Seilbahn / Foto-Webcam.eu:
  https://www.foto-webcam.eu/webcam/ettelsberg/

The hosting service is not silently presented as the copyright owner.
Scientific and commercial use requires the original operator's permission.
The importer preserves original provider links; SpaceWeatherLive is credited
as the discovery directory, not the camera producer.

## Repeat the audit

Run on the server from the repository:
`node tools/import-spaceweatherlive.mjs --write`

Without `--write` it only probes/reports. Four bounded concurrent requests,
15-second per-source timeouts, decoded-image validation and a 72-hour stale
check prevent broken HTML or very old snapshots being enabled as live cameras.
Review the generated diff before committing or restarting the crawler.
Existing database enable/disable choices and calibrations are preserved by
the normal source upsert; a rerun does not override an operator's choices.

Tests: `node --test tests/spaceweatherlive-sources.test.mjs`.
