use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Widget};
use std::path::PathBuf;

use crate::config::settings::{RepeatMode, Settings, ShuffleMode};

/// A setting item for display.
#[derive(Debug, Clone)]
struct SettingItem {
    label: String,
    value: String,
}

/// State for the Settings tab.
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub selected: usize,
    pub editing: bool,
    pub edit_buffer: String,
    pub status_message: Option<String>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            selected: 0,
            editing: false,
            edit_buffer: String::new(),
            status_message: None,
        }
    }
}

impl SettingsState {
    pub fn move_up(&mut self) {
        if !self.editing && self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self, max: usize) {
        if !self.editing && self.selected + 1 < max {
            self.selected += 1;
        }
    }

    /// Toggle or start editing the selected setting.
    pub fn toggle(&mut self, settings: &mut Settings) {
        if self.editing {
            // Save the edit.
            self.apply_edit(settings);
            return;
        }

        match self.selected {
            0 => {
                // Music directory: enter edit mode.
                self.editing = true;
                self.edit_buffer = settings
                    .music_directory
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
            }
            1 => {
                // Volume: enter edit mode.
                self.editing = true;
                self.edit_buffer = format!("{}", (settings.default_volume * 100.0).round() as u32);
            }
            2 => {
                // Seek step: enter edit mode.
                self.editing = true;
                self.edit_buffer = format!("{}", settings.seek_step_secs);
            }
            3 => {
                settings.repeat_mode = match settings.repeat_mode {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::Off,
                };
            }
            4 => {
                settings.shuffle = match settings.shuffle {
                    ShuffleMode::Off => ShuffleMode::On,
                    ShuffleMode::On => ShuffleMode::Off,
                };
            }
            5 => {
                settings.theme = match settings.theme.as_str() {
                    "default" => "catppuccin".into(),
                    "catppuccin" => "gruvbox".into(),
                    "gruvbox" => "default".into(),
                    _ => "default".into(),
                };
            }
            _ => {}
        }
    }

    /// Apply the current edit buffer to the corresponding setting.
    fn apply_edit(&mut self, settings: &mut Settings) {
        self.editing = false;
        match self.selected {
            0 => {
                let trimmed = self.edit_buffer.trim();
                if trimmed.is_empty() {
                    settings.music_directory = None;
                } else {
                    let expanded = if trimmed.starts_with('~') {
                        if let Some(home) = dirs_home() {
                            home.join(trimmed.strip_prefix("~/").unwrap_or(trimmed))
                        } else {
                            PathBuf::from(trimmed)
                        }
                    } else {
                        PathBuf::from(trimmed)
                    };
                    settings.music_directory = Some(expanded);
                }
                self.status_message = Some("Music directory updated".into());
            }
            1 => {
                if let Ok(pct) = self.edit_buffer.trim().parse::<u32>() {
                    settings.default_volume = (pct.min(100) as f32) / 100.0;
                    self.status_message = Some(format!("Volume set to {}%", pct.min(100)));
                } else {
                    self.status_message = Some("Invalid volume (enter 0-100)".into());
                }
            }
            2 => {
                if let Ok(secs) = self.edit_buffer.trim().parse::<u64>() {
                    settings.seek_step_secs = secs.max(1);
                    self.status_message = Some(format!("Seek step set to {}s", secs.max(1)));
                } else {
                    self.status_message = Some("Invalid seek step (enter seconds)".into());
                }
            }
            _ => {}
        }
        self.edit_buffer.clear();
    }

    /// Cancel editing without applying.
    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
    }

    /// Push a character to the edit buffer (when editing).
    pub fn edit_push(&mut self, ch: char) {
        if self.editing {
            self.edit_buffer.push(ch);
        }
    }

    /// Remove last character from the edit buffer.
    pub fn edit_pop(&mut self) {
        if self.editing {
            self.edit_buffer.pop();
        }
    }

    fn setting_items(settings: &Settings) -> Vec<SettingItem> {
        vec![
            SettingItem {
                label: "Music Directory".into(),
                value: settings
                    .music_directory
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(not set)".into()),
            },
            SettingItem {
                label: "Default Volume".into(),
                value: format!("{}%", (settings.default_volume * 100.0) as u8),
            },
            SettingItem {
                label: "Seek Step".into(),
                value: format!("{}s", settings.seek_step_secs),
            },
            SettingItem {
                label: "Repeat".into(),
                value: match settings.repeat_mode {
                    RepeatMode::Off => "Off",
                    RepeatMode::All => "All",
                    RepeatMode::One => "One",
                }
                .into(),
            },
            SettingItem {
                label: "Shuffle".into(),
                value: match settings.shuffle {
                    ShuffleMode::Off => "Off",
                    ShuffleMode::On => "On",
                }
                .into(),
            },
            SettingItem {
                label: "Theme".into(),
                value: settings.theme.clone(),
            },
        ]
    }

    pub fn item_count() -> usize {
        6
    }
}

/// Get the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

pub struct SettingsTab<'a> {
    pub state: &'a SettingsState,
    pub settings: &'a Settings,
}

impl<'a> Widget for SettingsTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = area;

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        let items = SettingsState::setting_items(self.settings);

        let chunks = Layout::vertical([
            Constraint::Length(1), // header
            Constraint::Min(0),   // list
            Constraint::Length(1), // status / help
        ])
        .split(inner);

        // Header.
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Settings",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  (Enter to toggle/edit, s to save)",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .render(chunks[0], buf);

        // Settings list.
        let list_items: Vec<ListItem> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = i == self.state.selected;
                let is_editing = is_selected && self.state.editing;

                let label_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::White)
                };

                let prefix = if is_selected { " > " } else { "   " };

                if is_editing {
                    let cursor = "_";
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{prefix}{:<20}", item.label), label_style),
                        Span::styled(
                            &self.state.edit_buffer,
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            cursor,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::SLOW_BLINK),
                        ),
                    ]))
                } else {
                    let value_style = if is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{prefix}{:<20}", item.label), label_style),
                        Span::styled(&item.value, value_style),
                    ]))
                }
            })
            .collect();

        let list = List::new(list_items);
        list.render(chunks[1], buf);

        // Status message.
        if let Some(ref msg) = self.state.status_message {
            Paragraph::new(Line::from(Span::styled(
                format!(" {msg}"),
                Style::default().fg(Color::Green),
            )))
            .render(chunks[2], buf);
        }
    }
}
