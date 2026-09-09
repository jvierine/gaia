//! Conservative OCR of explicitly UTC, full-date camera overlays.
use chrono::{DateTime,NaiveDateTime,Utc};

/// Optional local OCR. Bounded concurrency and runtime; never a network service.
/// File input avoids broken stdin pipes; Tokio kills/reaps timed-out children.
pub async fn read(bytes: &[u8], downloaded: DateTime<Utc>, root: &std::path::Path)
    -> Option<(DateTime<Utc>, &'static str)> {
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let _permit = SLOTS.acquire().await.ok()?;
    let scratch = root.join("ocr-tmp");
    std::fs::create_dir_all(&scratch).ok()?;
    let bytes = bytes.to_vec();
    let input = tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&bytes).ok()?.thumbnail(1600, 1600).grayscale();
        let file = tempfile::Builder::new().suffix(".png").tempfile_in(scratch).ok()?;
        image.save_with_format(file.path(), image::ImageFormat::Png).ok()?;
        Some(file)
    }).await.ok()??;
    let mut command = tokio::process::Command::new("tesseract");
    command.arg(input.path()).args(["stdout", "--psm", "11", "-l", "eng"])
        .env("OMP_THREAD_LIMIT", "1").kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(8), command.output()).await.ok()?.ok()?;
    if !output.status.success() { return None; }
    parse(&String::from_utf8_lossy(&output.stdout), downloaded)
}

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
