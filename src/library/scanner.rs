use std::path::Path;
use std::collections::BTreeMap;

use crossbeam_channel::Sender;
use lofty::file::AudioFile;
use lofty::file::TaggedFileExt;
use lofty::tag::Accessor;

use super::types::{Album, Library, Track};

/// Events sent from scanner thread back to the main thread.
#[derive(Debug)]
pub enum ScanEvent {
    /// Scanning started.
    Started,
    /// A batch of tracks was found (sent periodically).
    Progress { found: usize },
    /// Scanning completed with the full library.
    Complete(Library),
    /// Scanning encountered an error (non-fatal, scanning continues).
    Error(String),
}

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma",
];

/// Scan a directory tree for audio files and build a Library.
/// Sends ScanEvents via the channel as it progresses.
pub fn scan_directory(root: &Path, tx: Sender<ScanEvent>) {
    let _ = tx.send(ScanEvent::Started);

    let mut tracks = Vec::new();
    walk_dir(root, &mut tracks, &tx);

    let _ = tx.send(ScanEvent::Progress { found: tracks.len() });

    // Sort tracks by artist, album, track number.
    tracks.sort_by(|a, b| {
        a.artist
            .to_lowercase()
            .cmp(&b.artist.to_lowercase())
            .then(a.album.to_lowercase().cmp(&b.album.to_lowercase()))
            .then(a.track_number.cmp(&b.track_number))
            .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    // Group into artists → albums.
    let mut artists: BTreeMap<String, Vec<Album>> = BTreeMap::new();

    // Group tracks by (artist, album)
    let mut album_map: BTreeMap<(String, String), Vec<Track>> = BTreeMap::new();
    for track in &tracks {
        let artist = if track.artist.is_empty() {
            "Unknown Artist".to_string()
        } else {
            track.artist.clone()
        };
        let album = if track.album.is_empty() {
            "Unknown Album".to_string()
        } else {
            track.album.clone()
        };
        album_map
            .entry((artist, album))
            .or_default()
            .push(track.clone());
    }

    for ((artist, album_name), album_tracks) in album_map {
        let album = Album {
            name: album_name,
            artist: artist.clone(),
            tracks: album_tracks,
        };
        artists.entry(artist).or_default().push(album);
    }

    let library = Library {
        artists,
        all_tracks: tracks,
        scanning: false,
    };

    let _ = tx.send(ScanEvent::Complete(library));
}

fn walk_dir(dir: &Path, tracks: &mut Vec<Track>, tx: &Sender<ScanEvent>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            let _ = tx.send(ScanEvent::Error(format!("Cannot read {}: {e}", dir.display())));
            return;
        }
    };

    let mut subdirs = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if is_audio_file(&path) {
            if let Some(track) = scan_file(&path) {
                tracks.push(track);
                // Send progress every 100 tracks.
                if tracks.len() % 100 == 0 {
                    let _ = tx.send(ScanEvent::Progress { found: tracks.len() });
                }
            }
        }
    }

    // Sort subdirs for deterministic ordering.
    subdirs.sort();
    for subdir in subdirs {
        walk_dir(&subdir, tracks, tx);
    }
}

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn scan_file(path: &Path) -> Option<Track> {
    let tagged = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;

    let tag = tagged.primary_tag().or(tagged.first_tag());

    let title = tag
        .and_then(|t| t.title().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });

    let artist = tag
        .and_then(|t| t.artist().map(|s| s.to_string()))
        .unwrap_or_default();

    let album = tag
        .and_then(|t| t.album().map(|s| s.to_string()))
        .unwrap_or_default();

    let track_number = tag.and_then(|t| t.track());

    let duration = tagged
        .properties()
        .duration();

    Some(Track {
        path: path.to_path_buf(),
        title,
        artist,
        album,
        track_number,
        duration,
    })
}
