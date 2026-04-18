use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::ops::Range;

/// A top-level Markdown block with its pre-rendered styled lines.
///
/// Blocks are the unit of raw-vs-styled mode switching: when the cursor (or
/// an active selection) sits inside a block's `source_bytes`, the renderer
/// shows that block as raw source; otherwise it uses `rendered_lines`.
pub struct Block {
    // id is unused in M4 but carried through for M6/M8 bitmap and plugin caching.
    #[allow(dead_code)]
    pub id: u64,
    pub source_bytes: Range<usize>,
    pub rendered_lines: Vec<Line<'static>>,
}

pub fn render(markdown: &str) -> Vec<Block> {
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
            let rendered = render_events(slice);
            blocks.push(Block {
                id: next_id,
                source_bytes: start_byte..end_byte,
                rendered_lines: rendered,
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
                });
                next_id += 1;
            }
            i += 1;
        }
    }
    blocks
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

/// Render a single top-level block's events into styled lines.
fn render_events(events: Vec<(Event<'_>, Range<usize>)>) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];
    let mut list_stack: Vec<ListKind> = Vec::new();
    let mut link_urls: Vec<String> = Vec::new();
    let mut line_prefix: Vec<Span<'static>> = Vec::new();
    let mut in_code_block = false;

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

    for (event, _range) in events {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    flush(&mut current, &line_prefix, &mut out);
                    let (color, marker) = heading_style(level);
                    let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                    style_stack.push(style);
                    current.push(Span::styled(format!("{marker} "), style));
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
                Tag::Image { dest_url, .. } => current.push(Span::styled(
                    format!("[image: {dest_url}]"),
                    Style::default().fg(Color::Magenta),
                )),
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    style_stack.pop();
                    flush(&mut current, &line_prefix, &mut out);
                }
                TagEnd::Paragraph => {
                    flush(&mut current, &line_prefix, &mut out);
                }
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
                TagEnd::Item => {
                    flush(&mut current, &line_prefix, &mut out);
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::Link => {
                    style_stack.pop();
                    if let Some(url) = link_urls.pop() {
                        current.push(Span::styled(
                            format!(" ({url})"),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
                _ => {}
            },
            Event::Text(t) => {
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
                    current.push(Span::styled(t.into_string(), style));
                }
            }
            Event::Code(c) => current.push(Span::styled(
                c.into_string(),
                Style::default().fg(Color::LightYellow),
            )),
            Event::SoftBreak => current.push(Span::raw(" ")),
            Event::HardBreak => flush(&mut current, &line_prefix, &mut out),
            _ => {}
        }
    }

    flush(&mut current, &line_prefix, &mut out);
    out
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
