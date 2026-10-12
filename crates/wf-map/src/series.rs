//! The wind at one place over every hour on the map, read from the hours' uncompressed texels
//! (`{step:03}.rgba`, four bytes each): a tap on the map shows days of forecast at once,
//! straight from the model fields and without asking Open-Meteo.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::index::MapIndex;
use crate::texture::TextureMeta;

/// One hour at the place.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SeriesPoint {
    #[ts(type = "number")]
    pub valid_at: i64,
    #[ts(type = "number")]
    pub run_at: i64,
    /// kn
    pub wind: f32,
    /// kn; null where the model hour has no gusts
    pub gust: Option<f32>,
    /// Where the wind comes from, degrees (0 = north)
    pub dir: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SeriesLayer {
    pub model: String,
    /// The hours the place lies in the model's domain, ascending
    pub points: Vec<SeriesPoint>,
}

/// `GET /api/map/series?lat=&lon=`: per model on the map, the place's hours.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct MapSeries {
    pub layers: Vec<SeriesLayer>,
}

/// The four texels around a place (row 0 north) with their weights, blended as the GPU and the
/// app's `sampleWind` do; None outside the texture. A global texture wraps: the first column
/// is the last one's east neighbour.
pub fn around(meta: &TextureMeta, lat: f64, lon: f64) -> Option<[(usize, f64); 4]> {
    let (w, h) = (f64::from(meta.width), f64::from(meta.height));
    let span = meta.east - meta.west;
    let wraps = meta.wraps();
    let lon = if wraps {
        meta.west + (lon - meta.west).rem_euclid(span)
    } else {
        lon
    };
    let fx = (lon - meta.west) / span * w - 0.5;
    let fy = (meta.north - lat) / (meta.north - meta.south) * h - 0.5;
    if (!wraps && (fx < -0.5 || fx > w - 0.5)) || fy < -0.5 || fy > h - 0.5 {
        return None;
    }
    let (width, height) = (meta.width as usize, meta.height as usize);
    let fx0 = fx.floor();
    let (x0, x1, ax) = if wraps {
        // fx lies in −0.5…w − 0.5: left of the first texel's centre the last one is the neighbour
        let x0 = fx0.rem_euclid(w) as usize;
        (x0, (x0 + 1) % width, fx - fx0)
    } else {
        let x0 = fx0.max(0.0);
        (
            x0 as usize,
            (x0 as usize + 1).min(width - 1),
            (fx - x0).clamp(0.0, 1.0),
        )
    };
    let y0 = fy.floor().max(0.0);
    let ay = (fy - y0).clamp(0.0, 1.0);
    let y0 = y0 as usize;
    let y1 = (y0 + 1).min(height - 1);
    let at = |x: usize, y: usize| y * width + x;
    Some([
        (at(x0, y0), (1.0 - ax) * (1.0 - ay)),
        (at(x1, y0), ax * (1.0 - ay)),
        (at(x0, y1), (1.0 - ax) * ay),
        (at(x1, y1), ax * ay),
    ])
}

/// The blended colour (0…255 per channel) of texels and their weights, leaving out those
/// outside the domain (alpha 0); None when less than half the weight lies inside, as on the map.
pub fn blend(texels: &[([u8; 4], f64)]) -> Option<[f32; 3]> {
    let (mut sum, mut weight) = ([0.0f64; 3], 0.0);
    for ([r, g, b, a], w) in texels {
        if *a == 0 {
            continue;
        }
        for (s, c) in sum.iter_mut().zip([r, g, b]) {
            *s += f64::from(*c) * w;
        }
        weight += w;
    }
    (weight >= 0.5).then(|| sum.map(|s| (s / weight) as f32))
}

/// The wind of a blended colour, as the shaders read it.
pub fn decode(meta: &TextureMeta, [r, g, b]: [f32; 3]) -> SeriesPoint {
    let at = |x: f32, lo: f32, hi: f32| lo + (hi - lo) * x / 255.0;
    let u = at(r, meta.u_min, meta.u_max);
    let v = at(g, meta.v_min, meta.v_max);
    let wind = u.hypot(v);
    let round = |x: f32| (x * 10.0).round() / 10.0;
    SeriesPoint {
        valid_at: meta.valid_at,
        run_at: meta.run_at,
        wind: round(wind),
        // Never below the wind: both are scaled apart
        gust: meta.gust_max.map(|m| round(at(b, 0.0, m).max(wind))),
        dir: round((-u).atan2(-v).to_degrees().rem_euclid(360.0)),
    }
}

/// One hour at the place, or None outside the domain or when the hour is gone (pruned).
fn hour(root: &Path, path: &str, lat: f64, lon: f64) -> Option<SeriesPoint> {
    let json = std::fs::read(root.join(format!("{path}.json"))).ok()?;
    let meta: TextureMeta = serde_json::from_slice(&json).ok()?;
    let texels = around(&meta, lat, lon)?;
    let mut file = std::fs::File::open(root.join(format!("{path}.rgba"))).ok()?;
    let mut read = Vec::with_capacity(4);
    for (at, w) in texels {
        file.seek(SeekFrom::Start(at as u64 * 4)).ok()?;
        let mut px = [0u8; 4];
        file.read_exact(&mut px).ok()?;
        read.push((px, w));
    }
    Some(decode(&meta, blend(&read)?))
}

/// The place over every hour of every layer of the index. Some 200 small reads: all in one
/// blocking task, not a thread hand-off per read.
pub async fn series(root: &Path, index: &MapIndex, lat: f64, lon: f64) -> MapSeries {
    let (root, index) = (root.to_path_buf(), index.clone());
    let read = tokio::task::spawn_blocking(move || {
        let layers = index
            .layers
            .iter()
            .map(|layer| SeriesLayer {
                model: layer.model.clone(),
                points: layer
                    .frames
                    .iter()
                    .filter_map(|frame| hour(&root, &frame.path, lat, lon))
                    .collect(),
            })
            .collect();
        MapSeries { layers }
    });
    read.await.unwrap_or(MapSeries { layers: Vec::new() })
}
