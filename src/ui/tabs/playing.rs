use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use std::path::PathBuf;
use std::time::Duration;

use crate::lyrics::Lyrics;
use crate::playlist::queue::Queue;
use crate::ui::albumart::AlbumArt;
use crate::ui::widgets::progress_bar::{ProgressBar, bar_geometry};
use crate::ui::widgets::visualizer::{BarStyle, Oscilloscope, VizMode, Visualizer};
use crate::ui::widgets::waveform::{Waveform, WaveformData, WaveformMode, WaveformOscilloscope};

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
    /// Resolved lyrics for the current track.
    pub lyrics: Option<Lyrics>,
    /// Whether lyrics display is currently active.
    pub show_lyrics: bool,
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
            lyrics: None,
            show_lyrics: false,
        }
    }
}

impl PlayingState {
    /// Store raw spectrum data (smoothing is handled by VisualizerState).
    pub fn update_spectrum(&mut self, raw: &[f32]) {
        if self.spectrum.len() != raw.len() {
            self.spectrum = raw.to_vec();
        } else {
            self.spectrum.copy_from_slice(raw);
        }
    }
}

pub struct PlayingTab<'a> {
    pub state: &'a PlayingState,
    pub queue: &'a Queue,
    pub album_art: &'a AlbumArt,
    pub waveform: Option<&'a WaveformData>,
    pub seekbar_width: f64,
    /// Pre-processed spectrum (through Cava-style smoothing).
    pub processed_spectrum: &'a [f32],
    /// Bar rendering style.
    pub bar_style: BarStyle,
    /// Number of viz bars to render (12–64).
    pub viz_bars: usize,
    /// Gap in columns between bars (0 = joined).
    pub viz_gap: usize,
    /// Show Hz frequency labels below the viz (debug).
    pub show_hz_labels: bool,
    /// Visualizer display mode (Bars or Oscilloscope).
    pub viz_mode: VizMode,
    /// Waveform display mode (Classic or Oscilloscope).
    pub waveform_mode: WaveformMode,
    /// Maximum album art rows (reduced when EQ needs space).
    pub max_art_rows: Option<u16>,
    /// Whether to show lyrics instead of album art.
    pub show_lyrics: bool,
    /// Resolved lyrics for the current track.
    pub lyrics: Option<&'a Lyrics>,
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

        // Compute album art / lyrics display size.
        let show_lyrics = self.show_lyrics && self.lyrics.is_some();
        let art_source_rows = self.album_art.cells.len();
        let art_source_cols = self.album_art.cells.first().map_or(0, |r| r.len());
        let mut art_rows = if show_lyrics {
            // Lyrics get the same space allocation as art would.
            // Use at least 8 rows for lyrics readability.
            let max_rows = area.height.saturating_sub(9) as usize;
            max_rows.min(18).max(8) as u16
        } else {
            compute_art_rows(self.album_art.has_art, art_source_rows, area.height)
        };
        // Respect max_art_rows override (e.g., when EQ needs space).
        if let Some(max) = self.max_art_rows {
            art_rows = art_rows.min(max);
        }

        // Waveform rows below the seek bar.
        let has_waveform = self.waveform.is_some();
        let wave_rows: u16 = if has_waveform { 3 } else { 0 };

        // Layout top-to-bottom:
        //   album art | title | artist | album | controls | seek bar | waveform | visualizer | tail
        let chunks = Layout::vertical([
            Constraint::Length(art_rows),   // 0: album art
            Constraint::Length(1),          // 1: title
            Constraint::Length(1),          // 2: artist
            Constraint::Length(1),          // 3: album
            Constraint::Length(1),          // 4: controls
            Constraint::Length(1),          // 5: seek / progress bar
            Constraint::Length(wave_rows),  // 6: waveform (braille, below seek bar)
            Constraint::Min(0),            // 7: visualizer (fills remaining, capped to 6 rows)
        ])
        .split(area);

        // --- Album art or lyrics (top area) ---
        if art_rows > 0 {
            if show_lyrics {
                // Render lyrics in the art area.
                if let Some(lyrics) = self.lyrics {
                    render_lyrics(buf, chunks[0], lyrics, self.state.elapsed);
                }
            } else {
                // Half-block album art rendering (also serves as fallback under Kitty/iTerm2 images).
                let display_rows = art_rows as usize;
                let scale = display_rows as f32 / art_source_rows as f32;
                let display_cols = ((art_source_cols as f32) * scale).round() as usize;
                let display_cols = display_cols.min(chunks[0].width as usize);

                let art_x =
                    chunks[0].x + (chunks[0].width.saturating_sub(display_cols as u16)) / 2;

                for row_idx in 0..display_rows {
                    let y = chunks[0].y + row_idx as u16;
                    if y >= chunks[0].y + chunks[0].height {
                        break;
                    }
                    let src_row = (row_idx as f32 / scale).round() as usize;
                    let src_row = src_row.min(art_source_rows.saturating_sub(1));
                    let src = &self.album_art.cells[src_row];

                    for col_idx in 0..display_cols {
                        let x = art_x + col_idx as u16;
                        if x >= chunks[0].x + chunks[0].width {
                            break;
                        }
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
            .width_fraction(self.seekbar_width)
            .render(chunks[5], buf);

        // --- Waveform (braille dots below seek bar, aligned to bar-only region) ---
        if let Some(waveform) = self.waveform {
            let fraction = if self.state.duration.as_secs_f64() > 0.0 {
                (self.state.elapsed.as_secs_f64() / self.state.duration.as_secs_f64()).clamp(0.0, 1.0)
            } else {
                0.0
            };

            // Align waveform to the exact bar region (excluding time labels).
            if let Some((bar_x, bar_w)) = bar_geometry(chunks[5], self.state.elapsed, self.state.duration, self.seekbar_width) {
                let waveform_rect = Rect {
                    x: bar_x,
                    y: chunks[6].y,
                    width: bar_w,
                    height: wave_rows,
                };

                match self.waveform_mode {
                    WaveformMode::Classic => {
                        Waveform {
                            peaks: &waveform.peaks,
                            fraction,
                            bar_style: self.bar_style,
                        }
                        .render(waveform_rect, buf);
                    }
                    WaveformMode::Oscilloscope => {
                        WaveformOscilloscope {
                            peaks: &waveform.peaks,
                            fraction,
                            bar_style: self.bar_style,
                        }
                        .render(waveform_rect, buf);
                    }
                }
            }
        }

        // --- Spectrum visualizer (aligned to seekbar, max 6 rows) ---
        if !self.processed_spectrum.is_empty() {
            let viz_area = chunks[7];
            if viz_area.height > 0 {
                // Align viz to the exact seekbar region (excluding time labels).
                if let Some((bar_x, bar_w)) = bar_geometry(
                    chunks[5],
                    self.state.elapsed,
                    self.state.duration,
                    self.seekbar_width,
                ) {
                    let capped = Rect {
                        x: bar_x,
                        width: bar_w,
                        height: viz_area.height.min(6),
                        ..viz_area
                    };
                    match self.viz_mode {
                        VizMode::Bars => {
                            Visualizer {
                                spectrum: self.processed_spectrum,
                                bar_style: self.bar_style,
                                viz_bars: self.viz_bars,
                                viz_gap: self.viz_gap,
                            }
                            .render(capped, buf);
                        }
                        VizMode::Oscilloscope => {
                            Oscilloscope {
                                spectrum: self.processed_spectrum,
                                viz_bars: self.viz_bars,
                                viz_gap: self.viz_gap,
                                bar_style: self.bar_style,
                            }
                            .render(capped, buf);
                        }
                    }

                    // Hz frequency labels below the viz.
                    if self.show_hz_labels {
                        let label_y = capped.y + capped.height;
                        if label_y < area.y + area.height {
                            render_hz_labels(
                                buf, bar_x, bar_w, label_y,
                                self.viz_bars, self.viz_gap,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Render adaptive Hz frequency labels aligned to the visualizer's bar layout.
/// Uses "nice" frequencies (50, 100, 250, 500, 1k, 2k, 4k, 8k, 16k) placed
/// on the same inner span as the drawn bars, with adaptive density based on
/// available width and overlap avoidance.
fn render_hz_labels(
    buf: &mut Buffer,
    x: u16,
    width: u16,
    y: u16,
    viz_bars: usize,
    viz_gap: usize,
) {
    use crate::ui::widgets::visualizer::BarLayout;

    let layout = match BarLayout::compute(width, viz_bars, viz_gap) {
        Some(l) => l,
        None => return,
    };

    const MIN_FREQ: f64 = 50.0;
    const MAX_FREQ: f64 = 16_000.0;
    let log_ratio = (MAX_FREQ / MIN_FREQ).ln();

    // Nice candidate frequencies.
    const CANDIDATES: &[f64] = &[
        50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0,
    ];

    let style = Style::default().fg(Color::DarkGray);

    // Inner span: the actual drawn bar region.
    let inner_left = x + layout.left_pad as u16;
    let inner_w = layout.actual_width as u16;
    if inner_w < 8 {
        return;
    }

    // Build labels with pixel positions, then drop overlapping ones.
    struct Tick {
        label: String,
        cx: u16, // center x in absolute coords
    }

    let mut ticks: Vec<Tick> = Vec::new();
    for &freq in CANDIDATES {
        if freq < MIN_FREQ || freq > MAX_FREQ {
            continue;
        }
        let frac = ((freq / MIN_FREQ).ln() / log_ratio).clamp(0.0, 1.0);
        let px = inner_left as f64 + frac * (inner_w.saturating_sub(1)) as f64;

        let label = format_freq(freq);
        ticks.push(Tick { label, cx: px.round() as u16 });
    }

    // Place labels, skipping any that would overlap the previous one.
    let right_edge = inner_left + inner_w;
    let mut last_end: u16 = 0; // rightmost column used by previous label + 1
    for tick in &ticks {
        let half = tick.label.len() as u16 / 2;
        let mut lx = tick.cx.saturating_sub(half);
        // Clamp to inner bounds.
        lx = lx.max(inner_left);
        lx = lx.min(right_edge.saturating_sub(tick.label.len() as u16));
        if lx < inner_left {
            continue;
        }
        // Skip if it would overlap the previous label (need ≥1 col gap).
        if last_end > 0 && lx < last_end + 1 {
            continue;
        }
        if lx + tick.label.len() as u16 <= right_edge {
            buf.set_string(lx, y, &tick.label, style);
            last_end = lx + tick.label.len() as u16;
        }
    }
}

/// Format a frequency value concisely: "50", "500", "2k", "3.3k", "16k".
fn format_freq(freq: f64) -> String {
    if freq >= 1000.0 {
        let k = freq / 1000.0;
        if (k - k.round()).abs() < 0.05 {
            format!("{}k", k.round() as u32)
        } else {
            format!("{:.1}k", k)
        }
    } else {
        format!("{}", freq.round() as u32)
    }
}

/// Render lyrics in the given area, scrolling to center the current line.
fn render_lyrics(buf: &mut Buffer, area: Rect, lyrics: &Lyrics, elapsed: Duration) {
    if area.height == 0 || area.width < 10 {
        return;
    }

    let total_lines = lyrics.line_count();
    if total_lines == 0 {
        return;
    }

    let visible_rows = area.height as usize;
    let current_idx = lyrics.current_line_index(elapsed);

    // Collect all non-empty lines with their original indices.
    let all_visible: Vec<(usize, &str)> = (0..total_lines)
        .map(|i| (i, lyrics.line_text(i)))
        .filter(|(_, t)| !t.is_empty())
        .collect();

    // Find the visual index of the current line.
    let current_visual_idx = current_idx.and_then(|cur| {
        all_visible.iter().position(|(idx, _)| *idx == cur)
    });

    // Scroll so the current line is centered vertically.
    let scroll = match current_visual_idx {
        Some(vi) => vi.saturating_sub(visible_rows / 2),
        None => 0,
    };

    let visible_lines: Vec<(usize, &str)> = all_visible
        .into_iter()
        .skip(scroll)
        .take(visible_rows)
        .collect();

    // Find the visual position of the current line within the visible window.
    let current_visual = current_idx.and_then(|cur| {
        visible_lines.iter().position(|(idx, _)| *idx == cur)
    });

    for (visual_pos, &(_, text)) in visible_lines.iter().enumerate() {
        let y = area.y + visual_pos as u16;
        if y >= area.y + area.height {
            break;
        }

        // Determine style based on visual distance from current line.
        // Current = cyan, above = darker fading up, below = drop then lighter then fade.
        let style = match current_visual {
            Some(cur_vis) => {
                let diff = visual_pos as isize - cur_vis as isize;
                if diff == 0 {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if diff < 0 {
                    let dist = (-diff) as u8;
                    match dist {
                        1 => Style::default().fg(Color::Rgb(120, 120, 130)),
                        2 => Style::default().fg(Color::Rgb(90, 90, 100)),
                        3 => Style::default().fg(Color::Rgb(65, 65, 75)),
                        _ => Style::default().fg(Color::Rgb(45, 45, 55)),
                    }
                } else {
                    let dist = diff as u8;
                    match dist {
                        1 => Style::default().fg(Color::Rgb(170, 170, 180)),
                        2 => Style::default().fg(Color::Rgb(120, 120, 130)),
                        3 => Style::default().fg(Color::Rgb(80, 80, 90)),
                        _ => Style::default().fg(Color::Rgb(50, 50, 60)),
                    }
                }
            }
            None => Style::default().fg(Color::Rgb(180, 180, 190)),
        };

        // Center the text horizontally.
        let text_width = text.chars().count().min(area.width as usize);
        let x = area.x + (area.width.saturating_sub(text_width as u16)) / 2;

        let display_text: String = text.chars().take(area.width as usize).collect();
        buf.set_string(x, y, &display_text, style);
    }
}

/// Compute album art display rows — shared between render and mouse-rect code.
/// Fixed items below art: title(1) + artist(1) + album(1) + controls(1)
/// + seek(1) + waveform(3) + visualizer(1) = 9 rows minimum.
pub fn compute_art_rows(has_art: bool, source_rows: usize, area_height: u16) -> u16 {
    let max_art_rows = area_height.saturating_sub(9) as usize;
    if has_art && max_art_rows >= 4 && source_rows > 0 {
        source_rows.min(max_art_rows).min(18) as u16
    } else {
        0
    }
}

/// Compute effective art/lyrics rows for mouse rect calculations.
/// When lyrics are showing, uses the same allocation logic as PlayingTab::render.
pub fn compute_effective_art_rows(
    has_art: bool,
    source_rows: usize,
    area_height: u16,
    show_lyrics: bool,
    has_lyrics: bool,
) -> u16 {
    if show_lyrics && has_lyrics {
        let max_rows = area_height.saturating_sub(9) as usize;
        max_rows.min(18).max(8) as u16
    } else {
        compute_art_rows(has_art, source_rows, area_height)
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

