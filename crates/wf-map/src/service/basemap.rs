//! The basemap's background work: the world, the region and the relief renewed when they grow
//! old, and detail fetched for the spots people look at anywhere (tracked stations, places
//! opened in the last week, spots tapped on the map), a few per round so Protomaps is not asked
//! for much at once.

use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use wf_core::admin::settings::BasemapMode;

use super::MapData;
use crate::basemap::serve::Tile;
use crate::basemap::spots::{self, Spot};

/// How often to look for new spots
const ROUND: Duration = Duration::from_secs(60);
/// How often to check whether the region is due
const REGION_EVERY_S: i64 = 86_400;
/// Spots made per round at most
const PER_ROUND: usize = 2;
/// Places and taps count for this long
pub(super) const WANTED_S: i64 = 7 * 86_400;
/// Taps remembered at most: anyone can tap, without signing in
const MAX_TAPPED: usize = 200;

impl MapData {
    /// A spot tapped on the map: its detail comes within a minute or two.
    pub fn want(&self, lat: f64, lon: f64) {
        let max = crate::basemap::tiles::MAX_LAT;
        if !(-max..=max).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return;
        }
        let spot = Spot::of(lat, lon);
        let now = wf_core::time::now();
        let mut tapped = self.tapped.lock().expect("tapped");
        tapped.retain(|(s, at)| *s != spot && now - at < WANTED_S);
        tapped.push((spot, now));
        if tapped.len() > MAX_TAPPED {
            let over = tapped.len() - MAX_TAPPED;
            tapped.drain(..over);
        }
    }

    /// A detail tile, from the spot that holds it or the main archive.
    pub async fn basemap_tile(&self, z: u8, x: u32, y: u32) -> Option<Tile> {
        self.readers.tile(&self.root, z, x, y).await
    }

    /// The background loop: a minute after the start (the station lists first), then every
    /// minute. Follows the settings: off, renew by itself, or wait for an admin to say when.
    pub async fn keep_basemap(self: std::sync::Arc<Self>, db: wf_core::db::Db) {
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Cuts a restart broke off; none runs before this loop makes one
        match spots::clear_leftovers(&self.root).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("basemap: removed {n} unfinished spot cuts"),
            Err(e) => tracing::warn!("basemap: unfinished spot cuts: {e}"),
        }
        // When the region is next to be checked: at once, then daily, and when a setting changed
        let mut region_next = 0;
        let mut settings_seen = self.settings.version();
        let board = wf_core::status::board();
        board.register("basemap", REGION_EVERY_S);
        board.register("spots", ROUND.as_secs() as i64);
        let mut tick = tokio::time::interval(ROUND);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                () = board.asked() => {}
            }
            if board.take("basemap") || self.settings.version() != settings_seen {
                settings_seen = self.settings.version();
                region_next = 0;
            }
            board.take("spots");
            if self.settings.get().basemap_mode == BasemapMode::Off {
                continue;
            }
            let now = wf_core::time::now();
            if now >= region_next {
                region_next = self.keep_region(now).await;
            }
            let report = board.start("spots");
            match self.keep_spots(&db, now).await {
                Ok(n) => report.ok(Some(n as i64)),
                Err(e) => {
                    tracing::warn!("basemap spots: {e}");
                    board.source_failed("protomaps", &e);
                    report.failed(&e);
                }
            }
        }
    }

    /// The basemap and the relief, made anew when due (in the mode "ask" only once an admin
    /// said so, and not while postponed); returns when to check them next.
    async fn keep_region(&self, now: i64) -> i64 {
        let board = wf_core::status::board();
        let settings = self.settings.get();
        let depth = self.depth();
        let due = crate::basemap::fetch::due_now(&self.root, &depth.cut(), now).await;
        let terrain_due =
            crate::basemap::terrain::due_now(&self.root, depth.terrain_zoom, now).await;
        let asked_first = settings.basemap_mode == BasemapMode::Ask
            && (due.is_some() || terrain_due.is_some())
            && !self.renewal.approved.load(Ordering::Relaxed);
        if asked_first {
            if settings.basemap_postponed_until <= now {
                self.renewal
                    .look_for_newest(&self.http, &self.urls.protomaps, now)
                    .await;
            }
            // Asked again hourly, so a new build or the end of a postponement shows soon
            return now + 3_600;
        }
        let report = board.start("basemap");
        let work = self.renew(now, depth);
        let outcome = tokio::select! {
            failed = work => failed,
            () = self.renewal.cancel.notified() => vec!["cancelled".to_owned()],
        };
        self.renewal.approved.store(false, Ordering::Relaxed);
        *self.renewal.newest.lock().expect("newest") = None;
        match outcome.len() {
            0 => report.ok(None),
            1 => report.partly(None, outcome.join("; ")),
            _ => report.failed(outcome.join("; ")),
        }
        if outcome.is_empty() {
            now + REGION_EVERY_S
        } else {
            // Again in an hour
            now + 3_600
        }
    }

    /// Makes the basemap and the relief where due; returns what failed.
    async fn renew(&self, now: i64, depth: crate::basemap::Depth) -> Vec<String> {
        let board = wf_core::status::board();
        let mut failed = Vec::new();
        let cut = depth.cut();
        let basemap = crate::basemap::fetch::keep_current(
            &self.root,
            &self.http,
            &self.urls.protomaps,
            &cut,
            now,
        );
        match basemap.await {
            Ok(_) => board.source_ok("protomaps"),
            Err(e) => {
                tracing::warn!("basemap: {e}");
                board.source_failed("protomaps", &e);
                failed.push(format!("basemap: {e}"));
            }
        }
        let terrain = crate::basemap::terrain::keep_current(
            &self.root,
            &self.http,
            &self.urls.mapterhorn,
            depth.terrain_zoom,
            now,
        );
        match terrain.await {
            Ok(_) => board.source_ok("mapterhorn"),
            Err(e) => {
                tracing::warn!("terrain: {e}");
                board.source_failed("mapterhorn", &e);
                failed.push(format!("terrain: {e}"));
            }
        }
        failed
    }

    /// One round of spots; only for our own basemap (an operator's file is left as it is).
    async fn keep_spots(
        &self,
        db: &wf_core::db::Db,
        now: i64,
    ) -> Result<usize, crate::basemap::fetch::Error> {
        // An operator's file: nothing to add. Ours: what it holds now, not what the settings
        // ask for, which a download of hours may still be bringing.
        let Some(held) = crate::basemap::fetch::read_note(&self.root)
            .await
            .and_then(|n| n.cut())
        else {
            return Ok(0);
        };
        let places = wf_core::forecast::hot_places(db, now - WANTED_S)
            .await
            .map_err(std::io::Error::other)?;
        let tapped: Vec<(f64, f64)> = {
            let t = self.tapped.lock().expect("tapped");
            // newest first: when more are wanted than kept, the latest taps count
            t.iter()
                .rev()
                .filter(|(_, at)| now - at < WANTED_S)
                .map(|(s, _)| (s.lat, s.lon))
                .collect()
        };
        // Not in a region the main file holds as deep as the spots themselves
        let deep = |s: &Spot| {
            held.region.is_some_and(|([w, south, e, n], z)| {
                z >= crate::basemap::tiles::SPOT_ZOOM
                    && (south..=n).contains(&s.lat)
                    && (w..=e).contains(&s.lon)
            })
        };
        // Pinned on the admin page first, then tracked stations and places, then taps
        let settings = self.settings.get();
        let pinned = settings.pinned_spots.iter().map(|[lat, lon]| (*lat, *lon));
        let wanted: Vec<Spot> = spots::spots_of(
            pinned
                .chain(places.iter().map(|p| (p.lat(), p.lon())))
                .chain(tapped),
        )
        .into_iter()
        .filter(|s| !deep(s))
        .collect();
        let max_spots = settings.basemap_spots as usize;
        let have = spots::on_disk(&self.root).await;
        let (missing, touch, mut remove) = spots::plan(
            &wanted,
            &have,
            SystemTime::now(),
            PER_ROUND.min(max_spots),
            max_spots,
        );
        // Spots the main file now holds as deep go: they would be served first, from an older
        // build.
        let held_deep: Vec<Spot> = have
            .iter()
            .map(|(s, _)| *s)
            .filter(|s| deep(s) && !remove.contains(s))
            .collect();
        remove.extend(held_deep);
        // Just past the world's zoom: in the region the main file has those too, elsewhere only
        // the spot
        let from_zoom = held.world_zoom + 1;
        let missing_is_empty = missing.is_empty();
        for spot in missing {
            let n = spots::make(
                &self.root,
                &self.http,
                &self.urls.protomaps,
                spot,
                from_zoom,
                now,
            )
            .await?;
            tracing::info!(
                "basemap: {n} detail tiles around {:.1}, {:.1}",
                spot.lat,
                spot.lon
            );
        }
        for spot in touch {
            spots::touch(&self.root, spot).await?;
        }
        let changed = !missing_is_empty || !remove.is_empty();
        for spot in remove {
            tokio::fs::remove_file(spots::path(&self.root, spot)).await?;
        }
        if changed {
            self.readers.forget_spots();
        }
        Ok(spots::on_disk(&self.root).await.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_a_bounded_number_of_taps() {
        let dir = tempfile::tempdir().unwrap();
        let map = MapData::new(
            dir.path().to_path_buf(),
            reqwest::Client::new(),
            Default::default(),
        );
        for i in 0..(MAX_TAPPED + 50) {
            map.want(-60.0 + i as f64 * 0.4, 10.0);
        }
        // the same spot again replaces its tap
        map.want(-60.0 + (MAX_TAPPED + 49) as f64 * 0.4, 10.0);
        let t = map.tapped.lock().unwrap();
        assert_eq!(t.len(), MAX_TAPPED);
        assert_eq!(
            t.last().unwrap().0,
            Spot::of(-60.0 + (MAX_TAPPED + 49) as f64 * 0.4, 10.0)
        );
    }
}

#[cfg(test)]
mod renewal_tests {
    use super::*;
    use serde_json::json;

    /// A server that has nothing: every file is missing (404 is not asked again)
    async fn nowhere() -> crate::service::Urls {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let none = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().fallback(|| async { axum::http::StatusCode::NOT_FOUND });
        tokio::spawn(async move { axum::serve(listener, app).await });
        crate::service::Urls {
            icon_d2: none.clone(),
            icon_global: none.clone(),
            ecmwf: none.clone(),
            protomaps: none.clone(),
            mapterhorn: none,
        }
    }

    #[tokio::test]
    async fn asks_first_in_the_mode_ask() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let db = wf_core::db::connect(&dir.path().join("t.db")).await?;
        wf_core::db::migrate(&db).await?;
        let settings = wf_core::admin::settings::SettingsStore::fresh(db, Default::default())?;
        settings.set("basemapMode", json!("ask"), 0).await?;
        let map = MapData::new(
            dir.path().join("map"),
            reqwest::Client::new(),
            nowhere().await,
        )
        .with_settings(settings.clone());
        let now = 1_791_676_800;
        // Due (nothing there), but waits for an admin: nothing is fetched, asked again hourly
        assert_eq!(map.keep_region(now).await, now + 3_600);
        assert!(!map.renewal.approved.load(Ordering::Relaxed));
        // Said "now": it tries (and fails here, nothing answers), then waits an hour
        map.renew_now();
        assert_eq!(map.keep_region(now).await, now + 3_600);
        assert!(!map.renewal.approved.load(Ordering::Relaxed));
        let t = wf_core::status::board()
            .tasks()
            .into_iter()
            .find(|t| t.id == "basemap")
            .unwrap();
        assert_eq!(t.outcome, wf_core::status::Outcome::Failed);
        Ok(())
    }
}
