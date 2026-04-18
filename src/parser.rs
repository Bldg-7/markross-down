use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render Markdown text into a flat sequence of styled lines suitable for
/// preview display. Block-level source-range tracking will land with M4
/// when per-block inline mode switching needs it.
pub fn render(markdown: &str) -> Vec<Line<'static>> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(markdown, opts);

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

    for event in parser {
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
                Tag::BlockQuote(_) => {
                    line_prefix.push(Span::styled(
                        "│ ",
                        Style::default().fg(Color::DarkGray),
                    ));
                }
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
                Tag::Image { dest_url, .. } => {
                    current.push(Span::styled(
                        format!("[image: {dest_url}]"),
                        Style::default().fg(Color::Magenta),
                    ));
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    style_stack.pop();
                    flush(&mut current, &line_prefix, &mut out);
                    out.push(Line::raw(""));
                }
                TagEnd::Paragraph => {
                    flush(&mut current, &line_prefix, &mut out);
                    out.push(Line::raw(""));
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
                    out.push(Line::raw(""));
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
            Event::Code(c) => {
                current.push(Span::styled(
                    c.into_string(),
                    Style::default().fg(Color::LightYellow),
                ));
            }
            Event::SoftBreak => current.push(Span::raw(" ")),
            Event::HardBreak => flush(&mut current, &line_prefix, &mut out),
            Event::Rule => {
                flush(&mut current, &line_prefix, &mut out);
                out.push(Line::from(Span::styled(
                    "─".repeat(60),
                    Style::default().fg(Color::DarkGray),
                )));
                out.push(Line::raw(""));
            }
            _ => {}
        }
    }

    flush(&mut current, &line_prefix, &mut out);
    // Trim trailing blank lines so scroll bounds are tighter.
    while matches!(out.last(), Some(line) if line_is_blank(line)) {
        out.pop();
    }
    out
}

fn line_is_blank(line: &Line<'_>) -> bool {
    line.spans.iter().all(|s| s.content.trim().is_empty())
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
