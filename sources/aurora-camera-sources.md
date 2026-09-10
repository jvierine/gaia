# GAIA auroral camera source catalogue

Research snapshot: **2026-09-09 UTC**. This catalogue is input to a future,
low-impact near-real-time crawler. It is not a statement that GAIA has permission
to archive or republish every reachable image.

## Non-negotiable ingestion rules

1. Prefer fixed, calibrated all-sky cameras. Use ordinary wide-angle webcams only
   to fill a documented geographic gap.
2. Preserve the original file unchanged. Store operator, station, source URL,
   acknowledgement text, rights URL, retrieval time, and all available image
   timestamps beside it.
3. Treat an exposure timestamp supplied by the instrument as authoritative. If it
   is unavailable, record GAIA's UTC receipt time as an explicitly inferred time;
   never present receipt time as exposure time.
4. A reachable URL is not permission. `publish_status: permission_required` means
   GAIA may monitor metadata/freshness, but must not archive or redisplay images
   until the owner agrees.
5. Poll gently: only during local darkness, at no faster than the operator's own
   public page, with stable jitter, conditional requests, deduplication, and
   exponential backoff. Identify GAIA honestly in the User-Agent and provide a
   contact URL/email. “Normal user” means ordinary browser-compatible HTTP
   behavior, not disguising or rotating identity.

See [crawler-contract.md](#gentle-crawler-contract) for the crawler-facing fields and
timing policy.

The repository already contains a provisional `core.json` and crawler. Its
integration gaps are recorded in [existing-config-audit.md](#audit-of-the-provisional-crawler-configuration);
the current configuration should not be treated as rights-cleared production
input.

## Recommended rollout

| Priority | Source | Region/value | Realtime candidates | Rights gate |
|---|---|---|---:|---|
| 1 | [UCalgary SRS](#university-of-calgary-space-remote-sensing-srs) | Alaska, Canada, continental US; excellent Arctic and subauroral spine | 23 image streams observed | Open scientific data; dataset-specific citation required |
| 1 | [StarVisor](#starvisor-night-sky-patrol) | Exceptional Russian/Siberian and Chukotka longitude coverage | 39 public fixed-camera views; start with 6 | Copyrighted; request archival permission |
| 1 | [BACC / TGO / FMI](#bacc-six-camera-network) | Svalbard, northern Fennoscandia, Andøya | 6 BACC quicklooks plus Skibotn | Public quicklooks; retain PI-specific credit and confirm bulk retention |
| 1 | [IRF Kiruna](#irf-kiruna) | Northern Sweden; high-resolution fixed camera | 1 | Scientific use possible; publication/redistribution requires permission |
| 2 | [UAF GI](#uaf-geophysical-institute-alaska) | Poker Flat and Toolik, Alaska | 2 seasonal feeds | Permission required |
| 2 | [NIPR/PsA/PWING](#nipr-psa-and-pwing-quicklooks) | Scandinavia, Svalbard; high cadence/multiple wavelengths | 5–7 plausible | Permission required; HTTP-only endpoints |
| 2 | [AllSkyKamera](#allskykamera-subauroral-fallback) | Dense European subauroral fallback; one Alaska station | Curate, do not crawl all cameras | Embedding with credit is documented; archive rights are camera-specific |
| 2 | [Community directories](#community-and-commercial-discovery-directories) | Greenland, Iceland, northern US/Canada, Europe | Discovery only; resolve original providers | Mixed owner-specific rights |
| 2 | [AllSky7](#allsky7-fireball-network) | Full-sky composite stations, chiefly subauroral Europe | Network live view at 15-minute cadence | Non-commercial reuse with owner and network credit |
| 3 | [OMTI / ERG](#omti-and-erg-archive) | Iceland, Canada, Russia/Far East, Japan | Archive, not verified realtime | Contact PI before publication/presentation |
| 3 | [Greenland and Iceland candidates](#greenland-and-iceland) | Important longitudinal gaps | Mostly archive/discovery | Mixed; permission required unless explicitly licensed |

## Coverage assessment

- **Strong:** Alaska and Canada through SMILE/TREx RGB; Svalbard and northern
  Fennoscandia through BACC, TGO, FMI, IRF, and NIPR.
- **Useful extreme-storm belt:** Iowa, New Hampshire, the northern US, the UK,
  Netherlands, Germany, and southern Scandinavia.
- **Material gaps:** live Greenland above Narsarsuaq, Iceland with confirmed
  archival rights, and Jan Mayen/Bjørnøya/Hopen. StarVisor greatly improves
  Siberia/Russian Far East visually, though its cameras are usually fixed
  north-facing wide-angle cameras rather than calibrated all-sky instruments.
- Multiple pages continue serving the last winter image throughout polar day.
  HTTP 200 therefore means only “endpoint exists”; freshness must be decided from
  the instrument timestamp, source header, and station darkness.

## Timestamp ranking

| Rank | `time_basis` | Meaning |
|---:|---|---|
| 1 | `instrument_utc` | UTC exposure time in machine metadata, API header, EXIF, filename, or documented overlay |
| 2 | `instrument_local` | Instrument time is present but timezone must be resolved from station documentation |
| 3 | `source_server_utc` | Source's update header/event time; useful proxy, not exposure time |
| 4 | `gaia_received_utc` | GAIA receipt time only; uncertainty must be stored and displayed |

Keep every candidate timestamp and the evidence used to parse it. Never overwrite
the original overlay or EXIF.

## Permission work queue

Before public launch, request written archival/redisplay permission from IRF,
UAF GI, NIPR/PsA/PWING, DTU Space for Narsarsuaq, Kárhóll, and the selected
AllSkyKamera owners. For BACC/FMI, describe the exact polling rate, retention,
public display, acknowledgement, and takedown mechanism and ask whether public
quicklook status covers this use. UCalgary should still be notified of a sustained
service even where the scientific dataset is openly available.

---

## Gentle crawler contract

This document is deliberately implementation-oriented. Values in the source
notes should later be transcribed into a versioned machine-readable catalogue
(HDF5 for accumulated data products; no CSV).

### Source record

```text
source_id                 stable GAIA identifier
operator                  institution or individual camera owner
network                   array/network name
station_name
latitude_deg / longitude_deg
coordinate_status         verified | conflicting | approximate
camera_geometry           fixed_all_sky | fixed_wide | stream_unknown
image_url or discovery_url
transport                 https | http | sse_https | websocket_https
cadence_seconds           nominal source cadence
poll_floor_seconds        GAIA must never poll faster than this
darkness_sun_altitude_deg do not fetch images above this threshold
timestamp_source          api_header | exif | filename | overlay | none
timestamp_timezone        UTC | named zone | unknown
rights_status             open_with_attribution | permission_required | unknown
archive_status            enabled | metadata_only | disabled
publish_status            enabled | permission_required | disabled
credit_line
rights_url / citation_url / dataset_doi
contact
notes
```

For each retrieved object store at least:

```text
source_id
retrieved_at_utc
exposure_time_utc                 nullable
exposure_time_original            verbatim value
time_basis                        ranked value from README
time_uncertainty_seconds           nullable
source_last_modified / source_etag nullable
source_url
sha256
media_type / byte_length
credit_line_snapshot
rights_snapshot_date
```

### Fetch policy

- Send an honest, stable identifier such as
  `GAIA-Aurora-Archive/0.1 (+https://<project>/crawler; contact=<email>)`.
- Keep one connection pool per origin and at most one in-flight request to an
  origin. Do not rotate agents, addresses, or query strings to evade controls.
- GAIA polls enabled cameras during both day and night, at their configured
  conservative cadence. Do not add a local solar-altitude gate. Some upstream
  instruments only publish at night; record their real availability, not fake frames.
- Add deterministic station jitter of 0–20% to prevent synchronized bursts while
  keeping repeatable schedules.
- Use `If-None-Match` and `If-Modified-Since` when supported. Hash content and
  discard byte-identical frames even if headers change.
- Honor `Retry-After`. On 429/503/timeouts, exponentially back off from 5 minutes
  to 6 hours. After repeated invalid images, reduce to one daily health probe.
- Validate MIME signature and decode before retention. A 200 response with an
  empty body, HTML error, daylight placeholder, or old winter frame is not data.
- Never fetch native 1–3 second feeds at full cadence unless the operator offers
  a documented bulk/API mechanism and explicitly approves that use. A 60–120
  second archive cadence is adequate for the initial global context product.
- Discovery/catalogue pages: at most every 6 hours for dynamic community maps and
  every 24 hours for static institutional pages. UCalgary's realtime stream list
  may be refreshed every 15 minutes.

### Timestamp extraction

Evaluate without discarding alternatives:

1. machine-readable instrument/API UTC time;
2. documented UTC EXIF or filename;
3. OCR of an image overlay, retaining text, confidence, and parser version;
4. documented local instrument time converted with the station's IANA timezone;
5. server update time;
6. GAIA receipt time.

Reject or quarantine frames whose best exposure time is implausibly in the
future, regresses unexpectedly, or is stale relative to the observing season.
Clock problems should be source metadata, not silently repaired. OMTI documents
historical delays exceeding an hour at several stations, which is why both raw
and interpreted time are required.

### Rights and acknowledgement

- Save the exact credit line alongside every frame, not only in a web-page
  footer. Public exports must carry the same attribution metadata.
- Preserve the unaltered original. Derived thumbnails, projections, OCR results,
  and contrast enhancements must say they are GAIA derivatives.
- `permission_required` and `unknown` sources run in `metadata_only` mode until
  written approval is recorded. Monitoring a timestamp or status page does not
  authorize copying the image.
- Support owner-requested corrections, attribution changes, and takedown without
  destroying provenance records.

### Initial safe defaults

| Source class | Image poll | Discovery poll | Initial mode |
|---|---:|---:|---|
| UCalgary realtime API | 60–120 s/feed in darkness | 15 min | archive after dataset citation metadata is attached |
| BACC/TGO/FMI public quicklook | 60 s | 24 h | metadata-only pending confirmation of bulk retention |
| IRF latest image | 60 s medium image; full image only when changed | 24 h | metadata-only pending publication permission |
| UAF SSE | operator-directed 60 s reconnect; fetch changed path only | connection follows page behavior | metadata-only pending permission |
| NIPR quicklooks | 60 s | 24 h | metadata-only pending permission |
| AllSkyKamera curated cameras | 120 s in darkness | 6 h | embed/link only; archive per owner permission |
| Archive-only datasets | no realtime poll | monthly availability check | disabled until backfill workflow exists |

---

## University of Calgary Space Remote Sensing (SRS)

**Recommendation:** make this the first production source. It provides the
cleanest machine interface, explicit timestamps, fixed all-sky instruments, and
the best North American Arctic-to-subauroral coverage.

### Interface

- [Realtime API documentation](https://data.phys.ucalgary.ca/data/realtime.html)
- Stream discovery: `GET https://api.phys.ucalgary.ca/api/v1/rt`
- Latest frame: `GET https://api.phys.ucalgary.ca/api/v1/rt/{stream_id}/latest`
- The latest-frame response supplies `x-rt-stream-last-updated-utc`. Store this as
  a source timestamp and inspect the image's embedded timestamp where available.
- No authentication is required. WebSocket delivery is available, but a
  dark-only 60–120 second poll is simpler and well below the native cadence.
- Refresh stream discovery every 15 minutes and tolerate streams appearing or
  disappearing seasonally.

Probe on 2026-09-08: `trexrgb_yknf_standard` carried the embedded UTC timestamp
`2026-09-08 10:58:06 UTC`, with source update header seven seconds later. Resolute
Bay streams returned valid images last updated in April. This confirms that the
crawler must test freshness and not equate HTTP 200 with a current observation.

### Current SMILE realtime streams

SMILE is a continent-scale colour RGB all-sky imager array replacing THEMIS.
The following streams were advertised by the realtime API in the research
snapshot.

| Stream ID | Station | Latitude | Longitude | Coverage note |
|---|---|---:|---:|---|
| `smileasi_fsmi_standard` | Fort Smith | 60.03 | -111.93 | auroral |
| `smileasi_gill_standard` | Gillam | 56.38 | -94.64 | auroral/subauroral |
| `smileasi_iowa_standard` | Iowa City | 41.66 | -91.53 | extreme-storm fallback |
| `smileasi_kapu_standard` | Kapuskasing | 49.39 | -82.32 | subauroral |
| `smileasi_klun_standard` | Kluane Lake | 61.03 | -138.41 | auroral |
| `smileasi_luck_standard` | Lucky Lake | 51.15 | -107.26 | subauroral |
| `smileasi_pfrr_standard` | Poker Flat | 65.13 | -147.49 | Alaska |
| `smileasi_pina_standard` | Pinawa | 50.26 | -95.87 | subauroral |
| `smileasi_rank_standard` | Rankin Inlet | 62.82 | -92.11 | auroral |
| `smileasi_rabb_standard` | Rabbit Lake | 58.23 | -103.68 | auroral |
| `smileasi_resu_standard` | Resolute Bay | 74.74 | -94.95 | high Arctic |
| `smileasi_sach_standard` | Sachs Harbour | 71.99 | -125.26 | high Arctic |
| `smileasi_talo_standard` | Taloyoak | 69.54 | -93.56 | high Arctic |
| `smileasi_tool_standard` | Toolik | 68.63 | -149.59 | Alaska Arctic |
| `smileasi_wash_standard` | Mount Washington, NH | 44.27 | -71.30 | extreme-storm fallback |

The observatory API also listed Athabasca (54.60, -113.64), but no corresponding
SMILE realtime image stream was advertised in this snapshot. Discover dynamically.

### Current TREx RGB realtime streams

[TREx RGB](https://data.phys.ucalgary.ca/data/datasets/trex_rgb.html) is true
colour, with a nominal three-second cadence and three-Hz burst mode. GAIA should
not mirror that native cadence through the public latest-image endpoint.

| Stream ID | Station | Latitude | Longitude |
|---|---|---:|---:|
| `trexrgb_fsmi_standard` | Fort Smith | 60.03 | -111.93 |
| `trexrgb_gill_standard` | Gillam | 56.38 | -94.64 |
| `trexrgb_luck_standard` | Lucky Lake | 51.15 | -107.26 |
| `trexrgb_pina_standard` | Pinawa | 50.26 | -95.87 |
| `trexrgb_rabb_standard` | Rabbit Lake | 58.23 | -103.68 |
| `trexrgb_yknf_standard` | Yellowknife | 62.52 | -114.31 |

### Current REGO realtime streams

[REGO](https://data.phys.ucalgary.ca/data/datasets/rego.html) uses narrowband
630 nm all-sky imagers. Only two realtime streams were advertised, although its
archive contains more stations.

| Stream ID | Station | Latitude | Longitude |
|---|---|---:|---:|
| `rego_gill_standard` | Gillam | 56.38 | -94.64 |
| `rego_resu_standard` | Resolute Bay | 74.74 | -94.95 |

Archive observatories include Athabasca, Fort Simpson, Fort Smith, Gillam,
Kaktovik, Lucky Lake, Longyearbyen, Rabbit Lake, Rankin Inlet, Resolute Bay,
Sachs Harbour, and Taloyoak. The API's Kaktovik province/country label appears
incorrect; coordinates 70.13, -143.62 place it in Alaska, so retain the source
metadata but flag the label for correction.

### THEMIS archive

[THEMIS ASI](https://data.phys.ucalgary.ca/data/datasets/themis_asi.html) provides
22 white-light cameras and valuable historical coverage, including stations not
present in the current realtime API. No THEMIS image streams were advertised by
`/api/v1/rt` in this snapshot, so treat it as a backfill source rather than poll
invented “latest” URLs.

### Rights, citation, and contacts

The [SRS citation guidance](https://data.phys.ucalgary.ca/data/how_to_cite.html)
says its datasets are openly and freely available while requiring the relevant
dataset citation and acknowledgement. Do not use a generic University of Calgary
credit where a dataset-specific DOI/acknowledgement is available.

| Dataset | DOI | Contact |
|---|---|---|
| SMILE ASI | [10.11575/QKVX-TH24](https://doi.org/10.11575/QKVX-TH24) | Emma Spanswick |
| TREx RGB | [10.11575/4P8E-1K65](https://doi.org/10.11575/4P8E-1K65) | Emma Spanswick |
| REGO | [10.11575/Z7X6-5C42](https://doi.org/10.11575/Z7X6-5C42) | Emma Spanswick / Eric Donovan |
| THEMIS ASI | use the citation shown on its dataset page | Eric Donovan |

Store DOI, dataset title, acknowledgement, and contact per frame. Confirm the
wording on the dataset page at ingest-catalogue release time because citation
instructions can change.

### Crawler record defaults

```text
operator: University of Calgary Space Remote Sensing
camera_geometry: fixed_all_sky
transport: https
poll_floor_seconds: 60
darkness_sun_altitude_deg: -6
timestamp_source: api_header_plus_overlay
timestamp_timezone: UTC
rights_status: open_with_attribution
archive_status: enabled_after_citation_metadata_test
publish_status: enabled_after_citation_metadata_test
```

---

## European Arctic fixed all-sky cameras

### BACC six-camera network

The [Boreal Aurora Camera Constellation (BACC) display](https://fox.phys.uit.no/ASC/BACC.html)
combines fixed colour all-sky quicklooks operated by UiT, UiO, UNIS/KHO, FMI,
and Andøya Space. Its [2025 network document](https://aurora.unis.no/doc/BACC_2025.pdf)
defines the public product as quicklooks and keograms; raw images and movies stay
under each principal investigator's control. It asks third parties to credit the
responsible PI and to offer coauthorship when data are vital to a major result.

All BACC frames inspected had visible UTC date/time, station, exposure, and/or
coordinates. The wrapper pages refresh every 15 seconds, but GAIA should poll each
selected quicklook no faster than once per 60 seconds and only in darkness.

| GAIA ID | Station | Lat | Lon | Current-image URL | Timestamp | Notes |
|---|---|---:|---:|---|---|---|
| `bacc-longyearbyen` | Longyearbyen | 78.1500 | 16.0400 | [Allsky.jpg](https://kho.unis.no/Quicklooks/ZWO/Allsky.jpg) | embedded UTC | High Arctic; seasonal frame may remain all summer |
| `bacc-nyalesund` | Ny-Ålesund | 78.9235 | 11.9099 | [NyAalesund.jpg](https://kho.unis.no/upload/NyAalesund.jpg) | embedded UTC | High Arctic; seasonal freshness check mandatory |
| `bacc-kevo` | Kevo | 69.7600 | 27.0100 | [Allsky_KEVO.jpg](https://space.fmi.fi/MIRACLE/ASC/ASC_keograms/tmp_KEV_keo/Allsky_KEVO.jpg) | embedded UTC | Probe returned HTTP 200 with an empty body; require decode validation |
| `bacc-muonio` | Muonio | 67.9583 | 23.6833 | [Allsky_MUONIO.jpg](https://space.fmi.fi/MIRACLE/ASC/ASC_keograms/tmp_MUO_keo/Allsky_MUONIO.jpg) | embedded UTC | Current overlay coordinates differ from BACC table; see below |
| `bacc-skibotn` | Skibotn | 69.34815 | 20.36331 | [BACC5.jpg](https://fox.phys.uit.no/ASC/BACC5.jpg) | embedded UTC | UiT colour camera |
| `bacc-andoya` | Andøya/ALOMAR | **unverified** | **unverified** | [Alomar.jpg](https://kho.unis.no/upload/Alomar.jpg) | embedded UTC | BACC PDF lists 63.3 N, 16.01 E, which is inconsistent with Andøya; do not geolocate from it yet |

Muonio's inspected overlay showed approximately 68.0289 N, 23.5631 E, whereas
the BACC PDF table gives 67.9583 N, 23.6833 E and FMI's older station page gives
68.02 N, 23.53 E. Set `coordinate_status: conflicting`; ask FMI which values apply
to the active camera before projecting the image.

The BACC page identifies Magnar Gullikstad Johnsen as responsible editor. Before
bulk retention, send the operators the intended 60-second maximum rate, dark-only
schedule, credit display, retention policy, and takedown mechanism.

### UiT Skibotn standalone camera

- Information: [TGO All Sky Camera](https://fox.phys.uit.no/ASC/ASC01.html)
- Direct image: `https://fox.phys.uit.no/ASC/Latest_ASC01.png`
- Fixed all-sky; the page says it updates once per minute when solar elevation is
  below -2°, with time in UT.
- This may be a separate product from BACC5. Prefer the colour BACC camera for the
  initial catalogue, but keep this as a resilient second instrument rather than
  treating byte differences as duplicates.
- The former Andøya `ASC02` feed is stale from 2020 and hidden on the index; leave
  it disabled.

```text
poll_floor_seconds: 60
darkness_sun_altitude_deg: -2
timestamp_source: overlay
timestamp_timezone: UTC
rights_status: permission_required_for_bulk_archive
```

### DTU Space Narsarsuaq

- Information: [Narsarsuaq All Sky Camera](https://fox.phys.uit.no/ASC/NAQ.html)
- Direct image: `https://fox.phys.uit.no/ASC/naq.jpg`
- Fixed SKY-i-30 all-sky camera at Narsarsuaq, Greenland, approximately 61.16 N,
  314.56 E (equivalent to -45.44° longitude).
- The inspected frame carried an explicit UT date/time; on 2026-09-08 it still
  showed `2026-05-24 03:20:00 UT`, an expected polar-summer stale frame.
- No camera-specific redistribution licence was located. DTU's magnetic-station
  policy must not be assumed to cover this optical camera. Keep metadata-only and
  seek explicit DTU Space permission for retention and public redisplay.

### IRF Kiruna

The [IRF all-sky page](https://www2.irf.se/Observatory/?link=All-sky_sp_camera)
describes a fixed Sony α7S with an 8 mm fisheye at 67.840719 N, 20.411124 E,
425 m. It refreshes once per minute and links an archive.

- Full frame: `https://www.irf.se/alis/allsky/krn/latest.jpeg`
- Medium frame: `https://www.irf.se/alis/allsky/krn/latest_medium.jpeg`
- Movie: `https://www.irf.se/alis/allsky/krn/latest_movie.mp4`
- Keogram: `https://www.irf.se/alis/allsky/krn/latest_nkeogram.gif`
- Archive/browser: [Firmament](https://firmament.irf.se/)

Probe on 2026-09-08: the full JPEG was 2832×2832 pixels, about 3.6 MB, and its
EXIF `DateTime` was one second before the HTTP Last-Modified value. IRF states
times are normally UTC. Probe the medium image once per minute and request the
full image only after a new timestamp/content identity is observed. The `latest`
URL advertised a long cache lifetime, so use a conditional/no-cache request at
the scheduled interval rather than adding rapid random cache-busters.

The [IRF/KAGO licence](https://www.irf.se/en/forskning/kago/license/) allows
scientific/nonprofit use under conditions, but redistribution requires downstream
acceptance and publication in any medium requires permission from the Head of
KAGO. Scientific publishers should contact the PI, cite appropriately, and offer
coauthorship where warranted. Contact: Urban Brändström,
`urban.brandstrom@irf.se`.

```text
camera_geometry: fixed_all_sky
poll_floor_seconds: 60
timestamp_source: exif_plus_http_header
timestamp_timezone: UTC
rights_status: permission_required_for_publication
archive_status: metadata_only_until_confirmed
publish_status: permission_required
```

### FMI MIRACLE context

The [MIRACLE camera locations](https://space.fmi.fi/MIRACLE/ASC/asc_locations.shtml)
cover Hankasalmi, Nyrölä, Muonio, Kilpisjärvi, Kevo, Abisko, Sodankylä,
Longyearbyen, Ny-Ålesund, and historical/partner high-Arctic sites. The public
BACC quicklooks for Kevo and Muonio are the safest realtime entry points; do not
guess current-image URLs for every historical station.

The [FMI rules of the road](https://space.fmi.fi/MIRACLE/ASC/asc_rules_of_the_road_00.html)
permit teaching and non-commercial scientific research, request consultation
with FMI, require the same conditions on further distribution, and say significant
use should prompt a coauthorship offer. Send publication references to the data
providers. Contact Kirsti Kauristie for FMI data; Sodankylä Geophysical Observatory
has separate conditions and contact Tero Raita.

The [MIRACLE availability page](https://space.fmi.fi/MIRACLE/ASC/asc_avail.html)
is useful for backfill, but its aurora-selected products can include moonlit
clouds and artificial lights. Keep that selection label as operator metadata,
not ground truth.

---

## Additional Arctic and subauroral networks

These sources improve longitude coverage or offer high-cadence/spectral products,
but require more permission work than UCalgary.

### UAF Geophysical Institute, Alaska

Modern fixed-camera pages:

- [Poker Flat](https://allsky.gi.alaska.edu/poker-flat), approximately 65.13 N,
  -147.49 E
- [Toolik Lake](https://allsky.gi.alaska.edu/toolik-lake), approximately 68.63 N,
  -149.59 E
- [Gakona](https://allsky.gi.alaska.edu/gakona), listed as incoming/not active

The public page JavaScript uses server-sent events (SSE):

```text
https://allsky.gi.alaska.edu/src/checkLive.php?cam=poker-flat
https://allsky.gi.alaska.edu/src/checkLive.php?cam=toolik-lake
https://allsky.gi.alaska.edu/src/checkLive.php?cam=gakona
```

The server advertises a 60-second retry. Follow that rather than adding a faster
poller: maintain one ordinary SSE connection per active camera and fetch the
relative image path only when it changes. Daylight probes returned stable
`poker-notdark.jpg`, `toolik-notdark.jpg`, and `coming-soon.jpg` placeholders;
these must never enter the aurora archive.

The pages credit Don Hampton and Jason Ahrns (`dhampton@alaska.edu`,
`mjahrns@alaska.edu`). No image reuse licence was found, so public accessibility
does not authorize archival or redisplay. Set both active sources to metadata-only
until UAF grants permission. The [legacy optics realtime/archive page](https://optics.gi.alaska.edu/optics/realtime/)
documents more historical sites, but should not be scraped without agreement.

### StarVisor Night Sky Patrol

**High-value recommendation.** [StarVisor](https://starvisor.net/) is a nonprofit
night-sky camera aggregator that is unusually valuable to GAIA because it fills
the otherwise severe Russian and Siberian longitude gap. The public page listed
39 fixed cameras from roughly 45–69° N and 173° W–113° E in the 2026-09-08
snapshot. They are generally fixed, north-facing, low-light wide-angle/IP cameras,
not calibrated 180° all-sky instruments. That is acceptable for a GAIA context
layer as long as `camera_geometry: fixed_wide` is recorded and the image is not
projected as a calibrated all-sky view.

StarVisor exposes stable original-image paths on
`https://pics.starvisor.net/galleries/orig/`. Its single-camera pages refresh the
image once per minute by adding a cache-busting query value, while the whole page
has a one-hour refresh. Match that behavior: discover the station list no more
than every six hours, poll only a small selected set once per 60–120 seconds
during local darkness, and never sweep all 39 images every minute.

Initial gap-filling set:

| GAIA candidate | Location | Approx. coordinates | UTC offset shown | Original image URL |
|---|---|---|---|---|
| `starvisor-murmansk` | Murmansk | 69 N, 33 E | +3 | `https://pics.starvisor.net/galleries/orig/cap_mur.jpg` |
| `starvisor-vorkuta` | Vorkuta | 67 N, 64 E | +3 | `https://pics.starvisor.net/galleries/orig/cap_vrk.jpg` |
| `starvisor-aykhal` | Aykhal | 65 N, 111 E | +9 | `https://pics.starvisor.net/galleries/orig/cap_ayk2.jpg` |
| `starvisor-provideniya` | Provideniya | 64 N, 173 W | +12 | `https://pics.starvisor.net/galleries/orig/cap_prv.jpg` |
| `starvisor-solovetsky` | Solovetsky | 65 N, 35 E | +3 | `https://pics.starvisor.net/galleries/orig/cap_slvn.jpg` |
| `starvisor-strezhevoy` | Strezhevoy | 61 N, 77 E | +7 | `https://pics.starvisor.net/galleries/orig/capture_str.jpg` |

Second-wave subauroral stations include Berezniki (59 N, 57 E), Perm (58 N,
56 E), Irbit (57 N, 63 E), Kamensk-Uralsky (56 N, 61 E), Yurga (55 N, 84 E),
and Bagdarin (54 N, 113 E). Coordinates on the public page are rounded to whole
degrees, so they are discovery coordinates, not calibration coordinates.

The filenames are stable rather than timestamped. Before enabling a station,
verify whether its overlay contains local time against the published UTC offset
and store both the verbatim overlay and converted UTC. Otherwise use
`gaia_received_utc`; a cache-busting query value is not an exposure timestamp.

The footer says © StarVisor and no redistribution licence was located.
[Joining instructions](https://starvisor.net/joinus/) say cameras upload interval
snapshots by FTP and give `mail@starvisor.net` and `starvisor.ural@gmail.com` as
contacts. Ask for a supported feed, exact coordinates/timezones, owner credit,
retention permission, and capture timestamps. Until then:

```text
camera_geometry: fixed_wide
poll_floor_seconds: 60
timestamp_source: overlay_if_verified_else_gaia_received_utc
rights_status: permission_required
archive_status: metadata_only
publish_status: permission_required
```

### Community and commercial discovery directories

[Live Aurora Cams](https://www.liveauroracams.com/webcams/) maintains a large
worldwide directory with provider names and UTC update-status fields. It surfaced
several useful cameras that an institution-only search missed:

| Candidate | Region/value | Geometry | Acquisition recommendation |
|---|---|---|---|
| Chik-Wauk Dark Sky Cam | Grand Marais/Gunflint Trail, Minnesota | fixed all-sky | One-minute page; request permission from museum/UMD partnership |
| AuroraMAX | Yellowknife, NWT | fixed colour 180° all-sky | Six-second native feed; seek supported still/API or UCalgary route |
| Isle Royale / Mott Island | Lake Superior, Michigan | fixed park webcams | Extreme-storm coverage; contact US National Park Service |
| Porjus | northern Sweden | fixed wide view | Resolve Nature of Jokkmokk original source and terms |
| Gällivare/Dundret | northern Sweden | fixed wide tourism/weather view | Resolve resort owner and stable still image |
| Kangerlussuaq, Ilulissat, Kulusuk, Qaarsut | Greenland | fixed airport video views | Important gap; contact Greenland Airports rather than scraping YouTube |
| Northumberland Astro | northern England | fixed streams | Extreme-storm fallback; contact owner for still frames |
| Banff / Blue Mountains | Canada | fixed weather/resort cameras | Subauroral/extreme-storm fallback |

Treat the directory as discovery metadata, not proof that a source is fresh or
licensed. Check it at most daily and retrieve from the original owner rather than
copying aggregator thumbnails.

The [Chik-Wauk page](https://gunflinthistory.org/dark-sky-cam/) says its fixed
camera captures aurora and Milky Way images and asks viewers to refresh about once
a minute. It names the museum, historical society, and University of Minnesota
Duluth/Alworth Planetarium partnership and provides `info@gunflinthistory.org`.

[AuroraMAX](https://auroramax.com/live) documents a fixed 180° colour all-sky
camera at 62°26′ N, 114°21′ W, four-second exposure, with images about every six
seconds during dark seasons. GAIA needs only one frame per minute or two and
should request a supported still-image route rather than reverse-engineering its
JavaScript stream.

[Aurora Hunter](https://www.aurorahunter.it/en/webcam) is another discovery index
for Norway, Sweden, Finland, Iceland, Greenland, Alaska, and Canada.
[Live Aurora Network](https://liveauroranetwork.com/about-us/) operates fixed HD
video cameras at dark sites in Iceland, Norway, and Alaska. Because it is an
app/subscription product, pursue a partnership rather than scraping its streams.

[AllSkyCam.com](https://www.allskycam.com/) is a long-running community directory
where owners publish current full-sky images. It had six active uploaders in the
snapshot and no major Arctic coverage, but it is worth checking monthly for new
northern stations. No blanket reuse licence was found.

### AllSky7 Fireball Network

[AllSky7](https://www.allsky7.net/) uses seven fixed cameras per station to cover
the whole sky to the horizon. Its live view updates every 15 minutes. Although
optimized for meteors, it is a strong subauroral extreme-storm network with much
clearer reuse rules than most community camera sites.

The network permits scientific analysis, distribution to research facilities and
other non-commercial parties, and non-commercial online/media distribution. The
owner retains copyright. Every use must name the AllSky7 Fireball Network, camera
owner, and copyright; scientific work has a prescribed acknowledgement.

Discover station status daily, sample no faster than 15 minutes, and retain exact
owner attribution. Confirm that GAIA's persistent public archive fits the terms
before bulk collection. Contacts: `support@allsky7.groups.io` and Mike Hankey,
`mike.hankey@gmail.com`.

### NIPR PsA and PWING quicklooks

NIPR's [realtime optical summary](http://pc115.seg20.nipr.ac.jp/www/opt/realtime.html)
aggregates fixed Watec all-sky cameras installed with Scandinavian partners. The
page refreshes every 30 seconds; a 60-second conditional poll per selected camera
is sufficient for GAIA. Several endpoints are HTTP-only, so verify signatures and
hashes and never send secrets.

| Station | Approx. latitude | Direct quicklook | Snapshot condition |
|---|---:|---|---|
| Tromsø | 69.6 | `http://esr.nipr.ac.jp/www/optical/watec/tro/rt/TRO/wat_tro_rt1.jpg` | current on 2026-09-08 probe |
| Sodankylä | 67.4 | `http://pc115.seg20.nipr.ac.jp/www/optical/watec/tro/rt/SOD/wat_sod_rt.jpg` | endpoint plausible; continuously revalidate |
| Kiruna | 67.8 | `http://polaris.nipr.ac.jp/~kstdev/opt/KRN/wat_krn_rt.jpg` | six days stale on probe |
| Tjautjas | 67.3 | `http://polaris.nipr.ac.jp/~kstdev/opt/TJA/wat_tja_rt.jpg` | retained April winter frame on probe |
| Abisko | 68.36 | `http://polaris.nipr.ac.jp/~kstdev/opt/ABS/wat_abs_rt.jpg` | current on probe; new installation listed Sept. 2026 |
| Skibotn | 69.35 | `http://esr.nipr.ac.jp/www/optical/watec/skb/rt/wat_skb_rt1.jpg` | current on probe |
| Longyearbyen | 78.15 | `http://pc115.seg20.nipr.ac.jp/www/optical/watec/lyr/rt/wat_lyr_rt10.jpg` | seasonal; revalidate |
| Kilpisjärvi | 69.05 | `http://polaris.nipr.ac.jp/~kstdev/opt/KIL/wat_kil_rt1.jpg` | page notes internet ended in 2019; disabled |

Some sites expose multiple wavelengths and cadence as high as 2 Hz. Do not crawl
every variant. Select one colour/context product per station initially and obtain
a supported bulk route before attempting science cadence.

The aggregator gives Yasunobu Ogawa (`yogawa@nipr.ac.jp`) as contact. No reuse
licence was found and NIPR's general copyright notice reserves rights. Keep all
images metadata-only until NIPR and the relevant local operator approve retention
and publication. The [ERG ground-data page](https://ergsc.isee.nagoya-u.ac.jp/data_info/ground.shtml.en)
offers DOI-backed CDF products for later scientific backfill, not an excuse to
copy the realtime quicklooks.

### AllSkyKamera subauroral fallback

[AllSkyKamera](https://allskykamera.space/docs/docs.php?lang=en) is a collaborative
network of fixed all-sky cameras with current images and automated archives. Its
[map-data page](https://allskykamera.space/mapdata.php?lang=en) embeds a camera
array containing ID, location, latitude, longitude, online state, last-image
string, and proxy URL. The snapshot contained about 94 cameras in 14 countries.

Do not poll all cameras. Refresh the map every six hours, then select a small,
longitudinally useful subset. Initial high-latitude/subauroral candidates were:

| ID | Location | Lat | Lon | State in 2026-09-08 snapshot |
|---|---|---:|---:|---|
| `ASK046` | Eagle River, Alaska | 61.0000 | -149.0000 | offline/seasonal |
| `ASK045` | Landskrona, Sweden | 55.8703 | 12.8301 | offline |
| `ASK067` | HAW Kiel, Germany | 54.3342 | 10.1815 | online |
| `ASK080` | Penrith, Cumbria, UK | 54.6500 | -2.7600 | offline recently |
| `ASK075` | Llandegfan, Wales | 53.2450 | -4.1450 | online |
| `ASK081` | Anglesey, Wales | 53.2346 | -4.3513 | online |
| `ASK049` | Genemuiden, Netherlands | 52.6220 | 6.0340 | online |
| `ASK068` | Pymoor, UK | 52.3990 | 0.2620 | online |
| `ASK097` | Schönow/Bernau, Germany | 52.6728 | 13.5309 | online |
| `ASK060` | Newberg, Oregon | 45.3000 | -122.9600 | online; extreme-storm only |
| `ASK096` | Branched Oak, Nebraska | 40.9500 | -96.8500 | online; extreme-storm only |

Image template:
`https://allskykamera.space/imageProxy.php?cam={CAMERA_ID}&type=image`.
Poll a curated online camera at most every 120 seconds in darkness. The map's
`lastImage` timezone was not documented; use it for status only. Prefer a verified
timestamp within the image, otherwise record GAIA receipt UTC with uncertainty.

The [FAQ](https://www.allskykamera.space/faq.php?lang=en) allows non-owners to
embed a camera with visible credit, camera ID/location, and a link back. The
[content licence](https://allskykamera.space/content_license.php?lang=en) makes
clear that permissions are owner-specific. Embedding is therefore not blanket
permission for GAIA to copy and retain images. Ask the network contact
`gottie@web.de` and each selected owner for archival/redistribution permission.

### OMTI and ERG archive

The [OMTI network](https://stdb2.isee.nagoya-u.ac.jp/omti/index.html) is valuable
for otherwise weak longitudes: Husafell, Iceland; Magadan and Paratunka in the
Russian Far East; Japanese sites; and Canadian Arctic/subauroral stations. The
[data viewer](https://stdb2.isee.nagoya-u.ac.jp/omti/data/data.html) and
[ERG CDF tree](https://ergsc.isee.nagoya-u.ac.jp/data/ergsc/ground/camera/omti/asi/)
are archives, not verified low-impact latest-image feeds.

The [OMTI notes](https://stdb2.isee.nagoya-u.ac.jp/omti/notes.html) say browsing
plots are not necessarily fully calibrated and ask users to contact PI Kazuo
Shiokawa before publication or presentation. Digital-data access may be limited
by the PI. Keep OMTI disabled for realtime crawling and pursue a supported
backfill agreement instead.

OMTI also documents a serious historical clock problem: filenames at PTK, MGD,
ERK, KAP, NYR, GAK, IST, and HUS could lag actual acquisition by more than an
hour before December 2017. Preserve filename time and corrected time separately.
Husafell's dataset DOI is
[10.34515/DATA.GND-0006-0015-0121_v01](https://doi.org/10.34515/DATA.GND-0006-0015-0121_v01).

### Greenland and Iceland

#### Greenland

- **Narsarsuaq:** the DTU/TGO fixed camera is documented in
  [this catalogue](#dtu-space-narsarsuaq). Its direct 1200×1200 JPEG remains
  reachable and the operator page documents one-minute updates with an explicit
  UT overlay. A live probe on 2026-09-09 found a server `Last-Modified` value
  from that day but pixels stamped `2026-05-24 03:20:00 UT`; HTTP freshness is
  therefore unsafe for this feed. It is present in `iceland-greenland.json` but
  disabled until the pixels resume updating and camera-specific retention and
  redisplay permission is confirmed.
- **Pituffik/Thule:** the [THAAO sky-camera archive](https://www.thuleatmos-it.it/dataaccess/allskycamera/index.php)
  at 76.5 N, -68.8 E has DOI
  [10.13127/thaao/skycam](https://doi.org/10.13127/thaao/skycam) and CC BY 4.0
  licensing. It is a strong licensed backfill source, but download was temporarily
  locked and no realtime endpoint was verified. Owners are ENEA/INGV; PI Daniela
  Meloni asks to be contacted for scientific publication.
- UCalgary THEMIS has historical Greenland coverage. No verified high-Arctic
  Greenland realtime source with clear archival terms was found.

#### Iceland

- [China-Iceland Arctic Observatory, Kárhóll](https://karholl.arcticportal.org/en/)
  advertises a live fixed all-sky camera, linked to an HTTP IP address. Endpoint
  was unreachable on 2026-09-09; timestamp semantics and reuse rights remain
  unverified. Contact `info@karholl.is` before enabling.
- OMTI Husafell supplies DOI-backed archive coverage but is not a verified
  realtime image feed and requires PI consultation.
- [NetNURDS Aðaldalshraun](https://www.netnurds.com/) is a private fixed all-sky
  live/timelapse camera at the coordinates printed on its frame, 65.9 N,
  17.4 W. The stable JPEG is `https://netnurds.com/indi-allsky/image.jpg`.
  It contains a `YYYY.MM.DD HH:MM:SS` overlay, and Iceland uses UTC year-round;
  its `Last-Modified` header tracked the overlay within seconds in a 2026-09-09
  probe. GAIA polls it no faster than every five minutes, during day and night. Copyright and acknowledgement are retained in the
  source record. Contact `tim@netnurds.com`; no archival licence was found.
- Iceland-at-Night advertises fixed cameras near Hella and Arnarstapi, but its
  proprietary streams and absent reuse terms make it a permission/discovery lead,
  not an immediate crawler source.

Greenland, Iceland, and the Russian/Siberian sector remain the principal coverage
gaps. Institutional agreements or contributed GAIA cameras are preferable to
aggressive scraping of fragile or ambiguous feeds.

---

## Audit of the provisional crawler configuration

Snapshot: **2026-09-08**. This is a read-only review of the existing untracked
`sources/core.json`, `backend/model.rs`, and `backend/crawler.rs`. No crawler code
or JSON was changed during the source-research task.

### Blocking issues before a real crawl

1. Every configured source is `enabled: true`, including sources whose licence is
   null and whose publication/archival permission has not been established. Add
   distinct metadata-monitor, archive, and publish gates; `enabled` alone cannot
   express the required rights state.
2. `interval_seconds` is present in JSON but is not consulted by `run_loop`. The
   loop traverses all enabled sources and then sleeps 30 seconds. This can poll a
   nominal 60-second source nearly twice as fast as declared.
3. There is no darkness scheduler, deterministic jitter, per-origin concurrency
   limit, conditional GET, 429 `Retry-After` handling, or exponential backoff.
4. The User-Agent impersonates a Firefox browser version while `From: gaia@juha.no`
   identifies the project. Replace it with an honest GAIA agent/contact string.
   Browser compatibility is desirable; identity camouflage is not.
5. Snapshot sources marked `download_time` label crawler receipt as the observation
   time, even when the image carries an instrument timestamp. Add EXIF, filename,
   API, and overlay timestamp modes plus separate receipt/exposure fields.
6. Content type and a 1024-byte floor are checked, but an HTML error labelled as
   JPEG, a corrupt file, a daylight placeholder, and a stale winter image can
   still be archived. Decode images and apply placeholder/freshness tests.
7. HTML indexes fetch up to three files every pass. On startup this is bounded,
   but without a persistent high-water mark and conditional requests it still
   revisits the same objects repeatedly.
8. The configuration has producer acknowledgement and copyright strings, but no
   rights URL, DOI, permission record, credit version, or takedown state.

### Existing source entries

| ID | Technical assessment | Timestamp assessment | Rights assessment | Recommendation |
|---|---|---|---|---|
| `irf-kiruna-alis` | Timestamped hourly archive is preferable to `latest.jpeg` | Filename timestamp in UTC is strong | IRF publication/redistribution conditions apply | Keep disabled until permission record exists, then use high-water mark |
| `uit-skibotn-bacc5` | Reachable fixed colour BACC quicklook | Do not use download time; frame has embedded UTC | Public quicklook, but bulk retention should be confirmed | Metadata-only; implement overlay OCR or operator feed |
| `hornsund-allsky` | Promising fixed Svalbard archive with timestamped filenames | Filename appears strong; timezone must be documented | No verified reuse terms in this research pass | Metadata-only; contact IGF PAS and verify archive parser |
| `unis-kho-allsky` | Reachable fixed Longyearbyen image | Current JSON uses download time; prefer embedded time/API metadata | Licence null; BACC/UNIS conditions need confirmation | Prefer official KHO/BACC endpoint and metadata-only until approved |
| `sgo-sodankyla` | Official real-time Sky-I image, updated about once/minute | SGO documents UTC; parse instrument time, not download time | Public presentation prohibited without permission | Disable archival/publication until SGO approval |
| `tromsoe-ai-tromso` | Reachable latest image; project/instrument identity needs exact confirmation | HTTP update time exists; image/instrument time should be parsed | No verified redistribution permission | Metadata-only and contact project/operator |
| `tromsoe-ai-kiruna` | Reachable but duplicates IRF geography and may be a derivative | Do not substitute download time | IRF/originating-camera terms still apply | Prefer direct IRF source after permission |
| `tromsoe-ai-skibotn` | Reachable; likely overlaps Skibotn coverage | Do not substitute download time | Originating BACC/operator terms still apply | Prefer direct BACC source after permission |

### Source-specific evidence added during audit

#### Sodankylä SGO

The official [SGO realtime page](https://www.sgo.fi/Data/RealTime/allsky.php)
says the SOD Sky-I image updates once a minute and measurements are in UTC. The
[SGO optical-data page](https://www.sgo.fi/Data/Optical/allsky.php) explicitly
prohibits commercial use and public presentation of all-sky material without
permission. Its quicklook archive asks scientific users to contact SGO to decide
whether acknowledgement or coauthorship is appropriate. Therefore this source
must not be production-enabled merely because `SOD.jpg` is publicly reachable.

#### Tromsø AI

A recent project paper describes a fixed Sony α6400/6.5 mm fisheye at Skibotn,
normally one-minute cadence and 15 seconds after aurora detection, operating below
-13° solar elevation. The public `latest*.jpg` endpoints were reachable, but the
three configured endpoints may represent different originating instruments and
no blanket redistribution licence was found. Store the exact originating camera
and operator, not only “Tromsø AI,” before enabling any of them.

#### Hornsund

Timestamped names such as `allsky-YYYYMMDD-HHMMSS.jpg` are preferable to receipt
time and Hornsund fills an important Svalbard view near 77.0 N, 15.54 E. However,
the current entry points the generic HTML parser at a presentation page. Verify
that it exposes a stable, bounded current-day index, determine the timestamp's
timezone, and obtain Institute of Geophysics PAS permission before ingestion.

### Minimum schema/code changes implied by the catalogue

This is not an implementation request, but the future crawler needs:

```text
rights_status + permission_reference
archive_enabled + publish_enabled
instrument_timestamp + receipt_timestamp + time_basis + uncertainty
darkness threshold and operating season
poll floor, discovery interval, per-origin rate limit, jitter
ETag/Last-Modified/high-water mark
placeholder/staleness/decode validation
DOI, rights URL, exact credit line, contact, credit version
```

Until those fields are enforced, set rights-unknown sources to disabled and use
manual metadata probes only.
