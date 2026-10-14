//! Forecasts and measurements on the admin page: model runs, the Open-Meteo budget over two
//! weeks, the places kept current; station lists, tracked stations, measurements, wind
//! statistics, verification.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use ts_rs::TS;
use wf_core::admin::data::{
    self, HotPlace, MeasurementCounts, ModelRunState, StationList, TrackedStation,
};
use wf_core::usage;

use super::Admin;
use super::overview::Budget;
use crate::{ApiError, AppState};

/// Open-Meteo calls of a day
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BudgetDay {
    /// Days since 1970-01-01, UTC
    #[ts(type = "number")]
    pub day: i64,
    #[ts(type = "number")]
    pub refresh: i64,
    #[ts(type = "number")]
    pub opened: i64,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminForecasts {
    pub runs: Vec<ModelRunState>,
    pub budget: Budget,
    /// The last 14 days, oldest first, today last
    pub days: Vec<BudgetDay>,
    #[ts(type = "number")]
    pub hot_total: i64,
    pub hot: Vec<HotPlace>,
    /// Open-Meteo is used with a key (the customer API)
    pub with_key: bool,
}

pub async fn forecasts(
    State(s): State<AppState>,
    _: Admin,
) -> Result<Json<AdminForecasts>, ApiError> {
    let now = wf_core::time::now();
    let today = usage::day(now);
    let refresh = usage::history(&s.db, today - 13, usage::OPEN_METEO_REFRESH).await?;
    let opened = usage::history(&s.db, today - 13, usage::OPEN_METEO_OPENED).await?;
    let of = |rows: &[(i64, i64)], day: i64| rows.iter().find(|r| r.0 == day).map_or(0, |r| r.1);
    let days = (today - 13..=today)
        .map(|day| BudgetDay {
            day,
            refresh: of(&refresh, day),
            opened: of(&opened, day),
        })
        .collect();
    let u = s.forecasts.open_meteo().usage();
    let (hot_total, hot) = data::hot_places(&s.db, now - s.forecasts.hot_s(), 50).await?;
    Ok(Json(AdminForecasts {
        runs: data::model_runs(&s.db).await?,
        budget: Budget {
            refresh: u.refresh,
            opened: u.opened,
            limit: u.limit,
            opened_limit: u.opened_limit,
        },
        days,
        hot_total,
        hot,
        with_key: s.forecasts.open_meteo().with_key(),
    }))
}

/// Ten-year statistics of a station being made, or failed lately
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StatsWork {
    #[ts(type = "number")]
    pub station: i64,
    pub slug: Option<String>,
    pub running: bool,
    /// Seconds since it failed
    #[ts(type = "number | null")]
    pub failed_ago: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminMeasurements {
    pub lists: Vec<StationList>,
    pub tracked: Vec<TrackedStation>,
    pub counts: MeasurementCounts,
    pub stats: Vec<StatsWork>,
}

pub async fn measurements(
    State(s): State<AppState>,
    _: Admin,
) -> Result<Json<AdminMeasurements>, ApiError> {
    let now = wf_core::time::now();
    let mut stats = Vec::new();
    if let Some(c) = &s.climate {
        let work = c.work();
        let rows = work.running.iter().map(|id| (*id, true, None, None)).chain(
            work.failed
                .into_iter()
                .map(|(id, ago, e)| (id, false, Some(ago), Some(e))),
        );
        for (station, running, failed_ago, error) in rows {
            let slug: Option<String> = sqlx::query_scalar("SELECT slug FROM station WHERE id = ?")
                .bind(station)
                .fetch_optional(&s.db)
                .await?;
            stats.push(StatsWork {
                station,
                slug,
                running,
                failed_ago,
                error,
            });
        }
    }
    Ok(Json(AdminMeasurements {
        lists: data::station_lists(&s.db).await?,
        tracked: data::tracked(&s.db, &s.settings.get().tracked_stations).await?,
        counts: data::measurement_counts(&s.db, now).await?,
        stats,
    }))
}

/// `POST /api/admin/stats/{station}/retry`: tries a station's statistics again now
pub async fn retry_stats(
    State(s): State<AppState>,
    admin: Admin,
    Path(station): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let c = s.climate.as_ref().ok_or(ApiError::NotFound)?;
    c.retry(station);
    let st = sqlx::query_as::<_, (i64, String)>("SELECT id, slug FROM station WHERE id = ?")
        .bind(station)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if let Some(full) = wf_core::station::by_slug(&s.db, &st.1).await? {
        c.status(st.0, &full, wf_core::time::now())
            .await
            .map_err(ApiError::internal)?;
    }
    admin.log(&s, "stats.retry", Some(&st.1)).await?;
    Ok(StatusCode::NO_CONTENT)
}
