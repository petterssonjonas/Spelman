use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub music_directory: Option<PathBuf>,
    pub default_volume: f32,
    pub seek_step_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        let music_dir = directories::UserDirs::new()
            .and_then(|d| d.audio_dir().map(|p| p.to_path_buf()));

        Self {
            music_directory: music_dir,
            default_volume: 0.5,
            seek_step_secs: 5,
        }
    }
}
