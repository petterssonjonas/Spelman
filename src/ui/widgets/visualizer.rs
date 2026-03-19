use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

/// Block characters for sub-cell resolution (8 levels per cell).
const BARS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// CAVA-style spectrum visualizer.
pub struct Visualizer<'a> {
    /// Frequency bar heights, each 0.0 to 1.0.
    pub spectrum: &'a [f32],
}

impl<'a> Widget for Visualizer<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < 4 || self.spectrum.is_empty() {
            return;
        }

        let num_bars = self.spectrum.len();
        let max_height = area.height as f32;

        // Fill the entire width — no gaps between pillars.
        let total_width = area.width as usize;
        let bar_width = (total_width / num_bars).max(1);
        let total_bar_space = bar_width * num_bars;
        let left_pad = (total_width.saturating_sub(total_bar_space)) / 2;

        for (i, &value) in self.spectrum.iter().enumerate() {
            let x_start = area.x + left_pad as u16 + (i * bar_width) as u16;
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
                    // Full cell — shade by how high up the bar this cell is.
                    let intensity = 1.0 - (cell_from_bottom as f32 / full_cells.max(1) as f32);
                    (BARS[8], shade_color(intensity))
                } else if cell_from_bottom == full_cells && frac > 0 {
                    // Partial top cell — dimmest.
                    (BARS[frac], shade_color(0.0))
                } else {
                    continue;
                };

                let style = Style::default().fg(color);
                // Fill entire bar_width — no gap subtracted.
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

/// Shade a pillar cell based on intensity (1.0 = bottom/brightest, 0.0 = top/dimmest).
///
/// Uses RGB for a smooth cyan-to-dark gradient:
///   bottom → bright cyan, top → dark blue-gray.
fn shade_color(intensity: f32) -> Color {
    // Bright cyan at bottom, fading toward dark teal at top.
    let r = (20.0 + 30.0 * (1.0 - intensity)) as u8;
    let g = (60.0 + 195.0 * intensity) as u8;
    let b = (80.0 + 175.0 * intensity) as u8;
    Color::Rgb(r, g, b)
}
