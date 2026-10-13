//! Open-Meteo forecast API: point series of several models in one request.
//!
//! Requests go out only from the server, at most once per model run and place. Without a key
//! the free API is used (non-commercial, about 10,000 calls a day); with `OPEN_METEO_API_KEY`
//! the customer API. `OPEN_METEO_BASE_URL` points to a self-hosted instance.

use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use serde_json::Value;
use wf_core::forecast::SeriesData;
use wf_core::geo::GridPoint;

mod parse;
pub use parse::parse_forecast;

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
/// This part of the budget stays for the refreshes: places opened by someone may use the rest
/// (a third kept: 6,000 of 9,000 for them).
const KEPT_FOR_REFRESH: u32 = 3;

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
        self.spend(
            weight(HOURLY.len(), models.len(), FORECAST_DAYS),
            interactive,
        )?;
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
        let body = self.get(&url, Patience::META).await?;
        let meta: Value =
            serde_json::from_str(&body).map_err(|e| Error::Format(format!("meta.json: {e}")))?;
        Ok(meta["last_run_initialisation_time"].as_i64())
    }

    /// Counts calls against the day's budget. Places opened by someone (`interactive`) may use
    /// only part of it: the rest stays for keeping the places people use current, however many
    /// new ones get opened.
    fn spend(&self, calls: u32, interactive: bool) -> Result<(), Error> {
        let mut b = self.budget.lock().unwrap();
        let today = wf_core::time::now().div_euclid(86_400);
        if b.day != today {
            b.day = today;
            b.used = 0;
        }
        if let Some(limit) = b.limit {
            let usable = if interactive {
                limit - limit / KEPT_FOR_REFRESH
            } else {
                limit
            };
            if b.used + calls > usable {
                return Err(Error::Budget(limit));
            }
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
    /// Run metadata: small, asked every five minutes, so two short tries.
    const META: Self = Self {
        attempts: 2,
        timeout: Duration::from_secs(5),
        backoff: Duration::from_secs(1),
    };
}

/// How many calls Open-Meteo counts for a request: every 10 variables (per model) and every
/// two weeks count as one.
pub fn weight(variables: usize, models: usize, days: u32) -> u32 {
    let vars = (variables * models.max(1)).div_ceil(10) as u32;
    vars.max(1) * days.div_ceil(14).max(1)
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
        assert!(om.spend(3, false).is_ok());
        assert!(matches!(om.spend(2, false), Err(Error::Budget(4))));
        assert!(om.spend(1, false).is_ok());
        // Places opened by someone get two thirds; the rest stays for the refreshes.
        let om = OpenMeteo::new(
            reqwest::Client::new(),
            Config {
                daily_budget: Some(9),
                ..Config::default()
            },
        );
        assert!(om.spend(6, true).is_ok());
        assert!(matches!(om.spend(1, true), Err(Error::Budget(9))));
        assert!(om.spend(3, false).is_ok());
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
