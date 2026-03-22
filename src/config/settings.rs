use crossterm::event::KeyCode;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
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
    #[serde(default = "default_selection")]
    pub selection: String,
    #[serde(default = "default_hover")]
    pub hover: String,
}

fn default_selection() -> String { "cyan".into() }
fn default_hover() -> String { "yellow".into() }

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            accent: "cyan".into(),
            text: "white".into(),
            text_dim: "darkgray".into(),
            background: "reset".into(),
            highlight: "yellow".into(),
            error: "red".into(),
            selection: "cyan".into(),
            hover: "yellow".into(),
        }
    }
}

impl ThemeColors {
    pub fn accent(&self) -> Color { parse_color(&self.accent) }
    pub fn text(&self) -> Color { parse_color(&self.text) }
    pub fn text_dim(&self) -> Color { parse_color(&self.text_dim) }
    pub fn bg(&self) -> Color { parse_color(&self.background) }
    pub fn highlight(&self) -> Color { parse_color(&self.highlight) }
    pub fn error(&self) -> Color { parse_color(&self.error) }
    pub fn selection(&self) -> Color { parse_color(&self.selection) }
    pub fn hover(&self) -> Color { parse_color(&self.hover) }
}

/// Parse a color string into a ratatui Color.
/// Supports named colors ("cyan", "dark_gray"), hex ("#ff0000"), and "reset".
pub fn parse_color(s: &str) -> Color {
    match s.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "darkgray" | "dark_gray" => Color::DarkGray,
        "gray" | "grey" => Color::Gray,
        "lightred" | "light_red" => Color::LightRed,
        "lightgreen" | "light_green" => Color::LightGreen,
        "lightyellow" | "light_yellow" => Color::LightYellow,
        "lightblue" | "light_blue" => Color::LightBlue,
        "lightmagenta" | "light_magenta" => Color::LightMagenta,
        "lightcyan" | "light_cyan" => Color::LightCyan,
        "reset" | "default" | "none" => Color::Reset,
        hex if hex.starts_with('#') && hex.len() == 7 => {
            let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
            Color::Rgb(r, g, b)
        }
        _ => Color::White,
    }
}

// ── Bindable Actions ─────────────────────────────────────────────────────────

/// Every action that can be assigned to a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindableAction {
    Quit,
    TogglePlayPause,
    VolumeUp,
    VolumeDown,
    SeekForward,
    SeekBackward,
    TabNext,
    TabPrev,
    NextTrack,
    PrevTrack,
    Enter,
    Back,
    Backspace,
    ScrollDown,
    ScrollUp,
    // Pane switching (Home tab).
    SwitchPane,
    // Popup toggles.
    ToggleSearch,
    TogglePomodoro,
    ToggleKeybindings,
    // Context-specific (guarded by active tab in app.rs).
    ToggleEq,
    ToggleEqEnabled,
    EnqueueTrack,
    AddToPlaylist,
    ShowRecentlyPlayed,
    SavePlaylist,
    ToggleCheckbox,
    ViewTracks,
    DeletePlaylist,
    SkipPomodoro,
    CyclePomodoroStyle,
    ToggleLyrics,
    ToggleChroma,
}

impl BindableAction {
    /// Human-readable label for the settings UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Quit => "Quit",
            Self::TogglePlayPause => "Play / Pause",
            Self::VolumeUp => "Volume Up",
            Self::VolumeDown => "Volume Down",
            Self::SeekForward => "Seek Forward",
            Self::SeekBackward => "Seek Backward",
            Self::TabNext => "Next Tab",
            Self::TabPrev => "Previous Tab",
            Self::NextTrack => "Next Track",
            Self::PrevTrack => "Previous Track",
            Self::Enter => "Enter / Select",
            Self::Back => "Back / Escape",
            Self::Backspace => "Backspace",
            Self::ScrollDown => "Scroll Down",
            Self::ScrollUp => "Scroll Up",
            Self::SwitchPane => "Switch Pane",
            Self::ToggleSearch => "Search",
            Self::TogglePomodoro => "Pomodoro",
            Self::ToggleKeybindings => "Keybindings",
            Self::ToggleEq => "Toggle EQ Panel",
            Self::ToggleEqEnabled => "Toggle EQ On/Off",
            Self::EnqueueTrack => "Enqueue Track",
            Self::AddToPlaylist => "Add to Playlist",
            Self::ShowRecentlyPlayed => "Recently Played",
            Self::SavePlaylist => "New Playlist",
            Self::ToggleCheckbox => "Toggle Checkbox",
            Self::ViewTracks => "View / Cycle Style",
            Self::DeletePlaylist => "Delete Playlist",
            Self::SkipPomodoro => "Skip Pomodoro Phase",
            Self::CyclePomodoroStyle => "Cycle Timer Style",
            Self::ToggleLyrics => "Toggle Lyrics",
            Self::ToggleChroma => "Chroma Visualizer",
        }
    }

    /// All actions in display order.
    pub const ALL: &'static [BindableAction] = &[
        Self::Quit,
        Self::TogglePlayPause,
        Self::VolumeUp,
        Self::VolumeDown,
        Self::SeekForward,
        Self::SeekBackward,
        Self::ScrollDown,
        Self::ScrollUp,
        Self::Enter,
        Self::Back,
        Self::Backspace,
        Self::NextTrack,
        Self::PrevTrack,
        Self::TabNext,
        Self::TabPrev,
        Self::SwitchPane,
        Self::ToggleSearch,
        Self::TogglePomodoro,
        Self::ToggleKeybindings,
        Self::ToggleEq,
        Self::ToggleEqEnabled,
        Self::EnqueueTrack,
        Self::AddToPlaylist,
        Self::ShowRecentlyPlayed,
        Self::SavePlaylist,
        Self::ToggleCheckbox,
        Self::ViewTracks,
        Self::DeletePlaylist,
        Self::SkipPomodoro,
        Self::CyclePomodoroStyle,
        Self::ToggleLyrics,
        Self::ToggleChroma,
    ];
}

// ── Key ↔ String Conversion ─────────────────────────────────────────────────

/// Convert a crossterm KeyCode to a human-readable string for serialization.
pub fn key_to_string(code: &KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "backtab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::F(n) => format!("f{n}"),
        _ => "unknown".into(),
    }
}

/// Parse a string back into a crossterm KeyCode.
pub fn string_to_key(s: &str) -> Option<KeyCode> {
    match s {
        "space" => Some(KeyCode::Char(' ')),
        "enter" => Some(KeyCode::Enter),
        "esc" => Some(KeyCode::Esc),
        "tab" => Some(KeyCode::Tab),
        "backtab" => Some(KeyCode::BackTab),
        "backspace" => Some(KeyCode::Backspace),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "pageup" => Some(KeyCode::PageUp),
        "pagedown" => Some(KeyCode::PageDown),
        "delete" => Some(KeyCode::Delete),
        "insert" => Some(KeyCode::Insert),
        s if s.starts_with('f') => {
            s[1..].parse::<u8>().ok().map(KeyCode::F)
        }
        s if s.chars().count() == 1 => {
            s.chars().next().map(KeyCode::Char)
        }
        _ => None,
    }
}

// ── KeyBindings ──────────────────────────────────────────────────────────────

/// Configurable key→action mappings, serialized to TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    pub bindings: BTreeMap<BindableAction, Vec<String>>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        use BindableAction::*;
        let mut b = BTreeMap::new();

        b.insert(Quit, vec!["q".into(), "Q".into()]);
        b.insert(TogglePlayPause, vec!["space".into()]);
        b.insert(VolumeUp, vec!["+".into(), "=".into()]);
        b.insert(VolumeDown, vec!["-".into(), "_".into()]);
        b.insert(SeekForward, vec!["l".into()]);
        b.insert(SeekBackward, vec!["h".into()]);
        b.insert(ScrollDown, vec!["j".into(), "down".into()]);
        b.insert(ScrollUp, vec!["k".into(), "up".into()]);
        b.insert(Enter, vec!["enter".into()]);
        b.insert(Back, vec!["esc".into()]);
        b.insert(Backspace, vec!["backspace".into()]);
        b.insert(NextTrack, vec!["n".into()]);
        b.insert(PrevTrack, vec!["p".into()]);
        b.insert(TabNext, vec!["right".into()]);
        b.insert(TabPrev, vec!["left".into()]);
        b.insert(SwitchPane, vec!["tab".into(), "backtab".into()]);
        b.insert(ToggleSearch, vec!["s".into(), "S".into()]);
        b.insert(TogglePomodoro, vec!["P".into()]);
        b.insert(ToggleKeybindings, vec!["K".into()]);
        b.insert(ToggleEq, vec!["e".into()]);
        b.insert(ToggleEqEnabled, vec!["t".into()]);
        b.insert(EnqueueTrack, vec!["E".into()]);
        b.insert(AddToPlaylist, vec!["A".into()]);
        b.insert(ShowRecentlyPlayed, vec!["R".into()]);
        b.insert(SavePlaylist, vec!["a".into()]);
        b.insert(ToggleCheckbox, vec!["x".into()]);
        b.insert(ViewTracks, vec!["v".into()]);
        b.insert(DeletePlaylist, vec!["d".into()]);
        b.insert(SkipPomodoro, vec!["f".into()]);
        b.insert(CyclePomodoroStyle, vec!["v".into()]);
        b.insert(ToggleLyrics, vec!["L".into()]);
        b.insert(ToggleChroma, vec!["6".into()]);

        Self { bindings: b }
    }
}

impl KeyBindings {
    /// Build a reverse lookup: KeyCode → BindableAction.
    ///
    /// If two actions share the same key, the last one in iteration order wins.
    pub fn build_lookup(&self) -> HashMap<KeyCode, BindableAction> {
        let mut map = HashMap::new();
        for (action, keys) in &self.bindings {
            for key_str in keys {
                if let Some(code) = string_to_key(key_str) {
                    map.insert(code, *action);
                }
            }
        }
        map
    }

    /// Fill in default bindings for any actions not present in the loaded config.
    pub fn fill_missing_defaults(&mut self) {
        let defaults = Self::default();
        for (action, keys) in defaults.bindings {
            self.bindings.entry(action).or_insert(keys);
        }
    }

    /// Get the key strings for a given action.
    pub fn keys_for(&self, action: BindableAction) -> &[String] {
        self.bindings.get(&action).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get the default key strings for a given action.
    pub fn default_keys_for(action: BindableAction) -> &'static [&'static str] {
        use BindableAction::*;
        match action {
            Quit => &["q", "Q"],
            TogglePlayPause => &["space"],
            VolumeUp => &["+", "="],
            VolumeDown => &["-", "_"],
            SeekForward => &["l"],
            SeekBackward => &["h"],
            ScrollDown => &["j", "down"],
            ScrollUp => &["k", "up"],
            Enter => &["enter"],
            Back => &["esc"],
            Backspace => &["backspace"],
            NextTrack => &["n"],
            PrevTrack => &["p"],
            TabNext => &["right"],
            TabPrev => &["left"],
            SwitchPane => &["tab", "backtab"],
            ToggleSearch => &["s", "S"],
            TogglePomodoro => &["P"],
            ToggleKeybindings => &["K"],
            ToggleEq => &["e"],
            ToggleEqEnabled => &["t"],
            EnqueueTrack => &["E"],
            AddToPlaylist => &["A"],
            ShowRecentlyPlayed => &["R"],
            SavePlaylist => &["a"],
            ToggleCheckbox => &["x"],
            ViewTracks => &["v"],
            DeletePlaylist => &["d"],
            SkipPomodoro => &["f"],
            CyclePomodoroStyle => &["v"],
            ToggleLyrics => &["L"],
            ToggleChroma => &["6"],
        }
    }

    /// Set a single key for an action, removing it from any other action first.
    pub fn set_key(&mut self, action: BindableAction, key: String) {
        // Remove from any other action.
        for (_, keys) in self.bindings.iter_mut() {
            keys.retain(|k| k != &key);
        }
        // Set as the binding for this action.
        self.bindings.insert(action, vec![key]);
    }

    /// Add a key to an action, removing it from other actions first.
    pub fn add_key(&mut self, action: BindableAction, key: String) {
        // Remove from other actions.
        for (a, keys) in self.bindings.iter_mut() {
            if *a != action {
                keys.retain(|k| k != &key);
            }
        }
        // Add to this action.
        self.bindings.entry(action).or_default().push(key);
    }
}

// ── Settings ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub music_directory: Option<PathBuf>,
    pub default_volume: f32,
    pub seek_step_secs: u64,
    pub repeat_mode: RepeatMode,
    pub shuffle: ShuffleMode,
    pub theme: String,
    pub theme_colors: ThemeColors,
    #[serde(default)]
    pub keybindings: KeyBindings,
    #[serde(default = "default_true")]
    pub shimmer_enabled: bool,
    #[serde(default = "default_one")]
    pub shimmer_intensity: f32,
    #[serde(default = "default_one")]
    pub shimmer_speed: f32,
    #[serde(default)]
    pub waveform_enabled: bool,
    #[serde(default)]
    pub waveform_mode: crate::ui::widgets::waveform::WaveformMode,
    #[serde(default = "default_seekbar_width")]
    pub seekbar_width: f32,
    #[serde(default)]
    pub viz_mode: crate::ui::widgets::visualizer::VizMode,
    #[serde(default)]
    pub visualizer_bar_style: crate::ui::widgets::visualizer::BarStyle,
    #[serde(default = "default_viz_bars")]
    pub viz_bars: usize,
    #[serde(default = "default_viz_gap")]
    pub viz_gap: usize,
    #[serde(default)]
    pub show_hz_labels: bool,
    #[serde(default)]
    pub lyrics_enabled: bool,
    #[serde(default = "default_true")]
    pub lyrics_auto_fetch: bool,
    #[serde(default = "default_true")]
    pub gapless: bool,
    #[serde(default = "default_true")]
    pub replay_gain: bool,
    #[serde(default)]
    pub custom_eq_presets: Vec<CustomEqPreset>,
    #[serde(default)]
    pub chroma_enabled: bool,
    /// Lyrics backdrop darkness level in Chroma overlay: 0=off, 1=light, 2=medium, 3=heavy.
    #[serde(default = "default_chroma_backdrop")]
    pub chroma_lyrics_backdrop: u8,
}

/// A user-saved EQ preset, serializable to TOML.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomEqPreset {
    pub name: String,
    pub gains: [f32; 10],
}

fn default_true() -> bool { true }
fn default_one() -> f32 { 1.0 }
fn default_chroma_backdrop() -> u8 { 2 }
fn default_seekbar_width() -> f32 { 0.85 }
fn default_viz_bars() -> usize { 32 }
fn default_viz_gap() -> usize { 1 }

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
            keybindings: KeyBindings::default(),
            shimmer_enabled: true,
            shimmer_intensity: 1.0,
            shimmer_speed: 1.0,
            waveform_enabled: false,
            waveform_mode: crate::ui::widgets::waveform::WaveformMode::default(),
            seekbar_width: 0.85,
            viz_mode: crate::ui::widgets::visualizer::VizMode::default(),
            visualizer_bar_style: crate::ui::widgets::visualizer::BarStyle::default(),
            viz_bars: 32,
            viz_gap: 1,
            show_hz_labels: false,
            lyrics_enabled: false,
            lyrics_auto_fetch: true,
            gapless: true,
            replay_gain: true,
            custom_eq_presets: Vec::new(),
            chroma_enabled: false,
            chroma_lyrics_backdrop: 2,
        }
    }
}

impl Settings {
    /// Load settings from the config file, or return defaults.
    pub fn load() -> Self {
        let mut settings: Self = Self::config_path()
            .and_then(|path| {
                let content = std::fs::read_to_string(&path).ok()?;
                match toml::from_str(&content) {
                    Ok(settings) => Some(settings),
                    Err(e) => {
                        tracing::warn!("Failed to parse config {}: {e}", path.display());
                        None
                    }
                }
            })
            .unwrap_or_default();
        // Fill in default bindings for any new actions not in the saved config.
        settings.keybindings.fill_missing_defaults();
        settings
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

    /// Get the recently played file path: ~/.config/spelman/recent.json
    pub fn recent_tracks_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "spelman")
            .map(|d| d.config_dir().join("recent.json"))
    }
}
