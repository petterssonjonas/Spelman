use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use std::time::Duration;

pub struct ProgressBar {
    pub elapsed: Duration,
    pub total: Duration,
    pub style: Style,
    pub filled_char: char,
    pub empty_char: char,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            elapsed: Duration::ZERO,
            total: Duration::ZERO,
            style: Style::default().fg(Color::Cyan),
            filled_char: '━',
            empty_char: '─',
        }
    }
}

impl ProgressBar {
    pub fn elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = elapsed;
        self
    }

    pub fn total(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }
}

impl Widget for ProgressBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        let time_left = format_duration(self.elapsed);
        let time_right = format_duration(self.total);
        let overhead = time_left.len() + time_right.len() + 2;
        if (area.width as usize) <= overhead {
            return;
        }
        let bar_width = area.width as usize - overhead;

        if bar_width < 4 {
            return;
        }

        let fraction = if self.total.as_secs_f64() > 0.0 {
            (self.elapsed.as_secs_f64() / self.total.as_secs_f64()).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let filled = (fraction * bar_width as f64) as usize;

        // Render time left.
        buf.set_string(
            area.x,
            area.y,
            &time_left,
            Style::default().fg(Color::White),
        );

        // Render bar.
        let bar_x = area.x + time_left.len() as u16 + 1;
        let filled_str: String = std::iter::repeat(self.filled_char).take(filled).collect();
        let empty_str: String = std::iter::repeat(self.empty_char).take(bar_width - filled).collect();
        buf.set_string(bar_x, area.y, &filled_str, self.style);
        buf.set_string(bar_x + filled as u16, area.y, &empty_str, Style::default().fg(Color::DarkGray));

        // Render time right.
        let right_x = bar_x + bar_width as u16 + 1;
        buf.set_string(
            right_x,
            area.y,
            &time_right,
            Style::default().fg(Color::White),
        );
    }
}

use crate::util::format::format_duration;
