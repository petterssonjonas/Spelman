use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

/// Block characters for sub-cell resolution (8 levels per cell).
const BARS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Visualizer bar rendering style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarStyle {
    /// Flat cyan — no shading, no shadow.
    Cyan,
    /// Multi-shade cyan-teal-blue gradient with shadow.
    Gradient,
    /// Green → yellow → red spectrum with 6 colour zones and shadow.
    Spectrum,
}

impl Default for BarStyle {
    fn default() -> Self {
        Self::Gradient
    }
}

impl BarStyle {
    /// Cycle to the next style.
    pub fn next(self) -> Self {
        match self {
            Self::Gradient => Self::Spectrum,
            Self::Spectrum => Self::Cyan,
            Self::Cyan => Self::Gradient,
        }
    }

    /// Display name for settings UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cyan => "Cyan",
            Self::Gradient => "Gradient",
            Self::Spectrum => "Spectrum",
        }
    }
}

/// Visualizer display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VizMode {
    /// Classic vertical bars (default).
    Bars,
    /// Oscilloscope — braille waveform spreading from a center line.
    Oscilloscope,
}

impl Default for VizMode {
    fn default() -> Self {
        Self::Bars
    }
}

impl VizMode {
    pub fn next(self) -> Self {
        match self {
            Self::Bars => Self::Oscilloscope,
            Self::Oscilloscope => Self::Bars,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bars => "Bars",
            Self::Oscilloscope => "Oscilloscope",
        }
    }
}

/// Maximum number of spectrum bars rendered.
const MAX_BARS: usize = 64;
/// Minimum terminal columns needed before the visualizer renders.
const MIN_WIDTH_FOR_RENDER: u16 = 12;

// ── Shared bar layout ────────────────────────────────────────────────────────

/// Pre-computed horizontal layout for spectrum bars — shared between the
/// `Visualizer` widget and `render_hz_labels` so labels align to bars.
#[derive(Debug, Clone, Copy)]
pub struct BarLayout {
    pub num_bars: usize,
    pub bar_width: usize,
    pub gap: usize,
    pub stride: usize,
    pub left_pad: usize,
    pub actual_width: usize,
}

impl BarLayout {
    /// Compute bar layout for a given total width, desired bar count, and gap.
    pub fn compute(total_width: u16, viz_bars: usize, viz_gap: usize) -> Option<Self> {
        let total_width = total_width as usize;
        if total_width < MIN_WIDTH_FOR_RENDER as usize {
            return None;
        }
        let desired_bars = viz_bars.clamp(12, MAX_BARS);
        let gap = viz_gap;
        let max_fit = if gap > 0 {
            (total_width + gap) / (1 + gap)
        } else {
            total_width
        };
        let num_bars = desired_bars.min(max_fit);
        if num_bars == 0 {
            return None;
        }
        let total_gaps = if num_bars > 1 { (num_bars - 1) * gap } else { 0 };
        let usable = total_width.saturating_sub(total_gaps);
        let bar_width = (usable / num_bars).max(1);
        let stride = bar_width + gap;
        let actual_width = num_bars * bar_width + total_gaps;
        let left_pad = total_width.saturating_sub(actual_width) / 2;
        Some(Self { num_bars, bar_width, gap, stride, left_pad, actual_width })
    }

    /// X-center of bar `i` relative to the widget's left edge.
    pub fn bar_center(&self, i: usize) -> usize {
        self.left_pad + i * self.stride + self.bar_width / 2
    }
}

// ── Cava-style smoothing state ──────────────────────────────────────────────

/// Per-bar state for Cava-style gravity and smoothing.
#[derive(Debug, Clone)]
pub struct VisualizerState {
    /// Smoothed bar values (output of integral smoothing).
    mem: Vec<f32>,
    /// Previous frame output (for rise/fall detection).
    prev: Vec<f32>,
    /// Peak value for gravity falloff.
    peak: Vec<f32>,
    /// Fall accumulator per bar (quadratic gravity).
    fall: Vec<f32>,
    /// Auto-sensitivity multiplier.
    sensitivity: f32,
    /// Warmup counter — first N frames bypass EMA for instant display
    /// (mirrors Cava's frame_timer warmup behaviour).
    warm_frames: u32,
}

/// Frames to skip EMA during warmup (instant attack on track start).
const WARMUP_FRAMES: u32 = 16;

impl Default for VisualizerState {
    fn default() -> Self {
        Self {
            mem: Vec::new(),
            prev: Vec::new(),
            peak: Vec::new(),
            fall: Vec::new(),
            sensitivity: 3.0,
            warm_frames: 0,
        }
    }
}

impl VisualizerState {
    /// Process raw spectrum bars through Cava-style smoothing and gravity.
    ///
    /// `raw` is 0.0–1.0 per bar from the FFT/DSP chain.
    /// `framerate` is the approximate render FPS (typically 30–60).
    /// `noise_reduction` controls smoothing persistence (0.77 is Cava's default).
    ///
    /// Returns the processed bars ready for rendering.
    pub fn process(&mut self, raw: &[f32], framerate: f64, noise_reduction: f64) -> Vec<f32> {
        let n = raw.len();
        if n == 0 {
            return Vec::new();
        }

        // Resize state vectors if bar count changed.
        if self.mem.len() != n {
            self.mem = vec![0.0; n];
            self.prev = vec![0.0; n];
            self.peak = vec![0.0; n];
            self.fall = vec![0.0; n];
        }

        // Cava timing constants, adapted for our frame-based updates.
        let smoothing_time = (1.0 / framerate.max(1.0)) as f32;
        let fall_step = 0.048_f32 * smoothing_time * 30.0;

        // Gravity modifier — adapts to framerate (from Cava).
        let gravity_mod = ((60.0_f32 / framerate.max(1.0) as f32).powf(2.5) * 1.54
            / noise_reduction.max(0.1) as f32)
        .max(1.0);

        // Auto-sensitivity: scale raw input.
        let mut overshoot = false;
        let mut silence = true;
        let mut out = vec![0.0_f32; n];

        for i in 0..n {
            out[i] = raw[i] * self.sensitivity;
            if out[i] > 0.01 {
                silence = false;
            }
        }

        let warming = self.warm_frames < WARMUP_FRAMES;
        if warming {
            self.warm_frames += 1;
        }

        if warming {
            // Warmup phase: bypass EMA entirely — raw signal passes through
            // for instant visibility on track start (like Cava's frame_timer).
            // Seed mem so EMA has a warm start when smoothing kicks in.
            for i in 0..n {
                self.mem[i] = out[i];
            }
        } else {
            // Integral smoothing weights (exponential moving average).
            let integral_multiplier =
                (noise_reduction as f32).powf(smoothing_time * 30.0);
            let integral_weight = if noise_reduction < 1.0 {
                (1.0 - integral_multiplier) / (1.0 - noise_reduction as f32)
            } else {
                smoothing_time * 30.0
            };

            for i in 0..n {
                out[i] = self.mem[i] * integral_multiplier
                    + out[i] * integral_weight;
                self.mem[i] = out[i];
            }
        }

        // Gravity falloff and peak tracking.
        for i in 0..n {
            if out[i] >= self.prev[i] {
                // Rising — instant attack, reset fall.
                self.peak[i] = out[i];
                self.fall[i] = 0.0;
            } else if !warming {
                // Falling — quadratic gravity from peak.
                let fall_val =
                    (self.fall[i] + fall_step - 0.028).max(0.0);
                out[i] = self.peak[i]
                    * (1.0 - (fall_val * fall_val * gravity_mod));
                if out[i] < 0.0 {
                    out[i] = 0.0;
                }
                self.fall[i] += fall_step;
            }

            // Clamp and track overshoot for auto-sensitivity.
            if out[i] > 1.0 {
                overshoot = true;
                out[i] = 1.0;
            }
            self.prev[i] = out[i];
        }

        // Auto-sensitivity adjustment (from Cava).
        if overshoot {
            self.sensitivity *= 0.98;
        } else if !silence {
            self.sensitivity *= 1.002;
            self.sensitivity = self.sensitivity.min(5.0);
        }

        out
    }

    /// Reset state (e.g., on track change).
    pub fn reset(&mut self) {
        self.mem.clear();
        self.prev.clear();
        self.peak.clear();
        self.fall.clear();
        self.sensitivity = 3.0;
        self.warm_frames = 0;
    }
}

// ── Widget ──────────────────────────────────────────────────────────────────

/// Cava-style spectrum visualizer widget.
pub struct Visualizer<'a> {
    /// Frequency bar heights, each 0.0 to 1.0 (already processed through
    /// `VisualizerState::process`).
    pub spectrum: &'a [f32],
    /// Bar rendering style.
    pub bar_style: BarStyle,
    /// Desired number of bars (12–64).
    pub viz_bars: usize,
    /// Gap in columns between bars (0 = joined).
    pub viz_gap: usize,
}

impl<'a> Widget for Visualizer<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < MIN_WIDTH_FOR_RENDER || self.spectrum.is_empty() {
            return;
        }

        let layout = match BarLayout::compute(area.width, self.viz_bars, self.viz_gap) {
            Some(l) => l,
            None => return,
        };
        let BarLayout { num_bars, bar_width, stride, left_pad, .. } = layout;
        let max_height = area.height as f32;

        // Resample spectrum to the desired bar count.
        let bars = resample_spectrum(self.spectrum, num_bars);

        let is_cyan = self.bar_style == BarStyle::Cyan;

        for (i, &value) in bars.iter().enumerate() {
            let x_start = area.x + left_pad as u16 + (i * stride) as u16;
            if x_start >= area.x + area.width {
                break;
            }

            let bar_height = value * max_height;
            let full_cells = bar_height as u16;
            let frac = ((bar_height - full_cells as f32) * 8.0) as usize;

            for row in 0..area.height {
                let y = area.y + area.height - 1 - row;
                let cell_from_bottom = row;

                let (ch, color) = if cell_from_bottom < full_cells {
                    if is_cyan {
                        (BARS[8], Color::Cyan)
                    } else {
                        let intensity = 1.0 - (cell_from_bottom as f32 / max_height);
                        (BARS[8], self.bar_color(intensity))
                    }
                } else if cell_from_bottom == full_cells && frac > 0 {
                    if is_cyan {
                        (BARS[frac], Color::Cyan)
                    } else {
                        let intensity = 1.0 - (cell_from_bottom as f32 / max_height);
                        let base = self.bar_color(intensity);
                        (BARS[frac], darken(base, 0.45))
                    }
                } else {
                    continue;
                };

                let style = Style::default().fg(color);
                for bw in 0..bar_width {
                    let x = x_start + bw as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, ch, style);
                    }
                }
            }
        }
    }
}

/// Resample a spectrum slice to the desired number of bars.
/// If spectrum has more bars, average adjacent bins. If fewer, interpolate.
fn resample_spectrum(spectrum: &[f32], target: usize) -> Vec<f32> {
    let src_len = spectrum.len();
    if src_len == target {
        return spectrum.to_vec();
    }
    let mut out = vec![0.0_f32; target];
    for i in 0..target {
        let start = (i as f64 * src_len as f64 / target as f64) as usize;
        let end = (((i + 1) as f64 * src_len as f64 / target as f64) as usize).min(src_len);
        if end > start {
            let sum: f32 = spectrum[start..end].iter().sum();
            out[i] = sum / (end - start) as f32;
        } else if start < src_len {
            out[i] = spectrum[start];
        }
    }
    out
}

impl<'a> Visualizer<'a> {
    /// Compute bar colour based on vertical position intensity (1.0 = bottom, 0.0 = top).
    fn bar_color(&self, intensity: f32) -> Color {
        match self.bar_style {
            BarStyle::Cyan => Color::Cyan,
            BarStyle::Gradient => gradient_color(intensity),
            BarStyle::Spectrum => spectrum_color(intensity),
        }
    }
}

/// Darken an RGB colour by a factor (0.0 = black, 1.0 = unchanged).
fn darken(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        other => other,
    }
}

/// Multi-shade gradient: cyan base → green/teal mid → dark blue top.
fn gradient_color(intensity: f32) -> Color {
    if intensity > 0.66 {
        // Bottom — bright cyan.
        let t = (intensity - 0.66) / 0.34;
        let r = (10.0 + 20.0 * (1.0 - t)) as u8;
        let g = (180.0 + 75.0 * t) as u8;
        let b = (200.0 + 55.0 * t) as u8;
        Color::Rgb(r, g, b)
    } else if intensity > 0.33 {
        // Middle — green/teal.
        let t = (intensity - 0.33) / 0.33;
        let r = (20.0 + 20.0 * (1.0 - t)) as u8;
        let g = (120.0 + 60.0 * t) as u8;
        let b = (100.0 + 100.0 * t) as u8;
        Color::Rgb(r, g, b)
    } else {
        // Top — dim blue/purple.
        let t = intensity / 0.33;
        let r = (40.0 + 20.0 * (1.0 - t)) as u8;
        let g = (50.0 + 70.0 * t) as u8;
        let b = (80.0 + 20.0 * t) as u8;
        Color::Rgb(r, g, b)
    }
}

/// Green-to-red spectrum: 6 colour zones from bottom (green) to top (red).
/// Each zone blends smoothly into the next.
fn spectrum_color(intensity: f32) -> Color {
    // intensity 1.0 = bottom (green), 0.0 = top (red)
    // Zones: green → lime → yellow → orange → red-orange → red
    if intensity > 0.833 {
        // Zone 1 (bottom): green
        Color::Rgb(0, 220, 40)
    } else if intensity > 0.666 {
        // Zone 2: lime/light green
        let t = (intensity - 0.666) / 0.167;
        let r = (120.0 * (1.0 - t)) as u8;
        let g = (200.0 + 20.0 * t) as u8;
        Color::Rgb(r, g, 20)
    } else if intensity > 0.5 {
        // Zone 3: yellow
        let t = (intensity - 0.5) / 0.166;
        let r = (200.0 - 80.0 * t) as u8;
        let g = (200.0 + 0.0 * t) as u8;
        Color::Rgb(r, g, 10)
    } else if intensity > 0.333 {
        // Zone 4: orange
        let t = (intensity - 0.333) / 0.167;
        let g = (140.0 + 60.0 * t) as u8;
        Color::Rgb(220, g, 10)
    } else if intensity > 0.166 {
        // Zone 5: red-orange
        let t = (intensity - 0.166) / 0.167;
        let g = (60.0 + 80.0 * t) as u8;
        Color::Rgb(230, g, 10)
    } else {
        // Zone 6 (top): red
        let t = intensity / 0.166;
        let g = (20.0 + 40.0 * t) as u8;
        Color::Rgb(200, g, 10)
    }
}

/// Compute oscilloscope dot color based on bar style, intensity, and distance
/// from center line. `t` = 0.0–1.0 signal intensity, `dist_frac` = 0.0 (center)
/// to 1.0 (edge).
fn oscilloscope_color(bar_style: BarStyle, t: f32, dist_frac: f32) -> Color {
    match bar_style {
        BarStyle::Cyan => Color::Rgb(
            (40.0 + 180.0 * t * dist_frac) as u8,
            (180.0 - 40.0 * t) as u8,
            (220.0 - 60.0 * t) as u8,
        ),
        BarStyle::Gradient => {
            // Cyan at center → teal → blue at edges.
            if dist_frac < 0.5 {
                let u = dist_frac / 0.5;
                Color::Rgb(
                    (10.0 + 30.0 * u) as u8,
                    (180.0 + 40.0 * t - 40.0 * u) as u8,
                    (220.0 + 20.0 * t - 30.0 * u) as u8,
                )
            } else {
                let u = (dist_frac - 0.5) / 0.5;
                Color::Rgb(
                    (40.0 + 20.0 * u) as u8,
                    (140.0 + 20.0 * t - 50.0 * u) as u8,
                    (190.0 - 60.0 * u) as u8,
                )
            }
        }
        BarStyle::Spectrum => {
            // Green at center → yellow → red at edges.
            if dist_frac < 0.33 {
                Color::Rgb(
                    (30.0 + 40.0 * dist_frac * 3.0) as u8,
                    (180.0 + 40.0 * t) as u8,
                    30,
                )
            } else if dist_frac < 0.66 {
                let u = (dist_frac - 0.33) / 0.33;
                Color::Rgb(
                    (70.0 + 150.0 * u) as u8,
                    (180.0 + 20.0 * t - 20.0 * u) as u8,
                    20,
                )
            } else {
                let u = (dist_frac - 0.66) / 0.34;
                Color::Rgb(
                    (220.0 + 20.0 * u) as u8,
                    (160.0 - 120.0 * u) as u8,
                    10,
                )
            }
        }
    }
}

// ── Oscilloscope (braille) ──────────────────────────────────────────────────

/// Braille-dot oscilloscope — spectrum bars drive vertical displacement from
/// a center line.  When silent, a single horizontal line; when loud, the
/// waveform spreads symmetrically above and below the center.
///
/// Each terminal cell is a 2×4 braille grid (2 cols, 4 rows of dots).
/// We map spectrum values to dot patterns within the cell's vertical range.
pub struct Oscilloscope<'a> {
    /// Frequency bar heights, each 0.0 to 1.0 (already smoothed).
    pub spectrum: &'a [f32],
    /// Desired number of bars.
    pub viz_bars: usize,
    /// Gap in columns between bars.
    pub viz_gap: usize,
    /// Bar rendering style for color theming.
    pub bar_style: BarStyle,
}

/// Braille base codepoint (U+2800).  A braille char encodes an 8-dot pattern
/// in a 2-column × 4-row grid.  Dot numbering (bit positions):
///
///   col0  col1
///   0(0)  3(3)
///   1(1)  4(4)
///   2(2)  5(5)
///   6(6)  7(7)
const BRAILLE_BASE: u32 = 0x2800;

/// Map a (col, row) within a 2×4 braille cell to its bit index.
const BRAILLE_DOT: [[u8; 4]; 2] = [
    [0, 1, 2, 6], // col 0, rows 0-3
    [3, 4, 5, 7], // col 1, rows 0-3
];

impl<'a> Widget for Oscilloscope<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < 12 || self.spectrum.is_empty() {
            return;
        }

        let layout = match BarLayout::compute(area.width, self.viz_bars, self.viz_gap) {
            Some(l) => l,
            None => return,
        };

        // Resample spectrum to match bar count.
        let bars = resample_spectrum(self.spectrum, layout.num_bars);

        // Total dot-rows available: each cell row has 4 braille rows.
        let total_dot_rows = area.height as usize * 4;
        let center = total_dot_rows / 2;

        // For each column of cells, compute the braille pattern.
        // We iterate over cell columns and for each, determine which
        // spectrum bar it belongs to based on the layout.
        for col in 0..area.width as usize {
            // Which bar does this column fall in?
            let rel = col as isize - layout.left_pad as isize;
            if rel < 0 || rel as usize >= layout.actual_width {
                continue;
            }
            let rel = rel as usize;
            let bar_idx = rel / layout.stride;
            let within = rel % layout.stride;
            if within >= layout.bar_width || bar_idx >= bars.len() {
                continue; // in the gap
            }

            let value = bars[bar_idx];
            // Displacement in dot-rows from center (0.0 → 0, 1.0 → center).
            let disp = (value * center as f32).round() as usize;

            // For each cell row in this column, build the braille char.
            for cell_row in 0..area.height as usize {
                let mut pattern: u8 = 0;
                // This cell covers dot-rows [cell_row*4 .. cell_row*4+3].
                for dot_row in 0..4_usize {
                    let global_dot = cell_row * 4 + dot_row;
                    // Distance from center line.
                    let dist = (global_dot as isize - center as isize).unsigned_abs();
                    if dist <= disp {
                        // Light this dot in col 0 (single-column per cell).
                        pattern |= 1 << BRAILLE_DOT[0][dot_row];
                    }
                }
                if pattern == 0 {
                    continue;
                }
                let ch = char::from_u32(BRAILLE_BASE + pattern as u32).unwrap_or(' ');
                // Color based on bar style theme and distance from center.
                let color = if disp == 0 {
                    match self.bar_style {
                        BarStyle::Cyan => Color::Rgb(60, 140, 180),
                        BarStyle::Gradient => Color::Rgb(40, 120, 160),
                        BarStyle::Spectrum => Color::Rgb(30, 160, 60),
                    }
                } else {
                    // t = intensity: 0.0 for silence, ~0.8 for loud
                    let t = (value * 0.8).clamp(0.0, 1.0);
                    // Distance fraction from center (0.0 = center, 1.0 = edge)
                    let dist_frac = if disp > 0 {
                        let global_dot = cell_row * 4 + 2; // cell midpoint
                        let d = (global_dot as f32 - center as f32).abs();
                        (d / disp as f32).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    oscilloscope_color(self.bar_style, t, dist_frac)
                };
                let x = area.x + col as u16;
                let y = area.y + cell_row as u16;
                buf.set_string(x, y, &ch.to_string(), Style::default().fg(color));
            }
        }

        // Draw center line in empty columns (silence baseline).
        let center_cell_row = center / 4;
        let center_dot_in_cell = center % 4;
        if center_cell_row < area.height as usize {
            let cy = area.y + center_cell_row as u16;
            for col in 0..area.width as usize {
                let x = area.x + col as u16;
                let cell = buf.cell_mut((x, cy));
                if let Some(cell) = cell {
                    if cell.symbol() == " " {
                        let pattern = 1u8 << BRAILLE_DOT[0][center_dot_in_cell];
                        let ch = char::from_u32(BRAILLE_BASE + pattern as u32).unwrap_or(' ');
                        let line_color = match self.bar_style {
                            BarStyle::Cyan => Color::Rgb(40, 80, 100),
                            BarStyle::Gradient => Color::Rgb(30, 70, 90),
                            BarStyle::Spectrum => Color::Rgb(20, 80, 30),
                        };
                        cell.set_symbol(&ch.to_string());
                        cell.set_fg(line_color);
                    }
                }
            }
        }
    }
}
