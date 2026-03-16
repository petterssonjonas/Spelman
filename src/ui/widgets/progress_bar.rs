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
        let bar_width =
            area.width as usize - time_left.len() - time_right.len() - 2;

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
        for i in 0..bar_width {
            let ch = if i < filled {
                self.filled_char
            } else {
                self.empty_char
            };
            let style = if i < filled {
                self.style
            } else {
                Style::default().fg(Color::DarkGray)
            };
            buf.set_string(bar_x + i as u16, area.y, ch.to_string(), style);
        }

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

fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}
