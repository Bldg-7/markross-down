pub mod cursor;
pub mod input;

use anyhow::Result;
use ratatui::layout::Rect;
use ratatui::text::Line;
use std::ops::Range;
use unicode_width::UnicodeWidthChar;

use crate::clipboard;
use crate::document::{line_len_no_newline, Document};
use crate::parser;
use cursor::Cursor;
use input::{Action, Move};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Raw,
    Preview,
}

pub struct Editor {
    pub document: Document,
    pub cursor: Cursor,
    /// Character offset where Shift- or mouse-anchored selection started.
    pub selection_anchor: Option<usize>,
    pub viewport_top: usize,
    pub viewport_left: usize,
    pub preview_top: usize,
    pub viewport_height: u16,
    pub viewport_width: u16,
    pub content_area: Rect,
    pub status: Option<String>,
    pub mode: RenderMode,
    preview_cache: Option<Vec<Line<'static>>>,
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
            selection_anchor: None,
            viewport_top: 0,
            viewport_left: 0,
            preview_top: 0,
            viewport_height: 0,
            viewport_width: 0,
            content_area: Rect::default(),
            status: None,
            mode: RenderMode::Raw,
            preview_cache: None,
        }
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        let anchor = self.selection_anchor?;
        let total = self.document.rope.len_chars();
        let anchor = anchor.min(total);
        let head = self.cursor.char_offset(&self.document.rope).min(total);
        if anchor == head {
            None
        } else if anchor < head {
            Some(anchor..head)
        } else {
            Some(head..anchor)
        }
    }

    pub fn apply(&mut self, action: Action) -> Result<ActionOutcome> {
        self.status = None;
        if let Action::TogglePreview = action {
            self.toggle_mode();
            return Ok(ActionOutcome::Continue);
        }
        if matches!(self.mode, RenderMode::Preview) {
            return self.apply_preview(action);
        }
        self.apply_raw(action)
    }

    pub fn paste_text(&mut self, text: &str) {
        if self.mode != RenderMode::Raw {
            self.status = Some("preview mode is read-only — press F2 to edit".into());
            return;
        }
        self.delete_selection();
        let idx = self.cursor.char_offset(&self.document.rope);
        self.document.rope.insert(idx, text);
        self.document.dirty = true;
        let inserted = text.chars().count();
        self.set_cursor_to_char_offset(idx + inserted);
        self.selection_anchor = None;
        self.preview_cache = None;
        self.status = Some(format!("pasted {inserted} chars"));
    }

    pub fn preview_lines(&mut self) -> &[Line<'static>] {
        if self.preview_cache.is_none() {
            let text = self.document.rope.to_string();
            self.preview_cache = Some(parser::render(&text));
        }
        self.preview_cache.as_deref().unwrap_or(&[])
    }

    fn apply_raw(&mut self, action: Action) -> Result<ActionOutcome> {
        match action {
            Action::TogglePreview => unreachable!("handled before dispatch"),
            Action::Move(m, extend) => self.apply_move(m, extend),
            Action::InsertChar(c) => {
                self.delete_selection();
                let idx = self.cursor.char_offset(&self.document.rope);
                self.document.insert_char(idx, c);
                self.cursor.move_right(&self.document.rope);
                self.selection_anchor = None;
                self.preview_cache = None;
            }
            Action::InsertNewline => {
                self.delete_selection();
                let idx = self.cursor.char_offset(&self.document.rope);
                self.document.insert_char(idx, '\n');
                self.cursor.line += 1;
                self.cursor.col = 0;
                self.cursor.desired_col = 0;
                self.selection_anchor = None;
                self.preview_cache = None;
            }
            Action::Backspace => {
                if self.selection_range().is_some() {
                    self.delete_selection();
                } else {
                    let idx = self.cursor.char_offset(&self.document.rope);
                    if idx > 0 {
                        self.document.remove(idx - 1..idx);
                        self.cursor.move_left(&self.document.rope);
                        self.preview_cache = None;
                    }
                }
            }
            Action::DeleteForward => {
                if self.selection_range().is_some() {
                    self.delete_selection();
                } else {
                    let idx = self.cursor.char_offset(&self.document.rope);
                    if idx < self.document.rope.len_chars() {
                        self.document.remove(idx..idx + 1);
                        self.preview_cache = None;
                    }
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
            Action::Copy => self.copy_selection_or_line(),
            Action::Cut => self.cut_selection(),
            Action::SelectAll => {
                self.selection_anchor = Some(0);
                let total_chars = self.document.rope.len_chars();
                self.set_cursor_to_char_offset(total_chars);
            }
            Action::PasteHint => {
                self.status = Some(
                    "paste via terminal (Shift+Insert / Ctrl+Shift+V / Cmd+V)".into(),
                );
            }
            Action::MouseDown(col, row) => self.handle_mouse_down(col, row),
            Action::MouseDrag(col, row) => self.handle_mouse_drag(col, row),
            Action::MouseUp => {}
            Action::WheelUp => self.scroll_raw(-3),
            Action::WheelDown => self.scroll_raw(3),
        }
        self.cursor.clamp(&self.document.rope);
        Ok(ActionOutcome::Continue)
    }

    fn apply_preview(&mut self, action: Action) -> Result<ActionOutcome> {
        match action {
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
            Action::Move(Move::Up, _) => self.scroll_preview(-1),
            Action::Move(Move::Down, _) => self.scroll_preview(1),
            Action::Move(Move::PageUp, _) => {
                let step = self.viewport_height.max(1) as isize;
                self.scroll_preview(-step);
            }
            Action::Move(Move::PageDown, _) => {
                let step = self.viewport_height.max(1) as isize;
                self.scroll_preview(step);
            }
            Action::WheelUp => self.scroll_preview(-3),
            Action::WheelDown => self.scroll_preview(3),
            Action::MouseDown(_, _) | Action::MouseDrag(_, _) | Action::MouseUp => {}
            Action::TogglePreview => unreachable!(),
            _ => {
                self.status = Some("preview mode is read-only — press F2 to edit".into());
            }
        }
        Ok(ActionOutcome::Continue)
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            RenderMode::Raw => {
                self.preview_top = 0;
                RenderMode::Preview
            }
            RenderMode::Preview => RenderMode::Raw,
        };
    }

    fn apply_move(&mut self, m: Move, extend: bool) {
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor.char_offset(&self.document.rope));
            }
        } else {
            self.selection_anchor = None;
        }
        match m {
            Move::Left => self.cursor.move_left(&self.document.rope),
            Move::Right => self.cursor.move_right(&self.document.rope),
            Move::Up => self.cursor.move_up(&self.document.rope),
            Move::Down => self.cursor.move_down(&self.document.rope),
            Move::Home => self.cursor.move_home(),
            Move::End => self.cursor.move_end(&self.document.rope),
            Move::PageUp => {
                let step = self.viewport_height.max(1) as usize;
                for _ in 0..step {
                    self.cursor.move_up(&self.document.rope);
                }
            }
            Move::PageDown => {
                let step = self.viewport_height.max(1) as usize;
                for _ in 0..step {
                    self.cursor.move_down(&self.document.rope);
                }
            }
        }
    }

    fn delete_selection(&mut self) {
        if let Some(range) = self.selection_range() {
            self.document.remove(range.clone());
            self.set_cursor_to_char_offset(range.start);
            self.selection_anchor = None;
            self.preview_cache = None;
        }
    }

    fn set_cursor_to_char_offset(&mut self, char_idx: usize) {
        let rope = &self.document.rope;
        let char_idx = char_idx.min(rope.len_chars());
        let line = rope.char_to_line(char_idx);
        let line_start = rope.line_to_char(line);
        let col = char_idx - line_start;
        self.cursor.line = line;
        self.cursor.col = col;
        self.cursor.desired_col = col;
    }

    fn copy_selection_or_line(&mut self) {
        let text = match self.selection_range() {
            Some(range) => self.document.rope.slice(range).to_string(),
            None => self.document.rope.line(self.cursor.line).to_string(),
        };
        match clipboard::copy(&text) {
            Ok(()) => {
                self.status = Some(format!("copied {} chars", text.chars().count()));
            }
            Err(e) => {
                self.status = Some(format!("copy failed: {e}"));
            }
        }
    }

    fn cut_selection(&mut self) {
        let Some(range) = self.selection_range() else {
            self.status = Some("nothing selected to cut".into());
            return;
        };
        let text = self.document.rope.slice(range.clone()).to_string();
        match clipboard::copy(&text) {
            Ok(()) => {
                let n = text.chars().count();
                self.document.remove(range.clone());
                self.set_cursor_to_char_offset(range.start);
                self.selection_anchor = None;
                self.preview_cache = None;
                self.status = Some(format!("cut {n} chars"));
            }
            Err(e) => {
                self.status = Some(format!("cut failed: {e}"));
            }
        }
    }

    fn scroll_raw(&mut self, delta: isize) {
        let total = self.document.rope.len_lines();
        let cap = total.saturating_sub(1);
        self.viewport_top = adjust(self.viewport_top, delta, cap);
    }

    fn scroll_preview(&mut self, delta: isize) {
        let total = self.preview_cache.as_ref().map(|v| v.len()).unwrap_or(0);
        let cap = total.saturating_sub(self.viewport_height.max(1) as usize);
        self.preview_top = adjust(self.preview_top, delta, cap);
    }

    fn handle_mouse_down(&mut self, col: u16, row: u16) {
        let (line, column) = self.view_to_doc(col, row);
        self.cursor.line = line;
        self.cursor.col = column;
        self.cursor.desired_col = column;
        self.selection_anchor = Some(self.cursor.char_offset(&self.document.rope));
    }

    fn handle_mouse_drag(&mut self, col: u16, row: u16) {
        let (line, column) = self.view_to_doc(col, row);
        self.cursor.line = line;
        self.cursor.col = column;
        self.cursor.desired_col = column;
    }

    fn view_to_doc(&self, col: u16, row: u16) -> (usize, usize) {
        let rope = &self.document.rope;
        let total = rope.len_lines();
        let raw_line = self.viewport_top + row as usize;
        let line = raw_line.min(total.saturating_sub(1));
        let target_col = self.viewport_left + col as usize;
        let mut display = 0usize;
        for (i, ch) in rope.line(line).chars().enumerate() {
            if ch == '\n' || ch == '\r' {
                return (line, i);
            }
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
            if display + cw > target_col {
                return (line, i);
            }
            display += cw;
        }
        (line, line_len_no_newline(rope, line))
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

fn adjust(current: usize, delta: isize, cap: usize) -> usize {
    if delta < 0 {
        current.saturating_sub((-delta) as usize)
    } else {
        (current + delta as usize).min(cap)
    }
}
