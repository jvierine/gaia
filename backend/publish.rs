//! Offline public atlas publisher. No public database or projection API required.
use crate::{AppState, db, geometry, projection};
use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
const W:u32=4096;
const H:u32=2048;
fn atomic(path:&Path, bytes:&[u8])->Result<()> {
    let temp=path.with_extension("pending");std::fs::write(&temp,bytes)?;std::fs::rename(temp,path)?;Ok(())
}
fn publish_lens_models(conn:&rusqlite::Connection,assets:&Path)->Result<Vec<serde_json::Value>> {
    let mut query=conn.prepare("SELECT s.id,s.name,p.name,s.latitude_deg,s.longitude_deg,c.id,c.created_utc,c.valid_from_utc,c.valid_to_utc,c.method,c.hdf5_path,c.residual_px FROM sources s JOIN producers p ON p.id=s.producer_id JOIN calibrations c ON c.source_id=s.id WHERE s.enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) ORDER BY s.name,julianday(c.valid_from_utc),c.created_utc")?;
    let rows=query.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<f64>>(3)?,r.get::<_,Option<f64>>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,Option<String>>(8)?,r.get::<_,String>(9)?,r.get::<_,String>(10)?,r.get::<_,Option<f64>>(11)?)))?;
    let mut cameras:BTreeMap<String,serde_json::Value>=BTreeMap::new();
    for row in rows {
        let (source_id,name,producer,lat,lon,id,created,valid_from,valid_to,method,path,residual)=row?;
        let bytes=std::fs::read(&path)?;let sha=format!("{:x}",Sha256::digest(&bytes));let filename=format!("lens-{sha}.h5");let destination=assets.join(&filename);
        if !destination.exists(){atomic(&destination,&bytes)?}
        let camera=cameras.entry(source_id.clone()).or_insert_with(||json!({"source_id":source_id,"name":name,"producer":producer,"latitude_deg":lat,"longitude_deg":lon,"models":[]}));
        camera["models"].as_array_mut().unwrap().push(json!({"calibration_id":id,"created_utc":created,"valid_from_utc":valid_from,"valid_to_utc":valid_to,"method":method,"residual_px":residual,"format":"AIDA/WISC HDF5","sha256":sha,"url":format!("/gaia/public/assets/{filename}")}));
    }
    Ok(cameras.into_values().collect())
}
pub fn run(s:&AppState)->Result<()> {
    let root=s.archive_root.join("public");let assets=root.join("assets");std::fs::create_dir_all(&assets)?;
    let conn=db::open(&s.db_path)?;
    let cameras=conn.prepare("SELECT id,latitude_deg,longitude_deg,COALESCE(altitude_m,0) FROM sources WHERE enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=sources.id) AND latitude_deg IS NOT NULL AND longitude_deg IS NOT NULL AND EXISTS(SELECT 1 FROM calibrations WHERE source_id=sources.id) ORDER BY id")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,f64>(1)?,r.get::<_,f64>(2)?,r.get::<_,f64>(3)?)))?.collect::<Result<Vec<_>,_>>()?;
    let camera_indices:BTreeMap<String,u32>=cameras.iter().enumerate().map(|(i,(id,_,_,_))|(id.clone(),i as u32+1)).collect();
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
        let texture_key=format!("{:x}",Sha256::digest(format!("atlas-v1:{epoch}:{}",serde_json::to_string(&inputs)?)));
        let source_key=format!("{:x}",Sha256::digest(format!("atlas-v2-attribution:{epoch}:{}",serde_json::to_string(&inputs)?)));
        let name=format!("{texture_key}.webp");let dest=assets.join(&name);
        let source_name=format!("source-{source_key}.png");let source_dest=assets.join(&source_name);
        if !dest.exists()||!source_dest.exists(){
            let mut out=image::RgbaImage::new(W,H);let mut source_pixels=image::RgbImage::new(W,H);let mut scores=vec![-2f32;(W*H)as usize];
            for (id,lat,lon,alt,a) in &inputs {
                let source_index=*camera_indices.get(*id).expect("published camera index");
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
                        source_pixels.put_pixel(xx,y as u32,image::Rgb([((source_index>>16)&255)as u8,((source_index>>8)&255)as u8,(source_index&255)as u8]));
                    }}
                }
            }
            let temp=dest.with_extension("pending");out.save_with_format(&temp,image::ImageFormat::WebP)?;std::fs::rename(temp,dest)?;
            let source_small=image::imageops::resize(&source_pixels,W/4,H/4,image::imageops::FilterType::Nearest);
            let source_temp=source_dest.with_extension("pending");source_small.save_with_format(&source_temp,image::ImageFormat::Png)?;std::fs::rename(source_temp,source_dest)?;
        }
        frames.push(json!({"at":at.to_rfc3339(),"width":W,"height":H,"texture_url":format!("/gaia/public/assets/{name}"),"source_map_url":format!("/gaia/public/assets/{source_name}"),"contributors":inputs.iter().map(|(id,_,_,_,a)|json!({"source_id":id,"observation_utc":a["observation_utc"],"calibration_id":a["calibration_id"]})).collect::<Vec<_>>()}));
    }
    frames.reverse();
    anyhow::ensure!(!frames.is_empty(),"No composites available; retaining previous publication");
    let credits=conn.prepare("SELECT DISTINCT p.name,p.website,p.acknowledgement,p.copyright FROM producers p JOIN sources s ON s.producer_id=p.id")?.query_map([],|r|Ok(json!({"name":r.get::<_,String>(0)?,"website":r.get::<_,Option<String>>(1)?,"acknowledgement":r.get::<_,String>(2)?,"copyright":r.get::<_,String>(3)?})))?.collect::<Result<Vec<_>,_>>()?;
    let public_cameras=conn.prepare("SELECT s.id,s.name,p.name,COALESCE(p.institution,p.name),COALESCE(NULLIF(p.website,''),s.url),s.latitude_deg,s.longitude_deg,p.acknowledgement,p.copyright,EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id) FROM sources s JOIN producers p ON p.id=s.producer_id WHERE s.enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) ORDER BY p.name,s.name")?.query_map([],|r|{
        let id=r.get::<_,String>(0)?;
        Ok(json!({"source_id":id,"name":r.get::<_,String>(1)?,"producer":r.get::<_,String>(2)?,"institution":r.get::<_,String>(3)?,"website_url":r.get::<_,String>(4)?,"latitude_deg":r.get::<_,Option<f64>>(5)?,"longitude_deg":r.get::<_,Option<f64>>(6)?,"acknowledgement":r.get::<_,String>(7)?,"copyright":r.get::<_,String>(8)?,"calibrated":r.get::<_,bool>(9)?,"map_index":camera_indices.get(&id)}))
    })?.collect::<Result<Vec<_>,_>>()?;
    let lens_models=publish_lens_models(&conn,&assets)?;
    let lens_model_documentation=json!({"format":"AIDA/WISC HDF5","recommended_dataset":"/wisc_optpar_with_optmod","dimension_attributes":["image_width","image_height"],"pixel_coordinates":"zero-based raw image pixel centers","azimuth":"degrees clockwise from geographic north","elevation":"degrees above horizon","validity_interval":"valid_from_utc inclusive, valid_to_utc exclusive; null is open","python_mapper":"https://github.com/jvierine/widefield-star-calibrator/blob/main/wisc_lens.py"});
    let manifest=json!({"generated_utc":Utc::now().to_rfc3339(),"width":W,"height":H,"geometry_url":"/gaia/public/assets/shell-v1.bin","vertex_count":mesh.len()/20,"images":frames,"credits":credits,"cameras":public_cameras,"lens_models":lens_models,"lens_model_documentation":lens_model_documentation,"igrf_url":format!("/gaia/public/assets/igrf-{}.bin",s.igrf_year)});
    atomic(&root.join("manifest.json"),&serde_json::to_vec(&manifest)?)?;Ok(())
}
