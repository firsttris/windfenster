//! Places, model runs and the stored forecast series, plus the forecast the API sends.

mod api;
mod places;
mod runs;
mod stored;

pub use api::{Forecast, ModelSeries, Notice};
pub use places::{PlaceInfo, hot_places, place_info, purge, set_place_info, touch_place};
pub use runs::{Run, add_run, complete_run, confirm_run, latest_run};
pub use stored::{StoredSeries, latest_series, series_between, series_state, store_series};

/// Loading stored series failed: the database, or a blob that does not decode.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("stored series: {0}")]
    Series(#[from] crate::series::DecodeError),
}

/// One model's series for a place, decoded. Index i is the value at `start + i * step`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesData {
    pub start: i64,
    pub step_s: i64,
    pub wind: Vec<Option<f32>>,
    pub gust: Vec<Option<f32>>,
    pub dir: Vec<Option<f32>>,
    pub temp: Vec<Option<f32>>,
    pub rain: Vec<Option<f32>>,
    pub cloud: Vec<Option<f32>>,
}

impl SeriesData {
    /// A model that delivered no values at the place (outside its domain): stored as an empty
    /// series, so the place is not asked again for this model.
    pub fn is_empty(&self) -> bool {
        self.wind.is_empty()
    }

    pub fn len(&self) -> usize {
        self.wind.len()
    }

    /// The index of the value at `t`, when a step falls on it.
    pub fn index(&self, t: i64) -> Option<usize> {
        let d = t - self.start;
        (d >= 0 && self.step_s > 0 && d % self.step_s == 0)
            .then(|| (d / self.step_s) as usize)
            .filter(|i| *i < self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::GridPoint;

    #[tokio::test]
    async fn runs_series_places_and_purge() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let p = GridPoint::new(48.78, 8.08).unwrap();
        let free = GridPoint::new(47.0, 9.0).unwrap();

        let (r1, new) = add_run(&db, "icon_d2", 1_000, false).await?;
        assert!(new && !r1.complete);
        let (again, new) = add_run(&db, "icon_d2", 1_000, false).await?;
        assert!(!new);
        assert_eq!(again.id, r1.id);
        let (r2, _) = add_run(&db, "icon_d2", 2_000, false).await?;
        complete_run(&db, r2.id, 2_100).await?;
        let latest = latest_run(&db, "icon_d2").await?.unwrap();
        assert_eq!((latest.id, latest.complete), (r2.id, true));

        let data = |kn: f32| SeriesData {
            start: 3_600,
            step_s: 3_600,
            wind: vec![Some(kn), None],
            gust: vec![Some(kn + 5.0), None],
            dir: vec![Some(225.0), None],
            temp: vec![Some(17.5), None],
            rain: vec![Some(0.0), None],
            cloud: vec![Some(40.0), None],
        };
        touch_place(&db, p, true, 5_000).await?;
        touch_place(&db, free, false, 5_000).await?;
        set_place_info(&db, p, Some("Europe/Berlin"), Some(121.0)).await?;
        store_series(&db, p, "icon_d2", r1.id, &data(10.0), 1_100).await?;
        store_series(&db, p, "icon_d2", r2.id, &data(14.0), 2_100).await?;
        store_series(&db, free, "icon_d2", r2.id, &data(8.0), 2_100).await?;
        // A model with nothing at the place is stored empty.
        let (g, _) = add_run(&db, "gfs_global", 1_500, false).await?;
        store_series(&db, p, "gfs_global", g.id, &SeriesData::default(), 1_600).await?;

        let mut latest = latest_series(&db, p).await?;
        latest.sort_by(|a, b| a.model_id.cmp(&b.model_id));
        assert_eq!(latest.len(), 2);
        assert!(latest[0].data.is_empty());
        assert_eq!(latest[1].run_id, r2.id);
        assert_eq!(latest[1].data, data(14.0));
        assert_eq!(
            place_info(&db, p).await?.timezone.as_deref(),
            Some("Europe/Berlin")
        );
        assert_eq!(hot_places(&db, 4_000).await?.len(), 2);
        let mut state = series_state(&db, p).await?;
        state.sort();
        assert_eq!(
            state,
            [
                ("gfs_global".to_owned(), 1_500, true),
                ("icon_d2".to_owned(), 2_000, false)
            ]
        );

        // Run 1 is older than the cutoff; the free point was last asked for before its cutoff.
        let (runs, places) = purge(&db, 1_200, 6_000).await?;
        assert_eq!((runs, places), (1, 1));
        assert!(latest_series(&db, free).await?.is_empty());
        assert_eq!(latest_series(&db, p).await?.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn estimated_runs_take_the_real_time() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        // Guessed 06 UTC, Open-Meteo says 03 UTC: the run moves to 03 UTC.
        let (guess, _) = add_run(&db, "icon_d2", 21_600, true).await?;
        assert!(guess.estimated);
        let real = confirm_run(&db, "icon_d2", guess, 10_800).await?;
        assert_eq!(
            (real.id, real.run_at, real.estimated),
            (guess.id, 10_800, false)
        );
        assert_eq!(latest_run(&db, "icon_d2").await?, Some(real));

        // Guessed again, but the real run is already recorded: the guess goes.
        let (guess, _) = add_run(&db, "icon_d2", 21_600, true).await?;
        let kept = confirm_run(&db, "icon_d2", guess, 10_800).await?;
        assert_eq!(kept.id, real.id);
        assert_eq!(latest_run(&db, "icon_d2").await?, Some(real));
        Ok(())
    }
}
