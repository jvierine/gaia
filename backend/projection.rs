//! AIDA raw-pixel inverse and straight-ray, spherical 100 km shell projection.
use anyhow::{Context, Result, bail};
use serde_json::{Value,json};
use crate::{db,geometry,AppState};
#[derive(serde::Serialize)]
pub struct Projection {
    pub source_id:String,pub observation_utc:String,pub width:u32,pub height:u32,
    pub altitude_km:u32,pub excluded_pixels:u32,pub mask_polygon_count:usize,pub vertices:Vec<f32>,
    #[serde(skip)] pub uv:Vec<f32>,
}

fn numeric(path:&str, name:&str, attr:bool)->Result<Vec<f64>> {
    let out=std::process::Command::new("h5dump").args(["-y","-w","0","-m","%0.17g",if attr{"-a"}else{"-d"},name,path]).output()?;
    if !out.status.success(){bail!("cannot read calibration {name}")}
    let text=String::from_utf8(out.stdout)?;
    let data=text.split("DATA {").nth(1).context("missing calibration data")?.split('}').next().unwrap();
    data.split(',').map(|v|Ok(v.trim().parse()?)).collect()
}
/// First dataspace dimension of an HDF5 dataset, e.g. the number of identified
/// stars in `/selected_stars`. Read from the header so a change in column count
/// cannot be mistaken for a change in row count.
pub fn dataset_rows(path:&str,name:&str)->Result<usize>{
    let out=std::process::Command::new("h5dump").args(["-H","-d",name,path]).output()?;
    if !out.status.success(){bail!("cannot read calibration {name}")}
    let text=String::from_utf8(out.stdout)?;
    let dims=text.split("SIMPLE { (").nth(1).context("missing calibration dataspace")?;
    let first=dims.split(')').next().unwrap().split(',').next().context("empty dataspace")?;
    Ok(first.trim().parse()?)
}
fn ray(x:f64,y:f64,p:&[f64],mode:i32)->Option<[f64;3]>{
    let xd=(x-0.5-p[5])/p[0];let yd=(y-0.5-p[6])/p[1];
    let (mut u,mut v)=(xd,yd);
    let theta;
    if mode==20 {
        for _ in 0..30 {let r=u*u+v*v;let k=1.+p[7]*r+p[8]*r*r+p[9]*r*r*r;if k.abs()<1e-10{return None}let nu=(xd-2.*p[10]*u*v-p[11]*(r+2.*u*u))/k;let nv=(yd-p[10]*(r+2.*v*v)-2.*p[11]*u*v)/k;u=nu;v=nv;}
        let r=u*u+v*v;let k=1.+p[7]*r+p[8]*r*r+p[9]*r*r*r;
        if (u*k+2.*p[10]*u*v+p[11]*(r+2.*u*u)-xd).hypot(v*k+p[10]*(r+2.*v*v)+2.*p[11]*u*v-yd)>1e-5{return None}
        theta=u.hypot(v).atan();
    }else{let r=u.hypot(v);let a=p[7];theta=match mode{1=>r.atan(),2=>(r).asin()/a,4=>r.powf(1./a),5=>r.atan()/a,6=>2.*r.asin(),12=>if a>0.{(r*a).atan()/a}else if a<0.{(r*a).asin()/a}else{r},3=>{let(mut lo,mut hi)=(0.,std::f64::consts::FRAC_PI_2-1e-6);for _ in 0..40{let m:f64=(lo+hi)/2.;if (1.-a)*m.tan()+a*m<r{lo=m}else{hi=m}}(lo+hi)/2.},_=>return None};}
    if !theta.is_finite()||theta<0.||theta>std::f64::consts::PI{return None}
    let r=u.hypot(v);let(mut e,mut n,mut z)=if r<1e-12{(0.,0.,1.)}else{(u/r*theta.sin(),v/r*theta.sin(),theta.cos())};
    let(a,b,g)=(p[2].to_radians(),p[3].to_radians(),p[4].to_radians());
    (e,n)=(g.cos()*e-g.sin()*n,g.sin()*e+g.cos()*n);
    (n,z)=(b.cos()*n+b.sin()*z,-b.sin()*n+b.cos()*z);
    (e,z)=(a.cos()*e+a.sin()*z,-a.sin()*e+a.cos()*z);
    if z<0.{None}else{Some([e,n,z])}
}
pub fn build(s:&AppState,id:&str,at:Option<chrono::DateTime<chrono::Utc>>)->Result<Projection>{
    let conn=db::open(&s.db_path)?;
    let (crop_json,mask_json):(Option<String>,Option<String>)=conn.query_row("SELECT c.crop_json,c.mask_json FROM sources s LEFT JOIN camera_settings c ON c.source_id=s.id WHERE s.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let crop=if let Some(text)=crop_json {
        let v:Value=serde_json::from_str(&text)?;
        [v["left"].as_f64().context("invalid crop left")?,v["top"].as_f64().context("invalid crop top")?,v["right"].as_f64().context("invalid crop right")?,v["bottom"].as_f64().context("invalid crop bottom")?]
    }else{[0.,0.,1.,1.]};
    let polygons:Vec<Vec<[f64;2]>>=if let Some(text)=mask_json {
        let v:Value=serde_json::from_str(&text)?;
        if v["coordinate_system"]!="normalized_image"{bail!("unsupported mask coordinate system")}
        serde_json::from_value(v["polygons"].clone())?
    }else{vec![]};
    let at=at.map(|t|t.to_rfc3339());
    let (path,cal,lat,lon,alt,utc):(String,String,f64,f64,f64,String)=conn.query_row("SELECT i.archive_path,c.hdf5_path,s.latitude_deg,s.longitude_deg,COALESCE(s.altitude_m,0),i.observation_utc FROM sources s JOIN images i ON i.source_id=s.id JOIN calibrations c ON c.id=COALESCE((SELECT cs2.selected_calibration_id FROM camera_settings cs2 WHERE cs2.source_id=s.id AND EXISTS(SELECT 1 FROM calibrations csel WHERE csel.id=cs2.selected_calibration_id AND csel.source_id=s.id)),(SELECT cc.id FROM calibrations cc WHERE cc.source_id=s.id AND (cc.valid_from_utc IS NULL OR julianday(cc.valid_from_utc)<=julianday(i.observation_utc)) AND (cc.valid_to_utc IS NULL OR julianday(cc.valid_to_utc)>julianday(i.observation_utc)) ORDER BY julianday(cc.valid_from_utc) DESC,cc.created_utc DESC LIMIT 1)) WHERE s.id=?1 AND s.enabled=1 AND (?2 IS NULL OR (julianday(i.observation_utc)<=julianday(?2) AND julianday(i.observation_utc)>=julianday(?2)-10.0/1440.0)) ORDER BY i.observation_utc DESC LIMIT 1",rusqlite::params![id,at],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let p=numeric(&cal,"wisc_optpar_with_optmod",false)?;if p.len()<9{bail!("invalid lens parameters")}
    let cw=numeric(&cal,"image_width",true)?[0];let ch=numeric(&cal,"image_height",true)?[0];
    let image=image::open(&path)?.thumbnail(256,256).to_rgb8();let(w,h)=image.dimensions();
    let origin=geometry::observer_ecef(lat,lon,alt/1000.);let mut vertices=Vec::<f32>::new();let mut uv=Vec::new();
    let mut excluded_pixels=0;
    for y in 0..h {for x in 0..w {
        // Apply settings in full-image coordinates, never rebase or crop the lens.
        let rect=[x as f64/w as f64,y as f64/h as f64,(x+1) as f64/w as f64,(y+1) as f64/h as f64];
        if !crate::pixel_mask::allowed(rect,crop,&polygons){excluded_pixels+=1;continue}
        // Reduced pixel corners map to original corners x*W/w - 0.5;
        // cameraModel uses normalized coordinate (raw_pixel + 1)/W.
        let mut corners=Vec::new();for (dx,dy) in [(0,0),(1,0),(1,1),(0,1)]{
            let xn=(x+dx) as f64/w as f64+0.5/cw;let yn=(y+dy) as f64/h as f64+0.5/ch;
            let Some(enu)=ray(xn,yn,&p[1..],p[0] as i32)else{break};
            let d=geometry::az_el_direction(lat,lon,enu[0].atan2(enu[1]).to_degrees(),enu[2].clamp(-1.,1.).asin().to_degrees());
            let Some(hit)=geometry::intersect_emission_shell(origin,d,100.)else{break};
            corners.push([hit[1]/6371.,hit[2]/6371.,hit[0]/6371.]);
        }
        if corners.len()!=4{continue}let c=image.get_pixel(x,y).0;
        for i in [0,1,2,0,2,3]{vertices.extend(corners[i].map(|v|v as f32));vertices.extend(c.map(|v|v as f32/255.));let(dx,dy)=[(0,0),(1,0),(1,1),(0,1)][i];uv.extend([(x+dx) as f32/w as f32,(y+dy) as f32/h as f32]);}
    }}
    Ok(Projection{source_id:id.into(),observation_utc:utc,width:w,height:h,altitude_km:100,excluded_pixels,mask_polygon_count:polygons.len(),vertices,uv})
}

/// Content-addressed, reusable server projection plus a small frame texture.
pub fn assets(s:&AppState,id:&str,at:Option<chrono::DateTime<chrono::Utc>>)->Result<Value>{
    use sha2::{Digest,Sha256};
    let conn=db::open(&s.db_path)?;let time=at.map(|v|v.to_rfc3339());
    let (path,cal,utc,key,calibration_id):(String,String,String,String,String)=conn.query_row(
        "SELECT i.archive_path,c.hdf5_path,i.observation_utc,json_array(c.hdf5_path,s.latitude_deg,s.longitude_deg,s.altitude_m,i.width,i.height,cs.crop_json,cs.mask_json),c.id FROM sources s JOIN images i ON i.source_id=s.id JOIN calibrations c ON c.id=COALESCE((SELECT cs2.selected_calibration_id FROM camera_settings cs2 WHERE cs2.source_id=s.id AND EXISTS(SELECT 1 FROM calibrations csel WHERE csel.id=cs2.selected_calibration_id AND csel.source_id=s.id)),(SELECT cc.id FROM calibrations cc WHERE cc.source_id=s.id AND (cc.valid_from_utc IS NULL OR julianday(cc.valid_from_utc)<=julianday(i.observation_utc)) AND (cc.valid_to_utc IS NULL OR julianday(cc.valid_to_utc)>julianday(i.observation_utc)) ORDER BY julianday(cc.valid_from_utc) DESC,cc.created_utc DESC LIMIT 1)) LEFT JOIN camera_settings cs ON cs.source_id=s.id WHERE s.id=?1 AND s.enabled=1 AND (?2 IS NULL OR (julianday(i.observation_utc)<=julianday(?2) AND julianday(i.observation_utc)>=julianday(?2)-10.0/1440.0)) ORDER BY i.observation_utc DESC LIMIT 1",
        rusqlite::params![id,time],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    let cal_stamp=std::fs::metadata(cal)?.modified()?;
    let geometry_key=format!("{:x}",Sha256::digest(format!("geometry-v3-seasonal-adaptive4-256-100km:{key}:{cal_stamp:?}")));
    let texture_key=format!("{:x}",Sha256::digest(format!("texture-v1-256:{path}")));
    let dir=s.archive_root.join("projection-cache");std::fs::create_dir_all(&dir)?;
    let geometry=dir.join(format!("{geometry_key}.bin"));let texture=dir.join(format!("{texture_key}.png"));
    if !geometry.exists(){
        let p=build(s,id,at)?;let bytes=compact_geometry(&p);
        let tmp=dir.join(format!("{}.tmp",uuid::Uuid::new_v4()));std::fs::write(&tmp,bytes)?;std::fs::rename(tmp,&geometry)?;
    }
    if !texture.exists(){
        let im=image::open(&path)?.thumbnail(256,256).to_rgb8();
        let tmp=dir.join(format!("{}.tmp",uuid::Uuid::new_v4()));im.save_with_format(&tmp,image::ImageFormat::Png)?;std::fs::rename(tmp,&texture)?;
    }
    Ok(json!({"source_id":id,"observation_utc":utc,"calibration_id":calibration_id,"geometry_url":format!("/gaia/api/projection-assets/{geometry_key}.bin"),"texture_url":format!("/gaia/api/projection-assets/{texture_key}.png"),"vertex_count":std::fs::metadata(geometry)?.len()/20}))
}

/// One catalogue request replaces per-camera projection queries on every tick.
pub fn timeline(s:&AppState,id:&str)->Result<Value>{
    let mut manifest=assets(s,id,None)?;let conn=db::open(&s.db_path)?;
    let (w,h):(i64,i64)=conn.query_row("SELECT width,height FROM images WHERE source_id=?1 ORDER BY observation_utc DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let mut q=conn.prepare("SELECT id,observation_utc,width,height FROM images WHERE source_id=?1 AND julianday(observation_utc)>=julianday('now','-1 day','-10 minutes') ORDER BY observation_utc")?;
    let images=q.query_map([id],|r|Ok(json!({"at":r.get::<_,String>(1)?,"width":r.get::<_,i64>(2)?,"height":r.get::<_,i64>(3)?,"texture_url":format!("/gaia/api/images/{}/texture",r.get::<_,String>(0)?)})))?.collect::<Result<Vec<_>,_>>()?;
    manifest["images"]=json!(images);manifest["width"]=json!(w);manifest["height"]=json!(h);Ok(manifest)
}
pub fn image_texture(s:&AppState,id:&str)->Result<Vec<u8>>{
    use sha2::{Digest,Sha256};
    let conn=db::open(&s.db_path)?;let path:String=conn.query_row("SELECT archive_path FROM images WHERE id=?1",[id],|r|r.get(0))?;
    let key=format!("{:x}",Sha256::digest(format!("texture-v1-256:{path}")));
    let dir=s.archive_root.join("projection-cache");std::fs::create_dir_all(&dir)?;let dest=dir.join(format!("{key}.png"));
    if !dest.exists(){let im=image::open(path)?.thumbnail(256,256).to_rgb8();let tmp=dir.join(format!("{}.tmp",uuid::Uuid::new_v4()));im.save_with_format(&tmp,image::ImageFormat::Png)?;std::fs::rename(tmp,&dest)?;}
    Ok(std::fs::read(dest)?)
}

/// Merge only complete 4x4 blocks. Partial/masked blocks retain every original
/// pixel triangle, so no masked pixels are restored or additional pixels cut.
fn compact_geometry(p:&Projection)->Vec<u8>{
    let(w,h)=(p.width as usize,p.height as usize);let mut cells=vec![None;w*h];
    for (i,uv) in p.uv.chunks_exact(12).enumerate(){let x=(uv[0]*w as f32).round() as usize;let y=(uv[1]*h as f32).round() as usize;cells[y*w+x]=Some(i)}
    let mut out=Vec::new();
    let mut emit=|cell:usize,corner:usize|{let v=&p.vertices[cell*36+corner*6..];let uv=&p.uv[cell*12+corner*2..];for n in [v[0],v[1],v[2],uv[0],uv[1]]{out.extend_from_slice(&n.to_le_bytes())}};
    for y in (0..h).step_by(4){for x in (0..w).step_by(4){
        let(xe,ye)=((x+4).min(w),(y+4).min(h));
        let complete=(y..ye).all(|yy|(x..xe).all(|xx|cells[yy*w+xx].is_some()));
        // Bound positional error to < 0.00015 Earth radii (~0.96 km).
        // Retain full resolution wherever shell curvature exceeds this.
        let accurate=complete&&(y..ye).all(|yy|(x..xe).all(|xx|{
            let anchors=[(cells[y*w+x].unwrap(),0),(cells[y*w+xe-1].unwrap(),1),(cells[(ye-1)*w+xe-1].unwrap(),2),(cells[(ye-1)*w+x].unwrap(),5)];
            let i=cells[yy*w+xx].unwrap();(0..6).all(|k|{
                let uv=&p.uv[i*12+k*2..];let tx=(uv[0]*w as f32-x as f32)/(xe-x) as f32;let ty=(uv[1]*h as f32-y as f32)/(ye-y) as f32;
                let weights=if ty<=tx{[1.-tx,tx-ty,ty,0.]}else{[1.-ty,0.,tx,ty-tx]};
                let err:f32=(0..3).map(|axis|{let interp:f32=anchors.iter().zip(weights).map(|(&(i,k),weight)|p.vertices[i*36+k*6+axis]*weight).sum();(interp-p.vertices[i*36+k*6+axis]).powi(2)}).sum();err<=0.00015f32.powi(2)
            })
        }));
        if accurate{
            let a=cells[y*w+x].unwrap();let b=cells[y*w+xe-1].unwrap();let c=cells[(ye-1)*w+xe-1].unwrap();let d=cells[(ye-1)*w+x].unwrap();
            for (cell,corner) in [(a,0),(b,1),(c,2),(a,0),(c,2),(d,5)]{emit(cell,corner)}
        }else{for yy in y..ye{for xx in x..xe{if let Some(i)=cells[yy*w+xx]{for k in 0..6{emit(i,k)}}}}}
    }}out
}

#[cfg(test)]
mod mesh_tests{
    use super::*;
    #[test]
    fn ucalgary_aida_fit_reproduces_held_out_directions(){
        // Fit from SMILE KLUN's 2026-08-20 FULL_AZIMUTH/FULL_ELEVATION map.
        // These samples were not all members of the sparse fitting subset.
        let p=[-0.28888768847718865,0.288887577882544,0.365533848283011,-0.22368438903771679,152.35441605514723,-0.003628028658245223,-0.014281289421420368,1.0020226518531798];
        let samples:[(f64,f64,f64,f64);4]=[(252.,248.,213.8751983642578,89.84917449951172),(256.,128.,29.42223358154297,43.61399459838867),(306.,256.,126.4842529296875,68.7739028930664),(256.,384.,206.1214599609375,37.153343200683594)];
        for (x,y,az,el) in samples{
            let enu=ray((x+1.)/512.,(y+1.)/512.,&p,4).unwrap();
            let expected=[el.to_radians().cos()*az.to_radians().sin(),el.to_radians().cos()*az.to_radians().cos(),el.to_radians().sin()];
            let error=enu.iter().zip(expected).map(|(a,b)|a*b).sum::<f64>().clamp(-1.,1.).acos().to_degrees();
            assert!(error<0.15,"pixel ({x},{y}) differs by {error} degrees");
        }
    }
    #[test]
    fn compaction_preserves_mask_holes(){
        for hole in [false,true]{
            let mut p=Projection{source_id:String::new(),observation_utc:String::new(),width:4,height:4,altitude_km:100,excluded_pixels:0,mask_polygon_count:0,vertices:vec![],uv:vec![]};
            for y in 0..4{for x in 0..4{if hole&&x==1&&y==1{continue}for(dx,dy)in[(0,0),(1,0),(1,1),(0,0),(1,1),(0,1)]{let u=(x+dx)as f32/4.;let v=(y+dy)as f32/4.;p.vertices.extend([u,v,0.,1.,1.,1.]);p.uv.extend([u,v]);}}}
            let bytes=compact_geometry(&p);assert_eq!(bytes.len(),if hole{15*6*20}else{6*20});
            let v:Vec<f32>=bytes.chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).collect();
            let area:f32=v.chunks_exact(15).map(|t|((t[5]-t[0])*(t[11]-t[1])-(t[10]-t[0])*(t[6]-t[1])).abs()/2.).sum();
            assert_eq!(area,if hole{15./16.}else{1.});
        }
    }
}
