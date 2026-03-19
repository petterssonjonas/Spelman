use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::Terminal;

use crate::config::settings::{BindableAction, Settings, key_to_string};
use crate::coordinator::player::PlayerCoordinator;
use crate::library::scanner::{self, ScanEvent};
use crate::playlist::playlist::{Playlist, PlaylistManager};
use crate::pomodoro::timer::{PomodoroAction, PomodoroTimer};
use crate::ui::input::{self, Action};
use crate::ui::layout;
use crate::ui::tabs::home::{HomePane, HomeState, HomeTab};
use crate::ui::tabs::library::{LibraryState, LibrarySortMode, LibraryTab, LibraryView};
use crate::ui::tabs::playing::{PlaybackState, PlayingTab, compute_art_rows, compute_art_rect};
use crate::ui::tabs::playlists::PlaylistsState;
use crate::ui::tabs::pomodoro::PomodoroTab;
use crate::ui::tabs::search::{SearchState, SearchTab};
use crate::ui::tabs::settings::{SettingsState, SettingsTab};
use crate::ui::widgets::eq::{EqState, render_eq};
use crate::util::channels::AudioCommand;
use crossterm::event::KeyCode;
use std::collections::HashMap;

const TAB_COUNT: usize = 4;

const TAB_NAMES: [&str; TAB_COUNT] = [
    "Home", "Playing", "Library", "Settings",
];

pub struct App {
    settings: Settings,
    player: PlayerCoordinator,
    home_state: HomeState,
    library_state: LibraryState,
    playlists_state: PlaylistsState,
    search_state: SearchState,
    settings_state: SettingsState,
    pomodoro: PomodoroTimer,
    active_tab: usize,
    should_quit: bool,
    scan_rx: Option<crossbeam_channel::Receiver<ScanEvent>>,
    scan_handle: Option<thread::JoinHandle<()>>,
    /// Store the progress bar's screen rect for mouse-click-to-seek.
    progress_bar_rect: Option<Rect>,
    /// Store the controls line rect (play/pause click target).
    controls_rect: Option<Rect>,
    /// Store the tab bar's screen rect for mouse tab switching.
    tab_bar_rect: Option<Rect>,
    /// Store the main content area for list hover effects.
    content_rect: Option<Rect>,
    /// Recently played track paths (most recent first).
    recent_tracks: Vec<PathBuf>,
    /// Last recorded track path (to avoid re-recording on every frame).
    last_recorded_path: Option<PathBuf>,
    /// Whether we're in "name the playlist" mode.
    naming_playlist: Option<PlaylistSource>,
    /// Buffer for typing the playlist name.
    playlist_name_buf: String,
    /// Current mouse cursor position for hover effects.
    mouse_pos: (u16, u16),
    /// EQ overlay state.
    eq_state: EqState,
    /// Store the EQ overlay rect for mouse hit-testing.
    eq_rect: Option<Rect>,
    /// Search popup visible.
    search_visible: bool,
    /// Pomodoro popup visible.
    pomodoro_visible: bool,
    /// Keybindings reference popup visible.
    keybindings_visible: bool,
    /// Playlist picker popup visible.
    playlist_picker_visible: bool,
    /// Selected index in the playlist picker.
    playlist_picker_selected: usize,
    /// Tracks waiting to be added to a playlist (set before opening picker).
    playlist_picker_tracks: Vec<PathBuf>,
    /// Recently played popup visible.
    recent_popup_visible: bool,
    /// Selected index in the recently played popup.
    recent_popup_selected: usize,
    /// Reverse lookup: KeyCode → BindableAction, built from settings.
    key_lookup: HashMap<KeyCode, BindableAction>,
    /// Glimmer effect: when the last wave completed (or app start).
    glimmer_last: Instant,
    /// Glimmer effect: when the current wave started, or None if idle.
    glimmer_wave: Option<Instant>,
    /// True when focus is on the tab bar (left/right switch tabs, content not selected).
    focus_tabbar: bool,
    /// Keybindings hint rect at bottom of Home tab (for mouse click).
    keybindings_hint_rect: Option<Rect>,
    /// Queue indicator popup visible.
    queue_popup_visible: bool,
    /// Selected index in queue popup.
    queue_popup_selected: usize,
    /// Queue indicator rect in tab bar (for mouse click).
    queue_indicator_rect: Option<Rect>,
    /// Name of the currently active playlist (set when a playlist is started).
    active_playlist: Option<String>,
    /// Playlist indicator rect in tab bar (for mouse click).
    playlist_indicator_rect: Option<Rect>,
    /// Active playlist popup visible.
    active_playlist_popup_visible: bool,
    /// Selected index in the active playlist popup.
    active_playlist_popup_selected: usize,
    /// Close button [X] rect in tab bar.
    close_button_rect: Option<Rect>,
}

/// Where a new playlist's tracks come from.
enum PlaylistSource {
    /// From the current play queue.
    Queue,
    /// From library-selected tracks.
    LibrarySelection(Vec<PathBuf>),
}

impl App {
    pub fn new(settings: Settings) -> Self {
        let player = PlayerCoordinator::new(settings.default_volume);
        let mut playlists_state = PlaylistsState::default();
        playlists_state.reload();
        let key_lookup = settings.keybindings.build_lookup();
        let recent_tracks = Self::load_recent_tracks();

        Self {
            settings,
            player,
            home_state: HomeState::default(),
            library_state: LibraryState::default(),
            playlists_state,
            search_state: SearchState::default(),
            settings_state: SettingsState::default(),
            pomodoro: PomodoroTimer::default(),
            active_tab: 0,
            should_quit: false,
            scan_rx: None,
            scan_handle: None,
            progress_bar_rect: None,
            controls_rect: None,
            tab_bar_rect: None,
            content_rect: None,
            recent_tracks,
            last_recorded_path: None,
            naming_playlist: None,
            playlist_name_buf: String::new(),
            mouse_pos: (0, 0),
            key_lookup,
            eq_state: EqState::default(),
            eq_rect: None,
            search_visible: false,
            pomodoro_visible: false,
            keybindings_visible: false,
            playlist_picker_visible: false,
            playlist_picker_selected: 0,
            playlist_picker_tracks: Vec::new(),
            recent_popup_visible: false,
            recent_popup_selected: 0,
            glimmer_last: Instant::now(),
            glimmer_wave: None,
            focus_tabbar: true,
            keybindings_hint_rect: None,
            queue_popup_visible: false,
            queue_popup_selected: 0,
            queue_indicator_rect: None,
            active_playlist: None,
            playlist_indicator_rect: None,
            active_playlist_popup_visible: false,
            active_playlist_popup_selected: 0,
            close_button_rect: None,
        }
    }

    pub fn play_file(&mut self, path: PathBuf) {
        self.player.play_file(path);
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

        let handle = thread::Builder::new()
            .name("library-scan".into())
            .spawn(move || {
                scanner::scan_directory(&music_dir, tx);
            })
            .expect("Failed to spawn library scan thread");
        self.scan_handle = Some(handle);
    }

    /// Record a track as recently played (max 50).
    fn record_recent(&mut self, path: &Path) {
        self.recent_tracks.retain(|p| p != path);
        self.recent_tracks.insert(0, path.to_path_buf());
        self.recent_tracks.truncate(25);
    }

    /// Load recently played tracks from disk.
    fn load_recent_tracks() -> Vec<PathBuf> {
        Settings::recent_tracks_path()
            .and_then(|path| {
                let content = std::fs::read_to_string(&path).ok()?;
                serde_json::from_str(&content).ok()
            })
            .unwrap_or_default()
    }

    /// Save recently played tracks to disk.
    fn save_recent_tracks(&self) {
        if let Some(path) = Settings::recent_tracks_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(content) = serde_json::to_string(&self.recent_tracks) {
                let _ = std::fs::write(path, content);
            }
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        // Install a panic hook that restores the terminal before printing the panic.
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = input::disable_mouse();
            let _ = disable_raw_mode();
            let _ = crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);
            default_hook(info);
        }));

        enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), EnterAlternateScreen)?;
        input::enable_mouse()?;

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        self.start_library_scan();

        let result = self.event_loop(&mut terminal);

        // Save recent tracks on exit.
        self.save_recent_tracks();

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
            // Process events from coordinators.
            self.player.process_events(&self.settings);
            self.player.process_meta_events();
            self.process_scan_events();

            // Track recently played (only when the track changes).
            if self.player.playing.file_path != self.last_recorded_path {
                if let Some(ref path) = self.player.playing.file_path {
                    let path = path.clone();
                    self.record_recent(&path);
                    self.last_recorded_path = Some(path);
                } else {
                    self.last_recorded_path = None;
                }
            }

            // Tick the pomodoro timer.
            match self.pomodoro.tick() {
                PomodoroAction::PauseMusic => {
                    self.player.send(AudioCommand::Pause);
                }
                PomodoroAction::ResumeMusic => {
                    self.player.send(AudioCommand::Resume);
                }
                PomodoroAction::None => {}
            }

            // Glimmer timer: start a new wave every ~15 seconds.
            const GLIMMER_INTERVAL: f64 = 15.0;
            const GLIMMER_DURATION: f64 = 1.2; // seconds for wave to cross
            if self.glimmer_wave.is_none()
                && self.glimmer_last.elapsed().as_secs_f64() >= GLIMMER_INTERVAL
            {
                self.glimmer_wave = Some(Instant::now());
            }
            // Check if wave is finished.
            if let Some(start) = self.glimmer_wave {
                if start.elapsed().as_secs_f64() > GLIMMER_DURATION {
                    self.glimmer_wave = None;
                    self.glimmer_last = Instant::now();
                }
            }
            let glimmer_progress = self.glimmer_wave.map(|s| {
                (s.elapsed().as_secs_f64() / GLIMMER_DURATION).clamp(0.0, 1.0)
            });

            // Render.
            terminal.draw(|frame| {
                let area = frame.area();

                // Minimum usable size check.
                if area.width < 40 || area.height < 10 {
                    let msg = format!(
                        "Terminal too small ({}x{})\nMinimum: 40x10",
                        area.width, area.height
                    );
                    frame.render_widget(
                        Paragraph::new(msg)
                            .centered()
                            .style(Style::default().fg(Color::Red)),
                        area,
                    );
                    return;
                }

                let (header, content) = layout::main_layout(area);

                self.tab_bar_rect = Some(header);
                self.content_rect = Some(content);

                // Theme colors.
                let tc = &self.settings.theme_colors;
                let accent = tc.accent();
                let text_color = tc.text();
                let _dim = tc.text_dim();
                let highlight = tc.highlight();

                // Tab bar — no number prefixes, just names.
                let mut tab_spans = Vec::new();
                for (i, name) in TAB_NAMES.iter().enumerate() {
                    let label = format!(" {name} ");
                    let style = if i == self.active_tab {
                        Style::default()
                            .fg(accent)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(text_color)
                    };
                    tab_spans.push(Span::styled(label, style));
                }

                // Shuffle/Repeat indicators.
                let mode_indicator = self.mode_indicator();
                if !mode_indicator.is_empty() {
                    tab_spans.push(Span::raw("  "));
                    tab_spans.push(Span::styled(
                        mode_indicator,
                        Style::default().fg(highlight),
                    ));
                }

                // Render tab bar.
                frame.render_widget(
                    Paragraph::new(Line::from(tab_spans)),
                    header,
                );

                // Right-aligned indicators: [Playlist] [Queue] [X]
                // Build from right to left.
                let buf = frame.buffer_mut();
                let close_label = " [X] ";
                let close_len = close_label.len() as u16;
                let close_x = header.x + header.width.saturating_sub(close_len);
                self.close_button_rect = Some(Rect {
                    x: close_x,
                    y: header.y,
                    width: close_len,
                    height: 1,
                });
                buf.set_string(close_x, header.y, close_label, Style::default().fg(Color::Red));

                let mut right_x = close_x; // next indicator goes left of this

                // Queue indicator.
                let queue_len = self.player.queue.len();
                if queue_len > 0 {
                    let queue_label = format!(" Queue ({queue_len}) ");
                    let queue_label_len = queue_label.len() as u16;
                    let qx = right_x.saturating_sub(queue_label_len);
                    self.queue_indicator_rect = Some(Rect {
                        x: qx, y: header.y, width: queue_label_len, height: 1,
                    });
                    let qi_style = if self.queue_popup_visible {
                        Style::default().fg(accent).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(highlight)
                    };
                    buf.set_string(qx, header.y, &queue_label, qi_style);
                    right_x = qx;
                } else {
                    self.queue_indicator_rect = None;
                }

                // Active playlist indicator.
                if let Some(ref pl_name) = self.active_playlist {
                    let pl_label = format!(" {pl_name} ");
                    let pl_label_len = pl_label.len() as u16;
                    let px = right_x.saturating_sub(pl_label_len);
                    self.playlist_indicator_rect = Some(Rect {
                        x: px, y: header.y, width: pl_label_len, height: 1,
                    });
                    let pl_style = if self.active_playlist_popup_visible {
                        Style::default().fg(accent).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(highlight)
                    };
                    buf.set_string(px, header.y, &pl_label, pl_style);
                } else {
                    self.playlist_indicator_rect = None;
                }

                // Main content.
                match self.active_tab {
                    0 => {
                        // Home tab.
                        let kb_key = self.settings.keybindings
                            .keys_for(BindableAction::ToggleKeybindings)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "K".into());
                        frame.render_widget(
                            HomeTab {
                                state: &self.home_state,
                                recent_tracks: &self.recent_tracks,
                                playlists: &self.playlists_state.playlists,
                                theme: &self.settings.theme_colors,
                                keybindings_key: &kb_key,
                                focus_tabbar: self.focus_tabbar,
                            },
                            content,
                        );
                        // Store pane rects for mouse hit-testing (accounting for logo + separator).
                        let lh = crate::ui::tabs::home::logo_height(
                            content.height,
                            self.home_state.logo_index,
                        );
                        let below_y = content.y + lh;
                        let below_h = content.height.saturating_sub(lh + 1); // -1 for keybindings hint row
                        let left_w = (content.width.saturating_sub(1)) / 2;
                        let right_w = content.width.saturating_sub(left_w + 1);
                        self.home_state.recent_rect = Some(Rect {
                            x: content.x,
                            y: below_y,
                            width: left_w,
                            height: below_h,
                        });
                        self.home_state.playlist_rect = Some(Rect {
                            x: content.x + left_w + 1,
                            y: below_y,
                            width: right_w,
                            height: below_h,
                        });
                        // Store keybindings hint rect for mouse click.
                        let hint_y = content.y + content.height.saturating_sub(1);
                        self.keybindings_hint_rect = Some(Rect {
                            x: content.x,
                            y: hint_y,
                            width: content.width,
                            height: 1,
                        });
                        self.progress_bar_rect = None;
                        self.controls_rect = None;
                    }
                    1 => {
                        // Playing tab.
                        frame.render_widget(
                            PlayingTab {
                                state: &self.player.playing,
                                queue: &self.player.queue,
                                album_art: &self.player.album_art,
                            },
                            content,
                        );
                        // Compute rects for mouse zones (must match PlayingTab layout).
                        let art_rows = compute_art_rows(
                            self.player.album_art.has_art,
                            self.player.album_art.cells.len(),
                            content.height,
                        );
                        // Controls line (play/pause + volume).
                        let controls_y = content.y + art_rows + 3;
                        if controls_y < content.y + content.height {
                            self.controls_rect = Some(Rect {
                                x: content.x,
                                y: controls_y,
                                width: content.width,
                                height: 1,
                            });
                        }
                        // Progress/seek bar (centered at 80% width).
                        let progress_y = content.y + art_rows + 4;
                        if progress_y < content.y + content.height {
                            let bar_inner_w = ((content.width as f64) * 0.8) as u16;
                            let bar_inner_w = bar_inner_w.max(20);
                            let bar_x_off = (content.width.saturating_sub(bar_inner_w)) / 2;
                            self.progress_bar_rect = Some(Rect {
                                x: content.x + bar_x_off,
                                y: progress_y,
                                width: bar_inner_w,
                                height: 1,
                            });
                        }
                        // EQ overlay on top of the visualizer area.
                        if self.eq_state.visible {
                            let eq_y = content.y + art_rows + 6;
                            let eq_h = content.height.saturating_sub(art_rows + 6);
                            if eq_h >= 5 {
                                let eq_area = Rect {
                                    x: content.x,
                                    y: eq_y,
                                    width: content.width,
                                    height: eq_h,
                                };
                                self.eq_rect = Some(eq_area);
                                render_eq(&mut self.eq_state, eq_area, frame.buffer_mut());
                            } else {
                                self.eq_rect = None;
                            }
                        } else {
                            self.eq_rect = None;
                        }
                    }
                    2 => {
                        // Library tab.
                        let pl_key = self.settings.keybindings
                            .keys_for(BindableAction::SavePlaylist)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "a".into());
                        let eq_key = self.settings.keybindings
                            .keys_for(BindableAction::EnqueueTrack)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "E".into());
                        frame.render_widget(
                            LibraryTab {
                                state: &self.library_state,
                                playlist_key: &pl_key,
                                enqueue_key: &eq_key,
                                focus_tabbar: self.focus_tabbar,
                            },
                            content,
                        );
                        self.progress_bar_rect = None;
                        self.controls_rect = None;
                    }
                    3 => {
                        // Settings tab.
                        frame.render_widget(
                            SettingsTab {
                                state: &self.settings_state,
                                settings: &self.settings,
                                focus_tabbar: self.focus_tabbar,
                            },
                            content,
                        );
                        self.progress_bar_rect = None;
                        self.controls_rect = None;
                    }
                    _ => {
                        self.progress_bar_rect = None;
                        self.controls_rect = None;
                    }
                }

                // --- Popup overlays (rendered on top of everything) ---

                // Search popup (~80% height, ~70% width, centered).
                if self.search_visible {
                    let popup = centered_popup(area, 70, 80);
                    // Clear the popup area.
                    let buf = frame.buffer_mut();
                    for y in popup.y..popup.y + popup.height {
                        for x in popup.x..popup.x + popup.width {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.reset();
                            }
                        }
                    }
                    // Draw border.
                    render_popup_border(buf, popup, "Search", accent);
                    // Inner area (inside border).
                    let inner = Rect {
                        x: popup.x + 1,
                        y: popup.y + 1,
                        width: popup.width.saturating_sub(2),
                        height: popup.height.saturating_sub(2),
                    };
                    SearchTab { state: &self.search_state, library: &self.library_state.library }.render(inner, buf);
                }

                // Pomodoro popup (square, centered).
                if self.pomodoro_visible {
                    let popup = centered_square_popup(area);
                    let buf = frame.buffer_mut();
                    for y in popup.y..popup.y + popup.height {
                        for x in popup.x..popup.x + popup.width {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.reset();
                            }
                        }
                    }
                    render_popup_border(buf, popup, "Pomodoro", Color::Green);
                    let inner = Rect {
                        x: popup.x + 1,
                        y: popup.y + 1,
                        width: popup.width.saturating_sub(2),
                        height: popup.height.saturating_sub(2),
                    };
                    PomodoroTab { timer: &self.pomodoro }.render(inner, buf);
                }

                // Keybindings reference popup.
                if self.keybindings_visible {
                    let popup = centered_popup(area, 60, 80);
                    let buf = frame.buffer_mut();
                    for y in popup.y..popup.y + popup.height {
                        for x in popup.x..popup.x + popup.width {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.reset();
                            }
                        }
                    }
                    render_popup_border(buf, popup, "Keybindings", accent);
                    let inner = Rect {
                        x: popup.x + 1,
                        y: popup.y + 1,
                        width: popup.width.saturating_sub(2),
                        height: popup.height.saturating_sub(2),
                    };
                    render_keybindings_popup(buf, inner, &self.settings, accent, text_color);
                }

                // Playlist picker popup.
                if self.playlist_picker_visible {
                    let popup = centered_popup(area, 50, 60);
                    let buf = frame.buffer_mut();
                    for y in popup.y..popup.y + popup.height {
                        for x in popup.x..popup.x + popup.width {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.reset();
                            }
                        }
                    }
                    render_popup_border(buf, popup, "Add to Playlist", accent);
                    let inner = Rect {
                        x: popup.x + 1,
                        y: popup.y + 1,
                        width: popup.width.saturating_sub(2),
                        height: popup.height.saturating_sub(2),
                    };
                    render_playlist_picker(
                        buf, inner,
                        &self.playlists_state.playlists,
                        self.playlist_picker_selected,
                        self.playlist_picker_tracks.len(),
                        text_color,
                    );
                }

                // Recently played popup.
                if self.recent_popup_visible {
                    let popup = centered_popup(area, 60, 70);
                    let buf = frame.buffer_mut();
                    for y in popup.y..popup.y + popup.height {
                        for x in popup.x..popup.x + popup.width {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.reset();
                            }
                        }
                    }
                    render_popup_border(buf, popup, "Recently Played", accent);
                    let inner = Rect {
                        x: popup.x + 1,
                        y: popup.y + 1,
                        width: popup.width.saturating_sub(2),
                        height: popup.height.saturating_sub(2),
                    };
                    render_recent_popup(
                        buf, inner,
                        &self.recent_tracks,
                        self.recent_popup_selected,
                        text_color,
                    );
                }

                // Active playlist popup.
                if self.active_playlist_popup_visible {
                    if let Some(ref pl_name) = self.active_playlist {
                        let popup = centered_popup(area, 50, 60);
                        let buf = frame.buffer_mut();
                        for y in popup.y..popup.y + popup.height {
                            for x in popup.x..popup.x + popup.width {
                                if let Some(cell) = buf.cell_mut((x, y)) {
                                    cell.reset();
                                }
                            }
                        }
                        render_popup_border(buf, popup, pl_name, accent);
                        let inner = Rect {
                            x: popup.x + 1,
                            y: popup.y + 1,
                            width: popup.width.saturating_sub(2),
                            height: popup.height.saturating_sub(2),
                        };
                        render_queue_popup(
                            buf, inner,
                            &self.player.queue,
                            self.active_playlist_popup_selected,
                            text_color,
                        );
                    }
                }

                // Queue popup.
                if self.queue_popup_visible {
                    let popup = centered_popup(area, 50, 60);
                    let buf = frame.buffer_mut();
                    for y in popup.y..popup.y + popup.height {
                        for x in popup.x..popup.x + popup.width {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.reset();
                            }
                        }
                    }
                    render_popup_border(buf, popup, "Queue", accent);
                    let inner = Rect {
                        x: popup.x + 1,
                        y: popup.y + 1,
                        width: popup.width.saturating_sub(2),
                        height: popup.height.saturating_sub(2),
                    };
                    render_queue_popup(
                        buf, inner,
                        &self.player.queue,
                        self.queue_popup_selected,
                        text_color,
                    );
                }

                // Playlist naming overlay at bottom of screen.
                if self.naming_playlist.is_some() {
                    let footer_y = area.y + area.height.saturating_sub(1);
                    let footer_area = Rect {
                        x: area.x,
                        y: footer_y,
                        width: area.width,
                        height: 1,
                    };
                    let naming_hint = self.build_naming_hint();
                    frame.render_widget(Paragraph::new(naming_hint), footer_area);
                }

                // No post-render hover painting — mouse move syncs selection
                // state directly, so the selected item is already painted correctly
                // by the tab's render method using the same teal style.

                // Glimmer effect — traveling brightness wave on Playing tab text.
                let buf = frame.buffer_mut();
                if self.active_tab == 1 {
                    if let (Some(progress), Some(content_rect)) = (glimmer_progress, self.content_rect) {
                        let art_rows = compute_art_rows(
                            self.player.album_art.has_art,
                            self.player.album_art.cells.len(),
                            content_rect.height,
                        );

                        let text_rows = [
                            content_rect.y + art_rows,     // title
                            content_rect.y + art_rows + 1, // artist
                            content_rect.y + art_rows + 2, // album
                        ];

                        let w = content_rect.width as f64;
                        let wave_center = progress * (w + 10.0) - 5.0;
                        let wave_radius = 4.0;

                        for &row_y in &text_rows {
                            if row_y >= content_rect.y + content_rect.height {
                                continue;
                            }
                            for cx in content_rect.x..content_rect.x + content_rect.width {
                                let dist = ((cx - content_rect.x) as f64 - wave_center).abs();
                                if dist > wave_radius {
                                    continue;
                                }
                                if let Some(cell) = buf.cell_mut((cx, row_y)) {
                                    let factor = 1.0 - (dist / wave_radius);
                                    let bright = brighten_color(cell.fg, factor as f32);
                                    cell.set_style(Style::default().fg(bright));
                                }
                            }
                        }
                    }
                }
            })?;

            if self.should_quit {
                return Ok(());
            }

            // Rebinding mode: capture the next keypress as the new binding.
            if self.settings_state.rebinding {
                if let Some(action) = self.settings_state.rebind_action {
                    if crossterm::event::poll(Duration::from_millis(16))? {
                        if let crossterm::event::Event::Key(crossterm::event::KeyEvent { code, .. }) =
                            crossterm::event::read()?
                        {
                            let key_str = key_to_string(&code);
                            if key_str != "unknown" {
                                self.settings.keybindings.set_key(action, key_str.clone());
                                self.key_lookup = self.settings.keybindings.build_lookup();
                                self.settings_state.status_message =
                                    Some(format!("Bound '{}' to {}", key_str, action.label()));
                                let _ = self.settings.save();
                            }
                            self.settings_state.rebinding = false;
                            self.settings_state.rebind_action = None;
                        }
                    }
                    continue;
                }
            }

            // Determine if a text field is capturing input.
            let text_capture = self.is_text_capture_active();

            // Handle input (~60fps target → 16ms timeout).
            match input::poll_input(Duration::from_millis(16), text_capture, &self.key_lookup)? {
                Action::Bound(action) => {
                    self.handle_bound_action(action);
                }
                Action::Char(ch) => {
                    self.handle_char(ch);
                }
                Action::MouseScrollUp { col, row } => {
                    self.handle_mouse_scroll(col, row, true);
                }
                Action::MouseScrollDown { col, row } => {
                    self.handle_mouse_scroll(col, row, false);
                }
                Action::MouseClick { col, row } => {
                    self.handle_mouse_click(col, row);
                }
                Action::MouseMove { col, row } => {
                    self.mouse_pos = (col, row);
                    if self.eq_state.visible {
                        self.eq_state.hovered_band = self.eq_state.band_at(col, row);
                    }
                    self.handle_mouse_hover(col, row);
                }
                Action::None => {}
            }
        }
    }

    /// Handle a bound action from the keybinding system.
    fn handle_bound_action(&mut self, action: BindableAction) {
        use BindableAction::*;
        match action {
            Quit => {
                if self.naming_playlist.is_some() {
                    self.naming_playlist = None;
                    self.playlist_name_buf.clear();
                } else if self.search_visible {
                    self.search_visible = false;
                } else if self.pomodoro_visible {
                    self.pomodoro_visible = false;
                } else if self.keybindings_visible {
                    self.keybindings_visible = false;
                } else {
                    self.player.shutdown();
                    self.should_quit = true;
                }
            }
            TogglePlayPause => {
                if self.naming_playlist.is_some() {
                    return;
                }
                if self.pomodoro_visible {
                    self.pomodoro.toggle_pause();
                } else if self.player.playing.playback == PlaybackState::Stopped {
                    // Try resuming from queue first; if empty, load from current tab.
                    if self.player.queue.current_track().is_some() {
                        self.player.toggle_play_pause();
                    } else {
                        self.play_from_current_tab();
                    }
                } else {
                    self.player.toggle_play_pause();
                }
            }
            VolumeUp => self.player.volume_up(),
            VolumeDown => self.player.volume_down(),
            SeekForward => {
                if self.active_tab == 1 && self.eq_state.visible {
                    self.eq_state.next_preset();
                    self.send_eq_gains();
                } else {
                    self.player.seek_forward(self.settings.seek_step_secs);
                }
            }
            SeekBackward => {
                if self.active_tab == 1 && self.eq_state.visible {
                    self.eq_state.prev_preset();
                    self.send_eq_gains();
                } else {
                    self.player.seek_backward(self.settings.seek_step_secs);
                }
            }
            TabNext => {
                if self.naming_playlist.is_none() && !self.search_visible && !self.pomodoro_visible && !self.keybindings_visible {
                    // Left/Right in content: context-specific (e.g., sort mode, pane switch).
                    if !self.focus_tabbar {
                        match self.active_tab {
                            0 => self.home_state.switch_pane(),
                            2 => {
                                if matches!(self.library_state.view, LibraryView::Artists)
                                    && self.library_state.selected == 0
                                {
                                    // On breadcrumb row: cycle sort mode.
                                    self.library_state.sort_mode = self.library_state.sort_mode.next();
                                    self.library_state.scroll_offset = 0;
                                }
                            }
                            _ => {}
                        }
                    } else {
                        let prev = self.active_tab;
                        self.active_tab = (self.active_tab + 1) % TAB_COUNT;
                        self.focus_tabbar = true;
                        if self.active_tab == 0 && prev != 0 {
                            self.home_state.randomize_logo();
                        }
                    }
                }
            }
            TabPrev => {
                if self.naming_playlist.is_none() && !self.search_visible && !self.pomodoro_visible && !self.keybindings_visible {
                    if !self.focus_tabbar {
                        match self.active_tab {
                            0 => self.home_state.switch_pane(),
                            2 => {
                                if matches!(self.library_state.view, LibraryView::Artists)
                                    && self.library_state.selected == 0
                                {
                                    self.library_state.sort_mode = self.library_state.sort_mode.prev();
                                    self.library_state.scroll_offset = 0;
                                }
                            }
                            _ => {}
                        }
                    } else {
                        let prev = self.active_tab;
                        self.active_tab = (self.active_tab + TAB_COUNT - 1) % TAB_COUNT;
                        self.focus_tabbar = true;
                        if self.active_tab == 0 && prev != 0 {
                            self.home_state.randomize_logo();
                        }
                    }
                }
            }
            SwitchPane => {
                if self.active_tab == 0 && !self.search_visible && !self.pomodoro_visible && !self.keybindings_visible {
                    self.home_state.switch_pane();
                }
            }
            NextTrack => self.player.play_next(&self.settings),
            PrevTrack => self.player.play_prev(),
            ScrollDown => self.handle_scroll_down(),
            ScrollUp => self.handle_scroll_up(),
            Enter => self.handle_enter(),
            Back => self.handle_back(),
            Backspace => self.handle_backspace(),
            // Popup toggles.
            ToggleSearch => {
                if self.naming_playlist.is_none() {
                    self.search_visible = !self.search_visible;
                    self.pomodoro_visible = false;
                    self.keybindings_visible = false;
                    if self.search_visible {
                        self.search_state.is_active = true;
                    }
                }
            }
            TogglePomodoro => {
                if self.naming_playlist.is_none() {
                    self.pomodoro_visible = !self.pomodoro_visible;
                    self.search_visible = false;
                    self.keybindings_visible = false;
                }
            }
            ToggleKeybindings => {
                if self.naming_playlist.is_none() {
                    self.keybindings_visible = !self.keybindings_visible;
                    self.search_visible = false;
                    self.pomodoro_visible = false;
                }
            }
            // Context-specific actions.
            ToggleEq => {
                if self.active_tab == 1 {
                    self.eq_state.toggle_visible();
                }
            }
            ToggleEqEnabled => {
                if self.active_tab == 1 && self.eq_state.visible {
                    self.eq_state.toggle_enabled();
                    self.player.send(AudioCommand::ToggleEq);
                }
            }
            AddToPlaylist => {
                if self.active_tab == 2 && self.naming_playlist.is_none() {
                    if self.playlist_picker_visible {
                        self.playlist_picker_visible = false;
                    } else {
                        // Gather tracks to add from library.
                        let checked = self.library_state.take_checked_paths();
                        let tracks = if !checked.is_empty() {
                            checked
                        } else {
                            match &self.library_state.view {
                                LibraryView::Tracks { .. } => {
                                    self.library_state.selected_track_path()
                                        .into_iter().collect()
                                }
                                _ => {
                                    self.library_state.selected_flat_song_path()
                                        .into_iter().collect()
                                }
                            }
                        };
                        if !tracks.is_empty() {
                            self.playlist_picker_tracks = tracks;
                            self.playlist_picker_selected = 0;
                            self.playlist_picker_visible = true;
                            self.search_visible = false;
                            self.pomodoro_visible = false;
                            self.keybindings_visible = false;
                            self.recent_popup_visible = false;
                        }
                    }
                }
            }
            ShowRecentlyPlayed => {
                if self.naming_playlist.is_none() {
                    self.recent_popup_visible = !self.recent_popup_visible;
                    if self.recent_popup_visible {
                        self.recent_popup_selected = 0;
                        self.search_visible = false;
                        self.pomodoro_visible = false;
                        self.keybindings_visible = false;
                        self.playlist_picker_visible = false;
                    }
                }
            }
            EnqueueTrack => {
                if self.active_tab == 2 {
                    // Enqueue checked tracks, or the selected track if none checked.
                    let checked = self.library_state.take_checked_paths();
                    if !checked.is_empty() {
                        for path in checked {
                            self.player.queue.push(path);
                        }
                    } else {
                        // Enqueue single selected track.
                        let path = match &self.library_state.view {
                            LibraryView::Tracks { .. } => self.library_state.selected_track_path(),
                            _ => self.library_state.selected_flat_song_path(),
                        };
                        if let Some(path) = path {
                            self.player.queue.push(path);
                        }
                    }
                }
            }
            SavePlaylist => {
                if self.active_tab == 2 {
                    let selected = self.library_state.take_checked_paths();
                    if !selected.is_empty() {
                        self.naming_playlist = Some(PlaylistSource::LibrarySelection(selected));
                        self.playlist_name_buf.clear();
                    }
                }
            }
            ToggleCheckbox => {
                if self.active_tab == 2 {
                    self.library_state.toggle_selected();
                }
            }
            ViewTracks => {
                if self.active_tab == 2
                    && matches!(self.library_state.view, LibraryView::Artists)
                    && !self.search_visible
                    && !self.pomodoro_visible
                    && !self.keybindings_visible
                {
                    // On Library tab at top level, cycle through sort modes.
                    self.library_state.sort_mode = self.library_state.sort_mode.next();
                    self.library_state.selected = 0;
                    self.library_state.scroll_offset = 0;
                } else if self.pomodoro_visible {
                    self.pomodoro.cycle_style();
                }
            }
            DeletePlaylist => {
                // Only meaningful on Home tab playlists pane.
                if self.active_tab == 0 && self.home_state.pane == HomePane::Playlists {
                    if let Some(pl) = self.playlists_state.playlists.get(self.home_state.playlist_selected) {
                        let name = pl.name.clone();
                        if let Err(e) = PlaylistManager::delete(&name) {
                            tracing::error!("Delete failed: {e}");
                        } else {
                            self.playlists_state.reload();
                            if self.home_state.playlist_selected >= self.playlists_state.playlists.len()
                                && !self.playlists_state.playlists.is_empty()
                            {
                                self.home_state.playlist_selected = self.playlists_state.playlists.len() - 1;
                            }
                        }
                    }
                }
            }
            SkipPomodoro => {
                if self.pomodoro_visible {
                    match self.pomodoro.skip() {
                        PomodoroAction::PauseMusic => {
                            self.player.send(AudioCommand::Pause);
                        }
                        PomodoroAction::ResumeMusic => {
                            self.player.send(AudioCommand::Resume);
                        }
                        PomodoroAction::None => {}
                    }
                }
            }
            CyclePomodoroStyle => {
                if self.pomodoro_visible {
                    self.pomodoro.cycle_style();
                }
            }
        }
    }

    /// Whether a text input field is currently capturing keyboard input.
    fn is_text_capture_active(&self) -> bool {
        if self.naming_playlist.is_some() {
            return true;
        }
        // Settings tab in edit mode.
        if self.active_tab == 3 && self.settings_state.editing {
            return true;
        }
        // Search popup captures text when visible and active.
        if self.search_visible && self.search_state.is_active {
            return true;
        }
        false
    }

    /// Try to play the selected item from the current tab.
    /// Used when space is pressed but nothing is playing.
    fn play_from_current_tab(&mut self) {
        // If search popup is open, play selected search result.
        if self.search_visible {
            if let Some(path) = self.search_state.selected_track_path_from(&self.library_state.library) {
                self.player.enqueue_and_play(path);
            }
            return;
        }

        match self.active_tab {
            0 => {
                // Home tab: play from focused pane.
                match self.home_state.pane {
                    HomePane::RecentlyPlayed => {
                        if let Some(path) = self.home_state.selected_recent_path(&self.recent_tracks).cloned() {
                            self.player.enqueue_and_play(path);
                        }
                    }
                    HomePane::Playlists => {
                        if let Some(pl) = self.playlists_state.playlists.get(self.home_state.playlist_selected) {
                            let name = pl.name.clone();
                            let tracks = pl.tracks.clone();
                            if let Some(first) = tracks.first().cloned() {
                                self.player.queue.clear();
                                self.player.queue.extend(tracks);
                                self.player.send(AudioCommand::Play(first));
                                self.active_playlist = Some(name);
                                self.active_tab = 1;
                            }
                        }
                    }
                }
            }
            2 => {
                // Library: enqueue based on view level, switch to Playing tab.
                match &self.library_state.view {
                    LibraryView::Tracks { .. } => {
                        if let Some(path) = self.library_state.selected_track_path() {
                            self.player.enqueue_and_play(path);
                            self.active_tab = 1;
                        }
                    }
                    LibraryView::Albums { artist } => {
                        if let Some(album) = self.library_state.library
                            .albums_for(artist)
                            .get(self.library_state.selected)
                        {
                            let paths: Vec<_> = album.tracks.iter().map(|t| t.path.clone()).collect();
                            if let Some(first) = paths.first().cloned() {
                                self.player.queue.clear();
                                self.player.queue.extend(paths);
                                self.player.send(AudioCommand::Play(first));
                                self.active_playlist = None;
                                self.active_tab = 1;
                            }
                        }
                    }
                    LibraryView::Artists => {
                        let names = self.library_state.library.artist_names();
                        if let Some(name) = names.get(self.library_state.selected) {
                            let paths: Vec<_> = self.library_state.library
                                .albums_for(name)
                                .iter()
                                .flat_map(|a| a.tracks.iter().map(|t| t.path.clone()))
                                .collect();
                            if let Some(first) = paths.first().cloned() {
                                self.player.queue.clear();
                                self.player.queue.extend(paths);
                                self.player.send(AudioCommand::Play(first));
                                self.active_playlist = None;
                                self.active_tab = 1;
                            }
                        }
                    }
                }
            }
            _ => {
                // Playing tab or any other: try to play current track from queue.
                if let Some(path) = self.player.queue.current_track().cloned() {
                    self.player.send(AudioCommand::Play(path));
                }
            }
        }
    }

    fn handle_scroll_down(&mut self) {
        if self.search_visible {
            self.search_state.move_down();
            return;
        }
        if self.pomodoro_visible {
            return;
        }
        if self.playlist_picker_visible {
            if self.playlist_picker_selected + 1 < self.playlists_state.playlists.len() {
                self.playlist_picker_selected += 1;
            }
            return;
        }
        if self.recent_popup_visible {
            if self.recent_popup_selected + 1 < self.recent_tracks.len() {
                self.recent_popup_selected += 1;
            }
            return;
        }
        if self.queue_popup_visible {
            if self.queue_popup_selected + 1 < self.player.queue.len() {
                self.queue_popup_selected += 1;
            }
            return;
        }
        if self.active_playlist_popup_visible {
            if self.active_playlist_popup_selected + 1 < self.player.queue.len() {
                self.active_playlist_popup_selected += 1;
            }
            return;
        }
        // From tab bar, pressing Down enters the content.
        if self.focus_tabbar {
            self.focus_tabbar = false;
            return;
        }
        match self.active_tab {
            0 => {
                self.home_state.move_down(
                    self.recent_tracks.len(),
                    self.playlists_state.playlists.len(),
                );
            }
            2 => {
                self.library_state.move_down();
                self.update_library_scroll();
            }
            3 => {
                self.settings_state
                    .move_down(SettingsState::item_count());
            }
            _ => {}
        }
    }

    fn handle_scroll_up(&mut self) {
        if self.search_visible {
            self.search_state.move_up();
            return;
        }
        if self.pomodoro_visible {
            return;
        }
        if self.playlist_picker_visible {
            self.playlist_picker_selected = self.playlist_picker_selected.saturating_sub(1);
            return;
        }
        if self.recent_popup_visible {
            self.recent_popup_selected = self.recent_popup_selected.saturating_sub(1);
            return;
        }
        if self.queue_popup_visible {
            self.queue_popup_selected = self.queue_popup_selected.saturating_sub(1);
            return;
        }
        if self.active_playlist_popup_visible {
            self.active_playlist_popup_selected = self.active_playlist_popup_selected.saturating_sub(1);
            return;
        }
        if self.focus_tabbar {
            // Already at tab bar; nothing to do.
            return;
        }
        // Check if at top of content — if so, go back to tab bar.
        let at_top = match self.active_tab {
            0 => self.home_state.move_up(),
            2 => {
                if self.library_state.selected == 0 {
                    true
                } else {
                    self.library_state.move_up();
                    self.update_library_scroll();
                    false
                }
            }
            3 => {
                if self.settings_state.selected == 0 {
                    true
                } else {
                    self.settings_state.move_up();
                    false
                }
            }
            _ => true,
        };
        if at_top {
            self.focus_tabbar = true;
        }
    }

    fn handle_enter(&mut self) {
        // If naming a playlist, finalize it.
        if let Some(source) = self.naming_playlist.take() {
            let name = self.playlist_name_buf.trim().to_string();
            self.playlist_name_buf.clear();
            if name.is_empty() {
                return;
            }
            let tracks = match source {
                PlaylistSource::Queue => self.player.queue.tracks().to_vec(),
                PlaylistSource::LibrarySelection(paths) => paths,
            };
            if tracks.is_empty() {
                return;
            }
            let playlist = Playlist::new(name.clone(), tracks);
            match PlaylistManager::save(&playlist) {
                Ok(()) => {
                    self.playlists_state.reload();
                    tracing::info!("Saved playlist '{name}'");
                }
                Err(e) => {
                    tracing::error!("Failed to save playlist: {e}");
                }
            }
            return;
        }

        // Playlist picker: add tracks to selected playlist.
        if self.playlist_picker_visible {
            if let Some(playlist) = self.playlists_state.playlists.get_mut(self.playlist_picker_selected) {
                let new_tracks = std::mem::take(&mut self.playlist_picker_tracks);
                playlist.tracks.extend(new_tracks);
                let updated = playlist.clone();
                match PlaylistManager::save(&updated) {
                    Ok(()) => {
                        tracing::info!("Added tracks to playlist '{}'", updated.name);
                    }
                    Err(e) => {
                        tracing::error!("Failed to save playlist: {e}");
                    }
                }
            }
            self.playlist_picker_visible = false;
            return;
        }

        // Active playlist popup: play selected track.
        if self.active_playlist_popup_visible {
            let tracks = self.player.queue.tracks();
            if let Some(path) = tracks.get(self.active_playlist_popup_selected).cloned() {
                self.player.queue.set_current(self.active_playlist_popup_selected);
                self.player.send(AudioCommand::Play(path));
                self.active_tab = 1;
            }
            self.active_playlist_popup_visible = false;
            return;
        }

        // Queue popup: play selected track.
        if self.queue_popup_visible {
            let tracks = self.player.queue.tracks();
            if let Some(path) = tracks.get(self.queue_popup_selected).cloned() {
                self.player.queue.set_current(self.queue_popup_selected);
                self.player.send(AudioCommand::Play(path));
                self.active_tab = 1;
            }
            self.queue_popup_visible = false;
            return;
        }

        // Recently played: play selected track.
        if self.recent_popup_visible {
            if let Some(path) = self.recent_tracks.get(self.recent_popup_selected).cloned() {
                self.player.enqueue_and_play(path);
                self.active_tab = 1;
            }
            self.recent_popup_visible = false;
            return;
        }

        // Search popup: play selected result.
        if self.search_visible {
            if let Some(path) = self.search_state.selected_track_path_from(&self.library_state.library) {
                self.player.enqueue_and_play(path);
                self.search_visible = false;
                self.active_tab = 1; // switch to Playing tab
            }
            return;
        }

        // Pomodoro popup: start if not active.
        if self.pomodoro_visible {
            if !self.pomodoro.active {
                self.pomodoro.start();
            }
            return;
        }

        match self.active_tab {
            0 => {
                // Home tab: play from focused pane.
                match self.home_state.pane {
                    HomePane::RecentlyPlayed => {
                        if let Some(path) = self.home_state.selected_recent_path(&self.recent_tracks).cloned() {
                            self.player.enqueue_and_play(path);
                            self.active_tab = 1;
                        }
                    }
                    HomePane::Playlists => {
                        if let Some(pl) = self.playlists_state.playlists.get(self.home_state.playlist_selected) {
                            let name = pl.name.clone();
                            let tracks = pl.tracks.clone();
                            if !tracks.is_empty() {
                                self.player.queue.clear();
                                self.player.queue.extend(tracks);
                                if let Some(path) = self.player.queue.current_track().cloned() {
                                    self.player.send(AudioCommand::Play(path));
                                }
                                self.active_playlist = Some(name);
                                self.active_tab = 1;
                            }
                        }
                    }
                }
            }
            2 => {
                // Library.
                if let LibraryView::Tracks { .. } = &self.library_state.view {
                    if let Some(path) = self.library_state.selected_track_path() {
                        self.player.enqueue_and_play(path);
                        self.active_tab = 1;
                    }
                } else if matches!(
                    (&self.library_state.view, self.library_state.sort_mode),
                    (LibraryView::Artists, LibrarySortMode::Songs)
                ) {
                    // Songs flat-list: enter plays the selected song.
                    if let Some(path) = self.library_state.selected_flat_song_path() {
                        self.player.enqueue_and_play(path);
                        self.active_tab = 1;
                    }
                } else {
                    self.library_state.enter();
                }
            }
            3 => {
                // Settings.
                self.settings_state.toggle(&mut self.settings);
                let _ = self.settings.save();
            }
            _ => {}
        }
    }

    fn handle_back(&mut self) {
        if self.naming_playlist.is_some() {
            self.naming_playlist = None;
            self.playlist_name_buf.clear();
            return;
        }

        if self.search_visible {
            self.search_visible = false;
            return;
        }

        if self.pomodoro_visible {
            if self.pomodoro.active {
                self.pomodoro.stop();
            } else {
                self.pomodoro_visible = false;
            }
            return;
        }

        if self.keybindings_visible {
            self.keybindings_visible = false;
            return;
        }

        if self.playlist_picker_visible {
            self.playlist_picker_visible = false;
            self.playlist_picker_tracks.clear();
            return;
        }

        if self.recent_popup_visible {
            self.recent_popup_visible = false;
            return;
        }

        if self.queue_popup_visible {
            self.queue_popup_visible = false;
            return;
        }

        if self.active_playlist_popup_visible {
            self.active_playlist_popup_visible = false;
            return;
        }

        match self.active_tab {
            0 => {
                // Home tab: switch pane or do nothing.
            }
            2 => self.library_state.back(),
            3 => {
                if self.settings_state.editing {
                    self.settings_state.cancel_edit();
                }
            }
            _ => {}
        }
    }

    fn handle_char(&mut self, ch: char) {
        // If naming a playlist, capture characters.
        if self.naming_playlist.is_some() {
            self.playlist_name_buf.push(ch);
            return;
        }
        // Settings tab in edit mode.
        if self.active_tab == 3 && self.settings_state.editing {
            self.settings_state.edit_push(ch);
            return;
        }
        // Search popup: typing appends character.
        if self.search_visible {
            self.search_state.is_active = true;
            self.search_state.push_char(ch);
            self.search_state
                .update_results(&self.library_state.library);
        }
    }

    fn handle_backspace(&mut self) {
        if self.naming_playlist.is_some() {
            self.playlist_name_buf.pop();
            return;
        }

        if self.active_tab == 3 && self.settings_state.editing {
            self.settings_state.edit_pop();
            return;
        }
        if self.search_visible {
            self.search_state.pop_char();
            self.search_state
                .update_results(&self.library_state.library);
        }
    }

    fn handle_mouse_click(&mut self, col: u16, row: u16) {
        // Popup close button [x] click — must be checked before tab bar.
        // We need the terminal area to compute popup rects; use the last known tab_bar_rect
        // as a proxy for area.x/area.y. The actual area is the full terminal area.
        // We recompute popup rects using the same logic as in the render code.
        if let Some(tab_rect) = self.tab_bar_rect {
            // Approximate the full terminal area from the tab bar rect.
            // The tab bar is at y=0, x=0, so area.x = tab_rect.x and we infer height from
            // content_rect if available.
            let area = if let Some(content_rect) = self.content_rect {
                Rect {
                    x: tab_rect.x,
                    y: tab_rect.y,
                    width: tab_rect.width,
                    height: content_rect.y + content_rect.height - tab_rect.y,
                }
            } else {
                Rect {
                    x: tab_rect.x,
                    y: tab_rect.y,
                    width: tab_rect.width,
                    height: 40, // fallback
                }
            };

            let close_str_len = 3u16; // "[x]"
            if self.search_visible {
                let popup = centered_popup(area, 70, 80);
                let close_x = popup.x + popup.width.saturating_sub(close_str_len + 1);
                if row == popup.y && col >= close_x && col < close_x + close_str_len {
                    self.search_visible = false;
                    return;
                }
            }
            if self.pomodoro_visible {
                let popup = centered_square_popup(area);
                let close_x = popup.x + popup.width.saturating_sub(close_str_len + 1);
                if row == popup.y && col >= close_x && col < close_x + close_str_len {
                    self.pomodoro_visible = false;
                    return;
                }
            }
            if self.keybindings_visible {
                let popup = centered_popup(area, 60, 80);
                let close_x = popup.x + popup.width.saturating_sub(close_str_len + 1);
                if row == popup.y && col >= close_x && col < close_x + close_str_len {
                    self.keybindings_visible = false;
                    return;
                }
            }
            if self.playlist_picker_visible {
                let popup = centered_popup(area, 50, 60);
                let close_x = popup.x + popup.width.saturating_sub(close_str_len + 1);
                if row == popup.y && col >= close_x && col < close_x + close_str_len {
                    self.playlist_picker_visible = false;
                    self.playlist_picker_tracks.clear();
                    return;
                }
            }
            if self.recent_popup_visible {
                let popup = centered_popup(area, 60, 70);
                let close_x = popup.x + popup.width.saturating_sub(close_str_len + 1);
                if row == popup.y && col >= close_x && col < close_x + close_str_len {
                    self.recent_popup_visible = false;
                    return;
                }
            }
            if self.queue_popup_visible {
                let popup = centered_popup(area, 50, 60);
                let close_x = popup.x + popup.width.saturating_sub(close_str_len + 1);
                if row == popup.y && col >= close_x && col < close_x + close_str_len {
                    self.queue_popup_visible = false;
                    return;
                }
            }
            if self.active_playlist_popup_visible {
                let popup = centered_popup(area, 50, 60);
                let close_x = popup.x + popup.width.saturating_sub(close_str_len + 1);
                if row == popup.y && col >= close_x && col < close_x + close_str_len {
                    self.active_playlist_popup_visible = false;
                    return;
                }
            }
        }

        // Close button [X] click.
        if let Some(cb_rect) = self.close_button_rect {
            if row == cb_rect.y && col >= cb_rect.x && col < cb_rect.x + cb_rect.width {
                self.player.shutdown();
                self.should_quit = true;
                return;
            }
        }

        // Queue indicator click.
        if let Some(qi_rect) = self.queue_indicator_rect {
            if row == qi_rect.y && col >= qi_rect.x && col < qi_rect.x + qi_rect.width {
                self.queue_popup_visible = !self.queue_popup_visible;
                if self.queue_popup_visible {
                    self.queue_popup_selected = self.player.queue.current_index().unwrap_or(0);
                }
                return;
            }
        }

        // Playlist indicator click.
        if let Some(pi_rect) = self.playlist_indicator_rect {
            if row == pi_rect.y && col >= pi_rect.x && col < pi_rect.x + pi_rect.width {
                self.active_playlist_popup_visible = !self.active_playlist_popup_visible;
                if self.active_playlist_popup_visible {
                    self.active_playlist_popup_selected = self.player.queue.current_index().unwrap_or(0);
                }
                return;
            }
        }

        // Tab bar click.
        if let Some(tab_rect) = self.tab_bar_rect {
            if row == tab_rect.y && col >= tab_rect.x && col < tab_rect.x + tab_rect.width {
                let mut x = tab_rect.x;
                for (i, name) in TAB_NAMES.iter().enumerate() {
                    let label_len = name.len() as u16 + 2;
                    if col >= x && col < x + label_len {
                        if i < TAB_COUNT {
                            let prev = self.active_tab;
                            self.active_tab = i;
                            self.focus_tabbar = true;
                            if i == 0 && prev != 0 {
                                self.home_state.randomize_logo();
                            }
                        }
                        return;
                    }
                    x += label_len;
                }
            }
        }

        // Controls line click → toggle play/pause.
        if let Some(ctrl_rect) = self.controls_rect {
            if row == ctrl_rect.y && col >= ctrl_rect.x && col < ctrl_rect.x + ctrl_rect.width {
                self.player.toggle_play_pause();
                return;
            }
        }

        // Progress bar click (seek).
        if let Some(bar_rect) = self.progress_bar_rect {
            if row == bar_rect.y && col >= bar_rect.x && col < bar_rect.x + bar_rect.width {
                if self.player.playing.duration.as_secs_f64() > 0.0 {
                    let elapsed_label_len = format_duration(self.player.playing.elapsed).len() as u16;
                    let total_label_len = format_duration(self.player.playing.duration).len() as u16;
                    let bar_start = bar_rect.x + elapsed_label_len + 1;
                    let bar_end = bar_rect.x + bar_rect.width.saturating_sub(total_label_len + 1);

                    let fraction = if bar_end <= bar_start {
                        0.0
                    } else if col <= bar_start {
                        0.0
                    } else if col >= bar_end {
                        1.0
                    } else {
                        (col - bar_start) as f64 / (bar_end - bar_start) as f64
                    };
                    self.player.seek_to_fraction(fraction);
                }
                return;
            }
        }

        // Playing tab click — album art or title/artist/album area toggles play/pause.
        if self.active_tab == 1 {
            if let Some(content_rect) = self.content_rect {
                let art_rows = compute_art_rows(
                    self.player.album_art.has_art,
                    self.player.album_art.cells.len(),
                    content_rect.height,
                );
                // Click on album art (exact pixel area).
                let art_source_cols = self.player.album_art.cells.first().map_or(0, |r| r.len());
                if let Some(art_rect) = compute_art_rect(
                    self.player.album_art.has_art,
                    self.player.album_art.cells.len(),
                    art_source_cols,
                    content_rect.x,
                    content_rect.y,
                    content_rect.width,
                    art_rows,
                ) {
                    if row >= art_rect.y && row < art_rect.y + art_rect.height
                        && col >= art_rect.x && col < art_rect.x + art_rect.width
                    {
                        self.player.toggle_play_pause();
                        return;
                    }
                }
                // Click on title/artist/album text rows → also toggle play/pause.
                let text_start = content_rect.y + art_rows;
                let text_end = text_start + 3;
                if row >= text_start && row < text_end
                    && col >= content_rect.x && col < content_rect.x + content_rect.width
                {
                    self.player.toggle_play_pause();
                    return;
                }
            }
        }

        // Home tab click — select item in the clicked pane.
        if self.active_tab == 0 {
            // Check keybindings hint click.
            if let Some(hint_rect) = self.keybindings_hint_rect {
                if row == hint_rect.y && col >= hint_rect.x && col < hint_rect.x + hint_rect.width {
                    self.keybindings_visible = !self.keybindings_visible;
                    return;
                }
            }

            if let Some(recent_rect) = self.home_state.recent_rect {
                if row > recent_rect.y
                    && row < recent_rect.y + recent_rect.height
                    && col >= recent_rect.x
                    && col < recent_rect.x + recent_rect.width
                {
                    self.focus_tabbar = false;
                    self.home_state.pane = HomePane::RecentlyPlayed;
                    let clicked_idx = self.home_state.recent_scroll + (row - recent_rect.y - 1) as usize;
                    if clicked_idx < self.recent_tracks.len() {
                        if self.home_state.recent_selected == clicked_idx {
                            self.handle_enter();
                        } else {
                            self.home_state.recent_selected = clicked_idx;
                        }
                    }
                    return;
                }
            }
            if let Some(pl_rect) = self.home_state.playlist_rect {
                if row > pl_rect.y
                    && row < pl_rect.y + pl_rect.height
                    && col >= pl_rect.x
                    && col < pl_rect.x + pl_rect.width
                {
                    self.focus_tabbar = false;
                    self.home_state.pane = HomePane::Playlists;
                    let clicked_idx = self.home_state.playlist_scroll + (row - pl_rect.y - 1) as usize;
                    if clicked_idx < self.playlists_state.playlists.len() {
                        if self.home_state.playlist_selected == clicked_idx {
                            self.handle_enter();
                        } else {
                            self.home_state.playlist_selected = clicked_idx;
                        }
                    }
                    return;
                }
            }
        }

        // Click on list items → select (and double-click-like: Enter behavior).
        if let Some(content_rect) = self.content_rect {
            let header_rows: u16 = match self.active_tab {
                2 => 2, // spacer + breadcrumb
                3 => 1,
                _ => return,
            };
            let list_start_y = content_rect.y + header_rows;
            if row >= list_start_y
                && row < content_rect.y + content_rect.height
                && col >= content_rect.x
                && col < content_rect.x + content_rect.width
            {
                let clicked_row = (row - list_start_y) as usize;
                match self.active_tab {
                    2 => {
                        let idx = self.library_state.scroll_offset + clicked_row;
                        if idx < self.library_state.item_count() {
                            self.focus_tabbar = false;
                            if self.library_state.selected == idx {
                                self.handle_enter();
                            } else {
                                self.library_state.selected = idx;
                            }
                        }
                    }
                    3 => {
                        if clicked_row < SettingsState::item_count() {
                            self.focus_tabbar = false;
                            if self.settings_state.selected == clicked_row {
                                self.settings_state.toggle(&mut self.settings);
                                let _ = self.settings.save();
                            } else {
                                self.settings_state.selected = clicked_row;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Route mouse scroll based on cursor position.
    fn handle_mouse_scroll(&mut self, col: u16, row: u16, up: bool) {
        // Scroll on EQ band → adjust gain.
        if self.eq_state.visible {
            if let Some(band) = self.eq_state.band_at(col, row) {
                let delta = if up { 1.0 } else { -1.0 };
                self.eq_state.adjust_band(band, delta);
                self.send_eq_gains();
                return;
            }
        }

        // Scroll on progress bar → seek.
        if let Some(bar_rect) = self.progress_bar_rect {
            if row == bar_rect.y && col >= bar_rect.x && col < bar_rect.x + bar_rect.width {
                if up {
                    self.player.seek_forward(self.settings.seek_step_secs);
                } else {
                    self.player.seek_backward(self.settings.seek_step_secs);
                }
                return;
            }
        }

        // Scroll lists on content area.
        if let Some(content_rect) = self.content_rect {
            if row >= content_rect.y
                && row < content_rect.y + content_rect.height
                && col >= content_rect.x
                && col < content_rect.x + content_rect.width
            {
                // Search/Pomodoro popups.
                if self.search_visible {
                    if up { self.search_state.move_up(); } else { self.search_state.move_down(); }
                    return;
                }
                if self.pomodoro_visible {
                    return;
                }

                match self.active_tab {
                    0 => {
                        // Home tab: scroll within the hovered pane.
                        let in_recent = self.home_state.recent_rect.is_some_and(|r|
                            col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
                        );
                        let in_playlist = self.home_state.playlist_rect.is_some_and(|r|
                            col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
                        );
                        if in_recent {
                            self.home_state.pane = HomePane::RecentlyPlayed;
                            if up {
                                self.home_state.move_up();
                            } else {
                                self.home_state.move_down(
                                    self.recent_tracks.len(),
                                    self.playlists_state.playlists.len(),
                                );
                            }
                        } else if in_playlist {
                            self.home_state.pane = HomePane::Playlists;
                            if up {
                                self.home_state.move_up();
                            } else {
                                self.home_state.move_down(
                                    self.recent_tracks.len(),
                                    self.playlists_state.playlists.len(),
                                );
                            }
                        }
                        return;
                    }
                    2 => {
                        // Library.
                        if up {
                            self.library_state.move_up();
                        } else {
                            self.library_state.move_down();
                        }
                        self.update_library_scroll();
                        return;
                    }
                    3 => {
                        // Settings.
                        if up {
                            self.settings_state.move_up();
                        } else {
                            self.settings_state.move_down(SettingsState::item_count());
                        }
                        return;
                    }
                    _ => {}
                }
            }
        }

        // Scroll on controls line → volume.
        if let Some(ctrl_rect) = self.controls_rect {
            if row == ctrl_rect.y && col >= ctrl_rect.x && col < ctrl_rect.x + ctrl_rect.width {
                if up {
                    self.player.volume_up();
                } else {
                    self.player.volume_down();
                }
            }
        }
    }

    /// Sync selection state from mouse hover position.
    /// Moving the mouse over a list item selects it (same teal paint as keyboard).
    fn handle_mouse_hover(&mut self, col: u16, row: u16) {
        if self.search_visible || self.pomodoro_visible || self.keybindings_visible {
            return;
        }

        // Check if hovering over tab bar.
        if let Some(tab_rect) = self.tab_bar_rect {
            if row == tab_rect.y && col >= tab_rect.x && col < tab_rect.x + tab_rect.width {
                // Don't change focus_tabbar on hover — tabs are clicked, not hovered.
                return;
            }
        }

        match self.active_tab {
            0 => {
                // Home tab: check which pane the mouse is in.
                if let Some(r) = self.home_state.recent_rect {
                    let list_y = r.y + 1; // skip header
                    if row >= list_y && row < r.y + r.height
                        && col >= r.x && col < r.x + r.width
                    {
                        let idx = self.home_state.recent_scroll + (row - list_y) as usize;
                        if idx < self.recent_tracks.len() {
                            self.home_state.pane = HomePane::RecentlyPlayed;
                            self.home_state.recent_selected = idx;
                            self.focus_tabbar = false;
                        }
                        return;
                    }
                }
                if let Some(r) = self.home_state.playlist_rect {
                    let list_y = r.y + 1; // skip header
                    if row >= list_y && row < r.y + r.height
                        && col >= r.x && col < r.x + r.width
                    {
                        let idx = self.home_state.playlist_scroll + (row - list_y) as usize;
                        if idx < self.playlists_state.playlists.len() {
                            self.home_state.pane = HomePane::Playlists;
                            self.home_state.playlist_selected = idx;
                            self.focus_tabbar = false;
                        }
                        return;
                    }
                }
            }
            2 => {
                // Library: map row to list index.
                if let Some(content_rect) = self.content_rect {
                    // Library layout: spacer(1) + breadcrumb(1) + list + hints(1)
                    // The list starts at content_rect.y + 3 (spacer + breadcrumb + list area).
                    // But the actual list area starts after the breadcrumb which is in chunks[2].
                    // The list area starts at content_rect.y + 2 (spacer=1, breadcrumb=1).
                    let list_start_y = content_rect.y + 2;
                    if row >= list_start_y
                        && row < content_rect.y + content_rect.height.saturating_sub(1) // -1 for hints
                        && col >= content_rect.x
                        && col < content_rect.x + content_rect.width
                    {
                        let clicked_row = (row - list_start_y) as usize;
                        let idx = self.library_state.scroll_offset + clicked_row;
                        if idx < self.library_state.item_count() {
                            self.library_state.selected = idx;
                            self.focus_tabbar = false;
                        }
                    }
                }
            }
            3 => {
                // Settings: map row to setting index, accounting for scroll.
                if let Some(content_rect) = self.content_rect {
                    let list_y = content_rect.y + 1; // skip header row
                    let list_h = content_rect.height.saturating_sub(2) as usize; // -header -status
                    if row >= list_y
                        && row < list_y + list_h as u16
                        && col >= content_rect.x
                        && col < content_rect.x + content_rect.width
                    {
                        let scroll = self.settings_state.scroll_offset(list_h);
                        let idx = scroll + (row - list_y) as usize;
                        if idx < SettingsState::item_count() {
                            self.settings_state.selected = idx;
                            self.focus_tabbar = false;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn update_library_scroll(&mut self) {
        let selected = self.library_state.selected;
        let offset = self.library_state.scroll_offset;
        // Use actual content area height minus header row, fallback to 20.
        let visible = self
            .content_rect
            .map(|r| r.height.saturating_sub(1) as usize)
            .unwrap_or(20);

        if selected < offset {
            self.library_state.scroll_offset = selected;
        } else if visible > 0 && selected >= offset + visible {
            self.library_state.scroll_offset = selected - visible + 1;
        }
    }

    /// Send the current EQ gains to the audio engine.
    fn send_eq_gains(&mut self) {
        self.player.send(AudioCommand::SetEq(self.eq_state.gains));
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

    fn process_scan_events(&mut self) {
        let rx = match self.scan_rx.take() {
            Some(rx) => rx,
            None => return,
        };

        let mut done = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                ScanEvent::Started => {
                    self.library_state.library.scanning = true;
                }
                ScanEvent::Progress { found: _ } => {}
                ScanEvent::Complete(library) => {
                    self.library_state.library = library;
                    self.library_state.library.scanning = false;
                    done = true;
                    break;
                }
                ScanEvent::Error(msg) => {
                    tracing::warn!("Scan error: {msg}");
                }
            }
        }

        if !done {
            self.scan_rx = Some(rx);
        }
    }

    /// Build the naming overlay line (shown when naming a playlist).
    fn build_naming_hint(&self) -> Line<'static> {
        let accent = self.settings.theme_colors.accent();
        let dim = self.settings.theme_colors.text_dim();
        let text_color = self.settings.theme_colors.text();
        let highlight = self.settings.theme_colors.highlight();
        Line::from(vec![
            Span::styled(
                " Name: ",
                Style::default().fg(highlight).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                self.playlist_name_buf.clone(),
                Style::default().fg(text_color),
            ),
            Span::styled(
                "\u{2588}",
                Style::default().fg(accent),
            ),
            Span::styled(
                "  Enter:save  Esc:cancel",
                Style::default().fg(dim),
            ),
        ])
    }
}

/// Compute a centered popup rect at the given percentage of terminal size.
fn centered_popup(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let popup_width = (area.width as u32 * width_pct as u32 / 100) as u16;
    let popup_height = (area.height as u32 * height_pct as u32 / 100) as u16;
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    Rect {
        x,
        y,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    }
}

/// Compute a roughly square centered popup (accounting for terminal aspect ratio ~2:1).
fn centered_square_popup(area: Rect) -> Rect {
    let size = (area.height as u16).min(area.width / 2).max(10);
    let popup_height = size.min(area.height);
    let popup_width = (size * 2).min(area.width); // double width for square look
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    }
}

/// Draw a simple border around a popup area.
fn render_popup_border(buf: &mut ratatui::buffer::Buffer, area: Rect, title: &str, color: Color) {
    let style = Style::default().fg(color);
    let dim = Style::default().fg(Color::DarkGray);

    // Top border.
    let top: String = format!("\u{250c}{}\u{2510}", "\u{2500}".repeat((area.width as usize).saturating_sub(2)));
    buf.set_string(area.x, area.y, &top, dim);
    // Title overlay.
    let title_str = format!(" {title} ");
    buf.set_string(area.x + 2, area.y, &title_str, style);
    // Close button [x] at top-right.
    let close_str = "[x]";
    let close_x = area.x + area.width.saturating_sub(close_str.len() as u16 + 1);
    buf.set_string(close_x, area.y, close_str, Style::default().fg(Color::DarkGray));

    // Bottom border.
    let bottom: String = format!("\u{2514}{}\u{2518}", "\u{2500}".repeat((area.width as usize).saturating_sub(2)));
    if area.y + area.height > 0 {
        buf.set_string(area.x, area.y + area.height - 1, &bottom, dim);
    }

    // Side borders.
    for y in (area.y + 1)..(area.y + area.height.saturating_sub(1)) {
        buf.set_string(area.x, y, "\u{2502}", dim);
        if area.width > 1 {
            buf.set_string(area.x + area.width - 1, y, "\u{2502}", dim);
        }
    }
}

/// Render the keybindings reference table inside a popup.
fn render_keybindings_popup(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    settings: &Settings,
    accent: Color,
    text_color: Color,
) {
    use crate::config::settings::BindableAction;

    let dim = Style::default().fg(Color::DarkGray);
    let label_style = Style::default().fg(text_color);
    let key_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);

    // Two-column layout: split bindings into left and right halves.
    let all = BindableAction::ALL;
    let mid = (all.len() + 1) / 2;
    let col_width = (area.width as usize) / 2;

    // Find the longest label to right-align keys neatly.
    let max_label_len = all.iter().map(|a| a.label().len()).max().unwrap_or(0);
    let label_width = (max_label_len + 2).min(col_width.saturating_sub(2)); // +2 for padding

    for (i, &action) in all.iter().enumerate() {
        let keys = settings.keybindings.keys_for(action);
        let keys_str = if keys.is_empty() {
            "(unbound)".to_string()
        } else {
            keys.join(", ")
        };

        let (col_x, row) = if i < mid {
            (area.x, i)
        } else {
            (area.x + col_width as u16 + 1, i - mid)
        };

        let y = area.y + row as u16;
        if y >= area.y + area.height {
            continue;
        }

        let max_w = col_width.saturating_sub(1);
        // Label.
        let label = action.label();
        buf.set_string(col_x + 1, y, &label[..label.len().min(max_w)], label_style);
        // Key binding right after label.
        let key_x = col_x + 1 + label_width as u16;
        if key_x < col_x + max_w as u16 {
            let remaining = (col_x + max_w as u16).saturating_sub(key_x) as usize;
            buf.set_string(key_x, y, &keys_str[..keys_str.len().min(remaining)], key_style);
        }
    }

    // Draw a center divider.
    let div_x = area.x + col_width as u16;
    if div_x < area.x + area.width {
        for y in area.y..area.y + area.height.min(mid as u16) {
            buf.set_string(div_x, y, "\u{2502}", dim);
        }
    }
}

use crate::util::format::format_duration;

/// Render the playlist picker popup content.
fn render_playlist_picker(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    playlists: &[Playlist],
    selected: usize,
    track_count: usize,
    text_color: Color,
) {
    if area.height < 2 {
        return;
    }

    // Header line.
    let header = format!(" Adding {} track(s) to playlist:", track_count);
    buf.set_string(
        area.x,
        area.y,
        &header,
        Style::default().fg(Color::DarkGray),
    );

    if playlists.is_empty() {
        buf.set_string(
            area.x + 2,
            area.y + 2,
            "No playlists found. Create one first.",
            Style::default().fg(Color::DarkGray),
        );
        return;
    }

    let list_y = area.y + 1;
    let max_rows = (area.height.saturating_sub(2)) as usize;

    // Scroll to keep selected visible.
    let scroll = if selected >= max_rows {
        selected - max_rows + 1
    } else {
        0
    };

    for (i, playlist) in playlists.iter().enumerate().skip(scroll).take(max_rows) {
        let y = list_y + (i - scroll) as u16;
        if y >= area.y + area.height {
            break;
        }
        let is_sel = i == selected;
        let prefix = if is_sel { " > " } else { "   " };
        let track_info = format!("  ({} tracks)", playlist.tracks.len());
        let style = if is_sel {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(text_color)
        };
        let info_style = if is_sel {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let name_str = format!("{prefix}{}", playlist.name);
        let name_len = name_str.len() as u16;
        buf.set_string(area.x, y, &name_str, style);
        buf.set_string(area.x + name_len, y, &track_info, info_style);
    }

    // Hint at bottom.
    let hint_y = area.y + area.height.saturating_sub(1);
    buf.set_string(
        area.x + 1,
        hint_y,
        "Enter: add  │  Esc: cancel",
        Style::default().fg(Color::DarkGray),
    );
}

/// Render the recently played popup content.
fn render_recent_popup(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    recent_tracks: &[PathBuf],
    selected: usize,
    text_color: Color,
) {
    if area.height < 2 {
        return;
    }

    if recent_tracks.is_empty() {
        buf.set_string(
            area.x + 2,
            area.y + 1,
            "No recently played tracks.",
            Style::default().fg(Color::DarkGray),
        );
        return;
    }

    let max_rows = (area.height.saturating_sub(1)) as usize;

    let scroll = if selected >= max_rows {
        selected - max_rows + 1
    } else {
        0
    };

    for (i, path) in recent_tracks.iter().enumerate().skip(scroll).take(max_rows) {
        let y = area.y + (i - scroll) as u16;
        if y >= area.y + area.height {
            break;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "?".into());
        let is_sel = i == selected;
        let prefix = if is_sel { " > " } else { "   " };
        let style = if is_sel {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(text_color)
        };
        let display = format!("{prefix}{name}");
        buf.set_string(area.x, y, &display, style);
    }

    // Hint at bottom.
    let hint_y = area.y + area.height.saturating_sub(1);
    buf.set_string(
        area.x + 1,
        hint_y,
        "Enter: play  │  Esc: close",
        Style::default().fg(Color::DarkGray),
    );
}

/// Render the queue popup content.
fn render_queue_popup(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    queue: &crate::playlist::queue::Queue,
    selected: usize,
    text_color: Color,
) {
    if area.height < 2 {
        return;
    }

    let tracks = queue.tracks();
    if tracks.is_empty() {
        buf.set_string(
            area.x + 2,
            area.y + 1,
            "Queue is empty.",
            Style::default().fg(Color::DarkGray),
        );
        return;
    }

    let current_idx = queue.current_index();
    let max_rows = (area.height.saturating_sub(1)) as usize;

    let scroll = if selected >= max_rows {
        selected - max_rows + 1
    } else {
        0
    };

    for (i, path) in tracks.iter().enumerate().skip(scroll).take(max_rows) {
        let y = area.y + (i - scroll) as u16;
        if y >= area.y + area.height {
            break;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "?".into());
        let is_sel = i == selected;
        let is_current = current_idx == Some(i);
        let prefix = if is_current && is_sel {
            " ▶ "
        } else if is_current {
            " ▶ "
        } else if is_sel {
            " > "
        } else {
            "   "
        };
        let style = if is_sel {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if is_current {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(text_color)
        };
        let display = format!("{prefix}{name}");
        buf.set_string(area.x, y, &display, style);
    }

    // Hint at bottom.
    let hint_y = area.y + area.height.saturating_sub(1);
    buf.set_string(
        area.x + 1,
        hint_y,
        "Enter: play  │  Esc: close",
        Style::default().fg(Color::DarkGray),
    );
}

/// Brighten a ratatui Color toward white by `factor` (0.0 = unchanged, 1.0 = white).
fn brighten_color(color: Color, factor: f32) -> Color {
    let (r, g, b) = color_to_rgb(color);
    let boost = (factor * 180.0) as u8;
    Color::Rgb(
        r.saturating_add(boost),
        g.saturating_add(boost),
        b.saturating_add(boost),
    )
}

/// Map a ratatui named Color to approximate RGB values.
fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::White => (200, 200, 200),
        Color::Cyan => (0, 200, 200),
        Color::Yellow => (200, 200, 0),
        Color::Green => (0, 200, 0),
        Color::Red => (200, 0, 0),
        Color::Blue => (0, 0, 200),
        Color::Magenta => (200, 0, 200),
        Color::DarkGray => (100, 100, 100),
        Color::Gray => (150, 150, 150),
        Color::LightCyan => (100, 255, 255),
        Color::LightYellow => (255, 255, 100),
        Color::LightGreen => (100, 255, 100),
        Color::LightRed => (255, 100, 100),
        Color::LightBlue => (100, 100, 255),
        Color::LightMagenta => (255, 100, 255),
        Color::Black | Color::Reset => (0, 0, 0),
        _ => (150, 150, 150),
    }
}
