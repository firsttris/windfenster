//! Station pages for search engines and link previews: the SPA's index.html with the
//! station's title, description, Open Graph tags and JSON-LD. The SPA renders the content.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use serde_json::{Map, Value, json};
use wf_core::station::{self, Station};

use crate::{AppState, spa};

/// The texts live in the SPA's message files; the server reads its few from there too.
const DE: &str = include_str!("../../../web/messages/de.json");
const EN: &str = include_str!("../../../web/messages/en.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    /// The first of de/en the browser asks for; German otherwise.
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let accept = headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        for part in accept.split(',') {
            let tag = part
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if tag.starts_with("de") {
                return Lang::De;
            }
            if tag.starts_with("en") {
                return Lang::En;
            }
        }
        Lang::De
    }

    fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    fn og_locale(self) -> &'static str {
        match self {
            Lang::De => "de_DE",
            Lang::En => "en_US",
        }
    }
}

/// A message with its `{placeholders}` filled in.
pub fn text(lang: Lang, key: &str, inputs: &[(&str, &str)]) -> String {
    let file = match lang {
        Lang::De => DE,
        Lang::En => EN,
    };
    let messages: Map<String, Value> = serde_json::from_str(file).expect("message files are JSON");
    let mut out = messages
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(key)
        .to_owned();
    for (name, value) in inputs {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

pub async fn station_page(
    State(s): State<AppState>,
    Path(slug): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let Some(index) = spa::index_html() else {
        return spa::serve(State(s), uri, headers).await;
    };
    let lang = Lang::from_headers(&headers);
    match station::by_slug(&s.db, &slug).await {
        Ok(Some(st)) => {
            let head = head(&st, lang, s.public_url.as_deref());
            Html(replace_head(&index, &head, lang)).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Html(index)).into_response(),
        Err(e) => {
            tracing::error!("station page {slug}: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Html(index)).into_response()
        }
    }
}

fn head(st: &Station, lang: Lang, public_url: Option<&str>) -> String {
    let inputs = [("name", st.name.as_str()), ("code", st.code.as_str())];
    let title = text(lang, "meta_stationTitle", &inputs);
    let description = text(lang, "meta_stationDescription", &inputs);
    let url = public_url.map(|base| format!("{}/station/{}", base.trim_end_matches('/'), st.slug));
    let mut ld = json!({
        "@context": "https://schema.org",
        "@type": "Place",
        "name": st.name,
        "geo": { "@type": "GeoCoordinates", "latitude": st.lat, "longitude": st.lon },
    });
    if let Some(country) = &st.country {
        ld["address"] = json!({
            "@type": "PostalAddress",
            "addressCountry": country,
            "addressRegion": st.region,
            "addressLocality": st.municipality,
        });
    }
    if let Some(url) = &url {
        ld["url"] = json!(url);
    }
    let mut h = format!(
        "<title>{t}</title>\n    <meta name=\"description\" content=\"{d}\" />\n    \
         <meta property=\"og:type\" content=\"website\" />\n    \
         <meta property=\"og:site_name\" content=\"Windfenster\" />\n    \
         <meta property=\"og:title\" content=\"{t}\" />\n    \
         <meta property=\"og:description\" content=\"{d}\" />\n    \
         <meta property=\"og:locale\" content=\"{l}\" />\n",
        t = esc(&title),
        d = esc(&description),
        l = lang.og_locale(),
    );
    if let Some(url) = &url {
        h.push_str(&format!(
            "    <link rel=\"canonical\" href=\"{u}\" />\n    <meta property=\"og:url\" content=\"{u}\" />\n",
            u = esc(url)
        ));
    }
    // "</" inside the JSON would end the script element.
    let ld = ld.to_string().replace("</", "<\\/");
    h.push_str(&format!(
        "    <script type=\"application/ld+json\">{ld}</script>\n    "
    ));
    h
}

/// Swaps the block between `<!--wf:head-->` and `<!--/wf:head-->` and sets `<html lang>`.
fn replace_head(index: &str, head: &str, lang: Lang) -> String {
    const START: &str = "<!--wf:head-->";
    const END: &str = "<!--/wf:head-->";
    let html = index.replacen(
        "<html lang=\"de\">",
        &format!("<html lang=\"{}\">", lang.code()),
        1,
    );
    match (html.find(START), html.find(END)) {
        (Some(a), Some(b)) if a < b => {
            format!("{}\n    {}{}", &html[..a + START.len()], head, &html[b..])
        }
        _ => html,
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::station::StationSource;

    fn edsb() -> Station {
        Station {
            slug: "edsb".into(),
            source: StationSource::Metar,
            code: "EDSB".into(),
            name: "Karlsruhe Baden-Baden Airport".into(),
            country: Some("DE".into()),
            region: Some("Baden-Württemberg".into()),
            municipality: Some("Rheinmünster".into()),
            lat: 48.7794,
            lon: 8.0805,
            elevation_m: Some(124.4),
            has_taf: true,
        }
    }

    #[test]
    fn languages() {
        let h = |v: &str| {
            let mut m = HeaderMap::new();
            m.insert(header::ACCEPT_LANGUAGE, v.parse().unwrap());
            Lang::from_headers(&m)
        };
        assert_eq!(h("en-US,en;q=0.9,de;q=0.8"), Lang::En);
        assert_eq!(h("fr-FR,de;q=0.7,en;q=0.5"), Lang::De);
        assert_eq!(h("fr"), Lang::De);
        assert_eq!(Lang::from_headers(&HeaderMap::new()), Lang::De);
    }

    #[test]
    fn station_head() {
        let index = "<html lang=\"de\"><head>\n    <!--wf:head-->\n    <title>Windfenster</title>\n    <!--/wf:head-->\n  </head></html>";
        let mut st = edsb();
        st.name = "Test <\"&'> </script>".into();
        let html = replace_head(
            index,
            &head(&st, Lang::En, Some("https://wind.example/")),
            Lang::En,
        );
        assert!(html.starts_with("<html lang=\"en\">"));
        assert!(!html.contains("<title>Windfenster</title>"));
        assert!(html.contains("<title>Test &lt;&quot;&amp;&#39;&gt; &lt;/script&gt; (EDSB): wind forecast | Windfenster</title>"));
        assert!(
            html.contains("<link rel=\"canonical\" href=\"https://wind.example/station/edsb\" />")
        );
        assert!(html.contains("\"@type\":\"Place\""));
        // The name's "</script>" cannot close the JSON-LD element.
        assert_eq!(html.matches("</script>").count(), 1);

        let de = head(&edsb(), Lang::De, None);
        assert!(de.contains(
            "<title>Karlsruhe Baden-Baden Airport (EDSB): Windvorhersage | Windfenster</title>"
        ));
        assert!(!de.contains("canonical"));
        assert!(de.contains("\"addressRegion\":\"Baden-Württemberg\""));
    }
}
