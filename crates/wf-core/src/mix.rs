//! The Windfenster mix: per hour the weighted mean of every model with a value then; the
//! direction is that of the weighted mean wind vector.

use crate::forecast::SeriesData;

/// The mix's values at one time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Value {
    pub wind: Option<f32>,
    pub gust: Option<f32>,
    pub dir: Option<f32>,
    pub temp: Option<f32>,
    pub rain: Option<f32>,
    pub cloud: Option<f32>,
}

/// Weighted mean over the inputs that have a value of one quantity at index-free time `t`.
fn mean(
    inputs: &[(&str, &SeriesData)],
    t: i64,
    weight: &impl Fn(&str) -> f64,
    pick: impl Fn(&SeriesData) -> &Vec<Option<f32>>,
) -> Option<f32> {
    let (mut sum, mut total) = (0.0, 0.0);
    for (model, data) in inputs {
        if let Some(v) = data.index(t).and_then(|i| pick(data)[i]) {
            let w = weight(model);
            sum += w * f64::from(v);
            total += w;
        }
    }
    (total > 0.0).then(|| (sum / total) as f32)
}

/// The mix at time `t`. Wind, gusts, temperature, rain and cloud are weighted means; the
/// direction comes from the mean of the wind vectors (weight × speed), so a calm model does
/// not pull it around.
pub fn at(inputs: &[(&str, &SeriesData)], t: i64, weight: impl Fn(&str) -> f64) -> Value {
    let (mut u, mut v) = (0.0_f64, 0.0_f64);
    for (model, data) in inputs {
        if let Some(i) = data.index(t)
            && let (Some(speed), Some(dir)) = (data.wind[i], data.dir[i])
        {
            let w = weight(model) * f64::from(speed);
            let rad = f64::from(dir).to_radians();
            u += w * rad.sin();
            v += w * rad.cos();
        }
    }
    let dir = (u.hypot(v) > 1e-6).then(|| (u.atan2(v).to_degrees().rem_euclid(360.0)) as f32);
    Value {
        wind: mean(inputs, t, &weight, |d| &d.wind),
        gust: mean(inputs, t, &weight, |d| &d.gust),
        dir,
        temp: mean(inputs, t, &weight, |d| &d.temp),
        rain: mean(inputs, t, &weight, |d| &d.rain),
        cloud: mean(inputs, t, &weight, |d| &d.cloud),
    }
}

/// The mix as an hourly series over everything the inputs cover; `weight(model, t)`.
pub fn series(inputs: &[(&str, &SeriesData)], weight: impl Fn(&str, i64) -> f64) -> SeriesData {
    let start = inputs.iter().map(|(_, d)| d.start).min();
    let end = inputs
        .iter()
        .map(|(_, d)| d.start + d.step_s * d.len() as i64)
        .max();
    let (Some(start), Some(end)) = (start, end) else {
        return SeriesData::default();
    };
    let first = start + (3_600 - start.rem_euclid(3_600)) % 3_600;
    let mut out = SeriesData {
        start: first,
        step_s: 3_600,
        ..SeriesData::default()
    };
    for t in (first..end).step_by(3_600) {
        let v = at(inputs, t, |m| weight(m, t));
        out.wind.push(v.wind);
        out.gust.push(v.gust);
        out.dir.push(v.dir);
        out.temp.push(v.temp);
        out.rain.push(v.rain);
        out.cloud.push(v.cloud);
    }
    // Hours at the end where no model has wind any more are left out.
    while out.wind.last() == Some(&None) {
        for q in [
            &mut out.wind,
            &mut out.gust,
            &mut out.dir,
            &mut out.temp,
            &mut out.rain,
            &mut out.cloud,
        ] {
            q.pop();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(start: i64, wind: &[f32], dir: &[f32]) -> SeriesData {
        let n = wind.len();
        SeriesData {
            start,
            step_s: 3_600,
            wind: wind.iter().map(|v| Some(*v)).collect(),
            gust: vec![None; n],
            dir: dir.iter().map(|v| Some(*v)).collect(),
            temp: vec![Some(10.0); n],
            rain: vec![Some(0.0); n],
            cloud: vec![Some(50.0); n],
        }
    }

    #[test]
    fn weighted_mean_and_vector_direction() {
        let a = data(0, &[10.0, 12.0], &[350.0, 350.0]);
        let b = data(0, &[20.0, 20.0, 18.0], &[10.0, 10.0, 10.0]);
        let inputs = [("a", &a), ("b", &b)];
        // Equal weights: (10 + 20) / 2; 350° and 10° average to about north, not to 180°,
        // pulled towards the stronger wind from 10°.
        let v = at(&inputs, 0, |_| 1.0);
        assert_eq!(v.wind, Some(15.0));
        let d = v.dir.unwrap();
        assert!((0.0..10.0).contains(&d), "{d}");
        // 1/MAE²: a with MAE 1, b with MAE 2 → weights 1 and 0.25
        let w = |m: &str| if m == "a" { 1.0 } else { 0.25 };
        assert_eq!(at(&inputs, 0, w).wind, Some(12.0));
        // Where only b has a value, the mix is b.
        let s = series(&inputs, |m, _| w(m));
        assert_eq!((s.start, s.len()), (0, 3));
        assert_eq!(s.wind[2], Some(18.0));
        assert_eq!(s.temp[0], Some(10.0));
    }

    #[test]
    fn no_inputs_no_series() {
        assert!(series(&[], |_, _| 1.0).is_empty());
        let calm = data(0, &[0.0], &[0.0]);
        assert_eq!(at(&[("a", &calm)], 0, |_| 1.0).dir, None);
    }
}
