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

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .user_agent(concat!("Spelman/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent();

    let resp = agent
        .get("https://lrclib.net/api/get")
        .query("track_name", title)
        .query("artist_name", artist)
        .query("album_name", album)
        .query("duration", duration_secs.to_string())
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
