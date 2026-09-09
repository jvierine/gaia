//! Offline public atlas publisher. No public database or projection API required.
use crate::{AppState, db, geometry, projection};
use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;
const W:u32=4096;
const H:u32=2048;
fn atomic(path:&Path, bytes:&[u8])->Result<()> {
    let temp=path.with_extension("pending");std::fs::write(&temp,bytes)?;std::fs::rename(temp,path)?;Ok(())
}
pub fn run(s:&AppState)->Result<()> {
    let root=s.archive_root.join("public");let assets=root.join("assets");std::fs::create_dir_all(&assets)?;
    let conn=db::open(&s.db_path)?;
    let cameras=conn.prepare("SELECT id,latitude_deg,longitude_deg,COALESCE(altitude_m,0) FROM sources WHERE enabled=1 AND latitude_deg IS NOT NULL AND longitude_deg IS NOT NULL AND EXISTS(SELECT 1 FROM calibrations WHERE source_id=sources.id)")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,f64>(1)?,r.get::<_,f64>(2)?,r.get::<_,f64>(3)?)))?.collect::<Result<Vec<_>,_>>()?;
    let mut mesh=Vec::new();
    for y in 0..90 {for x in 0..180 {for(dx,dy)in[(0,0),(1,0),(1,1),(0,0),(1,1),(0,1)] {
        let u=(x+dx)as f64/180.;let v=(y+dy)as f64/90.;let lon=(u*360.-180.).to_radians();let lat=(90.-v*180.).to_radians();let r=1.+100./6371.;
        for f in [r*lat.cos()*lon.sin(),r*lat.sin(),r*lat.cos()*lon.cos(),u,v]{mesh.extend_from_slice(&(f as f32).to_le_bytes())}
    }}}
    atomic(&assets.join("shell-v1.bin"),&mesh)?;
    atomic(&assets.join(format!("igrf-{}.bin",s.igrf_year)),&s.igrf)?;
    let end=Utc::now().timestamp()/60*60;
    let mut frames=Vec::new();let igrf=ferromagnetic::igrf::IGRF::default();
    // Five-minute history plus the latest minute. Reuse immutable completed frames.
    let mut epochs:Vec<i64>=((end-86400)/300..=end/300).map(|n|n*300).collect();epochs.push(end);epochs.sort();epochs.dedup();
    // Work newest-first so initial publication has a live frame before backfill.
    for epoch in epochs.into_iter().rev() {
        let at=Utc.timestamp_opt(epoch,0).unwrap();let mut inputs=Vec::new();
        for (id,lat,lon,alt) in &cameras {
            match projection::assets(s,id,Some(at)) {Ok(a)=>inputs.push((id,lat,lon,alt,a)),Err(e)=>tracing::debug!(%id,%e,"No publishable camera frame")}
        }
        if inputs.is_empty(){continue}
        let key=format!("{:x}",Sha256::digest(format!("atlas-v1:{epoch}:{}",serde_json::to_string(&inputs)?)));
        let name=format!("{key}.webp");let dest=assets.join(&name);
        if !dest.exists(){
            let mut out=image::RgbaImage::new(W,H);let mut scores=vec![-2f32;(W*H)as usize];
            for (_,lat,lon,alt,a) in &inputs {
                let cache=s.archive_root.join("projection-cache");
                let geo=std::fs::read(cache.join(a["geometry_url"].as_str().unwrap().rsplit('/').next().unwrap()))?;
                let im=image::open(cache.join(a["texture_url"].as_str().unwrap().rsplit('/').next().unwrap()))?.to_rgb8();
                let field=igrf.calc(**lat,**lon,0.,s.igrf_year as f64).result;
                // IGRF upward field-line direction, not a dipole or geographic zenith.
                let (az,el)=if field.inclination>=0.{(field.declination+180.,field.inclination)}else{(field.declination,-field.inclination)};
                let m=geometry::az_el_direction(**lat,**lon,az,el);let o=geometry::observer_ecef(**lat,**lon,**alt/1000.);
                let g:Vec<f32>=geo.chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).collect();
                for t in g.chunks_exact(15){
                    let mut p=[[0f64;2];3];for k in 0..3{let x=t[k*5]as f64;let y=t[k*5+1]as f64;let z=t[k*5+2]as f64;p[k]=[(x.atan2(z)/std::f64::consts::TAU+0.5)*W as f64,(0.5-(y/(x*x+y*y+z*z).sqrt()).clamp(-1.,1.).asin()/std::f64::consts::PI)*H as f64];}
                    for k in 1..3{while p[k][0]-p[0][0]>W as f64/2.{p[k][0]-=W as f64}while p[k][0]-p[0][0]< -(W as f64)/2.{p[k][0]+=W as f64}}
                    let den=(p[1][1]-p[2][1])*(p[0][0]-p[2][0])+(p[2][0]-p[1][0])*(p[0][1]-p[2][1]);if den.abs()<1e-9{continue}
                    let xmin=p.iter().map(|p|p[0].floor()as i32).min().unwrap();let xmax=p.iter().map(|p|p[0].ceil()as i32).max().unwrap();
                    let ymin=p.iter().map(|p|p[1].floor()as i32).min().unwrap().max(0);let ymax=p.iter().map(|p|p[1].ceil()as i32).max().unwrap().min(H as i32-1);
                    for y in ymin..=ymax{for x in xmin..=xmax{
                        let a=((p[1][1]-p[2][1])*(x as f64+0.5-p[2][0])+(p[2][0]-p[1][0])*(y as f64+0.5-p[2][1]))/den;
                        let b=((p[2][1]-p[0][1])*(x as f64+0.5-p[2][0])+(p[0][0]-p[2][0])*(y as f64+0.5-p[2][1]))/den;let c=1.-a-b;if a<0.||b<0.||c<0.{continue}
                        let q=[a,b,c];let v=|axis:usize| (0..3).map(|k|q[k]*t[k*5+axis]as f64).sum::<f64>();
                        let d=[v(2)*6371.-o[0],v(0)*6371.-o[1],v(1)*6371.-o[2]];let norm=d.iter().map(|n|n*n).sum::<f64>().sqrt();let score=(d.iter().zip(m).map(|(d,m)|d*m).sum::<f64>()/norm)as f32;
                        let xx=x.rem_euclid(W as i32)as u32;let idx=(y as u32*W+xx)as usize;if score<=scores[idx]{continue}scores[idx]=score;
                        let tx=(v(3)*im.width()as f64).clamp(0.,im.width()as f64-1.)as u32;let ty=(v(4)*im.height()as f64).clamp(0.,im.height()as f64-1.)as u32;let rgb=im.get_pixel(tx,ty).0;out.put_pixel(xx,y as u32,image::Rgba([rgb[0],rgb[1],rgb[2],255]));
                    }}
                }
            }
            let temp=dest.with_extension("pending");out.save_with_format(&temp,image::ImageFormat::WebP)?;std::fs::rename(temp,dest)?;
        }
        frames.push(json!({"at":at.to_rfc3339(),"width":W,"height":H,"texture_url":format!("/gaia/public/assets/{name}"),"contributors":inputs.iter().map(|(id,_,_,_,a)|json!({"source_id":id,"observation_utc":a["observation_utc"]})).collect::<Vec<_>>()}));
    }
    frames.reverse();
    anyhow::ensure!(!frames.is_empty(),"No composites available; retaining previous publication");
    let credits=conn.prepare("SELECT DISTINCT p.name,p.website,p.acknowledgement,p.copyright FROM producers p JOIN sources s ON s.producer_id=p.id")?.query_map([],|r|Ok(json!({"name":r.get::<_,String>(0)?,"website":r.get::<_,Option<String>>(1)?,"acknowledgement":r.get::<_,String>(2)?,"copyright":r.get::<_,String>(3)?})))?.collect::<Result<Vec<_>,_>>()?;
    let manifest=json!({"generated_utc":Utc::now().to_rfc3339(),"width":W,"height":H,"geometry_url":"/gaia/public/assets/shell-v1.bin","vertex_count":mesh.len()/20,"images":frames,"credits":credits,"igrf_url":format!("/gaia/public/assets/igrf-{}.bin",s.igrf_year)});
    atomic(&root.join("manifest.json"),&serde_json::to_vec(&manifest)?)?;Ok(())
}
