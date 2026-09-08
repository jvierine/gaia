//! AIDA raw-pixel inverse and straight-ray, spherical 100 km shell projection.
use anyhow::{Context, Result, bail};
use serde_json::{Value,json};
use crate::{db,geometry,AppState};
#[derive(serde::Serialize)]
pub struct Projection {
    pub source_id:String,pub observation_utc:String,pub width:u32,pub height:u32,
    pub altitude_km:u32,pub excluded_pixels:u32,pub mask_polygon_count:usize,pub vertices:Vec<f32>,
}

fn numeric(path:&str, name:&str, attr:bool)->Result<Vec<f64>> {
    let out=std::process::Command::new("h5dump").args(["-y","-w","0","-m","%0.17g",if attr{"-a"}else{"-d"},name,path]).output()?;
    if !out.status.success(){bail!("cannot read calibration {name}")}
    let text=String::from_utf8(out.stdout)?;
    let data=text.split("DATA {").nth(1).context("missing calibration data")?.split('}').next().unwrap();
    data.split(',').map(|v|Ok(v.trim().parse()?)).collect()
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
    let (path,cal,lat,lon,alt,utc):(String,String,f64,f64,f64,String)=conn.query_row("SELECT i.archive_path,c.hdf5_path,s.latitude_deg,s.longitude_deg,COALESCE(s.altitude_m,0),i.observation_utc FROM sources s JOIN images i ON i.source_id=s.id JOIN calibrations c ON c.source_id=s.id WHERE s.id=?1 AND s.enabled=1 AND (?2 IS NULL OR (julianday(i.observation_utc)<=julianday(?2) AND julianday(i.observation_utc)>=julianday(?2)-10.0/1440.0)) ORDER BY c.created_utc DESC,i.observation_utc DESC LIMIT 1",rusqlite::params![id,at],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let p=numeric(&cal,"wisc_optpar_with_optmod",false)?;if p.len()<9{bail!("invalid lens parameters")}
    let cw=numeric(&cal,"image_width",true)?[0];let ch=numeric(&cal,"image_height",true)?[0];
    let image=image::open(&path)?.thumbnail(256,256).to_rgb8();let(w,h)=image.dimensions();
    let origin=geometry::observer_ecef(lat,lon,alt/1000.);let mut vertices=Vec::<f32>::new();
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
        for i in [0,1,2,0,2,3]{vertices.extend(corners[i].map(|v|v as f32));vertices.extend(c.map(|v|v as f32/255.));}
    }}
    Ok(Projection{source_id:id.into(),observation_utc:utc,width:w,height:h,altitude_km:100,excluded_pixels,mask_polygon_count:polygons.len(),vertices})
}
