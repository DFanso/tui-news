use chrono::{DateTime, Utc};

pub fn relative(ts: i64, now: i64) -> String {
    let delta = now.saturating_sub(ts);
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;

    match delta {
        d if d < MINUTE => "now".into(),
        d if d < HOUR => format!("{}m", d / MINUTE),
        d if d < DAY => format!("{}h", d / HOUR),
        d if d < 30 * DAY => format!("{}d", d / DAY),
        _ => DateTime::<Utc>::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| ts.to_string()),
    }
}

pub fn wire_clock(now: DateTime<Utc>) -> String {
    now.format("%a %d %b %Y  %H:%M").to_string().to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_buckets() {
        let now = 1_700_000_000;
        assert_eq!(relative(now - 10, now), "now");
        assert_eq!(relative(now - 5 * 60, now), "5m");
        assert_eq!(relative(now - 3 * 3600, now), "3h");
        assert_eq!(relative(now - 2 * 86400, now), "2d");
    }

    #[test]
    fn future_is_now() {
        assert_eq!(relative(100, 50), "now");
    }
}
