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

#[cfg(test)]
mod tests {
    use super::yyyymmdd;

    #[test]
    fn dates() {
        assert_eq!(yyyymmdd(0), 19700101);
        assert_eq!(yyyymmdd(1_791_676_800), 20261011);
        assert_eq!(yyyymmdd(951_782_400), 20000229);
        assert_eq!(yyyymmdd(-86_400), 19691231);
    }
}
