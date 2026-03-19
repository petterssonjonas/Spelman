use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Widget};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::UnicodeWidthStr;

use crate::config::settings::ThemeColors;
use crate::playlist::playlist::Playlist;

// ── ASCII art banners ───────────────────────────────────────────────────────

/// Parse banners from the embedded ASCIIART.md file.
fn parse_banners() -> Vec<Vec<&'static str>> {
    let raw = include_str!("../../../ASCIIART.md");
    let mut banners = Vec::new();
    let mut current_lines: Vec<&str> = Vec::new();
    let mut in_art = false;

    for line in raw.lines() {
        if line.starts_with("---") {
            if !current_lines.is_empty() {
                while current_lines.last().is_some_and(|l| l.trim().is_empty()) {
                    current_lines.pop();
                }
                if !current_lines.is_empty() {
                    banners.push(std::mem::take(&mut current_lines));
                }
            }
            in_art = false;
            continue;
        }

        if !in_art {
            if line.trim().is_empty()
                || line.trim().ends_with(':')
                || line.starts_with("Collection")
            {
                continue;
            }
            in_art = true;
        }

        if in_art {
            current_lines.push(line);
        }
    }

    // Handle last section.
    while current_lines.last().is_some_and(|l| l.trim().is_empty()) {
        current_lines.pop();
    }
    if !current_lines.is_empty() {
        banners.push(current_lines);
    }

    banners
}

/// Get the parsed banners (computed once, cached forever).
pub fn banners() -> &'static [Vec<&'static str>] {
    static BANNERS: OnceLock<Vec<Vec<&'static str>>> = OnceLock::new();
    BANNERS.get_or_init(parse_banners)
}

/// Pick a pseudo-random banner index using system time.
fn random_banner_index() -> usize {
    let count = banners().len();
    if count == 0 {
        return 0;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize % count)
        .unwrap_or(0)
}

// ── Home tab state ──────────────────────────────────────────────────────────

/// Which pane is focused on the Home tab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HomePane {
    RecentlyPlayed,
    Playlists,
}

/// State for the Home tab.
#[derive(Debug, Clone)]
pub struct HomeState {
    pub pane: HomePane,
    pub recent_selected: usize,
    pub playlist_selected: usize,
    pub recent_scroll: usize,
    pub playlist_scroll: usize,
    /// Stored rects for mouse hit-testing (set during render).
    pub recent_rect: Option<Rect>,
    pub playlist_rect: Option<Rect>,
    /// Index into the banners array for the current session's logo.
    pub logo_index: usize,
}

impl Default for HomeState {
    fn default() -> Self {
        Self {
            pane: HomePane::RecentlyPlayed,
            recent_selected: 0,
            playlist_selected: 0,
            recent_scroll: 0,
            playlist_scroll: 0,
            recent_rect: None,
            playlist_rect: None,
            logo_index: random_banner_index(),
        }
    }
}

impl HomeState {
    /// Pick a new random banner.
    pub fn randomize_logo(&mut self) {
        self.logo_index = random_banner_index();
    }

    pub fn move_down(&mut self, recent_count: usize, playlist_count: usize) {
        match self.pane {
            HomePane::RecentlyPlayed => {
                if recent_count > 0 && self.recent_selected < recent_count - 1 {
                    self.recent_selected += 1;
                }
            }
            HomePane::Playlists => {
                if playlist_count > 0 && self.playlist_selected < playlist_count - 1 {
                    self.playlist_selected += 1;
                }
            }
        }
    }

    /// Returns true if the cursor was already at position 0 (couldn't move up).
    pub fn move_up(&mut self) -> bool {
        match self.pane {
            HomePane::RecentlyPlayed => {
                if self.recent_selected > 0 {
                    self.recent_selected -= 1;
                    false
                } else {
                    true // at top
                }
            }
            HomePane::Playlists => {
                if self.playlist_selected > 0 {
                    self.playlist_selected -= 1;
                    false
                } else {
                    true // at top
                }
            }
        }
    }

    pub fn switch_pane(&mut self) {
        self.pane = match self.pane {
            HomePane::RecentlyPlayed => HomePane::Playlists,
            HomePane::Playlists => HomePane::RecentlyPlayed,
        };
    }

    /// Get the selected recent track path.
    pub fn selected_recent_path<'a>(&self, recent: &'a [PathBuf]) -> Option<&'a PathBuf> {
        recent.get(self.recent_selected)
    }

    /// Get the selected playlist.
    pub fn selected_playlist<'a>(&self, playlists: &'a [Playlist]) -> Option<&'a Playlist> {
        playlists.get(self.playlist_selected)
    }
}

// ── Home tab widget ─────────────────────────────────────────────────────────

pub struct HomeTab<'a> {
    pub state: &'a HomeState,
    pub recent_tracks: &'a [PathBuf],
    pub playlists: &'a [Playlist],
    pub theme: &'a ThemeColors,
    /// The key string for the keybindings popup (e.g. "K").
    pub keybindings_key: &'a str,
    /// When true, focus is on the tab bar — no content item is painted as selected.
    pub focus_tabbar: bool,
}

/// Height of the banner area (1 spacer above + art + 1 spacer below), or 0 if not enough room.
pub fn logo_height(area_height: u16, logo_index: usize) -> u16 {
    let bs = banners();
    if area_height >= 14 && !bs.is_empty() {
        let idx = logo_index.min(bs.len() - 1);
        bs[idx].len() as u16 + 2 // +1 spacer top, +1 spacer bottom
    } else {
        0
    }
}

impl<'a> Widget for HomeTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 20 {
            return;
        }

        let accent = self.theme.accent();
        let text_color = self.theme.text();
        let dim = self.theme.text_dim();
        let selection = self.theme.selection();

        let bs = banners();
        let lh = logo_height(area.height, self.state.logo_index);

        let rows = Layout::vertical([
            Constraint::Length(lh),  // spacer + logo + spacer
            Constraint::Min(4),     // two-column content
            Constraint::Length(1),  // keybindings hint
        ])
        .split(area);

        // Render ASCII logo centered (with 1-row top spacing from tabs).
        if lh > 0 && !bs.is_empty() {
            let idx = self.state.logo_index.min(bs.len() - 1);
            let logo = &bs[idx];
            let logo_width = logo.iter().map(|l| UnicodeWidthStr::width(*l)).max().unwrap_or(0) as u16;
            let x_offset = rows[0].x + (rows[0].width.saturating_sub(logo_width)) / 2;
            for (i, line) in logo.iter().enumerate() {
                let y = rows[0].y + 1 + i as u16; // +1 for top spacer
                if y < rows[0].y + rows[0].height {
                    buf.set_string(x_offset, y, line, Style::default().fg(accent));
                }
            }
        }

        // Keybindings hint at bottom, centered.
        let hint_text = format!("Keybindings reference: {}", self.keybindings_key);
        Paragraph::new(Line::from(Span::styled(hint_text, Style::default().fg(dim))))
            .centered()
            .render(rows[2], buf);

        // Two columns with a 1-char separator gap.
        let left_width = (area.width.saturating_sub(1)) / 2;
        let right_width = area.width.saturating_sub(left_width + 1);
        let left_area = Rect {
            x: rows[1].x,
            y: rows[1].y,
            width: left_width,
            height: rows[1].height,
        };
        let right_area = Rect {
            x: rows[1].x + left_width + 1,
            y: rows[1].y,
            width: right_width,
            height: rows[1].height,
        };

        // Draw vertical separator.
        let sep_x = rows[1].x + left_width;
        let sep_style = Style::default().fg(dim);
        for y in rows[1].y..rows[1].y + rows[1].height {
            buf.set_string(sep_x, y, "\u{2502}", sep_style);
        }

        self.render_recent_pane(left_area, buf, accent, text_color, dim, selection);
        self.render_playlists_pane(right_area, buf, accent, text_color, dim, selection);
    }
}

impl<'a> HomeTab<'a> {
    fn render_recent_pane(
        &self,
        area: Rect,
        buf: &mut Buffer,
        accent: Color,
        text_color: Color,
        dim: Color,
        selection: Color,
    ) {
        if area.height < 2 {
            return;
        }

        let is_active = !self.focus_tabbar && self.state.pane == HomePane::RecentlyPlayed;
        let header_style = if is_active {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim)
        };

        let chunks = Layout::vertical([
            Constraint::Length(1), // header
            Constraint::Min(0),   // list
        ])
        .split(area);

        Paragraph::new(Line::from(Span::styled(
            " Recently Played",
            header_style,
        )))
        .render(chunks[0], buf);

        if self.recent_tracks.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "  No history yet",
                Style::default().fg(dim),
            )))
            .render(chunks[1], buf);
            return;
        }

        let visible_height = chunks[1].height as usize;
        let scroll = self.state.recent_scroll;

        let items: Vec<ListItem> = self
            .recent_tracks
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible_height)
            .map(|(i, path)| {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "?".into());
                let style = if is_active && i == self.state.recent_selected {
                    Style::default()
                        .fg(selection)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(text_color)
                };
                ListItem::new(Line::from(Span::styled(format!("  {name}"), style)))
            })
            .collect();

        List::new(items).render(chunks[1], buf);
    }

    fn render_playlists_pane(
        &self,
        area: Rect,
        buf: &mut Buffer,
        accent: Color,
        text_color: Color,
        dim: Color,
        selection: Color,
    ) {
        if area.height < 2 {
            return;
        }

        let is_active = !self.focus_tabbar && self.state.pane == HomePane::Playlists;
        let header_style = if is_active {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim)
        };

        let chunks = Layout::vertical([
            Constraint::Length(1), // header
            Constraint::Min(0),   // list
        ])
        .split(area);

        Paragraph::new(Line::from(Span::styled(
            " Saved Playlists",
            header_style,
        )))
        .render(chunks[0], buf);

        if self.playlists.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "  No playlists saved",
                Style::default().fg(dim),
            )))
            .render(chunks[1], buf);
            return;
        }

        let visible_height = chunks[1].height as usize;
        let scroll = self.state.playlist_scroll;

        let items: Vec<ListItem> = self
            .playlists
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible_height)
            .map(|(i, pl)| {
                let style = if is_active && i == self.state.playlist_selected {
                    Style::default()
                        .fg(selection)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(text_color)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {}", pl.name), style),
                    Span::styled(
                        format!("  ({} tracks)", pl.tracks.len()),
                        Style::default().fg(dim),
                    ),
                ]))
            })
            .collect();

        List::new(items).render(chunks[1], buf);
    }
}
