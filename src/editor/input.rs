use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub enum Action {
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveHome,
    MoveEnd,
    MovePageUp,
    MovePageDown,
    InsertChar(char),
    InsertNewline,
    Backspace,
    DeleteForward,
    Save,
    Quit,
}

pub fn key_to_action(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match (key.code, ctrl) {
        (KeyCode::Left, _) => Some(Action::MoveLeft),
        (KeyCode::Right, _) => Some(Action::MoveRight),
        (KeyCode::Up, _) => Some(Action::MoveUp),
        (KeyCode::Down, _) => Some(Action::MoveDown),
        (KeyCode::Home, _) => Some(Action::MoveHome),
        (KeyCode::End, _) => Some(Action::MoveEnd),
        (KeyCode::PageUp, _) => Some(Action::MovePageUp),
        (KeyCode::PageDown, _) => Some(Action::MovePageDown),
        (KeyCode::Enter, _) => Some(Action::InsertNewline),
        (KeyCode::Backspace, _) => Some(Action::Backspace),
        (KeyCode::Delete, _) => Some(Action::DeleteForward),
        (KeyCode::Tab, _) => Some(Action::InsertChar('\t')),
        (KeyCode::Char('s'), true) => Some(Action::Save),
        (KeyCode::Char('q'), true) => Some(Action::Quit),
        (KeyCode::Char(c), false) => Some(Action::InsertChar(c)),
        _ => None,
    }
}
