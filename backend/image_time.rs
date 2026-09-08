//! Conservative OCR of explicitly UTC, full-date camera overlays.
use chrono::{DateTime,NaiveDateTime,Utc};
use std::io::{Cursor,Write};
use std::process::{Command,Stdio};

pub fn parse(text:&str,downloaded:DateTime<Utc>)->Option<(DateTime<Utc>,&'static str)>{
    let re=regex::Regex::new(r"(?i)(\d{4}-\d{2}-\d{2})\s+(\d{2}:\d{2})(:\d{2})?\s+UTC\b").ok()?;
    let mut result=None;
    for c in re.captures_iter(text){
        let seconds=c.get(3).map(|v|v.as_str()).unwrap_or(":00");
        let raw=format!("{} {}{}",&c[1],&c[2],seconds);
        let t=NaiveDateTime::parse_from_str(&raw,"%Y-%m-%d %H:%M:%S").ok()?.and_utc();
        if t>downloaded+chrono::Duration::minutes(2)||t<downloaded-chrono::Duration::days(7){return None}
        if result.as_ref().is_some_and(|(previous,_)|*previous!=t){return None}
        result=Some((t,if c.get(3).is_some(){"image_ocr_utc_second"}else{"image_ocr_utc_minute"}));
    }
    result
}

pub fn read(bytes:&[u8],downloaded:DateTime<Utc>)->Option<(DateTime<Utc>,&'static str)>{
    let image=image::load_from_memory(bytes).ok()?.to_luma8();
    let strip=(image.height()/6).max(1);let mut canvas=image::GrayImage::new(image.width(),strip*2);
    image::imageops::replace(&mut canvas,&image::imageops::crop_imm(&image,0,0,image.width(),strip).to_image(),0,0);
    image::imageops::replace(&mut canvas,&image::imageops::crop_imm(&image,0,image.height()-strip,image.width(),strip).to_image(),0,strip as i64);
    let scale=1600.0/canvas.width() as f64;
    let canvas=image::imageops::resize(&canvas,1600,(canvas.height() as f64*scale).round() as u32,image::imageops::FilterType::Triangle);
    let mut png=Cursor::new(Vec::new());image::DynamicImage::ImageLuma8(canvas).write_to(&mut png,image::ImageFormat::Png).ok()?;
    let mut child=Command::new("timeout").args(["8","tesseract","stdin","stdout","--psm","11"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    child.stdin.take()?.write_all(png.get_ref()).ok()?;
    let output=child.wait_with_output().ok()?;if !output.status.success(){return None}
    parse(&String::from_utf8_lossy(&output.stdout),downloaded)
}

#[cfg(test)] mod tests{
    use super::*;
    #[test]fn explicit_utc_only(){let now="2026-09-08T20:18:00Z".parse().unwrap();
        assert_eq!(parse("IRF KIRUNA 2026-09-08 20:17 UTC",now).unwrap().0.to_rfc3339(),"2026-09-08T20:17:00+00:00");
        assert!(parse("2026-09-08 20:17",now).is_none());
        assert!(parse("2026-09-08 23:17 UTC",now).is_none());
        assert!(parse("2026-02-31 20:17 UTC",now).is_none());
        assert!(parse("2026-09-08 20:17 UTC 2026-09-08 19:17 UTC",now).is_none());
    }
}
