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
 // Newest samples have independent previews; full sheets are only needed for Play.
 // Gzip preserves every geometry coordinate, UV and magnetic weight exactly.
 let count=selected.len();
 for (i,f) in selected.iter_mut().enumerate(){
  let geometry=f["geometry_url"].as_str().or_else(||p["geometry_url"].as_str());
  if let Some(url)=geometry {let src=asset(root,url)?;let gz=src.with_extension("bin.gz");
   if !gz.exists(){use std::io::Write;let pending=gz.with_extension("pending");let mut encoder=flate2::write::GzEncoder::new(fs::File::create(&pending)?,flate2::Compression::default());encoder.write_all(&fs::read(&src)?)?;encoder.finish()?;fs::rename(pending,&gz)?;}
   f["geometry_gzip_url"]=json!(format!("{prefix}{}",gz.file_name().unwrap().to_str().unwrap()));
  }
  if i+3>=count {let original=f["texture_url"].as_str().context("preview input")?;let name=format!("preview-{:x}.jpg",Sha256::digest(format!("96-q80-{source}-{original}")));let path=root.join("assets").join(&name);
   if !path.exists(){let im=image::open(asset(root,original)?)?.resize_exact(96,96,FilterType::Triangle).to_rgb8();let pending=path.with_extension("pending");JpegEncoder::new_with_quality(fs::File::create(&pending)?,80).encode_image(&im)?;fs::rename(pending,path)?;}
   f["preview_texture_url"]=json!(format!("{prefix}{name}"));
  }
 f["texture_url"]=json!(format!("{prefix}{name}"));f["texture_rect"]=json!([i as u32%columns*side,i as u32/columns*side,side,side]);}
 c["projection"]["images"]=json!(selected);Ok(c)
}
fn main()->Result<()>{
 let file=std::env::args().nth(1).context("manifest path")?;let path=Path::new(&file);let root=path.parent().context("root")?;
 let mut m:Value=serde_json::from_slice(&fs::read(path)?)?;
 if m["composition"]!="browser-layers-v1"{bail!("independent layers required")}
 let images=m["images"].as_array().context("frames")?;
 let end=images.iter().filter_map(|v|epoch(&v["at"])).max().context("empty period")?;
 let end=end/720*720;let day=m["date"].as_str().map(|s|chrono::NaiveDate::parse_from_str(s,"%Y-%m-%d")).transpose()?;let times:Vec<i64>=if let Some(day)=day{let start=day.and_hms_opt(0,0,0).unwrap().and_utc().timestamp();(0..120).map(|i|start+i*720).collect()}else{(0..120).map(|i|end-(119-i)*720).collect()};
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
 #[test] fn compressed_mesh_and_preview_match_original(){
  use std::io::Read;
  let dir=tempfile::tempdir().unwrap();fs::create_dir(dir.path().join("assets")).unwrap();
  let mesh:Vec<u8>=(0..1024).map(|i|(i%251) as u8).collect();fs::write(dir.path().join("assets/mesh.bin"),&mesh).unwrap();
  RgbImage::from_pixel(8,8,image::Rgb([0,255,0])).save(dir.path().join("assets/green.png")).unwrap();
  let t=epoch(&json!("2026-09-11T00:00:00Z")).unwrap();
  let c=json!({"source_id":"a","projection":{"geometry_url":"/gaia/open/assets/mesh.bin","images":[{"source_id":"a","at":"2026-09-11T00:00:00Z","texture_url":"/gaia/open/assets/green.png"}]}});
  let c=station(c,&[t],dir.path(),"/gaia/open/assets/").unwrap();let f=&c["projection"]["images"][0];
  let mut decoded=Vec::new();flate2::read::GzDecoder::new(fs::File::open(asset(dir.path(),f["geometry_gzip_url"].as_str().unwrap()).unwrap()).unwrap()).read_to_end(&mut decoded).unwrap();assert_eq!(decoded,mesh);
  let preview=image::open(asset(dir.path(),f["preview_texture_url"].as_str().unwrap()).unwrap()).unwrap().to_rgb8();assert_eq!(preview.dimensions(),(96,96));assert!(preview.get_pixel(48,48)[1]>240);
 }
 #[test] fn wrong_station_is_rejected(){let dir=tempfile::tempdir().unwrap();let c=json!({"source_id":"a","projection":{"images":[{"source_id":"b","at":"2026-09-11T00:00:00Z"}]}});assert!(station(c,&[epoch(&json!("2026-09-11T00:00:00Z")).unwrap()],dir.path(),"/gaia/open/assets/").is_err())}
}
