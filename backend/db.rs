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
    Ok(conn)
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
