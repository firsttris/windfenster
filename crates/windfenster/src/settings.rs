//! The settings an environment variable sets: they win over the admin page, which shows them
//! locked. A variable counts only when it is set (in the environment or `.env`); its parsed
//! value comes from the command line arguments.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::ServeArgs;

/// Setting key → (value, the variable's name), for each variable that is set.
pub fn from_env(args: &ServeArgs) -> BTreeMap<String, (Value, String)> {
    let candidates: [(&str, &str, Value); 10] = [
        ("WF_MODELS", "models", json!(args.open_meteo.models)),
        (
            "WF_OPEN_METEO_DAILY_BUDGET",
            "openMeteoBudget",
            json!(args.open_meteo.open_meteo_daily_budget),
        ),
        (
            "WF_TRACKED_STATIONS",
            "trackedStations",
            json!(args.tracked_stations),
        ),
        ("WF_ICON_GLOBAL", "iconGlobal", json!(args.icon_global)),
        ("WF_BASEMAP", "basemapMode", json!(args.basemap)),
        (
            "WF_BASEMAP_WORLD_ZOOM",
            "basemapWorldZoom",
            json!(args.basemap_world_zoom),
        ),
        (
            "WF_BASEMAP_REGION",
            "basemapRegion",
            json!(args.basemap_region.0),
        ),
        ("WF_BASEMAP_ZOOM", "basemapZoom", json!(args.basemap_zoom)),
        (
            "WF_BASEMAP_SPOTS",
            "basemapSpots",
            json!(args.basemap_spots),
        ),
        ("WF_TERRAIN_ZOOM", "terrainZoom", json!(args.terrain_zoom)),
    ];
    candidates
        .into_iter()
        .filter(|(var, _, _)| std::env::var_os(var).is_some_and(|v| !v.is_empty()))
        .map(|(var, key, value)| (key.to_owned(), (value, var.to_owned())))
        .collect()
}
