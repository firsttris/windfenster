//! Windfenster's domain: weather models, units, the forecast-series format and the database.
//!
//! Types that cross the HTTP boundary derive [`ts_rs::TS`]; `cargo test` writes them to
//! `web/src/bindings` (see `.cargo/config.toml`), and the SPA imports them from there.

pub mod db;
pub mod geo;
pub mod model;
pub mod series;
pub mod units;
