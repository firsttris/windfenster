//! What the map can show: per model the hours on disk, each from the newest run that has it.

use serde::Serialize;
use ts_rs::TS;

use crate::basemap::BasemapInfo;
use crate::store::MapRun;

/// `GET /api/map`: the layers with their hours, and the detailed basemap if there is one.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct MapIndex {
    pub layers: Vec<MapLayer>,
    /// Served at `/api/map/basemap.pmtiles`; null without one (the app falls back to its own)
    pub basemap: Option<BasemapInfo>,
    /// Elevation for the relief, served at `/api/map/terrain.pmtiles`; null without
    pub terrain: Option<BasemapInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MapLayer {
    /// Model id, as in the forecast
    pub model: String,
    /// One per hour, ascending
    pub frames: Vec<MapFrame>,
    /// The area its textures cover (west, south, east, north), from the newest run; null
    /// without hours
    pub area: Option<[f64; 4]>,
    /// Whether the app takes its textures in pieces (`crop`), not whole
    pub in_pieces: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MapFrame {
    #[ts(type = "number")]
    pub valid_at: i64,
    #[ts(type = "number")]
    pub run_at: i64,
    /// Under `/api/map/`, without the extension: `{model}/{run}/{step}`
    pub path: String,
}

/// The hours of a model's runs (newest first), each from the newest run that has it.
pub fn layer(model: &str, runs: &[MapRun]) -> MapLayer {
    let mut frames: Vec<MapFrame> = Vec::new();
    for run in runs {
        for &step in &run.steps {
            let valid_at = run.run_at + i64::from(step) * 3_600;
            if frames.iter().any(|f| f.valid_at == valid_at) {
                continue;
            }
            frames.push(MapFrame {
                valid_at,
                run_at: run.run_at,
                path: format!("{model}/{}/{step:03}", run.id),
            });
        }
    }
    frames.sort_by_key(|f| f.valid_at);
    MapLayer {
        model: model.to_owned(),
        frames,
        area: None,
        in_pieces: model == crate::icon_global::MODEL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::run_id;

    #[test]
    fn takes_each_hour_from_the_newest_run() {
        let (r06, r09) = (1_791_698_400, 1_791_709_200);
        let run = |run_at, steps: &[u32]| MapRun {
            id: run_id(run_at),
            run_at,
            steps: steps.to_vec(),
        };
        // 09 UTC has its first two hours; the rest comes from 06 UTC.
        let l = layer("icon_d2", &[run(r09, &[0, 1]), run(r06, &[0, 2, 3, 4, 5])]);
        let got: Vec<(i64, &str)> = l
            .frames
            .iter()
            .map(|f| ((f.valid_at - r06) / 3_600, f.path.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (0, "icon_d2/2026101106/000"),
                (2, "icon_d2/2026101106/002"),
                (3, "icon_d2/2026101109/000"),
                (4, "icon_d2/2026101109/001"),
                (5, "icon_d2/2026101106/005"),
            ]
        );
    }
}
