use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};

use crate::library::types::Library;

/// What level of the library browser the user is viewing.
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryView {
    Artists,
    Albums { artist: String },
    Tracks { artist: String, album_index: usize },
}

/// State for the Library tab.
#[derive(Debug, Clone)]
pub struct LibraryState {
    pub library: Library,
    pub view: LibraryView,
    /// Selected index in the current list.
    pub selected: usize,
    /// Scroll offset for long lists.
    pub scroll_offset: usize,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            library: Library::default(),
            view: LibraryView::Artists,
            selected: 0,
            scroll_offset: 0,
        }
    }
}

impl LibraryState {
    /// Number of items in the current view.
    pub fn item_count(&self) -> usize {
        match &self.view {
            LibraryView::Artists => self.library.artists.len(),
            LibraryView::Albums { artist } => {
                self.library.albums_for(artist).len()
            }
            LibraryView::Tracks { artist, album_index } => {
                self.library
                    .albums_for(artist)
                    .get(*album_index)
                    .map(|a| a.tracks.len())
                    .unwrap_or(0)
            }
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        let count = self.item_count();
        if count > 0 && self.selected < count - 1 {
            self.selected += 1;
        }
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Drill into the selected item.
    pub fn enter(&mut self) {
        match &self.view {
            LibraryView::Artists => {
                let names = self.library.artist_names();
                if let Some(name) = names.get(self.selected) {
                    let artist = name.to_string();
                    self.view = LibraryView::Albums { artist };
                    self.selected = 0;
                    self.scroll_offset = 0;
                }
            }
            LibraryView::Albums { artist } => {
                let album_count = self.library.albums_for(artist).len();
                if self.selected < album_count {
                    self.view = LibraryView::Tracks {
                        artist: artist.clone(),
                        album_index: self.selected,
                    };
                    self.selected = 0;
                    self.scroll_offset = 0;
                }
            }
            LibraryView::Tracks { .. } => {
                // Handled by App — enqueue the selected track.
            }
        }
    }

    /// Go back up one level.
    pub fn back(&mut self) {
        match &self.view {
            LibraryView::Artists => {}
            LibraryView::Albums { .. } => {
                self.view = LibraryView::Artists;
                self.selected = 0;
                self.scroll_offset = 0;
            }
            LibraryView::Tracks { artist, .. } => {
                self.view = LibraryView::Albums {
                    artist: artist.clone(),
                };
                self.selected = 0;
                self.scroll_offset = 0;
            }
        }
    }

    /// Get the path of the currently selected track (only in Tracks view).
    pub fn selected_track_path(&self) -> Option<std::path::PathBuf> {
        if let LibraryView::Tracks { artist, album_index } = &self.view {
            self.library
                .albums_for(artist)
                .get(*album_index)
                .and_then(|album| album.tracks.get(self.selected))
                .map(|track| track.path.clone())
        } else {
            None
        }
    }

    /// Get all track paths for the selected album (only in Albums view).
    pub fn selected_album_tracks(&self) -> Vec<std::path::PathBuf> {
        if let LibraryView::Albums { artist } = &self.view {
            self.library
                .albums_for(artist)
                .get(self.selected)
                .map(|album| album.tracks.iter().map(|t| t.path.clone()).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

pub struct LibraryTab<'a> {
    pub state: &'a LibraryState,
}

impl<'a> Widget for LibraryTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Library ");

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 10 {
            return;
        }

        if self.state.library.scanning {
            Paragraph::new(Line::from(Span::styled(
                "Scanning music directory...",
                Style::default().fg(Color::Yellow),
            )))
            .centered()
            .render(inner, buf);
            return;
        }

        if self.state.library.all_tracks.is_empty() {
            let chunks = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(inner);

            Paragraph::new(vec![
                Line::from(Span::styled(
                    "No music found",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "Set music_directory in config or pass a file to play",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .centered()
            .render(chunks[1], buf);
            return;
        }

        // Breadcrumb header.
        let chunks = Layout::vertical([
            Constraint::Length(1), // breadcrumb
            Constraint::Min(0),   // list
        ])
        .split(inner);

        let breadcrumb = match &self.state.view {
            LibraryView::Artists => Line::from(vec![
                Span::styled("Artists", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!(" ({} artists, {} tracks)", self.state.library.artists.len(), self.state.library.all_tracks.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            LibraryView::Albums { artist } => Line::from(vec![
                Span::styled("Artists", Style::default().fg(Color::DarkGray)),
                Span::styled(" › ", Style::default().fg(Color::DarkGray)),
                Span::styled(artist.as_str(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            LibraryView::Tracks { artist, album_index } => {
                let album_name = self.state.library
                    .albums_for(artist)
                    .get(*album_index)
                    .map(|a| a.name.as_str())
                    .unwrap_or("?");
                Line::from(vec![
                    Span::styled("Artists", Style::default().fg(Color::DarkGray)),
                    Span::styled(" › ", Style::default().fg(Color::DarkGray)),
                    Span::styled(artist.as_str(), Style::default().fg(Color::DarkGray)),
                    Span::styled(" › ", Style::default().fg(Color::DarkGray)),
                    Span::styled(album_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ])
            }
        };
        Paragraph::new(breadcrumb).render(chunks[0], buf);

        // Item list.
        let visible_height = chunks[1].height as usize;
        let items: Vec<ListItem> = match &self.state.view {
            LibraryView::Artists => {
                self.state.library.artist_names().iter().enumerate().map(|(i, name)| {
                    let style = if i == self.state.selected {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let album_count = self.state.library.albums_for(name).len();
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {name}"), style),
                        Span::styled(
                            format!("  ({album_count} albums)"),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                }).collect()
            }
            LibraryView::Albums { artist } => {
                self.state.library.albums_for(artist).iter().enumerate().map(|(i, album)| {
                    let style = if i == self.state.selected {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {}", album.name), style),
                        Span::styled(
                            format!("  ({} tracks)", album.tracks.len()),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                }).collect()
            }
            LibraryView::Tracks { artist, album_index } => {
                if let Some(album) = self.state.library.albums_for(artist).get(*album_index) {
                    album.tracks.iter().enumerate().map(|(i, track)| {
                        let style = if i == self.state.selected {
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let num = track.track_number.map(|n| format!("{n:2}. ")).unwrap_or_else(|| "    ".into());
                        let dur = format_duration(track.duration);
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("  {num}{}", track.title), style),
                            Span::styled(
                                format!("  {dur}"),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    }).collect()
                } else {
                    vec![]
                }
            }
        };

        // Apply scroll offset for the visible window.
        let scroll = self.state.scroll_offset;
        let visible_items: Vec<ListItem> = items
            .into_iter()
            .skip(scroll)
            .take(visible_height)
            .collect();

        let list = List::new(visible_items);
        list.render(chunks[1], buf);
    }
}

fn format_duration(d: std::time::Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}
