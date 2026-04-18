use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone)]
pub enum Action {
    Move(Move, bool /* extend selection */),
    InsertChar(char),
    InsertNewline,
    Backspace,
    DeleteForward,
    Save,
    Quit,
    Copy,
    Cut,
    SelectAll,
    PasteHint,
    MouseDown(u16, u16),
    MouseDrag(u16, u16),
    MouseUp,
    WheelUp,
    WheelDown,
}

pub fn key_to_action(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let mv = |m: Move| Some(Action::Move(m, shift));
    match (key.code, ctrl) {
        (KeyCode::Left, _) => mv(Move::Left),
        (KeyCode::Right, _) => mv(Move::Right),
        (KeyCode::Up, _) => mv(Move::Up),
        (KeyCode::Down, _) => mv(Move::Down),
        (KeyCode::Home, _) => mv(Move::Home),
        (KeyCode::End, _) => mv(Move::End),
        (KeyCode::PageUp, _) => mv(Move::PageUp),
        (KeyCode::PageDown, _) => mv(Move::PageDown),
        (KeyCode::Enter, _) => Some(Action::InsertNewline),
        (KeyCode::Backspace, _) => Some(Action::Backspace),
        (KeyCode::Delete, _) => Some(Action::DeleteForward),
        (KeyCode::Tab, _) => Some(Action::InsertChar('\t')),
        (KeyCode::Char('s'), true) => Some(Action::Save),
        (KeyCode::Char('q'), true) => Some(Action::Quit),
        (KeyCode::Char('c'), true) => Some(Action::Copy),
        (KeyCode::Char('x'), true) => Some(Action::Cut),
        (KeyCode::Char('v'), true) => Some(Action::PasteHint),
        (KeyCode::Char('a'), true) => Some(Action::SelectAll),
        (KeyCode::Char(c), false) => Some(Action::InsertChar(c)),
        _ => None,
    }
}

/// Translate a crossterm mouse event into an [`Action`], in content-local
/// coordinates. Returns `None` for events outside the content area or for
/// button/event kinds we don't handle.
pub fn mouse_to_action(event: MouseEvent, content_area: Rect) -> Option<Action> {
    let inside = event.column >= content_area.x
        && event.column < content_area.x + content_area.width
        && event.row >= content_area.y
        && event.row < content_area.y + content_area.height;
    if !inside {
        return None;
    }
    let col = event.column - content_area.x;
    let row = event.row - content_area.y;
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => Some(Action::MouseDown(col, row)),
        MouseEventKind::Drag(MouseButton::Left) => Some(Action::MouseDrag(col, row)),
        MouseEventKind::Up(MouseButton::Left) => Some(Action::MouseUp),
        MouseEventKind::ScrollUp => Some(Action::WheelUp),
        MouseEventKind::ScrollDown => Some(Action::WheelDown),
        _ => None,
    }
}
