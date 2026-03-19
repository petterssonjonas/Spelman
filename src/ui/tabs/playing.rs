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

/// Playback state — replaces ambiguous `is_playing: bool` + `file_path: Option`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    /// No track loaded or playback finished.
    Stopped,
    /// Actively playing audio.
    Playing,
    /// Playback paused — can resume.
    Paused,
}

/// State for the Now Playing tab.
#[derive(Debug, Clone)]
pub struct PlayingState {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub file_path: Option<PathBuf>,
    pub elapsed: Duration,
    pub duration: Duration,
    pub playback: PlaybackState,
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
            playback: PlaybackState::Stopped,
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
        if area.height < 4 || area.width < 20 {
            return;
        }

        // Empty state: show simple message.
        if self.state.file_path.is_none() && self.queue.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "No track loaded — select from Home tab or Library",
                Style::default().fg(Color::DarkGray),
            )))
            .centered()
            .render(area, buf);
            return;
        }

        // Compute album art display size.
        let art_source_rows = self.album_art.cells.len();
        let art_source_cols = self.album_art.cells.first().map_or(0, |r| r.len());
        let art_rows = compute_art_rows(self.album_art.has_art, art_source_rows, area.height);

        // Layout top-to-bottom:
        //   album art | track info (3 lines) | controls (1) | seek bar (1) | spacer | visualizer | spacer | queue
        let chunks = Layout::vertical([
            Constraint::Length(art_rows),  // album art
            Constraint::Length(1),         // title
            Constraint::Length(1),         // artist
            Constraint::Length(1),         // album
            Constraint::Length(1),         // controls: play/pause + volume + format
            Constraint::Length(1),         // seek / progress bar
            Constraint::Length(1),         // spacer
            Constraint::Min(4),           // visualizer (fills remaining, capped below)
            Constraint::Length(1),         // spacer
            Constraint::Min(0),           // queue
        ])
        .split(area);

        // --- Album art (top, centered, scaled to fit) ---
        if art_rows > 0 {
            let display_rows = art_rows as usize;
            // Scale the art width proportionally to the height.
            let scale = display_rows as f32 / art_source_rows as f32;
            let display_cols = ((art_source_cols as f32) * scale).round() as usize;
            let display_cols = display_cols.min(chunks[0].width as usize);

            let art_x = chunks[0].x + (chunks[0].width.saturating_sub(display_cols as u16)) / 2;

            for row_idx in 0..display_rows {
                let y = chunks[0].y + row_idx as u16;
                if y >= chunks[0].y + chunks[0].height {
                    break;
                }
                // Map display row back to source row.
                let src_row = (row_idx as f32 / scale).round() as usize;
                let src_row = src_row.min(art_source_rows.saturating_sub(1));
                let src = &self.album_art.cells[src_row];

                for col_idx in 0..display_cols {
                    let x = art_x + col_idx as u16;
                    if x >= chunks[0].x + chunks[0].width {
                        break;
                    }
                    // Map display col back to source col.
                    let src_col = (col_idx as f32 / scale).round() as usize;
                    let src_col = src_col.min(art_source_cols.saturating_sub(1));
                    let cell = &src[src_col];
                    buf.set_string(
                        x,
                        y,
                        "\u{2580}",
                        Style::default().fg(cell.fg).bg(cell.bg),
                    );
                }
            }
        }

        // --- Title ---
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
        Paragraph::new(Line::from(Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )))
        .centered()
        .render(chunks[1], buf);

        // --- Artist ---
        if !self.state.artist.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                &self.state.artist,
                Style::default().fg(Color::Yellow),
            )))
            .centered()
            .render(chunks[2], buf);
        }

        // --- Album ---
        if !self.state.album.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                &self.state.album,
                Style::default().fg(Color::DarkGray),
            )))
            .centered()
            .render(chunks[3], buf);
        }

        // --- Controls: play/pause + volume blocks + format info ---
        let status_icon = match self.state.playback {
            PlaybackState::Playing => "▶ Playing",
            PlaybackState::Paused => "⏸ Paused",
            PlaybackState::Stopped => "⏹ Stopped",
        };
        let vol_pct = (self.state.volume * 100.0).round() as u8;

        // Volume bar: 10 blocks, colored from green→yellow→red as level increases.
        let vol_blocks = 10;
        let filled_blocks = ((self.state.volume * vol_blocks as f32).round() as usize).min(vol_blocks);
        let mut vol_spans: Vec<Span> = Vec::with_capacity(vol_blocks + 4);
        vol_spans.push(Span::styled("  │  Vol: ", Style::default().fg(Color::DarkGray)));
        for i in 0..vol_blocks {
            let block_color = match i {
                0..=5 => Color::Green,
                6..=7 => Color::Yellow,
                _ => Color::Red,
            };
            if i < filled_blocks {
                vol_spans.push(Span::styled("█", Style::default().fg(block_color)));
            } else {
                vol_spans.push(Span::styled("░", Style::default().fg(Color::DarkGray)));
            }
        }
        vol_spans.push(Span::styled(format!(" {}%", vol_pct), Style::default().fg(Color::DarkGray)));

        let format_suffix = if self.state.sample_rate > 0 {
            format!(
                "  │  {}Hz {}ch",
                self.state.sample_rate, self.state.channels
            )
        } else {
            String::new()
        };
        vol_spans.push(Span::styled(format_suffix, Style::default().fg(Color::DarkGray)));

        let mut spans = vec![
            Span::styled(status_icon, Style::default().fg(Color::Green)),
        ];
        spans.extend(vol_spans);
        Paragraph::new(Line::from(spans))
            .centered()
            .render(chunks[4], buf);

        // --- Seek / progress bar ---
        ProgressBar::default()
            .elapsed(self.state.elapsed)
            .total(self.state.duration)
            .render(chunks[5], buf);

        // --- Spectrum visualizer (80% width centered, capped at 50 rows) ---
        if !self.state.spectrum.is_empty() {
            let viz_area = chunks[7];
            let viz_inner_w = ((viz_area.width as f64) * 0.8) as u16;
            let viz_inner_w = viz_inner_w.max(20);
            let viz_x_off = (viz_area.width.saturating_sub(viz_inner_w)) / 2;
            let capped = Rect {
                x: viz_area.x + viz_x_off,
                width: viz_inner_w,
                height: viz_area.height.min(50),
                ..viz_area
            };
            Visualizer {
                spectrum: &self.state.spectrum,
            }
            .render(capped, buf);
        }

        // --- Queue display ---
        if !self.queue.is_empty() && chunks[9].height >= 2 {
            let queue_area = chunks[9];
            let queue_header = Line::from(Span::styled(
                format!(" Queue ({} tracks) ", self.queue.len()),
                Style::default().fg(Color::DarkGray),
            ));
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

/// Compute album art display rows — shared between render and mouse-rect code.
/// Fixed items below art: title(1) + artist(1) + album(1) + controls(1) + seek(1)
/// + spacer(1) + min visualizer(4) + spacer(1) = 11 rows minimum.
pub fn compute_art_rows(has_art: bool, source_rows: usize, area_height: u16) -> u16 {
    let max_art_rows = area_height.saturating_sub(11) as usize;
    if has_art && max_art_rows >= 4 && source_rows > 0 {
        source_rows.min(max_art_rows).min(18) as u16
    } else {
        0
    }
}

/// Compute the centered album art x-range for mouse hit-testing.
pub fn compute_art_rect(
    has_art: bool,
    art_source_rows: usize,
    art_source_cols: usize,
    content_x: u16,
    content_y: u16,
    content_width: u16,
    art_rows: u16,
) -> Option<ratatui::layout::Rect> {
    if !has_art || art_rows == 0 || art_source_rows == 0 {
        return None;
    }
    let display_rows = art_rows as usize;
    let scale = display_rows as f32 / art_source_rows as f32;
    let display_cols = ((art_source_cols as f32) * scale).round() as usize;
    let display_cols = display_cols.min(content_width as usize);
    let art_x = content_x + (content_width.saturating_sub(display_cols as u16)) / 2;
    Some(ratatui::layout::Rect {
        x: art_x,
        y: content_y,
        width: display_cols as u16,
        height: art_rows,
    })
}

