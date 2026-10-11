//! Where Windfenster's data comes from: Open-Meteo for the forecast series, OurAirports and the
//! DWD for the station lists, Open-Meteo's geocoder for the search, and the scheduler that
//! keeps it all current.

pub mod forecasts;
pub mod geocode;
pub mod observations;
pub mod openmeteo;
pub mod scheduler;
pub mod stations;

use std::time::Duration;

/// HTTP client for all sources: identifies Windfenster, honours HTTPS_PROXY, times out.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!(
            "windfenster/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/firsttris/windfenster)"
        ))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
}
