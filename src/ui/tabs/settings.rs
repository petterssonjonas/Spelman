use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};

use crate::config::settings::{RepeatMode, Settings, ShuffleMode};

/// A setting item for display.
#[derive(Debug, Clone)]
struct SettingItem {
    label: String,
    value: String,
    editable: bool,
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
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self, max: usize) {
        if self.selected + 1 < max {
            self.selected += 1;
        }
    }

    /// Toggle the selected setting (for boolean/enum settings).
    pub fn toggle(&mut self, settings: &mut Settings) {
        match self.selected {
            // 0 = music directory (needs text edit, skip toggle)
            // 1 = volume (needs text edit, skip toggle)
            // 2 = seek step (needs text edit, skip toggle)
            3 => {
                // Repeat mode: cycle Off → All → One → Off
                settings.repeat_mode = match settings.repeat_mode {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::Off,
                };
            }
            4 => {
                // Shuffle: toggle
                settings.shuffle = match settings.shuffle {
                    ShuffleMode::Off => ShuffleMode::On,
                    ShuffleMode::On => ShuffleMode::Off,
                };
            }
            5 => {
                // Theme: cycle through known themes
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

    fn setting_items(settings: &Settings) -> Vec<SettingItem> {
        vec![
            SettingItem {
                label: "Music Directory".into(),
                value: settings
                    .music_directory
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(not set)".into()),
                editable: true,
            },
            SettingItem {
                label: "Default Volume".into(),
                value: format!("{}%", (settings.default_volume * 100.0) as u8),
                editable: true,
            },
            SettingItem {
                label: "Seek Step".into(),
                value: format!("{}s", settings.seek_step_secs),
                editable: true,
            },
            SettingItem {
                label: "Repeat".into(),
                value: match settings.repeat_mode {
                    RepeatMode::Off => "Off",
                    RepeatMode::All => "All",
                    RepeatMode::One => "One",
                }
                .into(),
                editable: true,
            },
            SettingItem {
                label: "Shuffle".into(),
                value: match settings.shuffle {
                    ShuffleMode::Off => "Off",
                    ShuffleMode::On => "On",
                }
                .into(),
                editable: true,
            },
            SettingItem {
                label: "Theme".into(),
                value: settings.theme.clone(),
                editable: true,
            },
        ]
    }

    pub fn item_count() -> usize {
        6
    }
}

pub struct SettingsTab<'a> {
    pub state: &'a SettingsState,
    pub settings: &'a Settings,
}

impl<'a> Widget for SettingsTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Settings ");

        let inner = block.inner(area);
        block.render(area, buf);

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
                "Settings",
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
                let label_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(Color::White)
                };
                let value_style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let prefix = if is_selected { " > " } else { "   " };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{prefix}{:<20}", item.label), label_style),
                    Span::styled(&item.value, value_style),
                ]))
            })
            .collect();

        let list = List::new(list_items);
        list.render(chunks[1], buf);

        // Status message.
        if let Some(ref msg) = self.state.status_message {
            Paragraph::new(Line::from(Span::styled(
                msg.as_str(),
                Style::default().fg(Color::Green),
            )))
            .render(chunks[2], buf);
        }
    }
}
