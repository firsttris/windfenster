//! `GET /api/stations?q=`: stations from the database, towns from the geocoder.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use moka::future::Cache;
use serde::Deserialize;
use wf_core::station::{self, PlaceHit, SearchResults};
use wf_core::text::fold;

use crate::client::ClientIp;
use crate::{ApiError, AppState};

pub type PlaceCache = Cache<(String, String), Arc<Vec<PlaceHit>>>;

pub fn new_cache() -> PlaceCache {
    Cache::builder()
        .max_capacity(5_000)
        .time_to_live(Duration::from_secs(3_600))
        .build()
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: String,
    /// Language for place names: de or en.
    lang: Option<String>,
}

pub async fn search(
    State(s): State<AppState>,
    ip: ClientIp,
    Query(q): Query<SearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let text = q.q.trim();
    let mut results = SearchResults {
        stations: Vec::new(),
        places: Vec::new(),
    };
    if text.chars().count() >= 2 {
        results.stations = station::search(&s.db, text, 8).await?;
        if let Some(geocoder) = s.geocoder.clone() {
            let lang = match q.lang.as_deref() {
                Some("en") => "en",
                _ => "de",
            }
            .to_owned();
            let query = text.to_owned();
            let key = (fold(text), lang.clone());
            // A search not asked lately goes out to the geocoder: limited per client, beyond
            // which only stations are found.
            let cached = s.places.contains_key(&key);
            let allowed = cached || s.limits.search(ip, wf_core::time::now()).await;
            let found = if allowed {
                s.places
                    .try_get_with(key, async move {
                        geocoder.search(&query, &lang, 5).await.map(Arc::new)
                    })
                    .await
            } else {
                Ok(Arc::default())
            };
            match found {
                Ok(places) => results.places = (*places).clone(),
                Err(e) => tracing::debug!("geocoding: {e:#}"),
            }
        }
    }
    Ok((
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        Json(results),
    ))
}
