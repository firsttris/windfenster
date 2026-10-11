use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The UTC date of a Unix time as a number yyyymmdd, e.g. 20261011.
pub fn yyyymmdd(unix: i64) -> u32 {
    // Howard Hinnant's civil_from_days
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y * 10_000 + m * 100 + d) as u32
}

/// Unix time of a UTC date and time.
pub fn unix(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> i64 {
    // Howard Hinnant's days_from_civil
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second)
}

/// Parses an ISO 8601 UTC time, e.g. `2026-10-11T08:50:00.000Z` or `2026-10-11T08:50:00Z`.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim().strip_suffix('Z').unwrap_or(s.trim());
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let time = time.split(['+', '.']).next()?;
    let mut t = time.split(':').map(|x| x.parse::<u32>().ok());
    let (h, mi) = (t.next()??, t.next()??);
    let sec = t.next().flatten().unwrap_or(0);
    Some(unix(y, mo as u32, da as u32, h, mi, sec))
}

/// Parses DWD's `yyyymmddhhmm` (UTC).
pub fn parse_compact(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() != 12 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s[r].parse::<u32>().ok();
    Some(unix(
        i64::from(n(0..4)?),
        n(4..6)?,
        n(6..8)?,
        n(8..10)?,
        n(10..12)?,
        0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_times() {
        assert_eq!(unix(2026, 10, 11, 0, 0, 0), 1_791_676_800);
        assert_eq!(unix(2000, 2, 29, 0, 0, 0), 951_782_400);
        assert_eq!(parse_iso("2026-10-11T08:50:00.000Z"), Some(1_791_708_600));
        assert_eq!(parse_iso("2026-10-11T08:50:00Z"), Some(1_791_708_600));
        assert_eq!(parse_iso("2026-10-11"), None);
        assert_eq!(parse_compact("202610110850"), Some(1_791_708_600));
        assert_eq!(parse_compact("20261011085"), None);
    }

    #[test]
    fn dates() {
        assert_eq!(yyyymmdd(0), 19700101);
        assert_eq!(yyyymmdd(1_791_676_800), 20261011);
        assert_eq!(yyyymmdd(951_782_400), 20000229);
        assert_eq!(yyyymmdd(-86_400), 19691231);
    }
}
