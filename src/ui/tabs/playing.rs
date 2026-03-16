use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use std::path::PathBuf;
use std::time::Duration;

use crate::ui::widgets::progress_bar::ProgressBar;

/// State for the Now Playing tab.
#[derive(Debug, Clone)]
pub struct PlayingState {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub file_path: Option<PathBuf>,
    pub elapsed: Duration,
    pub duration: Duration,
    pub is_playing: bool,
    pub volume: f32,
    pub sample_rate: u32,
    pub channels: u16,
    pub level: f32,
}

impl Default for PlayingState {
    fn default() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            file_path: None,
            elapsed: Duration::ZERO,
            duration: Duration::ZERO,
            is_playing: false,
            volume: 0.5,
            sample_rate: 0,
            channels: 0,
            level: 0.0,
        }
    }
}

pub struct PlayingTab<'a> {
    pub state: &'a PlayingState,
}

impl<'a> Widget for PlayingTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Now Playing ");

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        let chunks = Layout::vertical([
            Constraint::Length(1), // spacer
            Constraint::Length(1), // title
            Constraint::Length(1), // artist
            Constraint::Length(1), // album
            Constraint::Length(1), // spacer
            Constraint::Length(1), // status line
            Constraint::Length(1), // spacer
            Constraint::Length(1), // progress bar
            Constraint::Length(1), // spacer
            Constraint::Length(1), // volume / format info
            Constraint::Min(0),   // remaining space
        ])
        .split(inner);

        // Title.
        let title = if self.state.title.is_empty() {
            self.state
                .file_path
                .as_ref()
                .and_then(|p| {
                    p.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "No track loaded".into())
        } else {
            self.state.title.clone()
        };
        Paragraph::new(Line::from(vec![Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]))
        .centered()
        .render(chunks[1], buf);

        // Artist.
        if !self.state.artist.is_empty() {
            Paragraph::new(Line::from(vec![Span::styled(
                &self.state.artist,
                Style::default().fg(Color::Yellow),
            )]))
            .centered()
            .render(chunks[2], buf);
        }

        // Album.
        if !self.state.album.is_empty() {
            Paragraph::new(Line::from(vec![Span::styled(
                &self.state.album,
                Style::default().fg(Color::DarkGray),
            )]))
            .centered()
            .render(chunks[3], buf);
        }

        // Status.
        let status_icon = if self.state.is_playing {
            "▶ Playing"
        } else if self.state.file_path.is_some() {
            "⏸ Paused"
        } else {
            "⏹ Stopped"
        };

        // Simple level indicator.
        let level_bars = (self.state.level * 20.0).round() as usize;
        let level_str: String = "▮".repeat(level_bars.min(20));
        let empty_str: String = "▯".repeat(20_usize.saturating_sub(level_bars));

        let status_line = Line::from(vec![
            Span::styled(status_icon, Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled(level_str, Style::default().fg(Color::Cyan)),
            Span::styled(empty_str, Style::default().fg(Color::DarkGray)),
        ]);
        Paragraph::new(status_line)
            .centered()
            .render(chunks[5], buf);

        // Progress bar.
        ProgressBar::default()
            .elapsed(self.state.elapsed)
            .total(self.state.duration)
            .render(chunks[7], buf);

        // Volume / format info.
        let vol_pct = (self.state.volume * 100.0) as u8;
        let format_info = if self.state.sample_rate > 0 {
            format!(
                "Vol: {}%  │  {}Hz {}ch",
                vol_pct, self.state.sample_rate, self.state.channels
            )
        } else {
            format!("Vol: {}%", vol_pct)
        };
        Paragraph::new(Line::from(Span::styled(
            format_info,
            Style::default().fg(Color::DarkGray),
        )))
        .centered()
        .render(chunks[9], buf);
    }
}
