use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(include_str!("schema.sql"))?;
    // schema.sql is replayed on every open, but CREATE TABLE IF NOT EXISTS cannot
    // widen a table that already exists, so added columns are applied here.
    add_column(&conn, "calibrations", "star_count", "INTEGER")?;
    add_column(&conn, "camera_settings", "selected_calibration_id", "TEXT")?;
    add_column(&conn, "camera_settings", "mask_enabled", "INTEGER NOT NULL DEFAULT 1")?;
    add_column(&conn, "camera_settings", "quality_exponent", "INTEGER NOT NULL DEFAULT 0")?;
    for column in [
        "residual_std",
        "amplitude_snr",
        "flux_snr",
        "background_dx",
        "background_dy",
        "background_dxy",
    ] {
        add_column(&conn, "star_photometry", column, "REAL")?;
    }
    conn.execute_batch("
CREATE TABLE IF NOT EXISTS calibration_rebuild (id INTEGER PRIMARY KEY CHECK(id=1), requested INTEGER NOT NULL, completed INTEGER NOT NULL);
INSERT OR IGNORE INTO calibration_rebuild VALUES(1,1,0);
CREATE TRIGGER IF NOT EXISTS rebuild_calibration_insert AFTER INSERT ON calibrations BEGIN UPDATE calibration_rebuild SET requested=requested+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS rebuild_calibration_update AFTER UPDATE OF hdf5_path,valid_from_utc,valid_to_utc ON calibrations BEGIN UPDATE calibration_rebuild SET requested=requested+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS rebuild_camera_enabled AFTER UPDATE OF enabled ON sources WHEN NEW.enabled IS NOT OLD.enabled BEGIN UPDATE calibration_rebuild SET requested=requested+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS rebuild_selection_insert AFTER INSERT ON camera_settings WHEN NEW.selected_calibration_id IS NOT NULL BEGIN UPDATE calibration_rebuild SET requested=requested+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS rebuild_selection_update AFTER UPDATE OF selected_calibration_id ON camera_settings WHEN NEW.selected_calibration_id IS NOT OLD.selected_calibration_id BEGIN UPDATE calibration_rebuild SET requested=requested+1 WHERE id=1; END;
")?;
    Ok(conn)
}

/// Adds a column unless it is already present. Table and column names are
/// compile-time literals from this crate, never request data.
fn add_column(conn: &Connection, table: &str, column: &str, decl: &str) -> Result<()> {
    let mut q = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let existing = q
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !existing.iter().any(|name| name == column) {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"), [])?;
    }
    Ok(())
}

pub fn upsert_source(conn: &Connection, source: &crate::model::SourceConfig) -> Result<()> {
    let producer_id = format!("producer:{}", source.id);
    conn.execute("INSERT INTO producers(id,name,institution,website,acknowledgement,copyright,license)
        VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET name=excluded.name,institution=excluded.institution,
        website=excluded.website,acknowledgement=excluded.acknowledgement,copyright=excluded.copyright,license=excluded.license",
        params![producer_id, source.producer.name, source.producer.institution, source.producer.website,
            source.producer.acknowledgement, source.producer.copyright, source.producer.license])?;
    conn.execute("INSERT INTO sources(id,name,kind,url,timestamp_mode,interval_seconds,producer_id,latitude_deg,longitude_deg,altitude_m,enabled,config_json)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(id) DO UPDATE SET name=excluded.name,
        kind=excluded.kind,url=excluded.url,timestamp_mode=excluded.timestamp_mode,interval_seconds=excluded.interval_seconds,
        producer_id=excluded.producer_id,config_json=excluded.config_json",
        params![source.id, source.name, format!("{:?}", source.kind).to_lowercase(), source.url,
            format!("{:?}", source.timestamp_mode).to_lowercase(), source.interval_seconds as i64, producer_id,
            source.latitude_deg, source.longitude_deg, source.altitude_m, source.enabled, serde_json::to_string(source)?])?;
    Ok(())
}

#[cfg(test)]
mod rebuild_tests {
    #[test]
    fn requests_are_transactional_and_new_requests_survive_acknowledgement(){
        let dir=tempfile::tempdir().unwrap();let conn=super::open(&dir.path().join("test.sqlite")).unwrap();
        let revision=||conn.query_row("SELECT requested FROM calibration_rebuild",[],|r|r.get::<_,i64>(0)).unwrap();
        let initial=revision();
        conn.execute_batch("BEGIN; INSERT INTO calibrations(id,created_utc,method,hdf5_path) VALUES('rollback','now','test','test.h5'); ROLLBACK;").unwrap();
        assert_eq!(revision(),initial);
        conn.execute("INSERT INTO calibrations(id,created_utc,method,hdf5_path) VALUES('saved','now','test','test.h5')",[]).unwrap();
        let captured=revision();assert_eq!(captured,initial+1);
        conn.execute("UPDATE calibrations SET valid_from_utc='earlier' WHERE id='saved'",[]).unwrap();
        conn.execute("UPDATE calibration_rebuild SET completed=?1",[captured]).unwrap();
        assert!(revision()>captured);
    }
}
