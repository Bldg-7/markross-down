pub mod cursor;
pub mod input;

use anyhow::Result;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ropey::Rope;
use sha2::{Digest, Sha256};
use std::ops::Range;
use unicode_width::UnicodeWidthChar;

use crate::clipboard;
use crate::document::{line_len_no_newline, Document};
use crate::parser::{self, Block};
use crate::plugin::{PluginHost, PluginOutput, PluginState};
use crate::watcher::WatchEvent;
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
    pub selection_anchor: Option<usize>,
    pub viewport_top: usize,
    pub viewport_left: usize,
    pub preview_top: usize,
    pub viewport_height: u16,
    pub viewport_width: u16,
    pub content_area: Rect,
    pub status: Option<String>,
    pub mode: RenderMode,
    pub pending_reload: Option<PendingReload>,
    pub plugin_host: PluginHost,
    preview_cache: Option<Vec<Block>>,
}

pub struct PendingReload {
    pub disk_text: String,
    pub disk_hash: [u8; 32],
}

pub enum ActionOutcome {
    Continue,
    Saved,
    Quit,
}

/// Flattened preview output for one frame, produced by `Editor::preview_layout`.
/// `cursor_rendered_line` is the row within `lines` that currently contains
/// the cursor; it is always valid because the cursor's block is always
/// displayed as raw (so source lines map 1:1 inside it).
pub struct PreviewLayout {
    pub lines: Vec<Line<'static>>,
    pub cursor_rendered_line: usize,
}

impl Editor {
    pub fn new(document: Document, plugin_host: PluginHost) -> Self {
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
            pending_reload: None,
            plugin_host,
            preview_cache: None,
        }
    }

    pub fn handle_watch_event(&mut self, event: WatchEvent) {
        match event {
            WatchEvent::Changed => self.reconcile_disk_change(),
            WatchEvent::Removed => {
                self.status = Some("file removed from disk".into());
            }
            WatchEvent::Error(e) => {
                self.status = Some(format!("watcher error: {e}"));
            }
        }
    }

    fn reconcile_disk_change(&mut self) {
        let Some(path) = self.document.path.clone() else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => return,
        };
        let hash: [u8; 32] = Sha256::digest(&bytes).into();
        if Some(hash) == self.document.last_save_hash {
            return;
        }
        let disk_text = String::from_utf8_lossy(&bytes).into_owned();
        if !self.document.dirty {
            self.apply_reload(&disk_text, hash);
            self.status = Some("reloaded from disk".into());
        } else {
            self.pending_reload = Some(PendingReload { disk_text, disk_hash: hash });
            self.status = Some("DISK CHANGED — r: reload | i: keep mine".into());
        }
    }

    pub fn accept_reload(&mut self) {
        if let Some(p) = self.pending_reload.take() {
            self.apply_reload(&p.disk_text, p.disk_hash);
            self.status = Some("reloaded from disk, discarded local changes".into());
        }
    }

    pub fn reject_reload(&mut self) {
        if let Some(p) = self.pending_reload.take() {
            self.document.last_save_hash = Some(p.disk_hash);
            self.status = Some("ignored disk change, keeping local buffer".into());
        }
    }

    fn apply_reload(&mut self, text: &str, hash: [u8; 32]) {
        self.document.rope = Rope::from_str(text);
        self.document.dirty = false;
        self.document.last_save_hash = Some(hash);
        self.cursor.clamp(&self.document.rope);
        self.selection_anchor = None;
        self.preview_cache = None;
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

    fn selection_line_range(&self) -> Option<Range<usize>> {
        let range = self.selection_range()?;
        let rope = &self.document.rope;
        let start_line = rope.char_to_line(range.start);
        let end_line = rope.char_to_line(range.end);
        Some(start_line..(end_line + 1))
    }

    pub fn apply(&mut self, action: Action) -> Result<ActionOutcome> {
        self.status = None;
        if let Action::TogglePreview = action {
            self.mode = match self.mode {
                RenderMode::Raw => RenderMode::Preview,
                RenderMode::Preview => RenderMode::Raw,
            };
            return Ok(ActionOutcome::Continue);
        }
        match action {
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
            Action::WheelUp => self.wheel(-3),
            Action::WheelDown => self.wheel(3),
            Action::TogglePreview => unreachable!(),
        }
        self.cursor.clamp(&self.document.rope);
        Ok(ActionOutcome::Continue)
    }

    pub fn paste_text(&mut self, text: &str) {
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

    /// Build (or reuse) the preview layout for this frame.
    pub fn preview_layout(&mut self) -> PreviewLayout {
        self.ensure_preview_cache();
        let blocks = self.preview_cache.as_deref().unwrap_or(&[]);
        build_preview_layout(self, blocks)
    }

    fn ensure_preview_cache(&mut self) {
        if self.preview_cache.is_none() {
            let text = self.document.rope.to_string();
            self.preview_cache = Some(parser::render(&text, Some(&self.plugin_host)));
        }
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

    fn wheel(&mut self, delta: isize) {
        let total = self.document.rope.len_lines();
        let cap = total.saturating_sub(1);
        self.viewport_top = if delta < 0 {
            self.viewport_top.saturating_sub((-delta) as usize)
        } else {
            (self.viewport_top + delta as usize).min(cap)
        };
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

    pub fn scroll_to_cursor_raw(&mut self) {
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

fn block_display_lines(editor: &Editor, block: &Block) -> Vec<Line<'static>> {
    let Some(inv) = &block.plugin_invocation else {
        return block.rendered_lines.clone();
    };
    match editor.plugin_host.query(&inv.plugin_name, &inv.content) {
        PluginState::Ready(PluginOutput::Text(out)) => {
            plugin_text_lines(&inv.plugin_name, &out)
        }
        PluginState::Ready(PluginOutput::Error(err)) => {
            let mut lines = plugin_error_lines(&inv.plugin_name, &err);
            lines.extend(block.rendered_lines.iter().cloned());
            lines
        }
        PluginState::Pending => plugin_pending_lines(&inv.plugin_name),
        PluginState::NotFound => block.rendered_lines.clone(),
    }
}

fn plugin_text_lines(name: &str, text: &str) -> Vec<Line<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::Span;
    let header = Line::from(Span::styled(
        format!("─── {name} output ──────────────"),
        Style::default().fg(Color::DarkGray),
    ));
    let body = text
        .lines()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::Green))));
    let footer = Line::from(Span::styled(
        "──────────────────────────".to_string(),
        Style::default().fg(Color::DarkGray),
    ));
    std::iter::once(header).chain(body).chain(std::iter::once(footer)).collect()
}

fn plugin_error_lines(name: &str, err: &str) -> Vec<Line<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::Span;
    let mut lines = vec![Line::from(Span::styled(
        format!("─── {name} error ───────────────"),
        Style::default().fg(Color::Red),
    ))];
    for l in err.lines().take(3) {
        lines.push(Line::from(Span::styled(
            l.to_string(),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::from(Span::styled(
        "(falling back to raw source below)".to_string(),
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

fn plugin_pending_lines(name: &str) -> Vec<Line<'static>> {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;
    vec![Line::from(Span::styled(
        format!("[{name}] rendering …"),
        Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM),
    ))]
}

fn block_source_lines(editor: &Editor, block: &Block) -> Range<usize> {
    let rope = &editor.document.rope;
    let total = rope.len_lines();
    let start = rope.byte_to_line(block.source_bytes.start.min(rope.len_bytes()));
    let end_byte = block.source_bytes.end.min(rope.len_bytes());
    let end = if end_byte > block.source_bytes.start {
        rope.byte_to_line(end_byte.saturating_sub(1)) + 1
    } else {
        start
    };
    start..end.min(total)
}

fn build_preview_layout(editor: &Editor, blocks: &[Block]) -> PreviewLayout {
    let rope = &editor.document.rope;
    let total_lines = rope.len_lines();
    let cursor_line = editor.cursor.line;
    let sel_lines = editor.selection_line_range();

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut cursor_rendered_line: usize = 0;
    let mut prev_end = 0usize;

    let emit_raw = |range: Range<usize>,
                    lines: &mut Vec<Line<'static>>,
                    cursor_rendered_line: &mut usize| {
        for ln in range {
            if ln >= total_lines {
                break;
            }
            if ln == cursor_line {
                *cursor_rendered_line = lines.len();
            }
            let s: String = rope
                .line(ln)
                .chars()
                .take_while(|c| *c != '\n' && *c != '\r')
                .collect();
            lines.push(Line::raw(s));
        }
    };

    for block in blocks {
        let src = block_source_lines(editor, block);
        if src.start > prev_end {
            emit_raw(prev_end..src.start, &mut lines, &mut cursor_rendered_line);
        }
        let raw_fallback = src.contains(&cursor_line)
            || sel_lines
                .as_ref()
                .is_some_and(|s| !(s.end <= src.start || s.start >= src.end));
        if raw_fallback {
            emit_raw(src.clone(), &mut lines, &mut cursor_rendered_line);
        } else {
            for l in block_display_lines(editor, block) {
                lines.push(l);
            }
        }
        prev_end = src.end;
    }
    if prev_end < total_lines {
        emit_raw(prev_end..total_lines, &mut lines, &mut cursor_rendered_line);
    }

    PreviewLayout {
        lines,
        cursor_rendered_line,
    }
}
