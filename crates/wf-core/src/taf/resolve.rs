//! The hourly series a TAF adds up to.

use super::{TafChange, TafGroup, TafHour};

#[derive(Clone, Copy, PartialEq)]
struct Wind {
    dir: Option<f32>,
    wind: f32,
    gust: Option<f32>,
}

fn wind_of(g: &TafGroup) -> Option<Wind> {
    g.wind.map(|wind| Wind {
        dir: if g.variable || wind == 0.0 {
            None
        } else {
            g.dir
        },
        wind,
        gust: g.gust,
    })
}

fn max(a: Option<f32>, b: Option<f32>) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        _ => a.or(b),
    }
}

/// The hourly series of a TAF, one value per full hour in [valid_from, valid_to). The prevailing
/// wind follows the base group, FM and BECMG in order; during a BECMG the new wind holds from
/// the middle of the transition on, and both winds make up the span. TEMPO and PROB groups only
/// widen the span.
pub fn resolve(groups: &[TafGroup], valid_from: i64, valid_to: i64) -> Vec<TafHour> {
    let first = valid_from + (3_600 - valid_from.rem_euclid(3_600)) % 3_600;
    (first..valid_to)
        .step_by(3_600)
        .map(|at| {
            let mut cur: Option<Wind> = None;
            let mut span: Vec<f32> = Vec::new();
            for g in groups {
                let Some(w) = wind_of(g) else { continue };
                match g.change {
                    TafChange::Base | TafChange::From if g.from <= at => cur = Some(w),
                    TafChange::Becoming if g.from <= at => {
                        let by = g.by.unwrap_or(g.from);
                        if at >= by {
                            cur = Some(w);
                        } else {
                            if let Some(old) = cur {
                                span.extend([old.wind, w.wind]);
                            }
                            if at >= g.from + (by - g.from) / 2 {
                                cur = Some(w);
                            }
                        }
                    }
                    _ => {}
                }
            }
            let mut gust_high = cur.and_then(|w| w.gust);
            let mut tempo = false;
            let mut probability: Option<u8> = None;
            for g in groups.iter().filter(|g| {
                matches!(g.change, TafChange::Tempo | TafChange::Prob) && g.from <= at && at < g.to
            }) {
                if let Some(w) = wind_of(g) {
                    span.push(w.wind);
                    gust_high = max(gust_high, w.gust);
                    match g.probability {
                        Some(p) if !tempo => {
                            probability = Some(probability.map_or(p, |q| q.max(p)))
                        }
                        Some(_) => {}
                        None => {
                            tempo = true;
                            probability = None;
                        }
                    }
                }
            }
            if let Some(w) = cur {
                span.push(w.wind);
            }
            let low = span.iter().copied().reduce(f32::min);
            let high = span.iter().copied().reduce(f32::max);
            let wide = low != high;
            TafHour {
                at,
                wind: cur.map(|w| w.wind),
                gust: cur.and_then(|w| w.gust),
                dir: cur.and_then(|w| w.dir),
                low: low.filter(|_| wide),
                high: high.filter(|_| wide),
                gust_high,
                probability: probability.filter(|_| wide),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taf::fixtures::*;

    #[test]
    fn base_and_becoming() {
        // TAF EDSB 110500Z 1106/1206 22007KT CAVOK BECMG 1117/1120 VRB03KT (as decoded by
        // aviationweather.gov: the BECMG group runs 17–06 and is complete by 20).
        let mut becmg = group(
            TafChange::Becoming,
            T0 + 11 * H,
            T0 + 24 * H,
            Some((0.0, 3.0)),
        );
        becmg.variable = true;
        becmg.by = Some(T0 + 14 * H);
        let groups = [
            group(TafChange::Base, T0, T0 + 11 * H, Some((220.0, 7.0))),
            becmg,
        ];
        let hours = resolve(&groups, T0, T0 + 24 * H);
        assert_eq!(hours.len(), 24);
        let at = |h: usize| &hours[h];
        assert_eq!(
            (at(0).wind, at(0).dir, at(0).low),
            (Some(7.0), Some(220.0), None)
        );
        // 17 UTC: the change has begun, the old wind still prevails.
        assert_eq!(
            (at(11).wind, at(11).low, at(11).high),
            (Some(7.0), Some(3.0), Some(7.0))
        );
        // From the middle (18:30) on the new wind prevails; variable has no direction.
        assert_eq!((at(13).wind, at(13).dir), (Some(3.0), None));
        assert_eq!((at(14).wind, at(14).low), (Some(3.0), None));
        assert_eq!(at(23).wind, Some(3.0));
    }

    #[test]
    fn tempo_and_prob_widen_the_span() {
        let mut tempo = group(
            TafChange::Tempo,
            T0 + 2 * H,
            T0 + 6 * H,
            Some((230.0, 18.0)),
        );
        tempo.gust = Some(30.0);
        let mut prob = group(
            TafChange::Prob,
            T0 + 8 * H,
            T0 + 10 * H,
            Some((240.0, 25.0)),
        );
        prob.probability = Some(30);
        let mut base = group(TafChange::Base, T0, T0 + 12 * H, Some((220.0, 14.0)));
        base.gust = Some(24.0);
        // A TEMPO without wind (only clouds) changes nothing.
        let clouds = group(TafChange::Tempo, T0, T0 + 12 * H, None);
        let hours = resolve(&[base, tempo, prob, clouds], T0, T0 + 12 * H);
        assert_eq!(
            (hours[1].wind, hours[1].low, hours[1].gust_high),
            (Some(14.0), None, Some(24.0))
        );
        let t = &hours[2];
        assert_eq!(
            (t.wind, t.gust, t.low, t.high, t.gust_high, t.probability),
            (
                Some(14.0),
                Some(24.0),
                Some(14.0),
                Some(18.0),
                Some(30.0),
                None
            )
        );
        assert_eq!(hours[6].high, None);
        let p = &hours[8];
        assert_eq!((p.high, p.probability), (Some(25.0), Some(30)));
    }

    #[test]
    fn from_replaces_and_hours_start_on_the_hour() {
        let groups = [
            group(TafChange::Base, T0 + 1_800, T0 + 4 * H, Some((220.0, 10.0))),
            group(TafChange::From, T0 + 4 * H, T0 + 9 * H, Some((300.0, 20.0))),
        ];
        let hours = resolve(&groups, T0 + 1_800, T0 + 9 * H);
        assert_eq!(hours[0].at, T0 + H);
        assert_eq!(hours[2].wind, Some(10.0));
        assert_eq!(
            (hours[3].wind, hours[3].dir, hours[3].low),
            (Some(20.0), Some(300.0), None)
        );
    }
}
