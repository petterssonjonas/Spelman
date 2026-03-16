use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use std::path::PathBuf;
use std::time::Duration;

use crate::playlist::queue::Queue;
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
    pub queue: &'a Queue,
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
            Constraint::Length(1), // spacer
            Constraint::Min(0),   // queue
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

        // Queue display.
        if !self.queue.is_empty() && chunks[11].height >= 2 {
            let queue_area = chunks[11];
            let queue_header = Line::from(vec![
                Span::styled(
                    format!(" Queue ({} tracks) ", self.queue.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            buf.set_line(queue_area.x, queue_area.y, &queue_header, queue_area.width);

            let current_idx = self.queue.current_index();
            let tracks = self.queue.tracks();
            let max_rows = (queue_area.height as usize).saturating_sub(1);

            // Show tracks around the current one.
            let start = current_idx
                .map(|i| i.saturating_sub(max_rows / 2))
                .unwrap_or(0);

            for (row, idx) in (start..tracks.len()).enumerate() {
                if row >= max_rows {
                    break;
                }
                let path = &tracks[idx];
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "?".into());

                let is_current = current_idx == Some(idx);
                let prefix = if is_current { "▶ " } else { "  " };
                let style = if is_current {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let line = Line::from(Span::styled(
                    format!("{prefix}{name}"),
                    style,
                ));
                let y = queue_area.y + 1 + row as u16;
                if y < queue_area.y + queue_area.height {
                    buf.set_line(queue_area.x + 1, y, &line, queue_area.width.saturating_sub(2));
                }
            }
        }
    }
}
