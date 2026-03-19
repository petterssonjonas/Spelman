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

    /// Iterate over every track in the library, in artist → album → track order.
    ///
    /// This avoids the memory cost of a separate flat `Vec<Track>` by walking
    /// the artist tree on demand.
    pub fn all_tracks(&self) -> impl Iterator<Item = &Track> {
        self.artists
            .values()
            .flat_map(|albums| albums.iter())
            .flat_map(|album| album.tracks.iter())
    }

    /// Total number of tracks across all artists and albums.
    ///
    /// Counts without allocating a flat collection.
    pub fn track_count(&self) -> usize {
        self.artists
            .values()
            .flat_map(|albums| albums.iter())
            .map(|album| album.tracks.len())
            .sum()
    }
}
