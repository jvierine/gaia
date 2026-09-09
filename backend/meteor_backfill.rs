//! Resumable, bounded historical retrieval. Progress is metadata in the archive SQLite DB.
use crate::{crawler, db, model::{SourceConfig, SourceKind}, norsk_meteor};
use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::params;
use std::{path::Path, time::Duration};

fn schedule(source: &SourceConfig, date: NaiveDate) -> Vec<DateTime<Utc>> {
    // Norway's evening through the following morning, including the Swedish station.
    let start = date.and_hms_opt(12,0,0).unwrap().and_utc();
    let end = start + chrono::Duration::hours(24);
    let step = chrono::Duration::seconds(source.interval_seconds.max(300) as i64);
    let mut result = Vec::new();
    let mut time = start;
    while time < end {
        if !norsk_meteor::daylight(source, time) { result.push(time); }
        time += step;
    }
    result
}

pub async fn run(sources: &[SourceConfig], db_path: &Path, root: &Path, date: &str) -> Result<()> {
    let night = NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    if night >= Utc::now().date_naive() { bail!("choose a past night starting date"); }
    let client = reqwest::Client::builder().user_agent("GAIA Data Center (gaia@juha.no)")
        .timeout(Duration::from_secs(30)).build()?;
    let conn = db::open(db_path)?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS meteor_backfill_frames(
        source_id TEXT NOT NULL REFERENCES sources(id), requested_utc TEXT NOT NULL,
        night TEXT NOT NULL, state TEXT NOT NULL, attempted_utc TEXT, error TEXT,
        PRIMARY KEY(source_id,requested_utc));")?;
    let mut planned=0usize;
    for source in sources.iter().filter(|s| s.enabled && matches!(s.kind, SourceKind::NorskMeteor)) {
        for time in schedule(source,night).into_iter().filter(|t|*t<Utc::now()-chrono::Duration::minutes(2)) {
            conn.execute("INSERT OR IGNORE INTO meteor_backfill_frames(source_id,requested_utc,night,state) VALUES(?1,?2,?3,'pending')",
                params![source.id,time.to_rfc3339(),date])?;
            planned+=1;
        }
    }
    tracing::info!(night=date, planned, "meteor backfill planned; one upstream task at a time");
    // Interleave stations/cameras in time order, preserving a checkpoint after every frame.
    let work: Vec<(String,String)> = {
        let mut q=conn.prepare("SELECT source_id,requested_utc FROM meteor_backfill_frames WHERE night=?1 AND state IN ('pending','failed','running') ORDER BY requested_utc,source_id")?;
        let rows=q.query_map([date],|r|Ok((r.get(0)?,r.get(1)?)))?;
        rows.collect::<std::result::Result<_,_>>()?
    };
    for (id,requested) in work {
        let Some(source)=sources.iter().find(|s|s.id==id && s.enabled) else {continue};
        let enabled:bool=conn.query_row("SELECT enabled FROM sources WHERE id=?1",[&id],|r|r.get(0))?;
        if !enabled {continue}
        let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM images WHERE source_id=?1 AND abs(julianday(observation_utc)-julianday(?2))*86400 < 1)",params![id,requested],|r|r.get(0))?;
        if exists {
            conn.execute("UPDATE meteor_backfill_frames SET state='archived',error=NULL WHERE source_id=?1 AND requested_utc=?2",params![id,requested])?;
            continue;
        }
        let time=DateTime::parse_from_rfc3339(&requested)?.with_timezone(&Utc);
        conn.execute("UPDATE meteor_backfill_frames SET state='running',attempted_utc=?3,error=NULL WHERE source_id=?1 AND requested_utc=?2",params![id,requested,Utc::now().to_rfc3339()])?;
        let result = async {
            // Adapter subtracts its standard two-minute live latency.
            let urls=norsk_meteor::candidates(&client,source,time+chrono::Duration::minutes(2)).await?;
            crawler::archive_urls(source,&client,db_path,root,urls).await
        }.await;
        let (state,error)=match result {
            Ok((_,n,_)) if n>0 => ("archived",None),
            Ok((_,_,n)) if n>0 => ("duplicate",None),
            Ok(_) => ("unavailable",Some("No image returned".to_owned())),
            Err(e) => { let message=format!("{e:#}"); (if message.contains("no image available") {"unavailable"} else {"failed"},Some(message)) }
        };
        conn.execute("UPDATE meteor_backfill_frames SET state=?3,error=?4 WHERE source_id=?1 AND requested_utc=?2",params![id,requested,state,error])?;
        tracing::info!(source=id,time=requested,state,"meteor backfill frame");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    tracing::info!(night=date,"meteor backfill pass complete; inspect meteor_backfill_frames for gaps");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_and_night_schedule() {
        let sources:Vec<SourceConfig>=serde_json::from_str(include_str!("../sources/norsk-meteor.json")).unwrap();
        assert_eq!(sources.len(),98);
        assert_eq!(sources.iter().map(|s|&s.id).collect::<std::collections::HashSet<_>>().len(),98);
        assert_eq!(sources.iter().filter(|s|s.interval_seconds==900).count(),21);
        let date=NaiveDate::from_ymd_opt(2026,9,8).unwrap();
        for source in &sources {
            assert!(source.enabled);
            let times=schedule(source,date);
            assert!(!times.is_empty());
            for time in &times { assert!(!norsk_meteor::daylight(source,*time)); }
            for pair in times.windows(2) {assert_eq!((pair[1]-pair[0]).num_seconds(),source.interval_seconds as i64);}
        }
    }
    #[test]
    fn minute_filename_is_parseable() {
        let time=chrono::NaiveDateTime::parse_from_str("20260908_1955","%Y%m%d_%H%M").unwrap();
        assert_eq!(time.and_utc().to_rfc3339(),"2026-09-08T19:55:00+00:00");
    }
}
