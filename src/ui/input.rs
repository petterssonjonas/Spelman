use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::collections::HashMap;
use std::time::Duration;

use crate::config::settings::BindableAction;

/// Actions the input handler can produce.
pub enum Action {
    /// A keybinding matched a configured action.
    Bound(BindableAction),
    /// Mouse click at (column, row).
    MouseClick { col: u16, row: u16 },
    /// Mouse scroll up at position.
    MouseScrollUp { col: u16, row: u16 },
    /// Mouse scroll down at position.
    MouseScrollDown { col: u16, row: u16 },
    /// A character typed (for search input, etc).
    Char(char),
    /// Mouse moved to position (for hover effects).
    MouseMove { col: u16, row: u16 },
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
///
/// When `text_capture` is true, all alphanumeric and symbol keys are routed
/// as `Char(ch)` instead of their normal bindings. Only Ctrl+C, Enter, Esc,
/// Backspace, and arrow keys keep special meaning.
pub fn poll_input(
    timeout: Duration,
    text_capture: bool,
    key_lookup: &HashMap<KeyCode, BindableAction>,
) -> std::io::Result<Action> {
    if !event::poll(timeout)? {
        return Ok(Action::None);
    }

    let ev = event::read()?;

    match ev {
        Event::Key(KeyEvent {
            code, modifiers, ..
        }) => {
            // Ctrl+C always quits (hardcoded safety).
            if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Action::Bound(BindableAction::Quit));
            }

            // In text capture mode, route most keys as Char.
            if text_capture {
                return Ok(match code {
                    KeyCode::Enter => Action::Bound(BindableAction::Enter),
                    KeyCode::Esc => Action::Bound(BindableAction::Back),
                    KeyCode::Backspace => Action::Bound(BindableAction::Backspace),
                    KeyCode::Down => Action::Bound(BindableAction::ScrollDown),
                    KeyCode::Up => Action::Bound(BindableAction::ScrollUp),
                    KeyCode::Tab => Action::Bound(BindableAction::TabNext),
                    KeyCode::BackTab => Action::Bound(BindableAction::TabPrev),
                    KeyCode::Char(ch) => Action::Char(ch),
                    _ => Action::None,
                });
            }

            // Normal mode: look up the key in the bindings.
            if let Some(&action) = key_lookup.get(&code) {
                return Ok(Action::Bound(action));
            }

            // Unbound character keys pass through for context handling.
            if let KeyCode::Char(ch) = code {
                return Ok(Action::Char(ch));
            }

            Ok(Action::None)
        }
        Event::Mouse(MouseEvent { kind, column, row, .. }) => {
            Ok(match kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    Action::MouseClick { col: column, row }
                }
                MouseEventKind::Down(MouseButton::Right) => Action::Bound(BindableAction::Back),
                MouseEventKind::ScrollUp => Action::MouseScrollUp { col: column, row },
                MouseEventKind::ScrollDown => Action::MouseScrollDown { col: column, row },
                MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                    Action::MouseMove { col: column, row }
                }
                _ => Action::None,
            })
        }
        _ => Ok(Action::None),
    }
}
