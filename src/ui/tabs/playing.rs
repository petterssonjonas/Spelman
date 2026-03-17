use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use std::path::PathBuf;
use std::time::Duration;

use crate::playlist::queue::Queue;
use crate::ui::albumart::AlbumArt;
use crate::ui::widgets::progress_bar::ProgressBar;
use crate::ui::widgets::visualizer::Visualizer;

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
    /// Smoothed spectrum bars for visualizer (0.0-1.0 each).
    pub spectrum: Vec<f32>,
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
            spectrum: Vec::new(),
        }
    }
}

impl PlayingState {
    /// Smooth incoming spectrum data with exponential moving average.
    pub fn update_spectrum(&mut self, raw: &[f32]) {
        if self.spectrum.len() != raw.len() {
            self.spectrum = raw.to_vec();
            return;
        }
        let smoothing = 0.35;
        for (s, &r) in self.spectrum.iter_mut().zip(raw.iter()) {
            *s = *s * smoothing + r * (1.0 - smoothing);
        }
    }
}

pub struct PlayingTab<'a> {
    pub state: &'a PlayingState,
    pub queue: &'a Queue,
    pub album_art: &'a AlbumArt,
}

impl<'a> Widget for PlayingTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = area;

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        // If we have album art, split into left (art) and right (info).
        let (art_area, info_area) = if self.album_art.has_art && inner.width > 50 {
            let cols = Layout::horizontal([
                Constraint::Length(32), // art
                Constraint::Min(0),    // info
            ])
            .split(inner);
            (Some(cols[0]), cols[1])
        } else {
            (None, inner)
        };

        // Render ASCII album art if available.
        if let Some(art_rect) = art_area {
            for (i, line) in self.album_art.ascii_lines.iter().enumerate() {
                let y = art_rect.y + i as u16;
                if y >= art_rect.y + art_rect.height {
                    break;
                }
                let display: String = line
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\u{2580}')
                    .take(art_rect.width as usize)
                    .collect();
                buf.set_string(
                    art_rect.x,
                    y,
                    &display,
                    Style::default().fg(Color::DarkGray),
                );
            }
        }

        let chunks = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // artist
            Constraint::Length(1), // album
            Constraint::Length(1), // status + format info
            Constraint::Length(1), // progress bar
            Constraint::Length(1), // spacer
            Constraint::Min(4),   // visualizer
            Constraint::Length(1), // spacer
            Constraint::Min(0),   // queue
        ])
        .split(info_area);

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
        .render(chunks[0], buf);

        // Artist.
        if !self.state.artist.is_empty() {
            Paragraph::new(Line::from(vec![Span::styled(
                &self.state.artist,
                Style::default().fg(Color::Yellow),
            )]))
            .centered()
            .render(chunks[1], buf);
        }

        // Album.
        if !self.state.album.is_empty() {
            Paragraph::new(Line::from(vec![Span::styled(
                &self.state.album,
                Style::default().fg(Color::DarkGray),
            )]))
            .centered()
            .render(chunks[2], buf);
        }

        // Status + volume/format info combined.
        let status_icon = if self.state.is_playing {
            "▶ Playing"
        } else if self.state.file_path.is_some() {
            "⏸ Paused"
        } else {
            "⏹ Stopped"
        };

        let vol_pct = (self.state.volume * 100.0) as u8;
        let format_info = if self.state.sample_rate > 0 {
            format!(
                "│  Vol: {}%  │  {}Hz {}ch",
                vol_pct, self.state.sample_rate, self.state.channels
            )
        } else {
            format!("│  Vol: {}%", vol_pct)
        };

        let status_line = Line::from(vec![
            Span::styled(status_icon, Style::default().fg(Color::Green)),
            Span::styled(format!("  {format_info}"), Style::default().fg(Color::DarkGray)),
        ]);
        Paragraph::new(status_line)
            .centered()
            .render(chunks[3], buf);

        // Progress bar.
        ProgressBar::default()
            .elapsed(self.state.elapsed)
            .total(self.state.duration)
            .render(chunks[4], buf);

        // Spectrum visualizer.
        if !self.state.spectrum.is_empty() {
            Visualizer {
                spectrum: &self.state.spectrum,
            }
            .render(chunks[6], buf);
        }

        // Queue display.
        if !self.queue.is_empty() && chunks[8].height >= 2 {
            let queue_area = chunks[8];
            let queue_header = Line::from(vec![Span::styled(
                format!(" Queue ({} tracks) ", self.queue.len()),
                Style::default().fg(Color::DarkGray),
            )]);
            buf.set_line(queue_area.x, queue_area.y, &queue_header, queue_area.width);

            let current_idx = self.queue.current_index();
            let tracks = self.queue.tracks();
            let max_rows = (queue_area.height as usize).saturating_sub(1);

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

                let line = Line::from(Span::styled(format!("{prefix}{name}"), style));
                let y = queue_area.y + 1 + row as u16;
                if y < queue_area.y + queue_area.height {
                    buf.set_line(
                        queue_area.x + 1,
                        y,
                        &line,
                        queue_area.width.saturating_sub(2),
                    );
                }
            }
        }
    }
}
