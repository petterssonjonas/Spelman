use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Widget};
use std::path::PathBuf;

use crate::playlist::playlist::Playlist;

/// State for the Playlists tab.
#[derive(Debug, Clone, Default)]
pub struct PlaylistsState {
    pub playlists: Vec<Playlist>,
    /// Selected playlist index.
    pub selected: usize,
    /// If Some, we're viewing tracks inside a playlist.
    pub viewing: Option<usize>,
    /// Selected track index when viewing a playlist.
    pub track_selected: usize,
    /// Scroll offset.
    pub scroll_offset: usize,
    /// Status message (e.g. "Playlist deleted").
    pub status: Option<String>,
}

impl PlaylistsState {
    /// Reload playlists from disk.
    pub fn reload(&mut self) {
        self.playlists = crate::playlist::playlist::PlaylistManager::load_all();
    }

    pub fn move_down(&mut self) {
        if let Some(pl_idx) = self.viewing {
            let count = self.playlists.get(pl_idx).map_or(0, |p| p.tracks.len());
            if count > 0 && self.track_selected < count - 1 {
                self.track_selected += 1;
            }
        } else {
            let count = self.playlists.len();
            if count > 0 && self.selected < count - 1 {
                self.selected += 1;
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.viewing.is_some() {
            if self.track_selected > 0 {
                self.track_selected -= 1;
            }
        } else if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Enter a playlist to view its tracks.
    pub fn enter(&mut self) {
        if self.viewing.is_none() && !self.playlists.is_empty() {
            self.viewing = Some(self.selected);
            self.track_selected = 0;
            self.scroll_offset = 0;
        }
    }

    /// Go back to the playlist list.
    pub fn back(&mut self) {
        if self.viewing.is_some() {
            self.viewing = None;
            self.track_selected = 0;
            self.scroll_offset = 0;
        }
    }

    /// Get the path of the currently selected track (when viewing a playlist).
    pub fn selected_track_path(&self) -> Option<PathBuf> {
        let pl_idx = self.viewing?;
        self.playlists
            .get(pl_idx)
            .and_then(|pl| pl.tracks.get(self.track_selected))
            .cloned()
    }

    /// Get all track paths for the currently selected playlist.
    pub fn selected_playlist_tracks(&self) -> Vec<PathBuf> {
        if let Some(pl) = self.playlists.get(self.selected) {
            pl.tracks.clone()
        } else {
            Vec::new()
        }
    }

    /// Delete the currently selected playlist.
    pub fn delete_selected(&mut self) {
        if let Some(pl) = self.playlists.get(self.selected) {
            let name = pl.name.clone();
            if let Err(e) = crate::playlist::playlist::PlaylistManager::delete(&name) {
                self.status = Some(format!("Delete failed: {e}"));
            } else {
                self.status = Some(format!("Deleted '{name}'"));
                self.reload();
                if self.selected >= self.playlists.len() && !self.playlists.is_empty() {
                    self.selected = self.playlists.len() - 1;
                }
            }
        }
    }
}

pub struct PlaylistsTab<'a> {
    pub state: &'a PlaylistsState,
}

impl<'a> Widget for PlaylistsTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 10 {
            return;
        }

        if self.state.playlists.is_empty() {
            let chunks = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area);

            Paragraph::new(vec![
                Line::from(Span::styled(
                    "No playlists saved",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "Press 'a' in Playing tab to save queue as playlist",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "Or select tracks in Library with Space and press 'a'",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .centered()
            .render(chunks[1], buf);
            return;
        }

        // Breadcrumb header.
        let chunks = Layout::vertical([
            Constraint::Length(1), // breadcrumb / status
            Constraint::Min(0),   // list
        ])
        .split(area);

        if let Some(pl_idx) = self.state.viewing {
            // Viewing tracks inside a playlist.
            let pl_name = self.state.playlists.get(pl_idx)
                .map(|p| p.name.as_str())
                .unwrap_or("?");
            let breadcrumb = Line::from(vec![
                Span::styled("Playlists", Style::default().fg(Color::DarkGray)),
                Span::styled(" › ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    pl_name,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]);
            Paragraph::new(breadcrumb).render(chunks[0], buf);

            if let Some(pl) = self.state.playlists.get(pl_idx) {
                let visible = chunks[1].height as usize;
                let items: Vec<ListItem> = pl.tracks.iter().enumerate().map(|(i, path)| {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "?".into());
                    let style = if i == self.state.track_selected {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(Line::from(Span::styled(format!("  {name}"), style)))
                }).collect();

                let scroll = self.state.scroll_offset;
                let visible_items: Vec<ListItem> = items.into_iter().skip(scroll).take(visible).collect();
                List::new(visible_items).render(chunks[1], buf);
            }
        } else {
            // Listing playlists.
            let status_line = if let Some(ref status) = self.state.status {
                Line::from(Span::styled(status.as_str(), Style::default().fg(Color::Yellow)))
            } else {
                Line::from(vec![
                    Span::styled("Playlists", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!(" ({} saved)", self.state.playlists.len()),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            };
            Paragraph::new(status_line).render(chunks[0], buf);

            let visible = chunks[1].height as usize;
            let items: Vec<ListItem> = self.state.playlists.iter().enumerate().map(|(i, pl)| {
                let style = if i == self.state.selected {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {}", pl.name), style),
                    Span::styled(
                        format!("  ({} tracks)", pl.tracks.len()),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            }).collect();

            let scroll = self.state.scroll_offset;
            let visible_items: Vec<ListItem> = items.into_iter().skip(scroll).take(visible).collect();
            List::new(visible_items).render(chunks[1], buf);
        }
    }
}
