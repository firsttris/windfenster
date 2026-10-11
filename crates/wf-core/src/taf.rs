//! Airport forecasts (TAF): the change groups as aviationweather.gov decodes them, and the
//! hourly series they add up to.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

/// How a group relates to what comes before it (ICAO Annex 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum TafChange {
    /// The conditions at the start of the validity
    Base,
    /// FM: from this time on, everything as given
    From,
    /// BECMG: changing between `from` and `by`, then lasting
    Becoming,
    /// TEMPO: at times, each time for less than an hour
    Tempo,
    /// PROB30/40 without TEMPO: possible for the whole period
    Prob,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TafCloud {
    /// FEW, SCT, BKN, OVC, NSC …
    pub cover: String,
    pub base_ft: Option<f32>,
    /// CB or TCU
    pub kind: Option<String>,
}

/// One group of a TAF. Wind and gusts in knots; a group without wind leaves it unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TafGroup {
    pub change: TafChange,
    /// PROB30 / PROB40
    pub probability: Option<u8>,
    #[ts(type = "number")]
    pub from: i64,
    #[ts(type = "number")]
    pub to: i64,
    /// BECMG: the change is complete by then.
    #[ts(type = "number | null")]
    pub by: Option<i64>,
    /// Where the wind comes from; null when variable or not given
    pub dir: Option<f32>,
    pub variable: bool,
    pub wind: Option<f32>,
    pub gust: Option<f32>,
    /// Metres; 9999 for 10 km or more
    pub visibility_m: Option<f32>,
    /// Weather as coded, e.g. "-SHRA", "BCFG", "NSW"
    pub weather: Option<String>,
    pub clouds: Vec<TafCloud>,
}

/// The forecast for one hour. `low`/`high` span what TEMPO, PROB and a BECMG in progress allow
/// beside the prevailing wind; they are set only when that span is wider than `wind`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TafHour {
    #[ts(type = "number")]
    pub at: i64,
    pub wind: Option<f32>,
    pub gust: Option<f32>,
    pub dir: Option<f32>,
    pub low: Option<f32>,
    pub high: Option<f32>,
    /// Highest gust of the hour, TEMPO and PROB included
    pub gust_high: Option<f32>,
    /// Set when the span comes only from PROB groups
    pub probability: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Taf {
    #[ts(type = "number")]
    pub issued_at: i64,
    #[ts(type = "number")]
    pub valid_from: i64,
    #[ts(type = "number")]
    pub valid_to: i64,
    pub raw: String,
    pub groups: Vec<TafGroup>,
    pub hours: Vec<TafHour>,
    #[ts(type = "number")]
    pub fetched_at: i64,
}

impl Taf {
    pub fn new(
        issued_at: i64,
        valid_from: i64,
        valid_to: i64,
        raw: String,
        groups: Vec<TafGroup>,
        fetched_at: i64,
    ) -> Self {
        let hours = resolve(&groups, valid_from, valid_to);
        Self {
            issued_at,
            valid_from,
            valid_to,
            raw,
            groups,
            hours,
            fetched_at,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Wind {
    dir: Option<f32>,
    wind: f32,
    gust: Option<f32>,
}

fn wind_of(g: &TafGroup) -> Option<Wind> {
    g.wind.map(|wind| Wind {
        dir: if g.variable || wind == 0.0 {
            None
        } else {
            g.dir
        },
        wind,
        gust: g.gust,
    })
}

fn max(a: Option<f32>, b: Option<f32>) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        _ => a.or(b),
    }
}

/// The hourly series of a TAF, one value per full hour in [valid_from, valid_to). The prevailing
/// wind follows the base group, FM and BECMG in order; during a BECMG the new wind holds from
/// the middle of the transition on, and both winds make up the span. TEMPO and PROB groups only
/// widen the span.
pub fn resolve(groups: &[TafGroup], valid_from: i64, valid_to: i64) -> Vec<TafHour> {
    let first = valid_from + (3_600 - valid_from.rem_euclid(3_600)) % 3_600;
    (first..valid_to)
        .step_by(3_600)
        .map(|at| {
            let mut cur: Option<Wind> = None;
            let mut span: Vec<f32> = Vec::new();
            for g in groups {
                let Some(w) = wind_of(g) else { continue };
                match g.change {
                    TafChange::Base | TafChange::From if g.from <= at => cur = Some(w),
                    TafChange::Becoming if g.from <= at => {
                        let by = g.by.unwrap_or(g.from);
                        if at >= by {
                            cur = Some(w);
                        } else {
                            if let Some(old) = cur {
                                span.extend([old.wind, w.wind]);
                            }
                            if at >= g.from + (by - g.from) / 2 {
                                cur = Some(w);
                            }
                        }
                    }
                    _ => {}
                }
            }
            let mut gust_high = cur.and_then(|w| w.gust);
            let mut tempo = false;
            let mut probability: Option<u8> = None;
            for g in groups.iter().filter(|g| {
                matches!(g.change, TafChange::Tempo | TafChange::Prob) && g.from <= at && at < g.to
            }) {
                if let Some(w) = wind_of(g) {
                    span.push(w.wind);
                    gust_high = max(gust_high, w.gust);
                    match g.probability {
                        Some(p) if !tempo => {
                            probability = Some(probability.map_or(p, |q| q.max(p)))
                        }
                        Some(_) => {}
                        None => {
                            tempo = true;
                            probability = None;
                        }
                    }
                }
            }
            if let Some(w) = cur {
                span.push(w.wind);
            }
            let low = span.iter().copied().reduce(f32::min);
            let high = span.iter().copied().reduce(f32::max);
            let wide = low != high;
            TafHour {
                at,
                wind: cur.map(|w| w.wind),
                gust: cur.and_then(|w| w.gust),
                dir: cur.and_then(|w| w.dir),
                low: low.filter(|_| wide),
                high: high.filter(|_| wide),
                gust_high,
                probability: probability.filter(|_| wide),
            }
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct Row {
    issued_at: i64,
    valid_from: i64,
    valid_to: i64,
    raw: String,
    groups: String,
    fetched_at: i64,
}

impl Row {
    fn into_taf(self) -> Option<Taf> {
        let groups: Vec<TafGroup> = serde_json::from_str(&self.groups).ok()?;
        Some(Taf::new(
            self.issued_at,
            self.valid_from,
            self.valid_to,
            self.raw,
            groups,
            self.fetched_at,
        ))
    }
}

/// Stores TAFs; the same station and issue time again replaces the row (fetched again).
pub async fn store(db: &SqlitePool, tafs: &[(i64, Taf)]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    for (station_id, t) in tafs {
        let groups = serde_json::to_string(&t.groups).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            "INSERT OR REPLACE INTO taf (station_id, issued_at, valid_from, valid_to, raw, groups, \
             fetched_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(station_id)
        .bind(t.issued_at)
        .bind(t.valid_from)
        .bind(t.valid_to)
        .bind(&t.raw)
        .bind(groups)
        .bind(t.fetched_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// The newest TAF of a station that is still valid at `now`.
pub async fn current(db: &SqlitePool, station_id: i64, now: i64) -> sqlx::Result<Option<Taf>> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT issued_at, valid_from, valid_to, raw, groups, fetched_at FROM taf \
         WHERE station_id = ? AND valid_to > ? ORDER BY issued_at DESC LIMIT 1",
    )
    .bind(station_id)
    .bind(now)
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(Row::into_taf))
}

/// When the station's TAF was last fetched, if ever.
pub async fn last_fetch(db: &SqlitePool, station_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT MAX(fetched_at) FROM taf WHERE station_id = ?")
        .bind(station_id)
        .fetch_one(db)
        .await
}

/// Drops TAFs whose validity ended before `before`.
pub async fn purge(db: &SqlitePool, before: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM taf WHERE valid_to < ?")
        .bind(before)
        .execute(db)
        .await?
        .rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600;
    /// 2026-10-11 06:00 UTC
    const T0: i64 = 1_791_698_400;

    fn group(change: TafChange, from: i64, to: i64, wind: Option<(f32, f32)>) -> TafGroup {
        TafGroup {
            change,
            probability: None,
            from,
            to,
            by: None,
            dir: wind.map(|w| w.0),
            variable: false,
            wind: wind.map(|w| w.1),
            gust: None,
            visibility_m: None,
            weather: None,
            clouds: Vec::new(),
        }
    }

    #[test]
    fn base_and_becoming() {
        // TAF EDSB 110500Z 1106/1206 22007KT CAVOK BECMG 1117/1120 VRB03KT (as decoded by
        // aviationweather.gov: the BECMG group runs 17–06 and is complete by 20).
        let mut becmg = group(
            TafChange::Becoming,
            T0 + 11 * H,
            T0 + 24 * H,
            Some((0.0, 3.0)),
        );
        becmg.variable = true;
        becmg.by = Some(T0 + 14 * H);
        let groups = [
            group(TafChange::Base, T0, T0 + 11 * H, Some((220.0, 7.0))),
            becmg,
        ];
        let hours = resolve(&groups, T0, T0 + 24 * H);
        assert_eq!(hours.len(), 24);
        let at = |h: usize| &hours[h];
        assert_eq!(
            (at(0).wind, at(0).dir, at(0).low),
            (Some(7.0), Some(220.0), None)
        );
        // 17 UTC: the change has begun, the old wind still prevails.
        assert_eq!(
            (at(11).wind, at(11).low, at(11).high),
            (Some(7.0), Some(3.0), Some(7.0))
        );
        // From the middle (18:30) on the new wind prevails; variable has no direction.
        assert_eq!((at(13).wind, at(13).dir), (Some(3.0), None));
        assert_eq!((at(14).wind, at(14).low), (Some(3.0), None));
        assert_eq!(at(23).wind, Some(3.0));
    }

    #[test]
    fn tempo_and_prob_widen_the_span() {
        let mut tempo = group(
            TafChange::Tempo,
            T0 + 2 * H,
            T0 + 6 * H,
            Some((230.0, 18.0)),
        );
        tempo.gust = Some(30.0);
        let mut prob = group(
            TafChange::Prob,
            T0 + 8 * H,
            T0 + 10 * H,
            Some((240.0, 25.0)),
        );
        prob.probability = Some(30);
        let mut base = group(TafChange::Base, T0, T0 + 12 * H, Some((220.0, 14.0)));
        base.gust = Some(24.0);
        // A TEMPO without wind (only clouds) changes nothing.
        let clouds = group(TafChange::Tempo, T0, T0 + 12 * H, None);
        let hours = resolve(&[base, tempo, prob, clouds], T0, T0 + 12 * H);
        assert_eq!(
            (hours[1].wind, hours[1].low, hours[1].gust_high),
            (Some(14.0), None, Some(24.0))
        );
        let t = &hours[2];
        assert_eq!(
            (t.wind, t.gust, t.low, t.high, t.gust_high, t.probability),
            (
                Some(14.0),
                Some(24.0),
                Some(14.0),
                Some(18.0),
                Some(30.0),
                None
            )
        );
        assert_eq!(hours[6].high, None);
        let p = &hours[8];
        assert_eq!((p.high, p.probability), (Some(25.0), Some(30)));
    }

    #[test]
    fn from_replaces_and_hours_start_on_the_hour() {
        let groups = [
            group(TafChange::Base, T0 + 1_800, T0 + 4 * H, Some((220.0, 10.0))),
            group(TafChange::From, T0 + 4 * H, T0 + 9 * H, Some((300.0, 20.0))),
        ];
        let hours = resolve(&groups, T0 + 1_800, T0 + 9 * H);
        assert_eq!(hours[0].at, T0 + H);
        assert_eq!(hours[2].wind, Some(10.0));
        assert_eq!(
            (hours[3].wind, hours[3].dir, hours[3].low),
            (Some(20.0), Some(300.0), None)
        );
    }

    #[tokio::test]
    async fn stores_the_current_one() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        sqlx::query(
            "INSERT INTO station (slug, source, code, name, lat, lon, search, seen_at) \
             VALUES ('edsb', 'metar', 'EDSB', 'Baden', 48.78, 8.08, 'baden', 1)",
        )
        .execute(&db)
        .await?;
        let groups = vec![group(TafChange::Base, T0, T0 + 24 * H, Some((220.0, 7.0)))];
        let taf = |issued: i64, fetched: i64| {
            Taf::new(
                issued,
                T0,
                T0 + 24 * H,
                format!("TAF EDSB {issued}"),
                groups.clone(),
                fetched,
            )
        };
        store(
            &db,
            &[(1, taf(T0 - H, T0)), (1, taf(T0 + 2 * H, T0 + 2 * H))],
        )
        .await?;
        let got = current(&db, 1, T0 + 3 * H).await?.unwrap();
        assert_eq!(got.issued_at, T0 + 2 * H);
        assert_eq!(got.hours.len(), 24);
        assert_eq!(last_fetch(&db, 1).await?, Some(T0 + 2 * H));
        assert!(current(&db, 1, T0 + 24 * H).await?.is_none());
        assert_eq!(purge(&db, T0 + 25 * H).await?, 2);
        Ok(())
    }
}
