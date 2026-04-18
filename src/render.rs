use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::editor::{Editor, RenderMode};

pub fn draw(editor: &mut Editor, frame: &mut Frame) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let content = chunks[0];
    let status = chunks[1];

    editor.viewport_height = content.height;
    editor.viewport_width = content.width;
    editor.content_area = content;

    match editor.mode {
        RenderMode::Raw => {
            editor.scroll_to_cursor();
            draw_raw(editor, frame, content);
        }
        RenderMode::Preview => draw_preview(editor, frame, content),
    }
    draw_status(editor, frame, status);
    if matches!(editor.mode, RenderMode::Raw) {
        place_cursor(editor, frame, content);
    }
}

fn draw_raw(editor: &Editor, frame: &mut Frame, area: Rect) {
    let rope = &editor.document.rope;
    let top = editor.viewport_top;
    let left = editor.viewport_left;
    let h = area.height as usize;
    let w = area.width as usize;
    let sel_range = editor.selection_range();

    let plain = Style::default();
    let highlight = Style::default().add_modifier(Modifier::REVERSED);

    let flush = |spans: &mut Vec<Span>, buf: &mut String, selected: bool| {
        if !buf.is_empty() {
            let style = if selected { highlight } else { plain };
            spans.push(Span::styled(std::mem::take(buf), style));
        }
    };

    let mut lines: Vec<Line> = Vec::with_capacity(h);
    for i in 0..h {
        let ln = top + i;
        if ln >= rope.len_lines() {
            break;
        }
        let line_start_char = rope.line_to_char(ln);
        let mut spans: Vec<Span> = Vec::new();
        let mut buf = String::new();
        let mut buf_selected = false;
        let mut col = 0usize;
        let mut char_idx = line_start_char;

        for ch in rope.line(ln).chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            let selected = sel_range.as_ref().is_some_and(|r| r.contains(&char_idx));
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);

            if col >= left + w {
                break;
            }
            if col >= left {
                if selected != buf_selected {
                    flush(&mut spans, &mut buf, buf_selected);
                    buf_selected = selected;
                }
                buf.push(ch);
            } else if col + cw > left {
                if selected != buf_selected {
                    flush(&mut spans, &mut buf, buf_selected);
                    buf_selected = selected;
                }
                for _ in 0..(col + cw - left) {
                    buf.push(' ');
                }
            }
            col += cw;
            char_idx += 1;
        }
        flush(&mut spans, &mut buf, buf_selected);
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_preview(editor: &mut Editor, frame: &mut Frame, area: Rect) {
    let top = editor.preview_top;
    let h = area.height as usize;
    let preview = editor.preview_lines();
    let slice: Vec<Line> = preview.iter().skip(top).take(h).cloned().collect();
    frame.render_widget(Paragraph::new(slice), area);
}

fn draw_status(editor: &Editor, frame: &mut Frame, area: Rect) {
    let dirty_marker = if editor.document.dirty { "●" } else { " " };
    let name = editor.document.display_name();
    let mode_label = match editor.mode {
        RenderMode::Raw => "RAW",
        RenderMode::Preview => "PREVIEW",
    };
    let pos = match editor.mode {
        RenderMode::Raw => format!("{}:{}", editor.cursor.line + 1, editor.cursor.col + 1),
        RenderMode::Preview => format!("line {}", editor.preview_top + 1),
    };
    let status_text = editor.status.as_deref().unwrap_or("");
    let left = format!(" [{mode_label}] {dirty_marker} {name}  {status_text}");
    let right = format!(" {pos} ");
    let width = area.width as usize;
    let pad = width.saturating_sub(left.width() + right.width());
    let line = format!("{left}{}{right}", " ".repeat(pad));
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
