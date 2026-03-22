//! LRCLIB.net lyrics fetcher.
//!
//! Free, open API — no key required.
//! Returns synced (timestamped) lyrics when available, plain text as fallback.

use std::time::Duration;

/// Fetch lyrics from LRCLIB for a given track.
/// Returns `(synced_lrc, plain_text)` — either or both may be `Some`.
pub fn fetch_from_lrclib(
    title: &str,
    artist: &str,
    album: &str,
    duration_secs: u64,
) -> Option<(Option<String>, Option<String>)> {
    if title.is_empty() || artist.is_empty() {
        return None;
    }

    let url = format!(
        "https://lrclib.net/api/get?track_name={}&artist_name={}&album_name={}&duration={}",
        urlenc(title),
        urlenc(artist),
        urlenc(album),
        duration_secs,
    );

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .user_agent(concat!("Spelman/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent();

    let resp = agent
        .get(&url)
        .call()
        .ok()?;

    if resp.status() != 200 {
        return None;
    }

    let body: String = resp.into_body().read_to_string().ok()?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;

    let synced = json
        .get("syncedLyrics")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    let plain = json
        .get("plainLyrics")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    if synced.is_none() && plain.is_none() {
        return None;
    }

    Some((synced, plain))
}

/// Minimal percent-encoding for URL query parameters.
fn urlenc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            ' ' => out.push_str("%20"),
            _ => {
                let mut buf = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buf);
                for &b in encoded.as_bytes() {
                    out.push('%');
                    out.push_str(&format!("{:02X}", b));
                }
            }
        }
    }
    out
}
