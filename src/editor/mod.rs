pub mod cursor;
pub mod input;

use anyhow::Result;
use unicode_width::UnicodeWidthChar;

use crate::document::Document;
use cursor::Cursor;
use input::Action;

pub struct Editor {
    pub document: Document,
    pub cursor: Cursor,
    pub viewport_top: usize,
    pub viewport_left: usize,
    pub viewport_height: u16,
    pub viewport_width: u16,
    pub status: Option<String>,
}

pub enum ActionOutcome {
    Continue,
    Saved,
    Quit,
}

impl Editor {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            cursor: Cursor::default(),
            viewport_top: 0,
            viewport_left: 0,
            viewport_height: 0,
            viewport_width: 0,
            status: None,
        }
    }

    pub fn apply(&mut self, action: Action) -> Result<ActionOutcome> {
        self.status = None;
        match action {
            Action::MoveLeft => self.cursor.move_left(&self.document.rope),
            Action::MoveRight => self.cursor.move_right(&self.document.rope),
            Action::MoveUp => self.cursor.move_up(&self.document.rope),
            Action::MoveDown => self.cursor.move_down(&self.document.rope),
            Action::MoveHome => self.cursor.move_home(),
            Action::MoveEnd => self.cursor.move_end(&self.document.rope),
            Action::MovePageUp => {
                let step = self.viewport_height.max(1) as usize;
                for _ in 0..step {
                    self.cursor.move_up(&self.document.rope);
                }
            }
            Action::MovePageDown => {
                let step = self.viewport_height.max(1) as usize;
                for _ in 0..step {
                    self.cursor.move_down(&self.document.rope);
                }
            }
            Action::InsertChar(c) => {
                let idx = self.cursor.char_offset(&self.document.rope);
                self.document.insert_char(idx, c);
                self.cursor.move_right(&self.document.rope);
            }
            Action::InsertNewline => {
                let idx = self.cursor.char_offset(&self.document.rope);
                self.document.insert_char(idx, '\n');
                self.cursor.line += 1;
                self.cursor.col = 0;
                self.cursor.desired_col = 0;
            }
            Action::Backspace => {
                let idx = self.cursor.char_offset(&self.document.rope);
                if idx > 0 {
                    self.document.remove(idx - 1..idx);
                    self.cursor.move_left(&self.document.rope);
                }
            }
            Action::DeleteForward => {
                let idx = self.cursor.char_offset(&self.document.rope);
                if idx < self.document.rope.len_chars() {
                    self.document.remove(idx..idx + 1);
                }
            }
            Action::Save => match self.document.save() {
                Ok(()) => {
                    self.status = Some(format!("saved {}", self.document.display_name()));
                    return Ok(ActionOutcome::Saved);
                }
                Err(e) => {
                    self.status = Some(format!("save failed: {e}"));
                }
            },
            Action::Quit => return Ok(ActionOutcome::Quit),
        }
        self.cursor.clamp(&self.document.rope);
        Ok(ActionOutcome::Continue)
    }

    pub fn cursor_display_col(&self) -> usize {
        let rope = &self.document.rope;
        if self.cursor.line >= rope.len_lines() {
            return 0;
        }
        let mut display = 0usize;
        for (i, ch) in rope.line(self.cursor.line).chars().enumerate() {
            if i >= self.cursor.col || ch == '\n' || ch == '\r' {
                break;
            }
            display += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        display
    }

    pub fn scroll_to_cursor(&mut self) {
        let h = self.viewport_height as usize;
        let w = self.viewport_width as usize;
        if h == 0 || w == 0 {
            return;
        }
        if self.cursor.line < self.viewport_top {
            self.viewport_top = self.cursor.line;
        } else if self.cursor.line >= self.viewport_top + h {
            self.viewport_top = self.cursor.line + 1 - h;
        }
        let dc = self.cursor_display_col();
        if dc < self.viewport_left {
            self.viewport_left = dc;
        } else if dc >= self.viewport_left + w {
            self.viewport_left = dc + 1 - w;
        }
    }
}
