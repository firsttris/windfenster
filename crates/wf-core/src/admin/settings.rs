//! Settings changed on the admin page, in effect without a restart. A value comes from, in this
//! order: an environment variable that is set (then the page shows it locked), a row of `setting`
//! (written by the page), the default here. The loops and the API read [`SettingsStore::get`]
//! each time they need a value.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use ts_rs::TS;

/// Who may make an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SignupMode {
    Open,
    /// Only with an invitation link from an admin
    Invite,
    Closed,
}

/// Whether the server renews the basemap and relief by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BasemapMode {
    /// When due (old, or the depth changed), at once
    Auto,
    /// When due, an admin says when (or postpones)
    Ask,
    /// Never: the files are left as they are
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct Settings {
    /// Open-Meteo models to use; null: all
    pub models: Option<Vec<String>>,
    /// Open-Meteo calls per UTC day; null: 9,000 without a key, no limit with one
    pub open_meteo_budget: Option<u32>,
    /// Places opened within this many days are refreshed after every model run
    pub hot_days: u32,
    /// A point nobody opened for this many days is dropped (stations stay)
    pub points_days: u32,
    /// New places one client may make the server fetch
    pub places_per_hour: u32,
    pub places_per_day: u32,
    /// Place searches one client may send to the geocoder an hour
    pub searches_per_hour: u32,
    /// Stations (slugs) kept current, verified and kept 13 months even if nobody opens them
    pub tracked_stations: Vec<String>,
    /// E-mails the server sends a UTC day at most
    pub mail_per_day: u32,
    pub signup: SignupMode,
    /// A notice above every page; empty: none
    pub banner: String,
    /// Legal notice and privacy policy, Markdown; empty: no page
    pub imprint: String,
    pub privacy: String,
    pub basemap_mode: BasemapMode,
    pub basemap_world_zoom: u8,
    /// west, south, east, north; null: no deeper region
    pub basemap_region: Option<[f64; 4]>,
    pub basemap_zoom: u8,
    pub basemap_spots: u32,
    /// 0: no relief
    pub terrain_zoom: u8,
    pub icon_global: bool,
    /// Detail spots kept whoever looks there (lat, lon)
    pub pinned_spots: Vec<[f64; 2]>,
    /// A due renewal of the basemap waits until then (Unix time; 0: does not wait)
    #[ts(type = "number")]
    pub basemap_postponed_until: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            models: None,
            open_meteo_budget: None,
            hot_days: 7,
            points_days: 2,
            places_per_hour: 30,
            places_per_day: 100,
            searches_per_hour: 300,
            tracked_stations: vec!["edsb".into(), "lgrp".into()],
            mail_per_day: 500,
            signup: SignupMode::Open,
            banner: String::new(),
            imprint: String::new(),
            privacy: String::new(),
            basemap_mode: BasemapMode::Auto,
            basemap_world_zoom: 7,
            basemap_region: Some([-25.0, 27.0, 45.0, 72.0]),
            basemap_zoom: 8,
            basemap_spots: 60,
            terrain_zoom: 7,
            icon_global: true,
            pinned_spots: Vec::new(),
            basemap_postponed_until: 0,
        }
    }
}

/// Longest text a setting takes (legal notice, privacy policy)
const MAX_TEXT: usize = 50_000;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SettingError {
    #[error("unknown setting {0}")]
    Unknown(String),
    #[error("{0} is set by the environment ({1})")]
    Locked(String, String),
    #[error("invalid value for {0}")]
    Invalid(String),
}

impl Settings {
    /// Whether every value is within its bounds; the name of the first that is not.
    pub fn check(&self) -> Result<(), &'static str> {
        let within = |v: u32, lo: u32, hi: u32| (lo..=hi).contains(&v);
        let checks: [(&'static str, bool); 14] = [
            ("hotDays", within(self.hot_days, 1, 60)),
            ("pointsDays", within(self.points_days, 1, 60)),
            ("placesPerHour", within(self.places_per_hour, 1, 100_000)),
            ("placesPerDay", within(self.places_per_day, 1, 1_000_000)),
            (
                "searchesPerHour",
                within(self.searches_per_hour, 1, 100_000),
            ),
            (
                "openMeteoBudget",
                self.open_meteo_budget
                    .is_none_or(|b| within(b, 1, 10_000_000)),
            ),
            ("mailPerDay", self.mail_per_day <= 1_000_000),
            ("trackedStations", self.tracked_stations.len() <= 100),
            (
                "basemapWorldZoom",
                (4..=12).contains(&self.basemap_world_zoom),
            ),
            ("basemapZoom", (6..=15).contains(&self.basemap_zoom)),
            ("terrainZoom", self.terrain_zoom <= 12),
            ("basemapRegion", self.basemap_region.is_none_or(region_ok)),
            (
                "pinnedSpots",
                self.pinned_spots.len() <= 100
                    && self.pinned_spots.iter().all(|[la, lo]| {
                        (-85.0..=85.0).contains(la) && (-180.0..=180.0).contains(lo)
                    }),
            ),
            (
                "texts",
                [&self.banner, &self.imprint, &self.privacy]
                    .iter()
                    .all(|t| t.len() <= MAX_TEXT)
                    && self.banner.len() <= 500,
            ),
        ];
        match checks.iter().find(|(_, ok)| !ok) {
            Some((name, _)) => Err(name),
            None => Ok(()),
        }
    }
}

fn region_ok([w, s, e, n]: [f64; 4]) -> bool {
    (-180.0..=180.0).contains(&w)
        && (-180.0..=180.0).contains(&e)
        && (-90.0..=90.0).contains(&s)
        && (-90.0..=90.0).contains(&n)
        && w < e
        && s < n
}

/// The settings in effect, and where each comes from.
pub struct SettingsStore {
    db: Option<SqlitePool>,
    /// Values the environment sets, by setting key, with the variable's name
    env: BTreeMap<String, (Value, String)>,
    current: RwLock<Arc<Settings>>,
    /// Counts changes, so a loop can tell it should look again
    version: AtomicU64,
}

impl SettingsStore {
    /// The defaults only, nothing stored (tests, tools).
    pub fn defaults() -> Arc<Self> {
        Arc::new(SettingsStore {
            db: None,
            env: BTreeMap::new(),
            current: RwLock::new(Arc::new(Settings::default())),
            version: AtomicU64::new(0),
        })
    }

    /// Over a database with nothing stored yet: the defaults with the environment's values.
    pub fn fresh(
        db: SqlitePool,
        env: BTreeMap<String, (Value, String)>,
    ) -> anyhow::Result<Arc<Self>> {
        let store = SettingsStore {
            db: Some(db),
            env,
            current: RwLock::new(Arc::new(Settings::default())),
            version: AtomicU64::new(0),
        };
        let settings = store
            .merge(&Map::new())
            .map_err(|e| anyhow::anyhow!("settings: {e}"))?;
        *store.current.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(settings);
        Ok(Arc::new(store))
    }

    /// Reads the stored settings and lays the environment's over them. `env`: setting key →
    /// (value, the variable's name), for each variable that is set.
    pub async fn load(
        db: SqlitePool,
        env: BTreeMap<String, (Value, String)>,
    ) -> anyhow::Result<Arc<Self>> {
        let store = SettingsStore {
            db: Some(db),
            env,
            current: RwLock::new(Arc::new(Settings::default())),
            version: AtomicU64::new(0),
        };
        let stored = store.stored().await?;
        let settings = store
            .merge(&stored)
            .map_err(|e| anyhow::anyhow!("settings: {e}"))?;
        *store.current.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(settings);
        Ok(Arc::new(store))
    }

    pub fn get(&self) -> Arc<Settings> {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// Setting keys fixed by the environment, with the variable's name.
    pub fn locked(&self) -> BTreeMap<String, String> {
        self.env
            .iter()
            .map(|(k, (_, var))| (k.clone(), var.clone()))
            .collect()
    }

    async fn stored(&self) -> sqlx::Result<Map<String, Value>> {
        let Some(db) = &self.db else {
            return Ok(Map::new());
        };
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM setting")
            .fetch_all(db)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(k, v)| Some((k, serde_json::from_str(&v).ok()?)))
            .collect())
    }

    /// Defaults, then `stored`, then the environment. A stored value that no longer fits (an
    /// older version's) is left out.
    fn merge(&self, stored: &Map<String, Value>) -> Result<Settings, String> {
        let Value::Object(mut all) =
            serde_json::to_value(Settings::default()).map_err(|e| e.to_string())?
        else {
            return Err("settings are not an object".into());
        };
        for (k, v) in stored {
            if all.contains_key(k) {
                let mut trial = all.clone();
                trial.insert(k.clone(), v.clone());
                if serde_json::from_value::<Settings>(Value::Object(trial))
                    .is_ok_and(|s| s.check().is_ok())
                {
                    all.insert(k.clone(), v.clone());
                }
            }
        }
        for (k, (v, _)) in &self.env {
            all.insert(k.clone(), v.clone());
        }
        let s: Settings = serde_json::from_value(Value::Object(all)).map_err(|e| e.to_string())?;
        s.check().map_err(|k| format!("{k} out of bounds"))?;
        Ok(s)
    }

    /// Changes one setting (by its key, camelCase) and stores it; refused when the environment
    /// sets it or the value does not fit.
    pub async fn set(
        &self,
        key: &str,
        value: Value,
        now: i64,
    ) -> Result<Arc<Settings>, SettingError> {
        if let Some((_, var)) = self.env.get(key) {
            return Err(SettingError::Locked(key.to_owned(), var.clone()));
        }
        let Value::Object(mut all) =
            serde_json::to_value(&*self.get()).map_err(|_| SettingError::Invalid(key.into()))?
        else {
            return Err(SettingError::Invalid(key.into()));
        };
        if !all.contains_key(key) {
            return Err(SettingError::Unknown(key.to_owned()));
        }
        all.insert(key.to_owned(), value.clone());
        let next: Settings = serde_json::from_value(Value::Object(all))
            .map_err(|_| SettingError::Invalid(key.to_owned()))?;
        next.check()
            .map_err(|_| SettingError::Invalid(key.to_owned()))?;
        if let Some(db) = &self.db {
            sqlx::query(
                "INSERT INTO setting (key, value, updated_at) VALUES (?, ?, ?) \
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            )
            .bind(key)
            .bind(value.to_string())
            .bind(now)
            .execute(db)
            .await
            .map_err(|_| SettingError::Invalid(key.to_owned()))?;
        }
        let next = Arc::new(next);
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = next.clone();
        self.version.fetch_add(1, Ordering::Relaxed);
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn db() -> anyhow::Result<(tempfile::TempDir, SqlitePool)> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        Ok((dir, db))
    }

    #[tokio::test]
    async fn stored_then_environment_over_the_defaults() -> anyhow::Result<()> {
        let (_d, db) = db().await?;
        let env = BTreeMap::from([(
            "basemapZoom".to_owned(),
            (json!(10), "WF_BASEMAP_ZOOM".to_owned()),
        )]);
        let store = SettingsStore::load(db.clone(), env.clone()).await?;
        assert_eq!(store.get().basemap_zoom, 10);
        assert_eq!(store.get().hot_days, 7);
        store.set("hotDays", json!(14), 100).await?;
        store.set("signup", json!("invite"), 100).await?;
        assert_eq!(store.version(), 2);
        assert_eq!(
            store.set("basemapZoom", json!(9), 100).await,
            Err(SettingError::Locked(
                "basemapZoom".into(),
                "WF_BASEMAP_ZOOM".into()
            ))
        );
        assert_eq!(
            store.set("hotDays", json!(0), 100).await,
            Err(SettingError::Invalid("hotDays".into()))
        );
        assert_eq!(
            store.set("hotDays", json!("x"), 100).await,
            Err(SettingError::Invalid("hotDays".into()))
        );
        assert_eq!(
            store.set("nope", json!(1), 100).await,
            Err(SettingError::Unknown("nope".into()))
        );
        // After a restart
        let again = SettingsStore::load(db, env).await?;
        assert_eq!(
            (again.get().hot_days, again.get().signup),
            (14, SignupMode::Invite)
        );
        assert_eq!(
            again.locked().get("basemapZoom").map(String::as_str),
            Some("WF_BASEMAP_ZOOM")
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_stored_value_that_no_longer_fits_is_left_out() -> anyhow::Result<()> {
        let (_d, db) = db().await?;
        sqlx::query("INSERT INTO setting (key, value, updated_at) VALUES ('hotDays', '999', 0), ('gone', '1', 0)")
            .execute(&db)
            .await?;
        let store = SettingsStore::load(db, BTreeMap::new()).await?;
        assert_eq!(store.get().hot_days, 7);
        Ok(())
    }

    #[test]
    fn checks_bounds() {
        let mut s = Settings::default();
        assert_eq!(s.check(), Ok(()));
        s.basemap_region = Some([10.0, 50.0, 5.0, 55.0]);
        assert_eq!(s.check(), Err("basemapRegion"));
    }
}
