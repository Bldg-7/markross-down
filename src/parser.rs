use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use std::ops::Range;
use tui_big_text::{BigText, PixelSize};
use unicode_width::UnicodeWidthStr;

use crate::plugin::{PluginHost, PluginInvocation};

/// A top-level Markdown block with its pre-rendered styled lines.
pub struct Block {
    // id is reserved for future bitmap / plugin caches.
    #[allow(dead_code)]
    pub id: u64,
    pub source_bytes: Range<usize>,
    pub rendered_lines: Vec<Line<'static>>,
    /// If the block is a fenced code block whose language matches a registered
    /// plugin trigger, this carries the invocation so render can substitute
    /// the plugin's output for the raw code.
    pub plugin_invocation: Option<PluginInvocation>,
}

pub fn render(markdown: &str, host: Option<&PluginHost>) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let events: Vec<(Event<'_>, Range<usize>)> =
        Parser::new_ext(markdown, opts).into_offset_iter().collect();

    let mut blocks = Vec::new();
    let mut next_id = 0u64;
    let mut i = 0;
    while i < events.len() {
        if let Some(end_idx) = find_block_end(&events, i) {
            let start_byte = events[i].1.start;
            let end_byte = events[end_idx].1.end;
            let slice: Vec<(Event<'_>, Range<usize>)> = events[i..=end_idx].to_vec();
            let plugin_invocation = extract_plugin_invocation(&slice, host);
            let rendered = render_events(slice);
            blocks.push(Block {
                id: next_id,
                source_bytes: start_byte..end_byte,
                rendered_lines: rendered,
                plugin_invocation,
            });
            next_id += 1;
            i = end_idx + 1;
        } else {
            if matches!(events[i].0, Event::Rule) {
                blocks.push(Block {
                    id: next_id,
                    source_bytes: events[i].1.clone(),
                    rendered_lines: vec![Line::from(Span::styled(
                        "─".repeat(60),
                        Style::default().fg(Color::DarkGray),
                    ))],
                    plugin_invocation: None,
                });
                next_id += 1;
            }
            i += 1;
        }
    }
    blocks
}

fn extract_plugin_invocation(
    events: &[(Event<'_>, Range<usize>)],
    host: Option<&PluginHost>,
) -> Option<PluginInvocation> {
    let host = host?;
    let (first, _) = events.first()?;
    let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) = first else {
        return None;
    };
    let plugin = host.find_by_trigger(lang.as_ref())?;
    let mut content = String::new();
    for (e, _) in events {
        if let Event::Text(t) = e {
            content.push_str(t.as_ref());
        }
    }
    Some(PluginInvocation {
        plugin_name: plugin.name.clone(),
        content,
    })
}

fn find_block_end(events: &[(Event<'_>, Range<usize>)], start: usize) -> Option<usize> {
    let Event::Start(tag) = &events[start].0 else {
        return None;
    };
    if !is_block_tag(tag) {
        return None;
    }
    let mut depth = 1usize;
    let mut i = start + 1;
    while i < events.len() {
        match &events[i].0 {
            Event::Start(t) if is_block_tag(t) => depth += 1,
            Event::End(te) if is_block_tag_end(te) => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_block_tag(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::FootnoteDefinition(_)
            | Tag::Table(_)
    )
}

fn is_block_tag_end(te: &TagEnd) -> bool {
    matches!(
        te,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
    )
}

fn render_events(events: Vec<(Event<'_>, Range<usize>)>) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];
    let mut list_stack: Vec<ListKind> = Vec::new();
    let mut link_urls: Vec<String> = Vec::new();
    let mut line_prefix: Vec<Span<'static>> = Vec::new();
    let mut in_code_block = false;

    // H1 accumulates plain text and is rendered via tui-big-text at End(Heading).
    let mut big_heading: Option<(Color, String)> = None;

    // Table state
    let mut table_aligns: Vec<Alignment> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Span<'static>>>> = Vec::new();
    let mut in_table_cell = false;
    let mut current_cell: Vec<Span<'static>> = Vec::new();

    let flush = |current: &mut Vec<Span<'static>>,
                 prefix: &[Span<'static>],
                 out: &mut Vec<Line<'static>>| {
        if current.is_empty() && prefix.is_empty() {
            return;
        }
        let mut spans: Vec<Span<'static>> = prefix.to_vec();
        spans.append(current);
        out.push(Line::from(spans));
    };

    let push_inline = |span: Span<'static>,
                       in_table_cell: bool,
                       current: &mut Vec<Span<'static>>,
                       current_cell: &mut Vec<Span<'static>>| {
        if in_table_cell {
            current_cell.push(span);
        } else {
            current.push(span);
        }
    };

    for (event, _range) in events {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    flush(&mut current, &line_prefix, &mut out);
                    let (color, marker) = heading_style(level);
                    if level == HeadingLevel::H1 {
                        big_heading = Some((color, String::new()));
                    } else {
                        let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                        style_stack.push(style);
                        current.push(Span::styled(format!("{marker} "), style));
                    }
                }
                Tag::Paragraph => {}
                Tag::BlockQuote(_) => line_prefix.push(Span::styled(
                    "│ ",
                    Style::default().fg(Color::DarkGray),
                )),
                Tag::CodeBlock(kind) => {
                    flush(&mut current, &line_prefix, &mut out);
                    in_code_block = true;
                    let lang = match kind {
                        CodeBlockKind::Fenced(l) if !l.is_empty() => Some(l.to_string()),
                        _ => None,
                    };
                    let header = lang
                        .as_deref()
                        .map(|l| format!("─── {l} ───────────────────"))
                        .unwrap_or_else(|| "──────────────────────────".to_string());
                    out.push(Line::from(Span::styled(
                        header,
                        Style::default().fg(Color::DarkGray),
                    )));
                    style_stack.push(Style::default().fg(Color::LightYellow));
                }
                Tag::List(start) => list_stack.push(match start {
                    Some(n) => ListKind::Ordered(n),
                    None => ListKind::Unordered,
                }),
                Tag::Item => {
                    let marker = match list_stack.last_mut() {
                        Some(ListKind::Ordered(n)) => {
                            let m = format!("{n}. ");
                            *n += 1;
                            m
                        }
                        _ => "• ".to_string(),
                    };
                    let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                    current.push(Span::raw(format!("{indent}{marker}")));
                }
                Tag::Emphasis => {
                    let base = *style_stack.last().unwrap();
                    style_stack.push(base.add_modifier(Modifier::ITALIC));
                }
                Tag::Strong => {
                    let base = *style_stack.last().unwrap();
                    style_stack.push(base.add_modifier(Modifier::BOLD));
                }
                Tag::Strikethrough => {
                    let base = *style_stack.last().unwrap();
                    style_stack.push(base.add_modifier(Modifier::CROSSED_OUT));
                }
                Tag::Link { dest_url, .. } => {
                    link_urls.push(dest_url.into_string());
                    let base = *style_stack.last().unwrap();
                    style_stack.push(base.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED));
                }
                Tag::Image { dest_url, .. } => push_inline(
                    Span::styled(
                        format!("[image: {dest_url}]"),
                        Style::default().fg(Color::Magenta),
                    ),
                    in_table_cell,
                    &mut current,
                    &mut current_cell,
                ),
                Tag::Table(aligns) => {
                    flush(&mut current, &line_prefix, &mut out);
                    table_aligns = aligns.clone();
                    table_rows.clear();
                }
                Tag::TableHead | Tag::TableRow => {
                    table_rows.push(Vec::new());
                }
                Tag::TableCell => {
                    in_table_cell = true;
                    current_cell.clear();
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    if let Some((color, text)) = big_heading.take() {
                        out.extend(render_big_heading(&text, color));
                    } else {
                        style_stack.pop();
                        flush(&mut current, &line_prefix, &mut out);
                    }
                }
                TagEnd::Paragraph => flush(&mut current, &line_prefix, &mut out),
                TagEnd::BlockQuote(_) => {
                    line_prefix.pop();
                }
                TagEnd::CodeBlock => {
                    style_stack.pop();
                    in_code_block = false;
                    out.push(Line::from(Span::styled(
                        "──────────────────────────",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                }
                TagEnd::Item => flush(&mut current, &line_prefix, &mut out),
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::Link => {
                    style_stack.pop();
                    if let Some(url) = link_urls.pop() {
                        push_inline(
                            Span::styled(
                                format!(" ({url})"),
                                Style::default().fg(Color::DarkGray),
                            ),
                            in_table_cell,
                            &mut current,
                            &mut current_cell,
                        );
                    }
                }
                TagEnd::Table => {
                    let rendered = render_table(&table_rows, &table_aligns);
                    out.extend(rendered);
                    table_rows.clear();
                    table_aligns.clear();
                }
                TagEnd::TableHead | TagEnd::TableRow => {}
                TagEnd::TableCell => {
                    in_table_cell = false;
                    if let Some(row) = table_rows.last_mut() {
                        row.push(std::mem::take(&mut current_cell));
                    }
                }
                _ => {}
            },
            Event::Text(t) => {
                if let Some((_, ref mut acc)) = big_heading {
                    acc.push_str(&t);
                    continue;
                }
                let style = *style_stack.last().unwrap();
                if in_code_block {
                    for segment in t.split_inclusive('\n') {
                        let ends_with_nl = segment.ends_with('\n');
                        let text = segment.trim_end_matches('\n').to_string();
                        if !text.is_empty() {
                            current.push(Span::styled(text, style));
                        }
                        if ends_with_nl {
                            flush(&mut current, &line_prefix, &mut out);
                        }
                    }
                } else {
                    push_inline(
                        Span::styled(t.into_string(), style),
                        in_table_cell,
                        &mut current,
                        &mut current_cell,
                    );
                }
            }
            Event::Code(c) => {
                if big_heading.is_some() {
                    continue;
                }
                push_inline(
                    Span::styled(c.into_string(), Style::default().fg(Color::LightYellow)),
                    in_table_cell,
                    &mut current,
                    &mut current_cell,
                );
            }
            Event::SoftBreak => {
                if big_heading.is_some() {
                    continue;
                }
                push_inline(
                    Span::raw(" "),
                    in_table_cell,
                    &mut current,
                    &mut current_cell,
                );
            }
            Event::HardBreak => {
                if big_heading.is_some() {
                    continue;
                }
                if !in_table_cell {
                    flush(&mut current, &line_prefix, &mut out);
                }
            }
            _ => {}
        }
    }

    flush(&mut current, &line_prefix, &mut out);
    out
}

fn render_big_heading(text: &str, color: Color) -> Vec<Line<'static>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Quadrant pixel size: 4 cols x 4 rows per character.
    let char_count = trimmed.chars().count();
    let width = (char_count.saturating_mul(4).clamp(1, 240)) as u16;
    let height: u16 = 4;
    let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    let widget = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .style(style)
        .lines(vec![Line::from(trimmed.to_string())])
        .build();
    widget_to_lines(widget, width, height)
}

fn widget_to_lines<W: Widget>(widget: W, width: u16, height: u16) -> Vec<Line<'static>> {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);
    let mut lines = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut text = String::new();
        let mut current_style = Style::default();
        let mut started = false;
        for x in 0..width {
            let Some(cell) = buf.cell((x, y)) else {
                continue;
            };
            let style = Style::default()
                .fg(cell.fg)
                .bg(cell.bg)
                .underline_color(cell.underline_color)
                .add_modifier(cell.modifier);
            if !started {
                current_style = style;
                started = true;
            }
            if style != current_style && !text.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut text), current_style));
                current_style = style;
            }
            text.push_str(cell.symbol());
        }
        if !text.is_empty() {
            spans.push(Span::styled(text, current_style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn render_table(rows: &[Vec<Vec<Span<'static>>>], aligns: &[Alignment]) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return Vec::new();
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }
    let mut widths = vec![0usize; cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let w: usize = cell
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            if w > widths[i] {
                widths[i] = w;
            }
        }
    }
    let border = Style::default().fg(Color::DarkGray);
    let mut lines = Vec::new();
    lines.push(make_border(&widths, '┌', '┬', '┐', border));
    for (idx, row) in rows.iter().enumerate() {
        lines.push(make_row(row, &widths, aligns, border));
        if idx == 0 && rows.len() > 1 {
            lines.push(make_border(&widths, '├', '┼', '┤', border));
        }
    }
    lines.push(make_border(&widths, '└', '┴', '┘', border));
    lines
}

fn make_border(
    widths: &[usize],
    left: char,
    mid: char,
    right: char,
    style: Style,
) -> Line<'static> {
    let mut s = String::new();
    s.push(left);
    for (i, w) in widths.iter().enumerate() {
        for _ in 0..(*w + 2) {
            s.push('─');
        }
        if i + 1 < widths.len() {
            s.push(mid);
        }
    }
    s.push(right);
    Line::from(Span::styled(s, style))
}

fn make_row(
    row: &[Vec<Span<'static>>],
    widths: &[usize],
    aligns: &[Alignment],
    border: Style,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled("│", border));
    for (i, w) in widths.iter().enumerate() {
        spans.push(Span::raw(" "));
        let cell: Vec<Span<'static>> = row.get(i).cloned().unwrap_or_default();
        let cell_width: usize = cell
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        let align = aligns.get(i).copied().unwrap_or(Alignment::None);
        let padding = w.saturating_sub(cell_width);
        let (lpad, rpad) = match align {
            Alignment::Right => (padding, 0),
            Alignment::Center => (padding / 2, padding - padding / 2),
            _ => (0, padding),
        };
        if lpad > 0 {
            spans.push(Span::raw(" ".repeat(lpad)));
        }
        spans.extend(cell);
        if rpad > 0 {
            spans.push(Span::raw(" ".repeat(rpad)));
        }
        spans.push(Span::raw(" "));
        spans.push(Span::styled("│", border));
    }
    Line::from(spans)
}

enum ListKind {
    Ordered(u64),
    Unordered,
}

fn heading_style(level: HeadingLevel) -> (Color, &'static str) {
    match level {
        HeadingLevel::H1 => (Color::Magenta, "#"),
        HeadingLevel::H2 => (Color::Blue, "##"),
        HeadingLevel::H3 => (Color::Cyan, "###"),
        HeadingLevel::H4 => (Color::Green, "####"),
        HeadingLevel::H5 => (Color::Yellow, "#####"),
        HeadingLevel::H6 => (Color::Red, "######"),
    }
}
