//! Per-camera, atomic progress snapshots. Never change ingestion or calibration.
use crate::AppState;
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
fn path(s:&AppState,id:&str)->std::path::PathBuf {s.archive_root.join("processing-status").join(format!("{:x}.json",Sha256::digest(id.as_bytes())))}
pub fn write(s:&AppState,id:&str,state:&str,total:usize,done:usize,ready:usize,error:Option<&str>,started:&str)->anyhow::Result<()> {
 let p=path(s,id);std::fs::create_dir_all(p.parent().unwrap())?;
 let temp=p.with_extension(format!("{}.pending",uuid::Uuid::new_v4()));
 std::fs::write(&temp,serde_json::to_vec(&json!({"state":state,"total":total,"done":done,"ready":ready,"error":error,"started_utc":started,"updated_utc":chrono::Utc::now().to_rfc3339()}))?)?;std::fs::rename(temp,p)?;Ok(())
}
pub fn read(s:&AppState,id:&str,changed:Option<&str>)->Value {
 let status:Value=std::fs::read(path(s,id)).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
 let parse=|v:&str|chrono::DateTime::parse_from_rfc3339(v).ok();
 if status.is_null() || changed.and_then(parse).zip(status["started_utc"].as_str().and_then(parse)).is_some_and(|(change,start)|change>start) {return json!({"state":"queued","ready":0});}
 if status["state"]=="processing" || status["state"]=="publishing" {if status["updated_utc"].as_str().and_then(parse).is_some_and(|t|chrono::Utc::now().timestamp()-t.timestamp()>600){return json!({"state":"stalled","ready":status["ready"],"error":"Processing has not reported progress for over 10 minutes"});}}
 status
}
pub fn kick() {
 // Same user service and existing flock: never create a second publisher.
 std::thread::spawn(|| { if let Err(e)=std::process::Command::new("systemctl").args(["--user","start","--no-block","gaia-publish.service"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status(){tracing::warn!(%e,"Could not wake publisher; timer will retry");} });
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn snapshots_are_atomic_and_new_calibrations_requeue() {
 let root=std::env::temp_dir().join(format!("gaia-status-{}",uuid::Uuid::new_v4()));
 let s=AppState{db_path:root.join("unused"),archive_root:root.clone(),sources:std::sync::Arc::new(vec![]),igrf:std::sync::Arc::new(vec![]),igrf_year:2026};
 assert_eq!(read(&s,"camera",None)["state"],"queued");
 write(&s,"camera","ready",10,10,9,None,"2026-09-13T08:00:00Z").unwrap();
 assert_eq!(read(&s,"camera",Some("2026-09-13T07:00:00Z"))["ready"],9);
 assert_eq!(read(&s,"camera",Some("2026-09-13T09:00:00Z"))["state"],"queued");
 assert_eq!(read(&s,"another camera",None)["state"],"queued");
 std::fs::remove_file(path(&s,"camera")).unwrap();std::fs::remove_dir(root.join("processing-status")).unwrap();std::fs::remove_dir(root).unwrap();
 }
}
