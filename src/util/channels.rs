use std::path::PathBuf;
use std::time::Duration;

/// Commands sent from the UI thread to the audio engine.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    /// Load and play a file.
    Play(PathBuf),
    /// Pause playback.
    Pause,
    /// Resume playback.
    Resume,
    /// Toggle play/pause.
    TogglePlayPause,
    /// Stop playback entirely.
    Stop,
    /// Seek to an absolute position.
    Seek(Duration),
    /// Set volume (0.0 to 1.0).
    SetVolume(f32),
}

/// Events sent from the audio engine back to the UI thread.
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Playback started for a track.
    Playing {
        path: PathBuf,
        duration: Duration,
        sample_rate: u32,
        channels: u16,
    },
    /// Current playback position updated.
    Position(Duration),
    /// Playback was paused.
    Paused,
    /// Playback was resumed.
    Resumed,
    /// Playback stopped (track ended or was stopped).
    Stopped,
    /// An error occurred.
    Error(String),
    /// Decoded audio level for simple visualizer (RMS of recent samples, 0.0-1.0).
    Level(f32),
}
