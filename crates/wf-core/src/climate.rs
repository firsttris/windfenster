//! A station's wind over the years: hourly measurements folded into a rose and the windy days
//! per month. Kept as that summary only (a few KB a station), never as the hours themselves.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Lower edges of the speed bands in kn, as the app's wind scale (`web/src/ui/wind-scale.ts`)
pub const BANDS_KN: [f32; 8] = [0.0, 5.0, 9.0, 13.0, 17.0, 21.0, 25.0, 30.0];
/// Compass sectors of the rose, from north clockwise
pub const SECTORS: usize = 16;
/// Below this, or with no direction, an hour counts as calm or variable, outside the rose
pub const CALM_KN: f32 = 1.0;
/// A day is windy with at least `WINDY_HOURS` daylight hours of at least this mean wind, as the
/// alarms reckon a window
pub const WINDY_KN: f32 = 12.0;
pub const WINDY_HOURS: u32 = 2;
/// Daylight by sun time at the station (UTC plus longitude / 15°): from 9 to before 19 o'clock
const DAY_FROM: i64 = 9;
const DAY_TO: i64 = 19;

/// One measurement: when (Unix seconds), mean wind in kn, where it comes from (degrees, None
/// when variable)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub at: i64,
    pub kn: f32,
    pub dir: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MonthStats {
    /// Hours with a measurement
    pub hours: u32,
    /// Hours calm (below 1 kn) or of variable direction
    pub calm: u32,
    /// Hours per sector (16, from north clockwise) and speed band (8, as `BANDS_KN`)
    pub rose: Vec<Vec<u32>>,
    /// Days with measurements in daylight, and those of them that were windy
    pub days: u32,
    pub windy_days: u32,
}

/// `GET /api/stations/{slug}/stats`: a station's wind over whole years
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindStats {
    pub first_year: i32,
    pub last_year: i32,
    /// Where the hours came from: "iem" (METAR archive) or "dwd" (DWD climate archive)
    pub source: String,
    /// January to December
    pub months: Vec<MonthStats>,
}

fn empty_month() -> MonthStats {
    MonthStats {
        hours: 0,
        calm: 0,
        rose: vec![vec![0; BANDS_KN.len()]; SECTORS],
        days: 0,
        windy_days: 0,
    }
}

/// The sector of a direction the wind comes from: 0 is north (348.75° to 11.25°)
pub fn sector(dir: f32) -> usize {
    let step = 360.0 / SECTORS as f32;
    (((dir.rem_euclid(360.0) + step / 2.0) / step) as usize) % SECTORS
}

/// The band of a speed in kn
pub fn band(kn: f32) -> usize {
    BANDS_KN.iter().rposition(|&b| kn >= b).unwrap_or(0)
}

/// The month (0–11) and the sun-time day and hour of a moment at a longitude
fn local(at: i64, lon: f64) -> (usize, i64, i64) {
    let solar = at + (lon / 15.0 * 3_600.0).round() as i64;
    let day = solar.div_euclid(86_400);
    let hour = solar.rem_euclid(86_400) / 3_600;
    let month = (crate::time::yyyymmdd(day * 86_400) / 100 % 100) as usize - 1;
    (month, day, hour)
}

/// Folds measurements (any order, any spacing) into whole-year statistics for a station at
/// `lon`: one value per hour (the last of each), so half-hourly METARs and hourly archive values
/// weigh alike.
pub fn summarize(
    samples: &[Sample],
    lon: f64,
    source: &str,
    first_year: i32,
    last_year: i32,
) -> WindStats {
    let mut hourly: std::collections::BTreeMap<i64, Sample> = std::collections::BTreeMap::new();
    for s in samples {
        if s.kn.is_finite() && s.kn >= 0.0 {
            hourly.insert(s.at.div_euclid(3_600), *s);
        }
    }
    let mut months: Vec<MonthStats> = (0..12).map(|_| empty_month()).collect();
    // Per sun-time day: its month, daylight hours measured, windy daylight hours
    let mut days: std::collections::BTreeMap<i64, (usize, u32, u32)> =
        std::collections::BTreeMap::new();
    for s in hourly.values() {
        let (month, day, hour) = local(s.at, lon);
        let m = &mut months[month];
        m.hours += 1;
        match s.dir {
            Some(dir) if s.kn >= CALM_KN => m.rose[sector(dir)][band(s.kn)] += 1,
            _ => m.calm += 1,
        }
        if (DAY_FROM..DAY_TO).contains(&hour) {
            let d = days.entry(day).or_insert((month, 0, 0));
            d.1 += 1;
            if s.kn >= WINDY_KN {
                d.2 += 1;
            }
        }
    }
    for (month, measured, windy) in days.into_values() {
        // A day counts once half its daylight hours are measured
        if measured >= ((DAY_TO - DAY_FROM) / 2) as u32 {
            months[month].days += 1;
            if windy >= WINDY_HOURS {
                months[month].windy_days += 1;
            }
        }
    }
    WindStats {
        first_year,
        last_year,
        source: source.to_owned(),
        months,
    }
}

/// The last whole year at `now`: the year before the current one (UTC)
pub fn last_whole_year(now: i64) -> i32 {
    (crate::time::yyyymmdd(now) / 10_000) as i32 - 1
}

/// What is stored for a station: the statistics (None when its archive had no hours), the last
/// whole year they reach to and when they were made
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
    pub stats: Option<WindStats>,
    pub last_year: i32,
    pub computed_at: i64,
}

pub async fn stored(db: &sqlx::SqlitePool, station_id: i64) -> sqlx::Result<Option<Stored>> {
    let row: Option<(i32, i64, Option<String>)> =
        sqlx::query_as("SELECT last_year, computed_at, stats FROM wind_stats WHERE station_id = ?")
            .bind(station_id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(last_year, computed_at, stats)| Stored {
        // A summary that no longer reads (an older shape) counts as none: it is made anew
        stats: stats.and_then(|s| serde_json::from_str(&s).ok()),
        last_year,
        computed_at,
    }))
}

pub async fn store(db: &sqlx::SqlitePool, station_id: i64, s: &Stored) -> sqlx::Result<()> {
    let json = s
        .stats
        .as_ref()
        .map(|w| serde_json::to_string(w).unwrap_or_default());
    sqlx::query(
        "INSERT INTO wind_stats (station_id, last_year, computed_at, stats) VALUES (?, ?, ?, ?) \
         ON CONFLICT (station_id) DO UPDATE SET last_year = excluded.last_year, \
         computed_at = excluded.computed_at, stats = excluded.stats",
    )
    .bind(station_id)
    .bind(s.last_year)
    .bind(s.computed_at)
    .bind(json)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2024-07-01 00:00 UTC
    const JULY: i64 = 1_719_792_000;
    const H: i64 = 3_600;

    #[test]
    fn sectors_and_bands() {
        assert_eq!(
            [0.0, 11.0, 11.5, 90.0, 180.0, 349.0, 360.0].map(sector),
            [0, 0, 1, 4, 8, 0, 0]
        );
        assert_eq!(
            [0.0, 4.9, 5.0, 12.9, 13.0, 29.9, 30.0, 55.0].map(band),
            [0, 0, 1, 2, 3, 6, 7, 7]
        );
    }

    #[test]
    fn folds_hours_into_the_rose_and_the_windy_days() {
        let mut samples = Vec::new();
        // 1 July, at 8° E (sun time UTC + 32 min): every half hour 15 kn from 230° from 10 to 16
        // UTC, calm the rest of the day
        for half in 0..48 {
            let at = JULY + half * 1_800;
            let hour = half / 2;
            let windy = (10..16).contains(&hour);
            samples.push(Sample {
                at,
                kn: if windy { 15.0 } else { 0.5 },
                dir: Some(230.0),
            });
        }
        // 2 July: 20 kn from the west at 12 UTC only, else 8 kn from the north
        for hour in 0..24 {
            samples.push(Sample {
                at: JULY + 86_400 + hour * H,
                kn: if hour == 12 { 20.0 } else { 8.0 },
                dir: Some(if hour == 12 { 270.0 } else { 0.0 }),
            });
        }
        // A variable direction counts as calm
        samples.push(Sample {
            at: JULY + 3 * 86_400 + 12 * H,
            kn: 6.0,
            dir: None,
        });
        let stats = summarize(&samples, 8.0, "iem", 2016, 2025);
        let july = &stats.months[6];
        // One value per hour: 24 + 24 + 1
        assert_eq!(july.hours, 49);
        // 18 calm hours on day 1, the variable hour on day 4
        assert_eq!(july.calm, 19);
        assert_eq!(july.rose[sector(230.0)][band(15.0)], 6);
        assert_eq!(july.rose[sector(270.0)][band(20.0)], 1);
        assert_eq!(july.rose[0][band(8.0)], 23);
        // Day 1 windy (six hours), day 2 not (one), day 4 too thin to count
        assert_eq!((july.days, july.windy_days), (2, 1));
        assert!(
            stats
                .months
                .iter()
                .enumerate()
                .all(|(i, m)| i == 6 || m.hours == 0)
        );
    }

    #[tokio::test]
    async fn stores_and_reads() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        sqlx::query(
            "INSERT INTO station (id, slug, source, code, name, lat, lon, seen_at) \
             VALUES (1, 'edsb', 'metar', 'EDSB', 'Baden', 48.78, 8.08, 0)",
        )
        .execute(&db)
        .await?;
        assert_eq!(stored(&db, 1).await?, None);
        let made = Stored {
            stats: Some(summarize(
                &[Sample {
                    at: JULY,
                    kn: 14.0,
                    dir: Some(250.0),
                }],
                8.0,
                "iem",
                2016,
                2025,
            )),
            last_year: 2025,
            computed_at: JULY,
        };
        store(&db, 1, &made).await?;
        assert_eq!(stored(&db, 1).await?, Some(made));
        let none = Stored {
            stats: None,
            last_year: 2025,
            computed_at: JULY + 1,
        };
        store(&db, 1, &none).await?;
        assert_eq!(stored(&db, 1).await?, Some(none));
        assert_eq!(last_whole_year(JULY), 2023);
        Ok(())
    }

    #[test]
    fn days_by_sun_time() {
        // 23:30 UTC at 170° E is 10:50 the next day by the sun
        let s = Sample {
            at: JULY + 23 * H + 1_800,
            kn: 14.0,
            dir: Some(90.0),
        };
        let (month, day, hour) = local(s.at, 170.0);
        assert_eq!((month, day, hour), (6, JULY / 86_400 + 1, 10));
    }
}
