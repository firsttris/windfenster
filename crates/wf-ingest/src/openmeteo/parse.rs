//! Parsing Open-Meteo's forecast answer into series.

use serde::Deserialize;
use serde_json::Value;
use wf_core::forecast::SeriesData;

use super::{Error, HOURLY, PointData};

#[derive(Deserialize)]
struct Response {
    timezone: Option<String>,
    elevation: Option<f64>,
    hourly: Option<serde_json::Map<String, Value>>,
}

/// Parses a forecast response. With several models Open-Meteo suffixes each variable with
/// the model id (`wind_speed_10m_icon_d2`); with one model it does not.
pub fn parse_forecast(body: &str, models: &[&str]) -> Result<PointData, Error> {
    let res: Response = serde_json::from_str(body).map_err(|e| Error::Format(e.to_string()))?;
    let hourly = res
        .hourly
        .ok_or_else(|| Error::Format("no hourly data".into()))?;
    let times: Vec<i64> = hourly
        .get("time")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Format("no time axis".into()))?
        .iter()
        .map(|t| {
            t.as_i64()
                .ok_or_else(|| Error::Format("time is not unix".into()))
        })
        .collect::<Result<_, _>>()?;
    let step = match times.as_slice() {
        [a, b, ..] => b - a,
        _ => 3_600,
    };
    if step <= 0 || times.windows(2).any(|w| w[1] - w[0] != step) {
        return Err(Error::Format("time axis is not evenly spaced".into()));
    }
    let start = times.first().copied().unwrap_or(0);

    let column = |var: &str, model: &str| -> Vec<Option<f32>> {
        let key = if models.len() == 1 {
            var.to_owned()
        } else {
            format!("{var}_{model}")
        };
        let mut v: Vec<Option<f32>> = hourly
            .get(&key)
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|x| x.as_f64().map(|f| f as f32)).collect())
            .unwrap_or_default();
        v.resize(times.len(), None);
        v
    };

    let series = models
        .iter()
        .map(|&model| {
            let [wind, gust, dir, temp, rain, cloud] = HOURLY.map(|var| column(var, model));
            let mut data = SeriesData {
                start,
                step_s: step,
                wind,
                gust,
                dir,
                temp,
                rain,
                cloud,
            };
            // Regional models end early: cut the steps without any value.
            let len = (0..times.len())
                .rev()
                .find(|&i| {
                    [
                        &data.wind,
                        &data.gust,
                        &data.dir,
                        &data.temp,
                        &data.rain,
                        &data.cloud,
                    ]
                    .iter()
                    .any(|c| c[i].is_some())
                })
                .map_or(0, |i| i + 1);
            for c in [
                &mut data.wind,
                &mut data.gust,
                &mut data.dir,
                &mut data.temp,
                &mut data.rain,
                &mut data.cloud,
            ] {
                c.truncate(len);
            }
            (model.to_owned(), data)
        })
        .collect();

    Ok(PointData {
        timezone: res.timezone,
        elevation_m: res.elevation,
        series,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of an Open-Meteo answer for two models (documentation, trimmed to 4 hours).
    const TWO_MODELS: &str = r#"{
      "latitude": 48.78, "longitude": 8.08, "generationtime_ms": 1.2,
      "utc_offset_seconds": 7200, "timezone": "Europe/Berlin", "timezone_abbreviation": "GMT+2",
      "elevation": 121.0,
      "hourly_units": {"time": "unixtime", "wind_speed_10m_icon_d2": "kn"},
      "hourly": {
        "time": [1791669600, 1791673200, 1791676800, 1791680400],
        "wind_speed_10m_icon_d2": [6.1, 7.4, null, null],
        "wind_gusts_10m_icon_d2": [9.0, 11.2, null, null],
        "wind_direction_10m_icon_d2": [220, 225, null, null],
        "temperature_2m_icon_d2": [12.3, 13.0, null, null],
        "precipitation_icon_d2": [0.0, 0.2, null, null],
        "cloud_cover_icon_d2": [40, 55, null, null],
        "wind_speed_10m_gfs_global": [5.0, 5.5, 6.0, 6.5],
        "wind_gusts_10m_gfs_global": [8.0, 8.5, 9.0, 9.5],
        "wind_direction_10m_gfs_global": [210, 212, 214, 216],
        "temperature_2m_gfs_global": [11.0, 11.5, 12.0, 12.5],
        "precipitation_gfs_global": [0.0, 0.0, 0.1, 0.0],
        "cloud_cover_gfs_global": [20, 30, 40, 50]
      }
    }"#;

    #[test]
    fn parses_suffixed_models_and_cuts_empty_tails() {
        let d = parse_forecast(TWO_MODELS, &["icon_d2", "gfs_global", "outside"]).unwrap();
        assert_eq!(d.timezone.as_deref(), Some("Europe/Berlin"));
        assert_eq!(d.elevation_m, Some(121.0));
        let (id, icon) = &d.series[0];
        assert_eq!(id, "icon_d2");
        assert_eq!((icon.start, icon.step_s, icon.len()), (1791669600, 3600, 2));
        assert_eq!(icon.wind, [Some(6.1), Some(7.4)]);
        assert_eq!(icon.dir, [Some(220.0), Some(225.0)]);
        assert_eq!(d.series[1].1.len(), 4);
        // A model without values at the point comes back empty.
        assert_eq!(d.series[2].0, "outside");
        assert!(d.series[2].1.is_empty());
    }

    #[test]
    fn parses_a_single_model_without_suffix() {
        let body = r#"{"timezone":"GMT","elevation":5,"hourly":{"time":[0,3600],
            "wind_speed_10m":[1,2],"wind_gusts_10m":[3,4],"wind_direction_10m":[0,90],
            "temperature_2m":[10,11],"precipitation":[0,0],"cloud_cover":[0,100]}}"#;
        let d = parse_forecast(body, &["ecmwf_ifs025"]).unwrap();
        assert_eq!(d.series[0].1.gust, [Some(3.0), Some(4.0)]);
    }

    #[test]
    fn rejects_broken_answers() {
        assert!(parse_forecast("{}", &["a"]).is_err());
        let uneven = r#"{"hourly":{"time":[0,3600,9000]}}"#;
        assert!(matches!(
            parse_forecast(uneven, &["a"]),
            Err(Error::Format(_))
        ));
    }
}
