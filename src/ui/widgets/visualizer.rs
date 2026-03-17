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

        // Calculate bar width and gap to fill the area.
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
                    // Full cell.
                    (BARS[8], bar_color(cell_from_bottom, area.height))
                } else if cell_from_bottom == full_cells && frac > 0 {
                    // Partial top cell.
                    (BARS[frac], bar_color(cell_from_bottom, area.height))
                } else {
                    continue;
                };

                let style = Style::default().fg(color);
                for bw in 0..bar_width.saturating_sub(if bar_width > 1 { 1 } else { 0 }) {
                    let x = x_start + bw as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, ch, style);
                    }
                }
            }
        }
    }
}

/// Color gradient from bottom (cyan) to top (magenta) like CAVA.
fn bar_color(row: u16, total: u16) -> Color {
    if total <= 1 {
        return Color::Cyan;
    }
    let frac = row as f32 / (total - 1) as f32;
    if frac < 0.33 {
        Color::Cyan
    } else if frac < 0.55 {
        Color::Green
    } else if frac < 0.75 {
        Color::Yellow
    } else {
        Color::Red
    }
}
