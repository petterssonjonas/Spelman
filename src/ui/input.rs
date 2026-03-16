use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
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
    None,
}

/// Poll for input events and translate to actions.
pub fn poll_input(timeout: Duration) -> std::io::Result<Action> {
    if !event::poll(timeout)? {
        return Ok(Action::None);
    }

    let ev = event::read()?;

    match ev {
        Event::Key(KeyEvent {
            code, modifiers, ..
        }) => Ok(match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => Action::Quit,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                Action::Quit
            }
            KeyCode::Char(' ') => {
                Action::AudioCmd(AudioCommand::TogglePlayPause)
            }
            KeyCode::Char('+') | KeyCode::Char('=') => Action::VolumeUp,
            KeyCode::Char('-') | KeyCode::Char('_') => Action::VolumeDown,
            KeyCode::Right | KeyCode::Char('l') => Action::SeekForward,
            KeyCode::Left | KeyCode::Char('h') => Action::SeekBackward,
            _ => Action::None,
        }),
        _ => Ok(Action::None),
    }
}
