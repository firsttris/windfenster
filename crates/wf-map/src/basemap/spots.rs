//! Detail where people look: each spot gets an archive of its own,
//! `{root}/basemap-spots/{lat}_{lon}.pmtiles`, with 20 km around it from zoom 9 to 14 (about
//! 10 MB), cut from the newest Protomaps build. Spots are snapped to a 0.2° grid, so places
//! close together share one. The file's modification time says when the spot was last wanted;
//! beyond `MAX_SPOTS` the longest unwanted go.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::fetch::{self, Error};
use super::tiles::{self, Area};

pub const DIR: &str = "basemap-spots";
/// Spots kept at most: about 600 MB
pub const MAX_SPOTS: usize = 60;
/// Grid spots are snapped to, degrees: a place lies at most 13 km from its spot's middle,
/// well inside the 20 km of detail.
const SNAP: f64 = 0.2;
/// A wanted spot's file is touched at most this often
const TOUCH_S: u64 = 86_400;

/// A spot: the middle of its box, on the snapping grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    pub lat: f64,
    pub lon: f64,
}

impl Spot {
    /// The spot a place belongs to.
    pub fn of(lat: f64, lon: f64) -> Self {
        let s = |v: f64| ((v / SNAP).round() * SNAP * 10.0).round() / 10.0;
        Spot {
            lat: s(lat),
            lon: s(lon),
        }
    }

    pub fn area(self) -> Area {
        tiles::spot(self.lat, self.lon)
    }

    fn file_name(self) -> String {
        format!("{:.1}_{:.1}.pmtiles", self.lat, self.lon)
    }

    fn from_file_name(name: &str) -> Option<Self> {
        let (lat, lon) = name.strip_suffix(".pmtiles")?.split_once('_')?;
        Some(Spot {
            lat: lat.parse().ok()?,
            lon: lon.parse().ok()?,
        })
    }
}

/// The spots of some places, each once.
pub fn spots_of(places: impl IntoIterator<Item = (f64, f64)>) -> Vec<Spot> {
    let mut out: Vec<Spot> = Vec::new();
    for (lat, lon) in places {
        let s = Spot::of(lat, lon);
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

pub fn dir(root: &Path) -> PathBuf {
    root.join(DIR)
}

pub fn path(root: &Path, spot: Spot) -> PathBuf {
    dir(root).join(spot.file_name())
}

/// The spots on disk with when they were last wanted, most recent first.
pub async fn on_disk(root: &Path) -> Vec<(Spot, SystemTime)> {
    let mut out = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(dir(root)).await else {
        return out;
    };
    while let Ok(Some(e)) = entries.next_entry().await {
        let name = e.file_name().to_string_lossy().into_owned();
        let (Some(spot), Ok(meta)) = (Spot::from_file_name(&name), e.metadata().await) else {
            continue;
        };
        out.push((spot, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
    }
    out.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    out
}

/// Makes the spot's archive from the newest build; returns how many tiles it holds.
pub async fn make(
    root: &Path,
    http: &reqwest::Client,
    base: &str,
    spot: Spot,
    now: i64,
) -> Result<usize, Error> {
    let (_, reader) = fetch::newest_build(http, base, now).await?;
    tokio::fs::create_dir_all(dir(root)).await?;
    let target = path(root, spot);
    let partial = target.with_extension("part");
    let n = fetch::copy(reader, &[spot.area()], &partial).await?;
    tokio::fs::rename(&partial, &target).await?;
    Ok(n)
}

/// What one round does: the wanted spots missing on disk (at most `limit`, in the order wanted),
/// the wanted ones on disk to mark as wanted now, and those beyond `MAX_SPOTS` to remove
/// (never one wanted now).
pub fn plan(
    wanted: &[Spot],
    have: &[(Spot, SystemTime)],
    now: SystemTime,
    limit: usize,
) -> (Vec<Spot>, Vec<Spot>, Vec<Spot>) {
    let missing: Vec<Spot> = wanted
        .iter()
        .filter(|s| !have.iter().any(|(h, _)| h == *s))
        .take(limit)
        .copied()
        .collect();
    let touch = have
        .iter()
        .filter(|(s, t)| {
            wanted.contains(s)
                && now.duration_since(*t).unwrap_or_default() > Duration::from_secs(TOUCH_S)
        })
        .map(|(s, _)| *s)
        .collect();
    let keep = MAX_SPOTS.saturating_sub(missing.len());
    // `have` is most recently wanted first: what falls past `keep` goes, unless wanted.
    let remove = have
        .iter()
        .skip(keep)
        .filter(|(s, _)| !wanted.contains(s))
        .map(|(s, _)| *s)
        .collect();
    (missing, touch, remove)
}

/// Marks a spot as wanted now.
pub async fn touch(root: &Path, spot: Spot) -> std::io::Result<()> {
    let file = std::fs::File::options()
        .write(true)
        .open(path(root, spot))?;
    file.set_modified(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_places_to_spots() {
        // Baden Airpark and the Rhine beach at Iffezheim share one spot.
        assert_eq!(
            Spot::of(48.7794, 8.0805),
            Spot {
                lat: 48.8,
                lon: 8.0
            }
        );
        assert_eq!(
            Spot::of(48.83, 8.11),
            Spot {
                lat: 48.8,
                lon: 8.2
            }
        );
        assert_eq!(
            spots_of([(48.78, 8.08), (48.79, 8.01), (36.41, 28.09)]).len(),
            2
        );
        let s = Spot::of(-12.31, -0.05);
        assert_eq!(Spot::from_file_name(&s.file_name()), Some(s));
        assert_eq!(s.file_name(), "-12.4_-0.0.pmtiles");
    }

    #[test]
    fn plans_a_round() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 86_400);
        let day = |d: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(d * 86_400);
        let a = Spot {
            lat: 48.8,
            lon: 8.0,
        };
        let b = Spot {
            lat: 36.4,
            lon: 28.0,
        };
        let c = Spot {
            lat: 47.6,
            lon: 9.4,
        };
        let have = vec![(a, day(9)), (b, day(5))];
        let (missing, touch, remove) = plan(&[c, a, b], &have, now, 5);
        assert_eq!(missing, vec![c]);
        // a was touched yesterday, b five days ago
        assert_eq!(touch, vec![b]);
        assert!(remove.is_empty());
        // Full: the longest unwanted goes, never a wanted one.
        let many: Vec<(Spot, SystemTime)> = (0..MAX_SPOTS)
            .map(|i| {
                (
                    Spot {
                        lat: i as f64,
                        lon: 0.0,
                    },
                    day(9 - (i as u64 % 9)),
                )
            })
            .collect();
        let (_, _, remove) = plan(&[c], &many, now, 5);
        assert_eq!(remove.len(), 1);
        assert_eq!(remove[0], many[MAX_SPOTS - 1].0);
    }
}
