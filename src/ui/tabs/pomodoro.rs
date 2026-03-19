use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::pomodoro::timer::{PomodoroPhase, PomodoroTimer, TimerStyle};

pub struct PomodoroTab<'a> {
    pub timer: &'a PomodoroTimer,
}

impl<'a> Widget for PomodoroTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 20 {
            return;
        }

        if !self.timer.active {
            let chunks = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(5),
                Constraint::Min(0),
            ])
            .split(area);

            let lines = vec![
                Line::from(Span::styled(
                    "Pomodoro Timer",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Enter to start a work session",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "25 min work / 5 min break / 15 min long break",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "'v' cycle style (Analog/Hourglass/Digital)",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            Paragraph::new(lines).centered().render(chunks[1], buf);
            return;
        }

        // Active timer layout.
        let chunks = Layout::vertical([
            Constraint::Length(1), // phase label + session count
            Constraint::Length(1), // spacer
            Constraint::Min(0),   // timer visual
            Constraint::Length(1), // spacer
            Constraint::Length(1), // remaining time
            Constraint::Length(1), // status bar
        ])
        .split(area);

        // Phase label.
        let phase_color = match self.timer.phase {
            PomodoroPhase::Work => Color::Green,
            PomodoroPhase::ShortBreak => Color::Yellow,
            PomodoroPhase::LongBreak => Color::Cyan,
        };

        let slb = self.timer.sessions_before_long_break.max(1);
        let session_num = if self.timer.phase == PomodoroPhase::Work {
            self.timer.sessions_completed % slb + 1
        } else {
            self.timer.sessions_completed % slb
        };

        let mut phase_spans = vec![
            Span::styled(
                format!(" {} ", self.timer.phase_label()),
                Style::default()
                    .fg(phase_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " Session {}/{}",
                    session_num, self.timer.sessions_before_long_break
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ];
        if !self.timer.ticking {
            phase_spans.push(Span::styled(
                "  PAUSED",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        Paragraph::new(Line::from(phase_spans))
            .centered()
            .render(chunks[0], buf);

        // Timer visual.
        match self.timer.timer_style {
            TimerStyle::Analog => render_analog_clock(self.timer, chunks[2], buf),
            TimerStyle::Hourglass => render_hourglass(self.timer, chunks[2], buf),
            TimerStyle::Digital => render_digital_timer(self.timer, chunks[2], buf),
        }

        // Remaining time.
        let remaining_color = if self.timer.break_ended {
            Color::Red
        } else {
            Color::White
        };
        Paragraph::new(Line::from(Span::styled(
            self.timer.remaining_display(),
            Style::default()
                .fg(remaining_color)
                .add_modifier(Modifier::BOLD),
        )))
        .centered()
        .render(chunks[4], buf);

        // Status bar.
        let style_name = match self.timer.timer_style {
            TimerStyle::Analog => "Analog",
            TimerStyle::Hourglass => "Hourglass",
            TimerStyle::Digital => "Digital",
        };
        Paragraph::new(Line::from(Span::styled(
            format!("Style: {style_name}  |  'v' cycle  |  Space pause  |  's' skip  |  Esc stop"),
            Style::default().fg(Color::DarkGray),
        )))
        .centered()
        .render(chunks[5], buf);
    }
}

/// Render an analog clock face with minute hand and progress arc.
fn render_analog_clock(timer: &PomodoroTimer, area: Rect, buf: &mut Buffer) {
    if area.height < 5 || area.width < 10 {
        return;
    }

    let cx = area.x + area.width / 2;
    let cy = area.y + area.height / 2;
    let radius = (area.height.min(area.width / 2)).saturating_sub(1).max(3);

    let fraction = timer.fraction();
    let is_break = timer.phase != PomodoroPhase::Work;
    let active_color = if is_break { Color::Red } else { Color::Green };

    // Draw clock face — 12 hour markers and 60 minute dots.
    for step in 0..60 {
        let angle =
            (step as f64) * std::f64::consts::TAU / 60.0 - std::f64::consts::FRAC_PI_2;
        let x = (cx as f64 + (radius as f64 * 2.0) * angle.cos()).round() as u16;
        let y = (cy as f64 + (radius as f64) * angle.sin()).round() as u16;

        if x < area.x || x >= area.x + area.width || y < area.y || y >= area.y + area.height {
            continue;
        }

        let is_hour = step % 5 == 0;
        let ch = if is_hour { "◆" } else { "·" };
        let step_frac = step as f64 / 60.0;
        let style = if step_frac <= fraction {
            Style::default().fg(active_color)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        buf.set_string(x, y, ch, style);
    }

    // Center dot.
    buf.set_string(cx, cy, "●", Style::default().fg(Color::Cyan));

    // Minute hand — points to the elapsed fraction.
    let hand_angle = fraction * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    let hand_len = radius.saturating_sub(2).max(1);
    let hand_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    for r in 1..=hand_len {
        let hx = (cx as f64 + (r as f64 * 2.0) * hand_angle.cos()).round() as u16;
        let hy = (cy as f64 + (r as f64) * hand_angle.sin()).round() as u16;
        if hx >= area.x && hx < area.x + area.width && hy >= area.y && hy < area.y + area.height
        {
            buf.set_string(hx, hy, "─", hand_style);
        }
    }

    // Second hand — a short tick showing sub-fraction movement.
    // Uses the fractional seconds part to create a ticking visual.
    let remaining_secs = timer.remaining().as_secs_f64();
    let sec_frac = (remaining_secs.fract()) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    let sec_len = (hand_len / 2).max(1);
    let sec_style = Style::default().fg(Color::Red);
    for r in 1..=sec_len {
        let sx = (cx as f64 + (r as f64 * 2.0) * sec_frac.cos()).round() as u16;
        let sy = (cy as f64 + (r as f64) * sec_frac.sin()).round() as u16;
        if sx >= area.x && sx < area.x + area.width && sy >= area.y && sy < area.y + area.height
        {
            buf.set_string(sx, sy, "·", sec_style);
        }
    }
}

/// Render an hourglass / sand timer.
fn render_hourglass(timer: &PomodoroTimer, area: Rect, buf: &mut Buffer) {
    if area.height < 5 || area.width < 16 {
        // Fall back to simple progress text if too small.
        let pct = (timer.fraction() * 100.0) as u8;
        let text = format!("[{}%] {}", pct, timer.remaining_display());
        let x = area.x + area.width.saturating_sub(text.len() as u16) / 2;
        let y = area.y + area.height / 2;
        buf.set_string(x, y, &text, Style::default().fg(Color::Yellow));
        return;
    }

    let fraction = timer.fraction();
    let is_break = timer.phase != PomodoroPhase::Work;
    let sand_color = if is_break { Color::Red } else { Color::Yellow };
    let glass_color = Color::DarkGray;

    let max_h = area.height.min(12) as usize;
    let half = max_h / 2;
    let cx = area.x + area.width / 2;
    let top_fill = ((1.0 - fraction) * (half.saturating_sub(1)) as f64).round() as usize;
    let bottom_fill = (fraction * (half.saturating_sub(1)) as f64).round() as usize;

    for row in 0..max_h {
        let y = area.y + (area.height.saturating_sub(max_h as u16)) / 2 + row as u16;
        if y >= area.y + area.height {
            break;
        }

        let start = cx.saturating_sub(7);

        if row == 0 || row == max_h - 1 {
            buf.set_string(start, y, "+-----------+", Style::default().fg(glass_color));
        } else if row == half {
            buf.set_string(start, y, "     >.<     ", Style::default().fg(glass_color));
        } else if row < half {
            let chamber_width = 9_usize.saturating_sub((row.saturating_sub(1)) * 2).max(1);
            let pad = (9 - chamber_width) / 2;
            let is_filled = row <= top_fill;
            let fill_ch = if is_filled { ":" } else { " " };
            let fill: String = fill_ch.repeat(chamber_width);
            let lp = " ".repeat(pad + 1);
            let rp = " ".repeat(9usize.saturating_sub(pad + chamber_width) + 1);
            let content = format!("|{lp}{fill}{rp}|");
            let style = if is_filled {
                Style::default().fg(sand_color)
            } else {
                Style::default().fg(glass_color)
            };
            buf.set_string(start, y, &content, style);
        } else {
            let dist_from_bottom = max_h - 1 - row;
            let chamber_width = 9_usize.saturating_sub((dist_from_bottom.saturating_sub(1)) * 2).max(1);
            let pad = (9 - chamber_width) / 2;
            let is_filled = dist_from_bottom < bottom_fill;
            let fill_ch = if is_filled { ":" } else { " " };
            let fill: String = fill_ch.repeat(chamber_width);
            let lp = " ".repeat(pad + 1);
            let rp = " ".repeat(9usize.saturating_sub(pad + chamber_width) + 1);
            let content = format!("|{lp}{fill}{rp}|");
            let style = if is_filled {
                Style::default().fg(sand_color)
            } else {
                Style::default().fg(glass_color)
            };
            buf.set_string(start, y, &content, style);
        }
    }
}

/// Render a large digital countdown.
fn render_digital_timer(timer: &PomodoroTimer, area: Rect, buf: &mut Buffer) {
    if area.height < 5 || area.width < 20 {
        // Fallback: simple centered text.
        let text = timer.remaining_display();
        let x = area.x + area.width.saturating_sub(text.len() as u16) / 2;
        let y = area.y + area.height / 2;
        let color = if timer.phase != PomodoroPhase::Work { Color::Yellow } else { Color::Green };
        buf.set_string(x, y, &text, Style::default().fg(color).add_modifier(Modifier::BOLD));
        return;
    }

    let remaining = timer.remaining();
    let mins = remaining.as_secs() / 60;
    let secs = remaining.as_secs() % 60;
    let is_break = timer.phase != PomodoroPhase::Work;

    let color = if timer.break_ended {
        Color::Red
    } else if is_break {
        Color::Yellow
    } else {
        Color::Green
    };

    let time_str = format!("{mins:02}:{secs:02}");
    let big = render_big_digits(&time_str);

    let start_y = area.y + area.height.saturating_sub(big.len() as u16 + 2) / 2;
    let style = Style::default()
        .fg(color)
        .add_modifier(Modifier::BOLD);

    for (i, line) in big.iter().enumerate() {
        let y = start_y + i as u16;
        if y >= area.y + area.height {
            break;
        }
        let x = area.x + area.width.saturating_sub(line.len() as u16) / 2;
        buf.set_string(x, y, line, style);
    }

    // Progress bar below digits.
    let bar_y = start_y + big.len() as u16 + 1;
    if bar_y < area.y + area.height {
        let bar_width = area.width.min(40) as usize;
        let filled = (timer.fraction() * bar_width as f64) as usize;
        let bar: String =
            "#".repeat(filled) + &"-".repeat(bar_width.saturating_sub(filled));
        let x = area.x + (area.width.saturating_sub(bar_width as u16)) / 2;
        buf.set_string(x, bar_y, &format!("[{bar}]"), Style::default().fg(color));
    }
}

/// Render digits in 5-line tall font using only basic ASCII.
/// Each digit is 4 chars wide (3 content + 1 space).
fn render_big_digits(s: &str) -> Vec<String> {
    #[rustfmt::skip]
    const D: [[&str; 5]; 11] = [
        ["###", "# #", "# #", "# #", "###"], // 0
        ["  #", "  #", "  #", "  #", "  #"], // 1
        ["###", "  #", "###", "#  ", "###"], // 2
        ["###", "  #", "###", "  #", "###"], // 3
        ["# #", "# #", "###", "  #", "  #"], // 4
        ["###", "#  ", "###", "  #", "###"], // 5
        ["###", "#  ", "###", "# #", "###"], // 6
        ["###", "  #", "  #", "  #", "  #"], // 7
        ["###", "# #", "###", "# #", "###"], // 8
        ["###", "# #", "###", "  #", "###"], // 9
        [" ", ":", " ", ":", " "],            // :
    ];

    let mut lines = vec![String::new(); 5];
    for ch in s.chars() {
        let idx = match ch {
            '0'..='9' => (ch as u8 - b'0') as usize,
            ':' => 10,
            _ => continue,
        };
        for (row, line) in lines.iter_mut().enumerate() {
            line.push_str(D[idx][row]);
            line.push(' ');
        }
    }
    lines
}
