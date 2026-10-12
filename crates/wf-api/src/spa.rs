use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use rust_embed::{EmbeddedFile, RustEmbed};

use crate::AppState;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

/// The built SPA's index.html, if one is embedded.
pub fn index_html() -> Option<String> {
    Assets::get("index.html").map(|f| String::from_utf8_lossy(&f.data).into_owned())
}

/// Serves the built SPA; unknown paths get `index.html` so client-side routes work.
pub async fn serve(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    // Font stacks have spaces: /glyphs/Noto%20Sans%20Regular/0-255.pbf
    let decoded = percent_decode(uri.path());
    let path = decoded.trim_start_matches('/');
    let plain = !path.is_empty() && !path.split('/').any(|s| s == "..");
    if let Some(file) = Assets::get(path).filter(|_| plain) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        // Vite puts content-hashed files under assets/; they never change.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return embedded(&headers, path, file, mime.as_ref(), cache);
    }
    // The map's fonts: a range not shipped is missing, not a page of the app.
    if path.starts_with("glyphs/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Assets::get("index.html") {
        Some(index) => embedded(
            &headers,
            "index.html",
            index,
            "text/html; charset=utf-8",
            "no-cache",
        ),
        None => match &state.dev_ui {
            // `pnpm dev`: the SPA comes from Vite; send the browser there.
            Some(dev) => Redirect::temporary(&format!("{}{}", dev.trim_end_matches('/'), uri))
                .into_response(),
            None => (
                StatusCode::NOT_FOUND,
                "SPA not built. Run `pnpm build`, or `pnpm dev` and open http://localhost:5173.",
            )
                .into_response(),
        },
    }
}

/// `%XX` escapes of a URL path as bytes; anything malformed stays as it is.
fn percent_decode(path: &str) -> String {
    let b = path.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (
            b[i],
            b.get(i + 1).copied().and_then(hex),
            b.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The copies the build made beside a file (web/scripts/precompress.mjs), best first, as the
/// HTTP encoding and the file name's extension.
const PRECOMPRESSED: [(&str, &str); 2] = [("br", "br"), ("gzip", "gz")];

/// Whether `Accept-Encoding` takes an encoding: listed (or `*`) and not with `q=0`.
fn accepts(accept_encoding: &str, encoding: &str) -> bool {
    accept_encoding.split(',').any(|part| {
        let mut it = part.split(';');
        let name = it.next().unwrap_or("").trim();
        let refused = it.any(|p| {
            p.trim()
                .strip_prefix("q=")
                .and_then(|q| q.trim().parse::<f32>().ok())
                .is_some_and(|q| q == 0.0)
        });
        (name.eq_ignore_ascii_case(encoding) || name == "*") && !refused
    })
}

/// An embedded file with an ETag from its build-time SHA-256; a matching `If-None-Match`
/// gets `304 Not Modified`. Where the build made a Brotli or gzip copy the browser takes, that
/// copy goes out as it is, so the server compresses nothing per visitor. The tag is weak
/// because the same file goes out in several encodings.
fn embedded(
    headers: &HeaderMap,
    path: &str,
    file: EmbeddedFile,
    mime: &str,
    cache: &'static str,
) -> Response {
    let etag = format!("W/\"{}\"", hex::encode(file.metadata.sha256_hash()));
    let cache = HeaderValue::from_static(cache);
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| etag_matches(v, &etag));
    let etag = HeaderValue::from_str(&etag).expect("hex is a valid header value");
    let vary = (header::VARY, HeaderValue::from_static("accept-encoding"));
    if fresh {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag), (header::CACHE_CONTROL, cache), vary],
        )
            .into_response();
    }
    let mime = HeaderValue::from_str(mime).expect("mime type is a valid header value");
    let accept = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let copy = PRECOMPRESSED.iter().find_map(|&(encoding, ext)| {
        accepts(accept, encoding)
            .then(|| Assets::get(&format!("{path}.{ext}")))
            .flatten()
            .map(|f| (encoding, f))
    });
    let (body, encoding) = match copy {
        Some((encoding, f)) => (f.data, Some(HeaderValue::from_static(encoding))),
        None => (file.data, None),
    };
    let mut response = (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache),
            (header::ETAG, etag),
            vary,
        ],
        body,
    )
        .into_response();
    if let Some(encoding) = encoding {
        // Already encoded: the compression layer leaves it as it is
        response
            .headers_mut()
            .insert(header::CONTENT_ENCODING, encoding);
    }
    response
}

/// Weak comparison as for `If-None-Match` (RFC 9110 13.1.2): `*` or any listed tag, `W/` ignored.
fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    fn opaque(t: &str) -> &str {
        t.trim().trim_start_matches("W/")
    }
    let ours = opaque(etag);
    if_none_match
        .split(',')
        .any(|t| t.trim() == "*" || opaque(t) == ours)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_encodings_the_browser_accepts() {
        // Chrome over HTTPS, over plain http, curl without a header
        let https = "gzip, deflate, br, zstd";
        let http = "gzip, deflate";
        assert!(accepts(https, "br") && accepts(https, "gzip"));
        assert!(!accepts(http, "br") && accepts(http, "gzip"));
        assert!(!accepts("", "gzip"));
        // Refused with q=0, taken with any other weight, and by *
        assert!(!accepts("br;q=0, gzip", "br"));
        assert!(accepts("br;q=0.5", "br"));
        assert!(accepts("*", "br"));
        assert!(!accepts("gzip, *;q=0", "br"));
    }

    #[test]
    fn if_none_match() {
        let etag = "W/\"abc\"";
        assert!(etag_matches("W/\"abc\"", etag));
        assert!(etag_matches("\"abc\"", etag));
        assert!(etag_matches("\"x\", W/\"abc\"", etag));
        assert!(etag_matches("*", etag));
        assert!(!etag_matches("\"abcd\"", etag));
        assert!(!etag_matches("", etag));
    }

    #[test]
    fn decodes_paths() {
        assert_eq!(
            percent_decode("/glyphs/Noto%20Sans%20Regular/0-255.pbf"),
            "/glyphs/Noto Sans Regular/0-255.pbf"
        );
        assert_eq!(percent_decode("/M%C3%BCnster"), "/Münster");
        assert_eq!(percent_decode("/50%"), "/50%");
        assert_eq!(percent_decode("/a%zz"), "/a%zz");
    }
}
