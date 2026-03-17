use std::path::PathBuf;
use std::thread;
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
use crate::library::scanner::{self, ScanEvent};
use crate::playlist::queue::Queue;
use crate::pomodoro::timer::{PomodoroAction, PomodoroTimer};
use crate::ui::albumart::{AlbumArt, GraphicsProtocol};
use crate::ui::input::{self, Action};
use crate::ui::layout;
use crate::ui::tabs::library::{LibraryState, LibraryTab, LibraryView};
use crate::ui::tabs::playing::{PlayingState, PlayingTab};
use crate::ui::tabs::pomodoro::PomodoroTab;
use crate::ui::tabs::search::{SearchState, SearchTab};
use crate::ui::tabs::settings::{SettingsState, SettingsTab};
use crate::util::channels::{AudioCommand, AudioEvent};

const TAB_COUNT: usize = 6;

const TAB_NAMES: [&str; TAB_COUNT] = [
    "Playing", "Library", "Pomodoro", "Search", "Settings", "AI",
];

/// Which tabs are currently functional.
const TAB_ENABLED: [bool; TAB_COUNT] = [
    true, true, true, true, true, false,
];

pub struct App {
    settings: Settings,
    engine: AudioEngine,
    playing: PlayingState,
    library_state: LibraryState,
    search_state: SearchState,
    settings_state: SettingsState,
    queue: Queue,
    pomodoro: PomodoroTimer,
    album_art: AlbumArt,
    graphics_protocol: GraphicsProtocol,
    active_tab: usize,
    should_quit: bool,
    scan_rx: Option<crossbeam_channel::Receiver<ScanEvent>>,
    /// Store the progress bar's screen rect for mouse-click-to-seek.
    progress_bar_rect: Option<ratatui::layout::Rect>,
    /// Store the tab bar's screen rect for mouse tab switching.
    tab_bar_rect: Option<ratatui::layout::Rect>,
}

impl App {
    pub fn new(settings: Settings) -> Self {
        let volume = settings.default_volume;
        let engine = AudioEngine::new();
        let mut playing = PlayingState::default();
        playing.volume = volume;

        let protocol = crate::ui::albumart::detect_protocol();

        Self {
            settings,
            engine,
            playing,
            library_state: LibraryState::default(),
            search_state: SearchState::default(),
            settings_state: SettingsState::default(),
            queue: Queue::new(),
            pomodoro: PomodoroTimer::default(),
            album_art: AlbumArt::default(),
            graphics_protocol: protocol,
            active_tab: 0,
            should_quit: false,
            scan_rx: None,
            progress_bar_rect: None,
            tab_bar_rect: None,
        }
    }

    pub fn play_file(&mut self, path: PathBuf) {
        self.queue.clear();
        self.queue.push(path.clone());
        self.engine.send(AudioCommand::Play(path));
    }

    /// Start scanning the music directory in a background thread.
    fn start_library_scan(&mut self) {
        let music_dir = self
            .settings
            .music_directory
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));

        if !music_dir.is_dir() {
            tracing::warn!("Music directory does not exist: {}", music_dir.display());
            return;
        }

        self.library_state.library.scanning = true;

        let (tx, rx) = crossbeam_channel::unbounded();
        self.scan_rx = Some(rx);

        thread::Builder::new()
            .name("library-scan".into())
            .spawn(move || {
                scanner::scan_directory(&music_dir, tx);
            })
            .expect("Failed to spawn library scan thread");
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), EnterAlternateScreen)?;
        input::enable_mouse()?;

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Start library scan.
        self.start_library_scan();

        let result = self.event_loop(&mut terminal);

        input::disable_mouse()?;
        disable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), LeaveAlternateScreen)?;

        result
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> anyhow::Result<()> {
        loop {
            // Process audio events.
            self.process_audio_events();
            // Process scan events.
            self.process_scan_events();
            // Tick the pomodoro timer.
            match self.pomodoro.tick() {
                PomodoroAction::PauseMusic => {
                    self.engine.send(AudioCommand::Pause);
                }
                PomodoroAction::ResumeMusic => {
                    self.engine.send(AudioCommand::Resume);
                }
                PomodoroAction::None => {}
            }

            // Update album art if track changed.
            if let Some(ref path) = self.playing.file_path.clone() {
                self.album_art.update(path, self.graphics_protocol, 30, 15);
            }

            // Render.
            terminal.draw(|frame| {
                let area = frame.area();
                let (header, content, footer) = layout::main_layout(area);

                self.tab_bar_rect = Some(header);

                // Tab bar.
                let mut tab_spans = Vec::new();
                for (i, name) in TAB_NAMES.iter().enumerate() {
                    let label = format!(" {}:{name} ", i + 1);
                    let style = if i == self.active_tab {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else if TAB_ENABLED[i] {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    tab_spans.push(Span::styled(label, style));
                }

                // Shuffle/Repeat indicators.
                let mode_indicator = self.mode_indicator();
                if !mode_indicator.is_empty() {
                    tab_spans.push(Span::raw("  "));
                    tab_spans.push(Span::styled(
                        mode_indicator,
                        Style::default().fg(Color::Yellow),
                    ));
                }

                frame.render_widget(
                    Paragraph::new(Line::from(tab_spans)),
                    header,
                );

                // Main content.
                match self.active_tab {
                    0 => {
                        frame.render_widget(
                            PlayingTab {
                                state: &self.playing,
                                queue: &self.queue,
                                album_art: &self.album_art,
                            },
                            content,
                        );
                        let progress_y = content.y + 8;
                        if progress_y < content.y + content.height {
                            self.progress_bar_rect = Some(ratatui::layout::Rect {
                                x: content.x + 1,
                                y: progress_y,
                                width: content.width.saturating_sub(2),
                                height: 1,
                            });
                        }
                    }
                    1 => {
                        frame.render_widget(
                            LibraryTab {
                                state: &self.library_state,
                            },
                            content,
                        );
                        self.progress_bar_rect = None;
                    }
                    2 => {
                        frame.render_widget(
                            PomodoroTab {
                                timer: &self.pomodoro,
                            },
                            content,
                        );
                        self.progress_bar_rect = None;
                    }
                    3 => {
                        frame.render_widget(
                            SearchTab {
                                state: &self.search_state,
                            },
                            content,
                        );
                        self.progress_bar_rect = None;
                    }
                    4 => {
                        frame.render_widget(
                            SettingsTab {
                                state: &self.settings_state,
                                settings: &self.settings,
                            },
                            content,
                        );
                        self.progress_bar_rect = None;
                    }
                    _ => {
                        let msg = format!("{} — coming in a future release", TAB_NAMES[self.active_tab]);
                        frame.render_widget(
                            Paragraph::new(Line::from(Span::styled(
                                msg,
                                Style::default().fg(Color::DarkGray),
                            )))
                            .centered(),
                            content,
                        );
                        self.progress_bar_rect = None;
                    }
                }

                // Footer hints (context-sensitive).
                let hints = self.build_footer_hints();
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
                    if self.active_tab == 2 && matches!(cmd, AudioCommand::TogglePlayPause) {
                        self.pomodoro.toggle_pause();
                    } else {
                        self.engine.send(cmd);
                    }
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
                Action::SwitchTab(tab) => {
                    self.switch_tab(tab);
                }
                Action::NextTrack => {
                    self.play_next();
                }
                Action::PrevTrack => {
                    self.play_prev();
                }
                Action::ScrollDown => {
                    self.handle_scroll_down();
                }
                Action::ScrollUp => {
                    self.handle_scroll_up();
                }
                Action::Enter => {
                    self.handle_enter();
                }
                Action::Back => {
                    self.handle_back();
                }
                Action::Char(ch) => {
                    self.handle_char(ch);
                }
                Action::Backspace => {
                    self.handle_backspace();
                }
                Action::MouseClick { col, row } => {
                    self.handle_mouse_click(col, row);
                }
                Action::None => {}
            }
        }
    }

    fn switch_tab(&mut self, tab: usize) {
        if tab == usize::MAX {
            self.active_tab = (self.active_tab + 1) % TAB_COUNT;
        } else if tab == usize::MAX - 1 {
            self.active_tab = (self.active_tab + TAB_COUNT - 1) % TAB_COUNT;
        } else if tab < TAB_COUNT {
            self.active_tab = tab;
        }
    }

    fn handle_scroll_down(&mut self) {
        match self.active_tab {
            1 => {
                self.library_state.move_down();
                self.update_library_scroll();
            }
            3 => {
                self.search_state.move_down();
            }
            4 => {
                self.settings_state
                    .move_down(SettingsState::item_count());
            }
            _ => {}
        }
    }

    fn handle_scroll_up(&mut self) {
        match self.active_tab {
            1 => {
                self.library_state.move_up();
                self.update_library_scroll();
            }
            3 => {
                self.search_state.move_up();
            }
            4 => {
                self.settings_state.move_up();
            }
            _ => {}
        }
    }

    fn handle_enter(&mut self) {
        match self.active_tab {
            2 => {
                if !self.pomodoro.active {
                    self.pomodoro.start();
                }
            }
            1 => {
                if let LibraryView::Tracks { .. } = &self.library_state.view {
                    if let Some(path) = self.library_state.selected_track_path() {
                        self.queue.push(path.clone());
                        if !self.playing.is_playing && self.playing.file_path.is_none() {
                            self.engine.send(AudioCommand::Play(path));
                        } else {
                            let idx = self.queue.tracks().len() - 1;
                            self.queue.set_current(idx);
                            self.engine.send(AudioCommand::Play(path));
                        }
                    }
                } else {
                    self.library_state.enter();
                }
            }
            3 => {
                // Search: enqueue and play selected track.
                if let Some(path) = self.search_state.selected_track_path() {
                    self.queue.push(path.clone());
                    let idx = self.queue.tracks().len() - 1;
                    self.queue.set_current(idx);
                    self.engine.send(AudioCommand::Play(path));
                }
            }
            4 => {
                // Settings: toggle the selected setting.
                self.settings_state.toggle(&mut self.settings);
            }
            _ => {}
        }
    }

    fn handle_back(&mut self) {
        match self.active_tab {
            1 => self.library_state.back(),
            2 => self.pomodoro.stop(),
            _ => {}
        }
    }

    fn handle_char(&mut self, ch: char) {
        if self.active_tab == 2 {
            match ch {
                'v' => self.pomodoro.cycle_style(),
                's' => {
                    match self.pomodoro.skip() {
                        PomodoroAction::PauseMusic => {
                            self.engine.send(AudioCommand::Pause);
                        }
                        PomodoroAction::ResumeMusic => {
                            self.engine.send(AudioCommand::Resume);
                        }
                        PomodoroAction::None => {}
                    }
                }
                _ => {}
            }
            return;
        }
        if self.active_tab == 3 {
            // Search tab: type into search box.
            self.search_state.push_char(ch);
            self.search_state
                .update_results(&self.library_state.library);
        } else if self.active_tab == 4 && ch == 's' {
            // Save settings.
            match self.settings.save() {
                Ok(()) => {
                    self.settings_state.status_message =
                        Some("Settings saved!".into());
                }
                Err(e) => {
                    self.settings_state.status_message =
                        Some(format!("Save failed: {e}"));
                }
            }
        }
    }

    fn handle_backspace(&mut self) {
        if self.active_tab == 3 {
            self.search_state.pop_char();
            self.search_state
                .update_results(&self.library_state.library);
        }
    }

    fn handle_mouse_click(&mut self, col: u16, row: u16) {
        // Check if click is on tab bar.
        if let Some(tab_rect) = self.tab_bar_rect {
            if row == tab_rect.y && col >= tab_rect.x && col < tab_rect.x + tab_rect.width {
                let mut x = tab_rect.x;
                for (i, name) in TAB_NAMES.iter().enumerate() {
                    let label_len = format!(" {}:{name} ", i + 1).len() as u16;
                    if col >= x && col < x + label_len {
                        self.switch_tab(i);
                        return;
                    }
                    x += label_len;
                }
            }
        }

        // Check if click is on progress bar (seek).
        if let Some(bar_rect) = self.progress_bar_rect {
            if row == bar_rect.y && col >= bar_rect.x && col < bar_rect.x + bar_rect.width {
                if self.playing.duration.as_secs_f64() > 0.0 {
                    let elapsed_label_len = format_duration(self.playing.elapsed).len() as u16;
                    let total_label_len = format_duration(self.playing.duration).len() as u16;
                    let bar_start = bar_rect.x + elapsed_label_len + 1;
                    let bar_end = bar_rect.x + bar_rect.width - total_label_len - 1;

                    if col >= bar_start && col < bar_end {
                        let fraction = (col - bar_start) as f64
                            / (bar_end - bar_start) as f64;
                        let seek_pos = Duration::from_secs_f64(
                            fraction * self.playing.duration.as_secs_f64(),
                        );
                        self.engine.send(AudioCommand::Seek(seek_pos));
                    }
                }
            }
        }
    }

    fn play_next(&mut self) {
        let next = self.queue.next_with_mode(
            self.settings.shuffle,
            self.settings.repeat_mode,
        );
        if let Some(path) = next.cloned() {
            self.engine.send(AudioCommand::Play(path));
        }
    }

    fn play_prev(&mut self) {
        if self.playing.elapsed.as_secs() > 3 {
            self.engine.send(AudioCommand::Seek(Duration::ZERO));
            return;
        }
        if let Some(path) = self.queue.prev().cloned() {
            self.engine.send(AudioCommand::Play(path));
        }
    }

    fn update_library_scroll(&mut self) {
        let selected = self.library_state.selected;
        let offset = self.library_state.scroll_offset;
        let visible = 20;

        if selected < offset {
            self.library_state.scroll_offset = selected;
        } else if selected >= offset + visible {
            self.library_state.scroll_offset = selected - visible + 1;
        }
    }

    fn mode_indicator(&self) -> String {
        use crate::config::settings::{RepeatMode, ShuffleMode};
        let mut parts = Vec::new();
        match self.settings.shuffle {
            ShuffleMode::On => parts.push("Shuffle"),
            ShuffleMode::Off => {}
        }
        match self.settings.repeat_mode {
            RepeatMode::All => parts.push("Repeat:All"),
            RepeatMode::One => parts.push("Repeat:1"),
            RepeatMode::Off => {}
        }
        parts.join(" | ")
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
                AudioEvent::Finished => {
                    self.playing.is_playing = false;
                    self.play_next();
                }
                AudioEvent::Error(msg) => {
                    tracing::error!("Audio error: {msg}");
                }
                AudioEvent::Level(level) => {
                    self.playing.level =
                        self.playing.level * 0.7 + level * 0.3;
                }
            }
        }
    }

    fn process_scan_events(&mut self) {
        let rx = match &self.scan_rx {
            Some(rx) => rx.clone(),
            None => return,
        };

        while let Ok(event) = rx.try_recv() {
            match event {
                ScanEvent::Started => {
                    self.library_state.library.scanning = true;
                }
                ScanEvent::Progress { found: _ } => {}
                ScanEvent::Complete(library) => {
                    self.library_state.library = library;
                    self.library_state.library.scanning = false;
                    self.scan_rx = None;
                    return;
                }
                ScanEvent::Error(msg) => {
                    tracing::warn!("Scan error: {msg}");
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

    fn build_footer_hints(&self) -> Line<'static> {
        let hint = |key: &str, desc: &str| -> Vec<Span<'static>> {
            vec![
                Span::styled(
                    format!(" {key}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(":{desc} "), Style::default().fg(Color::DarkGray)),
            ]
        };

        let mut spans = Vec::new();

        spans.extend(hint("Space", "play/pause"));
        spans.extend(hint("n/p", "next/prev"));

        match self.active_tab {
            0 => {
                spans.extend(hint("h/l", "seek"));
                spans.extend(hint("+/-", "volume"));
            }
            1 => {
                spans.extend(hint("j/k", "navigate"));
                spans.extend(hint("Enter", "select"));
                spans.extend(hint("Bksp", "back"));
            }
            2 => {
                if self.pomodoro.active {
                    spans.extend(hint("Space", "pause/resume"));
                    spans.extend(hint("s", "skip phase"));
                    spans.extend(hint("v", "cycle style"));
                    spans.extend(hint("Esc", "stop"));
                } else {
                    spans.extend(hint("Enter", "start"));
                    spans.extend(hint("v", "cycle style"));
                }
            }
            3 => {
                spans.extend(hint("type", "search"));
                spans.extend(hint("j/k", "navigate"));
                spans.extend(hint("Enter", "play"));
            }
            4 => {
                spans.extend(hint("j/k", "navigate"));
                spans.extend(hint("Enter", "toggle"));
                spans.extend(hint("s", "save"));
            }
            _ => {}
        }

        spans.extend(hint("q", "quit"));

        Line::from(spans)
    }
}

fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}
