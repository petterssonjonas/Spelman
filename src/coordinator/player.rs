use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::audio::engine::AudioEngine;
use crate::config::settings::Settings;
use crate::playlist::queue::Queue;
use crate::ui::albumart::{self, AlbumArt, ArtCell};
use crate::ui::tabs::playing::{PlaybackState, PlayingState};
use crate::util::channels::{AudioCommand, AudioEvent};

/// Result of background metadata + album art loading.
struct TrackMeta {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    art_cells: Option<Vec<Vec<ArtCell>>>,
}

/// Coordinates all playback concerns: engine, queue, playing state,
/// metadata loading, and album art.
pub struct PlayerCoordinator {
    engine: AudioEngine,
    pub playing: PlayingState,
    pub queue: Queue,
    pub album_art: AlbumArt,
    meta_rx: Option<crossbeam_channel::Receiver<TrackMeta>>,
}

impl PlayerCoordinator {
    pub fn new(default_volume: f32) -> Self {
        let engine = AudioEngine::new();
        let mut playing = PlayingState::default();
        playing.volume = default_volume;

        Self {
            engine,
            playing,
            queue: Queue::new(),
            album_art: AlbumArt::default(),
            meta_rx: None,
        }
    }

    /// Send a command directly to the audio engine.
    pub fn send(&self, cmd: AudioCommand) {
        self.engine.send(cmd);
    }

    /// Play a file, clearing the queue first (used for CLI arg).
    pub fn play_file(&mut self, path: PathBuf) {
        self.queue.clear();
        self.queue.push(path.clone());
        self.engine.send(AudioCommand::Play(path));
    }

    /// Enqueue a track, set it as current, and start playing.
    pub fn enqueue_and_play(&mut self, path: PathBuf) {
        // Check if track is already in queue.
        if let Some(idx) = self.queue.tracks().iter().position(|p| p == &path) {
            self.queue.set_current(idx);
        } else {
            self.queue.push(path.clone());
            let idx = self.queue.tracks().len() - 1;
            self.queue.set_current(idx);
        }
        self.engine.send(AudioCommand::Play(path));
    }

    /// Handle spacebar: toggle play/pause, or start from queue if stopped.
    pub fn toggle_play_pause(&mut self) {
        if self.playing.playback == PlaybackState::Stopped {
            if let Some(path) = self.queue.current_track().cloned() {
                self.engine.send(AudioCommand::Play(path));
            }
        } else {
            self.engine.send(AudioCommand::TogglePlayPause);
        }
    }

    /// Advance to the next track in the queue.
    pub fn play_next(&mut self, settings: &Settings) {
        let next = self.queue.next_with_mode(
            settings.shuffle,
            settings.repeat_mode,
        );
        if let Some(path) = next.cloned() {
            self.engine.send(AudioCommand::Play(path));
        }
    }

    /// Go to the previous track, or restart current if past 3 seconds.
    pub fn play_prev(&mut self) {
        if self.playing.elapsed.as_secs() > 3 {
            self.engine.send(AudioCommand::Seek(Duration::ZERO));
            return;
        }
        if let Some(path) = self.queue.prev().cloned() {
            self.engine.send(AudioCommand::Play(path));
        }
    }

    /// Adjust volume up.
    pub fn volume_up(&mut self) {
        self.playing.volume = (self.playing.volume + 0.05).min(1.0);
        self.engine.send(AudioCommand::SetVolume(self.playing.volume));
    }

    /// Adjust volume down.
    pub fn volume_down(&mut self) {
        self.playing.volume = (self.playing.volume - 0.05).max(0.0);
        self.engine.send(AudioCommand::SetVolume(self.playing.volume));
    }

    /// Seek forward by the configured step.
    pub fn seek_forward(&mut self, seek_step_secs: u64) {
        let new_pos = self.playing.elapsed + Duration::from_secs(seek_step_secs);
        if new_pos < self.playing.duration {
            self.engine.send(AudioCommand::Seek(new_pos));
        }
    }

    /// Seek backward by the configured step.
    pub fn seek_backward(&mut self, seek_step_secs: u64) {
        let new_pos = self.playing.elapsed
            .saturating_sub(Duration::from_secs(seek_step_secs));
        self.engine.send(AudioCommand::Seek(new_pos));
    }

    /// Seek to a specific fraction of the track (0.0 to 1.0).
    pub fn seek_to_fraction(&mut self, fraction: f64) {
        let seek_pos = Duration::from_secs_f64(
            fraction * self.playing.duration.as_secs_f64(),
        );
        self.engine.send(AudioCommand::Seek(seek_pos));
    }

    /// Stop playback.
    pub fn stop(&self) {
        self.engine.send(AudioCommand::Stop);
    }

    /// Poll the engine for audio events and update state.
    /// Returns true if a track finished (caller may want to auto-advance).
    pub fn process_events(&mut self, settings: &Settings) {
        while let Ok(event) = self.engine.event_rx().try_recv() {
            match event {
                AudioEvent::Playing {
                    path,
                    duration,
                    sample_rate,
                    channels,
                } => {
                    self.playing.playback = PlaybackState::Playing;
                    self.playing.duration = duration;
                    self.playing.sample_rate = sample_rate;
                    self.playing.channels = channels;
                    self.playing.elapsed = Duration::ZERO;
                    self.playing.file_path = Some(path.clone());
                    self.load_metadata_async(&path);
                }
                AudioEvent::Position(pos) => {
                    self.playing.elapsed = pos;
                }
                AudioEvent::Paused => {
                    self.playing.playback = PlaybackState::Paused;
                }
                AudioEvent::Resumed => {
                    self.playing.playback = PlaybackState::Playing;
                }
                AudioEvent::Stopped => {
                    self.playing.playback = PlaybackState::Stopped;
                    self.playing.spectrum.clear();
                }
                AudioEvent::Finished => {
                    self.playing.playback = PlaybackState::Stopped;
                    self.playing.spectrum.clear();
                    self.play_next(settings);
                }
                AudioEvent::Error(msg) => {
                    tracing::error!("Audio error: {msg}");
                }
                AudioEvent::Level(level) => {
                    self.playing.level = self.playing.level * 0.7 + level * 0.3;
                }
                AudioEvent::Spectrum(ref bars) => {
                    self.playing.update_spectrum(bars);
                }
            }
        }
    }

    /// Poll the background metadata + album art loader.
    pub fn process_meta_events(&mut self) {
        let rx = match self.meta_rx.take() {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(meta) => {
                // Only apply if the track hasn't changed since we requested.
                if self.playing.file_path.as_ref() == Some(&meta.path) {
                    self.playing.title = meta.title;
                    self.playing.artist = meta.artist;
                    self.playing.album = meta.album;

                    if let Some(cells) = meta.art_cells {
                        self.album_art.track_path = Some(meta.path);
                        self.album_art.cells = cells;
                        self.album_art.has_art = true;
                    } else {
                        self.album_art.track_path = Some(meta.path);
                        self.album_art.cells.clear();
                        self.album_art.has_art = false;
                    }
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                self.meta_rx = Some(rx);
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {}
        }
    }

    /// Spawn a background thread to load metadata + album art.
    fn load_metadata_async(&mut self, path: &std::path::Path) {
        use lofty::file::TaggedFileExt;
        use lofty::tag::Accessor;

        let path = path.to_path_buf();
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.meta_rx = Some(rx);

        thread::Builder::new()
            .name("meta-loader".into())
            .spawn(move || {
                let mut meta = TrackMeta {
                    path: path.clone(),
                    title: String::new(),
                    artist: String::new(),
                    album: String::new(),
                    art_cells: None,
                };

                match lofty::probe::Probe::open(&path)
                    .and_then(|p| p.guess_file_type()?.read())
                {
                    Ok(tagged_file) => {
                        if let Some(tag) =
                            tagged_file.primary_tag().or(tagged_file.first_tag())
                        {
                            meta.title = tag.title().map(|s| s.to_string()).unwrap_or_default();
                            meta.artist = tag.artist().map(|s| s.to_string()).unwrap_or_default();
                            meta.album = tag.album().map(|s| s.to_string()).unwrap_or_default();
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Could not read metadata: {e}");
                    }
                }

                if let Some(data) = albumart::extract_cover(&path) {
                    if let Some(img) = albumart::load_image(&data) {
                        meta.art_cells = Some(albumart::render_art(&img, 30, 15));
                    }
                }

                let _ = tx.send(meta);
            })
            .expect("Failed to spawn metadata loader thread");
    }
}
