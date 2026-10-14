//! Who is asking, as far as limits go: the client's IP address, and how much it may make the
//! server fetch from outside. Opening a place the server has no current forecast for costs
//! calls to Open-Meteo, whose free API allows about 10,000 a day; a script opening ever new
//! places would use them up for everyone. So each address may open only so many places the
//! server has to fetch, per hour and per day; a person browsing spots stays well below.
//!
//! The address comes from a header the proxy in front sets, `CF-Connecting-IP` by default
//! (Cloudflare Tunnel; Cloudflare overwrites what a client sends), else from the connection.
//! `X-Forwarded-For` counts by its last entry, the one the nearest proxy added.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName};

use crate::AppState;

/// How much one address may make the server fetch; set on the admin page
/// (`wf_core::admin::settings`): 30 places an hour, 100 a day, 300 searches an hour by default.
#[derive(Debug, Clone, Copy)]
pub struct Allowance {
    pub places_per_hour: u32,
    pub places_per_day: u32,
    /// Place searches (geocoder calls); beyond, stations only
    pub searches_per_hour: u32,
}

impl From<&wf_core::admin::settings::Settings> for Allowance {
    fn from(s: &wf_core::admin::settings::Settings) -> Self {
        Allowance {
            places_per_hour: s.places_per_hour,
            places_per_day: s.places_per_day,
            searches_per_hour: s.searches_per_hour,
        }
    }
}

/// The client's address, if known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub Option<IpAddr>);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, s: &AppState) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        Ok(ClientIp(client_ip(
            &parts.headers,
            s.client_ip_header.as_ref(),
            peer,
        )))
    }
}

/// The address from `header` when the request has a valid one, else the connection's.
fn client_ip(
    headers: &HeaderMap,
    header: Option<&HeaderName>,
    peer: Option<IpAddr>,
) -> Option<IpAddr> {
    let from_header = header
        .and_then(|h| headers.get(h))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|v| v.trim().parse().ok());
    if header.is_some() && from_header.is_none() {
        // The proxy that should set it is not in front: anyone could send it themselves and
        // pass the limits per client; the admin page warns.
        wf_core::status::board().missing_ip_header();
    }
    from_header.or(peer)
}

/// Requests counted per address in a fixed window: (window start, count).
type Windows = moka::future::Cache<IpAddr, (i64, u32)>;

fn windows(seconds: u64) -> Windows {
    moka::future::Cache::builder()
        .max_capacity(100_000)
        .time_to_live(Duration::from_secs(seconds))
        .build()
}

/// Counts one more request for `ip` in its window of `len` seconds; true while within `limit`.
async fn count(w: &Windows, ip: IpAddr, now: i64, len: i64, limit: u32) -> bool {
    let (_, n) = w
        .entry(ip)
        .and_upsert_with(|e| {
            std::future::ready(match e.map(|e| e.into_value()) {
                Some((start, n)) if now - start < len => (start, n.saturating_add(1)),
                _ => (now, 1),
            })
        })
        .await
        .into_value();
    n <= limit
}

pub struct Limits {
    hour: Windows,
    day: Windows,
    searches: Windows,
}

impl Limits {
    pub fn new() -> Self {
        Limits {
            hour: windows(3_600),
            day: windows(86_400),
            searches: windows(3_600),
        }
    }

    /// Whether the client may make the server fetch a place now; counts it. A client without a
    /// known address (tests, a local socket) is not limited.
    pub async fn new_place(&self, ip: ClientIp, now: i64, a: Allowance) -> bool {
        let Some(ip) = ip.0 else { return true };
        // The hour counts every try, so hammering on past its limit keeps it closed.
        count(&self.hour, ip, now, 3_600, a.places_per_hour).await
            && count(&self.day, ip, now, 86_400, a.places_per_day).await
    }

    /// Whether the client may have a place search go out to the geocoder now; counts it.
    pub async fn search(&self, ip: ClientIp, now: i64, a: Allowance) -> bool {
        let Some(ip) = ip.0 else { return true };
        count(&self.searches, ip, now, 3_600, a.searches_per_hour).await
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn takes_the_address_from_the_proxy_header_or_the_connection() {
        let cf = HeaderName::from_static("cf-connecting-ip");
        let xff = HeaderName::from_static("x-forwarded-for");
        let mut h = HeaderMap::new();
        h.insert(&cf, HeaderValue::from_static("203.0.113.7"));
        h.insert(&xff, HeaderValue::from_static("1.2.3.4, 198.51.100.9"));
        let peer = Some(ip("172.18.0.2"));
        assert_eq!(client_ip(&h, Some(&cf), peer), Some(ip("203.0.113.7")));
        // The last entry is the one the nearest proxy added; what the client wrote comes first.
        assert_eq!(client_ip(&h, Some(&xff), peer), Some(ip("198.51.100.9")));
        // No header set, or not an address: the connection's
        assert_eq!(client_ip(&h, None, peer), peer);
        h.insert(&cf, HeaderValue::from_static("not an address"));
        assert_eq!(client_ip(&h, Some(&cf), peer), peer);
        assert_eq!(client_ip(&HeaderMap::new(), Some(&cf), None), None);
    }

    const PER_HOUR: u32 = 30;
    const PER_DAY: u32 = 100;
    const A: Allowance = Allowance {
        places_per_hour: PER_HOUR,
        places_per_day: PER_DAY,
        searches_per_hour: 300,
    };

    #[tokio::test]
    async fn an_address_opens_only_so_many_new_places() {
        let l = Limits::new();
        let a = ClientIp(Some(ip("203.0.113.7")));
        let b = ClientIp(Some(ip("203.0.113.8")));
        for _ in 0..PER_HOUR {
            assert!(l.new_place(a, 0, A).await);
        }
        assert!(!l.new_place(a, 10, A).await);
        // Another address has its own count; one without an address is not limited.
        assert!(l.new_place(b, 10, A).await);
        assert!(l.new_place(ClientIp(None), 10, A).await);
        // The next hour it may go on, until the day's limit.
        let mut allowed = PER_HOUR;
        for hour in 1..24 {
            for _ in 0..PER_HOUR {
                if l.new_place(a, hour * 3_600, A).await {
                    allowed += 1;
                }
            }
        }
        assert_eq!(allowed, PER_DAY);
        // A new day starts again.
        assert!(l.new_place(a, 86_400, A).await);
    }
}
