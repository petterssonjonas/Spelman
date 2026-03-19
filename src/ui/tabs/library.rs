use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Widget};

use crate::library::types::{Album, Library, Track};

/// Sort / view mode for the top-level library browser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LibrarySortMode {
    Artists,
    Albums,
    Songs,
}

impl LibrarySortMode {
    pub fn next(self) -> Self {
        match self {
            Self::Artists => Self::Albums,
            Self::Albums => Self::Songs,
            Self::Songs => Self::Artists,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Artists => Self::Songs,
            Self::Albums => Self::Artists,
            Self::Songs => Self::Albums,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Artists => "Artists",
            Self::Albums => "Albums",
            Self::Songs => "Songs",
        }
    }
}

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
    /// Sort mode active at the top level.
    pub sort_mode: LibrarySortMode,
    /// Selected index in the current list.
    pub selected: usize,
    /// Scroll offset for long lists.
    pub scroll_offset: usize,
    /// Checked track paths (for playlist creation).
    pub checked: HashSet<std::path::PathBuf>,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            library: Library::default(),
            view: LibraryView::Artists,
            sort_mode: LibrarySortMode::Artists,
            selected: 0,
            scroll_offset: 0,
            checked: HashSet::new(),
        }
    }
}

impl LibraryState {
    /// Flat list of all albums across all artists: `(artist_name, album)`.
    pub fn all_albums(&self) -> Vec<(&str, &Album)> {
        self.library
            .artists
            .iter()
            .flat_map(|(artist, albums)| albums.iter().map(move |a| (artist.as_str(), a)))
            .collect()
    }

    /// Flat list of all tracks across all artists/albums: `(artist_name, track)`.
    pub fn all_tracks(&self) -> Vec<(&str, &Track)> {
        self.library
            .artists
            .iter()
            .flat_map(|(artist, albums)| {
                albums
                    .iter()
                    .flat_map(move |a| a.tracks.iter().map(move |t| (artist.as_str(), t)))
            })
            .collect()
    }

    /// Number of items in the current view.
    pub fn item_count(&self) -> usize {
        match &self.view {
            LibraryView::Artists => match self.sort_mode {
                LibrarySortMode::Artists => self.library.artists.len(),
                LibrarySortMode::Albums => self.all_albums().len(),
                LibrarySortMode::Songs => self.library.track_count(),
            },
            LibraryView::Albums { artist } => self.library.albums_for(artist).len(),
            LibraryView::Tracks { artist, album_index } => self
                .library
                .albums_for(artist)
                .get(*album_index)
                .map(|a| a.tracks.len())
                .unwrap_or(0),
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
            LibraryView::Artists => match self.sort_mode {
                LibrarySortMode::Artists => {
                    let names = self.library.artist_names();
                    if let Some(name) = names.get(self.selected) {
                        let artist = name.to_string();
                        self.view = LibraryView::Albums { artist };
                        self.selected = 0;
                        self.scroll_offset = 0;
                    }
                }
                LibrarySortMode::Albums => {
                    // Drill into an album from the flat albums list.
                    let albums = self.all_albums();
                    if let Some((artist_name, album)) = albums.get(self.selected) {
                        let artist = artist_name.to_string();
                        // Find the index of this album in the artist's album list.
                        let album_index = self
                            .library
                            .albums_for(&artist)
                            .iter()
                            .position(|a| std::ptr::eq(a, *album))
                            .unwrap_or(0);
                        self.view = LibraryView::Tracks { artist, album_index };
                        self.selected = 0;
                        self.scroll_offset = 0;
                    }
                }
                LibrarySortMode::Songs => {
                    // Enter on a song plays it — handled by App via selected_flat_track_path.
                }
            },
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

    /// Get the path of the currently selected track when in Songs flat-list mode.
    pub fn selected_flat_song_path(&self) -> Option<std::path::PathBuf> {
        if matches!((&self.view, self.sort_mode), (LibraryView::Artists, LibrarySortMode::Songs)) {
            self.all_tracks()
                .get(self.selected)
                .map(|(_, t)| t.path.clone())
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

    /// Toggle the checkbox for the currently selected item.
    /// In Tracks view: toggles one track. In Albums view: toggles all tracks in the album.
    /// In Artists view: behaviour depends on sort_mode.
    pub fn toggle_selected(&mut self) {
        match &self.view {
            LibraryView::Tracks { artist, album_index } => {
                if let Some(track) = self
                    .library
                    .albums_for(artist)
                    .get(*album_index)
                    .and_then(|album| album.tracks.get(self.selected))
                {
                    let path = track.path.clone();
                    if !self.checked.remove(&path) {
                        self.checked.insert(path);
                    }
                }
            }
            LibraryView::Albums { artist } => {
                if let Some(album) = self.library.albums_for(artist).get(self.selected) {
                    let paths: Vec<_> = album.tracks.iter().map(|t| t.path.clone()).collect();
                    let all_checked = paths.iter().all(|p| self.checked.contains(p));
                    if all_checked {
                        for p in &paths {
                            self.checked.remove(p);
                        }
                    } else {
                        for p in paths {
                            self.checked.insert(p);
                        }
                    }
                }
            }
            LibraryView::Artists => match self.sort_mode {
                LibrarySortMode::Artists => {
                    let names = self.library.artist_names();
                    if let Some(name) = names.get(self.selected) {
                        let paths: Vec<_> = self
                            .library
                            .albums_for(name)
                            .iter()
                            .flat_map(|a| a.tracks.iter().map(|t| t.path.clone()))
                            .collect();
                        let all_checked = paths.iter().all(|p| self.checked.contains(p));
                        if all_checked {
                            for p in &paths {
                                self.checked.remove(p);
                            }
                        } else {
                            for p in paths {
                                self.checked.insert(p);
                            }
                        }
                    }
                }
                LibrarySortMode::Albums => {
                    let albums = self.all_albums();
                    if let Some((_, album)) = albums.get(self.selected) {
                        let paths: Vec<_> = album.tracks.iter().map(|t| t.path.clone()).collect();
                        let all_checked = paths.iter().all(|p| self.checked.contains(p));
                        if all_checked {
                            for p in &paths {
                                self.checked.remove(p);
                            }
                        } else {
                            for p in paths {
                                self.checked.insert(p);
                            }
                        }
                    }
                }
                LibrarySortMode::Songs => {
                    let tracks = self.all_tracks();
                    if let Some((_, track)) = tracks.get(self.selected) {
                        let path = track.path.clone();
                        if !self.checked.remove(&path) {
                            self.checked.insert(path);
                        }
                    }
                }
            },
        }
    }

    /// Take all checked paths and clear the selection.
    pub fn take_checked_paths(&mut self) -> Vec<std::path::PathBuf> {
        let paths: Vec<_> = self.checked.drain().collect();
        paths
    }

    /// Whether a specific track path is checked.
    pub fn is_checked(&self, path: &std::path::Path) -> bool {
        self.checked.contains(path)
    }

    /// Number of checked items.
    pub fn checked_count(&self) -> usize {
        self.checked.len()
    }
}

pub struct LibraryTab<'a> {
    pub state: &'a LibraryState,
    pub playlist_key: &'a str,
}

impl<'a> Widget for LibraryTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 10 {
            return;
        }

        if self.state.library.scanning {
            Paragraph::new(Line::from(Span::styled(
                "Scanning music directory...",
                Style::default().fg(Color::Yellow),
            )))
            .centered()
            .render(area, buf);
            return;
        }

        if self.state.library.track_count() == 0 {
            let chunks = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(area);

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

        // Layout: top spacer | breadcrumb | list | bottom hints
        let chunks = Layout::vertical([
            Constraint::Length(1), // spacer under tab bar
            Constraint::Length(1), // breadcrumb
            Constraint::Min(0),    // list
            Constraint::Length(1), // bottom hints
        ])
        .split(area);

        // Add 2-column left padding to breadcrumb and list areas.
        let padded = |r: Rect| -> Rect {
            Rect {
                x: r.x + 2,
                y: r.y,
                width: r.width.saturating_sub(2),
                height: r.height,
            }
        };

        let checked_count = self.state.checked_count();
        let checked_suffix = if checked_count > 0 {
            format!("  [{checked_count} selected]")
        } else {
            String::new()
        };

        let breadcrumb = match &self.state.view {
            LibraryView::Artists => {
                let mode = self.state.sort_mode;
                let count = match mode {
                    LibrarySortMode::Artists => self.state.library.artists.len(),
                    LibrarySortMode::Albums => self.state.all_albums().len(),
                    LibrarySortMode::Songs => self.state.library.track_count(),
                };
                let item_label = mode.label().to_lowercase();
                Line::from(vec![
                    Span::styled("\u{2190} ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        mode.label(),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" \u{2192}", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!("  ({count} {item_label})"),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(checked_suffix, Style::default().fg(Color::Yellow)),
                ])
            }
            LibraryView::Albums { artist } => Line::from(vec![
                Span::styled("Artists", Style::default().fg(Color::DarkGray)),
                Span::styled(" \u{203a} ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    artist.as_str(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(checked_suffix, Style::default().fg(Color::Yellow)),
            ]),
            LibraryView::Tracks { artist, album_index } => {
                let album_name = self
                    .state
                    .library
                    .albums_for(artist)
                    .get(*album_index)
                    .map(|a| a.name.as_str())
                    .unwrap_or("?");
                Line::from(vec![
                    Span::styled("Artists", Style::default().fg(Color::DarkGray)),
                    Span::styled(" \u{203a} ", Style::default().fg(Color::DarkGray)),
                    Span::styled(artist.as_str(), Style::default().fg(Color::DarkGray)),
                    Span::styled(" \u{203a} ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        album_name,
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(checked_suffix, Style::default().fg(Color::Yellow)),
                ])
            }
        };
        Paragraph::new(breadcrumb).render(padded(chunks[1]), buf);

        // Item list.
        let visible_height = chunks[2].height as usize;
        let items: Vec<ListItem> = match &self.state.view {
            LibraryView::Artists => match self.state.sort_mode {
                LibrarySortMode::Artists => self
                    .state
                    .library
                    .artist_names()
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let style = if i == self.state.selected {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let all_tracks: Vec<_> = self
                            .state
                            .library
                            .albums_for(name)
                            .iter()
                            .flat_map(|a| a.tracks.iter())
                            .collect();
                        let any_checked =
                            all_tracks.iter().any(|t| self.state.checked.contains(&t.path));
                        let all_checked = !all_tracks.is_empty()
                            && all_tracks.iter().all(|t| self.state.checked.contains(&t.path));
                        let checkbox = if all_checked {
                            "[*] "
                        } else if any_checked {
                            "[-] "
                        } else {
                            "[ ] "
                        };
                        let album_count = self.state.library.albums_for(name).len();
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                checkbox,
                                Style::default().fg(if any_checked {
                                    Color::Green
                                } else {
                                    Color::DarkGray
                                }),
                            ),
                            Span::styled(name.to_string(), style),
                            Span::styled(
                                format!("  ({album_count} albums)"),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    })
                    .collect(),
                LibrarySortMode::Albums => self
                    .state
                    .all_albums()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (artist_name, album))| {
                        let style = if i == self.state.selected {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let any_checked =
                            album.tracks.iter().any(|t| self.state.checked.contains(&t.path));
                        let all_checked = !album.tracks.is_empty()
                            && album.tracks.iter().all(|t| self.state.checked.contains(&t.path));
                        let checkbox = if all_checked {
                            "[*] "
                        } else if any_checked {
                            "[-] "
                        } else {
                            "[ ] "
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                checkbox,
                                Style::default().fg(if any_checked {
                                    Color::Green
                                } else {
                                    Color::DarkGray
                                }),
                            ),
                            Span::styled(album.name.clone(), style),
                            Span::styled(
                                format!(" - {artist_name}"),
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::styled(
                                format!("  ({} tracks)", album.tracks.len()),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    })
                    .collect(),
                LibrarySortMode::Songs => self
                    .state
                    .all_tracks()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (artist_name, track))| {
                        let style = if i == self.state.selected {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let checked = self.state.checked.contains(&track.path);
                        let checkbox = if checked { "[*] " } else { "[ ] " };
                        let dur = format_duration(track.duration);
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                checkbox,
                                Style::default().fg(if checked {
                                    Color::Green
                                } else {
                                    Color::DarkGray
                                }),
                            ),
                            Span::styled(track.title.clone(), style),
                            Span::styled(
                                format!(" - {artist_name}"),
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::styled(
                                format!("  {dur}"),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    })
                    .collect(),
            },
            LibraryView::Albums { artist } => self
                .state
                .library
                .albums_for(artist)
                .iter()
                .enumerate()
                .map(|(i, album)| {
                    let style = if i == self.state.selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let any_checked =
                        album.tracks.iter().any(|t| self.state.checked.contains(&t.path));
                    let all_checked = !album.tracks.is_empty()
                        && album.tracks.iter().all(|t| self.state.checked.contains(&t.path));
                    let checkbox = if all_checked {
                        "[*] "
                    } else if any_checked {
                        "[-] "
                    } else {
                        "[ ] "
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            checkbox,
                            Style::default().fg(if any_checked {
                                Color::Green
                            } else {
                                Color::DarkGray
                            }),
                        ),
                        Span::styled(album.name.clone(), style),
                        Span::styled(
                            format!("  ({} tracks)", album.tracks.len()),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect(),
            LibraryView::Tracks { artist, album_index } => {
                if let Some(album) = self.state.library.albums_for(artist).get(*album_index) {
                    album
                        .tracks
                        .iter()
                        .enumerate()
                        .map(|(i, track)| {
                            let style = if i == self.state.selected {
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            let checked = self.state.checked.contains(&track.path);
                            let checkbox = if checked { "[*] " } else { "[ ] " };
                            let num = track
                                .track_number
                                .map(|n| format!("{n:2}. "))
                                .unwrap_or_else(|| "    ".into());
                            let dur = format_duration(track.duration);
                            ListItem::new(Line::from(vec![
                                Span::styled(
                                    checkbox,
                                    Style::default().fg(if checked {
                                        Color::Green
                                    } else {
                                        Color::DarkGray
                                    }),
                                ),
                                Span::styled(format!("{num}{}", track.title), style),
                                Span::styled(
                                    format!("  {dur}"),
                                    Style::default().fg(Color::DarkGray),
                                ),
                            ]))
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
        };

        // Apply scroll offset for the visible window.
        let scroll = self.state.scroll_offset;
        let visible_items: Vec<ListItem> = items.into_iter().skip(scroll).take(visible_height).collect();

        let list = List::new(visible_items);
        list.render(padded(chunks[2]), buf);

        // Bottom hints row.
        let hint = format!("  {}:  Save as playlist", self.playlist_key);
        Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray)))
            .centered()
            .render(chunks[3], buf);
    }
}

fn format_duration(d: std::time::Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}
