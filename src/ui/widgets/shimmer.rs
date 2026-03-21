use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

/// Reusable shimmer effect: a traveling brightness wave that can be applied
/// to any rows or rectangular region on a terminal buffer.
pub struct Shimmer {
    last_wave: Instant,
    wave_start: Option<Instant>,
    pub enabled: bool,
    /// Seconds between waves.
    pub interval: f64,
    /// Seconds for wave to cross the width.
    pub duration: f64,
    /// Brightness boost (max added per channel).
    pub intensity: f32,
    /// Wave width in cells.
    pub radius: f64,
}

impl Shimmer {
    pub fn new() -> Self {
        Self {
            last_wave: Instant::now(),
            wave_start: None,
            enabled: true,
            interval: 15.0,
            duration: 1.2,
            intensity: 180.0,
            radius: 4.0,
        }
    }

    /// Call every tick. Returns progress 0.0–1.0 if a wave is active.
    pub fn tick(&mut self) -> Option<f64> {
        if !self.enabled {
            return None;
        }

        if self.wave_start.is_none()
            && self.last_wave.elapsed().as_secs_f64() >= self.interval
        {
            self.wave_start = Some(Instant::now());
        }

        if let Some(start) = self.wave_start {
            if start.elapsed().as_secs_f64() > self.duration {
                self.wave_start = None;
                self.last_wave = Instant::now();
                return None;
            }
        }

        self.wave_start.map(|s| {
            (s.elapsed().as_secs_f64() / self.duration).clamp(0.0, 1.0)
        })
    }

    /// Apply the shimmer wave to specific rows within a region on the buffer.
    /// `rows` are absolute Y coordinates to shimmer.
    pub fn apply_to_rows(
        &self,
        buf: &mut Buffer,
        progress: f64,
        x: u16,
        width: u16,
        rows: &[u16],
    ) {
        let w = width as f64;
        let wave_center = progress * (w + self.radius * 2.5) - self.radius * 1.25;

        for &row_y in rows {
            for cx in x..x + width {
                let dist = ((cx - x) as f64 - wave_center).abs();
                if dist > self.radius {
                    continue;
                }
                if let Some(cell) = buf.cell_mut((cx, row_y)) {
                    let factor = 1.0 - (dist / self.radius);
                    let bright = brighten_color(cell.fg, factor as f32, self.intensity);
                    cell.set_style(Style::default().fg(bright));
                }
            }
        }
    }

    /// Apply shimmer to an entire Rect (all rows).
    pub fn apply_to_rect(&self, buf: &mut Buffer, progress: f64, rect: Rect) {
        let rows: Vec<u16> = (rect.y..rect.y + rect.height).collect();
        self.apply_to_rows(buf, progress, rect.x, rect.width, &rows);
    }
}

/// Brighten a ratatui Color toward white by `factor` (0.0 = unchanged, 1.0 = max boost).
pub fn brighten_color(color: Color, factor: f32, intensity: f32) -> Color {
    let (r, g, b) = color_to_rgb(color);
    let boost = (factor * intensity) as u8;
    Color::Rgb(
        r.saturating_add(boost),
        g.saturating_add(boost),
        b.saturating_add(boost),
    )
}

/// Map a ratatui named Color to approximate RGB values.
pub fn color_to_rgb(color: Color) -> (u8, u8, u8) {
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
