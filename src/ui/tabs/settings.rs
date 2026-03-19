use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Widget};
use std::path::PathBuf;

use crate::config::settings::{BindableAction, KeyBindings, RepeatMode, Settings, ShuffleMode};

/// State for the Settings tab.
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub selected: usize,
    pub editing: bool,
    pub edit_buffer: String,
    pub status_message: Option<String>,
    /// Waiting for user to press a key for rebinding.
    pub rebinding: bool,
    /// Which action is being rebound.
    pub rebind_action: Option<BindableAction>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            selected: 0,
            editing: false,
            edit_buffer: String::new(),
            status_message: None,
            rebinding: false,
            rebind_action: None,
        }
    }
}

/// Number of non-keybinding settings items.
const BASE_ITEM_COUNT: usize = 6;
/// Separator row between settings and keybindings.
const SEPARATOR_COUNT: usize = 1;

impl SettingsState {
    pub fn move_up(&mut self) {
        if !self.editing && !self.rebinding && self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self, max: usize) {
        if !self.editing && !self.rebinding && self.selected + 1 < max {
            self.selected += 1;
        }
    }

    /// Toggle or start editing the selected setting.
    pub fn toggle(&mut self, settings: &mut Settings) {
        if self.editing {
            self.apply_edit(settings);
            return;
        }

        if self.selected < BASE_ITEM_COUNT {
            // Regular settings.
            match self.selected {
                0 => {
                    self.editing = true;
                    self.edit_buffer = settings
                        .music_directory
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                }
                1 => {
                    self.editing = true;
                    self.edit_buffer = format!("{}", (settings.default_volume * 100.0).round() as u32);
                }
                2 => {
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
        } else {
            // Keybinding row: start rebinding.
            let kb_index = self.selected - BASE_ITEM_COUNT - SEPARATOR_COUNT;
            if let Some(&action) = BindableAction::ALL.get(kb_index) {
                self.rebinding = true;
                self.rebind_action = Some(action);
                self.status_message = Some(format!("Press a key for '{}'...", action.label()));
            }
        }
    }

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

    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.rebinding = false;
        self.rebind_action = None;
        self.edit_buffer.clear();
    }

    pub fn edit_push(&mut self, ch: char) {
        if self.editing {
            self.edit_buffer.push(ch);
        }
    }

    pub fn edit_pop(&mut self) {
        if self.editing {
            self.edit_buffer.pop();
        }
    }

    /// Total number of rows in the settings list (settings + separator + keybindings).
    pub fn item_count() -> usize {
        BASE_ITEM_COUNT + SEPARATOR_COUNT + BindableAction::ALL.len()
    }

    /// Compute scroll offset for a given visible height (must match render logic).
    pub fn scroll_offset(&self, visible_height: usize) -> usize {
        if self.selected >= visible_height {
            self.selected - visible_height + 1
        } else {
            0
        }
    }
}

fn dirs_home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

pub struct SettingsTab<'a> {
    pub state: &'a SettingsState,
    pub settings: &'a Settings,
    pub focus_tabbar: bool,
}

impl<'a> Widget for SettingsTab<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 20 {
            return;
        }

        let chunks = Layout::vertical([
            Constraint::Length(1), // header
            Constraint::Min(0),   // list
            Constraint::Length(1), // status / help
        ])
        .split(area);

        // Header.
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Settings",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  (Enter to toggle/edit, auto-saved)",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .render(chunks[0], buf);

        // Build all rows.
        let mut list_items: Vec<ListItem> = Vec::new();

        // Base settings.
        let base_items = base_setting_items(self.settings);
        for (i, item) in base_items.iter().enumerate() {
            let is_selected = !self.focus_tabbar && i == self.state.selected;
            let is_editing = is_selected && self.state.editing;
            list_items.push(render_setting_row(
                &item.0, &item.1, is_selected, is_editing,
                &self.state.edit_buffer,
            ));
        }

        // Separator.
        let sep_selected = !self.focus_tabbar && self.state.selected == BASE_ITEM_COUNT;
        let sep_style = if sep_selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        list_items.push(ListItem::new(Line::from(Span::styled(
            "   ── Keybindings ──────────────────────────",
            sep_style,
        ))));

        // Keybinding rows.
        for (i, &action) in BindableAction::ALL.iter().enumerate() {
            let row_idx = BASE_ITEM_COUNT + SEPARATOR_COUNT + i;
            let is_selected = !self.focus_tabbar && row_idx == self.state.selected;
            let is_rebinding = is_selected && self.state.rebinding;

            let keys = self.settings.keybindings.keys_for(action);
            let keys_display = if is_rebinding {
                "Press a key...".to_string()
            } else if keys.is_empty() {
                "(unbound)".to_string()
            } else {
                keys.join(", ")
            };

            let label_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(Color::White)
            };

            let value_style = if is_rebinding {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK)
            } else if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let default_keys = KeyBindings::default_keys_for(action);
            let default_display = format!("  (Default: {})", default_keys.join(", "));

            let prefix = if is_selected { " > " } else { "   " };
            list_items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{prefix}{:<24}", action.label()), label_style),
                Span::styled(keys_display, value_style),
                Span::styled(default_display, Style::default().fg(Color::DarkGray)),
            ])));
        }

        // Apply scroll — keep selected item visible.
        let visible_height = chunks[1].height as usize;
        let scroll = if self.state.selected >= visible_height {
            self.state.selected - visible_height + 1
        } else {
            0
        };

        let visible_items: Vec<ListItem> = list_items
            .into_iter()
            .skip(scroll)
            .take(visible_height)
            .collect();

        List::new(visible_items).render(chunks[1], buf);

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

fn base_setting_items(settings: &Settings) -> Vec<(String, String)> {
    vec![
        (
            "Music Directory".into(),
            settings
                .music_directory
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(not set)".into()),
        ),
        (
            "Default Volume".into(),
            format!("{}%", (settings.default_volume * 100.0) as u8),
        ),
        (
            "Seek Step".into(),
            format!("{}s", settings.seek_step_secs),
        ),
        (
            "Repeat".into(),
            match settings.repeat_mode {
                RepeatMode::Off => "Off",
                RepeatMode::All => "All",
                RepeatMode::One => "One",
            }
            .into(),
        ),
        (
            "Shuffle".into(),
            match settings.shuffle {
                ShuffleMode::Off => "Off",
                ShuffleMode::On => "On",
            }
            .into(),
        ),
        ("Theme".into(), settings.theme.clone()),
    ]
}

fn render_setting_row<'a>(
    label: &str,
    value: &str,
    is_selected: bool,
    is_editing: bool,
    edit_buffer: &str,
) -> ListItem<'a> {
    let label_style = if is_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(Color::White)
    };

    let prefix = if is_selected { " > " } else { "   " };

    if is_editing {
        ListItem::new(Line::from(vec![
            Span::styled(format!("{prefix}{:<24}", label), label_style),
            Span::styled(
                edit_buffer.to_string(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "_",
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
            Span::styled(format!("{prefix}{:<24}", label), label_style),
            Span::styled(value.to_string(), value_style),
        ]))
    }
}
