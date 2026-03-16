use ratatui::layout::{Constraint, Layout, Rect};

/// Split the terminal area into header (tab bar) and main content.
pub fn main_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // tab bar
        Constraint::Min(0),   // main content
        Constraint::Length(1), // footer hints
    ])
    .split(area);

    (chunks[0], chunks[1], chunks[2])
}
