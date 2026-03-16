use std::path::PathBuf;
use std::time::Duration;

use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

use lofty::file::TaggedFileExt;
use lofty::tag::Accessor;

use crate::audio::engine::AudioEngine;
use crate::config::settings::Settings;
use crate::ui::input::{self, Action};
use crate::ui::layout;
use crate::ui::tabs::playing::{PlayingState, PlayingTab};
use crate::util::channels::{AudioCommand, AudioEvent};

pub struct App {
    settings: Settings,
    engine: AudioEngine,
    playing: PlayingState,
    should_quit: bool,
}

impl App {
    pub fn new(settings: Settings) -> Self {
        let volume = settings.default_volume;
        let engine = AudioEngine::new();
        let mut playing = PlayingState::default();
        playing.volume = volume;

        Self {
            settings,
            engine,
            playing,
            should_quit: false,
        }
    }

    pub fn play_file(&self, path: PathBuf) {
        // Extract metadata before sending play command.
        self.engine.send(AudioCommand::Play(path));
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let result = self.event_loop(&mut terminal);

        disable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), LeaveAlternateScreen)?;

        result
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> anyhow::Result<()> {
        loop {
            // Process audio events (non-blocking).
            self.process_audio_events();

            // Render.
            terminal.draw(|frame| {
                let area = frame.area();
                let (header, content, footer) = layout::main_layout(area);

                // Tab bar (Phase 1: only "Playing" tab).
                let tab_bar = Line::from(vec![
                    Span::styled(
                        " 1:Playing ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                    ),
                    Span::styled(
                        " 2:Library ",
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        " 3:Playlists ",
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        " 4:Search ",
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        " 5:Settings ",
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        " 6:AI ",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                frame.render_widget(Paragraph::new(tab_bar), header);

                // Main content.
                frame.render_widget(
                    PlayingTab {
                        state: &self.playing,
                    },
                    content,
                );

                // Footer hints.
                let hints = Line::from(vec![
                    Span::styled(
                        " Space",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(":play/pause ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "h/l",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(":seek ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "+/-",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(":volume ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "q",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(":quit", Style::default().fg(Color::DarkGray)),
                ]);
                frame.render_widget(Paragraph::new(hints), footer);
            })?;

            if self.should_quit {
                return Ok(());
            }

            // Handle input (~60fps target → 16ms timeout).
            match input::poll_input(Duration::from_millis(16))? {
                Action::Quit => {
                    self.engine.send(AudioCommand::Stop);
                    self.should_quit = true;
                }
                Action::AudioCmd(cmd) => {
                    self.engine.send(cmd);
                }
                Action::VolumeUp => {
                    self.playing.volume =
                        (self.playing.volume + 0.05).min(1.0);
                    self.engine
                        .send(AudioCommand::SetVolume(self.playing.volume));
                }
                Action::VolumeDown => {
                    self.playing.volume =
                        (self.playing.volume - 0.05).max(0.0);
                    self.engine
                        .send(AudioCommand::SetVolume(self.playing.volume));
                }
                Action::SeekForward => {
                    let new_pos = self.playing.elapsed
                        + Duration::from_secs(self.settings.seek_step_secs);
                    if new_pos < self.playing.duration {
                        self.engine.send(AudioCommand::Seek(new_pos));
                    }
                }
                Action::SeekBackward => {
                    let secs = self.settings.seek_step_secs;
                    let new_pos = self
                        .playing
                        .elapsed
                        .saturating_sub(Duration::from_secs(secs));
                    self.engine.send(AudioCommand::Seek(new_pos));
                }
                Action::None => {}
            }
        }
    }

    fn process_audio_events(&mut self) {
        while let Ok(event) = self.engine.event_rx().try_recv() {
            match event {
                AudioEvent::Playing {
                    path,
                    duration,
                    sample_rate,
                    channels,
                } => {
                    self.playing.is_playing = true;
                    self.playing.duration = duration;
                    self.playing.sample_rate = sample_rate;
                    self.playing.channels = channels;
                    self.playing.elapsed = Duration::ZERO;
                    self.playing.file_path = Some(path.clone());

                    // Try to read metadata.
                    self.load_metadata(&path);
                }
                AudioEvent::Position(pos) => {
                    self.playing.elapsed = pos;
                }
                AudioEvent::Paused => {
                    self.playing.is_playing = false;
                }
                AudioEvent::Resumed => {
                    self.playing.is_playing = true;
                }
                AudioEvent::Stopped => {
                    self.playing.is_playing = false;
                }
                AudioEvent::Error(msg) => {
                    tracing::error!("Audio error: {msg}");
                }
                AudioEvent::Level(level) => {
                    // Smooth the level a bit for display.
                    self.playing.level =
                        self.playing.level * 0.7 + level * 0.3;
                }
            }
        }
    }

    fn load_metadata(&mut self, path: &std::path::Path) {
        match lofty::probe::Probe::open(path)
            .and_then(|p| p.guess_file_type()?.read())
        {
            Ok(tagged_file) => {
                if let Some(tag) =
                    tagged_file.primary_tag().or(tagged_file.first_tag())
                {
                    self.playing.title = tag
                        .title()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    self.playing.artist = tag
                        .artist()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    self.playing.album = tag
                        .album()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                }
            }
            Err(e) => {
                tracing::warn!("Could not read metadata: {e}");
                self.playing.title.clear();
                self.playing.artist.clear();
                self.playing.album.clear();
            }
        }
    }
}
