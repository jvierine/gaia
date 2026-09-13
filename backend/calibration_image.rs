//! Calibration-only image copy. Never crop/rebase the lens coordinate system.
use anyhow::{Context,Result,bail};
use serde_json::Value;
pub fn render(bytes:&[u8],crop:Option<&str>,mask:Option<&str>,enabled:bool)->Result<Vec<u8>>{
 let crop=match crop {Some(s)=>{let v:Value=serde_json::from_str(s)?;[v["left"].as_f64().context("crop left")?,v["top"].as_f64().context("crop top")?,v["right"].as_f64().context("crop right")?,v["bottom"].as_f64().context("crop bottom")?]},None=>[0.,0.,1.,1.]};
 let polygons:Vec<Vec<[f64;2]>>=if enabled {if let Some(s)=mask {let v:Value=serde_json::from_str(s)?;if v["coordinate_system"]!="normalized_image"{bail!("unsupported mask coordinates")}serde_json::from_value(v["polygons"].clone())?}else{vec![]}}else{vec![]};
 let mut im=image::load_from_memory(bytes)?.to_rgb8();let(w,h)=im.dimensions();
 for (x,y,pixel) in im.enumerate_pixels_mut(){
  let rect=[x as f64/w as f64,y as f64/h as f64,(x+1) as f64/w as f64,(y+1) as f64/h as f64];
  if !crate::pixel_mask::allowed(rect,crop,&polygons){*pixel=image::Rgb([0,0,0])}
 }
 let mut out=std::io::Cursor::new(Vec::new());im.write_to(&mut out,image::ImageFormat::Png)?;Ok(out.into_inner())
}
#[cfg(test)]mod tests{
 use super::*;
 fn input()->Vec<u8>{let im=image::RgbImage::from_pixel(20,10,image::Rgb([51,102,153]));let mut b=std::io::Cursor::new(Vec::new());im.write_to(&mut b,image::ImageFormat::Png).unwrap();b.into_inner()}
 #[test]fn black_crop_and_multiple_masks_preserve_coordinates(){
  let mask=r#"{"coordinate_system":"normalized_image","polygons":[[[0.3,0.2],[0.5,0.2],[0.5,0.8],[0.3,0.8]],[[0.7,0.2],[1.2,0.2],[1.2,0.8],[0.7,0.8]]]}"#;
  let raw=input();let out=render(&raw,Some(r#"{"left":0.1,"top":0,"right":0.9,"bottom":1}"#),Some(mask),true).unwrap();
  let im=image::load_from_memory(&out).unwrap().to_rgb8();assert_eq!(im.dimensions(),(20,10));
  for p in [(0,5),(19,5),(8,5),(16,5)]{assert_eq!(im.get_pixel(p.0,p.1).0,[0,0,0])}
  assert_eq!(im.get_pixel(12,5).0,[51,102,153]);assert_eq!(input(),raw);
  let off=image::load_from_memory(&render(&raw,None,Some(mask),false).unwrap()).unwrap().to_rgb8();assert_eq!(off.get_pixel(8,5).0,[51,102,153]);
 }
 #[test]fn no_settings_preserves_all_pixels(){let raw=input();let out=render(&raw,None,None,true).unwrap();assert_eq!(image::load_from_memory(&raw).unwrap().to_rgb8(),image::load_from_memory(&out).unwrap().to_rgb8());}
}
