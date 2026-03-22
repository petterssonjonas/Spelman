//! Lyrics resolution: .lrc sidecar, embedded tags, LRCLIB fetch.

pub mod fetch;
pub mod lrc;

use std::path::Path;
use std::time::Duration;

/// A single line of lyrics with an optional timestamp.
#[derive(Debug, Clone)]
pub struct LyricLine {
    /// Timestamp for synced lyrics; `None` for unsynced.
    pub timestamp: Option<Duration>,
    pub text: String,
}

/// Resolved lyrics for a track.
#[derive(Debug, Clone)]
pub enum Lyrics {
    /// Timestamped lines (from .lrc or synced LRCLIB).
    Synced(Vec<LyricLine>),
    /// Plain text lines without timestamps.
    Unsynced(Vec<String>),
}

impl Lyrics {
    /// Find the index of the current line for a given playback position.
    /// For synced lyrics, finds the last line whose timestamp ≤ elapsed.
    /// For unsynced, returns `None` (no line tracking).
    pub fn current_line_index(&self, elapsed: Duration) -> Option<usize> {
        match self {
            Lyrics::Synced(lines) => {
                if lines.is_empty() {
                    return None;
                }
                // Binary search for the last line with timestamp ≤ elapsed.
                let mut lo = 0usize;
                let mut hi = lines.len();
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    if lines[mid].timestamp.unwrap_or(Duration::ZERO) <= elapsed {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                if lo == 0 { None } else { Some(lo - 1) }
            }
            Lyrics::Unsynced(_) => None,
        }
    }

    /// Total number of lines.
    pub fn line_count(&self) -> usize {
        match self {
            Lyrics::Synced(lines) => lines.len(),
            Lyrics::Unsynced(lines) => lines.len(),
        }
    }

    /// Get the text of line at index.
    pub fn line_text(&self, idx: usize) -> &str {
        match self {
            Lyrics::Synced(lines) => &lines[idx].text,
            Lyrics::Unsynced(lines) => &lines[idx],
        }
    }
}

/// Resolve lyrics for a track using the priority chain:
/// 1. `.lrc` sidecar file next to the audio file
/// 2. Embedded lyrics tag (USLT / Vorbis LYRICS)
/// 3. LRCLIB.net fetch (if `auto_fetch` is true)
///
/// On successful LRCLIB fetch, caches the result as a `.lrc` sidecar file.
pub fn resolve_lyrics(
    audio_path: &Path,
    title: &str,
    artist: &str,
    album: &str,
    duration: Duration,
    tag: &lofty::tag::Tag,
    auto_fetch: bool,
) -> Option<Lyrics> {
    // 1. Check for .lrc sidecar.
    let lrc_path = audio_path.with_extension("lrc");
    if lrc_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&lrc_path) {
            if let Some(lines) = lrc::parse_lrc(&content) {
                tracing::info!("Loaded lyrics from sidecar: {}", lrc_path.display());
                return Some(Lyrics::Synced(lines));
            }
            // If parsing failed (no timestamps), treat as unsynced.
            let plain: Vec<String> = content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if !plain.is_empty() {
                return Some(Lyrics::Unsynced(plain));
            }
        }
    }

    // 2. Embedded lyrics from tag.
    if let Some(lyrics) = extract_embedded_lyrics(tag) {
        return Some(lyrics);
    }

    // 3. LRCLIB fetch.
    if auto_fetch {
        let duration_secs = duration.as_secs();
        if let Some((synced, plain)) =
            fetch::fetch_from_lrclib(title, artist, album, duration_secs)
        {
            // Prefer synced.
            if let Some(ref lrc_text) = synced {
                if let Some(lines) = lrc::parse_lrc(lrc_text) {
                    // Cache as .lrc sidecar.
                    cache_lrc(audio_path, lrc_text);
                    tracing::info!(
                        "Fetched synced lyrics from LRCLIB for \"{}\" by {}",
                        title, artist
                    );
                    return Some(Lyrics::Synced(lines));
                }
            }
            if let Some(ref plain_text) = plain {
                let lines: Vec<String> = plain_text
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                if !lines.is_empty() {
                    // Cache plain text too (without timestamps).
                    cache_lrc(audio_path, plain_text);
                    tracing::info!(
                        "Fetched plain lyrics from LRCLIB for \"{}\" by {}",
                        title, artist
                    );
                    return Some(Lyrics::Unsynced(lines));
                }
            }
        }
    }

    None
}

/// Extract lyrics from embedded tags (USLT for ID3v2, LYRICS for Vorbis Comments).
fn extract_embedded_lyrics(tag: &lofty::tag::Tag) -> Option<Lyrics> {
    use lofty::tag::ItemKey;

    let text = tag.get_string(&ItemKey::Lyrics)?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    // Check if embedded text is LRC-formatted (has timestamps).
    if let Some(lines) = lrc::parse_lrc(text) {
        tracing::info!("Loaded synced lyrics from embedded tag");
        return Some(Lyrics::Synced(lines));
    }

    // Plain text.
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if lines.is_empty() {
        return None;
    }

    tracing::info!("Loaded plain lyrics from embedded tag");
    Some(Lyrics::Unsynced(lines))
}

/// Write lyrics text to a .lrc sidecar file next to the audio file.
/// Falls back to ~/.cache/spelman/lyrics/ if the audio dir isn't writable.
fn cache_lrc(audio_path: &Path, content: &str) {
    let lrc_path = audio_path.with_extension("lrc");

    // Try writing next to the audio file.
    if std::fs::write(&lrc_path, content).is_ok() {
        tracing::debug!("Cached lyrics to {}", lrc_path.display());
        return;
    }

    // Fallback: ~/.cache/spelman/lyrics/<hash>.lrc
    if let Some(cache_dir) = directories::ProjectDirs::from("", "", "spelman") {
        let lyrics_dir = cache_dir.cache_dir().join("lyrics");
        if std::fs::create_dir_all(&lyrics_dir).is_ok() {
            // Use a simple hash of the path for the filename.
            let hash = simple_hash(&audio_path.to_string_lossy());
            let fallback = lyrics_dir.join(format!("{:016x}.lrc", hash));
            if std::fs::write(&fallback, content).is_ok() {
                tracing::debug!("Cached lyrics to fallback: {}", fallback.display());
            }
        }
    }
}

/// Simple non-cryptographic hash for cache filenames.
fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3); // FNV-1a prime
    }
    h
}
