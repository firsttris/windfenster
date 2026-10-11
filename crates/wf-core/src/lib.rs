//! Windfenster's domain: weather models, units, the forecast-series format and the database.
//!
//! Types that cross the HTTP boundary derive [`ts_rs::TS`]; `cargo test` writes them to
//! `web/src/bindings` (see `.cargo/config.toml`), and the SPA imports them from there.

pub mod account;
pub mod db;
pub mod favorite;
pub mod forecast;
pub mod gauge;
pub mod geo;
pub mod mix;
pub mod model;
pub mod observation;
pub mod series;
pub mod station;
pub mod taf;
pub mod text;
pub mod time;
pub mod units;
pub mod verification;
