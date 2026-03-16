use std::path::PathBuf;
use std::time::Duration;
use std::collections::BTreeMap;

/// A single track in the library.
#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u32>,
    pub duration: Duration,
}

/// An album containing tracks.
#[derive(Debug, Clone)]
pub struct Album {
    pub name: String,
    pub artist: String,
    pub tracks: Vec<Track>,
}

/// The full library index, organized by artist → album → tracks.
#[derive(Debug, Clone, Default)]
pub struct Library {
    /// artist name → albums
    pub artists: BTreeMap<String, Vec<Album>>,
    /// Flat list of all tracks (for search, etc.)
    pub all_tracks: Vec<Track>,
    /// Whether scanning is in progress
    pub scanning: bool,
}

impl Library {
    /// Get a sorted list of all artist names.
    pub fn artist_names(&self) -> Vec<&str> {
        self.artists.keys().map(|s| s.as_str()).collect()
    }

    /// Get albums for an artist.
    pub fn albums_for(&self, artist: &str) -> &[Album] {
        self.artists.get(artist).map(|v| v.as_slice()).unwrap_or(&[])
    }
}
