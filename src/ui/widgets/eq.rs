/// 10-band graphic EQ overlay widget for the Spelman TUI.
///
/// This module provides:
///
/// - [`EqState`]: owned state stored in `App`, holding gains, preset selection,
///   mouse hit-testing rects, and visibility flags.
/// - [`EQ_PRESETS`]: a curated set of named 10-band gain presets.
/// - [`render_eq`]: a standalone render function that writes the EQ panel into
///   a [`Buffer`] and simultaneously updates `state.band_rects` so the caller
///   can perform mouse hit-testing without a separate layout pass.
///
/// # Layout
///
/// ```text
///  EQ [ON]  ◂ Rock ▸                     [e:close  ◂/▸:preset  scroll:adjust]
///
///   60    170   310   600    1k    3k    6k   12k   14k   16k
///  ┌────┐┌────┐┌────┐┌────┐┌────┐┌────┐┌────┐┌────┐┌────┐┌────┐
///  │    ││    ││    ││    ││    ││    ││████││████││████││████│  +12
///  │    ││    ││    ││    ││    ││    ││████││████││████││████│
///  │████││    ││    ││    ││    ││████││████││████││████││████│
///  │████││████││    ││    ││    ││████││████││████││████││████│
///  │████││████││████││    ││    ││████││████││████││████││████│
///  │████││████││████││████││████││████││████││████││████││████│  ← 0 dB
///  │████││████││████││████││████││████││    ││    ││    ││    │
///  │    ││    ││████││████││████││    ││    ││    ││    ││    │
///  │    ││    ││    ││████││████││    ││    ││    ││    ││    │
///  │    ││    ││    ││    ││    ││    ││    ││    ││    ││    │
///  └────┘└────┘└────┘└────┘└────┘└────┘└────┘└────┘└────┘└────┘
///   +4    +2     0    -2    -2    +2    +4    +6    +6    +6
/// ```
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::audio::eq::{EQ_FREQUENCIES, NUM_EQ_BANDS};

// ── Presets ───────────────────────────────────────────────────────────────────

/// Named presets for the 10-band EQ.
///
/// Each entry is `(name, [gain_band_0..gain_band_9])` in dB.  Gain range per
/// band is −12 dB to +12 dB; all values here are within that range.
pub const EQ_PRESETS: &[(&str, [f32; NUM_EQ_BANDS])] = &[
    (
        "Flat",
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    ),
    (
        "Rock",
        [4.0, 3.0, 1.0, 0.0, -1.0, 1.0, 3.0, 4.0, 4.0, 3.0],
    ),
    (
        "Pop",
        [-1.0, 1.0, 3.0, 4.0, 3.0, 1.0, -1.0, -2.0, -2.0, -1.0],
    ),
    (
        "Classical",
        [0.0, 0.0, 0.0, 0.0, 0.0, -1.0, -2.0, -2.0, -1.0, 0.0],
    ),
    (
        "Bass Boost",
        [8.0, 6.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    ),
    (
        "Treble Boost",
        [0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0, 6.0, 6.0, 6.0],
    ),
    (
        "Vocal",
        [-2.0, -1.0, 0.0, 3.0, 5.0, 4.0, 2.0, 0.0, -1.0, -2.0],
    ),
    (
        "Electronic",
        [5.0, 4.0, 1.0, -2.0, -1.0, 1.0, 3.0, 5.0, 5.0, 4.0],
    ),
    (
        "Jazz",
        [3.0, 2.0, 0.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0, 3.0],
    ),
    (
        "R&B",
        [4.0, 6.0, 3.0, -1.0, -2.0, 1.0, 3.0, 3.0, 2.0, 1.0],
    ),
];

// ── EqState ───────────────────────────────────────────────────────────────────

/// Owned state for the EQ overlay, stored in `App`.
///
/// All interaction methods (`next_preset`, `adjust_band`, etc.) live here so
/// the event handler can update state without reaching into the widget.
#[derive(Debug, Clone)]
pub struct EqState {
    /// Whether the EQ panel is visible.
    pub visible: bool,
    /// Whether the EQ is currently processing audio.
    pub enabled: bool,
    /// Current gain per band in dB, clamped to `[−12, +12]`.
    pub gains: [f32; NUM_EQ_BANDS],
    /// Index into [`EQ_PRESETS`] for the currently displayed preset name.
    pub preset_index: usize,
    /// Band the mouse cursor is hovering over, or `None`.
    pub hovered_band: Option<usize>,
    /// Band that is keyboard-selected for scroll / arrow adjustment.
    pub selected_band: usize,
    /// Per-band column [`Rect`]s populated during the last `render_eq` call.
    ///
    /// These cover the full slider column including borders, and are used for
    /// mouse hit-testing via [`EqState::band_at`].
    pub band_rects: [Option<Rect>; NUM_EQ_BANDS],
}

impl Default for EqState {
    fn default() -> Self {
        Self {
            visible: false,
            enabled: false,
            gains: [0.0; NUM_EQ_BANDS],
            preset_index: 0,
            hovered_band: None,
            selected_band: 0,
            band_rects: [None; NUM_EQ_BANDS],
        }
    }
}

impl EqState {
    /// Toggle panel visibility.
    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    /// Toggle whether the EQ is processing audio.
    pub fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
    }

    /// Advance to the next preset, wrapping around.
    ///
    /// Immediately copies the preset gains into `self.gains`.
    pub fn next_preset(&mut self) {
        self.preset_index = (self.preset_index + 1) % EQ_PRESETS.len();
        self.gains = EQ_PRESETS[self.preset_index].1;
    }

    /// Go back to the previous preset, wrapping around.
    ///
    /// Immediately copies the preset gains into `self.gains`.
    pub fn prev_preset(&mut self) {
        self.preset_index = (self.preset_index + EQ_PRESETS.len() - 1) % EQ_PRESETS.len();
        self.gains = EQ_PRESETS[self.preset_index].1;
    }

    /// Adjust `band`'s gain by `delta` dB, clamping the result to `[−12, +12]`.
    ///
    /// Does nothing if `band >= NUM_EQ_BANDS`.
    pub fn adjust_band(&mut self, band: usize, delta: f32) {
        if band < NUM_EQ_BANDS {
            self.gains[band] = (self.gains[band] + delta).clamp(-12.0, 12.0);
        }
    }

    /// Return the band index that contains terminal cell `(col, row)`, or
    /// `None` if the position is outside every band rect.
    ///
    /// Uses the rects stored by the most recent [`render_eq`] call.
    #[must_use]
    pub fn band_at(&self, col: u16, row: u16) -> Option<usize> {
        for (i, rect) in self.band_rects.iter().enumerate() {
            if let Some(r) = rect {
                if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
                    return Some(i);
                }
            }
        }
        None
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Render the EQ panel into `area`, updating `state.band_rects` as a side
/// effect so the caller can hit-test mouse events.
///
/// The layout consumes exactly `area` and works at any terminal width >= 22
/// columns and height >= 5 rows (graceful bail-out below those minimums).
///
/// # Sections (top to bottom)
///
/// 1. **Header** (1 row) — on/off badge, preset name with ◂ ▸ arrows, key hints.
/// 2. **Frequency labels** (1 row) — abbreviated Hz/kHz labels centred over
///    each band column.
/// 3. **Slider area** (remaining height − 1) — bordered column per band, filled
///    with `█` above or below the 0 dB centre line, coloured by intensity.
///    A dotted centre line (`·`) marks 0 dB across every column.
/// 4. **Gain values** (1 row) — `+6`, `-3`, ` 0`, … centred under each band.
pub fn render_eq(state: &mut EqState, area: Rect, buf: &mut Buffer) {
    // Minimum viable render area.
    if area.width < 22 || area.height < 5 {
        return;
    }

    // ── Vertical layout ───────────────────────────────────────────────────────
    //   [0] header         1 row
    //   [1] freq labels    1 row
    //   [2] slider area    remaining − 1
    //   [3] gain values    1 row
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let header_area = rows[0];
    let freq_area = rows[1];
    let slider_area = rows[2];
    let gain_area = rows[3];

    // ── Band column geometry ──────────────────────────────────────────────────
    // Each band column: border-left (1) + fill (4) + border-right (1) = 6 chars.
    // Columns are packed with no gap (the right border of band N is the left
    // border of band N+1, so shared walls would overlap; instead we just let
    // each column own its own walls and distribute the available width).
    let total_width = slider_area.width as usize;
    let band_col_width = (total_width / NUM_EQ_BANDS).max(4);
    // Inner fill width is band_col_width − 2 (left + right border).
    let inner_width = band_col_width.saturating_sub(2).max(1);
    // Left offset to centre the band block.
    let bands_total = band_col_width * NUM_EQ_BANDS;
    let left_pad = (total_width.saturating_sub(bands_total)) / 2;

    // Pre-compute column x positions and store band rects.
    let mut col_xs = [0u16; NUM_EQ_BANDS];
    for (i, col_x) in col_xs.iter_mut().enumerate() {
        *col_x = slider_area.x + left_pad as u16 + (i * band_col_width) as u16;
        state.band_rects[i] = Some(Rect {
            x: *col_x,
            y: slider_area.y,
            width: band_col_width as u16,
            height: slider_area.height,
        });
    }

    // ── 1. Header ─────────────────────────────────────────────────────────────
    render_header(state, header_area, buf);

    // ── 2. Frequency labels ───────────────────────────────────────────────────
    render_freq_labels(&col_xs, band_col_width, freq_area, buf);

    // ── 3. Slider columns ─────────────────────────────────────────────────────
    render_sliders(state, &col_xs, band_col_width, inner_width, slider_area, buf);

    // ── 4. Gain values ────────────────────────────────────────────────────────
    render_gain_labels(state, &col_xs, band_col_width, gain_area, buf);
}

// ── Header ────────────────────────────────────────────────────────────────────

fn render_header(state: &EqState, area: Rect, buf: &mut Buffer) {
    let on_off = if state.enabled {
        Span::styled(" [ON] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("[OFF] ", Style::default().fg(Color::DarkGray))
    };

    let preset_name = EQ_PRESETS
        .get(state.preset_index)
        .map(|(n, _)| *n)
        .unwrap_or("Flat");

    let preset_span = Span::styled(
        format!(" \u{25c2} {preset_name} \u{25b8}"),
        Style::default().fg(Color::Cyan),
    );

    let hint = Span::styled(
        "  [e:close  \u{25c2}/\u{25b8}:preset  scroll:adjust]",
        Style::default().fg(Color::DarkGray),
    );

    let eq_label = Span::styled(
        "EQ",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    );

    let line = Line::from(vec![eq_label, on_off, preset_span, hint]);
    Paragraph::new(line).render(area, buf);
}

// ── Frequency labels ──────────────────────────────────────────────────────────

/// Abbreviated frequency label for display.
fn freq_label(hz: f32) -> &'static str {
    // Match against the known EQ_FREQUENCIES values exactly.
    if hz < 100.0 {
        "60"
    } else if hz < 200.0 {
        "170"
    } else if hz < 400.0 {
        "310"
    } else if hz < 800.0 {
        "600"
    } else if hz < 2_000.0 {
        "1k"
    } else if hz < 4_000.0 {
        "3k"
    } else if hz < 8_000.0 {
        "6k"
    } else if hz < 13_000.0 {
        "12k"
    } else if hz < 15_000.0 {
        "14k"
    } else {
        "16k"
    }
}

fn render_freq_labels(
    col_xs: &[u16; NUM_EQ_BANDS],
    band_col_width: usize,
    area: Rect,
    buf: &mut Buffer,
) {
    for (i, &x) in col_xs.iter().enumerate() {
        let label = freq_label(EQ_FREQUENCIES[i]);
        let label_len = label.len() as u16;
        // Centre the label over the column.
        let offset = (band_col_width as u16).saturating_sub(label_len) / 2;
        let draw_x = x + offset;
        if draw_x + label_len <= area.x + area.width {
            buf.set_string(
                draw_x,
                area.y,
                label,
                Style::default().fg(Color::DarkGray),
            );
        }
    }
}

// ── Slider columns ────────────────────────────────────────────────────────────

/// Pick a fill colour based on absolute gain magnitude.
///
/// | abs gain  | colour        |
/// |-----------|---------------|
/// | ≤ 3 dB    | Cyan          |
/// | ≤ 6 dB    | Green         |
/// | ≤ 9 dB    | Yellow        |
/// | > 9 dB    | Red           |
///
/// If the band is hovered/selected the colour is brightened to `LightXxx`.
fn gain_color(abs_gain: f32, highlighted: bool) -> Color {
    let base = if abs_gain <= 3.0 {
        Color::Cyan
    } else if abs_gain <= 6.0 {
        Color::Green
    } else if abs_gain <= 9.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    if highlighted {
        // Map to the Light variant.
        match base {
            Color::Cyan => Color::LightCyan,
            Color::Green => Color::LightGreen,
            Color::Yellow => Color::LightYellow,
            Color::Red => Color::LightRed,
            other => other,
        }
    } else {
        base
    }
}

fn render_sliders(
    state: &EqState,
    col_xs: &[u16; NUM_EQ_BANDS],
    band_col_width: usize,
    inner_width: usize,
    area: Rect,
    buf: &mut Buffer,
) {
    let h = area.height as usize;
    if h < 3 {
        return;
    }

    // 0 dB maps to the vertical centre row of the slider area.
    // With even height the centre is biased toward the upper half so that the
    // top (+12 dB) and bottom (−12 dB) extremes are symmetric.
    let center_row = h / 2; // 0-indexed row within slider area

    // Top and bottom dB marker positions.
    let top_row = 0usize;
    let bottom_row = h.saturating_sub(1);

    // Pre-render the +12 / -12 / 0 dB markers on the right margin.
    let right_margin = area.x + area.width;
    // Only render if there is at least 4 columns of margin after the bands.
    let bands_right = col_xs
        .last()
        .copied()
        .unwrap_or(area.x)
        .saturating_add(band_col_width as u16);

    if bands_right + 3 <= right_margin {
        buf.set_string(
            bands_right + 1,
            area.y + top_row as u16,
            "+12",
            Style::default().fg(Color::DarkGray),
        );
        buf.set_string(
            bands_right + 1,
            area.y + center_row as u16,
            " 0 ",
            Style::default().fg(Color::DarkGray),
        );
        buf.set_string(
            bands_right + 1,
            area.y + bottom_row as u16,
            "-12",
            Style::default().fg(Color::DarkGray),
        );
    }

    for band in 0..NUM_EQ_BANDS {
        let x = col_xs[band];
        let gain = state.gains[band];
        let abs_gain = gain.abs();
        let highlighted = state.hovered_band == Some(band) || state.selected_band == band;
        let fill_color = gain_color(abs_gain, highlighted);


        // How many rows of fill above/below centre?
        // Map gain/12 linearly onto center_row rows (boost = up, cut = down).
        let fill_rows = ((abs_gain / 12.0) * center_row as f32).round() as usize;

        // The fill range in 0-indexed rows within slider area.
        // Positive gain: rows [center_row - fill_rows, center_row).
        // Negative gain: rows (center_row, center_row + fill_rows].
        let (fill_top, fill_bottom) = if gain >= 0.0 {
            let top = center_row.saturating_sub(fill_rows);
            let bottom = if fill_rows > 0 { center_row } else { center_row };
            (top, bottom)
        } else {
            let top = center_row + 1;
            let bottom = (center_row + fill_rows).min(h.saturating_sub(1));
            (top, bottom)
        };

        // ── Top border ────────────────────────────────────────────────────────
        {
            let y = area.y;
            // Left corner: ┌ (only for first column), or middle ┬ appearance
            // We draw individual columns so each gets ┌ and ┐.
            let border_style = if highlighted {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            buf.set_string(x, y, "\u{250c}", border_style); // ┌
            for dx in 1..=(inner_width as u16) {
                buf.set_string(x + dx, y, "\u{2500}", border_style); // ─
            }
            buf.set_string(
                x + inner_width as u16 + 1,
                y,
                "\u{2510}",
                border_style,
            ); // ┐
        }

        // ── Bottom border ─────────────────────────────────────────────────────
        {
            let y = area.y + area.height.saturating_sub(1);
            let border_style = if highlighted {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            buf.set_string(x, y, "\u{2514}", border_style); // └
            for dx in 1..=(inner_width as u16) {
                buf.set_string(x + dx, y, "\u{2500}", border_style); // ─
            }
            buf.set_string(
                x + inner_width as u16 + 1,
                y,
                "\u{2518}",
                border_style,
            ); // ┘
        }

        // ── Inner rows ────────────────────────────────────────────────────────
        // Row 0 = top border (already drawn); iterate interior rows 1..h-1.
        for row in 1..h.saturating_sub(1) {
            let y = area.y + row as u16;
            let border_style = if highlighted {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            // Left wall.
            buf.set_string(x, y, "\u{2502}", border_style); // │
            // Right wall.
            buf.set_string(x + inner_width as u16 + 1, y, "\u{2502}", border_style); // │

            // Determine whether this interior row should be filled.
            let is_filled = row >= fill_top && row <= fill_bottom && fill_rows > 0;
            // 0 dB centre line uses a middle-dot sentinel when not filled.
            let is_center = row == center_row;

            for dx in 1..=(inner_width as u16) {
                let (ch, style) = if is_filled {
                    ("\u{2588}", Style::default().fg(fill_color)) // █
                } else if is_center {
                    ("\u{00b7}", Style::default().fg(Color::DarkGray)) // ·
                } else {
                    (" ", Style::default())
                };
                buf.set_string(x + dx, y, ch, style);
            }
        }
    }
}

// ── Gain labels ───────────────────────────────────────────────────────────────

fn render_gain_labels(
    state: &EqState,
    col_xs: &[u16; NUM_EQ_BANDS],
    band_col_width: usize,
    area: Rect,
    buf: &mut Buffer,
) {
    for (band, &x) in col_xs.iter().enumerate() {
        let gain = state.gains[band];
        let label = format_gain(gain);
        let label_len = label.len() as u16;
        let offset = (band_col_width as u16).saturating_sub(label_len) / 2;
        let draw_x = x + offset;

        if draw_x + label_len <= area.x + area.width {
            let highlighted =
                state.hovered_band == Some(band) || state.selected_band == band;
            let color = if highlighted {
                Color::White
            } else if gain.abs() < 0.5 {
                Color::DarkGray
            } else {
                gain_color(gain.abs(), false)
            };
            buf.set_string(
                draw_x,
                area.y,
                &label,
                Style::default().fg(color),
            );
        }
    }
}

/// Format a gain value for display: `+6`, `-3`, ` 0`.
fn format_gain(gain: f32) -> String {
    let rounded = gain.round() as i32;
    if rounded == 0 {
        " 0".to_string()
    } else if rounded > 0 {
        format!("+{rounded}")
    } else {
        format!("{rounded}")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_gain_zero() {
        assert_eq!(format_gain(0.0), " 0");
        assert_eq!(format_gain(0.4), " 0");
        assert_eq!(format_gain(-0.4), " 0");
    }

    #[test]
    fn format_gain_positive() {
        assert_eq!(format_gain(6.0), "+6");
        assert_eq!(format_gain(12.0), "+12");
    }

    #[test]
    fn format_gain_negative() {
        assert_eq!(format_gain(-3.0), "-3");
        assert_eq!(format_gain(-12.0), "-12");
    }

    #[test]
    fn eq_state_default_is_hidden_and_flat() {
        let s = EqState::default();
        assert!(!s.visible);
        assert!(!s.enabled);
        assert!(s.gains.iter().all(|&g| g == 0.0));
        assert_eq!(s.preset_index, 0);
        assert_eq!(EQ_PRESETS[0].0, "Flat");
    }

    #[test]
    fn toggle_visible_flips_flag() {
        let mut s = EqState::default();
        s.toggle_visible();
        assert!(s.visible);
        s.toggle_visible();
        assert!(!s.visible);
    }

    #[test]
    fn preset_navigation_wraps() {
        let mut s = EqState::default();
        let len = EQ_PRESETS.len();
        // Backward from index 0 wraps to last.
        s.prev_preset();
        assert_eq!(s.preset_index, len - 1);
        // Forward from last wraps to 0.
        s.next_preset();
        assert_eq!(s.preset_index, 0);
    }

    #[test]
    fn preset_copies_gains() {
        let mut s = EqState::default();
        s.next_preset(); // Rock
        assert_eq!(s.gains, EQ_PRESETS[1].1);
    }

    #[test]
    fn adjust_band_clamps() {
        let mut s = EqState::default();
        s.adjust_band(0, 999.0);
        assert_eq!(s.gains[0], 12.0);
        s.adjust_band(0, -999.0);
        assert_eq!(s.gains[0], -12.0);
    }

    #[test]
    fn adjust_band_out_of_range_is_noop() {
        let mut s = EqState::default();
        s.adjust_band(NUM_EQ_BANDS, 6.0); // must not panic
        assert!(s.gains.iter().all(|&g| g == 0.0));
    }

    #[test]
    fn band_at_returns_none_for_empty_rects() {
        let s = EqState::default();
        assert!(s.band_at(0, 0).is_none());
    }

    #[test]
    fn band_at_returns_correct_band() {
        let mut s = EqState::default();
        s.band_rects[3] = Some(Rect { x: 30, y: 5, width: 6, height: 10 });
        assert_eq!(s.band_at(32, 8), Some(3));
        assert!(s.band_at(36, 8).is_none()); // just outside right edge
    }

    #[test]
    fn render_eq_does_not_panic_on_small_area() {
        let mut state = EqState::default();
        let mut buf = Buffer::empty(Rect { x: 0, y: 0, width: 10, height: 3 });
        // Must not panic even though width < 22 and height < 5.
        render_eq(&mut state, buf.area, &mut buf);
    }

    #[test]
    fn render_eq_populates_band_rects() {
        let mut state = EqState::default();
        let area = Rect { x: 0, y: 0, width: 80, height: 20 };
        let mut buf = Buffer::empty(area);
        render_eq(&mut state, area, &mut buf);
        // Every band should now have a rect.
        for (i, rect) in state.band_rects.iter().enumerate() {
            assert!(rect.is_some(), "band {i} rect was not set");
        }
    }

    #[test]
    fn all_presets_gains_within_range() {
        for (name, gains) in EQ_PRESETS {
            for &g in gains {
                assert!(
                    g >= -12.0 && g <= 12.0,
                    "preset '{name}' has out-of-range gain {g}"
                );
            }
        }
    }
}
