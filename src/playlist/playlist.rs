use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A named playlist — a saved list of track paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<PathBuf>,
}

impl Playlist {
    pub fn new(name: String, tracks: Vec<PathBuf>) -> Self {
        Self { name, tracks }
    }
}

/// Manages loading and saving playlists from disk.
///
/// Playlists are stored as individual TOML files in `~/.config/spelman/playlists/`.
pub struct PlaylistManager;

impl PlaylistManager {
    /// Directory where playlists are stored.
    fn playlists_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "spelman")
            .map(|d| d.config_dir().join("playlists"))
    }

    /// Load all saved playlists from disk.
    pub fn load_all() -> Vec<Playlist> {
        let dir = match Self::playlists_dir() {
            Some(d) => d,
            None => return Vec::new(),
        };

        if !dir.is_dir() {
            return Vec::new();
        }

        let mut playlists = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        match toml::from_str::<Playlist>(&content) {
                            Ok(pl) => playlists.push(pl),
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse playlist {}: {e}",
                                    path.display()
                                );
                            }
                        }
                    }
                }
            }
        }

        playlists.sort_by(|a, b| a.name.cmp(&b.name));
        playlists
    }

    /// Save a playlist to disk.
    pub fn save(playlist: &Playlist) -> anyhow::Result<()> {
        let dir = Self::playlists_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
        std::fs::create_dir_all(&dir)?;

        // Sanitize name for filename.
        let filename: String = playlist
            .name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{filename}.toml"));
        let content = toml::to_string_pretty(playlist)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Delete a playlist from disk.
    pub fn delete(name: &str) -> anyhow::Result<()> {
        let dir = Self::playlists_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        let filename: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{filename}.toml"));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}
