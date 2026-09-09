//! Conservative OCR of explicitly UTC, full-date camera overlays.
use chrono::{DateTime,NaiveDateTime,Utc};

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
