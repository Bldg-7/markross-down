use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::editor::Editor;

pub fn draw(editor: &mut Editor, frame: &mut Frame) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let content = chunks[0];
    let status = chunks[1];

    editor.viewport_height = content.height;
    editor.viewport_width = content.width;
    editor.scroll_to_cursor();

    draw_content(editor, frame, content);
    draw_status(editor, frame, status);
    place_cursor(editor, frame, content);
}

fn draw_content(editor: &Editor, frame: &mut Frame, area: Rect) {
    let rope = &editor.document.rope;
    let top = editor.viewport_top;
    let left = editor.viewport_left;
    let h = area.height as usize;
    let w = area.width as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(h);
    for i in 0..h {
        let ln = top + i;
        if ln >= rope.len_lines() {
            break;
        }
        let mut out = String::new();
        let mut col = 0usize;
        for ch in rope.line(ln).chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
            if col >= left + w {
                break;
            }
            if col >= left {
                out.push(ch);
            } else if col + cw > left {
                // Wide character straddling the left edge — pad with spaces.
                for _ in 0..(col + cw - left) {
                    out.push(' ');
                }
            }
            col += cw;
        }
        lines.push(Line::raw(out));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_status(editor: &Editor, frame: &mut Frame, area: Rect) {
    let dirty_marker = if editor.document.dirty { "●" } else { " " };
    let name = editor.document.display_name();
    let pos = format!("{}:{}", editor.cursor.line + 1, editor.cursor.col + 1);
    let status_text = editor.status.as_deref().unwrap_or("");
    let left = format!(" {} {}  {}", dirty_marker, name, status_text);
    let right = format!(" {} ", pos);
    let width = area.width as usize;
    let pad = width.saturating_sub(left.width() + right.width());
    let line = format!("{}{}{}", left, " ".repeat(pad), right);
    let paragraph =
        Paragraph::new(line).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(paragraph, area);
}

fn place_cursor(editor: &Editor, frame: &mut Frame, area: Rect) {
    let rel_line = editor.cursor.line.saturating_sub(editor.viewport_top);
    if rel_line >= area.height as usize {
        return;
    }
    let dc = editor.cursor_display_col();
    if dc < editor.viewport_left {
        return;
    }
    let rel_col = dc - editor.viewport_left;
    if rel_col >= area.width as usize {
        return;
    }
    frame.set_cursor_position((area.x + rel_col as u16, area.y + rel_line as u16));
}
