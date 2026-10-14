//! Whether a newer Windfenster is out: once a day the server asks GitHub for the newest release,
//! and the admin page says so. Nothing is installed by itself.

use std::time::Duration;

use serde::Deserialize;
use wf_core::status::{self, Release};

/// The newest release of the project on GitHub
const LATEST: &str = "https://api.github.com/repos/firsttris/windfenster/releases/latest";
const EVERY: Duration = Duration::from_secs(86_400);

#[derive(Deserialize)]
struct Latest {
    tag_name: String,
    html_url: String,
}

/// Looks for the newest release daily; `url` overrides GitHub's (tests: the stand-in).
pub async fn keep_looking(http: reqwest::Client, url: Option<String>) {
    let url = url.unwrap_or_else(|| LATEST.to_owned());
    let board = status::board();
    board.register("release", EVERY.as_secs() as i64);
    let mut tick = tokio::time::interval(EVERY);
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            () = board.asked() => if !board.take("release") { continue },
        }
        let report = board.start("release");
        match look(&http, &url).await {
            Ok(r) => {
                board.source_ok("github");
                board.set_release(r);
                report.ok(None);
            }
            Err(e) => {
                tracing::debug!("newest release: {e}");
                board.source_failed("github", &e);
                report.failed(&e);
            }
        }
    }
}

async fn look(http: &reqwest::Client, url: &str) -> Result<Release, reqwest::Error> {
    let latest: Latest = http
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(Release {
        version: latest.tag_name.trim_start_matches('v').to_owned(),
        url: latest.html_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_the_newest_release() {
        let app = axum::Router::new().route(
            "/latest",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({
                    "tag_name": "v0.1.17",
                    "html_url": "https://github.com/firsttris/windfenster/releases/tag/v0.1.17"
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/latest", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let r = look(&http, &url).await.unwrap();
        assert_eq!(r.version, "0.1.17");
    }
}
