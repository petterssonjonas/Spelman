//! Chroma GPU visualizer integration.
//!
//! Runs a wgpu compute shader on a dedicated GPU thread, converts output
//! to ASCII+color, and renders into a ratatui Buffer as a fullscreen overlay.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use chroma::ascii::AsciiConverter;
use chroma::params::{PatternType, ShaderParams};
use chroma::shader::{ShaderPipeline, ShaderUniforms};

use crate::lyrics::Lyrics;

/// Command sent from UI thread to GPU thread.
enum ChromaCommand {
    Render {
        params: ShaderParams,
        width: u32,
        height: u32,
    },
    Shutdown,
}

/// A rendered frame from the GPU thread.
pub struct ChromaFrame {
    pub cells: Vec<Vec<(char, crossterm::style::Color)>>,
    pub width: u32,
    pub height: u32,
}

/// Manages the GPU thread and communication channels.
pub struct ChromaState {
    cmd_tx: Sender<ChromaCommand>,
    frame_rx: Receiver<ChromaFrame>,
    _thread: Option<thread::JoinHandle<()>>,
    /// Most recent frame received from the GPU thread.
    pub latest_frame: Option<ChromaFrame>,
    /// Current pattern index.
    pub pattern_index: usize,
    /// All available patterns.
    patterns: Vec<PatternType>,
    /// Animation time origin.
    pub start_time: Instant,
    /// GPU init failed — don't try again.
    pub gpu_failed: bool,
    /// Last dimensions sent, to detect resize.
    last_size: (u32, u32),
}

impl ChromaState {
    /// Spawn the GPU thread. Returns None if GPU init fails.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let (cmd_tx, cmd_rx) = crossbeam_channel::bounded::<ChromaCommand>(1);
        let (frame_tx, frame_rx) = crossbeam_channel::bounded::<ChromaFrame>(1);
        let gpu_ready = Arc::new(AtomicBool::new(false));
        let gpu_ok = Arc::new(AtomicBool::new(false));
        let ready_clone = gpu_ready.clone();
        let ok_clone = gpu_ok.clone();

        let thread = thread::Builder::new()
            .name("chroma-gpu".into())
            .spawn(move || {
                let pipeline = match pollster::block_on(ShaderPipeline::new(
                    width,
                    height,
                    None,
                    &mut std::io::sink(),
                )) {
                    Ok(p) => {
                        ok_clone.store(true, Ordering::Release);
                        ready_clone.store(true, Ordering::Release);
                        p
                    }
                    Err(e) => {
                        tracing::warn!("Chroma GPU init failed: {e}");
                        ready_clone.store(true, Ordering::Release);
                        return;
                    }
                };

                let converter = AsciiConverter::default();
                let mut pipeline = pipeline;

                loop {
                    match cmd_rx.recv() {
                        Ok(ChromaCommand::Render {
                            params,
                            width: req_w,
                            height: req_h,
                        }) => {
                            // Recreate pipeline if terminal was resized.
                            if req_w != pipeline.width() || req_h != pipeline.height() {
                                match pollster::block_on(ShaderPipeline::new(
                                    req_w,
                                    req_h,
                                    None,
                                    &mut std::io::sink(),
                                )) {
                                    Ok(p) => pipeline = p,
                                    Err(e) => {
                                        tracing::debug!("Chroma resize failed: {e}");
                                        // Continue with old pipeline dimensions.
                                    }
                                }
                            }

                            let pw = pipeline.width();
                            let ph = pipeline.height();
                            let mut p = params;
                            p.resolution_width = pw as _;
                            p.resolution_height = ph as _;
                            let uniforms = ShaderUniforms::from_params(&p);
                            match pipeline.render(&uniforms) {
                                Ok(pixels) => {
                                    let cells =
                                        converter.convert_frame(&pixels, pw, ph);
                                    let _ = frame_tx.try_send(ChromaFrame {
                                        cells,
                                        width: pw,
                                        height: ph,
                                    });
                                }
                                Err(e) => {
                                    tracing::debug!("Chroma render error: {e}");
                                }
                            }
                        }
                        Ok(ChromaCommand::Shutdown) | Err(_) => break,
                    }
                }
            })
            .ok()?;

        // Wait for GPU init (with timeout).
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gpu_ready.load(Ordering::Acquire) {
            if Instant::now() > deadline {
                tracing::warn!("Chroma GPU init timed out");
                return None;
            }
            thread::sleep(Duration::from_millis(10));
        }

        if !gpu_ok.load(Ordering::Acquire) {
            return None;
        }

        let patterns = vec![
            PatternType::Plasma,
            PatternType::Waves,
            PatternType::Ripples,
            PatternType::Vortex,
            PatternType::Noise,
            PatternType::Voronoi,
            PatternType::Fractal,
            PatternType::Spiral,
            PatternType::Rings,
            PatternType::Kaleidoscope,
            PatternType::Tunnel,
            PatternType::Metaballs,
            PatternType::Fluid,
            PatternType::Hexagonal,
            PatternType::Interference,
            PatternType::Diamonds,
            PatternType::Sphere,
            PatternType::WarpedFbm,
            PatternType::World,
        ];

        Some(Self {
            cmd_tx,
            frame_rx,
            _thread: Some(thread),
            latest_frame: None,
            pattern_index: 0,
            patterns,
            start_time: Instant::now(),
            gpu_failed: false,
            last_size: (width, height),
        })
    }

    /// Send a render command and poll for the latest frame.
    pub fn tick(&mut self, spectrum: &[f32], width: u32, height: u32) {
        let time = self.start_time.elapsed().as_secs_f32();
        let pattern = self.patterns[self.pattern_index];
        let params = spectrum_to_params(spectrum, time, pattern);

        // Only send if channel is ready (don't block).
        let _ = self.cmd_tx.try_send(ChromaCommand::Render {
            params,
            width,
            height,
        });
        self.last_size = (width, height);

        // Drain to latest frame.
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.latest_frame = Some(frame);
        }
    }

    /// Cycle to the next pattern.
    pub fn next_pattern(&mut self) {
        self.pattern_index = (self.pattern_index + 1) % self.patterns.len();
    }

    /// Cycle to the previous pattern.
    pub fn prev_pattern(&mut self) {
        self.pattern_index = (self.pattern_index + self.patterns.len() - 1) % self.patterns.len();
    }

    /// Current pattern name for display.
    pub fn pattern_name(&self) -> &'static str {
        self.patterns[self.pattern_index].name()
    }

    /// Shut down the GPU thread.
    pub fn shutdown(&mut self) {
        let _ = self.cmd_tx.send(ChromaCommand::Shutdown);
    }
}

/// Map Spelman's 32-bar FFT spectrum to Chroma ShaderParams.
fn spectrum_to_params(spectrum: &[f32], time: f32, pattern: PatternType) -> ShaderParams {
    let bass = spectrum.get(0..4).map(|s| s.iter().sum::<f32>() / 4.0).unwrap_or(0.0);
    let mid = spectrum.get(4..12).map(|s| s.iter().sum::<f32>() / 8.0).unwrap_or(0.0);
    let treble = spectrum.get(12..32).map(|s| s.iter().sum::<f32>() / 20.0).unwrap_or(0.0);
    let energy = bass * 0.1 + mid * 0.3 + treble * 0.6;

    let mut p = ShaderParams::default();
    p.time = time;
    p.audio_enabled = true;
    p.bass_influence = bass;
    p.mid_influence = mid;
    p.treble_influence = treble;
    p.amplitude = 0.5 + energy;
    p.speed = 0.3 + energy * 1.5;
    p.brightness = 0.4 + energy * 0.6;
    p.contrast = 0.8 + bass * 0.4;
    p.pattern_type = pattern;
    // Beat effects from bass spikes.
    let beat = (bass * 2.0).min(1.0);
    p.beat_distortion_strength = beat * 0.3;
    p.beat_zoom_strength = beat * 0.2;
    p.beat_distortion_time = if beat > 0.6 { time } else { 0.0 };
    p
}

/// Fullscreen overlay widget that paints Chroma output into ratatui Buffer.
pub struct ChromaOverlay<'a> {
    pub frame: &'a ChromaFrame,
    pub lyrics: Option<&'a Lyrics>,
    pub elapsed: Duration,
    pub show_lyrics: bool,
    pub pattern_name: &'a str,
    /// Backdrop darkness for lyrics: 0=off, 1=light, 2=medium, 3=heavy.
    pub backdrop_level: u8,
}

impl<'a> Widget for ChromaOverlay<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the entire area first — reset all cells to blank/black.
        // This prevents underlying tab content from bleeding through
        // if the Chroma frame is smaller than the terminal.
        let blank_style = Style::default();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ");
                    cell.set_style(blank_style);
                }
            }
        }

        // Paint Chroma char+color grid into the buffer.
        for (row_idx, row) in self.frame.cells.iter().enumerate() {
            let y = area.y + row_idx as u16;
            if y >= area.y + area.height {
                break;
            }
            for (col_idx, &(ch, ref color)) in row.iter().enumerate() {
                let x = area.x + col_idx as u16;
                if x >= area.x + area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(&ch.to_string());
                    let fg = crossterm_to_ratatui_color(color);
                    cell.set_style(Style::default().fg(fg));
                }
            }
        }

        // Pattern name indicator (bottom-right corner).
        let label = format!(" {} ", self.pattern_name);
        let label_len = label.len() as u16;
        let lx = area.x + area.width.saturating_sub(label_len + 1);
        let ly = area.y + area.height.saturating_sub(1);
        if ly >= area.y && lx >= area.x {
            buf.set_string(lx, ly, &label, Style::default().fg(Color::DarkGray));
        }

        // Overlay lyrics on top (full-screen backdrop).
        if self.show_lyrics && self.backdrop_level > 0 {
            if let Some(lyrics) = self.lyrics {
                render_lyrics_overlay(buf, area, lyrics, self.elapsed, self.backdrop_level);
            }
        } else if self.show_lyrics {
            if let Some(lyrics) = self.lyrics {
                render_lyrics_overlay(buf, area, lyrics, self.elapsed, 0);
            }
        }
    }
}

/// Convert crossterm::style::Color to ratatui::style::Color.
fn crossterm_to_ratatui_color(c: &crossterm::style::Color) -> Color {
    match c {
        crossterm::style::Color::Rgb { r, g, b } => Color::Rgb(*r, *g, *b),
        crossterm::style::Color::White => Color::White,
        crossterm::style::Color::Black => Color::Black,
        crossterm::style::Color::Red => Color::Red,
        crossterm::style::Color::Green => Color::Green,
        crossterm::style::Color::Yellow => Color::Yellow,
        crossterm::style::Color::Blue => Color::Blue,
        crossterm::style::Color::Magenta => Color::Magenta,
        crossterm::style::Color::Cyan => Color::Cyan,
        crossterm::style::Color::DarkGrey => Color::DarkGray,
        _ => Color::White,
    }
}

/// Render lyrics on top of the Chroma visualizer with a full-screen dark backdrop.
/// `backdrop_level`: 0=no darkening, 1=light, 2=medium, 3=heavy.
fn render_lyrics_overlay(buf: &mut Buffer, area: Rect, lyrics: &Lyrics, elapsed: Duration, backdrop_level: u8) {
    let total_lines = lyrics.line_count();
    if total_lines == 0 || area.height < 5 {
        return;
    }

    let current_idx = lyrics.current_line_index(elapsed);

    // Collect non-empty lines.
    let all_visible: Vec<(usize, &str)> = (0..total_lines)
        .map(|i| (i, lyrics.line_text(i)))
        .filter(|(_, t)| !t.is_empty())
        .collect();

    if all_visible.is_empty() {
        return;
    }

    let current_visual_idx = current_idx.and_then(|cur| {
        all_visible.iter().position(|(idx, _)| *idx == cur)
    });

    // Show up to 9 lines, centered in the area.
    let visible_count = 9usize.min(all_visible.len());
    let lyrics_height = visible_count as u16 + 2; // +2 padding
    let lyrics_y = area.y + (area.height.saturating_sub(lyrics_height)) / 2;

    // Scroll so current line is centered within the visible window.
    let scroll = match current_visual_idx {
        Some(vi) => vi.saturating_sub(visible_count / 2),
        None => 0,
    };

    let window: Vec<(usize, &str)> = all_visible
        .into_iter()
        .skip(scroll)
        .take(visible_count)
        .collect();

    let current_in_window = current_idx.and_then(|cur| {
        window.iter().position(|(idx, _)| *idx == cur)
    });

    // Full-screen backdrop: darken ALL Chroma content.
    if backdrop_level > 0 {
        // Divisor: level 1 = /2 (light), level 2 = /3 (medium), level 3 = /5 (heavy)
        let divisor = match backdrop_level {
            1 => 2u8,
            2 => 3,
            _ => 5,
        };
        let bg = match backdrop_level {
            1 => Color::Reset,
            2 => Color::Rgb(5, 5, 8),
            _ => Color::Rgb(8, 8, 12),
        };
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    if let Color::Rgb(r, g, b) = cell.fg {
                        cell.set_fg(Color::Rgb(r / divisor, g / divisor, b / divisor));
                    }
                    if backdrop_level >= 2 {
                        cell.set_bg(bg);
                    }
                }
            }
        }
    }

    // Render lyrics lines centered, with 1-row padding top.
    for (visual_pos, &(_, text)) in window.iter().enumerate() {
        let y = lyrics_y + 1 + visual_pos as u16;
        if y >= area.y + area.height {
            break;
        }

        // Directional gradient: above = darker, below = lighter then fade.
        let style = match current_in_window {
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

        // Center text.
        let text_width = text.chars().count().min(area.width as usize);
        let x = area.x + (area.width.saturating_sub(text_width as u16)) / 2;
        let display_text: String = text.chars().take(area.width as usize).collect();
        buf.set_string(x, y, &display_text, style);
    }
}
