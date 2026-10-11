//! Open-Meteo forecast API: point series of several models in one request.
//!
//! Requests go out only from the server, at most once per model run and place. Without a key
//! the free API is used (non-commercial, about 10,000 calls a day); with `OPEN_METEO_API_KEY`
//! the customer API. `OPEN_METEO_BASE_URL` points to a self-hosted instance.

use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use serde::Deserialize;
use serde_json::Value;
use wf_core::forecast::SeriesData;
use wf_core::geo::GridPoint;

const FREE_BASE: &str = "https://api.open-meteo.com";
const CUSTOMER_BASE: &str = "https://customer-api.open-meteo.com";

/// Hourly variables, in the order of [`SeriesData`]'s fields.
pub const HOURLY: [&str; 6] = [
    "wind_speed_10m",
    "wind_gusts_10m",
    "wind_direction_10m",
    "temperature_2m",
    "precipitation",
    "cloud_cover",
];

/// Days requested: the table shows ten.
pub const FORECAST_DAYS: u32 = 10;

/// Default daily budget of the free API, a little under its limit of 10,000 calls.
pub const FREE_DAILY_BUDGET: u32 = 9_000;

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Self-hosted instance; default the public (or, with a key, the customer) API.
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    /// Calls per UTC day; default 9,000 without a key, unlimited with one.
    pub daily_budget: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Open-Meteo rejected the request: {0}")]
    Rejected(String),
    #[error("Open-Meteo daily budget of {0} calls is used up")]
    Budget(u32),
    #[error("Open-Meteo unreachable: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Open-Meteo answered HTTP {0}")]
    Status(u16),
    #[error("unexpected Open-Meteo response: {0}")]
    Format(String),
}

/// What Open-Meteo returned for one point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PointData {
    pub timezone: Option<String>,
    pub elevation_m: Option<f64>,
    /// One entry per requested model; empty where the model has no values at the point.
    pub series: Vec<(String, SeriesData)>,
}

struct Budget {
    limit: Option<u32>,
    day: i64,
    used: u32,
}

pub struct OpenMeteo {
    http: reqwest::Client,
    base: String,
    key: Option<String>,
    limiter: DefaultDirectRateLimiter,
    budget: Mutex<Budget>,
}

impl OpenMeteo {
    pub fn new(http: reqwest::Client, cfg: Config) -> Self {
        let base = cfg
            .base_url
            .clone()
            .unwrap_or_else(|| {
                if cfg.api_key.is_some() {
                    CUSTOMER_BASE
                } else {
                    FREE_BASE
                }
                .to_owned()
            })
            .trim_end_matches('/')
            .to_owned();
        let limit = cfg.daily_budget.or(if cfg.api_key.is_some() {
            None
        } else {
            Some(FREE_DAILY_BUDGET)
        });
        Self {
            http,
            base,
            key: cfg.api_key,
            // The free API allows 600 calls a minute; stay well below.
            limiter: RateLimiter::direct(Quota::per_second(NonZeroU32::new(5).unwrap())),
            budget: Mutex::new(Budget {
                limit,
                day: 0,
                used: 0,
            }),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn forecast_url(&self, p: GridPoint, models: &[&str]) -> String {
        let mut url = format!(
            "{}/v1/forecast?latitude={:.2}&longitude={:.2}&hourly={}&models={}\
             &wind_speed_unit=kn&timeformat=unixtime&timezone=auto&forecast_days={FORECAST_DAYS}\
             &cell_selection=nearest",
            self.base,
            p.lat(),
            p.lon(),
            HOURLY.join(","),
            models.join(","),
        );
        if let Some(key) = &self.key {
            url.push_str("&apikey=");
            url.push_str(key);
        }
        url
    }

    /// Series of `models` at a point, one request. `interactive` (someone waits for the page)
    /// gives up sooner than a background refresh.
    pub async fn forecast(
        &self,
        p: GridPoint,
        models: &[&str],
        interactive: bool,
    ) -> Result<PointData, Error> {
        self.spend(weight(HOURLY.len(), models.len(), FORECAST_DAYS))?;
        let patience = if interactive {
            Patience::INTERACTIVE
        } else {
            Patience::BACKGROUND
        };
        let body = self.get(&self.forecast_url(p, models), patience).await?;
        parse_forecast(&body, models)
    }

    /// Initialisation time of the newest run Open-Meteo serves for a dataset, from its
    /// `meta.json`; `None` if the instance does not publish one.
    pub async fn latest_run(&self, meta_id: &str) -> Result<Option<i64>, Error> {
        let url = format!("{}/data/{meta_id}/static/meta.json", self.base);
        let body = self.get(&url, Patience::ONCE).await?;
        let meta: Value =
            serde_json::from_str(&body).map_err(|e| Error::Format(format!("meta.json: {e}")))?;
        Ok(meta["last_run_initialisation_time"].as_i64())
    }

    fn spend(&self, calls: u32) -> Result<(), Error> {
        let mut b = self.budget.lock().unwrap();
        let today = wf_core::time::now().div_euclid(86_400);
        if b.day != today {
            b.day = today;
            b.used = 0;
        }
        if let Some(limit) = b.limit
            && b.used + calls > limit
        {
            return Err(Error::Budget(limit));
        }
        b.used += calls;
        Ok(())
    }

    /// GET with rate limit and retries for timeouts, 429 and 5xx.
    async fn get(&self, url: &str, patience: Patience) -> Result<String, Error> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.limiter.until_ready().await;
            let result = self.http.get(url).timeout(patience.timeout).send().await;
            let retry_after = match result {
                Ok(res) if res.status().is_success() => return Ok(res.text().await?),
                Ok(res) if res.status().as_u16() == 400 => {
                    let reason = res
                        .json::<Value>()
                        .await
                        .ok()
                        .and_then(|v| v["reason"].as_str().map(str::to_owned))
                        .unwrap_or_else(|| "HTTP 400".into());
                    return Err(Error::Rejected(reason));
                }
                Ok(res) if res.status().as_u16() == 429 || res.status().is_server_error() => {
                    if attempt >= patience.attempts {
                        return Err(Error::Status(res.status().as_u16()));
                    }
                    let backoff = patience.backoff * attempt;
                    res.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map_or(backoff, Duration::from_secs)
                        .min(Duration::from_secs(60))
                }
                Ok(res) => return Err(Error::Status(res.status().as_u16())),
                Err(e)
                    if attempt < patience.attempts
                        && (e.is_timeout() || e.is_connect() || e.is_request()) =>
                {
                    patience.backoff * attempt
                }
                Err(e) => return Err(e.into()),
            };
            tracing::debug!("Open-Meteo: retrying in {retry_after:?}");
            tokio::time::sleep(retry_after).await;
        }
    }
}

/// How long to keep trying a request.
#[derive(Debug, Clone, Copy)]
struct Patience {
    attempts: u32,
    timeout: Duration,
    /// Wait before the next attempt, times the attempt number.
    backoff: Duration,
}

impl Patience {
    const INTERACTIVE: Self = Self {
        attempts: 2,
        timeout: Duration::from_secs(15),
        backoff: Duration::from_secs(1),
    };
    const BACKGROUND: Self = Self {
        attempts: 3,
        timeout: Duration::from_secs(60),
        backoff: Duration::from_secs(5),
    };
    const ONCE: Self = Self {
        attempts: 1,
        timeout: Duration::from_secs(5),
        backoff: Duration::ZERO,
    };
}

/// How many calls Open-Meteo counts for a request: every 10 variables (per model) and every
/// two weeks count as one.
pub fn weight(variables: usize, models: usize, days: u32) -> u32 {
    let vars = (variables * models.max(1)).div_ceil(10) as u32;
    vars.max(1) * days.div_ceil(14).max(1)
}

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

/// The run a model probably has out by `now` when no `meta.json` is available: runs start
/// every 24 h / `runs_per_day` and appear about `delay_s` later.
pub fn scheduled_run(now: i64, runs_per_day: u32, delay_s: i64) -> i64 {
    let every = 86_400 / i64::from(runs_per_day.max(1));
    (now - delay_s).div_euclid(every) * every
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

    #[test]
    fn url_and_key() {
        let p = GridPoint::new(48.7794, 8.0805).unwrap();
        let free = OpenMeteo::new(reqwest::Client::new(), Config::default());
        let url = free.forecast_url(p, &["icon_d2", "gfs_global"]);
        assert!(
            url.starts_with(
                "https://api.open-meteo.com/v1/forecast?latitude=48.78&longitude=8.08&"
            )
        );
        assert!(url.contains("models=icon_d2,gfs_global"));
        assert!(url.contains("wind_speed_unit=kn") && url.contains("timeformat=unixtime"));
        assert!(!url.contains("apikey"));

        let paid = OpenMeteo::new(
            reqwest::Client::new(),
            Config {
                api_key: Some("k3y".into()),
                ..Config::default()
            },
        );
        let url = paid.forecast_url(p, &["icon_d2"]);
        assert!(url.starts_with("https://customer-api.open-meteo.com/v1/forecast?"));
        assert!(url.ends_with("&apikey=k3y"));

        let own = OpenMeteo::new(
            reqwest::Client::new(),
            Config {
                base_url: Some("http://meteo.local:8080/".into()),
                ..Config::default()
            },
        );
        assert_eq!(own.base_url(), "http://meteo.local:8080");
    }

    #[test]
    fn budget() {
        assert_eq!(weight(6, 1, 10), 1);
        assert_eq!(weight(6, 2, 10), 2);
        assert_eq!(weight(6, 5, 10), 3);
        assert_eq!(weight(6, 1, 16), 2);
        let om = OpenMeteo::new(
            reqwest::Client::new(),
            Config {
                daily_budget: Some(4),
                ..Config::default()
            },
        );
        assert!(om.spend(3).is_ok());
        assert!(matches!(om.spend(2), Err(Error::Budget(4))));
        assert!(om.spend(1).is_ok());
    }

    #[test]
    fn schedule_fallback() {
        // 8 runs a day, 2 h delay: at 13:30 UTC the 09 UTC run is out, at 14:30 the 12 UTC one.
        let day = 1_791_676_800; // 2026-10-11 00:00 UTC
        assert_eq!(
            scheduled_run(day + 13 * 3600 + 1800, 8, 7200),
            day + 9 * 3600
        );
        assert_eq!(
            scheduled_run(day + 14 * 3600 + 1800, 8, 7200),
            day + 12 * 3600
        );
        // At 00:30 with a 7 h delay the 18 UTC run is not out yet: the 12 UTC one of the day before.
        assert_eq!(scheduled_run(day + 1800, 4, 7 * 3600), day - 12 * 3600);
    }
}
