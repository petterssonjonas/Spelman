use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};
use std::path::PathBuf;

use crate::library::types::{Library, Track};

/// State for the Search tab.
#[derive(Debug, Clone)]
pub struct SearchState {
    /// Current search query.
    pub query: String,
    /// Tracks matching the current query (cloned from library).
    pub results: Vec<Track>,
    /// Index of the selected result.
    pub selected: usize,
    /// Scroll offset for the results list.
    pub scroll_offset: usize,
    /// Whether the search input is focused.
    pub is_active: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            is_active: true,
        }
    }
}

impl SearchState {
    /// Filter library tracks by checking if artist, album, or title contains
    /// the query (case-insensitive). Clears results when query is empty.
    pub fn update_results(&mut self, library: &Library) {
        if self.query.is_empty() {
            self.results.clear();
            self.selected = 0;
            self.scroll_offset = 0;
            return;
        }

        let query_lower = self.query.to_lowercase();

        self.results = library
            .all_tracks
            .iter()
            .filter(|track| {
                track.artist.to_lowercase().contains(&query_lower)
                    || track.album.to_lowercase().contains(&query_lower)
                    || track.title.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        // Clamp selection to valid range.
        if self.results.is_empty() {
            self.selected = 0;
            self.scroll_offset = 0;
        } else if self.selected >= self.results.len() {
            self.selected = self.results.len() - 1;
        }
    }

    /// Append a character to the query.
    pub fn push_char(&mut self, ch: char) {
        self.query.push(ch);
    }

    /// Remove the last character from the query.
    pub fn pop_char(&mut self) {
        self.query.pop();
    }

    /// Move selection up by one.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down by one.
    pub fn move_down(&mut self) {
        if !self.results.is_empty() && self.selected < self.results.len() - 1 {
            self.selected += 1;
        }
    }

    /// Get the file path of the currently selected track.
    pub fn selected_track_path(&self) -> Option<PathBuf> {
        self.results.get(self.selected).map(|t| t.path.clone())
    }
}

pub struct SearchTab<'a> {
    pub state: &'a SearchState,
}

impl<'a> Widget for SearchTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Search ");

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        let chunks = Layout::vertical([
            Constraint::Length(1), // search input
            Constraint::Length(1), // result count
            Constraint::Min(0),   // results list
        ])
        .split(inner);

        // Search input with cursor.
        let cursor = if self.state.is_active { "_" } else { "" };
        let input_line = Line::from(vec![
            Span::styled(
                " / ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &self.state.query,
                Style::default().fg(Color::White),
            ),
            Span::styled(
                cursor,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]);
        Paragraph::new(input_line).render(chunks[0], buf);

        // Result count.
        let count_text = if self.state.query.is_empty() {
            String::from(" Type to search...")
        } else {
            format!(" {} results", self.state.results.len())
        };
        Paragraph::new(Line::from(Span::styled(
            count_text,
            Style::default().fg(Color::DarkGray),
        )))
        .render(chunks[1], buf);

        // Results list.
        if self.state.results.is_empty() {
            return;
        }

        let visible_height = chunks[2].height as usize;

        // Adjust scroll offset to keep selection visible.
        let scroll = {
            let mut offset = self.state.scroll_offset;
            if self.state.selected < offset {
                offset = self.state.selected;
            } else if visible_height > 0 && self.state.selected >= offset + visible_height {
                offset = self.state.selected - visible_height + 1;
            }
            offset
        };

        let items: Vec<ListItem> = self
            .state
            .results
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible_height)
            .map(|(i, track)| {
                let style = if i == self.state.selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::White)
                };

                let display = format!(
                    "  {} - {} ({})",
                    track.artist, track.title, track.album
                );
                ListItem::new(Line::from(Span::styled(display, style)))
            })
            .collect();

        let list = List::new(items);
        list.render(chunks[2], buf);
    }
}
