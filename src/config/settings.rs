use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ShuffleMode {
    Off,
    On,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub accent: String,
    pub text: String,
    pub text_dim: String,
    pub background: String,
    pub highlight: String,
    pub error: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            accent: "cyan".into(),
            text: "white".into(),
            text_dim: "darkgray".into(),
            background: "reset".into(),
            highlight: "yellow".into(),
            error: "red".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub music_directory: Option<PathBuf>,
    pub default_volume: f32,
    pub seek_step_secs: u64,
    pub repeat_mode: RepeatMode,
    pub shuffle: ShuffleMode,
    pub theme: String,
    pub theme_colors: ThemeColors,
}

impl Default for Settings {
    fn default() -> Self {
        let music_dir = directories::UserDirs::new()
            .and_then(|d| d.audio_dir().map(|p| p.to_path_buf()));

        Self {
            music_directory: music_dir,
            default_volume: 0.5,
            seek_step_secs: 5,
            repeat_mode: RepeatMode::Off,
            shuffle: ShuffleMode::Off,
            theme: "default".into(),
            theme_colors: ThemeColors::default(),
        }
    }
}

impl Settings {
    /// Load settings from the config file, or return defaults.
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| {
                let content = std::fs::read_to_string(&path).ok()?;
                toml::from_str(&content).ok()
            })
            .unwrap_or_default()
    }

    /// Save settings to the config file.
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = toml::to_string_pretty(self)?;
            std::fs::write(path, content)?;
        }
        Ok(())
    }

    /// Get the config file path: ~/.config/spelman/config.toml
    fn config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "spelman")
            .map(|d| d.config_dir().join("config.toml"))
    }
}
