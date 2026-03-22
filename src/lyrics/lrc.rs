//! LRC format parser.
//!
//! Handles standard LRC timestamps: `[mm:ss.xx]`, `[mm:ss.xxx]`, `[mm:ss]`.
//! Multiple timestamps per line are expanded to separate entries.

use std::time::Duration;

use super::LyricLine;

/// Parse LRC-formatted text into timestamped lyrics lines.
/// Returns `None` if no valid timestamps are found.
pub fn parse_lrc(text: &str) -> Option<Vec<LyricLine>> {
    let mut lines: Vec<LyricLine> = Vec::new();

    for raw_line in text.lines() {
        let raw_line = raw_line.trim();
        if raw_line.is_empty() {
            continue;
        }

        // Collect all timestamps at the start of the line.
        let mut timestamps = Vec::new();
        let mut rest = raw_line;

        while rest.starts_with('[') {
            if let Some(end) = rest.find(']') {
                let tag = &rest[1..end];
                if let Some(ts) = parse_timestamp(tag) {
                    timestamps.push(ts);
                    rest = &rest[end + 1..];
                } else {
                    // Metadata tag like [ar:Artist] — skip the whole tag.
                    rest = &rest[end + 1..];
                }
            } else {
                break;
            }
        }

        let text = rest.trim().to_string();

        // Skip lines that are only metadata (no timestamps, no text).
        if timestamps.is_empty() {
            continue;
        }

        // Skip empty text lines (instrumental markers).
        if text.is_empty() {
            for ts in &timestamps {
                lines.push(LyricLine {
                    timestamp: Some(*ts),
                    text: String::new(),
                });
            }
            continue;
        }

        for ts in &timestamps {
            lines.push(LyricLine {
                timestamp: Some(*ts),
                text: text.clone(),
            });
        }
    }

    if lines.is_empty() {
        return None;
    }

    // Sort by timestamp.
    lines.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    Some(lines)
}

/// Parse a single LRC timestamp tag like "01:23.45" or "1:23.456" or "01:23".
fn parse_timestamp(tag: &str) -> Option<Duration> {
    // Try mm:ss.xx, mm:ss.xxx, or mm:ss formats.
    let (min_str, rest) = tag.split_once(':')?;
    let min: u64 = min_str.trim().parse().ok()?;

    let (secs, frac_ms) = if let Some((s, f)) = rest.split_once('.') {
        let secs: u64 = s.trim().parse().ok()?;
        // Handle 2-digit centiseconds or 3-digit milliseconds.
        let f = f.trim();
        let ms: u64 = match f.len() {
            1 => f.parse::<u64>().ok()? * 100,
            2 => f.parse::<u64>().ok()? * 10,
            3 => f.parse::<u64>().ok()?,
            _ => return None,
        };
        (secs, ms)
    } else {
        let secs: u64 = rest.trim().parse().ok()?;
        (secs, 0)
    };

    Some(Duration::from_millis(min * 60_000 + secs * 1_000 + frac_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_lrc() {
        let lrc = "[00:12.34] Hello world\n[00:15.00] Second line\n";
        let lines = parse_lrc(lrc).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[0].timestamp, Some(Duration::from_millis(12_340)));
        assert_eq!(lines[1].text, "Second line");
    }

    #[test]
    fn test_no_fraction() {
        let lrc = "[01:30] No fraction\n";
        let lines = parse_lrc(lrc).unwrap();
        assert_eq!(lines[0].timestamp, Some(Duration::from_secs(90)));
    }

    #[test]
    fn test_metadata_skipped() {
        let lrc = "[ar:Artist]\n[ti:Title]\n[00:05.00] Actual lyric\n";
        let lines = parse_lrc(lrc).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Actual lyric");
    }
}
