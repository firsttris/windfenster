//! The wind at one place over every hour on the map, read from the hours' uncompressed texels
//! (`{step:03}.rgba`, four bytes each): a tap on the map shows days of forecast at once,
//! straight from the model fields and without asking Open-Meteo.

use std::io::SeekFrom;
use std::path::Path;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
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
/// app's `sampleWind` do; None outside the texture.
pub fn around(meta: &TextureMeta, lat: f64, lon: f64) -> Option<[(usize, f64); 4]> {
    let (w, h) = (f64::from(meta.width), f64::from(meta.height));
    let fx = (lon - meta.west) / (meta.east - meta.west) * w - 0.5;
    let fy = (meta.north - lat) / (meta.north - meta.south) * h - 0.5;
    if fx < -0.5 || fy < -0.5 || fx > w - 0.5 || fy > h - 0.5 {
        return None;
    }
    let (x0, y0) = (fx.floor().max(0.0), fy.floor().max(0.0));
    let (ax, ay) = ((fx - x0).clamp(0.0, 1.0), (fy - y0).clamp(0.0, 1.0));
    let (x0, y0) = (x0 as usize, y0 as usize);
    let x1 = (x0 + 1).min(meta.width as usize - 1);
    let y1 = (y0 + 1).min(meta.height as usize - 1);
    let at = |x: usize, y: usize| y * meta.width as usize + x;
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
async fn hour(root: &Path, path: &str, lat: f64, lon: f64) -> Option<SeriesPoint> {
    let json = tokio::fs::read(root.join(format!("{path}.json")))
        .await
        .ok()?;
    let meta: TextureMeta = serde_json::from_slice(&json).ok()?;
    let texels = around(&meta, lat, lon)?;
    let mut file = tokio::fs::File::open(root.join(format!("{path}.rgba")))
        .await
        .ok()?;
    let mut read = Vec::with_capacity(4);
    for (at, w) in texels {
        file.seek(SeekFrom::Start(at as u64 * 4)).await.ok()?;
        let mut px = [0u8; 4];
        file.read_exact(&mut px).await.ok()?;
        read.push((px, w));
    }
    Some(decode(&meta, blend(&read)?))
}

/// The place over every hour of every layer of the index.
pub async fn series(root: &Path, index: &MapIndex, lat: f64, lon: f64) -> MapSeries {
    let mut layers = Vec::with_capacity(index.layers.len());
    for layer in &index.layers {
        let mut points = Vec::with_capacity(layer.frames.len());
        for frame in &layer.frames {
            if let Some(p) = hour(root, &frame.path, lat, lon).await {
                points.push(p);
            }
        }
        layers.push(SeriesLayer {
            model: layer.model.clone(),
            points,
        });
    }
    MapSeries { layers }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> TextureMeta {
        TextureMeta {
            model: "icon_d2".into(),
            run_at: 0,
            step: 3,
            valid_at: 10_800,
            west: 0.0,
            south: 50.0,
            east: 2.0,
            north: 52.0,
            width: 2,
            height: 2,
            domain: vec![],
            u_min: -10.0,
            u_max: 10.0,
            v_min: -10.0,
            v_max: 10.0,
            gust_max: Some(30.0),
        }
    }

    #[test]
    fn blends_the_texels_around_a_place() {
        let m = meta();
        // On a texel centre it alone counts.
        let c = around(&m, 51.5, 0.5).unwrap();
        assert_eq!(c[0], (0, 1.0));
        // Halfway between the two western centres
        let w = around(&m, 51.0, 0.5).unwrap();
        assert_eq!((w[0], w[2]), ((0, 0.5), (2, 0.5)));
        assert_eq!(around(&m, 49.9, 0.5), None);
        // Outside the domain left out; less than half inside is no value.
        let blended = blend(&[([255, 0, 0, 255], 0.5), ([0, 0, 0, 0], 0.5)]);
        assert_eq!(blended, Some([255.0, 0.0, 0.0]));
        assert_eq!(
            blend(&[([255, 0, 0, 255], 0.25), ([0, 0, 0, 0], 0.75)]),
            None
        );
        assert_eq!(
            blend(&[([200, 100, 0, 255], 0.5), ([100, 0, 50, 255], 0.5)]),
            Some([150.0, 50.0, 25.0])
        );
    }

    #[test]
    fn reads_the_wind() {
        let m = meta();
        // U +10 (towards the east), V 0: from the west at 10 kn, gusts 15
        let p = decode(&m, [255.0, 127.5, 127.5]);
        assert!((p.wind - 10.0).abs() < 0.05);
        assert!((p.dir - 270.0).abs() < 0.5);
        assert_eq!(p.gust, Some(15.0));
        // Gusts under the wind are lifted to it.
        assert_eq!(decode(&m, [255.0, 127.5, 0.0]).gust, Some(p.wind));
        let none = TextureMeta {
            gust_max: None,
            ..meta()
        };
        assert_eq!(decode(&none, [255.0, 127.5, 127.5]).gust, None);
    }
}
