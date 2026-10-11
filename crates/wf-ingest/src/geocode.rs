//! Place search through Open-Meteo's geocoding API (GeoNames data).

use serde::Deserialize;
use wf_core::station::PlaceHit;

const FREE_BASE: &str = "https://geocoding-api.open-meteo.com";
const CUSTOMER_BASE: &str = "https://customer-geocoding-api.open-meteo.com";

pub struct Geocoder {
    http: reqwest::Client,
    base: String,
    key: Option<String>,
}

impl Geocoder {
    pub fn new(http: reqwest::Client, base_url: Option<String>, api_key: Option<String>) -> Self {
        let base = base_url
            .unwrap_or_else(|| {
                if api_key.is_some() {
                    CUSTOMER_BASE
                } else {
                    FREE_BASE
                }
                .into()
            })
            .trim_end_matches('/')
            .to_owned();
        Self {
            http,
            base,
            key: api_key,
        }
    }

    /// Up to `count` places whose name starts with `query`, names in `language` (de, en).
    pub async fn search(
        &self,
        query: &str,
        language: &str,
        count: u8,
    ) -> anyhow::Result<Vec<PlaceHit>> {
        let mut req = self.http.get(format!("{}/v1/search", self.base)).query(&[
            ("name", query),
            ("count", &count.to_string()),
            ("language", language),
            ("format", "json"),
        ]);
        if let Some(key) = &self.key {
            req = req.query(&[("apikey", key)]);
        }
        let body = req.send().await?.error_for_status()?.text().await?;
        parse(&body)
    }
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Result>,
}

#[derive(Deserialize)]
struct Result {
    name: String,
    latitude: f64,
    longitude: f64,
    country_code: Option<String>,
    admin1: Option<String>,
}

/// Open-Meteo leaves out `results` when nothing matches.
pub fn parse(body: &str) -> anyhow::Result<Vec<PlaceHit>> {
    let res: Response = serde_json::from_str(body)?;
    Ok(res
        .results
        .into_iter()
        .map(|r| PlaceHit {
            name: r.name,
            admin: r.admin1,
            country: r.country_code,
            lat: r.latitude,
            lon: r.longitude,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_results() {
        // Shape of the documented answer, trimmed.
        let body = r#"{"results":[{"id":2950159,"name":"Berlin","latitude":52.52437,"longitude":13.41053,
            "elevation":74.0,"feature_code":"PPLC","country_code":"DE","admin1":"Land Berlin",
            "timezone":"Europe/Berlin","population":3426354,"country":"Deutschland"}],
            "generationtime_ms":0.9}"#;
        let hits = parse(body).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Berlin");
        assert_eq!(hits[0].country.as_deref(), Some("DE"));
        assert_eq!(hits[0].admin.as_deref(), Some("Land Berlin"));
        assert!(parse(r#"{"generationtime_ms":0.5}"#).unwrap().is_empty());
    }
}
