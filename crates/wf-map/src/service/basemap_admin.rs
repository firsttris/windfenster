//! The basemap as the admin page sees and steers it: what is there, whether a renewal is due and
//! why, the newest build, the job under way, the detail spots; and saying "now", cancelling,
//! removing a spot.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tokio::sync::Notify;
use ts_rs::TS;

use super::MapData;
use crate::basemap::fetch::{self, Due};
use crate::basemap::spots::{self, Spot};
use crate::basemap::terrain::{self, TerrainNote};
use crate::basemap::work::{self, Work};

/// What the admin page said about the next renewal.
#[derive(Default)]
pub struct Renewal {
    /// "Now": the next round renews what is due, even in the mode "ask"
    pub(super) approved: AtomicBool,
    /// Ends a renewal under way
    pub(super) cancel: Notify,
    /// The newest Protomaps build seen while a renewal waits, and when it was looked for
    pub(super) newest: Mutex<Option<(String, i64)>>,
}

impl Renewal {
    /// Looks for the newest build at most every six hours (up to ten requests).
    pub(super) async fn look_for_newest(&self, http: &reqwest::Client, base: &str, now: i64) {
        let fresh = self
            .newest
            .lock()
            .expect("newest")
            .as_ref()
            .is_some_and(|(_, at)| now - at < 6 * 3_600);
        if fresh {
            return;
        }
        if let Ok((build, _)) = fetch::newest_build(http, base, now).await {
            *self.newest.lock().expect("newest") = Some((build, now));
        }
    }
}

/// Our basemap as it is.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BasemapFile {
    /// Protomaps build, YYYYMMDD
    pub build: String,
    #[ts(type = "number")]
    pub made_at: i64,
    #[ts(type = "number")]
    pub renew_at: i64,
    pub world_zoom: Option<u8>,
    pub region: Option<[f64; 4]>,
    pub region_zoom: u8,
}

/// Why a spot is kept
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SpotReason {
    Pinned,
    /// A tracked station, or a place opened in the last week
    Place,
    Tapped,
    /// Nobody wants it now; it goes when room is needed
    Unwanted,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SpotInfo {
    pub lat: f64,
    pub lon: f64,
    #[ts(type = "number")]
    pub bytes: u64,
    /// When it was last wanted
    #[ts(type = "number")]
    pub wanted_at: i64,
    pub reason: SpotReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BasemapState {
    /// None: none there, or the operator's own file (`operator`)
    pub file: Option<BasemapFile>,
    /// A basemap is there that is not ours: it is never touched
    pub operator: bool,
    pub terrain: Option<TerrainNote>,
    pub terrain_operator: bool,
    /// Why the basemap is to be made now
    pub due: Option<Due>,
    pub terrain_due: Option<Due>,
    /// Said "now" on the admin page; done on the next round
    pub approved: bool,
    /// The newest Protomaps build, when a renewal waits for an admin
    pub newest_build: Option<String>,
    pub work: Vec<Work>,
    pub spots: Vec<SpotInfo>,
}

impl MapData {
    pub async fn basemap_state(&self, db: &wf_core::db::Db) -> BasemapState {
        let now = wf_core::time::now();
        let depth = self.depth();
        let note = fetch::read_note(&self.root).await;
        let exists = tokio::fs::try_exists(crate::basemap::path(&self.root))
            .await
            .unwrap_or(false);
        let terrain_exists = tokio::fs::try_exists(terrain::path(&self.root))
            .await
            .unwrap_or(false);
        let terrain_note = terrain::note(&self.root).await;
        let newest_build = self
            .renewal
            .newest
            .lock()
            .expect("newest")
            .as_ref()
            .map(|(b, _)| b.clone());
        let spots = self.spot_list(db, now).await;
        BasemapState {
            operator: exists && note.is_none(),
            file: note.map(|n| BasemapFile {
                renew_at: fetch::renew_at(&n),
                build: n.build,
                made_at: n.made_at,
                world_zoom: n.world_zoom,
                region: n.region,
                region_zoom: n.zoom,
            }),
            terrain_operator: terrain_exists && terrain_note.is_none(),
            terrain: terrain_note,
            due: fetch::due_now(&self.root, &depth.cut(), now).await,
            terrain_due: terrain::due_now(&self.root, depth.terrain_zoom, now).await,
            approved: self.renewal.approved.load(Ordering::Relaxed),
            newest_build,
            work: work::current(),
            spots,
        }
    }

    /// Whether a renewal waits for an admin: the mode is "ask", the basemap or relief is due,
    /// nobody said "now" yet and it is not put off.
    pub async fn renewal_waiting(&self) -> bool {
        let now = wf_core::time::now();
        let s = self.settings.get();
        if s.basemap_mode != wf_core::admin::settings::BasemapMode::Ask
            || s.basemap_postponed_until > now
            || self.renewal.approved.load(Ordering::Relaxed)
        {
            return false;
        }
        let depth = self.depth();
        fetch::due_now(&self.root, &depth.cut(), now)
            .await
            .is_some()
            || terrain::due_now(&self.root, depth.terrain_zoom, now)
                .await
                .is_some()
    }

    /// Renews what is due on the next round, whatever the mode; the loop is woken.
    pub fn renew_now(&self) {
        self.renewal.approved.store(true, Ordering::Relaxed);
        wf_core::status::board().ask("basemap");
    }

    /// Ends the renewal under way: the old files stay; a cut starts over next time, a download
    /// goes on from its pieces.
    pub fn cancel_renewal(&self) {
        self.renewal.approved.store(false, Ordering::Relaxed);
        self.renewal.cancel.notify_waiters();
    }

    /// Removes a spot's file (it comes again if someone wants the place). False: none there.
    pub async fn remove_spot(&self, lat: f64, lon: f64) -> std::io::Result<bool> {
        let path = spots::path(&self.root, Spot::of(lat, lon));
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {
                self.readers.forget_spots();
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Cuts and downloads a restart broke off, when nothing runs. Returns how many went.
    pub async fn clear_leftovers(&self) -> std::io::Result<usize> {
        if !work::current().is_empty() {
            return Ok(0);
        }
        spots::clear_leftovers(&self.root).await
    }

    async fn spot_list(&self, db: &wf_core::db::Db, now: i64) -> Vec<SpotInfo> {
        let settings = self.settings.get();
        let pinned: Vec<Spot> =
            spots::spots_of(settings.pinned_spots.iter().map(|[a, b]| (*a, *b)));
        let places: Vec<Spot> = wf_core::forecast::hot_places(db, now - super::basemap::WANTED_S)
            .await
            .map(|ps| spots::spots_of(ps.iter().map(|p| (p.lat(), p.lon()))))
            .unwrap_or_default();
        let tapped: Vec<Spot> = self
            .tapped
            .lock()
            .expect("tapped")
            .iter()
            .map(|(s, _)| *s)
            .collect();
        let mut out = Vec::new();
        for (spot, at) in spots::on_disk(&self.root).await {
            let bytes = tokio::fs::metadata(spots::path(&self.root, spot))
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            let reason = if pinned.contains(&spot) {
                SpotReason::Pinned
            } else if places.contains(&spot) {
                SpotReason::Place
            } else if tapped.contains(&spot) {
                SpotReason::Tapped
            } else {
                SpotReason::Unwanted
            };
            out.push(SpotInfo {
                lat: spot.lat,
                lon: spot.lon,
                bytes,
                wanted_at: at
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
                reason,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shows_the_state_and_lets_spots_go() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let db = wf_core::db::connect(&dir.path().join("t.db")).await?;
        wf_core::db::migrate(&db).await?;
        let root = dir.path().join("map");
        let map = MapData::new(root.clone(), reqwest::Client::new(), Default::default());
        let state = map.basemap_state(&db).await;
        assert_eq!(
            (state.file, state.operator, state.due),
            (None, false, Some(Due::Missing))
        );
        assert_eq!(state.terrain_due, Some(Due::Missing));

        // An operator's file is never due
        std::fs::create_dir_all(&root)?;
        std::fs::write(crate::basemap::path(&root), b"x")?;
        let state = map.basemap_state(&db).await;
        assert!(state.operator);
        assert_eq!(state.due, None);

        let spot = Spot::of(48.78, 8.08);
        std::fs::create_dir_all(spots::dir(&root))?;
        std::fs::write(spots::path(&root, spot), b"12345")?;
        let state = map.basemap_state(&db).await;
        assert_eq!(state.spots.len(), 1);
        assert_eq!(
            (state.spots[0].bytes, state.spots[0].reason),
            (5, SpotReason::Unwanted)
        );
        assert!(map.remove_spot(48.78, 8.08).await?);
        assert!(!map.remove_spot(48.78, 8.08).await?);

        map.renew_now();
        assert!(map.basemap_state(&db).await.approved);
        map.cancel_renewal();
        assert!(!map.basemap_state(&db).await.approved);
        Ok(())
    }
}
