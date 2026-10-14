//! The basemap's background work: the world, the region and the relief renewed when they grow
//! old, and detail fetched for the spots people look at anywhere (tracked stations, places
//! opened in the last week, spots tapped on the map), a few per round so Protomaps is not asked
//! for much at once.

use std::time::{Duration, SystemTime};

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
const WANTED_S: i64 = 7 * 86_400;
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
    /// minute.
    pub async fn keep_basemap(self: std::sync::Arc<Self>, db: wf_core::db::Db) {
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Cuts a restart broke off; none runs before this loop makes one
        match spots::clear_leftovers(&self.root).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("basemap: removed {n} unfinished spot cuts"),
            Err(e) => tracing::warn!("basemap: unfinished spot cuts: {e}"),
        }
        // When the region is next to be checked: at once, then daily
        let mut region_next = 0;
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
            if board.take("basemap") {
                region_next = 0;
            }
            board.take("spots");
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

    /// The basemap and the relief, made anew when due; returns when to check them next.
    async fn keep_region(&self, now: i64) -> i64 {
        let board = wf_core::status::board();
        let report = board.start("basemap");
        let mut failed = Vec::new();
        let mut next = now + REGION_EVERY_S;
        match crate::basemap::fetch::keep_current(
            &self.root,
            &self.http,
            &self.urls.protomaps,
            &self.depth.cut(),
            now,
        )
        .await
        {
            Ok(_) => board.source_ok("protomaps"),
            // Again in an hour
            Err(e) => {
                tracing::warn!("basemap: {e}");
                board.source_failed("protomaps", &e);
                failed.push(format!("basemap: {e}"));
                next = now + 3_600;
            }
        }
        let terrain = crate::basemap::terrain::keep_current(
            &self.root,
            &self.http,
            &self.urls.mapterhorn,
            self.depth.terrain_zoom,
            now,
        );
        match terrain.await {
            Ok(_) => board.source_ok("mapterhorn"),
            Err(e) => {
                tracing::warn!("terrain: {e}");
                board.source_failed("mapterhorn", &e);
                failed.push(format!("terrain: {e}"));
                next = now + 3_600;
            }
        }
        match failed.len() {
            0 => report.ok(None),
            1 => report.partly(None, failed.join("; ")),
            _ => report.failed(failed.join("; ")),
        }
        next
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
        let wanted: Vec<Spot> =
            spots::spots_of(places.iter().map(|p| (p.lat(), p.lon())).chain(tapped))
                .into_iter()
                .filter(|s| !deep(s))
                .collect();
        let have = spots::on_disk(&self.root).await;
        let (missing, touch, mut remove) = spots::plan(
            &wanted,
            &have,
            SystemTime::now(),
            PER_ROUND.min(self.depth.max_spots),
            self.depth.max_spots,
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
