//! When a spot works, from its forecast hours: the hours with enough wind, from directions that
//! work there, in daylight (and, if asked, with a piece of gear that fits) make a window when they
//! hold for two hours. What was told before decides whether a window is news, changed or gone.

use crate::favorite::Sector;
use crate::gear::{self, GearItem};

/// A window is told only when it holds this long.
pub const MIN_LEN_S: i64 = 2 * 3_600;
/// Forecast hours further apart than this do not join (ECMWF steps are 1 or 3 hours).
const MAX_STEP_S: i64 = 3 * 3_600;
/// A window told before counts as changed when it starts or ends this much off …
const SHIFT_S: i64 = 2 * 3_600;
/// … or its strongest wind differs by this much (kn).
const PEAK_KN: f64 = 4.0;

/// One forecast hour at the spot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hour {
    pub at: i64,
    /// kn
    pub wind: f64,
    /// Where the wind comes from, degrees
    pub dir: f64,
}

/// What makes an hour count at a spot.
#[derive(Debug, Clone, Copy)]
pub struct Rule<'a> {
    pub min_kn: f64,
    pub sector: Option<Sector>,
    /// The account's gear, when only hours some piece fits count
    pub gear: Option<&'a [GearItem]>,
    pub lat: f64,
    pub lon: f64,
}

impl Rule<'_> {
    pub fn counts(&self, at: i64, wind: f64, dir: f64) -> bool {
        wind >= self.min_kn
            && self.sector.is_none_or(|s| s.holds(dir))
            && self.gear.is_none_or(|g| gear::best_for(g, wind).is_some())
            && sun_up(self.lat, self.lon, at)
    }
}

/// Hours in a row that count: from the first one's start to the last one's end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub start: i64,
    pub end: i64,
    /// The strongest mean wind in it, kn, and when and from where it blows
    pub peak_kn: f64,
    pub peak_at: i64,
    pub dir: f64,
}

/// The windows in the hours (ascending), at least `MIN_LEN_S` long.
pub fn windows(hours: &[Hour], rule: &Rule) -> Vec<Window> {
    let mut out = Vec::new();
    let mut open: Option<Window> = None;
    for (i, h) in hours.iter().enumerate() {
        // An hour lasts until the next one; the last as long as the one before.
        let step = match (hours.get(i + 1), i.checked_sub(1).map(|j| hours[j])) {
            (Some(n), _) => (n.at - h.at).min(MAX_STEP_S),
            (None, Some(p)) => (h.at - p.at).min(MAX_STEP_S),
            (None, None) => 3_600,
        };
        if !rule.counts(h.at, h.wind, h.dir) {
            out.extend(open.take());
            continue;
        }
        open = Some(match open {
            Some(w) if h.at <= w.end => Window {
                end: h.at + step,
                ..if h.wind > w.peak_kn {
                    Window {
                        peak_kn: h.wind,
                        peak_at: h.at,
                        dir: h.dir,
                        ..w
                    }
                } else {
                    w
                }
            },
            prev => {
                out.extend(prev);
                Window {
                    start: h.at,
                    end: h.at + step,
                    peak_kn: h.wind,
                    peak_at: h.at,
                    dir: h.dir,
                }
            }
        });
    }
    out.extend(open);
    out.retain(|w| w.end - w.start >= MIN_LEN_S);
    out
}

/// The last word on a day: the window told, or None after telling it was gone.
pub type Told = Option<Window>;

/// What to tell about a day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Notice {
    New(Window),
    Changed {
        before: Window,
        now: Window,
    },
    /// The window told is no longer in the forecast.
    Gone(Window),
}

/// What to tell, given what was told about the day (None: nothing yet) and its best window now.
/// A window that has begun is not told about again.
pub fn decide(told: Option<Told>, now: Option<Window>, at: i64) -> Option<Notice> {
    match (told.flatten(), now) {
        (None, Some(w)) => Some(Notice::New(w)),
        (Some(before), None) if before.start > at => Some(Notice::Gone(before)),
        (Some(before), Some(w)) if before.start > at && differs(&before, &w) => {
            Some(Notice::Changed { before, now: w })
        }
        _ => None,
    }
}

fn differs(a: &Window, b: &Window) -> bool {
    (a.start - b.start).abs() >= SHIFT_S
        || (a.end - b.end).abs() >= SHIFT_S
        || (a.peak_kn - b.peak_kn).abs() >= PEAK_KN
}

/// Whether the sun is above the horizon (refraction included) at a place and time, after the
/// low-precision solar position of the Astronomical Almanac (good to about 0.01°).
pub fn sun_up(lat: f64, lon: f64, at: i64) -> bool {
    // Days since J2000.0 (2000-01-01 12:00 UTC)
    let d = (at - 946_728_000) as f64 / 86_400.0;
    let g = (357.529 + 0.985_600_28 * d).to_radians();
    let q = 280.459 + 0.985_647_36 * d;
    let l = (q + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).to_radians();
    let e = (23.439 - 0.000_000_36 * d).to_radians();
    let decl = (e.sin() * l.sin()).asin();
    let ra = (e.cos() * l.sin()).atan2(l.cos()).to_degrees();
    let gmst = (280.460_618_37 + 360.985_647_366_29 * d).rem_euclid(360.0);
    let hour_angle = (gmst + lon - ra).to_radians();
    let lat = lat.to_radians();
    let elevation = (lat.sin() * decl.sin() + lat.cos() * decl.cos() * hour_angle.cos()).asin();
    elevation.to_degrees() > -0.833
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::GearKind;

    /// 2026-06-21 00:00 UTC
    const JUNE: i64 = 1_782_000_000;
    const H: i64 = 3_600;

    fn rule() -> Rule<'static> {
        Rule {
            min_kn: 12.0,
            sector: None,
            gear: None,
            lat: 48.78,
            lon: 8.08,
        }
    }

    fn hours(winds: &[f64]) -> Vec<Hour> {
        winds
            .iter()
            .enumerate()
            .map(|(i, &wind)| Hour {
                at: JUNE + (10 + i as i64) * H,
                wind,
                dir: 240.0,
            })
            .collect()
    }

    #[test]
    fn the_sun_is_up_by_day() {
        // Baden-Baden in June: up at noon (10 UTC), down at 1 UTC; Rhodes in December at 10 UTC
        assert!(sun_up(48.78, 8.08, JUNE + 10 * H));
        assert!(!sun_up(48.78, 8.08, JUNE + H));
        assert!(sun_up(36.4, 28.1, JUNE + 183 * 86_400 + 10 * H));
        // Sunrise there is about 3:20 UTC
        assert!(!sun_up(48.78, 8.08, JUNE + 3 * H));
        assert!(sun_up(48.78, 8.08, JUNE + 4 * H));
    }

    #[test]
    fn hours_that_count_make_windows() {
        let w = windows(&hours(&[8.0, 13.0, 15.0, 18.0, 11.0, 14.0, 9.0]), &rule());
        // 11–14 UTC; the single hour at 15 is too short.
        assert_eq!(
            w,
            [Window {
                start: JUNE + 11 * H,
                end: JUNE + 14 * H,
                peak_kn: 18.0,
                peak_at: JUNE + 13 * H,
                dir: 240.0
            }]
        );
        // Not from the east, when only the west works
        let west = Rule {
            sector: Some(Sector { from: 180, to: 300 }),
            ..rule()
        };
        assert_eq!(windows(&hours(&[13.0, 15.0]), &west).len(), 1);
        let east = Rule {
            sector: Some(Sector { from: 45, to: 135 }),
            ..rule()
        };
        assert!(windows(&hours(&[13.0, 15.0]), &east).is_empty());
        // Nor past dark
        let night: Vec<Hour> = hours(&[13.0, 15.0])
            .into_iter()
            .map(|h| Hour {
                at: h.at - 10 * H,
                ..h
            })
            .collect();
        assert!(windows(&night, &rule()).is_empty());
    }

    #[test]
    fn only_hours_the_gear_fits_when_asked() {
        let wing = [GearItem {
            kind: GearKind::Wing,
            size: 5.0,
            min_kn: 14.0,
            max_kn: 22.0,
        }];
        let r = Rule {
            gear: Some(&wing),
            ..rule()
        };
        // 13 kn is enough by the threshold but below the wing's range; 25 is above it.
        let w = windows(&hours(&[13.0, 15.0, 20.0, 25.0]), &r);
        assert_eq!((w[0].start, w[0].end), (JUNE + 11 * H, JUNE + 13 * H));
    }

    #[test]
    fn tells_news_changes_and_cancellations_once() {
        let at = JUNE;
        let w = Window {
            start: JUNE + 12 * H,
            end: JUNE + 16 * H,
            peak_kn: 18.0,
            peak_at: JUNE + 14 * H,
            dir: 240.0,
        };
        assert_eq!(decide(None, Some(w), at), Some(Notice::New(w)));
        assert_eq!(decide(None, None, at), None);
        // A small shift is no news.
        let near = Window {
            end: w.end + H,
            peak_kn: 20.0,
            ..w
        };
        assert_eq!(decide(Some(Some(w)), Some(near), at), None);
        let later = Window {
            start: w.start + 2 * H,
            ..w
        };
        assert_eq!(
            decide(Some(Some(w)), Some(later), at),
            Some(Notice::Changed {
                before: w,
                now: later
            })
        );
        assert_eq!(decide(Some(Some(w)), None, at), Some(Notice::Gone(w)));
        // Told gone, it comes back as news; once begun, nothing more.
        assert_eq!(decide(Some(None), Some(w), at), Some(Notice::New(w)));
        assert_eq!(decide(Some(Some(w)), None, w.start + H), None);
    }
}
