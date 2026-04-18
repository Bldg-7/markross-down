use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
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
            editor.scroll_to_cursor_raw();
            draw_raw(editor, frame, content);
            place_cursor_raw(editor, frame, content);
        }
        RenderMode::Preview => draw_preview(editor, frame, content),
    }
    draw_status(editor, frame, status);
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
    let h = area.height as usize;
    let layout = editor.preview_layout();
    let total = layout.lines.len();

    // Keep the cursor's rendered row in view.
    if layout.cursor_rendered_line < editor.preview_top {
        editor.preview_top = layout.cursor_rendered_line;
    } else if layout.cursor_rendered_line >= editor.preview_top + h && h > 0 {
        editor.preview_top = layout.cursor_rendered_line + 1 - h;
    }
    let max_top = total.saturating_sub(h);
    editor.preview_top = editor.preview_top.min(max_top);

    let slice: Vec<Line> = layout
        .lines
        .iter()
        .skip(editor.preview_top)
        .take(h)
        .cloned()
        .collect();
    frame.render_widget(Paragraph::new(slice), area);

    // Place cursor if it falls into the visible range.
    let rel = layout.cursor_rendered_line as isize - editor.preview_top as isize;
    if rel >= 0 && (rel as usize) < h {
        let dc = editor.cursor_display_col();
        if dc < area.width as usize {
            frame.set_cursor_position((area.x + dc as u16, area.y + rel as u16));
        }
    }
}

fn draw_status(editor: &Editor, frame: &mut Frame, area: Rect) {
    let dirty_marker = if editor.document.dirty { "●" } else { " " };
    let name = editor.document.display_name();
    let mode_label = match editor.mode {
        RenderMode::Raw => "RAW",
        RenderMode::Preview => "PREVIEW",
    };
    let pos = format!("{}:{}", editor.cursor.line + 1, editor.cursor.col + 1);
    let status_text = editor.status.as_deref().unwrap_or("");
    let left = format!(" [{mode_label}] {dirty_marker} {name}  {status_text}");
    let right = format!(" {pos} ");
    let width = area.width as usize;
    let pad = width.saturating_sub(left.width() + right.width());
    let line = format!("{left}{}{right}", " ".repeat(pad));
    let paragraph = Paragraph::new(line)
        .style(Style::default().bg(editor.theme.status_bg()).fg(editor.theme.status_fg()));
    frame.render_widget(paragraph, area);
}

fn place_cursor_raw(editor: &Editor, frame: &mut Frame, area: Rect) {
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
