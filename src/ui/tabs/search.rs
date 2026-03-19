use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Widget};
use std::path::PathBuf;

use crate::library::types::Library;

/// State for the Search tab.
#[derive(Debug, Clone)]
pub struct SearchState {
    /// Current search query.
    pub query: String,
    /// Flat indices into the library's `all_tracks()` iterator for matching tracks,
    /// ordered by best score descending.
    pub result_indices: Vec<usize>,
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
            result_indices: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            is_active: true,
        }
    }
}

/// Score a fuzzy subsequence match of `query` against `target`.
///
/// Returns `None` when `query` is not a subsequence of `target` (i.e. every
/// character of `query` must appear in order inside `target`, but gaps are
/// allowed).
///
/// When a match exists the returned score is higher for tighter, more
/// contextually meaningful matches:
///
/// - Consecutive run of matching characters: **+4** per character in the run
/// - Match immediately after a word boundary (`' '`, `'-'`, `'_'`, `'/'`): **+3**
/// - Match at position 0: **+3**
/// - Match on an ASCII uppercase letter (camelCase boundary): **+2**
/// - Any other matching character: **+1**
/// - Each skipped character in `target` (gap): **-1**
fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    // Collect target bytes once; we only need ASCII-level inspection for the
    // boundary checks, and Rust `char` iteration handles multi-byte correctly
    // for the subsequence walk.
    let target_chars: Vec<char> = target.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();

    let mut qi = 0; // index into query_chars
    let mut score: i32 = 0;
    let mut prev_matched = false;
    let mut gaps_since_last_match: i32 = 0;

    for (ti, &tc) in target_chars.iter().enumerate() {
        if qi >= query_chars.len() {
            break;
        }

        if tc == query_chars[qi] {
            // Apply gap penalty accumulated since the last match.
            score -= gaps_since_last_match;
            gaps_since_last_match = 0;

            // Determine positional bonus.
            let bonus = if ti == 0 {
                3
            } else if prev_matched {
                // Consecutive run — highest reward.
                4
            } else {
                let prev = target_chars[ti - 1];
                if prev == ' ' || prev == '-' || prev == '_' || prev == '/' {
                    // Start of a word token.
                    3
                } else if tc.is_ascii_uppercase() {
                    // camelCase boundary.
                    2
                } else {
                    1
                }
            };

            score += bonus;
            prev_matched = true;
            qi += 1;
        } else {
            gaps_since_last_match += 1;
            prev_matched = false;
        }
    }

    // All query characters must have been matched.
    if qi == query_chars.len() {
        Some(score)
    } else {
        None
    }
}

impl SearchState {
    /// Number of results.
    pub fn result_count(&self) -> usize {
        self.result_indices.len()
    }

    /// Filter library tracks using fuzzy matching against artist, album, and
    /// title. Results are sorted by best score descending. Clears results when
    /// the query is empty. Stores flat indices instead of cloning tracks.
    pub fn update_results(&mut self, library: &Library) {
        if self.query.is_empty() {
            self.result_indices.clear();
            self.selected = 0;
            self.scroll_offset = 0;
            return;
        }

        let query_lower = self.query.to_lowercase();

        let mut scored: Vec<(i32, usize)> = library
            .all_tracks()
            .enumerate()
            .filter_map(|(idx, track)| {
                let best = [
                    fuzzy_score(&query_lower, &track.artist.to_lowercase()),
                    fuzzy_score(&query_lower, &track.album.to_lowercase()),
                    fuzzy_score(&query_lower, &track.title.to_lowercase()),
                ]
                .into_iter()
                .flatten()
                .max();

                best.map(|score| (score, idx))
            })
            .collect();

        // Sort best matches first.
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        self.result_indices = scored.into_iter().map(|(_, idx)| idx).collect();

        // Clamp selection to valid range.
        if self.result_indices.is_empty() {
            self.selected = 0;
            self.scroll_offset = 0;
        } else if self.selected >= self.result_indices.len() {
            self.selected = self.result_indices.len() - 1;
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
        if !self.result_indices.is_empty() && self.selected < self.result_indices.len() - 1 {
            self.selected += 1;
        }
    }

    /// Get the file path of the currently selected track from the library.
    pub fn selected_track_path_from(&self, library: &Library) -> Option<PathBuf> {
        let &flat_idx = self.result_indices.get(self.selected)?;
        library.all_tracks().nth(flat_idx).map(|t| t.path.clone())
    }
}

pub struct SearchTab<'a> {
    pub state: &'a SearchState,
    pub library: &'a Library,
}

impl<'a> Widget for SearchTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 10 {
            return;
        }

        let chunks = Layout::vertical([
            Constraint::Length(1), // search input
            Constraint::Length(1), // result count
            Constraint::Min(0),   // results list
        ])
        .split(area);

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
            format!(" {} results", self.state.result_indices.len())
        };
        Paragraph::new(Line::from(Span::styled(
            count_text,
            Style::default().fg(Color::DarkGray),
        )))
        .render(chunks[1], buf);

        // Results list.
        if self.state.result_indices.is_empty() {
            return;
        }

        // Collect library tracks into a vec for indexed access.
        let all_tracks: Vec<_> = self.library.all_tracks().collect();

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
            .result_indices
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible_height)
            .filter_map(|(i, &flat_idx)| {
                let track = all_tracks.get(flat_idx)?;
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
                Some(ListItem::new(Line::from(Span::styled(display, style))))
            })
            .collect();

        let list = List::new(items);
        list.render(chunks[2], buf);
    }
}
