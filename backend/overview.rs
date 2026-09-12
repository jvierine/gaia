//! Compact station-separated contact sheets. No final composition on the server.
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use image::{RgbImage, imageops::FilterType, codecs::jpeg::JpegEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::{Mutex, atomic::{AtomicUsize, Ordering}}};
fn epoch(v:&Value)->Option<i64>{DateTime::parse_from_rfc3339(v.as_str()?).ok().map(|t|t.timestamp())}
fn asset(root:&Path,url:&str)->Result<std::path::PathBuf>{let name=url.rsplit('/').next().context("asset name")?;if name.contains("..")||!name.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_.".contains(&c)){bail!("unsafe asset")};Ok(root.join("assets").join(name))}
fn station(mut c:Value,times:&[i64],root:&Path,prefix:&str)->Result<Value>{
 let Some(p)=c.get("projection").filter(|v|v.is_object()).cloned() else{return Ok(c)};
 let source=c["source_id"].as_str().context("source id")?;
 let input=p["images"].as_array().context("camera frames")?;
 let mut selected=Vec::new();
 for &t in times {if let Some(f)=input.iter().rev().find(|f|epoch(&f["at"]).is_some_and(|at|at<=t&&t-at<=600)){
  if let Some(id)=f["source_id"].as_str(){if id!=source{bail!("station identity mismatch")}}
  let mut f=f.clone();f["source_id"]=json!(source);f["at"]=json!(DateTime::<Utc>::from_timestamp(t,0).unwrap().to_rfc3339());
  f["observation_at"]=input.iter().rev().find(|v|epoch(&v["at"]).is_some_and(|at|at<=t&&t-at<=600)).unwrap()["at"].clone();
  selected.push(f);
 }}
 if selected.is_empty(){c["projection"]=Value::Null;return Ok(c)}
 let digest=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!(["overview-v1-96-q80",source,selected]))?));
 let name=format!("overview-{digest}.jpg");let target=root.join("assets").join(&name);
 let side=96u32;let columns=12u32;let rows=(selected.len() as u32).div_ceil(columns);
 if !target.exists(){
  let mut sheet=RgbImage::new(columns*side,rows*side);
  for (i,f) in selected.iter().enumerate(){let im=image::open(asset(root,f["texture_url"].as_str().context("texture url")?)?)?.resize_exact(side,side,FilterType::Triangle).to_rgb8();image::imageops::replace(&mut sheet,&im,(i as u32%columns*side) as i64,(i as u32/columns*side) as i64)}
  let pending=target.with_extension("pending");let mut file=fs::File::create(&pending)?;JpegEncoder::new_with_quality(&mut file,80).encode_image(&sheet)?;fs::rename(pending,&target)?;
 }
 for (i,f) in selected.iter_mut().enumerate(){f["texture_url"]=json!(format!("{prefix}{name}"));f["texture_rect"]=json!([i as u32%columns*side,i as u32/columns*side,side,side]);}
 c["projection"]["images"]=json!(selected);Ok(c)
}
fn main()->Result<()>{
 let file=std::env::args().nth(1).context("manifest path")?;let path=Path::new(&file);let root=path.parent().context("root")?;
 let mut m:Value=serde_json::from_slice(&fs::read(path)?)?;
 if m["composition"]!="browser-layers-v1"{bail!("independent layers required")}
 let images=m["images"].as_array().context("frames")?;
 let end=images.iter().filter_map(|v|epoch(&v["at"])).max().context("empty period")?;
 let end=end/720*720;let times:Vec<i64>=(0..120).map(|i|end-(119-i)*720).collect();
 let cameras=m["cameras"].as_array().context("cameras")?;let prefix=if m["audience"]=="anonymous-no-starvisor"{"/gaia/open/assets/"}else{"/gaia/public/assets/"};
 let next=AtomicUsize::new(0);let output=Mutex::new(Vec::new());
 std::thread::scope(|scope|{for _ in 0..16{let next=&next;let output=&output;let times=&times;scope.spawn(move||{loop{let i=next.fetch_add(1,Ordering::Relaxed);if i>=cameras.len(){break}let value=station(cameras[i].clone(),times,root,prefix);output.lock().unwrap().push((i,value));}});}});
 let mut result=output.into_inner().unwrap();result.sort_by_key(|(i,_)|*i);
 let cameras:Vec<Value>=result.into_iter().map(|(_,v)|v).collect::<Result<_>>()?;
 m["overview"]=json!({"composition":"browser-layers-v1","stitching":m["stitching"],"images":times.iter().map(|t|json!({"at":DateTime::<Utc>::from_timestamp(*t,0).unwrap().to_rfc3339()})).collect::<Vec<_>>(),"cameras":cameras,"sample_minutes":12,"duration_ms":15000});
 let pending=path.with_extension("overview-pending");fs::write(&pending,serde_json::to_vec(&m)?)?;fs::rename(pending,path)?;
 println!("Prepared 120 full-day overview samples, station-separated JPEG sheets");Ok(())
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn sheet_tiles_preserve_station_and_time(){
  let dir=tempfile::tempdir().unwrap();fs::create_dir(dir.path().join("assets")).unwrap();
  for (name,color) in [("red.png",[255,0,0]),("green.png",[0,255,0])]{RgbImage::from_pixel(8,8,image::Rgb(color)).save(dir.path().join("assets").join(name)).unwrap()}
  let camera=json!({"source_id":"station-a","copyright":"Producer retains copyright","projection":{"images":[{"at":"2026-09-11T00:00:00Z","source_id":"station-a","texture_url":"/gaia/open/assets/red.png"},{"at":"2026-09-11T00:12:00Z","source_id":"station-a","texture_url":"/gaia/open/assets/green.png"}]}});
  let t=epoch(&json!("2026-09-11T00:00:00Z")).unwrap();let result=station(camera,&[t,t+720],dir.path(),"/gaia/open/assets/").unwrap();
  assert_eq!(result["copyright"],"Producer retains copyright");let frames=result["projection"]["images"].as_array().unwrap();assert_eq!(frames.len(),2);assert_eq!(frames[1]["texture_rect"],json!([96,0,96,96]));
  let sheet=image::open(asset(dir.path(),frames[0]["texture_url"].as_str().unwrap()).unwrap()).unwrap().to_rgb8();assert!(sheet.get_pixel(48,48)[0]>240);assert!(sheet.get_pixel(144,48)[1]>240);assert_eq!(frames[1]["source_id"],"station-a");
 }
 #[test] fn wrong_station_is_rejected(){let dir=tempfile::tempdir().unwrap();let c=json!({"source_id":"a","projection":{"images":[{"source_id":"b","at":"2026-09-11T00:00:00Z"}]}});assert!(station(c,&[epoch(&json!("2026-09-11T00:00:00Z")).unwrap()],dir.path(),"/gaia/open/assets/").is_err())}
}
