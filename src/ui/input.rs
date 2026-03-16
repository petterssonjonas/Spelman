use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::time::Duration;

use crate::util::channels::AudioCommand;

/// Actions the input handler can produce.
pub enum Action {
    AudioCmd(AudioCommand),
    Quit,
    VolumeUp,
    VolumeDown,
    SeekForward,
    SeekBackward,
    /// Switch to tab (0-indexed).
    SwitchTab(usize),
    /// Next track in queue.
    NextTrack,
    /// Previous track in queue.
    PrevTrack,
    /// Mouse click at (column, row).
    MouseClick { col: u16, row: u16 },
    /// Scroll up.
    ScrollUp,
    /// Scroll down.
    ScrollDown,
    /// Navigate into selection (library drill-down, or enqueue).
    Enter,
    /// Navigate back (library drill-up).
    Back,
    /// A character typed (for search input, etc).
    Char(char),
    /// Backspace (for search input).
    Backspace,
    None,
}

/// Enable mouse capture (call once at startup).
pub fn enable_mouse() -> std::io::Result<()> {
    crossterm::execute!(std::io::stdout(), EnableMouseCapture)
}

/// Disable mouse capture (call at shutdown).
pub fn disable_mouse() -> std::io::Result<()> {
    crossterm::execute!(std::io::stdout(), DisableMouseCapture)
}

/// Poll for input events and translate to actions.
/// `search_active` indicates if the search tab is focused (chars go to search box).
pub fn poll_input(timeout: Duration) -> std::io::Result<Action> {
    if !event::poll(timeout)? {
        return Ok(Action::None);
    }

    let ev = event::read()?;

    match ev {
        Event::Key(KeyEvent {
            code, modifiers, ..
        }) => Ok(match code {
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                Action::Quit
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => Action::Quit,
            KeyCode::Char(' ') => {
                Action::AudioCmd(AudioCommand::TogglePlayPause)
            }
            KeyCode::Char('+') | KeyCode::Char('=') => Action::VolumeUp,
            KeyCode::Char('-') | KeyCode::Char('_') => Action::VolumeDown,
            KeyCode::Right | KeyCode::Char('l') => Action::SeekForward,
            KeyCode::Left | KeyCode::Char('h') => Action::SeekBackward,
            // Tab switching: 1-6.
            KeyCode::Char('1') => Action::SwitchTab(0),
            KeyCode::Char('2') => Action::SwitchTab(1),
            KeyCode::Char('3') => Action::SwitchTab(2),
            KeyCode::Char('4') => Action::SwitchTab(3),
            KeyCode::Char('5') => Action::SwitchTab(4),
            KeyCode::Char('6') => Action::SwitchTab(5),
            KeyCode::Tab => Action::SwitchTab(usize::MAX),
            KeyCode::BackTab => Action::SwitchTab(usize::MAX - 1),
            // Queue controls.
            KeyCode::Char('n') => Action::NextTrack,
            KeyCode::Char('p') => Action::PrevTrack,
            // Navigation.
            KeyCode::Enter => Action::Enter,
            KeyCode::Esc => Action::Back,
            KeyCode::Backspace => Action::Backspace,
            KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown,
            KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp,
            // Pass through other characters for search/settings.
            KeyCode::Char(ch) => Action::Char(ch),
            _ => Action::None,
        }),
        Event::Mouse(MouseEvent { kind, column, row, .. }) => {
            Ok(match kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    Action::MouseClick { col: column, row }
                }
                MouseEventKind::ScrollUp => Action::ScrollUp,
                MouseEventKind::ScrollDown => Action::ScrollDown,
                _ => Action::None,
            })
        }
        _ => Ok(Action::None),
    }
}
