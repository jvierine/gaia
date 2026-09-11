CREATE TABLE IF NOT EXISTS producers(
  id TEXT PRIMARY KEY, name TEXT NOT NULL, institution TEXT, website TEXT,
  acknowledgement TEXT NOT NULL, copyright TEXT NOT NULL, license TEXT
);
CREATE TABLE IF NOT EXISTS sources(
  id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, url TEXT NOT NULL,
  timestamp_mode TEXT NOT NULL CHECK(timestamp_mode IN ('archive','downloadtime','download_time')),
  interval_seconds INTEGER NOT NULL, producer_id TEXT NOT NULL REFERENCES producers(id),
  latitude_deg REAL, longitude_deg REAL, altitude_m REAL, enabled INTEGER NOT NULL DEFAULT 1,
  config_json TEXT NOT NULL, last_attempt_utc TEXT, last_success_utc TEXT, last_error TEXT
);
CREATE TABLE IF NOT EXISTS images(
  id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES sources(id), observation_utc TEXT NOT NULL,
  downloaded_utc TEXT NOT NULL, timestamp_basis TEXT NOT NULL, archive_path TEXT NOT NULL UNIQUE,
  original_url TEXT, sha256 TEXT NOT NULL UNIQUE, media_type TEXT NOT NULL, width INTEGER, height INTEGER,
  calibration_id TEXT, cloud_fraction REAL, obstruction_fraction REAL, usable_fraction REAL,
  processing_state TEXT NOT NULL DEFAULT 'received', error TEXT
);
CREATE TABLE IF NOT EXISTS calibrations(
  id TEXT PRIMARY KEY, source_id TEXT REFERENCES sources(id), created_utc TEXT NOT NULL,
  valid_from_utc TEXT, valid_to_utc TEXT, method TEXT NOT NULL, hdf5_path TEXT NOT NULL,
  residual_px REAL, image_sha256 TEXT, submitted_by TEXT, star_count INTEGER
);
CREATE TABLE IF NOT EXISTS camera_settings(
  source_id TEXT PRIMARY KEY REFERENCES sources(id), updated_utc TEXT NOT NULL,
  crop_json TEXT, mask_json TEXT, updated_by TEXT, selected_calibration_id TEXT,
  mask_enabled INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS removed_sources(
  source_id TEXT PRIMARY KEY REFERENCES sources(id), removed_utc TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS crawler_runs(
  id INTEGER PRIMARY KEY, source_id TEXT NOT NULL REFERENCES sources(id), started_utc TEXT NOT NULL,
  finished_utc TEXT, state TEXT NOT NULL, discovered INTEGER NOT NULL DEFAULT 0,
  downloaded INTEGER NOT NULL DEFAULT 0, duplicate INTEGER NOT NULL DEFAULT 0, error TEXT
);
CREATE TABLE IF NOT EXISTS mosaics(
  id TEXT PRIMARY KEY, observation_utc TEXT NOT NULL, created_utc TEXT NOT NULL,
  hdf5_path TEXT NOT NULL, preview_path TEXT, source_count INTEGER NOT NULL, image_count INTEGER NOT NULL,
  clear_fraction REAL, emission_altitude_km REAL NOT NULL DEFAULT 100.0
);
CREATE TABLE IF NOT EXISTS suggestions(
  id INTEGER PRIMARY KEY, created_utc TEXT NOT NULL, name TEXT, contact TEXT,
  suggestion TEXT NOT NULL, page_url TEXT, user_agent TEXT, state TEXT NOT NULL DEFAULT 'new'
);
CREATE INDEX IF NOT EXISTS idx_images_observation ON images(observation_utc DESC);
CREATE INDEX IF NOT EXISTS idx_images_source_observation ON images(source_id, observation_utc DESC);
CREATE INDEX IF NOT EXISTS idx_images_pending ON images(processing_state, observation_utc) WHERE processing_state != 'complete';
CREATE INDEX IF NOT EXISTS idx_crawler_runs_source ON crawler_runs(source_id, started_utc DESC);
CREATE INDEX IF NOT EXISTS idx_mosaics_observation ON mosaics(observation_utc DESC);
CREATE INDEX IF NOT EXISTS idx_suggestions_state ON suggestions(state, created_utc DESC);
PRAGMA optimize;
-- Per-star brightness time series used to measure cloud thickness. One row per
-- camera, frame, star and colour channel. Sky position and image position are
-- both retained, along with every fitted Gaussian parameter and the background,
-- so a measurement can be re-examined without refitting.
CREATE TABLE IF NOT EXISTS star_photometry(
  source_id TEXT NOT NULL REFERENCES sources(id),
  image_id TEXT NOT NULL REFERENCES images(id),
  observation_utc TEXT NOT NULL,
  star_key TEXT NOT NULL,
  channel TEXT NOT NULL,
  ra_hours_j2000 REAL NOT NULL, dec_deg_j2000 REAL NOT NULL, vt_mag REAL NOT NULL,
  azimuth_deg REAL NOT NULL, elevation_deg REAL NOT NULL,
  predicted_x REAL NOT NULL, predicted_y REAL NOT NULL,
  centroid_x REAL, centroid_y REAL, centroid_offset_px REAL,
  background REAL, amplitude REAL,
  sigma_major REAL, sigma_minor REAL, angle_deg REAL,
  flux REAL, rms_residual REAL,
  residual_std REAL, amplitude_snr REAL, flux_snr REAL,
  background_dx REAL, background_dy REAL, background_dxy REAL,
  PRIMARY KEY(source_id,image_id,star_key,channel)
);
CREATE INDEX IF NOT EXISTS star_photometry_series
  ON star_photometry(source_id,star_key,channel,observation_utc);
CREATE INDEX IF NOT EXISTS star_photometry_frame
  ON star_photometry(source_id,observation_utc);
-- Sky conditions for one frame at one station. The Moon is a property of the
-- frame rather than of any star, so it is kept once per frame; moonlight raises
-- the background and its gradient, which is what the star fits have to work
-- against. The solar elevation is stored beside it because it gates everything.
CREATE TABLE IF NOT EXISTS frame_sky(
  source_id TEXT NOT NULL REFERENCES sources(id),
  image_id TEXT NOT NULL REFERENCES images(id),
  observation_utc TEXT NOT NULL,
  sun_elevation_deg REAL NOT NULL,
  moon_azimuth_deg REAL NOT NULL,
  moon_elevation_deg REAL NOT NULL,
  moon_illuminated_fraction REAL NOT NULL,
  moon_phase_angle_deg REAL NOT NULL,
  moon_distance_km REAL NOT NULL,
  moon_apparent_magnitude REAL NOT NULL,
  moon_sky_brightness REAL NOT NULL,
  PRIMARY KEY(source_id,image_id)
);
CREATE INDEX IF NOT EXISTS frame_sky_series ON frame_sky(source_id,observation_utc);
