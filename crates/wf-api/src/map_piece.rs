//! `GET /api/map/{model}/{run}/{step}/{west}_{south}_{east}_{north}.png|json`: a piece of an
//! hour's world texture, for models whose whole world is too much to send (ICON global). Cut
//! from the texels on first request and kept in memory a while; never changes once cut.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use wf_map::crop::{self, Piece};
use wf_map::texture::TextureMeta;

use crate::{ApiError, AppState};

/// Models the app takes in pieces
const IN_PIECES: &[&str] = &[wf_map::icon_global::MODEL];

/// Pieces cut lately: PNG and description, about 200–600 KB each
pub type PieceCache = moka::future::Cache<String, Arc<(Vec<u8>, Vec<u8>)>>;

pub fn new_cache() -> PieceCache {
    moka::future::Cache::builder()
        .weigher(|_, v: &Arc<(Vec<u8>, Vec<u8>)>| {
            (v.0.len() + v.1.len()).try_into().unwrap_or(u32::MAX)
        })
        .max_capacity(256 << 20)
        .time_to_idle(std::time::Duration::from_secs(3 * 3_600))
        .build()
}

pub async fn piece(
    State(s): State<AppState>,
    Path((model, run, step, file)): Path<(String, String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let map = s.map.as_ref().ok_or(ApiError::NotFound)?;
    let digits = |v: &str, n: usize| v.len() == n && v.bytes().all(|b| b.is_ascii_digit());
    let (name, ext) = file.rsplit_once('.').ok_or(ApiError::NotFound)?;
    let piece = Piece::parse(name).ok_or(ApiError::NotFound)?;
    if !IN_PIECES.contains(&model.as_str()) || !digits(&run, 10) || !digits(&step, 3) {
        return Err(ApiError::NotFound);
    }
    let mime = match ext {
        "png" => "image/png",
        "json" => "application/json",
        _ => return Err(ApiError::NotFound),
    };
    let dir = map.root().join(&model).join(&run);
    let key = format!("{model}/{run}/{step}/{name}");
    let cut = s
        .pieces
        .try_get_with(key, async move {
            let world: TextureMeta = match tokio::fs::read(dir.join(format!("{step}.json"))).await {
                Ok(b) => serde_json::from_slice(&b).map_err(ApiError::internal)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(ApiError::NotFound);
                }
                Err(e) => return Err(ApiError::internal(e)),
            };
            let rgba = dir.join(format!("{step}.rgba"));
            let (png, meta) = tokio::task::spawn_blocking(move || crop::cut(&rgba, &world, piece))
                .await
                .map_err(ApiError::internal)?
                .map_err(ApiError::internal)?;
            let json = serde_json::to_vec(&meta).map_err(ApiError::internal)?;
            Ok(Arc::new((png, json)))
        })
        .await
        .map_err(|e: Arc<ApiError>| match *e {
            ApiError::NotFound => ApiError::NotFound,
            ref other => ApiError::internal(format!("{other:?}")),
        })?;
    let body = if ext == "png" {
        cut.0.clone()
    } else {
        cut.1.clone()
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        body,
    ))
}
