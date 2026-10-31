use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use ts_rs::TS;

use crate::geo::distance_km;
use crate::text::fold;

/// Where a station's measurements come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
#[ts(export)]
pub enum StationSource {
    /// Airport weather reports
    Metar,
    /// Deutscher Wetterdienst, 10-minute values
    Dwd,
    /// OpenWindMap community stations
    Owm,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Station {
    /// URL key: /station/{slug}
    pub slug: String,
    pub source: StationSource,
    /// The source's id, e.g. the ICAO code EDSB.
    pub code: String,
    pub name: String,
    /// ISO 3166-1 alpha-2
    pub country: Option<String>,
    pub region: Option<String>,
    pub municipality: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: Option<f64>,
    /// Publishes a TAF (airport forecast).
    pub has_taf: bool,
}

/// A station as an import delivers it.
#[derive(Debug, Clone, PartialEq)]
pub struct NewStation {
    pub station: Station,
    /// More words to find it by, e.g. "Baden Airpark".
    pub keywords: Option<String>,
}

const COLUMNS: &str =
    "slug, source, code, name, country, region, municipality, lat, lon, elevation_m, has_taf";

/// Inserts or updates stations by (source, code); returns how many were written.
pub async fn upsert(db: &SqlitePool, stations: &[NewStation], seen_at: i64) -> sqlx::Result<usize> {
    let mut tx = db.begin().await?;
    for s in stations {
        let st = &s.station;
        let search = fold(
            &[
                Some(st.name.as_str()),
                Some(st.code.as_str()),
                st.municipality.as_deref(),
                st.region.as_deref(),
                s.keywords.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
        );
        sqlx::query(
            "INSERT INTO station (slug, source, code, name, country, region, municipality, lat, lon, \
             elevation_m, has_taf, search, seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (source, code) DO UPDATE SET slug = excluded.slug, name = excluded.name, \
             country = excluded.country, region = excluded.region, \
             municipality = excluded.municipality, lat = excluded.lat, lon = excluded.lon, \
             elevation_m = excluded.elevation_m, has_taf = excluded.has_taf, \
             search = excluded.search, seen_at = excluded.seen_at",
        )
        .bind(&st.slug)
        .bind(st.source)
        .bind(&st.code)
        .bind(&st.name)
        .bind(&st.country)
        .bind(&st.region)
        .bind(&st.municipality)
        .bind(st.lat)
        .bind(st.lon)
        .bind(st.elevation_m)
        .bind(st.has_taf)
        .bind(&search)
        .bind(seen_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(stations.len())
}

/// Drops stations of a source that the import at `seen_at` no longer listed.
pub async fn remove_unseen(
    db: &SqlitePool,
    source: StationSource,
    seen_at: i64,
) -> sqlx::Result<u64> {
    Ok(
        sqlx::query("DELETE FROM station WHERE source = ? AND seen_at < ?")
            .bind(source)
            .bind(seen_at)
            .execute(db)
            .await?
            .rows_affected(),
    )
}

/// Marks exactly these stations (by slug) as tracked; returns the slugs that do not exist.
pub async fn set_tracked(db: &SqlitePool, slugs: &[String]) -> sqlx::Result<Vec<String>> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE station SET tracked = 0")
        .execute(&mut *tx)
        .await?;
    let mut unknown = Vec::new();
    for slug in slugs {
        let n = sqlx::query("UPDATE station SET tracked = 1 WHERE slug = ?")
            .bind(slug)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if n == 0 {
            unknown.push(slug.clone());
        }
    }
    tx.commit().await?;
    Ok(unknown)
}

/// Row ids of the stations whose history is kept whole: tracked ones, those opened since the
/// one bound value, and anyone's favourites. For `IN (…)`.
pub const KEPT_IDS: &str = "SELECT s.id FROM station s WHERE s.tracked \
     OR EXISTS (SELECT 1 FROM place p WHERE p.requested_at >= ? \
       AND p.lat_e2 = CAST(round(s.lat * 100) AS INTEGER) \
       AND p.lon_e2 = CAST(round(s.lon * 100) AS INTEGER)) \
     OR EXISTS (SELECT 1 FROM favorite f WHERE f.slug = s.slug)";

/// Stations of a source to keep current: tracked ones and those opened since `since`, with
/// their row ids.
pub async fn watched(
    db: &SqlitePool,
    source: StationSource,
    since: i64,
) -> sqlx::Result<Vec<(i64, Station)>> {
    #[derive(sqlx::FromRow)]
    struct Watched {
        id: i64,
        #[sqlx(flatten)]
        station: Station,
    }
    let rows: Vec<Watched> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT id, {COLUMNS} FROM station s WHERE source = ? AND (tracked OR EXISTS ( \
           SELECT 1 FROM place p WHERE p.requested_at >= ? \
           AND p.lat_e2 = CAST(round(s.lat * 100) AS INTEGER) \
           AND p.lon_e2 = CAST(round(s.lon * 100) AS INTEGER)))"
    )))
    .bind(source)
    .bind(since)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|w| (w.id, w.station)).collect())
}

/// The station's row id, for joins (measurements, verification).
pub async fn id_of(db: &SqlitePool, slug: &str) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT id FROM station WHERE slug = ?")
        .bind(slug)
        .fetch_optional(db)
        .await
}

pub async fn by_slug(db: &SqlitePool, slug: &str) -> sqlx::Result<Option<Station>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM station WHERE slug = ?"
    )))
    .bind(slug)
    .fetch_optional(db)
    .await
}

/// Time of the newest import of a source, 0 if it never ran.
pub async fn last_import(db: &SqlitePool, source: StationSource) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COALESCE(MAX(seen_at), 0) FROM station WHERE source = ?")
        .bind(source)
        .fetch_one(db)
        .await
}

/// Stations whose name, code, town, region or keywords contain every word of `query`
/// (case and accents ignored). An exact code comes first, then names starting with the query.
pub async fn search(db: &SqlitePool, query: &str, limit: u32) -> sqlx::Result<Vec<Station>> {
    let folded = fold(query.trim());
    let words: Vec<&str> = folded.split_whitespace().collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let escape = |w: &str| {
        w.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    };
    let mut q: QueryBuilder<Sqlite> =
        QueryBuilder::new(format!("SELECT {COLUMNS} FROM station WHERE 1 = 1"));
    for w in &words {
        q.push(" AND search LIKE ")
            .push_bind(format!("%{}%", escape(w)))
            .push(" ESCAPE '\\'");
    }
    let prefix = format!("{}%", escape(&folded));
    q.push(" ORDER BY lower(code) = ")
        .push_bind(folded.clone())
        .push(" DESC, search LIKE ")
        .push_bind(prefix)
        .push(" ESCAPE '\\' DESC, length(name), name LIMIT ")
        .push_bind(limit);
    q.build_query_as().fetch_all(db).await
}

/// The station closest to a point within `max_km`, with its distance.
pub async fn nearest(
    db: &SqlitePool,
    lat: f64,
    lon: f64,
    max_km: f64,
) -> sqlx::Result<Option<(Station, f64)>> {
    let dlat = max_km / 111.0;
    let dlon = max_km / (111.0 * lat.to_radians().cos().max(0.05));
    let candidates: Vec<Station> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM station WHERE lat BETWEEN ? AND ? AND lon BETWEEN ? AND ?"
    )))
    .bind(lat - dlat)
    .bind(lat + dlat)
    .bind(lon - dlon)
    .bind(lon + dlon)
    .fetch_all(db)
    .await?;
    Ok(candidates
        .into_iter()
        .map(|s| {
            let d = distance_km(lat, lon, s.lat, s.lon);
            (s, d)
        })
        .filter(|(_, d)| *d <= max_km)
        .min_by(|a, b| a.1.total_cmp(&b.1)))
}

/// A town or place from the geocoder, for the search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlaceHit {
    pub name: String,
    /// State or region, e.g. Baden-Württemberg
    pub admin: Option<String>,
    /// ISO 3166-1 alpha-2
    pub country: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

/// `GET /api/stations?q=`
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchResults {
    pub stations: Vec<Station>,
    pub places: Vec<PlaceHit>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(slug: &str, code: &str, name: &str, lat: f64, lon: f64) -> NewStation {
        NewStation {
            station: Station {
                slug: slug.into(),
                source: StationSource::Metar,
                code: code.into(),
                name: name.into(),
                country: Some("DE".into()),
                region: Some("Baden-Württemberg".into()),
                municipality: None,
                lat,
                lon,
                elevation_m: None,
                has_taf: false,
            },
            keywords: None,
        }
    }

    #[tokio::test]
    async fn search_and_nearest() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let mut edsb = station(
            "edsb",
            "EDSB",
            "Karlsruhe Baden-Baden Airport",
            48.7794,
            8.0805,
        );
        edsb.station.municipality = Some("Rheinmünster".into());
        edsb.keywords = Some("Baden Airpark".into());
        let stations = [
            edsb,
            station("edds", "EDDS", "Stuttgart Airport", 48.6899, 9.2220),
            station("eddf", "EDDF", "Frankfurt Main Airport", 50.0267, 8.5584),
        ];
        upsert(&db, &stations, 100).await?;
        // Updating keeps one row per (source, code).
        upsert(&db, &stations[..1], 200).await?;
        assert_eq!(last_import(&db, StationSource::Metar).await?, 200);

        let names = |v: Vec<Station>| v.into_iter().map(|s| s.slug).collect::<Vec<_>>();
        assert_eq!(names(search(&db, "edsb", 10).await?), ["edsb"]);
        assert_eq!(names(search(&db, "baden airpark", 10).await?), ["edsb"]);
        assert_eq!(names(search(&db, "RHEINMUNSTER", 10).await?), ["edsb"]);
        assert_eq!(names(search(&db, "airport", 10).await?).len(), 3);
        // "Stuttgart Airport" starts with the query and comes before longer matches.
        assert_eq!(names(search(&db, "stutt", 10).await?), ["edds"]);
        assert!(search(&db, "  ", 10).await?.is_empty());
        assert!(search(&db, "100%", 10).await?.is_empty());

        let (s, d) = nearest(&db, 48.80, 8.10, 50.0).await?.unwrap();
        assert_eq!(s.slug, "edsb");
        assert!(d < 3.0);
        assert!(nearest(&db, 36.4, 28.1, 50.0).await?.is_none());
        assert_eq!(by_slug(&db, "eddf").await?.unwrap().code, "EDDF");
        Ok(())
    }
}
